// SPDX-License-Identifier: LGPL-3.0-only
//! One bounded structural intent, one candidate and one coherent publication.
use super::*;
use otrios_interfaces::oib::authoring::{
    DefinitionEdit, DocumentEdit, MAX_BATCH_BYTES, MAX_OPERATIONS, PresetEdit,
};

/// Reservations preserve command identities when phase ordering places a later
/// definition edit before an earlier preset/instance allocation.
#[derive(Clone)]
pub(in crate::canvas::state) struct IdentityPlan {
    floor: u64,
    explicit: BTreeSet<u64>,
    claimed: BTreeSet<u64>,
}
impl IdentityPlan {
    fn new(
        source: &otrios_interfaces::oib::authoring::DocumentSource,
        batch: &InstanceOperation,
    ) -> Result<Self> {
        let mut explicit = BTreeSet::new();
        let mut slots = BTreeSet::new();
        let mut reserve = |id: u64| -> Result<()> {
            if id < source.next_identity || id == u64::MAX || !explicit.insert(id) {
                return Err("Invalid or duplicate reserved OIB identity".into());
            }
            Ok(())
        };
        for operation in &batch.operations {
            match operation {
                DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
                    definition,
                    root,
                    ..
                }) => {
                    reserve(definition.0)?;
                    reserve(root.id.0)?;
                }
                DocumentEdit::Definition(DefinitionEdit::CreateNode { node, .. }) => {
                    reserve(node.id.0)?
                }
                DocumentEdit::Definition(DefinitionEdit::SetSlots {
                    definition,
                    slots: values,
                }) => {
                    for slot in values {
                        let existing = source
                            .definitions
                            .iter()
                            .find(|d| d.id == *definition)
                            .is_some_and(|d| d.slots.iter().any(|s| s.id == slot.id));
                        if !existing && slots.insert((*definition, slot.id)) {
                            reserve(slot.id.0)?;
                        }
                    }
                }
                DocumentEdit::Preset(PresetEdit::Create { preset }) => reserve(preset.id)?,
                _ => {}
            }
        }
        Ok(Self {
            floor: source.next_identity,
            explicit,
            claimed: BTreeSet::new(),
        })
    }
    fn next_implicit(&self) -> Result<u64> {
        let mut id = self.floor;
        while self.explicit.contains(&id) || self.claimed.contains(&id) {
            id = id.checked_add(1).ok_or("OIB identity space exhausted")?;
        }
        if id == u64::MAX {
            return Err("OIB identity space exhausted".into());
        }
        Ok(id)
    }
    fn explicit(&mut self, id: u64) -> Result<()> {
        if !self.explicit.contains(&id) || !self.claimed.insert(id) {
            return Err("OIB identity was not reserved or was already claimed".into());
        }
        Ok(())
    }
    fn implicit(&mut self, id: u64) -> Result<()> {
        if id != self.next_implicit()? || !self.claimed.insert(id) {
            return Err("Invalid implicit OIB identity".into());
        }
        Ok(())
    }
    fn finish(self, next: u64) -> Result<()> {
        if !self.explicit.is_subset(&self.claimed)
            || next.checked_sub(self.floor) != Some(self.claimed.len() as u64)
            || self
                .claimed
                .iter()
                .any(|id| *id < self.floor || *id >= next)
        {
            return Err(
                "OIB identity reservations must fill one contiguous authority interval".into(),
            );
        }
        Ok(())
    }
}

impl Document {
    pub(super) fn oib_allocating_id(&self) -> Result<u64> {
        match &self.oib_identity_plan {
            Some(plan) => plan.next_implicit(),
            None => self.oib_next_id(),
        }
    }
    pub(super) fn oib_claim_implicit(&mut self, id: u64) -> Result<()> {
        if let Some(plan) = &mut self.oib_identity_plan {
            plan.implicit(id)?;
        }
        Ok(())
    }
    fn oib_claim_explicit(&mut self, next: &mut u64, id: u64) -> Result<()> {
        if let Some(plan) = &mut self.oib_identity_plan {
            plan.explicit(id)?;
            *next = (*next).max(id.checked_add(1).ok_or("OIB identity space exhausted")?);
            Ok(())
        } else {
            claim(next, id)
        }
    }
    pub(super) fn oib_validate_structure(&self, batch: &InstanceOperation) -> Result<()> {
        if batch.generation == 0
            || batch.operations.is_empty()
            || batch.operations.len() > MAX_OPERATIONS
            || bytes(batch)?.len() > MAX_BATCH_BYTES
        {
            return Err("Invalid OIB edit batch envelope".into());
        }
        let mut phase = 0;
        for operation in &batch.operations {
            match operation {
                DocumentEdit::Definition(_) if phase > 0 => {
                    return Err("Definition edits must precede preset and instance edits".into());
                }
                DocumentEdit::Definition(_) => {}
                DocumentEdit::Preset(_) if phase > 1 => {
                    return Err("Preset edits must precede instance edits".into());
                }
                DocumentEdit::Preset(_) => phase = 1,
                DocumentEdit::Hierarchy(_) => phase = 2,
                DocumentEdit::Restore { .. } if batch.operations.len() != 1 => {
                    return Err("Restore must be the only operation in its batch".into());
                }
                DocumentEdit::Restore { .. } => {}
            }
        }
        let status = self.oib_status()?;
        if batch.generation != status.structural_generation {
            return Err("Document changed; review structure and retry".into());
        }
        // Reset-all-defaults is an explicit owner command that can recover an
        // invalid/conflicted property draft. Every other structural command
        // requires the coherent draft the author observed.
        let defaults = matches!(
            batch.operations.as_slice(),
            [DocumentEdit::Restore { defaults: true }]
        );
        if !defaults
            && (!status.conflicts.is_empty() || self.oib_draft_root()? != status.source_draft)
        {
            return Err("Resolve the UI draft before changing structure".into());
        }
        if self.oib_binding()?.seed.instance_surface.is_none() {
            return Err("OIB structural authoring is not admitted".into());
        }
        Ok(())
    }

    pub(super) fn oib_structure_write(
        &mut self,
        writer: Actor,
        batch: &InstanceOperation,
    ) -> Result<()> {
        let mut candidate = self.clone();
        let original = self.tree.hash();
        let result = (|| {
            candidate.oib_stage_structure(writer, batch)?;
            if candidate.tree.hash() != original {
                // activate_oib compiles the complete draft once. A failed
                // compilation stays private and the whole candidate is dropped.
                candidate.activate_oib(candidate.tree.clone())?;
                let status = candidate.oib_status()?;
                if !status.conflicts.is_empty() || status.error.is_some() {
                    return Err(status
                        .error
                        .unwrap_or_else(|| "Resolve UI conflicts first".into()));
                }
                let mut status = status;
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
            } else {
                // Exact no-ops still receive durable acceptance, and clear an
                // earlier rejection without rewriting properties or generation.
                let mut status = candidate.oib_status()?;
                status.error = None;
                candidate.change(
                    &[],
                    vec![Edit {
                        path: vec![4, 3],
                        schema: candidate.extra_schema(4, 3),
                        value: Some(bytes(&status)?),
                    }],
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.tree = candidate.tree,
            Err(error) => {
                self.oib_reject_structure(&error)?;
            }
        }
        Ok(())
    }

    /// Stage one already-bounded structural batch without activating it.
    /// The caller owns the private candidate and decides its single publication.
    pub(super) fn oib_stage_structure(
        &mut self,
        writer: Actor,
        batch: &InstanceOperation,
    ) -> Result<bool> {
        let original = self.tree.hash();
        self.oib_validate_structure(batch)?;
        if !matches!(batch.operations.as_slice(), [DocumentEdit::Restore { .. }]) {
            if self.oib_identity_plan.is_some() {
                return Err("Nested OIB identity reservation".into());
            }
            self.oib_identity_plan = Some(IdentityPlan::new(&self.oib_source()?, batch)?);
        }
        let definitions: Vec<_> = batch
            .operations
            .iter()
            .filter_map(|operation| {
                if let DocumentEdit::Definition(operation) = operation {
                    Some(operation.clone())
                } else {
                    None
                }
            })
            .collect();
        if !definitions.is_empty() {
            self.oib_stage_definitions(&definitions)?;
        }
        let presets: Vec<_> = batch
            .operations
            .iter()
            .filter_map(|operation| {
                if let DocumentEdit::Preset(operation) = operation {
                    Some(operation.clone())
                } else {
                    None
                }
            })
            .collect();
        if !presets.is_empty() {
            self.oib_stage_presets(&presets)?;
        }
        let hierarchy: Vec<_> = batch
            .operations
            .iter()
            .filter_map(|operation| {
                if let DocumentEdit::Hierarchy(operation) = operation {
                    Some(operation.clone())
                } else {
                    None
                }
            })
            .collect();
        if !hierarchy.is_empty() {
            self.oib_hierarchy_stage(&hierarchy)?;
        }
        if let [DocumentEdit::Restore { defaults }] = batch.operations.as_slice() {
            if *defaults {
                let signed = self.oib_binding()?;
                let resolved = self.oib_baseline_graph()?;
                let (mut restored, desired) =
                    reset::restore(&signed.seed, &resolved, &self.oib_source()?)?;
                restored.instances = desired.iter().map(Into::into).collect();
                let mut edits = vec![self.oib_source_edit(|source| *source = restored)?];
                for allocation in desired {
                    edits.push(Edit {
                        path: cell_path(5, allocation.id),
                        schema: self.oib_instance_schema(allocation.id),
                        value: Some(bytes(&true)?),
                    });
                }
                self.change(&[], edits)?;
            } else {
                self.oib_restore_residents(false, writer)?;
            }
        }
        let reset_defaults = matches!(
            batch.operations.as_slice(),
            [DocumentEdit::Restore { defaults: true }]
        );
        if let Some(plan) = self.oib_identity_plan.take() {
            plan.finish(self.oib_source()?.next_identity)?;
        }
        if self.tree.hash() != original || reset_defaults {
            self.oib_prepare_final_structure(reset_defaults, writer)?;
        }
        Ok(self.tree.hash() != original)
    }

    fn oib_stage_definitions(&mut self, operations: &[DefinitionEdit]) -> Result<()> {
        let mut seed = self.oib_authored_seed()?;
        let graph = self
            .oib_binding()?
            .seed
            .graph
            .clone()
            .ok_or("Missing signed graph")?;
        let shell = graph
            .instances
            .iter()
            .find(|instance| instance.id == graph.presented_root)
            .ok_or("Missing presented shell")?;
        let shell_definition = graph
            .presets
            .iter()
            .find(|preset| preset.id == shell.preset)
            .ok_or("Missing shell preset")?
            .definition;
        let mut next = self.oib_source()?.next_identity;
        for operation in operations {
            match operation {
                DefinitionEdit::DeleteDefinition { definition }
                    if *definition == shell_definition =>
                {
                    return Err("The presented shell definition has fixed ownership".into());
                }
                DefinitionEdit::CreateDefinition {
                    definition, root, ..
                } => {
                    self.oib_claim_explicit(&mut next, definition.0)?;
                    self.oib_claim_explicit(&mut next, root.id.0)?;
                }
                DefinitionEdit::CreateNode { node, .. } => {
                    self.oib_claim_explicit(&mut next, node.id.0)?
                }
                DefinitionEdit::SetSlots { definition, slots } => {
                    let existing = seed
                        .designer
                        .as_ref()
                        .ok_or("Missing authored document")?
                        .definitions
                        .iter()
                        .find(|current| current.id == *definition)
                        .ok_or("Unknown slot definition")?;
                    for slot in slots {
                        if !existing.slots.iter().any(|current| current.id == slot.id) {
                            self.oib_claim_explicit(&mut next, slot.id.0)?;
                        }
                    }
                }
                _ => {}
            }
            let definitions = super::super::super::oib::document::authoring::transform(
                &seed,
                std::slice::from_ref(operation),
            )?;
            seed.designer
                .as_mut()
                .ok_or("Missing authored document")?
                .definitions = definitions.clone();
            seed.graph
                .as_mut()
                .ok_or("Missing canonical graph")?
                .definitions = definitions;
        }
        let definitions = seed
            .designer
            .ok_or("Missing authored document")?
            .definitions;
        let before = self.oib_source()?;
        if bytes(&definitions)? != bytes(&before.definitions)? || next != before.next_identity {
            let edit = self.oib_source_edit(|source| {
                source.definitions = definitions;
                source.next_identity = next;
            })?;
            self.change(&[], vec![edit])?;
        }
        Ok(())
    }

    fn oib_stage_presets(&mut self, operations: &[PresetEdit]) -> Result<()> {
        let before = self.oib_source()?;
        let graph = self
            .oib_binding()?
            .seed
            .graph
            .clone()
            .ok_or("Missing signed graph")?;
        let shell = graph
            .instances
            .iter()
            .find(|instance| instance.id == graph.presented_root)
            .ok_or("Missing presented shell")?;
        let shell_preset = graph
            .presets
            .iter()
            .find(|preset| preset.id == shell.preset)
            .ok_or("Missing shell preset")?;
        let mut presets = before.presets.clone();
        let mut next = before.next_identity;
        for operation in operations {
            match operation {
                PresetEdit::Create { preset } => {
                    self.oib_claim_explicit(&mut next, preset.id)?;
                    if presets.iter().any(|existing| existing.id == preset.id) {
                        return Err("Preset identity already exists".into());
                    }
                    presets.push(preset.clone());
                }
                PresetEdit::Update { preset } => {
                    if preset.id == shell.preset
                        && (preset.definition != shell_preset.definition || preset.palette)
                    {
                        return Err("The presented shell preset cannot be rebound".into());
                    }
                    *presets
                        .iter_mut()
                        .find(|existing| existing.id == preset.id)
                        .ok_or("Unknown preset")? = preset.clone();
                }
                PresetEdit::Delete { preset } => {
                    if *preset == shell.preset {
                        return Err("The presented shell preset has fixed ownership".into());
                    }
                    let index = presets
                        .iter()
                        .position(|existing| existing.id == *preset)
                        .ok_or("Unknown preset")?;
                    presets.remove(index);
                }
            }
        }
        presets.sort_by_key(|preset| preset.id);
        if bytes(&presets)? != bytes(&before.presets)? || next != before.next_identity {
            let edit = self.oib_source_edit(|source| {
                source.presets = presets;
                source.next_identity = next;
            })?;
            self.change(&[], vec![edit])?;
        }
        Ok(())
    }

    fn oib_prepare_final_structure(&mut self, reset_defaults: bool, writer: Actor) -> Result<()> {
        let live = self.oib_live(1)?;
        let source = self.oib_source()?;
        let residents: Vec<_> = source
            .instances
            .iter()
            .filter(|instance| live.contains(&instance.id))
            .cloned()
            .collect();
        for instance in &residents {
            let preset = source
                .presets
                .iter()
                .find(|preset| preset.id == instance.prototype)
                .ok_or("Remove instances of deleted presets explicitly")?;
            if preset.definition.0 != instance.component {
                return Err(
                    "Remove or replace instances before changing their preset definition".into(),
                );
            }
        }
        if residents.len() != source.instances.len() {
            let edit = self.oib_source_edit(|source| source.instances = residents)?;
            self.change(&[], vec![edit])?;
        }
        // Deleted semantic addresses disappear only from this private candidate.
        // All surviving registers are projected unchanged, then newly introduced
        // properties inherit their current source defaults before compilation.
        let seed = self.oib_resolved_seed()?;
        let mut edits = Vec::new();
        for cell in &seed.cells {
            let path = address::cell_path(&seed, 1, cell.id)?;
            if reset_defaults || self.tree.subtree(&path).is_err() {
                edits.push(Edit {
                    path,
                    schema: address::schema(&self.scope, &seed, cell.id)?,
                    value: None,
                });
            }
        }
        self.tree = self.tree.edit(writer, &edits).map_err(err)?.0;
        self.oib_reclaim()
    }

    pub(super) fn oib_reject_structure(&mut self, error: &str) -> Result<()> {
        let mut status = self.oib_status()?;
        let mut diagnostic = format!("Structure rejected: {error}");
        while diagnostic.len() > 256 {
            diagnostic.pop();
        }
        status.error = Some(diagnostic);
        self.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )
        .map(|_| ())
    }
}

fn claim(next: &mut u64, identity: u64) -> Result<()> {
    if identity == 0 || identity != *next {
        return Err("New identities must use the observed authority counter".into());
    }
    *next = next.checked_add(1).ok_or("OIB identity space exhausted")?;
    Ok(())
}
