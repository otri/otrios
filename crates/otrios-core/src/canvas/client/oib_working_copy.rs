// SPDX-License-Identifier: LGPL-3.0-only
//! A bounded editor command log over a pinned source. Never a transport owner.
use super::*;
use otrios_interfaces::oib::{
    Change,
    authoring::{
        Apply, ApplyResult, DefinitionEdit, DocumentEdit, EditBatch, HierarchyEdit,
        MAX_BATCH_BYTES, MAX_OPERATIONS, PresetEdit,
    },
    designer::{PropertyAddress, Selection},
};
const HISTORY_BYTES: usize = 64 * 1024;
use super::oib_applied_history::{AppliedHistory, Direction, Review};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditLocation {
    pub(super) selection: Selection,
    pub(super) task: oib_workspace::Task,
}
#[derive(Clone)]
struct HistoryEntry {
    apply: Apply,
    location: Option<EditLocation>,
}
impl HistoryEntry {
    fn bytes(&self) -> usize {
        let path_bytes = match self.location.as_ref().map(|l| &l.selection) {
            Some(Selection::Object(object)) => {
                object.references.len()
                    * std::mem::size_of::<otrios_interfaces::oib::designer::NodeId>()
            }
            _ => 0,
        };
        std::mem::size_of::<Self>()
            + flexbuffers::to_vec(&self.apply).map_or(HISTORY_BYTES + 1, |b| b.len())
            + path_bytes
    }
}
pub(super) struct WorkingCopy {
    pub(super) base: StateAdapter,
    pub(super) candidate: StateAdapter,
    pub(super) apply: Apply,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    location: Option<EditLocation>,
    pub(super) error: Option<String>,
    pub(super) submitted: Option<Vec<u8>>,
    pub(super) revision: u64,
    base_ui_generation: Option<u64>,
    base_oib_root: Option<crate::merkle::Hash>,
    published: AppliedHistory,
    base_location: Option<EditLocation>,
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
            location: None,
            error: None,
            submitted: None,
            revision: 0,
            base_ui_generation: live.oib_generation(),
            base_oib_root: live.oib_root(),
            published: AppliedHistory::default(),
            base_location: None,
        })
    }
    pub(super) fn dirty(&self) -> bool {
        !self.apply.operations.is_empty() || !self.apply.changes.is_empty()
    }
    pub(super) fn can_undo(&self) -> bool {
        self.error.is_some()
            || !self.undo.is_empty()
            || (!self.dirty() && self.published.can_undo())
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty() || (!self.dirty() && self.published.can_redo())
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
            DocumentEdit::Style(_) => 0,
            DocumentEdit::Preset(_) => 1,
            DocumentEdit::Hierarchy(_) => 2,
            DocumentEdit::Restore { .. } => 3,
        }
    }
    fn replace_key(old: &DocumentEdit, next: &DocumentEdit) -> bool {
        match (old, next) {
            (
                DocumentEdit::Definition(DefinitionEdit::SetProperty {
                    definition: a,
                    descriptor: b,
                }),
                DocumentEdit::Definition(DefinitionEdit::SetProperty {
                    definition: c,
                    descriptor: d,
                }),
            ) => a == c && b.id == d.id,
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
    pub(super) fn remember_location(&mut self, selection: Selection, task: oib_workspace::Task) {
        self.location = Some(EditLocation { selection, task });
        if !self.dirty() && self.undo.is_empty() {
            self.base_location = self.location.clone();
        }
    }
    pub(super) fn location(&self) -> Option<EditLocation> {
        self.location.clone()
    }
    fn entry(&self) -> HistoryEntry {
        HistoryEntry {
            apply: self.apply.clone(),
            location: self.location.clone(),
        }
    }
    fn trim_history(&mut self) {
        // Both directions share one budget, including retained selection paths.
        while self
            .undo
            .iter()
            .chain(&self.redo)
            .map(HistoryEntry::bytes)
            .sum::<usize>()
            + self.published.bytes()
            > HISTORY_BYTES
        {
            if self.published.evict_oldest() {
                continue;
            }
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else {
                self.published.drop_review_detail();
                break;
            }
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
                self.published.normal_edit();
                self.undo.push(self.entry());
                self.redo.clear();
                self.trim_history();
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
            if let DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
                definition,
                property,
            }) = &op
            {
                let source = self.candidate.oib_history_source()?;
                next.changes.retain(|change| {
                    !super::super::oib::endpoint_impact::addresses_endpoint(
                        &source,
                        &change.address,
                        *definition,
                        *property,
                    )
                });
            }
            if let DocumentEdit::Definition(DefinitionEdit::DeleteDefinition { definition }) = &op {
                // Confirmed component deletion also discards its unapplied
                // literal overrides. Keep unrelated edits, and retain the
                // complete prior command in local Undo through set().
                next.changes.retain(|change| {
                    !matches!(
                        &change.address,
                        PropertyAddress::Definition { definition: owner, .. } if owner == definition
                    )
                });
            }
            if let DocumentEdit::Hierarchy(HierarchyEdit::Remove { instance }) = &op {
                // Remove only this occurrence's private literals. Local Undo
                // retains the complete prior candidate through set().
                next.changes.retain(|change| {
                    !matches!(&change.address,
                    PropertyAddress::Occurrence { instance: owner, .. } if owner == instance)
                });
            }
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
        let Some(previous) = self.undo.last().cloned() else {
            return if !self.dirty() {
                self.prepare_applied(Direction::Undo)
            } else {
                Ok(false)
            };
        };
        let candidate = self.base.preview_oib_apply(&previous.apply)?;
        self.undo.pop();
        self.redo.push(self.entry());
        self.apply = previous.apply;
        self.location = previous.location;
        self.trim_history();
        self.candidate = candidate;
        self.revision += 1;
        Ok(true)
    }
    pub(super) fn redo(&mut self) -> Result<bool> {
        if self.applying() {
            return Ok(false);
        }
        let Some(next) = self.redo.last().cloned() else {
            return if !self.dirty() {
                self.prepare_applied(Direction::Redo)
            } else {
                Ok(false)
            };
        };
        let candidate = self.base.preview_oib_apply(&next.apply)?;
        self.redo.pop();
        self.undo.push(self.entry());
        self.apply = next.apply;
        self.location = next.location;
        self.trim_history();
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
        self.published.check_submission(&digest);
        self.submitted = Some(digest);
        Ok(true)
    }
    pub(super) fn history_review_is_current(&self, live: &StateAdapter) -> bool {
        self.base_oib_root == live.oib_root()
    }
    pub(super) fn history_review(&self) -> Option<&Review> {
        self.published.review()
    }
    pub(super) fn history_review_mut(&mut self) -> Option<&mut Review> {
        self.published.review_mut()
    }
    pub(super) fn clear_history_review(&mut self) {
        self.published.clear_review();
    }
    /// Discard private commands while retaining receipt-owned applied history.
    pub(super) fn reset_to_live(&mut self, live: &StateAdapter) -> Result<()> {
        let mut next = Self::new(live)?;
        next.published = std::mem::take(&mut self.published);
        next.location = self.location.clone();
        next.base_location = next.location.clone();
        // A discarded prepared inverse must not classify a later normal Apply.
        next.published.normal_edit();
        *self = next;
        self.trim_history();
        Ok(())
    }
    fn prepare_applied(&mut self, direction: Direction) -> Result<bool> {
        let Some(prepared) = self.published.prepare(direction, &self.base)? else {
            self.trim_history();
            return Ok(self.published.review().is_some());
        };
        let apply = prepared.apply.clone();
        let location = prepared.location.clone();
        // set validates the complete private inverse but never sends it.
        let changed = match self.set(apply) {
            Ok(changed) => changed,
            Err(reason) => {
                self.error = None;
                self.published.unavailable(direction, reason, vec![]);
                self.trim_history();
                return Ok(true);
            }
        };
        if changed {
            self.published.bind(prepared);
            if location.is_some() {
                self.location = location;
            }
            self.trim_history();
        }
        Ok(changed)
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
                match &receipt.result {
                    ApplyResult::Accepted { .. } => {
                        self.published.accepted(
                            &self.base,
                            &self.candidate,
                            &self.apply,
                            &receipt,
                            self.base_location.clone(),
                            self.location.clone(),
                        )?;
                        self.reset_to_live(live)?;
                        return Ok(true);
                    }
                    ApplyResult::Rejected { message } => {
                        self.submitted = None;
                        self.error = Some(message.clone());
                        return Ok(true);
                    }
                }
            }
        } else if !self.dirty()
            && self.error.is_none()
            && self.history_review().is_none()
            && (self.base_ui_generation != live.oib_generation()
                || self.base_oib_root != live.oib_root()
                || self.stale(live))
        {
            self.reset_to_live(live)?;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
#[path = "oib_applied_history_tests.rs"]
pub(super) mod applied_history_tests;
