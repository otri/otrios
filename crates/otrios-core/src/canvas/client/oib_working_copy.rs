// SPDX-License-Identifier: LGPL-3.0-only
//! A bounded editor command log over a pinned source. Never a transport owner.
use super::*;
use otrios_interfaces::oib::{
    Change,
    authoring::{
        Apply, ApplyResult, DefinitionEdit, DocumentEdit, EditBatch, MAX_BATCH_BYTES,
        MAX_OPERATIONS, PresetEdit,
    },
    designer::PropertyAddress,
};
const HISTORY_BYTES: usize = 64 * 1024;

pub(super) struct WorkingCopy {
    pub(super) base: StateAdapter,
    pub(super) candidate: StateAdapter,
    pub(super) apply: Apply,
    undo: Vec<Apply>,
    redo: Vec<Apply>,
    pub(super) error: Option<String>,
    pub(super) submitted: Option<Vec<u8>>,
    pub(super) revision: u64,
    base_ui_generation: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn working_copy_choice_checks_pinned_register_and_keeps_its_observed_context() {
        use crate::canvas::{StateSelection, state::Document};
        let ui = crate::test_support::oib_designer::ui(&[]);
        let mut authority = Document::new(ui.state_schema.clone(), "oib:working-choice").unwrap();
        authority.configure_oib(&ui).unwrap();
        let (mut session, _) = authority.open_session(&[81; 16], 0).unwrap();
        session.set_oib_editable(true);
        session
            .set_selection(StateSelection::from_ui(&ui).unwrap())
            .unwrap();
        let mut live = StateAdapter::for_ui(&ui, "oib:working-choice", &[], [81; 16]).unwrap();
        for _ in 0..30 {
            while let Some(frame) = authority.generate(&mut session).unwrap() {
                live.receive(&frame).unwrap();
            }
            while let Some(frame) = live.generate().unwrap() {
                authority.receive(&mut session, &frame).unwrap();
            }
            if live.confirmed() {
                break;
            }
        }
        assert!(live.confirmed());
        let mut copy = WorkingCopy::new(&live).unwrap();
        let seed = live.oib_authoring_seed().unwrap();
        let cell = seed
            .cells
            .iter()
            .find(|cell| cell.owner == 20 && matches!(cell.default, Value::Color(_)))
            .unwrap();
        let address = crate::canvas::oib::document::address_for_cell(&seed, cell.id).unwrap();
        let view = copy.base.oib_property_register(&address).unwrap();
        let value = view.alternatives[0].clone();
        let observed = copy
            .base
            .oib_capture_change(&address, value.clone())
            .unwrap()
            .observed;
        let mut stale = view.clone();
        stale.version[0] ^= 1;
        assert!(
            copy.stage_choice(&stale, value.clone())
                .unwrap_err()
                .contains("refresh choices")
        );
        let mut wrong_cell = view.clone();
        wrong_cell.cell += 1;
        assert!(copy.stage_choice(&wrong_cell, value.clone()).is_err());
        assert!(
            copy.stage_choice(&view, Some(Value::Text("unobserved".into())))
                .unwrap_err()
                .contains("not an observed")
        );
        assert!(!copy.dirty());
        assert!(copy.stage_choice(&view, value).unwrap());
        assert_eq!(copy.apply.changes[0].observed, observed);
    }
}
impl WorkingCopy {
    pub(super) fn new(live: &StateAdapter) -> Result<Self> {
        let apply = live.oib_apply_base()?;
        let base = live.preview_clone();
        let candidate = base.preview_oib_apply(&apply)?;
        Ok(Self {
            base,
            candidate,
            apply,
            undo: vec![],
            redo: vec![],
            error: None,
            submitted: None,
            revision: 0,
            base_ui_generation: live.oib_generation(),
        })
    }
    pub(super) fn dirty(&self) -> bool {
        !self.apply.operations.is_empty() || !self.apply.changes.is_empty()
    }
    pub(super) fn can_undo(&self) -> bool {
        self.error.is_some() || !self.undo.is_empty()
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub(super) fn applying(&self) -> bool {
        self.submitted.is_some()
    }
    pub(super) fn stale(&self, live: &StateAdapter) -> bool {
        live.oib_apply_base().is_ok_and(|base| {
            base.epoch != self.apply.epoch
                || base.baseline != self.apply.baseline
                || base.generation != self.apply.generation
        })
    }
    fn phase(op: &DocumentEdit) -> u8 {
        match op {
            DocumentEdit::Definition(_) => 0,
            DocumentEdit::Preset(_) => 1,
            DocumentEdit::Hierarchy(_) => 2,
            DocumentEdit::Restore { .. } => 3,
        }
    }
    fn replace_key(old: &DocumentEdit, next: &DocumentEdit) -> bool {
        match (old, next) {
            (
                DocumentEdit::Definition(DefinitionEdit::SetVisual {
                    definition: a,
                    node: b,
                    ..
                }),
                DocumentEdit::Definition(DefinitionEdit::SetVisual {
                    definition: c,
                    node: d,
                    ..
                }),
            )
            | (
                DocumentEdit::Definition(DefinitionEdit::RenameNode {
                    definition: a,
                    node: b,
                    ..
                }),
                DocumentEdit::Definition(DefinitionEdit::RenameNode {
                    definition: c,
                    node: d,
                    ..
                }),
            ) => a == c && b == d,
            (
                DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                    definition: a, ..
                }),
                DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                    definition: b, ..
                }),
            ) => a == b,
            (
                DocumentEdit::Definition(DefinitionEdit::SetDefault {
                    definition: a,
                    node: b,
                    property: c,
                    ..
                }),
                DocumentEdit::Definition(DefinitionEdit::SetDefault {
                    definition: d,
                    node: e,
                    property: f,
                    ..
                }),
            ) => a == d && b == e && c == f,
            (
                DocumentEdit::Preset(PresetEdit::Update { preset: a }),
                DocumentEdit::Preset(PresetEdit::Update { preset: b }),
            ) => a.id == b.id,
            _ => false,
        }
    }
    fn recapture_new(&self, next: &mut Apply) -> Result<()> {
        let new: Vec<usize> = next
            .changes
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                self.base
                    .oib_capture_change(&c.address, c.value.clone())
                    .is_err()
                    .then_some(i)
            })
            .collect();
        if !new.is_empty() {
            let mut source = next.clone();
            source.changes.clear();
            let initial = self.base.preview_oib_apply(&source)?;
            for index in new {
                let c = &next.changes[index];
                next.changes[index] = initial.oib_capture_change(&c.address, c.value.clone())?;
            }
        }
        Ok(())
    }
    fn history_push(history: &mut Vec<Apply>, value: Apply) {
        history.push(value);
        while history
            .iter()
            .map(|a| flexbuffers::to_vec(a).map_or(HISTORY_BYTES + 1, |b| b.len()))
            .sum::<usize>()
            > HISTORY_BYTES
        {
            history.remove(0);
        }
    }
    fn set(&mut self, mut next: Apply) -> Result<bool> {
        if self.applying() {
            return Err("Apply is still awaiting its receipt".into());
        }
        if next == self.apply {
            return Ok(false);
        }
        next.operations.sort_by_key(Self::phase);
        let result = (|| {
            if next.operations.len() + next.changes.len() > MAX_OPERATIONS
                || flexbuffers::to_vec(&next).map_err(|e| e.to_string())?.len() > MAX_BATCH_BYTES
            {
                return Err("Working copy is full; Apply or undo changes before continuing".into());
            }
            self.recapture_new(&mut next)?;
            self.base.preview_oib_apply(&next)
        })();
        match result {
            Ok(candidate) => {
                Self::history_push(&mut self.undo, self.apply.clone());
                self.redo.clear();
                self.apply = next;
                self.candidate = candidate;
                self.error = None;
                self.revision += 1;
                Ok(true)
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }
    pub(super) fn stage_structure(&mut self, batch: EditBatch) -> Result<bool> {
        if Some(batch.generation) != self.candidate.oib_structural_generation() {
            return Err("That command belongs to an older working copy".into());
        }
        let mut next = self.apply.clone();
        for op in batch.operations {
            if let Some(i) = next
                .operations
                .iter()
                .position(|old| Self::replace_key(old, &op))
            {
                next.operations[i] = op;
            } else {
                next.operations.push(op);
            }
        }
        self.set(next)
    }
    pub(super) fn stage_property(
        &mut self,
        address: &PropertyAddress,
        value: Option<Value>,
    ) -> Result<bool> {
        let op = match address {
            PropertyAddress::Structural { .. } => Some(
                self.candidate
                    .oib_structural_operation(address, value.clone())?,
            ),
            PropertyAddress::Placement { object, property } => Some(
                self.candidate
                    .oib_placement_operation(object, *property, value.clone())?,
            ),
            _ => None,
        };
        if let Some(op) = op {
            return self.stage_structure(EditBatch {
                generation: self
                    .candidate
                    .oib_structural_generation()
                    .ok_or("Missing working source")?,
                operations: vec![op],
            });
        }
        let mut next = self.apply.clone();
        let change: Change = match self.base.oib_capture_change(address, value.clone()) {
            Ok(change) => change,
            Err(_) => {
                let mut source = next.clone();
                source.changes.clear();
                self.base
                    .preview_oib_apply(&source)?
                    .oib_capture_change(address, value)?
            }
        };
        if let Some(i) = next.changes.iter().position(|c| c.address == *address) {
            next.changes[i] = change;
        } else {
            next.changes.push(change);
        }
        self.set(next)
    }
    pub(super) fn stage_choice(
        &mut self,
        view: &crate::canvas::oib::document::PropertyRegisterView,
        value: Option<Value>,
    ) -> Result<bool> {
        let current = self.base.oib_property_register(&view.address)?;
        if current.cell != view.cell || current.version != view.version {
            return Err("OIB property register changed; refresh choices".into());
        }
        if !current.alternatives.contains(&value) {
            return Err("Value is not an observed OIB property alternative".into());
        }
        self.stage_property(&view.address, value)
    }
    pub(super) fn undo(&mut self) -> Result<bool> {
        if self.applying() {
            return Ok(false);
        }
        if self.error.take().is_some() {
            return Ok(true);
        }
        let Some(previous) = self.undo.pop() else {
            return Ok(false);
        };
        let candidate = self.base.preview_oib_apply(&previous)?;
        Self::history_push(&mut self.redo, self.apply.clone());
        self.apply = previous;
        self.candidate = candidate;
        self.revision += 1;
        Ok(true)
    }
    pub(super) fn redo(&mut self) -> Result<bool> {
        if self.applying() {
            return Ok(false);
        }
        let Some(next) = self.redo.pop() else {
            return Ok(false);
        };
        let candidate = self.base.preview_oib_apply(&next)?;
        Self::history_push(&mut self.undo, self.apply.clone());
        self.apply = next;
        self.candidate = candidate;
        self.error = None;
        self.revision += 1;
        Ok(true)
    }
    pub(super) fn submit(&mut self, live: &mut StateAdapter) -> Result<bool> {
        if !self.dirty() || self.applying() || self.error.is_some() {
            return Ok(false);
        };
        if self.stale(live) {
            self.error = Some("Design changed; review or discard this working copy".into());
            return Ok(false);
        };
        let digest = StateAdapter::oib_apply_digest(&self.apply)?;
        if !live.apply_oib(self.apply.clone())? {
            return Ok(false);
        };
        self.submitted = Some(digest);
        Ok(true)
    }
    /// Live receipts remain on the live adapter. A matching durable result, not
    /// a shared status string, decides whether to release the working copy.
    pub(super) fn synchronize(&mut self, live: &StateAdapter) -> Result<bool> {
        if let Some(digest) = &self.submitted {
            if !live.confirmed() {
                return Ok(false);
            };
            if let Some(receipt) = live.oib_apply_result()?
                && &receipt.digest == digest
            {
                match receipt.result {
                    ApplyResult::Accepted { .. } => {
                        *self = Self::new(live)?;
                        return Ok(true);
                    }
                    ApplyResult::Rejected { message } => {
                        self.submitted = None;
                        self.error = Some(message);
                        return Ok(true);
                    }
                }
            }
        } else if !self.dirty()
            && self.error.is_none()
            && (self.base_ui_generation != live.oib_generation() || self.stale(live))
        {
            *self = Self::new(live)?;
            return Ok(true);
        }
        Ok(false)
    }
}
