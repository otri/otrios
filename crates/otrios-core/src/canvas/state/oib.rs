// SPDX-License-Identifier: LGPL-3.0-only
//! OIB shares the app checkpoint, journal, receipts and publication root.
use super::*;
use otrios_interfaces::oib::{
    Allocation, Change, InspectorAction, InstanceAction, InstanceOperation, MAX_ALLOCATIONS,
    MAX_CHANGES,
};
use std::collections::BTreeSet;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub generation: u64,
    pub active_root: merkle::Hash,
    pub source_draft: merkle::Hash,
    pub conflicts: Vec<u64>,
    pub error: Option<String>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    seed: otrios_interfaces::oib::Seed,
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
            ui_hash: hash(&bytes(ui)?),
            ui_ref: ui.ui_ref.clone(),
        };
        let encoded = bytes(&binding)?;
        let existing = payload(&self.tree, &[4, 0]).ok();
        let mut initialize_instances = seed.instance_surface.is_some();
        if let Some(existing) = &existing {
            let previous: Binding = decode(existing)?;
            initialize_instances &= previous.seed.instance_surface.is_none();
            if *existing == encoded {
                self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
                self.oib_compiled(2)?;
                return Ok(false);
            }
            if !compatible_upgrade(&previous, &binding) {
                return Err("OIB seed update needs explicit compatible migration".into());
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
            for cell in &seed.cells {
                let value = if cell.inherit.is_some() {
                    None
                } else {
                    Some(bytes(&cell.default)?)
                };
                for branch in [1, 2] {
                    edits.push(Edit {
                        path: cell_path(branch, cell.id),
                        schema: self.oib_cell_schema(cell.id),
                        value: value.clone(),
                    });
                }
            }
        }
        if initialize_instances {
            let allocations = super::super::oib::instances::initial_allocations(seed);
            edits.push(Edit {
                path: vec![4, 4],
                schema: self.extra_schema(4, 4),
                value: Some(bytes(&allocations)?),
            });
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
    fn oib_cell_schema(&self, id: u64) -> merkle::Hash {
        digest(
            &[
                b"OTriOS\0oib-cell-v1\0".as_slice(),
                self.scope.as_bytes(),
                &id.to_le_bytes(),
            ]
            .concat(),
        )
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
        let seed = self.oib_binding()?.seed;
        if seed.instance_surface.is_none() {
            return Ok(super::super::oib::instances::initial_allocations(&seed));
        }
        decode(&payload(&self.tree, &[4, 4])?)
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
        let seed = if let Some(ui) = &self.oib_ui {
            super::super::oib::seed(ui)
                .ok_or("Missing OIB seed")?
                .clone()
        } else {
            self.oib_binding()?.seed
        };
        super::super::oib::instances::expanded_seed(&seed, &self.oib_allocations()?)
    }
    fn oib_active_root(&self) -> Result<merkle::Hash> {
        let properties = self.tree.subtree(&[4, 2]).map_err(err)?.hash();
        if self.tree.subtree(&[4, 6]).is_err() {
            return Ok(properties);
        }
        let instances = self.tree.subtree(&[4, 6]).map_err(err)?.hash();
        Ok(digest(
            &[
                b"OTriOS\0oib-active-v2\0".as_slice(),
                &properties,
                &instances,
            ]
            .concat(),
        ))
    }
    fn oib_draft_root(&self) -> Result<merkle::Hash> {
        let properties = self.tree.subtree(&[4, 1]).map_err(err)?.hash();
        let Ok(instances) = self.tree.subtree(&[4, 5]) else {
            return Ok(properties);
        };
        Ok(digest(
            &[
                b"OTriOS\0oib-draft-v2\0".as_slice(),
                &properties,
                &instances.hash(),
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
        let ui = self.oib_ui()?;
        let resolved = super::super::oib::instances::expand(
            ui,
            &self.oib_allocations()?,
            &self.oib_live(branch)?,
            selected,
        )?;
        let layout = super::super::oib::compile(&resolved, &self.oib_values(branch)?)?;
        Ok((resolved, layout))
    }
    pub fn oib_values(&self, branch: u64) -> Result<BTreeMap<u64, Option<Value>>> {
        let seed = self.oib_resolved_seed()?;
        let live = self.oib_live(branch)?;
        let mut values = BTreeMap::new();
        for cell in &seed.cells {
            let leaf = match self.tree.subtree(&cell_path(branch, cell.id)) {
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
            && !compatible_upgrade(&decode(&before)?, &decode(&payload(tree, &[4, 0])?)?)
        {
            return Err("OIB immutable seed changed".into());
        }
        let mut candidate = self.clone();
        candidate.tree = tree.clone();
        let binding = candidate.oib_binding()?;
        super::super::oib::validate_seed(&binding.seed)?;
        if binding.ui_hash.len() != 32 || binding.ui_ref.is_empty() || binding.ui_ref.len() > 128 {
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
                    3
                }
            })
        {
            return Err("Invalid OIB branch".into());
        }
        for index in [0, 3] {
            let leaf = tree.subtree(&[4, index]).map_err(err)?;
            if leaf.child_keys().next().is_some()
                || leaf
                    .register()
                    .is_none_or(|r| r.schema() != self.extra_schema(4, index as usize))
            {
                return Err("Invalid OIB metadata".into());
            }
        }
        let status = candidate.oib_status()?;
        if status.generation == 0
            || status.conflicts.len() > 64
            || status.error.as_ref().is_some_and(|s| s.len() > 256)
            || status
                .conflicts
                .iter()
                .any(|id| !seed.cells.iter().any(|c| c.id == *id))
        {
            return Err("Invalid OIB status".into());
        }
        for kind in [1, 2] {
            let Ok(branch) = tree.subtree(&[4, kind]) else {
                continue;
            };
            fn walk(
                tree: &Snapshot,
                depth: usize,
                id: u64,
                seen: &mut BTreeSet<u64>,
            ) -> Result<()> {
                if depth == 13 {
                    if tree.child_keys().next().is_some()
                        || tree.register().is_none()
                        || !seen.insert(id)
                    {
                        return Err("Invalid OIB cell leaf".into());
                    }
                } else {
                    if tree.register().is_some() {
                        return Err("Invalid OIB cell trie".into());
                    }
                    for key in tree.child_keys() {
                        if key > 31 || (depth == 0 && key > 15) {
                            return Err("Invalid OIB ID digit".into());
                        }
                        walk(
                            &tree.subtree(&[key]).map_err(err)?,
                            depth + 1,
                            (id << 5) | key,
                            seen,
                        )?;
                    }
                }
                Ok(())
            }
            let mut seen = BTreeSet::new();
            walk(&branch, 0, 0, &mut seen)?;
            let expected = seed
                .cells
                .iter()
                .filter(|c| {
                    kind == 1
                        || seed.instance_surface.is_none()
                        || c.inherit.is_none()
                        || tree.subtree(&cell_path(6, c.owner)).is_ok()
                })
                .map(|c| c.id)
                .collect();
            if seen != expected {
                return Err("OIB cell table changed".into());
            }
            if kind == 1 && branch.canonical_bytes() > otrios_interfaces::oib::MAX_BYTES {
                return Err("OIB draft byte budget".into());
            }
            for cell in &seed.cells {
                if kind == 2 && !seen.contains(&cell.id) {
                    continue;
                }
                let leaf = tree.subtree(&cell_path(kind, cell.id)).map_err(err)?;
                let r = leaf.register().ok_or("Missing OIB cell")?;
                if r.schema() != self.oib_cell_schema(cell.id) {
                    return Err("Wrong OIB cell schema".into());
                }
                for (_, _, value) in r.values() {
                    if let Some(value) = value {
                        super::super::oib::validate_value(cell, &decode(value)?)?;
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
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == change.cell)
                .ok_or("Unknown OIB cell")?;
            if !seen.insert(cell.id) {
                return Err("Duplicate OIB cell edit".into());
            }
            if let Some(value) = &change.value {
                super::super::oib::validate_value(cell, value)?;
            } else if cell.inherit.is_none() {
                return Err("Only instance overrides can be cleared".into());
            }
            let observed = Snapshot::from_bytes(&change.observed).map_err(err)?;
            let edit = Edit {
                path: cell_path(1, cell.id),
                schema: self.oib_cell_schema(cell.id),
                value: change.value.as_ref().map(bytes).transpose()?,
            };
            next = next.edit_observed(writer, &edit, &observed).map_err(err)?;
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
            let leaf = draft.tree.subtree(&cell_path(1, cell.id)).map_err(err)?;
            let alternatives: BTreeSet<_> = leaf
                .register()
                .ok_or("Missing OIB cell")?
                .values()
                .map(|(_, _, v)| v)
                .collect();
            if alternatives.len() > 1 {
                status.conflicts.push(cell.id);
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
                        if self.tree.subtree(&cell_path(2, cell.id)).is_ok_and(|old| {
                            old.register().is_some_and(|r| {
                                r.values()
                                    .next()
                                    .is_some_and(|(_, _, v)| v == value.as_deref())
                            })
                        }) {
                            continue;
                        }
                        edits.push(Edit {
                            path: cell_path(2, cell.id),
                            schema: self.oib_cell_schema(cell.id),
                            value,
                        });
                    }
                }
                Err(e) => status.error = Some(e.chars().take(256).collect()),
            }
        }
        draft.change(&[], edits)?;
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
            conflicts: status.conflicts.clone(),
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
            allocations_remaining: MAX_ALLOCATIONS
                .saturating_sub(self.doc.oib_allocations()?.len()),
        })
    }
    pub(in crate::canvas) fn admitted_oib_hash(&self) -> Option<Vec<u8>> {
        self.doc.oib_binding().ok().map(|b| b.ui_hash)
    }
    pub fn oib_instances(&self) -> Result<Vec<super::super::oib::InstanceInfo>> {
        Ok(self.oib_info()?.instances)
    }
    pub fn oib_conflicts(&self) -> Vec<u64> {
        self.doc
            .oib_status()
            .map_or_else(|_| vec![], |s| s.conflicts)
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
                "Live · revision {} · {} instances · {} IDs left",
                status.generation,
                self.doc.oib_live(2).map_or(0, |v| v.len()),
                MAX_ALLOCATIONS.saturating_sub(self.doc.oib_allocations().map_or(0, |v| v.len()))
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
            if let Some(value) = value {
                super::super::oib::validate_value(cell, value)?;
            } else if cell.inherit.is_none() {
                return Err("Only instance overrides can be cleared".into());
            }
            edits.push(Change {
                cell: *id,
                value: value.clone(),
                observed: self
                    .doc
                    .tree
                    .subtree(&cell_path(1, *id))
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
            InspectorAction::Resolve => Some(cell.default.clone()),
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
    if status.conflicts.contains(&cell.id) {
        return Ok(format!("{}: conflict", cell.name));
    }
    let value = match super::super::oib::effective(seed, values, cell.id)? {
        Value::Text(v) => v,
        Value::Number(v) => format!("{v:.0}"),
        Value::Color(c) => format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]),
        _ => String::new(),
    };
    let kind = if cell.inherit.is_none() {
        "default"
    } else if values[&cell.id].is_some() {
        "override"
    } else {
        "inherited"
    };
    Ok(format!("{}: {} ({})", cell.name, value, kind))
}

fn compatible_upgrade(old: &Binding, new: &Binding) -> bool {
    // A signed presentation update can retain the exact OIB contract. Keep all
    // allocated identities, tombstones and overrides; this is not seed rebasing.
    if old.ui_ref == new.ui_ref
        && bytes(&old.seed)
            .ok()
            .zip(bytes(&new.seed).ok())
            .is_some_and(|(old, new)| old == new)
    {
        return true;
    }
    let Some(surface) = &new.seed.instance_surface else {
        return false;
    };
    if old.seed.instance_surface.is_some()
        || surface.previous_ui_sha256 != old.ui_hash
        || old.ui_ref != new.ui_ref
        || old.seed.main_oib != new.seed.main_oib
        || old.seed.main_class != new.seed.main_class
        || old.seed.components != new.seed.components
        || old.seed.instances != new.seed.instances
        || old.seed.cells.len() != new.seed.cells.len()
    {
        return false;
    }
    old.seed.cells.iter().zip(&new.seed.cells).all(|(a, b)| {
        a.id == b.id
            && a.owner == b.owner
            && a.name == b.name
            && a.default == b.default
            && a.choices == b.choices
            && a.inherit == b.inherit
    })
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
            return Ok(());
        }
        let allocations = self.oib_allocations()?;
        let initial = super::super::oib::instances::initial_allocations(&seed);
        let meta = self.tree.subtree(&[4, 4]).map_err(err)?;
        if meta
            .register()
            .is_none_or(|r| r.schema() != self.extra_schema(4, 4))
            || meta.child_keys().next().is_some()
            || allocations.len() > MAX_ALLOCATIONS
            || !allocations.starts_with(&initial)
        {
            return Err("Invalid OIB allocation ledger".into());
        }
        if let Some(previous) = old.and_then(|t| payload(t, &[4, 4]).ok()) {
            let previous: Vec<Allocation> = decode(&previous)?;
            if !allocations.starts_with(&previous) {
                return Err("OIB identities cannot be changed or reused".into());
            }
        }
        for (index, a) in allocations.iter().enumerate() {
            if !instance_name(&a.name)
                || (index >= initial.len() && a.id != 0x1_0000 + index as u64)
                || !seed
                    .instances
                    .iter()
                    .any(|i| i.id == a.prototype && i.component == Some(a.component))
            {
                return Err("Invalid OIB allocated identity or binding".into());
            }
        }
        if let Ok(properties) = self.tree.subtree(&[4, 1]) {
            let instances = self.tree.subtree(&[4, 5]).map_err(err)?;
            if properties.canonical_bytes() + instances.canonical_bytes() + meta.canonical_bytes()
                > otrios_interfaces::oib::MAX_BYTES
            {
                return Err("OIB combined draft byte budget".into());
            }
        }
        for kind in [5, 6] {
            let Ok(branch) = self.tree.subtree(&[4, kind]) else {
                continue;
            };
            let mut seen = BTreeSet::new();
            trie_ids(&branch, 0, 0, &mut seen)?;
            let allowed = allocations.iter().map(|a| a.id).collect();
            let required = initial.iter().map(|a| a.id).collect::<BTreeSet<_>>();
            if (kind == 5 && seen != allowed)
                || !seen.is_subset(&allowed)
                || !required.is_subset(&seen)
            {
                return Err("OIB presence table mismatch".into());
            }
            for a in &allocations {
                if !seen.contains(&a.id) {
                    continue;
                }
                let leaf = self.tree.subtree(&cell_path(kind, a.id)).map_err(err)?;
                let r = leaf.register().ok_or("Missing instance register")?;
                if r.schema() != self.oib_instance_schema(a.id) {
                    return Err("Wrong instance register schema".into());
                }
                for (_, _, v) in r.values() {
                    if let Some(v) = v
                        && !decode::<bool>(v)?
                    {
                        return Err("Invalid instance presence".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn oib_instance_write(
        &mut self,
        writer: Actor,
        operation: &InstanceOperation,
    ) -> Result<()> {
        let ui = self.oib_ui()?.clone();
        let seed = super::super::oib::seed(&ui).ok_or("Missing OIB seed")?;
        if seed.instance_surface.is_none() {
            return Err("OIB instance factories not admitted".into());
        }
        let mut candidate = self.clone();
        let mut allocations = self.oib_allocations()?;
        match operation {
            InstanceOperation::Remove { instance, observed } => {
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
            InstanceOperation::Instantiate { .. } | InstanceOperation::Duplicate { .. } => {
                if allocations.len() >= MAX_ALLOCATIONS {
                    return Err("OIB retained identity capacity exhausted".into());
                }
                let (component, prototype, source, name) = match operation {
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
                let index = allocations.len();
                let id = 0x1_0000 + index as u64;
                let name = name
                    .clone()
                    .unwrap_or_else(|| format!("instance-{}", index + 1));
                if !instance_name(&name) {
                    return Err(
                        "Instance names use 1–64 ASCII letters, digits, hyphens or underscores"
                            .into(),
                    );
                }
                let new = Allocation {
                    id,
                    name,
                    component,
                    prototype,
                };
                allocations.push(new);
                let expanded = super::super::oib::instances::expanded_seed(seed, &allocations)?;
                let values = self.oib_values(2)?;
                let mut edits = vec![Edit {
                    path: vec![4, 4],
                    schema: self.extra_schema(4, 4),
                    value: Some(bytes(&allocations)?),
                }];
                edits.push(Edit {
                    path: cell_path(5, id),
                    schema: self.oib_instance_schema(id),
                    value: Some(bytes(&true)?),
                });
                for cell in expanded.cells.iter().filter(|c| c.owner == id) {
                    let value = if let Some(source) = source {
                        let source_cell = super::super::oib::instances::cell_id(
                            seed,
                            &allocations,
                            source,
                            cell.inherit.ok_or("Missing inherited property")?,
                        )?;
                        values[&source_cell].as_ref().map(bytes).transpose()?
                    } else {
                        None
                    };
                    edits.push(Edit {
                        path: cell_path(1, cell.id),
                        schema: self.oib_cell_schema(cell.id),
                        value,
                    });
                }

                candidate.change(&[], edits)?;
            }
        }
        let next = candidate.tree.clone();
        candidate.activate_oib(next)?;
        self.tree = candidate.tree;
        Ok(())
    }
}
impl StateAdapter {
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
    pub fn duplicate_oib(&mut self, instance: u64, name: Option<String>) -> Result<bool> {
        self.edit_oib_instance(InstanceOperation::Duplicate { instance, name })
    }
    pub fn remove_oib(&mut self, instance: u64) -> Result<bool> {
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
        let allocations = self.doc.oib_allocations()?;
        match &operation {
            InstanceOperation::Instantiate { component, name } => {
                if !seed.components.iter().any(|c| c.id == *component) {
                    return Err("Unknown OIB component".into());
                }
                if allocations.len() >= MAX_ALLOCATIONS
                    || self.doc.oib_live(1)?.len() >= otrios_interfaces::oib::MAX_LIVE_INSTANCES
                {
                    return Ok(false);
                }
                if let Some(name) = name
                    && !instance_name(name)
                {
                    return Err("Invalid OIB instance name".into());
                }
            }
            InstanceOperation::Duplicate { instance, name } => {
                if !self.doc.oib_live(2)?.contains(instance) {
                    return Err("Source instance is not active".into());
                }
                if allocations.len() >= MAX_ALLOCATIONS
                    || self.doc.oib_live(1)?.len() >= otrios_interfaces::oib::MAX_LIVE_INSTANCES
                {
                    return Ok(false);
                }
                if let Some(name) = name
                    && !instance_name(name)
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
