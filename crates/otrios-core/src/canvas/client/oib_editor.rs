// SPDX-License-Identifier: LGPL-3.0-only
//! Designer working source and selected-definition presentation. The live adapter
//! remains the sole transport/receipt owner for the entire mount lifetime.
use super::*;
use otrios_interfaces::oib::{
    authoring::EditBatch,
    designer::{DefinitionId, PropertyAddress, Selection},
};
pub(super) struct DefinitionPresentation {
    pub(super) preview: super::super::oib::graph::DefinitionPreview,
    pub(super) samples: StateAdapter,
    pub(super) testing: bool,
}
impl Mount {
    pub(super) fn design_doc(&self) -> &StateAdapter {
        self.working_copy
            .as_ref()
            .map_or(&self.doc, |working| &working.candidate)
    }
    pub(super) fn sync_working_copy(&mut self) -> Result<bool> {
        if self.designer.is_none() {
            self.working_copy = None;
            self.definition_presentation = None;
            self.working_presented = None;
            return Ok(false);
        };
        if self.working_copy.is_none() {
            self.working_copy = Some(oib_working_copy::WorkingCopy::new(&self.doc)?);
            self.working_presented = None;
            return Ok(true);
        }
        let fields_dirty = self
            .designer
            .as_ref()
            .is_some_and(|d| d.drafts.drafts().iter().any(|f| f.dirty));
        // Raw input is already local intent: keep its observed source pinned
        // even before a valid field has entered the command log.
        if fields_dirty && !self.working_copy.as_ref().unwrap().applying() {
            return Ok(false);
        };
        let changed = self.working_copy.as_mut().unwrap().synchronize(&self.doc)?;
        if changed {
            self.working_presented = None;
            if let Some(d) = &mut self.designer {
                d.revision = None;
                d.graph_tools = None;
            }
        }
        Ok(changed)
    }
    pub(super) fn stage_structure(&mut self, operation: EditBatch) -> Result<bool> {
        self.sync_working_copy()?;
        self.remember_edit_location();
        let changed = self
            .working_copy
            .as_mut()
            .ok_or("Missing working copy")?
            .stage_structure(operation)?;
        if changed {
            self.working_changed()?;
        }
        Ok(changed)
    }
    pub(super) fn stage_property(
        &mut self,
        address: &PropertyAddress,
        value: Option<Value>,
    ) -> Result<bool> {
        self.sync_working_copy()?;
        self.remember_edit_location();
        let changed = self
            .working_copy
            .as_mut()
            .ok_or("Missing working copy")?
            .stage_property(address, value)?;
        if changed {
            self.working_changed()?;
        }
        Ok(changed)
    }
    pub(super) fn remember_edit_location(&mut self) {
        if let Some(d) = &self.designer
            && let Some(w) = &mut self.working_copy
        {
            w.remember_location(d.selection.clone(), d.workspace_task());
        }
    }
    pub(super) fn working_changed(&mut self) -> Result<()> {
        if let Some(d) = &mut self.designer {
            d.revision = None;
            d.graph_tools = None;
        }
        self.working_presented = None;
        self.invalidate();
        self.refresh()?;
        Ok(())
    }
    pub(super) fn editor_chrome(&self) -> oib_workspace::EditorChrome {
        let editable =
            self.designer_editing() && self.binding.as_ref().is_none_or(|b| b.ui_editable);
        let working = self.working_copy.as_ref();
        let busy = working.is_some_and(|w| w.applying());
        let fields_invalid = self
            .designer
            .as_ref()
            .is_some_and(|d| d.drafts.drafts().iter().any(|f| f.error.is_some()));
        let fields_dirty = self
            .designer
            .as_ref()
            .is_some_and(|d| d.drafts.drafts().iter().any(|f| f.dirty));
        oib_workspace::EditorChrome {
            component_name: self
                .designer
                .as_ref()
                .and_then(|d| d.component_name())
                .map(str::to_owned),
            management: self
                .designer
                .as_ref()
                .is_some_and(|d| d.management_visible()),
            new_component: editable && !busy,
            undo: editable && !busy && working.is_some_and(|w| w.can_undo()),
            redo: editable && !busy && working.is_some_and(|w| w.can_redo()),
            discard: !busy
                && (fields_dirty || working.is_some_and(|w| w.dirty() || w.error.is_some())),
            apply: editable
                && !busy
                && !fields_invalid
                && self.doc.confirmed()
                && working.is_some_and(|w| w.error.is_none() && (w.dirty() || fields_dirty)),
            modal: self
                .leave_modal()
                .or_else(|| self.delete_modal())
                .or_else(|| self.history_review_modal()),
        }
    }
    pub(super) fn selected_definition(&self) -> Option<DefinitionId> {
        match &self.designer.as_ref()?.selection {
            Selection::Definition(id) | Selection::DefinitionNode { definition: id, .. } => {
                Some(*id)
            }
            _ => None,
        }
    }
    pub(super) fn editor_status(&self) -> Option<String> {
        let w = self.working_copy.as_ref()?;
        Some(if w.applying() {
            "Applying changes".into()
        } else if let Some(error) = &w.error {
            error.clone()
        } else if w.stale(&self.doc) && w.dirty() {
            "Design changed; review local edits".into()
        } else if w.dirty() {
            "Unapplied changes".into()
        } else if self.doc.oib_status_text().contains("rejected:") {
            self.doc.oib_status_text()
        } else {
            "All changes applied".into()
        })
    }
    pub(super) fn sync_oib_style_mode(&mut self) {
        let mode = match self.designer.as_ref().map(|designer| designer.appearance()) {
            Some(oib_workspace::Appearance::Light) => otrios_interfaces::oib::styles::Mode::Light,
            _ => otrios_interfaces::oib::styles::Mode::Dark,
        };
        self.doc.set_oib_style_mode(mode);
        if let Some(working) = &mut self.working_copy {
            working.base.set_oib_style_mode(mode);
            working.candidate.set_oib_style_mode(mode);
        }
        if let Some(preview) = &mut self.definition_presentation {
            preview.samples.set_oib_style_mode(mode);
        }
        if let Some(preview) = &mut self.designer_preview {
            preview.doc.set_oib_style_mode(mode);
        }
    }
    pub(super) fn refresh_definition_presentation(&mut self) -> Result<bool> {
        // Ordinary apps have no OIB authoring subtree or definition preview.
        if super::super::oib::seed(&self.ui).is_none() {
            return Ok(false);
        }
        let definition = self.selected_definition();
        let has_styles =
            super::super::oib::styles::has_styles(&self.design_doc().oib_authoring_seed()?);
        let style_mode = has_styles.then_some(self.design_doc().oib_style_mode());
        let old = self
            .definition_presentation
            .as_ref()
            .map(|p| p.preview.definition);
        let revision = self.working_copy.as_ref().map(|w| w.revision);
        if definition == old
            && revision == self.working_presented
            && style_mode == self.oib_style_mode_presented
        {
            return Ok(false);
        };
        if self.input.active()
            || self.keyboard_pointer.is_some()
            || self
                .designer
                .as_ref()
                .is_some_and(oib_designer::Designer::busy)
        {
            return Ok(false);
        };
        let (mut ui, mut layout) = self.design_doc().oib_compiled(self.oib_selected)?;
        let mut presentation = None;
        if let Some(definition) = definition {
            let seed = self.design_doc().oib_authoring_seed()?;
            let active = self.design_doc().oib_active()?;
            let mut values = BTreeMap::new();
            for (cell, value) in active {
                if let Ok(address) = super::super::oib::document::address_for_cell(&seed, cell)
                    && matches!(address, PropertyAddress::Definition { .. })
                {
                    values.insert(address, value);
                }
            }
            let preview = super::super::oib::graph::definition_preview_with_values_mode(
                &ui,
                definition,
                [320., 240.],
                &values,
                self.design_doc().oib_style_mode(),
            )?;
            let samples = preview.sample_adapter()?;
            ui = preview.ui.clone();
            layout = preview.layout.clone();
            presentation = Some(DefinitionPresentation {
                preview,
                samples,
                testing: self.designer_testing(),
            });
        }
        // Qualification may already hold the exact execution set for this app
        // composition. Reusing it avoids a third set beside active+qualified
        // mounts, and retains the locals resumed by atomic code activation.
        // Dedicated definition previews still reset locals with their samples.
        let composition = definition.is_none() && old.is_none();
        let components = if composition && self.components.matches(&ui, &layout)? {
            None
        } else {
            Some(super::super::oib::component_behaviors::Instances::prepare(
                &ui,
                &layout,
                &self.component_runtime,
                composition.then_some(&self.components),
            )?)
        };
        let sample = presentation
            .as_ref()
            .map_or(self.design_doc(), |p| &p.samples);
        components
            .as_ref()
            .unwrap_or(&self.components)
            .validate_current(&ui, sample)?;
        self.ui = ui;
        self.layout = layout;
        if let Some(components) = components {
            self.components = components;
        }
        self.definition_presentation = presentation;
        self.working_presented = revision;
        self.oib_style_mode_presented = style_mode;
        if let Some(seed) = super::super::oib::seed(&self.ui)
            && let Some(surface) = &seed.instance_surface
        {
            self.preview_scroll = Some(super::super::text_scroll::Scroll::new(
                surface.preview_viewport,
                super::super::oib::document::placement::preview_content(seed),
            ));
        }
        self.invalidate();
        Ok(true)
    }
}
