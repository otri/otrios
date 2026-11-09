// SPDX-License-Identifier: LGPL-3.0-only
//! One bounded, recoverable baseline transition inside the existing document.
use super::*;

pub(super) const FORMAT: &str = "otrios-oib-binding-v2";
const ADOPTION: &str = "otrios-oib-baseline-adoption-v1";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Adoption {
    format: String,
    generation: u64,
    previous: merkle::Hash,
}

pub(super) fn validate_metadata(binding: &Binding, status: Option<&Status>) -> Result<()> {
    if let Some(adoption) = &binding.adoption {
        if adoption.format != ADOPTION || adoption.generation == 0 {
            return Err("Unsupported OIB baseline adoption metadata".into());
        }
        if status.is_some_and(|status| status.structural_generation <= adoption.generation) {
            return Err("Invalid OIB baseline adoption generation".into());
        }
    }
    Ok(())
}

/// Complete canonical source/pin identity. Adoption evidence and allocator
/// bookkeeping are excluded; semantic epoch remains a separate contract.
pub(super) fn digest(binding: &Binding) -> Result<merkle::Hash> {
    Ok(super::digest(
        &[
            b"OTriOS\0oib-baseline-v1\0".as_slice(),
            &bytes(&(
                &binding.seed,
                &binding.admission,
                &binding.ui_hash,
                &binding.ui_ref,
            ))?,
        ]
        .concat(),
    ))
}

fn same_branch(before: &Snapshot, after: &Snapshot, path: &[u64]) -> bool {
    match (before.subtree(path), after.subtree(path)) {
        (Ok(a), Ok(b)) => a.hash() == b.hash(),
        (Err(merkle::Error::Missing), Err(merkle::Error::Missing)) => true,
        _ => false,
    }
}
fn paths(tree: &Snapshot, branch: u64) -> Result<BTreeSet<Vec<u64>>> {
    let mut result = BTreeSet::new();
    match tree.subtree(&[4, branch]) {
        Ok(tree) => address::leaves(&tree, &mut vec![4, branch], &mut result)?,
        Err(merkle::Error::Missing) => {}
        Err(error) => return Err(err(error)),
    }
    Ok(result)
}
fn preserve_properties(before: &Snapshot, after: &Snapshot) -> Result<()> {
    for branch in [1, 2] {
        let old = paths(before, branch)?;
        let new = paths(after, branch)?;
        if !old.is_subset(&new) {
            return Err("Baseline adoption removed a stored semantic property address".into());
        }
        for path in &old {
            if !same_branch(before, after, path) {
                return Err("Baseline adoption rewrote a causal property register".into());
            }
        }
        for path in new.difference(&old) {
            let leaf = after.subtree(path).map_err(err)?;
            if leaf
                .register()
                .is_none_or(|register| register.values().any(|(_, _, value)| value.is_some()))
            {
                return Err("New baseline properties must inherit without a user override".into());
            }
        }
    }
    Ok(())
}

impl Document {
    /// The observed structural revision and complete canonical baseline lock.
    pub fn oib_baseline(&self) -> Result<(u64, merkle::Hash)> {
        Ok((
            self.oib_status()?.structural_generation,
            digest(self.oib_binding()?.as_ref())?,
        ))
    }

    /// Authority-only candidate construction. Call only after signed package
    /// closure verification, persist its journal delta, then publish the clone.
    /// Every rejection leaves this document, its receipts and causal heads intact.
    pub fn adopt_oib_baseline(
        &mut self,
        ui: &Ui,
        expected_generation: u64,
        expected_lock: merkle::Hash,
    ) -> Result<bool> {
        if self.oib_projection {
            return Err("Baseline adoption requires the complete authority document".into());
        }
        if self.oib_baseline()? != (expected_generation, expected_lock) {
            return Err("OIB baseline changed; review the current lock and generation".into());
        }
        let previous = self.oib_binding()?;
        let normalized = super::super::super::oib::graph::materialize(ui)?;
        super::super::super::oib::validate(&normalized)?;
        let seed =
            super::super::super::oib::seed(&normalized).ok_or("Missing candidate OIB graph")?;
        let mut replacement = Binding {
            format: FORMAT.into(),
            identity_floor: previous.identity_floor,
            adoption: None,
            seed: seed.clone(),
            admission: super::super::super::oib::contract::admission(&normalized)?,
            ui_hash: hash(&super::super::super::encode_ui(&normalized)?),
            ui_ref: normalized.ui_ref.clone(),
        };
        if replacement.admission != previous.admission
            || replacement.ui_ref != previous.ui_ref
            || replacement.seed.graph.as_ref().map(|graph| graph.id)
                != previous.seed.graph.as_ref().map(|graph| graph.id)
        {
            return Err("Baseline adoption changed semantic admission".into());
        }
        if digest(&replacement)? == expected_lock {
            return Ok(false);
        }
        if same_source(&previous, &replacement) {
            let mut candidate = self.clone();
            let changed = candidate.configure_oib(&normalized)?;
            self.validate_changed(&candidate.tree)?;
            *self = candidate;
            return Ok(changed);
        }
        super::super::super::oib::imports::compatible(&previous.seed, &replacement.seed)?;
        replacement.adoption = Some(Adoption {
            format: ADOPTION.into(),
            generation: expected_generation,
            previous: expected_lock,
        });
        let mut candidate = self.clone();
        candidate.oib_ui = Some(std::sync::Arc::new(normalized));
        candidate.change(
            &[],
            vec![Edit {
                path: vec![4, 0],
                schema: self.extra_schema(4, 0),
                value: Some(bytes(&replacement)?),
            }],
        )?;
        let resolved = candidate.oib_resolved_seed()?;
        let mut additions = vec![];
        for cell in &resolved.cells {
            for branch in [1, 2] {
                let path = address::cell_path(&resolved, branch, cell.id)?;
                if matches!(candidate.tree.subtree(&path), Err(merkle::Error::Missing)) {
                    additions.push(Edit {
                        path,
                        schema: address::schema(&candidate.scope, &resolved, cell.id)?,
                        value: None,
                    });
                }
            }
        }
        candidate.change(&[], additions)?;
        // Compile the active values without copying/resolving a pending draft.
        candidate.oib_compiled(2)?;
        let mut status = candidate.oib_status()?;
        status.generation = status
            .generation
            .checked_add(1)
            .ok_or("OIB generation exhausted")?;
        status.structural_generation = status
            .structural_generation
            .checked_add(1)
            .ok_or("OIB structural generation exhausted")?;
        status.active_root = candidate.oib_active_root()?;
        if self.oib_status()?.source_draft == self.oib_draft_root()? {
            status.source_draft = candidate.oib_draft_root()?;
        }
        candidate.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )?;
        self.validate_changed(&candidate.tree)?;
        *self = candidate;
        Ok(true)
    }

    /// Reconstruct an exact baseline from admitted chrome and self-contained
    /// source only when its canonical UI hash matches. Mount keeps its prior
    /// complete Ui/layout pinned until its existing activation gates release.
    pub(in crate::canvas) fn oib_reconcile_ui(&mut self) -> Result<()> {
        let Some(previous) = self.oib_ui.clone() else {
            return Ok(());
        };
        let binding = self.oib_binding()?;
        if hash(&super::super::super::encode_ui(&previous)?) == binding.ui_hash {
            return Ok(());
        }
        let mut candidate = (*previous).clone();
        candidate.presentation = Some(Presentation::Oib(Box::new(binding.seed.clone())));
        let Ok(candidate) = super::super::super::oib::graph::materialize(&candidate) else {
            self.oib_ui = None;
            return Ok(());
        };
        if hash(&super::super::super::encode_ui(&candidate)?) == binding.ui_hash {
            super::super::super::oib::validate(&candidate)?;
            self.oib_ui = Some(std::sync::Arc::new(candidate));
        } else {
            self.oib_ui = None;
        }
        Ok(())
    }
    /// Check the dedicated transition witness during normal delta validation and
    /// durable replay. The witness alone is not admission: all compatible-source,
    /// causal-preservation and final closure checks still run below.
    pub(super) fn oib_validate_baseline_transition(
        &self,
        before: &Snapshot,
        after: &Snapshot,
    ) -> Result<bool> {
        let Ok(old_payload) = payload(before, &[4, 0]) else {
            return Ok(false);
        };
        let new_payload = payload(after, &[4, 0])?;
        if old_payload == new_payload {
            return Ok(false);
        }
        let old: Binding = decode(&old_payload)?;
        let new: Binding = decode(&new_payload)?;
        if old.format != FORMAT
            || new.format != FORMAT
            || old.identity_floor != new.identity_floor
            || old.admission != new.admission
            || old.ui_ref != new.ui_ref
            || old.seed.graph.as_ref().map(|graph| graph.id)
                != new.seed.graph.as_ref().map(|graph| graph.id)
        {
            return Err("OIB semantic admission or allocator floor changed".into());
        }
        if same_source(&old, &new) {
            return Ok(true);
        }
        super::super::super::oib::imports::compatible(&old.seed, &new.seed)?;
        let old_status: Status = decode(&payload(before, &[4, 3])?)?;
        let new_status: Status = decode(&payload(after, &[4, 3])?)?;
        let adoption = new
            .adoption
            .as_ref()
            .ok_or("Changed OIB baseline requires an adoption witness")?;
        if adoption.format != ADOPTION
            || adoption.generation != old_status.structural_generation
            || adoption.previous != digest(&old)?
            || new_status.structural_generation
                != old_status
                    .structural_generation
                    .checked_add(1)
                    .ok_or("OIB structural generation exhausted")?
            || new_status.generation
                != old_status
                    .generation
                    .checked_add(1)
                    .ok_or("OIB generation exhausted")?
            || bytes(&new_status.conflicts)? != bytes(&old_status.conflicts)?
            || new_status.error != old_status.error
        {
            return Err("Invalid OIB baseline adoption witness".into());
        }
        for path in [
            vec![1],
            vec![2],
            vec![3],
            vec![4, 4],
            vec![4, 5],
            vec![4, 6],
        ] {
            if !same_branch(before, after, &path) {
                return Err("Baseline adoption changed app data, receipts or local source".into());
            }
        }
        preserve_properties(before, after)?;
        Ok(true)
    }
}
