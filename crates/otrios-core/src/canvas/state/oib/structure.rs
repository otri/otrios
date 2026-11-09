// SPDX-License-Identifier: LGPL-3.0-only
//! One bounded structural intent, one candidate and one coherent publication.
use super::*;
use otrios_interfaces::oib::authoring::{
    DefinitionEdit, DocumentEdit, MAX_BATCH_BYTES, MAX_OPERATIONS, PresetEdit,
};

impl Document {
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
            candidate.oib_validate_structure(batch)?;
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
                candidate.oib_stage_definitions(&definitions)?;
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
                candidate.oib_stage_presets(&presets)?;
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
                candidate.oib_hierarchy_stage(&hierarchy)?;
            }
            if let [DocumentEdit::Restore { defaults }] = batch.operations.as_slice() {
                if *defaults {
                    let signed = candidate.oib_binding()?;
                    let (mut restored, desired) =
                        reset::restore(&signed.seed, &candidate.oib_source()?)?;
                    restored.instances = desired.iter().map(Into::into).collect();
                    let mut edits = vec![candidate.oib_source_edit(|source| *source = restored)?];
                    for allocation in desired {
                        edits.push(Edit {
                            path: cell_path(5, allocation.id),
                            schema: candidate.oib_instance_schema(allocation.id),
                            value: Some(bytes(&true)?),
                        });
                    }
                    candidate.change(&[], edits)?;
                } else {
                    candidate.oib_restore_residents(false, writer)?;
                }
            }
            let reset_defaults = matches!(
                batch.operations.as_slice(),
                [DocumentEdit::Restore { defaults: true }]
            );
            if candidate.tree.hash() != original || reset_defaults {
                candidate.oib_prepare_final_structure(reset_defaults, writer)?;
            }
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
                    claim(&mut next, definition.0)?;
                    claim(&mut next, root.id.0)?;
                }
                DefinitionEdit::CreateNode { node, .. } => claim(&mut next, node.id.0)?,
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
                            claim(&mut next, slot.id.0)?;
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
                    claim(&mut next, preset.id)?;
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
