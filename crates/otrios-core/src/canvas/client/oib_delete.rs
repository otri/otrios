// SPDX-License-Identifier: LGPL-3.0-only
//! Document-scoped deletion review; confirmation stages one private candidate.
use super::*;
use crate::canvas::oib::{
    endpoint_impact,
    impact::{self, Kind},
    instance_impact,
};
use oib_workspace::{
    MODAL_ACTION_START, MODAL_CONTENT, MODAL_LINK_START, Modal, ModalAction, ModalLink,
};
use otrios_interfaces::oib::{
    authoring::*,
    designer::{DefinitionId, InstanceId, PropertyId, Selection},
};
use std::ops::Range;

const CLOSE: u32 = MODAL_ACTION_START + 5;
const DELETE: u32 = MODAL_ACTION_START + 6;
const REFRESH: u32 = MODAL_ACTION_START + 7;
const PREVIOUS: u32 = MODAL_ACTION_START + 8;
const NEXT: u32 = MODAL_ACTION_START + 9;

#[derive(Clone, Copy)]
enum Target {
    Definition(DefinitionId),
    Instance(InstanceId),
    Property(DefinitionId, PropertyId),
}
struct Report {
    target: Target,
    name: String,
    scope: &'static str,
    complete: bool,
    readonly: bool,
    items: Vec<impact::Use>,
    blockers: Vec<String>,
    removable_presets: Vec<u64>,
    removal_order: Vec<InstanceId>,
    removable: bool,
}
impl Report {
    fn can_delete(&self) -> bool {
        self.removable
    }
    fn instance(&self) -> bool {
        matches!(self.target, Target::Instance(_))
    }
    fn property(&self) -> bool {
        matches!(self.target, Target::Property(..))
    }
}
fn query(state: &StateAdapter, target: Target) -> Result<Report> {
    let source = state.oib_history_source()?;
    let seed = state.oib_authoring_seed()?;
    Ok(match target {
        Target::Property(definition, property) => {
            let r = endpoint_impact::endpoint(&seed, &source, definition, property);
            let removable = r.can_remove();
            Report {
                target: Target::Property(r.target, r.property),
                removable,
                name: r.name,
                scope: r.scope,
                complete: r.complete,
                readonly: r.readonly,
                items: r.items,
                blockers: r.blockers,
                removable_presets: vec![],
                removal_order: vec![],
            }
        }
        Target::Definition(id) => {
            let r = impact::component(&seed, &source, id);
            let removable = r.can_delete();
            Report {
                target: Target::Definition(r.target),
                removable,
                name: r.name,
                scope: r.scope,
                complete: r.complete,
                readonly: r.readonly,
                items: r.items,
                blockers: r.blockers,
                removable_presets: r.removable_presets,
                removal_order: vec![],
            }
        }
        Target::Instance(id) => {
            let r = instance_impact::instance(&seed, &source, id);
            let removable = r.can_remove();
            Report {
                target: Target::Instance(r.target),
                removable,
                name: r.name,
                scope: r.scope,
                complete: r.complete,
                readonly: r.readonly,
                items: r.items,
                blockers: r.blockers,
                removable_presets: vec![],
                removal_order: r.removal_order,
            }
        }
    })
}

pub(super) struct Pending {
    report: Report,
    live_root: Option<crate::merkle::Hash>,
    candidate_root: Option<crate::merkle::Hash>,
    can_stage: bool,
    error: Option<String>,
    scroll: u16,
    page: usize,
    pages: Vec<Range<usize>>,
    pointer: Option<oib_dialog::Pointer>,
}
fn group(kind: Kind) -> &'static str {
    match kind {
        Kind::Definition => "Components",
        Kind::Instance => "Instances",
        Kind::Preset => "Presets",
        Kind::Api => "API and slots",
        Kind::Export => "Exports",
        Kind::Ownership => "Ownership",
        Kind::Unknown => "Unresolved dependencies",
    }
}
impl Pending {
    fn current(&self, mount: &Mount) -> bool {
        self.live_root == mount.doc.oib_root()
            && self.candidate_root == mount.design_doc().oib_root()
    }
    fn modal_for(&self, range: Range<usize>, current: bool, page: Option<(usize, usize)>) -> Modal {
        let mut lines = vec![
            self.report.name.clone(),
            self.report.scope.into(),
            if current {
                ""
            } else {
                "Sources changed; check dependencies again before deleting."
            }
            .into(),
            if !self.report.complete {
                "Cannot delete: some dependencies could not be checked.".into()
            } else if self.report.readonly {
                if self.report.property() {
                    "This property belongs to an inherited class or imported component."
                } else if self.report.instance() {
                    "This instance is fixed by the host."
                } else {
                    "This component belongs to an imported library."
                }
                .into()
            } else if self.report.can_delete() {
                if self.report.property() { "Removes this property, its authored fallback and outgoing forwarding routes. The component and linked controls remain." } else if self.report.instance() { "Removes this instance, its owned children and their connections. The component remains in the library." } else { "Deletion removes this component and the listed palette presets." }.into()
            } else {
                if self.report.property() {
                    "Cannot remove while in use. Open a use to inspect it."
                } else {
                    "Cannot delete while in use. Open a use to inspect it."
                }
                .into()
            },
        ];
        if let Some(error) = &self.error {
            lines.push(error.clone());
        }
        for blocker in &self.report.blockers {
            if !self
                .report
                .items
                .iter()
                .any(|item| item.kind == Kind::Unknown && item.detail == *blocker)
            {
                lines.push(blocker.clone());
            }
        }
        if let Some((index, count)) = page {
            lines.push(format!("Dependencies: page {} of {count}", index + 1));
        }
        let mut links = vec![];
        let mut previous = None;
        for index in range {
            let item = &self.report.items[index];
            if previous != Some(item.kind) {
                lines.push(group(item.kind).into());
                previous = Some(item.kind);
            }
            let line = lines.len();
            lines.push(format!(
                "{}: {}{}",
                item.label,
                item.detail,
                if item.direct
                    || matches!(item.kind, Kind::Unknown | Kind::Ownership | Kind::Export)
                {
                    ""
                } else {
                    " (indirect)"
                }
            ));
            if item.selection.is_some() {
                links.push(ModalLink {
                    line,
                    id: MODAL_LINK_START + links.len() as u32,
                });
            }
        }
        if self.report.items.is_empty() && self.report.complete {
            lines.push("No uses found within this document.".into());
        }
        let mut actions = vec![
            ModalAction {
                id: CLOSE,
                label: if self.report.property() {
                    "Keep property"
                } else if self.report.instance() {
                    "Keep instance"
                } else {
                    "Keep component"
                }
                .into(),
                enabled: true,
            },
            ModalAction {
                id: REFRESH,
                label: "Check again".into(),
                enabled: true,
            },
            ModalAction {
                id: DELETE,
                label: if self.report.property() {
                    "Remove property"
                } else if self.report.instance() {
                    "Remove instance"
                } else {
                    "Delete component"
                }
                .into(),
                enabled: current && self.can_stage,
            },
        ];
        if let Some((index, count)) = page {
            actions.push(ModalAction {
                id: PREVIOUS,
                label: "Previous uses".into(),
                enabled: index > 0,
            });
            actions.push(ModalAction {
                id: NEXT,
                label: "Next uses".into(),
                enabled: index + 1 < count,
            });
        }
        Modal {
            title: if self.report.property() {
                "Remove property?"
            } else if self.report.instance() {
                "Remove instance?"
            } else {
                "Delete component?"
            }
            .into(),
            lines,
            links,
            actions,
            scroll: Some(self.scroll),
        }
    }
    fn modal(&self, mount: &Mount) -> Modal {
        self.modal_for(
            self.pages[self.page].clone(),
            self.current(mount),
            (self.pages.len() > 1).then_some((self.page, self.pages.len())),
        )
    }
    fn link_selection(&self, id: u32) -> Option<Selection> {
        let index = id.checked_sub(MODAL_LINK_START)? as usize;
        self.pages[self.page]
            .clone()
            .filter_map(|index| self.report.items[index].selection.clone())
            .nth(index)
    }
    fn paginate(&mut self) -> Result<()> {
        self.pages.clear();
        self.page = 0;
        self.scroll = 0;
        self.pointer = None;
        let count = self.report.items.len();
        let full = self.modal_for(0..count, true, None);
        if oib_workspace::modal_content_height(&full, 800.)? <= 8192. && full.links.len() <= 256 {
            self.pages.push(0..count);
            return Ok(());
        }
        // Paging is used only when the full admitted list exceeds the existing
        // coordinate/link envelope. Reserve one line for the page indicator.
        let mut start = 0;
        while start < count {
            let mut end = start;
            while end < count {
                let modal = self.modal_for(start..end + 1, true, Some((0, count)));
                if modal.links.len() > 256
                    || oib_workspace::modal_content_height(&modal, 800.)? > 8192.
                {
                    break;
                }
                end += 1;
            }
            if end == start {
                return Err("A dependency row exceeds the Designer dialog budget".into());
            }
            self.pages.push(start..end);
            start = end;
        }
        Ok(())
    }
}
fn candidate(state: &StateAdapter, report: &Report) -> Result<EditBatch> {
    if !report.can_delete() {
        return Err("Component dependencies must be repaired before deletion".into());
    }
    let mut apply = state.oib_apply_base()?;
    match report.target {
        Target::Property(definition, property) => {
            apply
                .operations
                .push(DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
                    definition,
                    property,
                }));
        }
        Target::Definition(definition) => {
            apply
                .operations
                .push(DocumentEdit::Definition(DefinitionEdit::DeleteDefinition {
                    definition,
                }));
            apply.operations.extend(
                report
                    .removable_presets
                    .iter()
                    .map(|id| DocumentEdit::Preset(PresetEdit::Delete { preset: *id })),
            );
        }
        Target::Instance(_) => {
            for instance in &report.removal_order {
                apply.operations.extend(
                    otrios_interfaces::oib::InstanceOperation::remove(apply.generation, instance.0)
                        .operations,
                );
            }
        }
    }
    // Run the same final authority admission locally. No app/transport write.
    state.preview_oib_apply(&apply)?;
    Ok(EditBatch {
        generation: apply.generation,
        operations: apply.operations,
    })
}
impl Mount {
    pub(super) fn delete_modal(&self) -> Option<Modal> {
        self.delete_review.as_ref().map(|review| review.modal(self))
    }
    pub(super) fn delete_pointer_active(&self) -> bool {
        self.delete_review
            .as_ref()
            .is_some_and(|r| r.pointer.is_some())
    }
}
impl Client {
    pub(super) fn designer_review_delete(&mut self, definition: DefinitionId) -> Result<bool> {
        self.review_delete(Target::Definition(definition))
    }
    pub(super) fn designer_review_instance_delete(&mut self, instance: InstanceId) -> Result<bool> {
        self.review_delete(Target::Instance(instance))
    }
    pub(super) fn designer_review_property_delete(
        &mut self,
        definition: DefinitionId,
        property: PropertyId,
    ) -> Result<bool> {
        self.review_delete(Target::Property(definition, property))
    }
    fn review_delete(&mut self, target: Target) -> Result<bool> {
        let Some(m) = self.active.as_mut() else {
            return Ok(false);
        };
        if m.local_modal_open() || m.designer.is_none() {
            return Ok(false);
        }
        m.sync_working_copy()?;
        let report = query(m.design_doc(), target)?;
        let writable = m.binding.as_ref().is_none_or(|b| b.ui_editable)
            && m.designer_editing()
            && m.doc.confirmed()
            && m.working_copy.as_ref().is_none_or(|w| !w.applying());
        let stale = m.working_copy.as_ref().is_some_and(|w| w.stale(&m.doc));
        let (can_stage, error) = if stale {
            (
                false,
                Some(
                    "Local changes use older source. Review or discard them before deleting."
                        .into(),
                ),
            )
        } else if !writable {
            (
                false,
                Some("This editor is read-only or is waiting for confirmation".into()),
            )
        } else if report.can_delete() {
            match candidate(m.design_doc(), &report) {
                Ok(_) => (true, None),
                Err(reason) => (false, Some(reason)),
            }
        } else {
            (false, None)
        };
        let mut review = Pending {
            report,
            live_root: m.doc.oib_root(),
            candidate_root: m.design_doc().oib_root(),
            can_stage,
            error,
            scroll: 0,
            page: 0,
            pages: vec![],
            pointer: None,
        };
        review.paginate()?;
        m.show_history_review(); // shared capture/keyboard cancellation only
        m.delete_review = Some(review);
        m.invalidate();
        Ok(true)
    }
    pub(super) fn delete_review_activate(&mut self, id: u32) -> Result<Option<bool>> {
        let Some(m) = self.active.as_ref() else {
            return Ok(None);
        };
        let Some(review) = m.delete_review.as_ref() else {
            return Ok(None);
        };
        let modal = review.modal(m);
        if modal.links.iter().any(|link| link.id == id) {
            let selection = review.link_selection(id);
            let current = review.current(m);
            if !current {
                return Ok(Some(false));
            }
            if let Some(selection) = selection {
                self.active.as_mut().unwrap().delete_review = None;
                self.inspect_oib_selection(selection)?;
                return Ok(Some(true));
            }
        }
        if !modal
            .actions
            .iter()
            .any(|action| action.id == id && action.enabled)
        {
            return Ok(Some(false));
        }
        let target = review.report.target;
        match id {
            CLOSE => {
                let m = self.active.as_mut().unwrap();
                m.delete_review = None;
                m.invalidate();
            }
            REFRESH => {
                let m = self.active.as_mut().unwrap();
                m.delete_review = None;
                self.review_delete(target)?;
            }
            PREVIOUS | NEXT => {
                let m = self.active.as_mut().unwrap();
                let review = m.delete_review.as_mut().unwrap();
                if id == PREVIOUS {
                    review.page -= 1;
                } else {
                    review.page += 1;
                }
                review.scroll = 0;
                review.pointer = None;
                m.invalidate_presentation();
            }
            DELETE => {
                // Recompute from today's exact source, not an executable command
                // retained by an old dependency screen.
                let m = self.active.as_mut().unwrap();
                let review = m.delete_review.take().unwrap();
                let result = (|| {
                    if !review.current(m) {
                        return Err("Sources changed; check dependencies again".into());
                    }
                    if !m.designer_editing()
                        || m.binding.as_ref().is_some_and(|b| !b.ui_editable)
                        || !m.doc.confirmed()
                        || m.working_copy.as_ref().is_some_and(|w| w.applying())
                    {
                        return Err(
                            "This editor is read-only or is waiting for confirmation".into()
                        );
                    }
                    if m.working_copy.as_ref().is_some_and(|w| w.stale(&m.doc)) {
                        return Err("Local changes use older source. Review or discard them before deleting.".into());
                    }
                    let state = m.design_doc();
                    let report = query(state, target)?;
                    let changed = m.stage_structure(candidate(state, &report)?)?;
                    if changed {
                        match report.target {
                            Target::Instance(_) => {
                                m.discard_removed_occurrence_drafts(&report.removal_order)?
                            }
                            Target::Definition(id) => m.discard_removed_definition_drafts(id)?,
                            Target::Property(definition, property) => {
                                m.discard_removed_property_drafts(definition, property)?
                            }
                        }
                    }
                    Ok::<_, String>(changed)
                })();
                match result {
                    Ok(_) => {
                        m.invalidate();
                    }
                    Err(reason) => {
                        let mut review = review;
                        review.can_stage = false;
                        review.error = Some(reason);
                        review.pointer = None;
                        m.delete_review = Some(review);
                        m.invalidate();
                    }
                }
            }
            _ => return Ok(Some(false)),
        }
        Ok(Some(true))
    }
    pub(super) fn delete_review_pointer(&mut self, event: PointerEvent) -> Result<Option<bool>> {
        if self
            .active
            .as_ref()
            .is_none_or(|m| m.delete_review.is_none())
        {
            return Ok(None);
        }
        let scene = self.scene()?;
        let modal = self.active.as_ref().unwrap().delete_modal().unwrap();
        let m = self.active.as_mut().unwrap();
        let review = m.delete_review.as_mut().unwrap();
        let update = oib_dialog::update(
            &mut review.pointer,
            event,
            &scene,
            &modal,
            &mut review.scroll,
        )?;
        if update.changed {
            m.invalidate_presentation();
        }
        if let Some(id) = update.action {
            self.delete_review_activate(id)?;
        }
        Ok(Some(true))
    }
    pub(super) fn delete_review_scroll(
        &mut self,
        point: [f32; 2],
        delta: f32,
    ) -> Result<Option<bool>> {
        if self
            .active
            .as_ref()
            .is_none_or(|m| m.delete_review.is_none())
        {
            return Ok(None);
        }
        let scene = self.scene()?;
        if scene
            .hit_path(point)
            .iter()
            .any(|node| node.node.id == MODAL_CONTENT)
        {
            let modal = self.active.as_ref().unwrap().delete_modal().unwrap();
            let limit = oib_workspace::modal_scroll_limit(
                &modal,
                scene.root().frame.width,
                scene.root().frame.height,
            )?;
            let m = self.active.as_mut().unwrap();
            let review = m.delete_review.as_mut().unwrap();
            review.scroll = (f32::from(review.scroll) + delta)
                .round()
                .clamp(0., f32::from(limit)) as u16;
            m.invalidate_presentation();
        }
        Ok(Some(true))
    }
}

#[cfg(test)]
#[path = "oib_delete_tests.rs"]
mod tests;
