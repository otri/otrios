// SPDX-License-Identifier: LGPL-3.0-only
//! Local scrolling dialog capture, independent of any document or command.
use super::*;
use oib_workspace::{MODAL_CONTENT, Modal};

pub(super) struct Pointer {
    id: u32,
    action: Option<u32>,
    start: [f32; 2],
    scroll: u16,
    content: bool,
    moved: bool,
}
#[derive(Default)]
pub(super) struct Update {
    pub(super) action: Option<u32>,
    /// Only scrolling needs a new presentation; capture alone has no paint.
    pub(super) changed: bool,
}

/// The owner retains one immutable dialog projection while a pointer is held.
/// Replacing its rows requires explicitly canceling this capture first.
pub(super) fn update(
    capture: &mut Option<Pointer>,
    event: PointerEvent,
    scene: &Scene,
    modal: &Modal,
    scroll: &mut u16,
) -> Result<Update> {
    let hit = scene.hit_path(event.position);
    let action = hit.iter().find_map(|node| {
        let id = node.node.id;
        (modal.actions.iter().any(|a| a.id == id && a.enabled)
            || modal.links.iter().any(|link| link.id == id))
        .then_some(id)
    });
    let content = hit.iter().any(|node| node.node.id == MODAL_CONTENT);
    let limit = f32::from(oib_workspace::modal_scroll_limit(
        modal,
        scene.root().frame.width,
        scene.root().frame.height,
    )?);
    let mut result = Update::default();
    match event.phase {
        PointerPhase::Down => {
            if capture.is_none() {
                *capture = Some(Pointer {
                    id: event.id,
                    action,
                    start: event.position,
                    scroll: *scroll,
                    content,
                    moved: false,
                });
            }
        }
        PointerPhase::Move => {
            if let Some(pointer) = capture
                && pointer.id == event.id
            {
                let delta = event.position[1] - pointer.start[1];
                pointer.moved |=
                    (event.position[0] - pointer.start[0]).abs() > 8. || delta.abs() > 8.;
                if pointer.content && pointer.moved {
                    let next = (f32::from(pointer.scroll) - delta).round().clamp(0., limit) as u16;
                    result.changed = next != *scroll;
                    *scroll = next;
                }
            }
        }
        PointerPhase::Up | PointerPhase::Cancel => {
            if capture
                .as_ref()
                .is_some_and(|pointer| pointer.id == event.id)
            {
                let pointer = capture.take().unwrap();
                if event.phase == PointerPhase::Up && !pointer.moved && pointer.action == action {
                    result.action = action;
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "oib_dialog_tests.rs"]
mod tests;
