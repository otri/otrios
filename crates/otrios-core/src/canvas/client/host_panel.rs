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
            .is_none_or(|m| m.panel_scroll.is_none() || m.designer.is_some())
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
    if root.id == id {
        let size = [root.frame.width, root.frame.height];
        for child in &mut root.children {
            crate::dashboard::scene::clip_controls(child, [0., 0.], size);
        }
    } else {
        for child in &mut root.children {
            present_panel(child, id);
        }
    }
}

impl Client {
    pub(super) fn oib_preview_pointer(
        &mut self,
        epoch: u32,
        event: PointerEvent,
        at: u64,
    ) -> Result<Option<bool>> {
        let Some(m) = self.active.as_ref() else {
            return Ok(None);
        };
        if m.preview_scroll.is_none() {
            return Ok(None);
        }
        if epoch != m.epoch {
            return Ok(Some(false));
        }
        if event.phase != PointerPhase::Down
            && !m.preview_scroll.as_ref().is_some_and(|s| s.touching())
        {
            return Ok(None);
        }
        if event.phase == PointerPhase::Down {
            // Slider gestures keep their target. Panning starts on the preview's
            // noninteractive background, never on a sibling panel or modal.
            if self.host.drag.is_some() || m.input.active() {
                return Ok(None);
            }
            let scene = self.scene()?;
            let hit = scene.hit_path(event.position);
            let viewport = m.preview_scroll.as_ref().ok_or("Missing preview")?.node();
            let path = scene.path(viewport).ok_or("Missing preview frame")?;
            let parent = path.iter().rev().nth(1).map(|p| p.node.id);
            // Unpainted viewport space hits the card painted behind it, rather
            // than the viewport itself. Admit that background through its
            // containing card; Scroll::pointer still checks viewport bounds.
            // A sibling overlay or modal keeps its own hit path/barrier.
            if !hit.is_empty()
                && !hit
                    .iter()
                    .any(|p| p.node.id == viewport || Some(p.node.id) == parent)
            {
                return Ok(None);
            }
            if hit.iter().any(|p| {
                p.node.responder.is_some_and(|r| {
                    matches!(
                        r,
                        TouchResponder::Drag(_)
                            | TouchResponder::Activate(_)
                            | TouchResponder::Consume
                    )
                })
            }) {
                return Ok(None);
            }
        }
        let m = self.active.as_mut().ok_or("Missing Canvas")?;
        let scene = cached_layout(m)?;
        let changed = m
            .preview_scroll
            .as_mut()
            .ok_or("Missing preview")?
            .pointer(&scene, event, at)?;
        if changed.is_some() {
            m.invalidate_presentation();
        }
        Ok(changed)
    }
}

impl Client {
    /// Route a wheel delta to the viewport under the pointer; keyboard commands
    /// retain their existing inspector/map destination.
    pub fn scroll_at(&mut self, epoch: u32, delta: f32, position: [f32; 2]) -> Result<bool> {
        if epoch != self.epoch()
            || !delta.is_finite()
            || position.iter().any(|v| !v.is_finite())
            || self.input_busy()
        {
            return Ok(false);
        }
        if !self.host_active
            && let Some(changed) = self.history_review_scroll(position, delta)?
        {
            return Ok(changed);
        }
        if self.active.as_ref().is_some_and(|m| m.leave.is_some()) {
            return Ok(false);
        }
        if !self.host_active
            && let Some(changed) = self.designer_scroll_at(delta, position)?
        {
            return Ok(changed);
        }
        if !self.host_active
            && let Some(m) = &mut self.active
        {
            m.sync_panel_scroll()?;
            let scene = cached_layout(m)?;
            if let Some(scroll) = &mut m.preview_scroll {
                let path = scene.path(scroll.node()).ok_or("Missing OIB preview")?;
                let p = path.last().ok_or("Missing preview frame")?;
                let x = (position[0] - p.origin[0]) / p.scale;
                let y = (position[1] - p.origin[1]) / p.scale;
                if x >= 0. && y >= 0. && x < p.node.frame.width && y < p.node.frame.height {
                    let before = scroll.info().offset;
                    scroll.set_offset([0., before[1] + delta]);
                    let changed = before != scroll.info().offset;
                    if changed {
                        m.invalidate_presentation();
                    }
                    return Ok(changed);
                }
            }
        }
        self.scroll_map(epoch, delta)
    }
}
