// SPDX-License-Identifier: LGPL-3.0-only
//! Authority-only reclamation. Clients submit typed intents, never old OIB roots.
use super::*;

pub(super) fn first_id(seed: &otrios_interfaces::oib::Seed) -> Result<u64> {
    seed.components
        .iter()
        .chain(&seed.instances)
        .map(|o| o.id)
        .chain(std::iter::once(seed.main_oib))
        .chain(
            seed.designer
                .iter()
                .flat_map(|d| d.instances.iter().map(|i| i.id.0)),
        )
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "OIB identity space exhausted".into())
}
const MAX_ID: u64 = u64::MAX;

impl Document {
    pub(super) fn oib_next_id(&self) -> Result<u64> {
        Ok(self.oib_source()?.next_instance)
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
        if !(first_id(&seed)?..MAX_ID).contains(&id) {
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
            vec![self.oib_source_edit(|source| source.next_instance = id + 1)?],
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
            || (first_id(&seed)?..self.oib_next_id()?).contains(&id))
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
        let mut edits = vec![self.oib_source_edit(|source| {
            source.instances = allocations.iter().map(Into::into).collect()
        })?];
        for a in &allocations {
            edits.push(Edit {
                path: cell_path(5, a.id),
                schema: self.oib_instance_schema(a.id),
                value: Some(bytes(&true)?),
            });
        }
        for cell in &resolved.cells {
            if defaults
                || self
                    .tree
                    .subtree(&address::cell_path(&resolved, 1, cell.id)?)
                    .is_err()
            {
                edits.push(Edit {
                    path: address::cell_path(&resolved, 1, cell.id)?,
                    schema: address::schema(&self.scope, &resolved, cell.id)?,
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
        let mut edits = vec![self.oib_source_edit(|source| {
            source.instances = resident.iter().map(Into::into).collect()
        })?];
        for a in &resident {
            edits.push(Edit {
                path: cell_path(5, a.id),
                schema: self.oib_instance_schema(a.id),
                value: desired.contains(&a.id).then(|| bytes(&true)).transpose()?,
            });
        }
        for cell in &resolved.cells {
            let wanted = cell.inherit.is_none() || desired.contains(&cell.owner);
            if (defaults && wanted)
                || self
                    .tree
                    .subtree(&address::cell_path(&resolved, 1, cell.id)?)
                    .is_err()
            {
                edits.push(Edit {
                    path: address::cell_path(&resolved, 1, cell.id)?,
                    schema: address::schema(&self.scope, &resolved, cell.id)?,
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
        if self.oib_binding()?.seed.instance_surface.is_none() {
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
                vec![self.oib_source_edit(|source| {
                    source.instances = allocations.iter().map(Into::into).collect()
                })?],
            )?;
        }
        let mut keep: Vec<Vec<u64>> = self
            .tree
            .child_keys()
            .filter(|k| *k != 4)
            .map(|k| vec![k])
            .collect();
        keep.extend([0, 3, 4].map(|k| vec![4, k]));
        let resolved = self.oib_resolved_seed()?;
        for cell in &resolved.cells {
            for branch in [1, 2] {
                keep.push(address::cell_path(&resolved, branch, cell.id)?);
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
        if !(first_id(&seed)?..=MAX_ID).contains(&next) || allocations.len() > MAX_ALLOCATIONS {
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
                || (!original && !(first_id(&seed)?..next).contains(&a.id))
                || !seed
                    .instances
                    .iter()
                    .any(|s| s.id == a.prototype && s.component == Some(a.component))
            {
                return Err("Invalid OIB resident descriptor".into());
            }
        }
        if let Some(old) = old {
            let source: Option<otrios_interfaces::oib::authoring::DocumentSource> =
                payload(old, &[4, 4]).ok().map(|v| decode(&v)).transpose()?;
            let previous_next = source.as_ref().map(|s| s.next_instance);
            let previous = source.map_or_else(Vec::new, |s| s.instances);
            if previous_next.is_some_and(|n| n > next) {
                return Err("OIB identity counter regressed".into());
            }
            for a in &allocations {
                if let Some(before) = previous.iter().find(|b| b.id == a.id) {
                    if before.id != a.id
                        || before.name != a.name
                        || before.component != a.component
                        || before.prototype != a.prototype
                    {
                        return Err("OIB resident identity changed".into());
                    }
                } else if previous_next.is_some_and(|n| a.id < n) {
                    return Err("OIB retired identity reused".into());
                }
            }
        }
        let mut size = 0;
        for branch in [1, 2, 4, 5, 6] {
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
