// SPDX-License-Identifier: LGPL-3.0-only
//! Atomic local style edits share the canonical source leaf and identity cursor.
use super::*;
use otrios_interfaces::oib::{authoring::DocumentSource, styles::*};

impl Document {
    pub(super) fn oib_stage_styles(&mut self, operations: &[StyleEdit]) -> Result<()> {
        let seed = self.oib_binding()?.seed.clone();
        let before = self.oib_source()?;
        let mut source = before.clone();
        for operation in operations {
            let mut operation = operation.clone();
            match &mut operation {
                StyleEdit::CreateStyle { id, .. } | StyleEdit::CreateVariable { id, .. } => {
                    self.oib_claim_explicit(&mut source.next_identity, *id)?
                }
                StyleEdit::Replace {
                    styles: Some(restored),
                } => {
                    let existing = source.clone();
                    self.oib_remap_style_restoration(
                        &existing,
                        restored,
                        &mut source.next_identity,
                    )?;
                }
                _ => {}
            }
            crate::canvas::oib::styles::edit_local(&seed, &mut source, &operation)?;
        }
        if bytes(&source)? != bytes(&before)? {
            let edit = self.oib_source_edit(|current| *current = source)?;
            self.change(&[], vec![edit])?;
        }
        Ok(())
    }
    fn oib_remap_style_restoration(
        &mut self,
        source: &DocumentSource,
        restored: &mut DocumentStyles,
        next: &mut u64,
    ) -> Result<()> {
        let current = source
            .styles
            .as_ref()
            .and_then(|styles| styles.catalog.systems.first());
        let mut styles = std::collections::BTreeMap::new();
        let mut variables = std::collections::BTreeMap::new();
        let mut contracts = std::collections::BTreeMap::new();
        for system in &restored.catalog.systems {
            for style in &system.styles {
                if !current.is_some_and(|current| {
                    current
                        .styles
                        .iter()
                        .any(|existing| existing.id == style.id)
                }) {
                    let id = self.oib_allocating_id()?;
                    self.oib_claim_implicit(id)?;
                    *next = (*next).max(id.checked_add(1).ok_or("Style identity exhausted")?);
                    styles.insert(style.id, StyleId(id));
                    if !current.is_some_and(|current| {
                        current
                            .contracts
                            .iter()
                            .any(|contract| contract.id == style.contract)
                    }) {
                        contracts.entry(style.contract).or_insert(ContractId(id));
                    }
                }
            }
            for variable in &system.variables {
                if !current.is_some_and(|current| {
                    current
                        .variables
                        .iter()
                        .any(|existing| existing.id == variable.id)
                }) {
                    let id = self.oib_allocating_id()?;
                    self.oib_claim_implicit(id)?;
                    *next = (*next).max(id.checked_add(1).ok_or("Variable identity exhausted")?);
                    variables.insert(variable.id, VariableId(id));
                }
            }
        }
        for system in &mut restored.catalog.systems {
            for contract in &mut system.contracts {
                if let Some(id) = contracts.get(&contract.id) {
                    contract.id = *id;
                }
            }
            for style in &mut system.styles {
                if let Some(id) = styles.get(&style.id) {
                    style.id = *id;
                }
                if let Some(id) = contracts.get(&style.contract) {
                    style.contract = *id;
                }
                for entry in style
                    .defaults
                    .iter_mut()
                    .chain(style.patches.iter_mut().flat_map(|patch| &mut patch.fields))
                {
                    if let FieldValue::Variable(reference) = &mut entry.value
                        && let Some(id) = variables.get(&reference.variable)
                    {
                        reference.variable = *id;
                    }
                }
            }
            for variable in &mut system.variables {
                if let Some(id) = variables.get(&variable.id) {
                    variable.id = *id;
                }
            }
            system.contracts.sort_by_key(|contract| contract.id);
            system.styles.sort_by_key(|style| style.id);
            system.variables.sort_by_key(|variable| variable.id);
        }
        for component in &mut restored.components {
            if let Some(id) = styles.get(&component.style.style) {
                component.style.style = *id;
            }
        }
        Ok(())
    }
}
