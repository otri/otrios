// SPDX-License-Identifier: LGPL-3.0-only
//! Shared local modal panning preserves the admitted mount and tap semantics.
use super::*;
use crate::canvas::host_ui::{Drag, DragKind};

impl Client {
    pub(super) fn prepare_panel_scroll(
        &self,
        mount: &mut Mount,
        viewport: Option<(u16, [f32; 2])>,
    ) -> Result<()> {
        if let Some((node, content)) = viewport {
            let scene = cached_layout(mount)?;
            let mut scroll = super::super::text_scroll::Scroll::new(node, content);
            scroll.sync(&scene)?;
            scroll.set_offset([0., self.host.panel_scroll]);
            mount.panel_scroll = Some(scroll);
        }
        Ok(())
    }
    pub(super) fn host_panel_pointer(
        &mut self,
        epoch: u32,
        event: PointerEvent,
        at: u64,
    ) -> Result<Option<bool>> {
        if self
            .active
            .as_ref()
            .is_none_or(|m| m.panel_scroll.is_none())
        {
            return Ok(None);
        }
        if epoch != self.epoch() {
            return Ok(Some(false));
        }
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.keyboard_pointer == Some(event.id))
        {
            return Ok(None);
        }
        if event.phase == PointerPhase::Down {
            if self.host.drag.is_some() {
                return Ok(Some(false));
            }
            let scene = self.scene()?;
            let mount = self.active.as_ref().ok_or("Missing panel")?;
            if mount.keyboard.visible
                && scene.hit_path(event.position).iter().rev().any(|node| {
                    let Some(TouchResponder::Activate(Action::AppEvent(control))) =
                        node.node.responder
                    else {
                        return false;
                    };
                    mount
                        .ui
                        .handlers
                        .iter()
                        .any(|h| h.control == control && matches!(h.operation, Operation::Key(_)))
                })
            {
                return Ok(None);
            }
            let control = scene
                .hit_path(event.position)
                .iter()
                .rev()
                .find(|p| matches!(p.node.responder, Some(TouchResponder::Activate(_))))
                .map(|p| p.node.id);
            let mount = self.active.as_mut().ok_or("Missing panel")?;
            let layout = cached_layout(mount)?;
            let scroll = mount.panel_scroll.as_mut().ok_or("Missing panel scroll")?;
            if scroll.pointer(&layout, event, at)?.is_none() {
                return Ok(None);
            }
            self.host.panel_control = control;
            self.host.drag = Some(Drag {
                pointer: event.id,
                epoch,
                generation: self.spaces.snapshot().map_or(0, |s| s.generation),
                kind: DragKind::Pan,
                start: event.position,
                position: event.position,
                moving: false,
                last_tick: at,
                action: None,
            });
            mount.invalidate_presentation();
            return Ok(Some(true));
        }
        let Some(mut drag) = self.host.drag.take() else {
            return Ok(None);
        };
        if drag.pointer != event.id {
            self.host.drag = Some(drag);
            return Ok(Some(false));
        }
        drag.moving |= (event.position[0] - drag.start[0])
            .abs()
            .max((event.position[1] - drag.start[1]).abs())
            > 8.;
        let mount = self.active.as_mut().ok_or("Missing panel")?;
        let scene = cached_layout(mount)?;
        let scroll = mount.panel_scroll.as_mut().ok_or("Missing panel scroll")?;
        scroll.pointer(&scene, event, at)?;
        self.host.panel_scroll = scroll.info().offset[1];
        let tap = event.phase == PointerPhase::Up && !drag.moving;
        if tap {
            scroll.cancel();
        }
        mount.invalidate_presentation();
        if matches!(event.phase, PointerPhase::Up | PointerPhase::Cancel) {
            let control = self.host.panel_control.take();
            if tap && let Some(control) = control {
                return self.activate(epoch, control).map(Some);
            }
        } else {
            self.host.drag = Some(drag);
        }
        Ok(Some(true))
    }
}

pub(super) fn present_panel(root: &mut SceneNode, id: u32) {
    fn clip(node: &mut SceneNode, origin: [f32; 2], size: [f32; 2]) {
        let at = [origin[0] + node.frame.x, origin[1] + node.frame.y];
        if node.semantics.is_some()
            && (at[0] < 0.
                || at[1] < 0.
                || at[0] + node.frame.width > size[0]
                || at[1] + node.frame.height > size[1])
        {
            node.semantics = None;
            node.responder = None;
        }
        for child in &mut node.children {
            clip(child, at, size);
        }
    }
    if root.id == id {
        let size = [root.frame.width, root.frame.height];
        for child in &mut root.children {
            clip(child, [0., 0.], size);
        }
    } else {
        for child in &mut root.children {
            present_panel(child, id);
        }
    }
}
