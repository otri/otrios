// SPDX-License-Identifier: LGPL-3.0-only
//! Authority-only reclamation. Clients submit typed intents, never old OIB roots.
use super::*;

pub(super) fn first_id(seed: &otrios_interfaces::oib::Seed) -> Result<u64> {
    let graph = seed.graph.as_ref().ok_or("Missing canonical OIB graph")?;
    graph
        .instances
        .iter()
        .map(|instance| instance.id.0)
        .chain(graph.presets.iter().map(|preset| preset.id))
        .chain(graph.definitions.iter().flat_map(|definition| {
            std::iter::once(definition.id.0)
                .chain(definition.nodes.iter().map(|node| node.id.0))
                .chain(definition.slots.iter().map(|slot| slot.id.0))
        }))
        .chain(
            graph
                .imports
                .as_ref()
                .into_iter()
                .flat_map(|imports| &imports.bindings)
                .flat_map(|binding| {
                    std::iter::once(binding.id).chain(
                        binding
                            .definitions
                            .iter()
                            .map(|definition| definition.target.0),
                    )
                }),
        )
        .chain(std::iter::once(graph.presented_root.0))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "OIB identity space exhausted".into())
}
const MAX_ID: u64 = u64::MAX;

impl Document {
    pub(super) fn oib_next_id(&self) -> Result<u64> {
        Ok(self.oib_source_shared()?.next_identity)
    }

    pub(super) fn oib_allocate(
        &mut self,
        allocations: &[Allocation],
        prototype: u64,
        name: Option<String>,
    ) -> Result<Allocation> {
        let ui = self.oib_authored_ui()?;
        let seed = super::super::super::oib::seed(&ui).ok_or("Missing authored graph")?;
        let source = seed
            .instances
            .iter()
            .find(|i| i.id == prototype)
            .ok_or("Undeclared OIB factory")?;
        let slot = (0..MAX_ALLOCATIONS as u16)
            .find(|slot| {
                allocations
                    .iter()
                    .all(|allocation| allocation.slot != *slot)
            })
            .ok_or("OIB resident instance capacity exhausted")?;
        let id = self.oib_allocating_id()?;
        if !(self.oib_binding()?.identity_floor..MAX_ID).contains(&id) {
            return Err("OIB identity counter exhausted".into());
        }
        let name = name.unwrap_or_else(|| format!("instance-{id}"));
        if !admitted_instance_name(seed, &name) {
            return Err(
                "Instance names use 1–64 ASCII letters, digits, hyphens or underscores".into(),
            );
        }
        self.oib_claim_implicit(id)?;
        self.change(
            &[],
            vec![self.oib_source_edit(|source| {
                source.next_identity = source.next_identity.max(id + 1)
            })?],
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
        let seed = self.oib_binding()?.seed.clone();
        Ok(
            super::super::super::oib::instances::initial_allocations(&seed)
                .iter()
                .any(|i| i.id == id)
                || (self.oib_binding()?.identity_floor..self.oib_next_id()?).contains(&id),
        )
    }

    pub(super) fn oib_retired_property(
        &self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
    ) -> Result<bool> {
        use otrios_interfaces::oib::{authoring::BaselineIdentity, designer::*};
        let source = self.oib_source_shared()?;
        let current = self.oib_authored_shared()?;
        let metadata = current
            .designer
            .as_ref()
            .ok_or("Missing resolved metadata")?;
        let current = &metadata.definitions;
        let first = self.oib_binding()?.identity_floor;
        let allocated = |identity| (first..source.next_identity).contains(&identity);
        fn resolve<'a>(
            definitions: &'a [ComponentDefinition],
            mut definition: DefinitionId,
            references: &[NodeId],
            node: NodeId,
        ) -> Option<(&'a ComponentDefinition, &'a DefinitionNode)> {
            for boundary in references {
                let next = definitions
                    .iter()
                    .find(|current| current.id == definition)
                    .and_then(|current| current.nodes.iter().find(|node| node.id == *boundary))
                    .and_then(|node| node.reference.as_ref())?;
                definition = next.definition;
            }
            let definition = definitions
                .iter()
                .find(|current| current.id == definition)?;
            Some((
                definition,
                definition.nodes.iter().find(|current| current.id == node)?,
            ))
        }
        let contains =
            |definitions: &[ComponentDefinition], definition, references: &[NodeId], node| {
                resolve(definitions, definition, references, node).is_some()
            };
        let retired_endpoint = |definition,
                                references: &[NodeId],
                                node,
                                property: PropertyId|
         -> Result<Option<bool>> {
            let Some((definition, node)) = resolve(current, definition, references, node) else {
                return Ok(None);
            };
            if node.id != definition.root
                || definition.next_property == 0
                || property.0 >= definition.next_property
                || definition
                    .properties
                    .iter()
                    .any(|descriptor| descriptor.id == property)
            {
                return Ok(Some(false));
            }
            let inherited = super::super::super::oib::document::class_descriptors(
                &metadata.classes,
                node.class,
            )?;
            Ok(Some(
                property.0
                    > inherited
                        .iter()
                        .map(|descriptor| descriptor.id.0)
                        .max()
                        .unwrap_or(0),
            ))
        };
        match address {
            PropertyAddress::Definition {
                definition,
                node,
                property,
            } => {
                if let Some(retired) = retired_endpoint(*definition, &[], *node, *property)? {
                    return Ok(retired);
                }
                let signed = self.oib_baseline_graph()?;
                Ok(contains(&signed.definitions, *definition, &[], *node)
                    || allocated(definition.0)
                    || allocated(node.0))
            }
            PropertyAddress::Occurrence { instance, path } => {
                if self.oib_retired_instance(instance.0)? {
                    return Ok(true);
                }
                let Some(root) = source.instances.iter().find(|root| root.id == instance.0) else {
                    return Ok(false);
                };
                let definition = DefinitionId(root.component);
                if let Some(retired) =
                    retired_endpoint(definition, &path.references, path.node, path.property)?
                {
                    return Ok(retired);
                }
                let original = source
                    .baseline
                    .iter()
                    .find_map(|mapping| match mapping {
                        BaselineIdentity::Definition { source, current }
                            if *current == definition =>
                        {
                            Some(*source)
                        }
                        _ => None,
                    })
                    .unwrap_or(definition);
                let signed = self.oib_baseline_graph()?;
                Ok(
                    contains(&signed.definitions, original, &path.references, path.node)
                        || allocated(definition.0)
                        || allocated(path.node.0)
                        || path.references.iter().any(|node| allocated(node.0)),
                )
            }
            _ => Ok(false),
        }
    }

    pub(super) fn oib_restore_residents(&mut self, defaults: bool, writer: Actor) -> Result<()> {
        use otrios_interfaces::oib::{authoring::BaselineIdentity, designer::*};
        if defaults {
            return Err("Reset defaults uses the baseline restoration candidate".into());
        }
        let signed = self.oib_binding()?.seed.clone();
        let initial = super::super::super::oib::instances::initial_allocations(&signed);
        let source = self.oib_source()?;
        let live = self.oib_live(1)?;
        let mut resident = self.oib_allocations()?;
        let mut desired = Vec::new();
        let mut baseline = source.baseline.clone();
        for original in initial {
            let preset = baseline
                .iter()
                .find_map(|mapping| match mapping {
                    BaselineIdentity::Preset { source, current }
                        if *source == original.prototype =>
                    {
                        Some(*current)
                    }
                    _ => None,
                })
                .unwrap_or(original.prototype);
            let definition = baseline
                .iter()
                .find_map(|mapping| match mapping {
                    BaselineIdentity::Definition { source, current }
                        if source.0 == original.component =>
                    {
                        Some(current.0)
                    }
                    _ => None,
                })
                .unwrap_or(original.component);
            if !source.presets.iter().any(|current| {
                current.id == preset && current.definition.0 == definition && current.palette
            }) {
                return Err("An authored initial preset was removed or rebound; Reset defaults restores the baseline".into());
            }
            let identity = baseline
                .iter()
                .find_map(|mapping| match mapping {
                    BaselineIdentity::Instance { source, current } if source.0 == original.id => {
                        Some(current.0)
                    }
                    _ => None,
                })
                .unwrap_or(original.id);
            if let Some(existing) = resident
                .iter()
                .find(|current| current.id == identity && live.contains(&current.id))
            {
                if existing.prototype != preset || existing.component != definition {
                    return Err("Initial instance role was rebound".into());
                }
                desired.push(existing.clone());
            } else {
                let new = self.oib_allocate(&resident, preset, Some(original.name))?;
                baseline.retain(|mapping| !matches!(mapping, BaselineIdentity::Instance { source, .. } if source.0 == original.id));
                baseline.push(BaselineIdentity::Instance {
                    source: InstanceId(original.id),
                    current: InstanceId(new.id),
                });
                resident.push(new.clone());
                desired.push(new);
            }
        }
        // Canonical initial graph currently admits canvas-owned mutable roots
        // only. No name matching or guessed replacement hosts are involved.
        let mut edits = vec![self.oib_source_edit(|source| {
            source.instances = desired.iter().map(Into::into).collect();
            source.baseline = baseline;
            source.placements.clear();
            source.connections.clear();
        })?];
        for allocation in desired {
            edits.push(Edit {
                path: cell_path(5, allocation.id),
                schema: self.oib_instance_schema(allocation.id),
                value: Some(bytes(&true)?),
            });
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
        let resolved = self.oib_resolved_shared()?;
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
        let seed = self.oib_authored_shared()?;
        let allocations = self.oib_allocations()?;
        let next = self.oib_next_id()?;
        let first = self.oib_binding()?.identity_floor;
        // This exact source/binding/UI metadata already qualified the same
        // expansion; raw descriptor and causal presence checks below still run.
        self.oib_resolved_shared()?;
        let mut slots = BTreeSet::new();
        let mut ids = BTreeSet::new();
        if !(first..=MAX_ID).contains(&next) || allocations.len() > MAX_ALLOCATIONS {
            return Err("Invalid OIB resident allocator".into());
        }
        let initial = super::super::super::oib::instances::initial_allocations(&seed);
        for a in &allocations {
            let original = initial.iter().any(|source| {
                source.id == a.id
                    && source.prototype == a.prototype
                    && source.component == a.component
            });
            if !admitted_instance_name(&seed, &a.name)
                || !ids.insert(a.id)
                || !slots.insert(a.slot)
                || usize::from(a.slot) >= MAX_ALLOCATIONS
                || (!original && !(first..next).contains(&a.id))
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
            let previous_next = source.as_ref().map(|s| s.next_identity);
            let previous = source.map_or_else(Vec::new, |s| s.instances);
            if previous_next.is_some_and(|n| n > next) {
                return Err("OIB identity counter regressed".into());
            }
            for a in &allocations {
                if let Some(before) = previous.iter().find(|b| b.id == a.id) {
                    if before.id != a.id
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
            let tree = match self.tree.subtree(&[4, branch]) {
                Ok(tree) => tree,
                Err(merkle::Error::Missing)
                    if branch == 5 && !self.oib_projection && !ids.is_empty() =>
                {
                    return Err("Missing OIB draft presence in complete checkpoint".into());
                }
                Err(_) => continue,
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
        let cell_capacity = self.doc.oib_authored_shared().map_or(0, |seed| {
            if let Some(d) = &seed.designer {
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
                    .min();
                match per_root {
                    Some(0) => MAX_ALLOCATIONS,
                    Some(per_root) => {
                        otrios_interfaces::oib::MAX_CELLS.saturating_sub(used) / per_root
                    }
                    None => 0,
                }
            } else {
                MAX_ALLOCATIONS
            }
        });
        MAX_ALLOCATIONS
            .saturating_sub(allocations.len())
            .min(cell_capacity)
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
