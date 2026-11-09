// SPDX-License-Identifier: LGPL-3.0-only
//! Trusted map gestures never become publisher app events or document writes.
use super::*;
use crate::canvas::host_ui::{Drag, DragKind, HostAction, Pane};
use otrios_interfaces::spaces::{Change, Role};
pub(super) struct Preview {
    caption: String,
    position: [f32; 2],
}
impl Client {
    /// Trusted host selection controls expose their role independently of value.
    pub fn control_is_checkbox(&self, id: u32) -> Result<bool> {
        if !self.host_active {
            return Ok(false);
        }
        Ok(self.scene()?.controls().iter().any(|c| {
            c.node.id == id
                && matches!(c.node.responder, Some(TouchResponder::Activate(Action::AppEvent(control)))
                    if matches!(self.host_actions.get(&control), Some(HostAction::ToggleRemoveData)))
        }))
    }
    pub(super) fn preview_map_drag(&mut self) {
        let preview = self.host.drag.as_ref().filter(|d| d.moving).and_then(|d| {
            let snapshot = self.spaces.snapshot()?;
            let caption = match &d.kind {
                DragKind::Space(id) => snapshot.spaces.iter().find(|s| &s.id == id)?.name.clone(),
                DragKind::App(space, id) => snapshot
                    .spaces
                    .iter()
                    .find(|s| &s.id == space)?
                    .apps
                    .iter()
                    .find(|a| &a.app == id)?
                    .title
                    .clone(),
                DragKind::Resize(_) => "Resize Space".into(),
                DragKind::Dataset(space, _, cid) => format!(
                    "{} (data)",
                    snapshot
                        .spaces
                        .iter()
                        .find(|s| s.id == *space)?
                        .retained
                        .iter()
                        .find(|d| d.collection == *cid)?
                        .title
                ),
                DragKind::Pan => return None,
            };
            Some(Preview {
                caption,
                position: d.position,
            })
        });
        if let Some(mount) = &mut self.active {
            mount.map_drag = preview;
            mount.invalidate_presentation();
        }
    }
    pub(super) fn tick_map_drag(&mut self, now: u64) -> Result<bool> {
        let Some(drag) = &mut self.host.drag else {
            return Ok(false);
        };
        let dt = now.saturating_sub(drag.last_tick).min(50) as f32 / 1000.;
        drag.last_tick = now;
        if !drag.moving
            || !matches!(
                drag.kind,
                DragKind::Space(_) | DragKind::App(..) | DragKind::Dataset(..)
            )
        {
            return Ok(false);
        }
        let Some(mount) = &mut self.active else {
            return Ok(false);
        };
        let scene = cached_layout(mount)?;
        let Some(scroll) = &mut mount.map_scroll else {
            return Ok(false);
        };
        let path = scene.path(scroll.node()).ok_or("Missing map viewport")?;
        let frame = path.last().ok_or("Missing map frame")?;
        let [x, y] = drag.position;
        let top = frame.origin[1];
        let bottom = top + frame.node.frame.height * frame.scale;
        if x < frame.origin[0] || x > frame.origin[0] + frame.node.frame.width * frame.scale {
            return Ok(false);
        }
        let edge = 64_f32.min((bottom - top) / 3.);
        let direction = if y < top + edge {
            -((top + edge - y) / edge).clamp(0., 1.)
        } else if y > bottom - edge {
            ((y - bottom + edge) / edge).clamp(0., 1.)
        } else {
            0.
        };
        let before = scroll.info().offset;
        scroll.set_offset([0., before[1] + direction * 480. * dt]);
        self.host.scroll = scroll.info().offset[1];
        let changed = before != scroll.info().offset;
        if changed {
            mount.invalidate_presentation();
        }
        Ok(changed)
    }
    pub(super) fn prepare_map_scroll(
        &self,
        mount: &mut Mount,
        viewport: Option<u16>,
    ) -> Result<()> {
        let Some(viewport) = viewport else {
            return Ok(());
        };
        let scene = cached_layout(mount)?;
        let path = scene
            .path(u32::from(viewport))
            .ok_or("Missing Spaces map")?;
        let node = path.last().ok_or("Missing Spaces map frame")?.node;
        let mut scroll = super::super::text_scroll::Scroll::new(
            viewport,
            [node.frame.width, node.frame.height + self.host.scroll_limit],
        );
        scroll.sync(&scene)?;
        scroll.set_offset([0., self.host.scroll]);
        mount.map_scroll = Some(scroll);
        Ok(())
    }
    fn pan_map(&mut self, event: PointerEvent, at: u64) -> Result<()> {
        let mount = self.active.as_mut().ok_or("No Spaces map")?;
        let scene = cached_layout(mount)?;
        if let Some(scroll) = &mut mount.map_scroll {
            scroll.pointer(&scene, event, at)?;
            self.host.scroll = scroll.info().offset[1];
            mount.invalidate_presentation();
        }
        Ok(())
    }
    /// Local wheel input never changes the map mount or authoritative metadata.
    pub fn scroll_map(&mut self, epoch: u32, delta: f32) -> Result<bool> {
        if (!self.host_active
            && self
                .active
                .as_ref()
                .is_none_or(|m| super::super::oib::seed(&m.ui).is_none()))
            || epoch != self.epoch()
            || !delta.is_finite()
            || self.host.drag.is_some()
        {
            return Ok(false);
        }
        if let Some(viewport) = self
            .active
            .as_ref()
            .and_then(|mount| mount.designer.as_ref())
            .and_then(oib_designer::Designer::inspector_viewport)
        {
            let scene = self.scene()?;
            let Some(path) = scene.path(viewport) else {
                return Ok(false);
            };
            let Some(frame) = path.last() else {
                return Ok(false);
            };
            let position = [
                frame.origin[0] + frame.node.frame.width * frame.scale / 2.,
                frame.origin[1] + frame.node.frame.height * frame.scale / 2.,
            ];
            return Ok(self.designer_scroll_at(delta, position)?.unwrap_or(false));
        }
        let Some(mount) = &mut self.active else {
            return Ok(false);
        };
        mount.sync_panel_scroll()?;
        if let Some(scroll) = &mut mount.panel_scroll {
            let before = scroll.info().offset;
            scroll.set_offset([0., before[1] + delta]);
            self.host.panel_scroll = scroll.info().offset[1];
            let changed = before != scroll.info().offset;
            mount.invalidate_presentation();
            return Ok(changed);
        }
        let Some(scroll) = &mut mount.map_scroll else {
            return Ok(false);
        };
        let before = scroll.info().offset;
        scroll.set_offset([0., before[1] + delta]);
        self.host.scroll = scroll.info().offset[1];
        let changed = before != scroll.info().offset;
        mount.invalidate_presentation();
        Ok(changed)
    }

    fn map_action_at(&self, position: [f32; 2]) -> Result<Option<HostAction>> {
        Ok(self.scene()?.hit_path(position).iter().rev().find_map(|n| {
            if let Some(TouchResponder::Activate(Action::AppEvent(c))) = n.node.responder {
                self.host_actions.get(&c).cloned()
            } else {
                None
            }
        }))
    }
    fn map_action_for(&self, id: u32) -> Option<&HostAction> {
        let scene = self.scene().ok()?;
        match InputRouter::activate(&scene, id).action? {
            Action::AppEvent(c) => self.host_actions.get(&c),
            _ => None,
        }
    }
    fn map_admin(&self, id: &str) -> bool {
        self.spaces.snapshot().is_some_and(|s| {
            s.spaces
                .iter()
                .any(|s| s.id == id && s.role == Some(Role::Admin))
        })
    }
    pub(super) fn host_map_draggable(&self, id: u32) -> bool {
        self.host.editing
            && match self.map_action_for(id) {
                Some(
                    HostAction::MapSpace(s)
                    | HostAction::MapApp(s, _)
                    | HostAction::MapDataset(s, _, _)
                    | HostAction::Resize(s),
                ) => self.map_admin(s),
                _ => false,
            }
    }
    fn send_map_change(&mut self, change: Change) -> Result<bool> {
        self.host.message = if self.spaces.request(change).is_ok() {
            ""
        } else {
            "Wait for the current change, then try again."
        }
        .into();
        self.build_host(true)?;
        Ok(true)
    }
    pub(super) fn host_map_adjust(&mut self, id: u32, dx: i16, dy: i16) -> Result<bool> {
        if !self.host_map_draggable(id) {
            return Ok(false);
        }
        let Some(action) = self.map_action_for(id).cloned() else {
            return Ok(false);
        };
        if dx == 0 && dy == 0 {
            return match action {
                HostAction::MapApp(s, a) => self.begin_host_move(s, a, None, None),
                HostAction::MapDataset(s, a, cid) => self.begin_host_move(s, a, Some(cid), None),
                _ => Ok(false),
            };
        }
        let direction = if dx != 0 { dx.signum() } else { -dy.signum() };
        let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
        let change = match action {
            HostAction::MapSpace(space) => {
                let visible = snapshot
                    .spaces
                    .iter()
                    .filter(|s| {
                        !s.archived && (s.attached_count > 0 || s.role == Some(Role::Admin))
                    })
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>();
                let index = visible
                    .iter()
                    .position(|s| s == &space)
                    .ok_or("Space unavailable")?;
                let target = (index as isize + direction as isize)
                    .clamp(0, visible.len() as isize - 1) as usize;
                if target == index {
                    return Ok(false);
                }
                let mut remaining = visible;
                remaining.remove(index);
                Change::PlaceSpace {
                    space,
                    before: remaining.get(target).cloned(),
                }
            }
            HostAction::MapApp(space, app) => {
                let apps = snapshot
                    .spaces
                    .iter()
                    .find(|s| s.id == space)
                    .ok_or("Space unavailable")?
                    .apps
                    .iter()
                    .map(|a| a.app.clone())
                    .collect::<Vec<_>>();
                let index = apps
                    .iter()
                    .position(|a| a == &app)
                    .ok_or("App unavailable")?;
                let target = (index as isize + direction as isize).clamp(0, apps.len() as isize - 1)
                    as usize;
                if target == index {
                    return Ok(false);
                }
                let mut remaining = apps;
                remaining.remove(index);
                Change::PlaceApp {
                    space,
                    app,
                    before: remaining.get(target).cloned(),
                }
            }
            HostAction::Resize(space) => {
                let summary = snapshot
                    .spaces
                    .iter()
                    .find(|s| s.id == space)
                    .ok_or("Space unavailable")?;
                let mut layout = summary.layout;
                if dx != 0 {
                    layout.width = (i16::from(layout.width) + dx.signum()).clamp(1, 6) as u8;
                }
                if dy != 0 {
                    layout.height = (i16::from(layout.height) - dy.signum()).clamp(1, 6) as u8;
                }
                Change::ConfigureSpace {
                    space,
                    name: summary.name.clone(),
                    layout,
                }
            }
            _ => return Ok(false),
        };
        self.send_map_change(change)
    }
    pub(super) fn host_map_pointer(
        &mut self,
        epoch: u32,
        event: PointerEvent,
        at: u64,
    ) -> Result<Option<bool>> {
        if !matches!(self.host.pane, Pane::Home | Pane::Spaces) {
            return self.host_panel_pointer(epoch, event, at);
        }
        let viewport = self
            .viewport
            .or(self.layout_viewport)
            .unwrap_or([1280., 800.]);
        if event.phase == PointerPhase::Down {
            if epoch != self.epoch() || self.host.drag.is_some() {
                return Ok(None);
            }
            if event.position[1] < 104. || event.position[1] >= viewport[1] {
                return Ok(None);
            }
            let action = self.map_action_at(event.position)?;
            // Plus/ellipsis retain ordinary activation and never start a drag.
            if matches!(
                action,
                Some(
                    HostAction::Settings(..)
                        | HostAction::Edit
                        | HostAction::Page(_)
                        | HostAction::Pane(Pane::Create)
                )
            ) {
                return Ok(None);
            }
            let kind = match &action {
                Some(HostAction::MapSpace(s)) if self.host.editing && self.map_admin(s) => {
                    DragKind::Space(s.clone())
                }
                Some(HostAction::MapApp(s, a)) if self.host.editing && self.map_admin(s) => {
                    DragKind::App(s.clone(), a.clone())
                }
                Some(HostAction::MapDataset(s, a, cid))
                    if self.host.editing && self.map_admin(s) =>
                {
                    DragKind::Dataset(s.clone(), a.clone(), cid.clone())
                }
                Some(HostAction::Resize(s)) if self.host.editing && self.map_admin(s) => {
                    DragKind::Resize(s.clone())
                }
                _ => DragKind::Pan,
            };
            if matches!(kind, DragKind::Pan) {
                self.pan_map(event, at)?;
            } else if let Some(scroll) = self.active.as_mut().and_then(|m| m.map_scroll.as_mut()) {
                scroll.cancel();
            }
            self.host.drag = Some(Drag {
                pointer: event.id,
                epoch,
                generation: self.spaces.snapshot().map_or(0, |s| s.generation),
                kind,
                start: event.position,
                position: event.position,
                moving: false,
                last_tick: at,
                action,
            });
            return Ok(Some(true));
        }
        let Some(mut drag) = self.host.drag.take() else {
            return Ok(None);
        };
        if drag.pointer != event.id {
            self.host.drag = Some(drag);
            return Ok(Some(false));
        }
        if epoch != drag.epoch && epoch != self.epoch() {
            self.host.drag = Some(drag);
            return Ok(Some(false));
        }
        drag.position = event.position;
        let delta = [
            drag.position[0] - drag.start[0],
            drag.position[1] - drag.start[1],
        ];
        drag.moving |= delta[0].abs().max(delta[1].abs()) > 8.;
        let pan = matches!(drag.kind, DragKind::Pan);
        if pan {
            self.pan_map(event, at)?;
        }
        if event.phase == PointerPhase::Cancel {
            self.preview_map_drag();
            return Ok(Some(true));
        }
        if event.phase == PointerPhase::Move {
            let moving = drag.moving;
            self.host.drag = Some(drag);
            if !pan && moving {
                self.preview_map_drag();
            }
            return Ok(Some(pan || moving));
        }
        if event.phase != PointerPhase::Up {
            return Ok(Some(false));
        }
        self.preview_map_drag();
        if self
            .spaces
            .snapshot()
            .is_none_or(|s| s.generation != drag.generation)
        {
            self.build_host(true)?;
            return Ok(Some(true));
        }
        if !drag.moving {
            return match drag.action {
                Some(action) => self.host_action(action).map(Some),
                None => Ok(Some(false)),
            };
        }
        let target = if let Some(mount) = &self.active
            && let Some(scroll) = &mount.map_scroll
        {
            let scene = cached_layout(mount)?;
            let path = scene.path(scroll.node()).ok_or("Missing map viewport")?;
            let frame = path.last().ok_or("Missing map frame")?;
            self.map_action_at([
                event.position[0],
                event.position[1].clamp(
                    frame.origin[1] + 1.,
                    frame.origin[1] + frame.node.frame.height * frame.scale - 1.,
                ),
            ])?
        } else {
            None
        };
        let change = match drag.kind {
            DragKind::Pan => return Ok(Some(true)),
            DragKind::Space(space) => {
                let before = match target {
                    Some(
                        HostAction::MapSpace(s)
                        | HostAction::SpaceBody(s)
                        | HostAction::MapApp(s, _)
                        | HostAction::MapDataset(s, _, _)
                        | HostAction::Settings(s, _),
                    ) if s != space => Some(s),
                    _ => return Ok(Some(true)),
                };
                let before = if let Some(target) = before {
                    let spaces = &self.spaces.snapshot().ok_or("Spaces unavailable")?.spaces;
                    let from = spaces
                        .iter()
                        .position(|s| s.id == space)
                        .ok_or("Space unavailable")?;
                    let to = spaces
                        .iter()
                        .position(|s| s.id == target)
                        .ok_or("Space unavailable")?;
                    if from < to {
                        spaces.get(to + 1).map(|s| s.id.clone())
                    } else {
                        Some(target)
                    }
                } else {
                    None
                };
                Change::PlaceSpace { space, before }
            }
            DragKind::App(space, app) => {
                if let Some(destination) = target
                    .as_ref()
                    .and_then(target_space)
                    .filter(|s| *s != space)
                {
                    if self.map_admin(destination) {
                        return self
                            .begin_host_move(space, app, None, Some(destination.into()))
                            .map(Some);
                    }
                    return Ok(Some(true));
                }
                let before = match target {
                    Some(HostAction::MapApp(s, a)) if s == space && a == app => {
                        return Ok(Some(true));
                    }
                    Some(HostAction::MapApp(s, a)) if s == space && a != app => {
                        let apps = &self
                            .spaces
                            .snapshot()
                            .ok_or("Spaces unavailable")?
                            .spaces
                            .iter()
                            .find(|s| s.id == space)
                            .ok_or("Space unavailable")?
                            .apps;
                        let from = apps
                            .iter()
                            .position(|v| v.app == app)
                            .ok_or("App unavailable")?;
                        let to = apps
                            .iter()
                            .position(|v| v.app == a)
                            .ok_or("App unavailable")?;
                        if from < to {
                            apps.get(to + 1).map(|a| a.app.clone())
                        } else {
                            Some(a)
                        }
                    }
                    Some(
                        HostAction::MapSpace(s)
                        | HostAction::SpaceBody(s)
                        | HostAction::MapDataset(s, _, _)
                        | HostAction::Settings(s, _),
                    ) if s == space => None,
                    _ => {
                        return Ok(Some(true));
                    }
                };
                Change::PlaceApp { space, app, before }
            }
            DragKind::Dataset(space, app, cid) => {
                if let Some(destination) = target
                    .as_ref()
                    .and_then(target_space)
                    .filter(|s| *s != space)
                    && self.map_admin(destination)
                {
                    return self
                        .begin_host_move(space, app, Some(cid), Some(destination.into()))
                        .map(Some);
                }
                return Ok(Some(true));
            }
            DragKind::Resize(space) => {
                let summary = self
                    .spaces
                    .snapshot()
                    .and_then(|s| s.spaces.iter().find(|s| s.id == space))
                    .ok_or("Space unavailable")?;
                let mut layout = summary.layout;
                layout.width =
                    (f32::from(layout.width) + (delta[0] / 160.).round()).clamp(1., 6.) as u8;
                layout.height =
                    (f32::from(layout.height) + (delta[1] / 152.).round()).clamp(1., 6.) as u8;
                Change::ConfigureSpace {
                    space,
                    name: summary.name.clone(),
                    layout,
                }
            }
        };
        self.send_map_change(change).map(Some)
    }
}

fn target_space(action: &HostAction) -> Option<&str> {
    match action {
        HostAction::MapSpace(s)
        | HostAction::SpaceBody(s)
        | HostAction::MapApp(s, _)
        | HostAction::MapDataset(s, _, _)
        | HostAction::Settings(s, _) => Some(s),
        _ => None,
    }
}

pub(super) fn present_drag(root: &mut SceneNode, preview: &Preview, viewport: [f32; 2]) {
    use crate::dashboard::components::{background, canvas, text};
    use otrios_interfaces::scene::Rect;
    let [w, h] = viewport;
    let x = (preview.position[0] - 90.).clamp(0., (w - 180.).max(0.));
    let y = if preview.position[1] + 76. < h - 80. {
        preview.position[1] + 20.
    } else {
        preview.position[1] - 76.
    };
    let caption: String = preview.caption.chars().take(24).collect();
    let mut preview = canvas(
        4_210_000_000,
        Rect::new(x, y, 180., 56.),
        vec![
            background(4_210_000_001, 180., 56., [255, 254, 250, 255], 14.),
            text(
                4_210_000_002,
                Rect::new(12., 14., 156., 32.),
                &caption,
                2,
                [44, 50, 43, 255],
            ),
        ],
    );
    preview.responder = Some(TouchResponder::PassThrough);
    root.children.push(preview);
}

/// Clip semantic controls on the presented clone, never the canonical layout.
/// A quiet edge fade appears only while there is more map content below.
pub(super) fn present_map(root: &mut SceneNode, node: u32, info: super::super::text_scroll::Info) {
    use crate::dashboard::components::background;
    use otrios_interfaces::scene::Rect;
    if root.id == node {
        root.semantics = None;
        let bounds = [root.frame.width, root.frame.height];
        for child in &mut root.children {
            crate::dashboard::scene::clip_controls(child, [0., 0.], bounds);
        }
        if info.offset[1] + 0.5 < info.limit[1] {
            let fade_height = 80_f32.min(bounds[1]);
            let step_height = fade_height / 16.;
            for step in 0..16 {
                let mut fade = background(
                    4_200_000_000 + step,
                    bounds[0],
                    step_height,
                    [247, 246, 240, ((step + 1) * 255 / 16) as u8],
                    0.,
                );
                fade.frame = Rect::new(
                    0.,
                    bounds[1] - fade_height + step as f32 * step_height,
                    bounds[0],
                    step_height,
                );
                fade.responder = Some(TouchResponder::PassThrough);
                root.children.push(fade);
            }
        }
    } else {
        for child in &mut root.children {
            present_map(child, node, info.clone());
        }
    }
}

/// Reuse an app's authored home breadcrumb, with a trusted host return action.
pub(super) fn present_breadcrumb(root: &mut SceneNode, mount: &Mount, control: u8) {
    use crate::dashboard::components::{canvas, text};
    use otrios_interfaces::scene::{Primitive, Rect, SwitchSemantics};
    fn replace(node: &mut SceneNode, ui: &Ui, control: u8) -> bool {
        if let Some(TouchResponder::Activate(Action::AppNavigate(id))) = node.responder
            && ui
                .navigation
                .iter()
                .any(|n| n.action == id && n.ui_ref == "app:home/ui")
        {
            // The first breadcrumb owns the surrounding corner padding too.
            node.frame.x -= 24.;
            node.frame.y -= 24.;
            node.frame.width += 48.;
            node.frame.height = 96.;
            for child in &mut node.children {
                child.frame.x += 24.;
                child.frame.y += 24.;
            }
            node.responder = Some(TouchResponder::Activate(Action::AppEvent(control)));
            node.semantics = Some(SwitchSemantics {
                label: "Back to OTriOS".into(),
                checked: false,
            });
            return true;
        }
        // Visit every ancestor link; a short-circuit would miss sibling links.
        let mut found = false;
        for child in &mut node.children {
            found |= replace(child, ui, control);
        }
        found
    }
    if replace(root, &mount.ui, control) {
        return;
    }
    let scale = otrios_interfaces::theme::HEADER_FONT_SCALE;
    let width = mount
        .font
        .as_ref()
        .map_or(144., |f| f.text_width("< OTriOS", scale as f32) + 8.);
    let height = mount
        .font
        .as_ref()
        .map_or(40., |f| f.metrics().line_height(scale as f32));
    let y = (60. - height) / 2.;
    // AppUi reserves this title area even when it has no authored ancestors.
    if let Some(header) = root.children.iter_mut().find(|n| {
        n.frame.x == 24.
            && n.frame.y == 24.
            && n.frame.height == 72.
            && matches!(n.primitive, Primitive::Canvas { .. })
    }) {
        let shift = width + 42.;
        for child in &mut header.children {
            child.frame.x += shift;
            child.frame.width = (child.frame.width - shift).max(1.);
        }
        header.children.push(text(
            4_100_000_003,
            Rect::new(width + 12., y, 24., height),
            "•",
            scale,
            crate::dashboard::components::ACCENT,
        ));
    }
    let mut back = canvas(
        4_100_000_000,
        Rect::new(0., 0., width + 48., 96.),
        vec![text(
            4_100_000_002,
            Rect::new(24., 24. + y, width, height),
            "< OTriOS",
            scale,
            otrios_interfaces::theme::TEXT,
        )],
    );
    back.responder = Some(TouchResponder::Activate(Action::AppEvent(control)));
    back.semantics = Some(SwitchSemantics {
        label: "Back to OTriOS".into(),
        checked: false,
    });
    root.children.push(back);
}
