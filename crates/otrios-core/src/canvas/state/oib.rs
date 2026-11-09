// SPDX-License-Identifier: LGPL-3.0-only
//! OIB shares the app checkpoint, journal, receipts and publication root.
use super::super::oib::document::PropertyRegisterView;
use super::*;
use otrios_interfaces::oib::{
    Allocation, Change, InspectorAction, InstanceAction, InstanceOperation, MAX_ALLOCATIONS,
    MAX_CHANGES,
};
use std::collections::BTreeSet;
mod address;
mod hierarchy;
mod lifecycle;
mod source;
mod structure;

/// One disposable compiled structural revision, shared by document clones and
/// keyed by both signed admission identity and canonical authored source hash.
pub(super) struct AuthoredUi {
    signed: std::sync::Arc<Ui>,
    source: merkle::Hash,
    ui: std::sync::Arc<Ui>,
}
const MAX_STRUCTURE_BYTES: usize = otrios_interfaces::oib::authoring::MAX_SOURCE_BYTES;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub generation: u64,
    pub active_root: merkle::Hash,
    pub source_draft: merkle::Hash,
    pub conflicts: Vec<otrios_interfaces::oib::designer::PropertyAddress>,
    pub error: Option<String>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    seed: otrios_interfaces::oib::Seed,
    admission: Vec<u8>,
    ui_hash: Vec<u8>,
    ui_ref: String,
}
pub fn cell_path(branch: u64, id: u64) -> Vec<u64> {
    let mut p = vec![4, branch];
    p.extend((0..13).rev().map(|i| (id >> (i * 5)) & 31));
    p
}
impl Document {
    pub fn configure_oib(&mut self, ui: &Ui) -> Result<bool> {
        let Some(seed) = super::super::oib::seed(ui) else {
            if self.tree.subtree(&[4]).is_ok() && self.oib_binding()?.ui_ref == ui.ui_ref {
                return Err("Installed app no longer supports retained OIB data".into());
            }
            return Ok(false);
        };
        super::super::oib::validate(ui)?;
        let binding = Binding {
            seed: seed.clone(),
            admission: hash(&bytes(&(
                &ui.state_schema,
                &ui.selectors,
                &ui.handlers,
                &ui.behavior,
            ))?),
            ui_hash: hash(&bytes(ui)?),
            ui_ref: ui.ui_ref.clone(),
        };
        let encoded = bytes(&binding)?;
        let existing = payload(&self.tree, &[4, 0]).ok();
        if let Some(existing) = &existing {
            let previous: Binding = decode(existing)?;
            if !same_source(&previous, &binding) {
                return Err(
                    "OIB format or signed source changed; initialize a fresh collection".into(),
                );
            }
            if *existing == encoded {
                self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
                self.oib_compiled(2)?;
                return Ok(false);
            }
        }
        self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
        let mut status = if existing.is_some() {
            self.oib_status()?
        } else {
            Status {
                generation: 1,
                active_root: [0; 32],
                source_draft: [0; 32],
                conflicts: vec![],
                error: None,
            }
        };
        if existing.is_some() {
            status.generation = status
                .generation
                .checked_add(1)
                .ok_or("OIB generation exhausted")?;
        }
        let mut edits = vec![Edit {
            path: vec![4, 0],
            schema: self.extra_schema(4, 0),
            value: Some(encoded),
        }];
        if existing.is_none() {
            for cell in seed.cells.iter().filter(|c| {
                c.inherit.is_none() || !super::super::oib::instances::palette_only(seed, c.owner)
            }) {
                let value = if cell.inherit.is_some() {
                    None
                } else {
                    Some(bytes(&cell.default)?)
                };
                for branch in [1, 2] {
                    edits.push(Edit {
                        path: address::cell_path(seed, branch, cell.id)?,
                        schema: address::schema(&self.scope, seed, cell.id)?,
                        value: value.clone(),
                    });
                }
            }
            let allocations = super::super::oib::instances::initial_allocations(seed);
            let next = allocations
                .iter()
                .map(|a| a.id.saturating_add(1))
                .max()
                .unwrap_or(0)
                .max(lifecycle::first_id(seed)?);
            let source = otrios_interfaces::oib::authoring::DocumentSource {
                definitions: seed
                    .designer
                    .as_ref()
                    .map_or_else(Vec::new, |d| d.definitions.clone()),
                instances: allocations.iter().map(Into::into).collect(),
                next_instance: next,
                placements: vec![],
                connections: vec![],
            };
            edits.push(Edit {
                path: vec![4, 4],
                schema: self.extra_schema(4, 4),
                value: Some(bytes(&source)?),
            });
            if seed.instance_surface.is_some() {
                for allocation in &allocations {
                    for branch in [5, 6] {
                        edits.push(Edit {
                            path: cell_path(branch, allocation.id),
                            schema: self.oib_instance_schema(allocation.id),
                            value: Some(bytes(&true)?),
                        });
                    }
                }
            }
        }
        self.change(&[], edits)?;
        status.active_root = self.oib_active_root()?;
        status.source_draft = self.oib_draft_root()?;
        self.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )?;
        Ok(true)
    }
    fn oib_ui(&self) -> Result<&Ui> {
        self.oib_ui
            .as_deref()
            .ok_or_else(|| "OIB package must be admitted before resolving".into())
    }
    fn oib_binding(&self) -> Result<Binding> {
        decode(&payload(&self.tree, &[4, 0])?)
    }
    fn oib_authored_ui(&self) -> Result<std::sync::Arc<Ui>> {
        let signed = self.oib_ui.clone().ok_or("Missing admitted OIB UI")?;
        if super::super::oib::seed(&signed).is_none_or(|seed| seed.designer.is_none()) {
            return Ok(signed);
        }
        let definitions = self.oib_definition_snapshot()?;
        let source = digest(&bytes(&definitions)?);
        let mut cache = self.oib_authored.lock().map_err(err)?;
        if let Some(cached) = cache.as_ref()
            && std::sync::Arc::ptr_eq(&signed, &cached.signed)
            && cached.source == source
        {
            return Ok(cached.ui.clone());
        }
        let ui = std::sync::Arc::new(super::super::oib::document::authoring::restore(
            &signed,
            &definitions,
        )?);
        *cache = Some(AuthoredUi {
            signed,
            source,
            ui: ui.clone(),
        });
        Ok(ui)
    }
    fn oib_definition_snapshot(
        &self,
    ) -> Result<Vec<otrios_interfaces::oib::designer::ComponentDefinition>> {
        Ok(self.oib_source()?.definitions)
    }
    fn oib_authored_seed(&self) -> Result<otrios_interfaces::oib::Seed> {
        let mut seed = if self.oib_ui.is_some() {
            super::super::oib::seed(&*self.oib_authored_ui()?)
                .ok_or("Missing OIB seed")?
                .clone()
        } else {
            let signed = self.oib_binding()?.seed;
            if signed.designer.is_none() {
                signed
            } else {
                super::super::oib::document::authoring::restore_seed(
                    &signed,
                    &self.oib_definition_snapshot()?,
                )?
            }
        };
        if let Some(doc) = &mut seed.designer {
            doc.placements = self.oib_placements()?;
            doc.connections = self.oib_connections()?;
        }
        Ok(seed)
    }
    /// Collection incarnation plus signed source. Fresh initialization cannot
    /// accept an old client's still-in-flight property or structural intent.
    pub fn oib_epoch(&self) -> Result<Vec<u8>> {
        let binding = self.oib_binding()?;
        Ok(hash(&bytes(&(
            "otrios-oib-epoch-v2",
            self.actor,
            binding.seed,
            binding.admission,
        ))?))
    }
    /// Read a semantic register through a disposable compiler handle. The handle
    /// is resolved anew; it is never the register key or a state request field.
    pub fn oib_property_leaf(&self, branch: u64, cell: u64) -> Result<Snapshot> {
        self.tree
            .subtree(&address::cell_path(
                &self.oib_resolved_seed()?,
                branch,
                cell,
            )?)
            .map_err(err)
    }
    pub(super) fn check_oib_epoch(&self, epoch: &[u8]) -> Result<()> {
        if epoch != self.oib_epoch()? {
            return Err("OIB document changed; reconnect before editing".into());
        }
        Ok(())
    }
    pub fn oib_status(&self) -> Result<Status> {
        decode(&payload(&self.tree, &[4, 3])?)
    }
    fn oib_instance_schema(&self, id: u64) -> merkle::Hash {
        digest(
            &[
                b"OTriOS\0oib-instance-v1\0".as_slice(),
                self.scope.as_bytes(),
                &id.to_le_bytes(),
            ]
            .concat(),
        )
    }
    pub fn oib_allocations(&self) -> Result<Vec<Allocation>> {
        let source = self.oib_source()?;
        let signed = super::super::oib::instances::initial_allocations(&self.oib_binding()?.seed);
        let mut dynamic: Vec<_> = source
            .instances
            .iter()
            .filter(|a| a.id != a.prototype)
            .map(|a| a.id)
            .collect();
        dynamic.sort_unstable();
        source
            .instances
            .into_iter()
            .map(|a| {
                let slot = if a.id == a.prototype {
                    signed.iter().find(|s| s.id == a.id).map(|s| s.slot)
                } else {
                    dynamic
                        .iter()
                        .position(|id| *id == a.id)
                        .map(|slot| slot as u16)
                }
                .ok_or("Unknown OIB resident identity")?;
                Ok(Allocation {
                    id: a.id,
                    name: a.name,
                    component: a.component,
                    prototype: a.prototype,
                    slot,
                })
            })
            .collect()
    }
    pub fn oib_live(&self, branch: u64) -> Result<BTreeSet<u64>> {
        let seed = if let Some(ui) = &self.oib_ui {
            super::super::oib::seed(ui)
                .ok_or("Missing OIB seed")?
                .clone()
        } else {
            self.oib_binding()?.seed
        };
        if seed.instance_surface.is_none() {
            return Ok(seed.instances.iter().map(|i| i.id).collect());
        }
        let mut live = BTreeSet::new();
        for a in self.oib_allocations()? {
            let leaf = match self
                .tree
                .subtree(&cell_path(if branch == 1 { 5 } else { 6 }, a.id))
            {
                Ok(leaf) => leaf,
                Err(_) if branch == 2 => continue,
                Err(e) => return Err(err(e)),
            };
            let register = leaf.register().ok_or("Missing instance presence")?;
            // Removal wins: editing a retained property cannot resurrect an object.
            if register.values().all(|(_, _, v)| v.is_some()) {
                live.insert(a.id);
            }
        }
        Ok(live)
    }
    fn oib_resolved_seed(&self) -> Result<otrios_interfaces::oib::Seed> {
        let seed = self.oib_authored_seed()?;
        super::super::oib::instances::expanded_seed(&seed, &self.oib_allocations()?)
    }
    fn oib_active_root(&self) -> Result<merkle::Hash> {
        let properties = self.tree.subtree(&[4, 2]).map_err(err)?.hash();
        if self.tree.subtree(&[4, 6]).is_err() {
            return self.oib_structural_root(properties, 2);
        }
        let instances = self.tree.subtree(&[4, 6]).map_err(err)?.hash();
        let root = digest(
            &[
                b"OTriOS\0oib-active-v2\0".as_slice(),
                &properties,
                &instances,
            ]
            .concat(),
        );
        self.oib_structural_root(root, 2)
    }
    fn oib_draft_root(&self) -> Result<merkle::Hash> {
        let properties = self.tree.subtree(&[4, 1]).map_err(err)?.hash();
        let Ok(instances) = self.tree.subtree(&[4, 5]) else {
            return self.oib_structural_root(properties, 1);
        };
        let root = digest(
            &[
                b"OTriOS\0oib-draft-v2\0".as_slice(),
                &properties,
                &instances.hash(),
            ]
            .concat(),
        );
        self.oib_structural_root(root, 1)
    }
    fn oib_structural_root(
        &self,
        properties_and_presence: merkle::Hash,
        branch: u64,
    ) -> Result<merkle::Hash> {
        let mut source = self.oib_source()?;
        let live = self.oib_live(branch)?;
        source
            .instances
            .retain(|instance| live.contains(&instance.id));
        // The allocation cursor and retained inactive descriptors are residency
        // bookkeeping. They cannot change the published graph of a slow reader.
        source.next_instance = 0;
        Ok(digest(
            &[
                b"OTriOS\0oib-document-v2\0".as_slice(),
                &properties_and_presence,
                &digest(&bytes(&source)?),
            ]
            .concat(),
        ))
    }
    pub fn oib_compiled(&self, branch: u64) -> Result<(Ui, crate::dashboard::layout::Layout)> {
        self.oib_compiled_selected(branch, None)
    }
    pub fn oib_compiled_selected(
        &self,
        branch: u64,
        selected: Option<u64>,
    ) -> Result<(Ui, crate::dashboard::layout::Layout)> {
        let mut ui = (*self.oib_authored_ui()?).clone();
        if let Some(Presentation::Oib(seed)) = &mut ui.presentation
            && let Some(doc) = &mut seed.designer
        {
            doc.placements = self.oib_placements()?;
            doc.connections = self.oib_connections()?;
        }
        let values = self.oib_values(branch)?;
        let resolved = super::super::oib::instances::expand(
            &ui,
            &self.oib_allocations()?,
            &self.oib_live(branch)?,
            selected,
            &values,
        )?;
        let layout = super::super::oib::compile(&resolved, &values)?;
        Ok((resolved, layout))
    }
    pub fn oib_values(&self, branch: u64) -> Result<BTreeMap<u64, Option<Value>>> {
        let seed = self.oib_resolved_seed()?;
        let live = self.oib_live(branch)?;
        let mut values = BTreeMap::new();
        for cell in &seed.cells {
            let leaf = match self
                .tree
                .subtree(&address::cell_path(&seed, branch, cell.id)?)
            {
                Ok(leaf) => leaf,
                Err(_) if branch == 2 && cell.inherit.is_some() => {
                    values.insert(cell.id, None);
                    continue;
                }
                Err(e) => return Err(err(e)),
            };
            let register = leaf.register().ok_or("Missing OIB register")?;
            let mut selected = None;
            let mut first = true;
            for (_, _, data) in register.values() {
                let value = data.map(decode::<Value>).transpose()?;
                if !first && selected != value {
                    if branch == 1 && cell.inherit.is_some() && !live.contains(&cell.owner) {
                        selected = None;
                        break;
                    }
                    return Err("Unresolved OIB conflict".into());
                }
                selected = value;
                first = false;
            }
            values.insert(cell.id, selected);
        }
        Ok(values)
    }
    pub(super) fn validate_oib(&self, old: Option<&Snapshot>, tree: &Snapshot) -> Result<()> {
        if let Some(old) = old
            && let Ok(before) = payload(old, &[4, 0])
            && before != payload(tree, &[4, 0])?
            && !same_source(&decode(&before)?, &decode(&payload(tree, &[4, 0])?)?)
        {
            return Err("OIB immutable seed changed".into());
        }
        let mut candidate = self.clone();
        candidate.tree = tree.clone();
        let binding = candidate.oib_binding()?;
        super::super::oib::validate_seed(&binding.seed)?;
        if binding.ui_hash.len() != 32
            || binding.admission.len() != 32
            || binding.ui_ref.is_empty()
            || binding.ui_ref.len() > 128
        {
            return Err("Invalid OIB seed digest".into());
        }
        candidate.validate_allocations(old)?;
        let resolved = candidate.oib_resolved_seed()?;
        let seed = &resolved;
        let branch = tree.subtree(&[4]).map_err(err)?;
        if branch.register().is_some()
            || branch.child_keys().any(|k| {
                k > if binding.seed.instance_surface.is_some() {
                    6
                } else {
                    4
                }
            })
        {
            return Err("Invalid OIB branch".into());
        }
        for index in [0, 3, 4] {
            let leaf = tree.subtree(&[4, index]).map_err(err)?;
            if leaf.child_keys().next().is_some()
                || leaf
                    .register()
                    .is_none_or(|r| r.schema() != self.extra_schema(4, index as usize))
            {
                return Err("Invalid OIB metadata".into());
            }
        }
        candidate.oib_source()?;
        let total = [1, 2, 4, 5, 6]
            .iter()
            .filter_map(|kind| tree.subtree(&[4, *kind]).ok())
            .map(|t| t.canonical_bytes())
            .sum::<usize>();
        if total > otrios_interfaces::oib::MAX_BYTES {
            return Err("OIB combined authored/causal byte budget".into());
        }
        let status = candidate.oib_status()?;
        if status.generation == 0
            || status.conflicts.len() > 64
            || status.error.as_ref().is_some_and(|s| s.len() > 256)
            || status.conflicts.iter().any(|address| {
                !seed.cells.iter().any(|c| {
                    super::super::oib::document::address_for_cell(seed, c.id)
                        .ok()
                        .as_ref()
                        == Some(address)
                })
            })
        {
            return Err("Invalid OIB status".into());
        }
        for kind in [1, 2] {
            let Ok(branch) = tree.subtree(&[4, kind]) else {
                continue;
            };
            let mut seen = BTreeSet::new();
            address::leaves(&branch, &mut vec![4, kind], &mut seen)?;
            let expected: BTreeSet<_> = seed
                .cells
                .iter()
                .filter(|c| {
                    kind == 1
                        || seed.instance_surface.is_none()
                        || c.inherit.is_none()
                        || tree.subtree(&cell_path(6, c.owner)).is_ok()
                })
                .map(|c| address::cell_path(seed, kind, c.id))
                .collect::<Result<_>>()?;
            if seen != expected {
                return Err("OIB property table changed".into());
            }
            if kind == 1 && branch.canonical_bytes() > otrios_interfaces::oib::MAX_BYTES {
                return Err("OIB draft byte budget".into());
            }
            for cell in &seed.cells {
                if kind == 2 && !seen.contains(&address::cell_path(seed, kind, cell.id)?) {
                    continue;
                }
                let leaf = tree
                    .subtree(&address::cell_path(seed, kind, cell.id)?)
                    .map_err(err)?;
                let r = leaf.register().ok_or("Missing OIB cell")?;
                if r.schema() != address::schema(&self.scope, seed, cell.id)? {
                    return Err("Wrong OIB cell schema".into());
                }
                for (_, _, value) in r.values() {
                    if let Some(value) = value {
                        let value = decode(value)?;
                        super::super::oib::validate_value(cell, &value)?;
                        super::super::oib::document::validate_edit(seed, cell.id, Some(&value))?;
                    } else if cell.inherit.is_none() {
                        return Err("Deleted OIB definition default".into());
                    }
                }
            }
            if kind == 2 {
                if status.active_root != candidate.oib_active_root()? {
                    return Err("OIB active closure mismatch".into());
                }
                candidate.oib_values(2)?;
                if self.oib_ui.is_some() {
                    candidate.oib_compiled(2)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn oib_write(&mut self, writer: Actor, changes: &[Change]) -> Result<()> {
        if changes.is_empty() || changes.len() > MAX_CHANGES {
            return Err("OIB change batch limit".into());
        }
        let seed = self.oib_resolved_seed()?;
        let mut next = self.tree.clone();
        let mut seen = BTreeSet::new();
        for change in changes {
            if let otrios_interfaces::oib::designer::PropertyAddress::Occurrence {
                instance, ..
            } = &change.address
                && self.oib_retired_instance(instance.0)?
            {
                continue;
            }
            let id = super::super::oib::document::cell_for_address(
                &seed,
                &self.oib_allocations()?,
                &change.address,
            )?;
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == id)
                .ok_or("Unknown OIB cell")?;
            if !seen.insert(cell.id) {
                return Err("Duplicate OIB cell edit".into());
            }
            super::super::oib::document::validate_edit(&seed, cell.id, change.value.as_ref())?;
            if let Some(value) = &change.value {
                super::super::oib::validate_value(cell, value)?;
            } else if cell.inherit.is_none() {
                return Err("Only instance overrides can be cleared".into());
            }
            let observed = Snapshot::from_bytes(&change.observed).map_err(err)?;
            let edit = Edit {
                path: address::cell_path(&seed, 1, cell.id)?,
                schema: address::schema(&self.scope, &seed, cell.id)?,
                value: change.value.as_ref().map(bytes).transpose()?,
            };
            next = next.edit_observed(writer, &edit, &observed).map_err(err)?;
        }
        if seen.is_empty() {
            return Ok(());
        }
        self.activate_oib(next)
    }
    fn activate_oib(&mut self, next: Snapshot) -> Result<()> {
        let seed = self.oib_resolved_seed()?;
        let mut status = self.oib_status()?;
        status.conflicts.clear();
        status.error = None;
        let mut draft = self.clone();
        draft.tree = next;
        let live = draft.oib_live(1)?;
        for cell in &seed.cells {
            if cell.inherit.is_some() && !live.contains(&cell.owner) {
                continue;
            }
            let leaf = draft
                .tree
                .subtree(&address::cell_path(&seed, 1, cell.id)?)
                .map_err(err)?;
            let alternatives: BTreeSet<_> = leaf
                .register()
                .ok_or("Missing OIB cell")?
                .values()
                .map(|(_, _, v)| v)
                .collect();
            if alternatives.len() > 1 {
                status
                    .conflicts
                    .push(super::super::oib::document::address_for_cell(
                        &seed, cell.id,
                    )?);
            }
        }
        let mut edits = Vec::new();
        if status.conflicts.is_empty() {
            let values = draft.oib_values(1)?;
            match draft.oib_compiled(1) {
                Ok(_) => {
                    status.generation = status
                        .generation
                        .checked_add(1)
                        .ok_or("OIB generation exhausted")?;
                    status.source_draft = draft.oib_draft_root()?;
                    if seed.instance_surface.is_some() {
                        for allocation in draft.oib_allocations()? {
                            let value = live
                                .contains(&allocation.id)
                                .then(|| bytes(&true))
                                .transpose()?;
                            let matches = self
                                .tree
                                .subtree(&cell_path(6, allocation.id))
                                .ok()
                                .and_then(|leaf| {
                                    leaf.register()
                                        .and_then(|r| r.values().next())
                                        .map(|(_, _, v)| v == value.as_deref())
                                })
                                .unwrap_or(false);
                            if !matches {
                                edits.push(Edit {
                                    path: cell_path(6, allocation.id),
                                    schema: self.oib_instance_schema(allocation.id),
                                    value,
                                });
                            }
                        }
                    }
                    for cell in &seed.cells {
                        let value = values[&cell.id].as_ref().map(bytes).transpose()?;
                        if self
                            .tree
                            .subtree(&address::cell_path(&seed, 2, cell.id)?)
                            .is_ok_and(|old| {
                                old.register().is_some_and(|r| {
                                    r.values()
                                        .next()
                                        .is_some_and(|(_, _, v)| v == value.as_deref())
                                })
                            })
                        {
                            continue;
                        }
                        edits.push(Edit {
                            path: address::cell_path(&seed, 2, cell.id)?,
                            schema: address::schema(&self.scope, &seed, cell.id)?,
                            value,
                        });
                    }
                }
                Err(e) => status.error = Some(e.chars().take(256).collect()),
            }
        }
        draft.change(&[], edits)?;
        draft.oib_reclaim()?;
        if status.error.is_none() && status.conflicts.is_empty() {
            status.source_draft = draft.oib_draft_root()?;
        }
        status.active_root = draft.oib_active_root()?;
        draft.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )?;
        draft.validate_oib(Some(&self.tree), &draft.tree)?;
        self.tree = draft.tree;
        Ok(())
    }
}

impl StateAdapter {
    pub(in crate::canvas) fn oib_saving(&self) -> bool {
        self.flight
            .as_ref()
            .is_some_and(|f| !f.oib.is_empty() || f.oib_instance.is_some())
    }
    pub(in crate::canvas) fn oib_root(&self) -> Option<merkle::Hash> {
        self.doc.tree.subtree(&[4]).ok().map(|n| n.hash())
    }
    pub(in crate::canvas) fn oib_info(&self) -> Result<super::super::oib::Info> {
        let resolved = self.doc.oib_resolved_seed()?;
        let seed = &resolved;
        let status = self.doc.oib_status()?;
        let values = self.doc.oib_values(2)?;
        let mut properties = BTreeMap::new();
        for cell in &seed.cells {
            properties.insert(cell.id, property_text(seed, cell, &status, &values)?);
        }
        Ok(super::super::oib::Info {
            active_generation: status.generation,
            presented_generation: 0,
            conflicts: self.oib_conflicts(),
            status: self.oib_status_text(),
            properties,
            instances: self
                .doc
                .oib_allocations()?
                .into_iter()
                .map(|a| super::super::oib::InstanceInfo {
                    active: self.doc.oib_live(2).is_ok_and(|live| live.contains(&a.id)),
                    id: a.id,
                    name: a.name,
                    component: a.component,
                    prototype: a.prototype,
                })
                .collect(),
            selected_instance: None,
            available_instance_slots: self.oib_available_slots(),
        })
    }
    pub(in crate::canvas) fn admitted_oib_hash(&self) -> Option<Vec<u8>> {
        self.doc.oib_binding().ok().map(|b| b.ui_hash)
    }
    pub fn oib_instances(&self) -> Result<Vec<super::super::oib::InstanceInfo>> {
        Ok(self.oib_info()?.instances)
    }
    pub fn oib_conflicts(&self) -> Vec<u64> {
        let Ok(seed) = self.doc.oib_resolved_seed() else {
            return vec![];
        };
        let Ok(status) = self.doc.oib_status() else {
            return vec![];
        };
        seed.cells
            .iter()
            .filter_map(|cell| {
                let address = super::super::oib::document::address_for_cell(&seed, cell.id).ok()?;
                status.conflicts.contains(&address).then_some(cell.id)
            })
            .collect()
    }
    pub fn oib_property_text(&self, id: u64) -> String {
        let result = (|| -> Result<String> {
            let resolved = self.doc.oib_resolved_seed()?;
            let seed = &resolved;
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == id)
                .ok_or("Unknown OIB property")?;
            let status = self.doc.oib_status()?;
            let values = self.doc.oib_values(2)?;
            property_text(seed, cell, &status, &values)
        })();
        result.unwrap_or_else(|_| "UI unavailable".into())
    }

    pub fn oib_generation(&self) -> Option<u64> {
        self.doc.oib_status().ok().map(|s| s.generation)
    }
    pub fn oib_active(&self) -> Result<BTreeMap<u64, Option<Value>>> {
        self.doc.oib_values(2)
    }
    pub(in crate::canvas) fn oib_compiled(
        &self,
        selected: Option<u64>,
    ) -> Result<(Ui, crate::dashboard::layout::Layout)> {
        self.doc.oib_compiled_selected(2, selected)
    }
    pub fn oib_status_text(&self) -> String {
        let Ok(status) = self.doc.oib_status() else {
            return "Live UI unavailable".into();
        };
        if !status.conflicts.is_empty() {
            format!(
                "{} conflicting properties — resolve in inspector",
                status.conflicts.len()
            )
        } else if let Some(error) = &status.error {
            format!("Draft: {}", error.chars().take(84).collect::<String>())
        } else if self.oib_saving() {
            "Saving UI change…".into()
        } else if self
            .doc
            .oib_binding()
            .is_ok_and(|b| b.seed.instance_surface.is_some())
        {
            format!(
                "Live · revision {} · {} instances · {} slots available",
                status.generation,
                self.doc.oib_live(2).map_or(0, |v| v.len()),
                self.oib_available_slots()
            )
        } else {
            format!("Live · revision {}", status.generation)
        }
    }

    pub fn edit_oib(&mut self, changes: &[(u64, Option<Value>)]) -> Result<bool> {
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        if changes.is_empty() || changes.len() > MAX_CHANGES {
            return Err("OIB change batch limit".into());
        }
        let resolved = self.doc.oib_resolved_seed()?;
        let seed = &resolved;
        let mut edits = Vec::new();
        let mut seen = BTreeSet::new();
        for (id, value) in changes {
            if !seen.insert(*id) {
                return Err("Duplicate OIB cell edit".into());
            }
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == *id)
                .ok_or("Unknown OIB property")?;
            super::super::oib::document::validate_edit(seed, *id, value.as_ref())?;
            if let Some(value) = value {
                super::super::oib::validate_value(cell, value)?;
            } else if cell.inherit.is_none() {
                return Err("Only instance overrides can be cleared".into());
            }
            edits.push(Change {
                address: super::super::oib::document::address_for_cell(seed, *id)?,
                value: value.clone(),
                observed: self
                    .doc
                    .tree
                    .subtree(&address::cell_path(seed, 1, *id)?)
                    .map_err(err)?
                    .to_bytes()
                    .map_err(err)?,
            });
        }
        // This is an intent, not an optimistic UI activation. Its observed
        // registers come directly from the admitted replica; the authority
        // compiles/validates the draft and journals the active result atomically.
        // Repeating that work here only blocks delivery of the button release.
        self.flight = Some(Flight {
            oib_epoch: Some(self.doc.oib_epoch()?),
            sequence: self
                .acknowledged
                .checked_add(1)
                .ok_or("Intent sequence exhausted")?,
            values: vec![],
            oib: edits,
            oib_instance: None,
            sent: false,
            issued_at: self.clock,
        });
        Ok(true)
    }
    pub fn oib_address_action(
        &mut self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
        action: InspectorAction,
    ) -> Result<bool> {
        if let otrios_interfaces::oib::designer::PropertyAddress::Occurrence { instance, .. } =
            address
            && self.doc.oib_retired_instance(instance.0)?
        {
            return Ok(false);
        }
        let seed = self.doc.oib_resolved_seed()?;
        let cell = super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        self.oib_inspector_action(cell, action)
    }
    pub fn oib_inspector_action(&mut self, id: u64, action: InspectorAction) -> Result<bool> {
        let resolved = self.doc.oib_resolved_seed()?;
        let seed = &resolved;
        let cell = seed
            .cells
            .iter()
            .find(|c| c.id == id)
            .ok_or("Unknown OIB cell")?;
        let value = match action {
            InspectorAction::Clear => None,
            InspectorAction::Resolve => Some(
                cell.authored_override
                    .as_ref()
                    .unwrap_or(&cell.default)
                    .clone(),
            ),
            InspectorAction::Cycle => {
                let active = self.doc.oib_values(2)?;
                let current = super::super::oib::effective(seed, &active, id)?;
                if cell.choices.is_empty() {
                    return Err("No inspector choices declared for this property".into());
                }
                let index = cell
                    .choices
                    .iter()
                    .position(|v| *v == current)
                    .map_or(0, |i| (i + 1) % cell.choices.len());
                Some(cell.choices[index].clone())
            }
        };
        self.edit_oib(&[(id, value)])
    }
}

fn property_text(
    seed: &otrios_interfaces::oib::Seed,
    cell: &otrios_interfaces::oib::Cell,
    status: &Status,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<String> {
    if status
        .conflicts
        .contains(&super::super::oib::document::address_for_cell(
            seed, cell.id,
        )?)
    {
        return Ok(format!("{}: conflict", cell.name));
    }
    let value = match super::super::oib::effective(seed, values, cell.id)? {
        Value::Text(v) => v,
        Value::Number(v) => format!("{v:.0}"),
        Value::Color(c) => format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]),
        Value::Boolean(v) => v.to_string(),
        _ => String::new(),
    };
    let kind = if cell.inherit.is_none() {
        "default"
    } else if values[&cell.id].is_some() {
        "override"
    } else if cell.authored_override.is_some() {
        "authored"
    } else {
        "inherited"
    };
    Ok(format!("{}: {} ({})", cell.name, value, kind))
}

fn same_source(old: &Binding, new: &Binding) -> bool {
    old.ui_ref == new.ui_ref && old.seed == new.seed && old.admission == new.admission
}
fn admitted_instance_name(seed: &otrios_interfaces::oib::Seed, name: &str) -> bool {
    if seed.designer.is_some() {
        !name.is_empty() && name.len() <= 64 && !name.contains('\0')
    } else {
        instance_name(name)
    }
}
fn instance_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn trie_ids(tree: &Snapshot, depth: usize, id: u64, seen: &mut BTreeSet<u64>) -> Result<()> {
    if depth == 13 {
        if tree.register().is_none() || tree.child_keys().next().is_some() || !seen.insert(id) {
            return Err("Invalid instance leaf".into());
        }
    } else {
        if tree.register().is_some() {
            return Err("Invalid instance trie".into());
        }
        for key in tree.child_keys() {
            if key > 31 || (depth == 0 && key > 15) {
                return Err("Invalid instance ID digit".into());
            }
            trie_ids(
                &tree.subtree(&[key]).map_err(err)?,
                depth + 1,
                (id << 5) | key,
                seen,
            )?;
        }
    }
    Ok(())
}
impl Document {
    fn validate_allocations(&self, old: Option<&Snapshot>) -> Result<()> {
        let seed = self.oib_binding()?.seed;
        if seed.instance_surface.is_none() {
            let source = self.oib_source()?;
            let initial: Vec<otrios_interfaces::oib::authoring::InstanceRecord> =
                super::super::oib::instances::initial_allocations(&seed)
                    .iter()
                    .map(Into::into)
                    .collect();
            if source.instances != initial
                || source.next_instance != lifecycle::first_id(&seed)?
                || !source.placements.is_empty()
                || !source.connections.is_empty()
            {
                return Err("Invalid fixed OIB document source".into());
            }
            return Ok(());
        }
        self.validate_resident_allocations(old)
    }
    pub(super) fn oib_instance_write(
        &mut self,
        writer: Actor,
        operation: &InstanceOperation,
    ) -> Result<()> {
        let placements = self.oib_placements()?;
        let connections = self.oib_connections()?;
        if let InstanceOperation::Remove { instance, .. } | InstanceOperation::Duplicate { instance, .. } = operation
            && (placements.iter().any(|p| p.instance.0 == *instance
                || matches!(&p.destination, otrios_interfaces::oib::authoring::Destination::Slot { component, .. } if component.instance.0 == *instance))
                || connections.iter().any(|c| c.object.instance.0 == *instance)) {
            return self.oib_reject_structure("Placed components require a hierarchy delete or owned subtree duplicate");
        }
        if let InstanceOperation::Restore { defaults } = operation {
            let mut candidate = self.clone();
            let signed_definitions = self
                .oib_binding()?
                .seed
                .designer
                .map_or_else(Vec::new, |d| d.definitions);
            let edit = candidate.oib_source_edit(|source| {
                source.placements.clear();
                source.connections.clear();
                if *defaults {
                    source.definitions = signed_definitions;
                }
            })?;
            candidate.change(&[], vec![edit])?;
            candidate.oib_instance_apply(writer, operation)?;
            if let Some(error) = candidate.oib_status()?.error {
                return self.oib_reject_structure(&error);
            }
            self.tree = candidate.tree;
            return Ok(());
        }
        if matches!(
            operation,
            InstanceOperation::Insert { .. }
                | InstanceOperation::Reorder { .. }
                | InstanceOperation::Definitions(_)
                | InstanceOperation::Hierarchy(_)
        ) {
            return self.oib_structure_write(writer, operation);
        }
        self.oib_instance_apply(writer, operation)
    }
    fn oib_instance_apply(&mut self, writer: Actor, operation: &InstanceOperation) -> Result<()> {
        self.oib_instance_stage(writer, operation, true)
    }
    fn oib_instance_stage(
        &mut self,
        writer: Actor,
        operation: &InstanceOperation,
        activate: bool,
    ) -> Result<()> {
        let ui = self.oib_ui()?.clone();
        let seed = super::super::oib::seed(&ui).ok_or("Missing OIB seed")?;
        if seed.instance_surface.is_none() {
            return Err("OIB instance factories not admitted".into());
        }
        let mut candidate = self.clone();
        let mut allocations = self.oib_allocations()?;
        match operation {
            InstanceOperation::Definitions(_) | InstanceOperation::Hierarchy(_) => {
                return Err("Definition edit requires atomic structural admission".into());
            }
            InstanceOperation::Reorder {
                instance, before, ..
            } => {
                let index = allocations
                    .iter()
                    .position(|a| a.id == *instance)
                    .ok_or("Unknown canvas child")?;
                let moved = allocations.remove(index);
                let index = before
                    .map(|id| {
                        allocations
                            .iter()
                            .position(|a| a.id == id)
                            .ok_or("Unknown destination sibling")
                    })
                    .transpose()?
                    .unwrap_or(allocations.len());
                allocations.insert(index, moved);
                if allocations == self.oib_allocations()? {
                    return Ok(());
                }
                candidate.change(
                    &[],
                    vec![candidate.oib_source_edit(|source| {
                        source.instances = allocations.iter().map(Into::into).collect()
                    })?],
                )?;
            }
            InstanceOperation::Restore { defaults } => {
                candidate.oib_restore_residents(*defaults, writer)?;
                if candidate.oib_compiled(1).is_err() {
                    // A restore replaces the roster atomically. Its disposable
                    // candidate may free old slots, but an invalid target must
                    // not reclaim objects still used by the published graph.
                    return self.activate_oib(self.tree.clone());
                }
            }
            InstanceOperation::Remove { instance, observed } => {
                if self.oib_retired_instance(*instance)? {
                    return Ok(());
                }
                if !allocations.iter().any(|a| a.id == *instance) {
                    return Err("Unknown instance".into());
                }
                let observed = Snapshot::from_bytes(observed).map_err(err)?;
                candidate.tree = self
                    .tree
                    .edit_observed(
                        writer,
                        &Edit {
                            path: cell_path(5, *instance),
                            schema: self.oib_instance_schema(*instance),
                            value: None,
                        },
                        &observed,
                    )
                    .map_err(err)?;
            }
            InstanceOperation::Instantiate { .. }
            | InstanceOperation::Duplicate { .. }
            | InstanceOperation::Insert { .. } => {
                if allocations.len() >= MAX_ALLOCATIONS {
                    return Err("OIB retained identity capacity exhausted".into());
                }
                let (component, prototype, source, name) = match operation {
                    InstanceOperation::Insert { prototype, .. } => {
                        let source = seed
                            .instances
                            .iter()
                            .find(|i| i.id == *prototype)
                            .ok_or("Undeclared signed factory")?;
                        (
                            source.component.ok_or("Missing factory component")?,
                            *prototype,
                            None,
                            &None,
                        )
                    }
                    InstanceOperation::Instantiate { component, name } => {
                        let prototype = seed
                            .instances
                            .iter()
                            .find(|i| i.component == Some(*component))
                            .ok_or("Undeclared component factory")?
                            .id;
                        (*component, prototype, None, name)
                    }
                    InstanceOperation::Duplicate { instance, name } => {
                        if self.oib_retired_instance(*instance)? {
                            return Ok(());
                        }
                        let source = allocations
                            .iter()
                            .find(|a| a.id == *instance)
                            .ok_or("Unknown source instance")?;
                        if !self.oib_live(2)?.contains(instance) {
                            return Err("Source instance is not active".into());
                        }
                        (source.component, source.prototype, Some(*instance), name)
                    }
                    _ => unreachable!(),
                };
                let new = candidate.oib_allocate(&allocations, prototype, name.clone())?;
                debug_assert_eq!(new.component, component);
                let id = new.id;
                let index = if let InstanceOperation::Insert {
                    before: Some(id), ..
                } = operation
                {
                    allocations
                        .iter()
                        .position(|a| a.id == *id)
                        .ok_or("Unknown destination sibling")?
                } else {
                    allocations.len()
                };
                allocations.insert(index, new);
                let expanded = super::super::oib::instances::expanded_seed(seed, &allocations)?;
                let values = self.oib_values(2)?;
                let mut edits = vec![candidate.oib_source_edit(|source| {
                    source.instances = allocations.iter().map(Into::into).collect()
                })?];
                edits.push(Edit {
                    path: cell_path(5, id),
                    schema: self.oib_instance_schema(id),
                    value: Some(bytes(&true)?),
                });
                for cell in expanded.cells.iter().filter(|c| c.owner == id) {
                    let value = if let Some(source) = source {
                        let source_cell = if seed.designer.is_some() {
                            let doc = expanded
                                .designer
                                .as_ref()
                                .ok_or("Missing expanded D1 mappings")?;
                            let row = doc
                                .mappings
                                .roots
                                .iter()
                                .find(|t| t.prototype.0 == id)
                                .and_then(|t| t.cells.iter().find(|r| r.initial_cell == cell.id))
                                .ok_or("Missing copied D1 relative key")?;
                            super::super::oib::document::cell_for_address(
                                seed,
                                &allocations,
                                &otrios_interfaces::oib::designer::PropertyAddress::Occurrence {
                                    instance: otrios_interfaces::oib::designer::InstanceId(source),
                                    path: row.path.clone(),
                                },
                            )?
                        } else {
                            super::super::oib::instances::cell_id(
                                seed,
                                &allocations,
                                source,
                                cell.inherit.ok_or("Missing inherited property")?,
                            )?
                        };
                        values[&source_cell].as_ref().map(bytes).transpose()?
                    } else {
                        None
                    };
                    edits.push(Edit {
                        path: address::cell_path(&expanded, 1, cell.id)?,
                        schema: address::schema(&self.scope, &expanded, cell.id)?,
                        value,
                    });
                }

                candidate.change(&[], edits)?;
            }
        }
        if activate {
            let next = candidate.tree.clone();
            candidate.activate_oib(next)?;
        }
        self.tree = candidate.tree;
        Ok(())
    }
}
impl StateAdapter {
    /// Canonical signed factory metadata, before resident expansion removes
    /// palette templates or replaces prototype IDs with allocated identities.
    pub(in crate::canvas) fn oib_authoring_seed(&self) -> Result<otrios_interfaces::oib::Seed> {
        self.doc.oib_authored_seed()
    }
    pub fn restore_oib(&mut self, defaults: bool) -> Result<bool> {
        self.edit_oib_instance(InstanceOperation::Restore { defaults })
    }
    pub fn edit_oib_instance_property(
        &mut self,
        instance: u64,
        property: &str,
        value: Option<Value>,
    ) -> Result<bool> {
        let seed = self.doc.oib_resolved_seed()?;
        let cell = seed
            .cells
            .iter()
            .find(|c| c.owner == instance && c.inherit.is_some() && c.name == property)
            .ok_or("Unknown declared instance property")?
            .id;
        self.edit_oib(&[(cell, value)])
    }
    pub fn instantiate_oib(&mut self, component: u64, name: Option<String>) -> Result<bool> {
        self.edit_oib_instance(InstanceOperation::Instantiate { component, name })
    }
    /// Original signed factories, including their distinct authored bindings.
    /// An empty list means this document does not admit single-canvas authoring.
    pub fn oib_canvas_factories(&self) -> Result<Vec<(u64, String)>> {
        let seed = self.doc.oib_binding()?.seed;
        let Some(first) = seed.instances.first() else {
            return Ok(vec![]);
        };
        let operation = InstanceOperation::Insert {
            prototype: first.id,
            before: None,
            generation: self.doc.oib_status()?.generation,
        };
        if self.doc.oib_validate_structure(&operation).is_err() {
            return Ok(vec![]);
        }
        Ok(seed
            .instances
            .iter()
            .map(|i| (i.id, i.name.clone()))
            .collect())
    }
    /// Submit an explicitly captured generation; never silently rebase a move.
    pub fn edit_oib_structure(&mut self, operation: InstanceOperation) -> Result<bool> {
        if !matches!(
            operation,
            InstanceOperation::Insert { .. }
                | InstanceOperation::Reorder { .. }
                | InstanceOperation::Definitions(_)
                | InstanceOperation::Hierarchy(_)
        ) {
            return Err("Expected a canvas structure operation".into());
        }
        self.edit_oib_instance(operation)
    }
    pub fn insert_oib(&mut self, prototype: u64, before: Option<u64>) -> Result<bool> {
        self.edit_oib_instance(InstanceOperation::Insert {
            prototype,
            before,
            generation: self.doc.oib_status()?.generation,
        })
    }
    pub fn reorder_oib(&mut self, instance: u64, before: Option<u64>) -> Result<bool> {
        if before == Some(instance) {
            return Ok(false);
        }
        self.edit_oib_instance(InstanceOperation::Reorder {
            instance,
            before,
            generation: self.doc.oib_status()?.generation,
        })
    }
    pub fn duplicate_oib(&mut self, instance: u64, name: Option<String>) -> Result<bool> {
        if !self.doc.oib_placements()?.is_empty() || !self.doc.oib_connections()?.is_empty() {
            let placements = self.doc.oib_placements()?;
            let destination = super::super::oib::document::placement::destination(
                &placements,
                otrios_interfaces::oib::designer::InstanceId(instance),
            );
            let frame = placements
                .iter()
                .find(|p| p.instance.0 == instance)
                .and_then(|p| p.frame);
            return self.edit_oib_structure(InstanceOperation::Hierarchy(
                otrios_interfaces::oib::authoring::HierarchyBatch {
                    generation: self.doc.oib_status()?.generation,
                    operations: vec![
                        otrios_interfaces::oib::authoring::HierarchyEdit::Duplicate {
                            instance: otrios_interfaces::oib::designer::InstanceId(instance),
                            name,
                            destination,
                            before: None,
                            frame,
                        },
                    ],
                },
            ));
        }
        self.edit_oib_instance(InstanceOperation::Duplicate { instance, name })
    }
    pub fn remove_oib(&mut self, instance: u64) -> Result<bool> {
        if !self.doc.oib_placements()?.is_empty() || !self.doc.oib_connections()?.is_empty() {
            return self.edit_oib_structure(InstanceOperation::Hierarchy(
                otrios_interfaces::oib::authoring::HierarchyBatch {
                    generation: self.doc.oib_status()?.generation,
                    operations: vec![otrios_interfaces::oib::authoring::HierarchyEdit::Remove {
                        instance: otrios_interfaces::oib::designer::InstanceId(instance),
                    }],
                },
            ));
        }
        let observed = self
            .doc
            .tree
            .subtree(&cell_path(5, instance))
            .map_err(err)?
            .to_bytes()
            .map_err(err)?;
        self.edit_oib_instance(InstanceOperation::Remove { instance, observed })
    }
    fn edit_oib_instance(&mut self, operation: InstanceOperation) -> Result<bool> {
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        let seed = self.doc.oib_resolved_seed()?;
        if seed.instance_surface.is_none() {
            return Err("OIB instance surface unavailable".into());
        }
        match &operation {
            InstanceOperation::Hierarchy(batch) => {
                if batch.generation == 0
                    || batch.operations.is_empty()
                    || batch.operations.len() > otrios_interfaces::oib::authoring::MAX_OPERATIONS
                    || bytes(batch)?.len() > otrios_interfaces::oib::authoring::MAX_BATCH_BYTES
                {
                    return Err("Invalid hierarchy batch envelope".into());
                }
            }
            InstanceOperation::Definitions(batch) => {
                if batch.generation == self.doc.oib_status()?.generation {
                    super::super::oib::document::authoring::apply(
                        &*self.doc.oib_authored_ui()?,
                        batch,
                    )?;
                }
            }
            InstanceOperation::Insert { generation, .. }
            | InstanceOperation::Reorder { generation, .. } => {
                // Captured actions retain their observed generation even if a
                // remote publication reached this replica before release. Send
                // stale actions unchanged so the authority durably rejects and
                // acknowledges them instead of turning a normal race into a
                // local host error. Current-generation actions still preflight.
                if *generation == self.doc.oib_status()?.generation {
                    self.doc.oib_validate_structure(&operation)?;
                    if let InstanceOperation::Insert { prototype, .. } = &operation
                        && self.oib_available_slots_for(*prototype) == 0
                    {
                        return Ok(false);
                    }
                }
            }
            InstanceOperation::Restore { .. } => {}
            InstanceOperation::Instantiate { component, name } => {
                if !seed.components.iter().any(|c| c.id == *component) {
                    return Err("Unknown OIB component".into());
                }
                let prototype = self
                    .doc
                    .oib_binding()?
                    .seed
                    .instances
                    .iter()
                    .find(|i| i.component == Some(*component))
                    .ok_or("Undeclared component factory")?
                    .id;
                if self.oib_available_slots_for(prototype) == 0 {
                    return Ok(false);
                }
                if let Some(name) = name
                    && !admitted_instance_name(&seed, name)
                {
                    return Err("Invalid OIB instance name".into());
                }
            }
            InstanceOperation::Duplicate { instance, name } => {
                if !self.doc.oib_live(2)?.contains(instance) {
                    return Err("Source instance is not active".into());
                }
                let prototype = self
                    .doc
                    .oib_allocations()?
                    .iter()
                    .find(|a| a.id == *instance)
                    .ok_or("Unknown source instance")?
                    .prototype;
                if self.oib_available_slots_for(prototype) == 0 {
                    return Ok(false);
                }
                if let Some(name) = name
                    && !admitted_instance_name(&seed, name)
                {
                    return Err("Invalid OIB instance name".into());
                }
            }
            InstanceOperation::Remove { instance, .. } => {
                if !self.doc.oib_live(1)?.contains(instance) {
                    return Ok(false);
                }
            }
        }
        self.flight = Some(Flight {
            oib_epoch: Some(self.doc.oib_epoch()?),
            sequence: self
                .acknowledged
                .checked_add(1)
                .ok_or("Intent sequence exhausted")?,
            values: vec![],
            oib: vec![],
            oib_instance: Some(operation),
            sent: false,
            issued_at: self.clock,
        });
        Ok(true)
    }
    pub fn oib_instance_action(&mut self, target: u64, action: InstanceAction) -> Result<bool> {
        match action {
            InstanceAction::Restore => self.restore_oib(false),
            InstanceAction::ResetDefaults => self.restore_oib(true),
            InstanceAction::Inspect => Ok(false),
            InstanceAction::RemovePending => {
                let active = self.doc.oib_live(2)?;
                let draft = self.doc.oib_live(1)?;
                let pending = self
                    .doc
                    .oib_allocations()?
                    .into_iter()
                    .rev()
                    .find(|a| draft.contains(&a.id) && !active.contains(&a.id));
                if let Some(a) = pending {
                    self.remove_oib(a.id)
                } else {
                    Ok(false)
                }
            }
            InstanceAction::Add => self.instantiate_oib(target, None),
            InstanceAction::Duplicate => self.duplicate_oib(target, None),
            InstanceAction::Remove => self.remove_oib(target),
        }
    }
}

impl StateAdapter {
    /// Actual authority-owned resident descriptors, including reusable slots.
    pub(in crate::canvas) fn oib_allocations(&self) -> Result<Vec<Allocation>> {
        self.doc.oib_allocations()
    }
    pub fn edit_oib_property(
        &mut self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
        value: Option<Value>,
    ) -> Result<bool> {
        if let otrios_interfaces::oib::designer::PropertyAddress::Placement { object, property } =
            address
        {
            return self.edit_oib_placement_property(object, *property, value);
        }
        let seed = self.doc.oib_authored_seed()?;
        let id = super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        self.edit_oib(&[(id, value)])
    }
}

impl StateAdapter {
    pub fn oib_property_register(
        &self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
    ) -> Result<PropertyRegisterView> {
        let seed = self.doc.oib_binding()?.seed;
        let cell = super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        let leaf = self
            .doc
            .tree
            .subtree(&address::path(1, address)?)
            .map_err(err)?;
        let register = leaf.register().ok_or("Missing OIB property register")?;
        let mut alternatives = Vec::new();
        for (_, _, data) in register.values() {
            let value = data.map(decode::<Value>).transpose()?;
            if !alternatives.contains(&value) {
                alternatives.push(value);
            }
        }
        Ok(PropertyRegisterView {
            address: address.clone(),
            cell,
            version: leaf.hash(),
            alternatives,
        })
    }

    /// Enqueue a choice only from this exact observed draft version. A stale
    /// view must be refreshed; no unseen alternative is implicitly resolved.
    /// True means queued, not authority acceptance or active publication.
    pub fn choose_oib_property(
        &mut self,
        view: &PropertyRegisterView,
        value: Option<Value>,
    ) -> Result<bool> {
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        let current = self.oib_property_register(&view.address)?;
        if current.cell != view.cell || current.version != view.version {
            return Err("OIB property register changed; refresh choices".into());
        }
        if !current.alternatives.contains(&value) {
            return Err("Value is not an observed OIB property alternative".into());
        }
        // No replica mutation occurs between the version check and edit_oib's
        // capture of the observed register. Existing type/binding/write guards
        // and authority-side validation remain in force.
        self.edit_oib(&[(current.cell, value)])
    }
}
