// SPDX-License-Identifier: LGPL-3.0-only
//! Authority-only reclamation. Clients submit typed intents, never old OIB roots.
use super::*;

pub(super) const FIRST_ID: u64 = 0x1_0002;
const MAX_ID: u64 = (u64::MAX - 0x100_0000 - 63) / 64 + 0x1_0000;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAllocation {
    id: u64,
    name: String,
    component: u64,
    prototype: u64,
}

pub(super) fn decode_legacy_allocations(
    encoded: &[u8],
    originals: usize,
) -> Result<Vec<Allocation>> {
    let old: Vec<LegacyAllocation> = decode(encoded)?;
    Ok(old
        .into_iter()
        .enumerate()
        .map(|(slot, a)| Allocation {
            id: a.id,
            name: a.name,
            component: a.component,
            prototype: a.prototype,
            slot: if slot < originals {
                slot
            } else {
                slot - originals
            } as u16,
        })
        .collect())
}

impl Document {
    pub(super) fn oib_next_id(&self) -> Result<u64> {
        decode(&payload(&self.tree, &[4, 7])?)
    }

    pub(super) fn oib_allocate(
        &mut self,
        allocations: &[Allocation],
        prototype: u64,
        name: Option<String>,
    ) -> Result<Allocation> {
        let seed = self.oib_binding()?.seed;
        let source = seed
            .instances
            .iter()
            .find(|i| i.id == prototype)
            .ok_or("Undeclared OIB factory")?;
        let slot = (0
            ..(MAX_ALLOCATIONS - super::super::super::oib::instances::initial_count(&seed)) as u16)
            .find(|s| {
                allocations
                    .iter()
                    .all(|a| a.id == a.prototype || a.slot != *s)
            })
            .ok_or("OIB resident instance capacity exhausted")?;
        let id = self.oib_next_id()?;
        if !(FIRST_ID..MAX_ID).contains(&id) {
            return Err("OIB identity counter exhausted".into());
        }
        let name = name.unwrap_or_else(|| format!("instance-{id}"));
        if !admitted_instance_name(&seed, &name) {
            return Err(
                "Instance names use 1–64 ASCII letters, digits, hyphens or underscores".into(),
            );
        }
        if seed.designer.is_some() {
            let mut resident = allocations.to_vec();
            resident.push(Allocation {
                id,
                name: name.clone(),
                component: source.component.ok_or("Missing component")?,
                prototype,
                slot,
            });
            // Check the complete draft/active resident union before advancing the
            // monotonic identity, including each occurrence's entire key table.
            super::super::super::oib::instances::expanded_seed(&seed, &resident)?;
        }
        self.change(
            &[],
            vec![Edit {
                path: vec![4, 7],
                schema: self.extra_schema(4, 7),
                value: Some(bytes(&(id + 1))?),
            }],
        )?;
        Ok(Allocation {
            id,
            name,
            component: source.component.ok_or("Missing component")?,
            prototype,
            slot,
        })
    }

    pub(super) fn oib_retired_instance(&self, id: u64) -> Result<bool> {
        if self.oib_live(1)?.contains(&id) {
            return Ok(false);
        }
        let seed = self.oib_binding()?.seed;
        Ok(seed.instances.iter().any(|i| i.id == id)
            || (FIRST_ID..self.oib_next_id()?).contains(&id))
    }

    pub(super) fn oib_retired_cell(&self, id: u64) -> Result<bool> {
        let seed = self.oib_binding()?.seed;
        if let Some(cell) = seed.cells.iter().find(|c| c.id == id) {
            return if cell.inherit.is_some() {
                self.oib_retired_instance(cell.owner)
            } else {
                Ok(false)
            };
        }
        if id >= 0x100_0000 {
            let owner = (id - 0x100_0000) / 64 + 0x1_0000;
            return self.oib_retired_instance(owner);
        }
        Ok(false)
    }

    pub(super) fn oib_restore_residents(&mut self, defaults: bool, writer: Actor) -> Result<()> {
        let seed = self.oib_binding()?.seed;
        if seed.designer.is_some() {
            return self.oib_restore_designer(defaults, writer);
        }
        let previous = self.oib_allocations()?;
        let live = self.oib_live(1)?;
        let mut allocations: Vec<_> = seed
            .instances
            .iter()
            .filter_map(|s| {
                previous
                    .iter()
                    .find(|a| {
                        live.contains(&a.id)
                            && a.prototype == s.id
                            && (a.id == s.id || a.name == s.name)
                    })
                    .cloned()
            })
            .collect();
        for source in &seed.instances {
            if !allocations
                .iter()
                .any(|a| a.prototype == source.id && (a.id == source.id || a.name == source.name))
            {
                allocations.push(self.oib_allocate(
                    &allocations,
                    source.id,
                    Some(source.name.clone()),
                )?);
            }
        }
        // Put the authored factories in signed order, independent of creation IDs.
        allocations.sort_by_key(|a| seed.instances.iter().position(|s| s.id == a.prototype));
        let resolved = super::super::super::oib::instances::expanded_seed(&seed, &allocations)?;
        let mut edits = vec![Edit {
            path: vec![4, 4],
            schema: self.extra_schema(4, 4),
            value: Some(bytes(&allocations)?),
        }];
        for a in &allocations {
            edits.push(Edit {
                path: cell_path(5, a.id),
                schema: self.oib_instance_schema(a.id),
                value: Some(bytes(&true)?),
            });
        }
        for cell in resolved.cells {
            if defaults || self.tree.subtree(&cell_path(1, cell.id)).is_err() {
                edits.push(Edit {
                    path: cell_path(1, cell.id),
                    schema: self.oib_cell_schema(cell.id),
                    value: if cell.inherit.is_some() {
                        None
                    } else {
                        Some(bytes(&cell.default)?)
                    },
                });
            }
        }
        self.tree = self.tree.edit(writer, &edits).map_err(err)?.0;
        self.oib_reclaim()
    }

    /// Retain the union until the coherent new graph activates. In particular,
    /// restoring a deleted factory cannot borrow cells/slots still pinned by the
    /// current active graph or consume an identity on capacity failure.
    fn oib_restore_designer(&mut self, defaults: bool, writer: Actor) -> Result<()> {
        let seed = self.oib_binding()?.seed;
        let mut resident = self.oib_allocations()?;
        let live = self.oib_live(1)?;
        let mut desired = BTreeSet::new();
        for factory in seed
            .instances
            .iter()
            .filter(|i| !super::super::super::oib::instances::palette_only(&seed, i.id))
        {
            if let Some(existing) = resident
                .iter()
                .filter(|a| live.contains(&a.id) && a.prototype == factory.id)
                .min_by_key(|a| (a.id != factory.id, a.id))
            {
                desired.insert(existing.id);
            } else {
                let new = self.oib_allocate(&resident, factory.id, Some(factory.name.clone()))?;
                desired.insert(new.id);
                resident.push(new);
            }
        }
        // Restore returns the authored residents to signed canvas order. Keep
        // the active/draft union's other descriptors until activation reclaims
        // them, without ever changing a resident slot or occurrence identity.
        let initial = super::super::super::oib::instances::initial_allocations(&seed);
        resident.sort_by_key(|a| {
            if desired.contains(&a.id) {
                initial
                    .iter()
                    .position(|i| i.prototype == a.prototype)
                    .unwrap_or(usize::MAX)
            } else {
                usize::MAX
            }
        });
        let resolved = super::super::super::oib::instances::expanded_seed(&seed, &resident)?;
        let mut edits = vec![Edit {
            path: vec![4, 4],
            schema: self.extra_schema(4, 4),
            value: Some(bytes(&resident)?),
        }];
        for a in &resident {
            edits.push(Edit {
                path: cell_path(5, a.id),
                schema: self.oib_instance_schema(a.id),
                value: desired.contains(&a.id).then(|| bytes(&true)).transpose()?,
            });
        }
        for cell in &resolved.cells {
            let wanted = cell.inherit.is_none() || desired.contains(&cell.owner);
            if (defaults && wanted) || self.tree.subtree(&cell_path(1, cell.id)).is_err() {
                edits.push(Edit {
                    path: cell_path(1, cell.id),
                    schema: self.oib_cell_schema(cell.id),
                    value: if cell.inherit.is_some() {
                        None
                    } else {
                        Some(bytes(&cell.default)?)
                    },
                });
            }
        }
        self.tree = self.tree.edit(writer, &edits).map_err(err)?.0;
        Ok(())
    }

    /// Drop only objects absent from both draft and active graphs. Projection
    /// preserves the exact causal registers of survivors and pins old snapshots.
    /// A scalar high-water mark replaces an ever-growing retired-ID table.
    pub(super) fn oib_reclaim(&mut self) -> Result<()> {
        if self.tree.subtree(&[4, 7]).is_err() {
            return Ok(());
        }
        let mut allocations = self.oib_allocations()?;
        let draft = self.oib_live(1)?;
        let active = self.oib_live(2)?;
        let previous = allocations.len();
        allocations.retain(|a| draft.contains(&a.id) || active.contains(&a.id));
        if previous != allocations.len() {
            self.change(
                &[],
                vec![Edit {
                    path: vec![4, 4],
                    schema: self.extra_schema(4, 4),
                    value: Some(bytes(&allocations)?),
                }],
            )?;
        }
        let mut keep: Vec<Vec<u64>> = self
            .tree
            .child_keys()
            .filter(|k| *k != 4)
            .map(|k| vec![k])
            .collect();
        keep.extend([0, 3, 4, 7].map(|k| vec![4, k]));
        for kind in [8, 9, 10] {
            if self.tree.subtree(&[4, kind]).is_ok() {
                keep.push(vec![4, kind]);
            }
        }
        for cell in self.oib_resolved_seed()?.cells {
            for branch in [1, 2] {
                keep.push(cell_path(branch, cell.id));
            }
        }
        for a in &allocations {
            for branch in [5, 6] {
                keep.push(cell_path(branch, a.id));
            }
        }
        let coverage = merkle::Coverage::new(&[vec![]], &keep).map_err(err)?;
        self.tree = self.tree.project(&coverage).map_err(err)?.0;
        Ok(())
    }

    pub(super) fn validate_resident_allocations(&self, old: Option<&Snapshot>) -> Result<()> {
        let seed = self.oib_binding()?.seed;
        let allocations = self.oib_allocations()?;
        let next = self.oib_next_id()?;
        super::super::super::oib::instances::expanded_seed(&seed, &allocations)?;
        let mut slots = BTreeSet::new();
        let mut ids = BTreeSet::new();
        if !(FIRST_ID..=MAX_ID).contains(&next) || allocations.len() > MAX_ALLOCATIONS {
            return Err("Invalid OIB resident allocator".into());
        }
        for a in &allocations {
            let original = seed.instances.iter().any(|s| {
                s.id == a.id
                    && s.id == a.prototype
                    && s.name == a.name
                    && !super::super::super::oib::instances::palette_only(&seed, s.id)
            });
            if !admitted_instance_name(&seed, &a.name)
                || !ids.insert(a.id)
                || (!original
                    && (!slots.insert(a.slot)
                        || usize::from(a.slot)
                            >= MAX_ALLOCATIONS
                                - super::super::super::oib::instances::initial_count(&seed)))
                || (!original && !(FIRST_ID..next).contains(&a.id))
                || !seed
                    .instances
                    .iter()
                    .any(|s| s.id == a.prototype && s.component == Some(a.component))
            {
                return Err("Invalid OIB resident descriptor".into());
            }
        }
        for index in [4, 7] {
            let leaf = self.tree.subtree(&[4, index]).map_err(err)?;
            if leaf.child_keys().next().is_some()
                || leaf
                    .register()
                    .is_none_or(|r| r.schema() != self.extra_schema(4, index as usize))
            {
                return Err("Invalid OIB allocator schema".into());
            }
        }
        if let Some(old) = old {
            let previous_next: Option<u64> =
                payload(old, &[4, 7]).ok().map(|v| decode(&v)).transpose()?;
            let previous: Vec<Allocation> = if let Ok(encoded) = payload(old, &[4, 4]) {
                if previous_next.is_some() {
                    decode(&encoded)?
                } else {
                    decode_legacy_allocations(&encoded, seed.instances.len())?
                }
            } else if let Ok(encoded) = payload(old, &[4, 0]) {
                let binding: Binding = decode(&encoded)?;
                if binding.seed.instance_surface.is_some() {
                    return Err("Missing previous OIB allocator".into());
                }
                super::super::super::oib::instances::initial_allocations(&binding.seed)
            } else {
                Vec::new()
            };
            if previous_next.is_some_and(|n| n > next) {
                return Err("OIB identity counter regressed".into());
            }
            for a in &allocations {
                if let Some(before) = previous.iter().find(|b| b.id == a.id) {
                    if before.id != a.id
                        || before.name != a.name
                        || before.component != a.component
                        || before.prototype != a.prototype
                        || (previous_next.is_some() && before.slot != a.slot)
                    {
                        return Err("OIB resident identity changed".into());
                    }
                } else if previous_next.is_some_and(|n| a.id < n) {
                    return Err("OIB retired identity reused".into());
                }
            }
        }
        let mut size = 0;
        for branch in [1, 2, 4, 5, 6, 7, 8, 9, 10] {
            if let Ok(n) = self.tree.subtree(&[4, branch]) {
                size += n.canonical_bytes();
            }
        }
        if size > otrios_interfaces::oib::MAX_BYTES {
            return Err("OIB resident byte budget".into());
        }
        for branch in [5, 6] {
            let Ok(tree) = self.tree.subtree(&[4, branch]) else {
                continue;
            };
            let mut seen = BTreeSet::new();
            trie_ids(&tree, 0, 0, &mut seen)?;
            if !seen.is_subset(&ids) || (branch == 5 && seen != ids) {
                return Err("OIB resident presence mismatch".into());
            }
            for id in seen {
                let leaf = self.tree.subtree(&cell_path(branch, id)).map_err(err)?;
                let r = leaf.register().ok_or("Missing OIB presence")?;
                if r.schema() != self.oib_instance_schema(id) {
                    return Err("Wrong OIB presence schema".into());
                }
                for (_, _, v) in r.values() {
                    if let Some(v) = v
                        && !decode::<bool>(v)?
                    {
                        return Err("Invalid OIB presence".into());
                    }
                }
            }
        }
        Ok(())
    }
}

impl StateAdapter {
    pub(super) fn oib_available_slots(&self) -> usize {
        self.oib_slots(None)
    }
    pub(in crate::canvas) fn oib_available_slots_for(&self, prototype: u64) -> usize {
        self.oib_slots(Some(prototype))
    }
    fn oib_slots(&self, prototype: Option<u64>) -> usize {
        let Ok(allocations) = self.doc.oib_allocations() else {
            return 0;
        };
        let dynamic_capacity = self.doc.oib_binding().map_or(0, |b| {
            MAX_ALLOCATIONS
                .saturating_sub(super::super::super::oib::instances::initial_count(&b.seed))
        });
        let dynamic_used = allocations.iter().filter(|a| a.id != a.prototype).count();
        let cell_capacity = self.doc.oib_binding().map_or(0, |b| {
            if let Some(d) = &b.seed.designer {
                let used = d.mappings.definitions.len()
                    + allocations
                        .iter()
                        .map(|a| {
                            d.mappings
                                .roots
                                .iter()
                                .find(|t| t.prototype.0 == a.prototype)
                                .map_or(64, |t| t.cells.len())
                        })
                        .sum::<usize>();
                let per_root = d
                    .mappings
                    .roots
                    .iter()
                    .filter(|t| prototype.is_none_or(|p| t.prototype.0 == p))
                    .map(|t| t.cells.len())
                    .filter(|n| *n > 0)
                    .min()
                    .unwrap_or(64);
                otrios_interfaces::oib::MAX_CELLS.saturating_sub(used) / per_root
            } else {
                MAX_ALLOCATIONS
            }
        });
        MAX_ALLOCATIONS
            .saturating_sub(allocations.len())
            .min(cell_capacity)
            .min(dynamic_capacity.saturating_sub(dynamic_used))
            .min(
                otrios_interfaces::oib::MAX_LIVE_INSTANCES.saturating_sub(
                    self.doc
                        .oib_live(1)
                        .or_else(|_| self.doc.oib_live(2))
                        .map_or(0, |live| live.len()),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_full_ledger_is_compacted_without_reusing_any_previous_id() {
        let ui = crate::canvas::decode_ui(
            &lz4_flex::decompress_size_prepended(include_bytes!(
                "../../../../tests/fixtures/oib-v24-ui.lz4"
            ))
            .unwrap(),
        )
        .unwrap();
        let mut legacy = Document::new(ui.state_schema.clone(), "oib:legacy-gc").unwrap();
        legacy.configure_oib(&ui).unwrap();
        let seed = super::super::super::super::oib::seed(&ui).unwrap();
        let mut allocations = legacy.oib_allocations().unwrap();
        for index in 2..10 {
            allocations.push(Allocation {
                id: 0x1_0000 + index,
                name: format!("old-{index}"),
                component: 10,
                prototype: 20,
                slot: index as u16,
            });
        }
        #[derive(serde::Serialize)]
        struct OldAllocation<'a> {
            id: u64,
            name: &'a str,
            component: u64,
            prototype: u64,
        }
        let old: Vec<_> = allocations
            .iter()
            .map(|a| OldAllocation {
                id: a.id,
                name: &a.name,
                component: a.component,
                prototype: a.prototype,
            })
            .collect();
        let mut edits = vec![Edit {
            path: vec![4, 4],
            schema: legacy.extra_schema(4, 4),
            value: Some(bytes(&old).unwrap()),
        }];
        for a in allocations.iter().skip(2) {
            for branch in [5, 6] {
                edits.push(Edit {
                    path: cell_path(branch, a.id),
                    schema: legacy.oib_instance_schema(a.id),
                    value: None,
                });
            }
            for property in [100, 101, 102, 103] {
                let cell = super::super::super::super::oib::instances::cell_id(
                    seed,
                    &allocations,
                    a.id,
                    property,
                )
                .unwrap();
                for branch in [1, 2] {
                    edits.push(Edit {
                        path: cell_path(branch, cell),
                        schema: legacy.oib_cell_schema(cell),
                        value: None,
                    });
                }
            }
        }
        legacy.change(&[], edits).unwrap();
        let mut status = legacy.oib_status().unwrap();
        status.active_root = legacy.oib_active_root().unwrap();
        status.source_draft = legacy.oib_draft_root().unwrap();
        legacy
            .change(
                &[],
                vec![Edit {
                    path: vec![4, 3],
                    schema: legacy.extra_schema(4, 3),
                    value: Some(bytes(&status).unwrap()),
                }],
            )
            .unwrap();
        let mut paths: Vec<_> = legacy
            .tree
            .child_keys()
            .filter(|k| *k != 4)
            .map(|k| vec![k])
            .collect();
        paths.extend((0..7).map(|k| vec![4, k]));
        legacy.tree = legacy
            .tree
            .project(&merkle::Coverage::new(&[vec![]], &paths).unwrap())
            .unwrap()
            .0;
        let saved = legacy.save().unwrap();
        let mut current = Document::load(ui.state_schema.clone(), "oib:legacy-gc", &saved).unwrap();
        assert!(current.configure_oib(&ui).unwrap());
        assert_eq!(current.oib_allocations().unwrap().len(), 2);
        assert_eq!(current.oib_next_id().unwrap(), 0x1_000a);
        assert!(current.save().unwrap().len() < saved.len());
        let old_cell = 0x100_0000 + 2 * 64;
        let observed = legacy
            .tree
            .subtree(&cell_path(1, old_cell))
            .unwrap()
            .to_bytes()
            .unwrap();
        current
            .oib_instance_write(
                [4; 16],
                &InstanceOperation::Instantiate {
                    component: 10,
                    name: None,
                },
            )
            .unwrap();
        assert_eq!(
            current.oib_allocations().unwrap().last().unwrap().id,
            0x1_000a
        );
        let root = current.tree.hash();
        current
            .oib_write(
                [5; 16],
                &[otrios_interfaces::oib::Change {
                    cell: old_cell,
                    value: Some(Value::Text("Delayed".into())),
                    observed,
                }],
            )
            .unwrap();
        assert_eq!(current.tree.hash(), root);
        assert!(!current.oib_values(1).unwrap().contains_key(&old_cell));
        let mut recovered = Document::load(
            ui.state_schema.clone(),
            "oib:legacy-gc",
            &current.save().unwrap(),
        )
        .unwrap();
        assert!(!recovered.configure_oib(&ui).unwrap());
        assert_eq!(recovered.oib_next_id().unwrap(), 0x1_000b);
    }
}
