// SPDX-License-Identifier: LGPL-3.0-only
//! Receipt-owned applied history. Preparation is private; only an exact accepted
//! inverse receipt moves a record between directions.
use super::oib_working_copy::EditLocation;
use super::*;
use crate::canvas::oib::history::{Entry, ReviewConflict};
use otrios_interfaces::oib::{
    Change,
    authoring::{
        Apply, ApplyReceipt, ApplyResult, DefinitionEdit, DocumentEdit, HierarchyEdit,
        MAX_BATCH_BYTES, MAX_OPERATIONS,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Undo,
    Redo,
}
pub(super) struct Review {
    pub(super) direction: Direction,
    pub(super) reason: String,
    pub(super) conflicts: Vec<ReviewConflict>,
    pub(super) scroll: u16,
}
struct Record {
    id: u64,
    inverse: Option<Entry>,
    unsupported: Option<String>,
    before: Option<EditLocation>,
    after: Option<EditLocation>,
}
impl Record {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .inverse
                .as_ref()
                .map_or(0, |e| e.encoded_bytes().unwrap_or(usize::MAX / 2))
            + self.unsupported.as_ref().map_or(0, String::len)
            + location_bytes(&self.before)
            + location_bytes(&self.after)
    }
}
fn location_bytes(location: &Option<EditLocation>) -> usize {
    match location.as_ref().map(|l| &l.selection) {
        Some(otrios_interfaces::oib::designer::Selection::Object(o)) => {
            o.references.len() * std::mem::size_of::<otrios_interfaces::oib::designer::NodeId>()
        }
        _ => 0,
    }
}
#[derive(Clone)]
struct Intent {
    direction: Direction,
    id: u64,
    digest: Vec<u8>,
}
pub(super) struct Prepared {
    pub(super) apply: Apply,
    pub(super) location: Option<EditLocation>,
    intent: Intent,
}
#[derive(Default)]
pub(super) struct AppliedHistory {
    undo: Vec<Record>,
    redo: Vec<Record>,
    pending: Option<Intent>,
    review: Option<Review>,
    next: u64,
    review_id: Option<u64>,
}
impl AppliedHistory {
    #[cfg(test)]
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub(super) fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }
    pub(super) fn clear_review(&mut self) {
        self.review = None;
        self.review_id = None;
    }
    pub(super) fn review_mut(&mut self) -> Option<&mut Review> {
        self.review.as_mut()
    }
    pub(super) fn normal_edit(&mut self) {
        self.pending = None;
        self.review = None;
        self.review_id = None;
    }
    pub(super) fn bind(&mut self, prepared: Prepared) {
        self.pending = Some(prepared.intent);
        self.review = None;
        self.review_id = None;
    }
    pub(super) fn check_submission(&mut self, digest: &[u8]) {
        if self.pending.as_ref().is_some_and(|i| i.digest != digest) {
            self.pending = None;
        }
    }
    pub(super) fn bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(Record::bytes)
            .sum::<usize>()
            + self.review.as_ref().map_or(0, |r| {
                r.reason.len()
                    + r.conflicts
                        .iter()
                        .map(|c| {
                            c.target.len()
                                + c.before.len()
                                + c.applied.len()
                                + c.current.len()
                                + c.before_text.len()
                                + c.applied_text.len()
                                + c.current_text.len()
                                + std::mem::size_of::<ReviewConflict>()
                        })
                        .sum::<usize>()
            })
    }
    pub(super) fn evict_oldest(&mut self) -> bool {
        let locked = self.pending.as_ref().map(|i| i.id).or(self.review_id);
        let u = self.undo.iter().position(|r| Some(r.id) != locked);
        let r = self.redo.iter().position(|r| Some(r.id) != locked);
        match (u, r) {
            (Some(u), Some(r)) if self.undo[u].id < self.redo[r].id => {
                self.undo.remove(u);
            }
            (_, Some(r)) => {
                self.redo.remove(r);
            }
            (Some(u), None) => {
                self.undo.remove(u);
            }
            _ => return false,
        }
        true
    }
    pub(super) fn drop_review_detail(&mut self) {
        if let Some(review) = &mut self.review {
            review.conflicts.clear();
            review.reason="Comparison exceeds this editor’s history memory; inspect current sources before continuing".into();
        }
    }
    pub(super) fn unavailable(
        &mut self,
        direction: Direction,
        reason: String,
        conflicts: Vec<ReviewConflict>,
    ) {
        // The planner retains canonical proofs in Entry. This is a display
        // projection only; avoid retaining a second copy of every proof leaf.
        let conflicts = conflicts
            .into_iter()
            .map(|mut conflict| {
                conflict.before = vec![];
                conflict.applied = vec![];
                conflict.current = vec![];
                conflict
            })
            .collect();
        self.review = Some(Review {
            direction,
            scroll: 0,
            reason: reason.chars().take(256).collect(),
            conflicts,
        });
    }
    pub(super) fn prepare(
        &mut self,
        direction: Direction,
        current: &StateAdapter,
    ) -> Result<Option<Prepared>> {
        let record = match direction {
            Direction::Undo => self.undo.last(),
            Direction::Redo => self.redo.last(),
        };
        let Some(record) = record else {
            return Ok(None);
        };
        self.review_id = Some(record.id);
        let Some(inverse) = &record.inverse else {
            self.unavailable(
                direction,
                record
                    .unsupported
                    .clone()
                    .unwrap_or_else(|| "This change cannot be undone safely".into()),
                vec![],
            );
            return Ok(None);
        };
        let source = current.oib_history_source()?;
        let changes: Result<Vec<Change>> = inverse
            .property_values()
            .into_iter()
            .map(|(a, v)| current.oib_capture_change(&a, v))
            .collect();
        let changes = match changes {
            Ok(v) => v,
            Err(reason) => {
                self.unavailable(direction, reason, vec![]);
                return Ok(None);
            }
        };
        let generation = current.oib_generation().unwrap_or(0);
        let conflicts = inverse.review(&source, generation, &changes)?;
        if !conflicts.is_empty() {
            self.unavailable(
                direction,
                "Applied sources changed; review before, applied and current values".into(),
                conflicts,
            );
            return Ok(None);
        }
        let planned = (|| {
            let mut plan =
                inverse.plan(&source, generation, current.oib_apply_base()?, &changes)?;
            let restored = inverse.restored_property_values(&source)?;
            if !restored.is_empty() {
                // Fresh addresses have no old causal context. Seed them through
                // the exact structural candidate, then capture that candidate's
                // initial register context for the single final Apply.
                let seeded = current.preview_oib_apply(&plan)?;
                for (address, value) in restored {
                    plan.changes
                        .push(seeded.oib_capture_change(&address, value)?);
                }
            }
            if plan.operations.len() + plan.changes.len() > MAX_OPERATIONS
                || flexbuffers::to_vec(&plan).map_err(err)?.len() > MAX_BATCH_BYTES
            {
                return Err(
                    "Restored content exceeds the 16-operation / 8 KiB Apply budget".into(),
                );
            }
            let location = record
                .before
                .as_ref()
                .map(|location| -> Result<EditLocation> {
                    Ok(EditLocation {
                        selection: inverse.restored_selection(&source, &location.selection)?,
                        task: location.task,
                    })
                })
                .transpose()?;
            Ok((plan, location))
        })();
        let (plan, location) = match planned {
            Ok(result) => result,
            Err(reason) => {
                self.unavailable(direction, reason, vec![]);
                return Ok(None);
            }
        };
        let intent = Intent {
            direction,
            id: record.id,
            digest: StateAdapter::oib_apply_digest(&plan)?,
        };
        Ok(Some(Prepared {
            apply: plan,
            location,
            intent,
        }))
    }
    pub(super) fn accepted(
        &mut self,
        before: &StateAdapter,
        after: &StateAdapter,
        apply: &Apply,
        receipt: &ApplyReceipt,
        before_location: Option<EditLocation>,
        after_location: Option<EditLocation>,
    ) -> Result<()> {
        if !matches!(receipt.result, ApplyResult::Accepted { .. })
            || receipt.digest != StateAdapter::oib_apply_digest(apply)?
        {
            return Err("History requires the matching accepted Apply receipt".into());
        };
        let removed = apply.operations.iter().any(|operation| {
            matches!(
                operation,
                DocumentEdit::Definition(
                    DefinitionEdit::DeleteDefinition { .. }
                        | DefinitionEdit::DeleteNode { .. }
                        | DefinitionEdit::DeleteProperty { .. }
                ) | DocumentEdit::Hierarchy(HierarchyEdit::Remove { .. })
            )
        });
        let captured = (|| {
            // Structure admission guards the structural generation, allowing a
            // separate property publication immediately before this deletion.
            // The receipt must prove our captured pre-delete view was the last
            // coherent UI revision; otherwise removed values may be stale and
            // cannot be reconstructed from the post-delete replica.
            if removed {
                let ApplyResult::Accepted { generation, .. } = receipt.result else {
                    unreachable!("accepted receipt checked above")
                };
                if before.oib_generation().and_then(|g| g.checked_add(1)) != Some(generation) {
                    return Err("Sources changed before deletion was accepted; captured removed values cannot establish Undo history".into());
                }
            }
            let properties = if removed {
                before.oib_history_properties()?
            } else {
                vec![]
            };
            Entry::capture_with_properties(
                &before.oib_history_source()?,
                &after.oib_history_source()?,
                apply,
                receipt,
                &properties,
            )
        })();
        self.next = self
            .next
            .checked_add(1)
            .ok_or("History sequence exhausted")?;
        let (inverse, unsupported) = match captured {
            Ok(v) => (Some(v), None),
            Err(reason) => (None, Some(reason)),
        };
        let mut record = Record {
            id: self.next,
            inverse,
            unsupported,
            before: before_location,
            after: after_location,
        };
        if record.bytes() > 64 * 1024 - 2048 {
            record.inverse = None;
            record.unsupported=Some("This applied change exceeds the available history memory; inspect its sources before changing it".into());
        }
        let pending = self.pending.take().filter(|i| i.digest == receipt.digest);
        match pending {
            Some(intent)
                if intent.direction == Direction::Undo
                    && self.undo.last().is_some_and(|r| r.id == intent.id) =>
            {
                let undone = self.undo.pop().unwrap();
                if let (Some(undone), Some(inverse)) = (&undone.inverse, &record.inverse) {
                    for earlier in &mut self.undo {
                        if let Some(entry) = &mut earlier.inverse {
                            entry.reanchor_after_undo(undone, inverse);
                        }
                    }
                }
                self.redo.push(record);
            }
            Some(intent)
                if intent.direction == Direction::Redo
                    && self.redo.last().is_some_and(|r| r.id == intent.id) =>
            {
                let redone = self.redo.pop().unwrap();
                if let (Some(undone), Some(inverse)) = (&redone.inverse, &record.inverse) {
                    for earlier in &mut self.redo {
                        if let Some(entry) = &mut earlier.inverse {
                            entry.reanchor_after_undo(undone, inverse);
                        }
                    }
                }
                self.undo.push(record);
            }
            _ => {
                self.undo.push(record);
                self.redo.clear();
            }
        }
        self.review = None;
        self.review_id = None;
        Ok(())
    }
}
