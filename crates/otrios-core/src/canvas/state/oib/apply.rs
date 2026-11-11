// SPDX-License-Identifier: LGPL-3.0-only
//! OIB-only working-copy admission: one private candidate and one activation.
use super::*;
use otrios_interfaces::oib::authoring::{
    Apply, ApplyReceipt, EditBatch, MAX_BATCH_BYTES, MAX_OPERATIONS,
};

impl Document {
    pub(in crate::canvas::state) fn oib_apply_candidate(
        &self,
        writer: Actor,
        apply: &Apply,
    ) -> Result<Self> {
        self.oib_apply_candidate_mode(writer, apply, false)
    }
    fn oib_apply_candidate_mode(
        &self,
        writer: Actor,
        apply: &Apply,
        preview: bool,
    ) -> Result<Self> {
        if apply.epoch.len() != 32
            || apply.baseline.len() != 32
            || apply.operations.len() + apply.changes.len() > MAX_OPERATIONS
            || bytes(apply)?.len() > MAX_BATCH_BYTES
        {
            return Err("Invalid OIB Apply envelope".into());
        }
        self.check_oib_epoch(&apply.epoch)?;
        if apply.baseline != self.oib_baseline()?.1 {
            return Err("Library baseline changed; review working copy".into());
        }
        let status = self.oib_status()?;
        if status.structural_generation != apply.generation {
            return Err("Document changed; review working copy".into());
        }
        if apply.operations.is_empty() && apply.changes.is_empty() {
            return Ok(self.clone());
        }
        if !apply.operations.is_empty()
            && (!status.conflicts.is_empty() || self.oib_draft_root()? != status.source_draft)
        {
            return Err("Resolve the UI draft before Apply".into());
        }
        // Match every captured register against the original, including its full
        // causal context. Equal visible values alone cannot authorize overwrite.
        let original_seed = self.oib_resolved_shared()?;
        let allocations = self.oib_allocations()?;
        let mut seen = BTreeSet::new();
        let mut introduced = BTreeSet::new();
        for change in &apply.changes {
            if !seen.insert(change.address.clone()) {
                return Err("Duplicate OIB Apply address".into());
            }
            if self.oib_retired_property(&change.address)? {
                return Err("OIB Apply target was removed".into());
            }
            let id = match super::super::super::oib::document::cell_for_address(
                &original_seed,
                &allocations,
                &change.address,
            ) {
                Ok(id) => id,
                Err(_) => {
                    // A structurally introduced address has no original leaf.
                    // Its exact seeded context is checked after staging below.
                    introduced.insert(change.address.clone());
                    continue;
                }
            };
            let leaf = self.oib_property_leaf(1, id)?;
            let observed = Snapshot::from_bytes(&change.observed).map_err(err)?;
            if observed.to_bytes().map_err(err)? != change.observed
                || leaf.to_bytes().map_err(err)? != change.observed
            {
                return Err("Property changed; review working copy".into());
            }
        }
        if !apply.changes.is_empty()
            && apply.operations.iter().any(|op| {
                matches!(
                    op,
                    otrios_interfaces::oib::authoring::DocumentEdit::Restore { .. }
                )
            })
        {
            return Err("Restore cannot be mixed with property edits".into());
        }
        let mut candidate = self.clone();
        let structural = if apply.operations.is_empty() {
            false
        } else {
            candidate.oib_stage_structure(
                writer,
                &EditBatch {
                    generation: apply.generation,
                    operations: apply.operations.clone(),
                },
            )?
        };
        // Resolve again after structure staging. A removed/retyped property must
        // reject the whole candidate, never become a silently ignored edit.
        if !apply.changes.is_empty() {
            let seed = candidate.oib_resolved_shared()?;
            for change in &apply.changes {
                if candidate.oib_retired_property(&change.address)? {
                    return Err("OIB Apply target was removed".into());
                }
                let id = super::super::super::oib::document::cell_for_address(
                    &seed,
                    &candidate.oib_allocations()?,
                    &change.address,
                )?;
                if introduced.contains(&change.address) {
                    let observed = Snapshot::from_bytes(&change.observed).map_err(err)?;
                    if observed.to_bytes().map_err(err)? != change.observed
                        || candidate
                            .oib_property_leaf(1, id)?
                            .to_bytes()
                            .map_err(err)?
                            != change.observed
                    {
                        return Err(
                            "New property initial context changed; review working copy".into()
                        );
                    }
                }
            }
            candidate.tree = candidate.oib_stage_changes(writer, &apply.changes)?;
        }
        if candidate.tree.hash() != self.tree.hash() {
            candidate.activate_oib(candidate.tree.clone())?;
            let mut status = candidate.oib_status()?;
            if !(status.conflicts.is_empty() || preview && apply.operations.is_empty())
                || status.error.is_some()
            {
                return Err(status.error.unwrap_or_else(|| "OIB Apply conflicts".into()));
            }
            if structural {
                status.structural_generation = status
                    .structural_generation
                    .checked_add(1)
                    .ok_or("OIB structural generation exhausted")?;
                candidate.change(
                    &[],
                    vec![Edit {
                        path: vec![4, 3],
                        schema: candidate.extra_schema(4, 3),
                        value: Some(bytes(&status)?),
                    }],
                )?;
            }
        }
        Ok(candidate)
    }
}

impl StateAdapter {
    /// Produce a private placement command without queuing an authority write.
    pub(in crate::canvas) fn oib_placement_operation(
        &self,
        object: &otrios_interfaces::oib::designer::ObjectAddress,
        property: otrios_interfaces::oib::designer::PlacementProperty,
        value: Option<Value>,
    ) -> Result<otrios_interfaces::oib::authoring::DocumentEdit> {
        use otrios_interfaces::oib::{
            authoring::{DocumentEdit, HierarchyEdit},
            designer::PlacementProperty,
        };
        let mut frame = [0.; 4];
        for (index, info) in self.oib_placement_properties(object)?.iter().enumerate() {
            let Some(Value::Number(n)) = info.value else {
                return Err("Missing placement value".into());
            };
            frame[index] = n;
        }
        let Some(Value::Number(value)) = value else {
            return Err("Placement requires an explicit numeric value".into());
        };
        let index = match property {
            PlacementProperty::X => 0,
            PlacementProperty::Y => 1,
            PlacementProperty::Width => 2,
            PlacementProperty::Height => 3,
        };
        frame[index] = value;
        Ok(DocumentEdit::Hierarchy(HierarchyEdit::Place {
            instance: object.instance,
            frame,
        }))
    }
    /// Capture immutable working-copy preconditions before any local command.
    pub fn oib_apply_base(&self) -> Result<Apply> {
        Ok(Apply {
            epoch: self.doc.oib_epoch()?,
            baseline: self.doc.oib_baseline()?.1.to_vec(),
            generation: self.doc.oib_status()?.structural_generation,
            operations: vec![],
            changes: vec![],
        })
    }
    pub fn oib_capture_change(
        &self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
        value: Option<Value>,
    ) -> Result<Change> {
        let seed = self.doc.oib_resolved_shared()?;
        let id = super::super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        super::super::super::oib::document::validate_edit(&seed, id, value.as_ref())?;
        if let Some(value) = value.as_ref() {
            let cell = seed
                .cells
                .iter()
                .find(|cell| cell.id == id)
                .ok_or("Unknown OIB property")?;
            super::super::super::oib::validate_value(cell, value)?;
        }
        Ok(Change {
            address: address.clone(),
            value,
            observed: self.doc.oib_property_leaf(1, id)?.to_bytes().map_err(err)?,
        })
    }
    /// Run the authority's pure candidate staging against this pinned base.
    /// It has no receipt, flight, transport replica or network side effect.
    pub fn preview_oib_apply(&self, apply: &Apply) -> Result<Self> {
        let mut preview = self.preview_clone();
        preview.doc = self
            .doc
            .oib_apply_candidate_mode(self.writer, apply, true)?;
        Ok(preview)
    }
    /// Queue once; no authoritative source is changed until its durable receipt.
    pub fn apply_oib(&mut self, apply: Apply) -> Result<bool> {
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        if apply.operations.is_empty() && apply.changes.is_empty() {
            return Ok(false);
        }
        if apply.operations.len() + apply.changes.len() > MAX_OPERATIONS
            || bytes(&apply)?.len() > MAX_BATCH_BYTES
        {
            return Err("OIB Apply budget".into());
        }
        // Preserve the captured epoch even if admission changed since editing.
        self.flight = Some(Flight {
            oib_apply: Some(apply),
            oib_epoch: None,
            oib: vec![],
            oib_instance: None,
            sequence: self
                .acknowledged
                .checked_add(1)
                .ok_or("Intent sequence exhausted")?,
            values: vec![],
            sent: false,
            issued_at: self.clock,
        });
        Ok(true)
    }
    /// Stable operation identity; a content digest is not a write capability.
    pub fn oib_apply_digest(apply: &Apply) -> Result<Vec<u8>> {
        Ok(hash(&bytes(&("oib-apply-v1", apply))?))
    }
    pub fn oib_apply_result(&self) -> Result<Option<ApplyReceipt>> {
        Ok(self
            .doc
            .receipt(&self.writer)?
            .and_then(|(_, receipt)| receipt.apply))
    }
}

#[cfg(test)]
mod tests;
