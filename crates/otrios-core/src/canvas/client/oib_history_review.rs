// SPDX-License-Identifier: LGPL-3.0-only
//! A source comparison is a local modal. It never refreshes captured guards or
//! offers an implicit overwrite of a later author's change.
use super::*;
use oib_applied_history::Direction;
use oib_workspace::{MODAL_ACTION_START, MODAL_CONTENT, Modal, ModalAction};
const CLOSE: u32 = MODAL_ACTION_START + 3;
const INSPECT: u32 = MODAL_ACTION_START + 4;
pub(super) type Pointer = super::oib_dialog::Pointer;
fn summary(value: &str) -> String {
    let count = value.chars().count();
    if count <= 160 {
        value.to_owned()
    } else {
        format!(
            "{}... ({count} characters)",
            value.chars().take(144).collect::<String>()
        )
    }
}
impl Mount {
    pub(super) fn local_modal_open(&self) -> bool {
        self.leave.is_some()
            || self.delete_review.is_some()
            || self
                .working_copy
                .as_ref()
                .is_some_and(|w| w.history_review().is_some())
    }
    pub(super) fn history_review_modal(&self) -> Option<Modal> {
        let review = self.working_copy.as_ref()?.history_review()?;
        let current = self
            .working_copy
            .as_ref()?
            .history_review_is_current(&self.doc);
        let mut lines = vec![
            review.reason.clone(),
            if current {
                "Current values are captured at review time."
            } else {
                "Sources changed again; Inspect loads latest values."
            }
            .into(),
        ];
        for conflict in &review.conflicts {
            lines.push(summary(&conflict.target));
            lines.push(format!("Before: {}", summary(&conflict.before_text)));
            lines.push(format!("Applied: {}", summary(&conflict.applied_text)));
            lines.push(format!("Current: {}", summary(&conflict.current_text)));
            lines.push(String::new());
        }
        Some(Modal {
            links: vec![],
            title: match review.direction {
                Direction::Undo => "Undo needs review",
                Direction::Redo => "Redo needs review",
            }
            .into(),
            lines,
            actions: vec![
                ModalAction {
                    id: CLOSE,
                    label: "Close".into(),
                    enabled: true,
                },
                ModalAction {
                    id: INSPECT,
                    label: "Inspect current".into(),
                    enabled: review.conflicts.iter().any(|c| c.selection.is_some()),
                },
            ],
            scroll: Some(review.scroll),
        })
    }
    pub(super) fn show_history_review(&mut self) {
        if let Some(d) = &mut self.designer {
            d.cancel();
        }
        self.input.cancel_all();
        self.captured_handlers.clear();
        if let Some(s) = &mut self.preview_scroll {
            s.cancel();
        }
        self.keyboard.cancel();
        self.keyboard.hide();
        self.keyboard_pointer = None;
        self.order_drag = None;
        self.review_pointer = None;
        self.invalidate();
    }
}
impl Client {
    pub(super) fn history_review_activate(&mut self, id: u32) -> Result<Option<bool>> {
        let Some(m) = &self.active else {
            return Ok(None);
        };
        let Some(review) = m.working_copy.as_ref().and_then(|w| w.history_review()) else {
            return Ok(None);
        };
        if ![CLOSE, INSPECT].contains(&id) {
            return Ok(Some(false));
        };
        let selection = if id == INSPECT {
            review.conflicts.iter().find_map(|c| c.selection.clone())
        } else {
            None
        };
        if id == INSPECT && selection.is_none() {
            return Ok(Some(false));
        };
        let m = self.active.as_mut().unwrap();
        m.working_copy.as_mut().unwrap().clear_history_review();
        m.review_pointer = None;
        m.invalidate();
        if let Some(selection) = selection {
            self.inspect_oib_selection(selection)?;
        }
        Ok(Some(true))
    }
    pub(super) fn history_review_pointer(&mut self, event: PointerEvent) -> Result<Option<bool>> {
        if self.active.as_ref().is_none_or(|m| {
            m.working_copy
                .as_ref()
                .is_none_or(|w| w.history_review().is_none())
        }) {
            return Ok(None);
        }
        let scene = self.scene()?;
        let modal = self
            .active
            .as_ref()
            .unwrap()
            .history_review_modal()
            .unwrap();
        let m = self.active.as_mut().unwrap();
        let review = m
            .working_copy
            .as_mut()
            .unwrap()
            .history_review_mut()
            .unwrap();
        let update = oib_dialog::update(
            &mut m.review_pointer,
            event,
            &scene,
            &modal,
            &mut review.scroll,
        )?;
        if update.changed {
            m.invalidate_presentation();
        }
        if let Some(id) = update.action {
            self.history_review_activate(id)?;
        }
        Ok(Some(true))
    }
    pub(super) fn history_review_scroll(
        &mut self,
        point: [f32; 2],
        delta: f32,
    ) -> Result<Option<bool>> {
        if self.active.as_ref().is_none_or(|m| {
            m.working_copy
                .as_ref()
                .is_none_or(|w| w.history_review().is_none())
        }) {
            return Ok(None);
        };
        if !delta.is_finite() {
            return Err("Invalid review scroll".into());
        };
        let scene = self.scene()?;
        if scene
            .hit_path(point)
            .iter()
            .any(|n| n.node.id == MODAL_CONTENT)
        {
            let modal = self
                .active
                .as_ref()
                .unwrap()
                .history_review_modal()
                .unwrap();
            let limit = f32::from(oib_workspace::modal_scroll_limit(
                &modal,
                scene.root().frame.width,
                scene.root().frame.height,
            )?);
            let m = self.active.as_mut().unwrap();
            let review = m
                .working_copy
                .as_mut()
                .unwrap()
                .history_review_mut()
                .unwrap();
            review.scroll = (f32::from(review.scroll) + delta).round().clamp(0., limit) as u16;
            m.invalidate_presentation();
        }
        Ok(Some(true))
    }
}

#[cfg(test)]
#[path = "oib_history_review_tests.rs"]
mod tests;
