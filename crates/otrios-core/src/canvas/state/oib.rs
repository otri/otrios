// SPDX-License-Identifier: LGPL-3.0-only
//! OIB shares the app checkpoint, journal, receipts and publication root.
use super::*;
use otrios_interfaces::oib::{Change, InspectorAction, MAX_CHANGES};
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
        if super::super::oib::seed(ui).is_none() {
            if self.tree.subtree(&[4]).is_ok() && self.oib_binding()?.ui_ref == ui.ui_ref {
                return Err("Installed app no longer supports retained OIB data".into());
            }
            return Ok(false);
        }
        super::super::oib::validate(ui)?;
        let encoded = bytes(&Binding {
            seed: super::super::oib::seed(ui)
                .ok_or("Missing OIB seed")?
                .clone(),
            ui_hash: hash(&bytes(ui)?),
            ui_ref: ui.ui_ref.clone(),
        })?;
        if let Ok(existing) = payload(&self.tree, &[4, 0]) {
            if existing != encoded {
                return Err("OIB seed update needs explicit compatible migration".into());
            }
            super::super::oib::compile(ui, &self.oib_values(2)?)?;
            self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
            return Ok(false);
        }
        self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
        let seed = super::super::oib::seed(ui).ok_or("Missing OIB seed")?;
        let mut edits = vec![
            Edit {
                path: vec![4, 0],
                schema: self.extra_schema(4, 0),
                value: Some(encoded),
            },
            Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&Status {
                    generation: 1,
                    active_root: [0; 32],
                    source_draft: [0; 32],
                    conflicts: vec![],
                    error: None,
                })?),
            },
        ];
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
        self.change(&[], edits)?;
        let mut status = self.oib_status()?;
        status.active_root = self.tree.subtree(&[4, 2]).map_err(err)?.hash();
        status.source_draft = self.tree.subtree(&[4, 1]).map_err(err)?.hash();
        self.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )
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
    pub fn oib_values(&self, branch: u64) -> Result<BTreeMap<u64, Option<Value>>> {
        // Admission already binds this immutable seed to the signed UI. Keep
        // decoding the persisted binding for validation/recovery, not every read.
        let binding;
        let seed = if let Some(ui) = &self.oib_ui {
            super::super::oib::seed(ui).ok_or("Missing OIB seed")?
        } else {
            binding = self.oib_binding()?;
            &binding.seed
        };
        let mut values = BTreeMap::new();
        for cell in &seed.cells {
            let leaf = self
                .tree
                .subtree(&cell_path(branch, cell.id))
                .map_err(err)?;
            let register = leaf.register().ok_or("Missing OIB register")?;
            let mut selected = None;
            let mut first = true;
            for (_, _, data) in register.values() {
                let value = data.map(decode::<Value>).transpose()?;
                if !first && selected != value {
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
        if old.is_some_and(|o| {
            o.subtree(&[4, 0])
                .is_ok_and(|p| tree.subtree(&[4, 0]).map_or(true, |n| n.hash() != p.hash()))
        }) {
            return Err("OIB immutable seed changed".into());
        }
        let mut candidate = self.clone();
        candidate.tree = tree.clone();
        let binding = candidate.oib_binding()?;
        super::super::oib::validate_seed(&binding.seed)?;
        if binding.ui_hash.len() != 32 || binding.ui_ref.is_empty() || binding.ui_ref.len() > 128 {
            return Err("Invalid OIB seed digest".into());
        }
        let seed = &binding.seed;
        let branch = tree.subtree(&[4]).map_err(err)?;
        if branch.register().is_some() || branch.child_keys().any(|k| k > 3) {
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
            if seen != seed.cells.iter().map(|c| c.id).collect() {
                return Err("OIB cell table changed".into());
            }
            if kind == 1 && branch.canonical_bytes() > otrios_interfaces::oib::MAX_BYTES {
                return Err("OIB draft byte budget".into());
            }
            for cell in &seed.cells {
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
                if status.active_root != branch.hash() {
                    return Err("OIB active closure mismatch".into());
                }
                let values = candidate.oib_values(2)?;
                if let Some(ui) = &self.oib_ui {
                    super::super::oib::compile(ui, &values)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn oib_write(&mut self, writer: Actor, changes: &[Change]) -> Result<()> {
        if changes.is_empty() || changes.len() > MAX_CHANGES {
            return Err("OIB change batch limit".into());
        }
        let ui = self.oib_ui()?;
        let seed = super::super::oib::seed(ui).ok_or("Missing OIB seed")?;
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
        let mut status = self.oib_status()?;
        status.conflicts.clear();
        status.error = None;
        let mut draft = self.clone();
        draft.tree = next;
        for cell in &seed.cells {
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
            match super::super::oib::compile(ui, &values) {
                Ok(_) => {
                    status.generation = status
                        .generation
                        .checked_add(1)
                        .ok_or("OIB generation exhausted")?;
                    status.source_draft = draft.tree.subtree(&[4, 1]).map_err(err)?.hash();
                    for cell in &seed.cells {
                        let value = values[&cell.id].as_ref().map(bytes).transpose()?;
                        let old = self.tree.subtree(&cell_path(2, cell.id)).map_err(err)?;
                        if old.register().is_some_and(|r| {
                            r.values()
                                .next()
                                .is_some_and(|(_, _, v)| v == value.as_deref())
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
        status.active_root = draft.tree.subtree(&[4, 2]).map_err(err)?.hash();
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
        self.flight.as_ref().is_some_and(|f| !f.oib.is_empty())
    }
    pub(in crate::canvas) fn oib_root(&self) -> Option<merkle::Hash> {
        self.doc.tree.subtree(&[4]).ok().map(|n| n.hash())
    }
    pub(in crate::canvas) fn oib_info(&self) -> Result<super::super::oib::Info> {
        let ui = self.doc.oib_ui()?;
        let seed = super::super::oib::seed(ui).ok_or("Missing OIB seed")?;
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
        })
    }
    pub fn oib_conflicts(&self) -> Vec<u64> {
        self.doc
            .oib_status()
            .map_or_else(|_| vec![], |s| s.conflicts)
    }
    pub fn oib_property_text(&self, id: u64) -> String {
        let result = (|| -> Result<String> {
            let ui = self.doc.oib_ui()?;
            let seed = super::super::oib::seed(ui).ok_or("Missing OIB seed")?;
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
    pub fn oib_status_text(&self) -> String {
        let Ok(status) = self.doc.oib_status() else {
            return "Live UI unavailable".into();
        };
        if !status.conflicts.is_empty() {
            format!(
                "{} conflicting properties — resolve in inspector",
                status.conflicts.len()
            )
        } else if status.error.is_some() {
            "Draft needs correction; active UI retained".into()
        } else if self.oib_saving() {
            "Saving UI change…".into()
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
        let ui = self.doc.oib_ui()?;
        let seed = super::super::oib::seed(ui).ok_or("Missing OIB seed")?;
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
            sent: false,
            issued_at: self.clock,
        });
        Ok(true)
    }
    pub fn oib_inspector_action(&mut self, id: u64, action: InspectorAction) -> Result<bool> {
        let ui = self.doc.oib_ui()?;
        let seed = super::super::oib::seed(ui).ok_or("Missing OIB seed")?;
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
