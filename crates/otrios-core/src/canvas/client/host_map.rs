// SPDX-License-Identifier: LGPL-3.0-only
//! Trusted map gestures never become publisher app events or document writes.
use super::*;
use crate::canvas::host_ui::{Drag, DragKind, HostAction, Pane};
use otrios_interfaces::spaces::{Change, Role};
impl Client {
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
                    HostAction::MapSpace(s) | HostAction::MapApp(s, _) | HostAction::Resize(s),
                ) => self.map_admin(s),
                _ => false,
            }
    }
    fn send_map_change(&mut self, change: Change) -> Result<bool> {
        self.host.message = if self.spaces.request(change).is_ok() {
            "Saving…"
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
        let direction = if dx != 0 { dx.signum() } else { -dy.signum() };
        let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
        let change = match action {
            HostAction::MapSpace(space) => {
                let visible = snapshot
                    .spaces
                    .iter()
                    .filter(|s| s.attached_count > 0 || s.role == Some(Role::Admin))
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
    ) -> Result<Option<bool>> {
        if !matches!(self.host.pane, Pane::Home | Pane::Spaces) {
            return Ok(None);
        }
        let viewport = self
            .viewport
            .or(self.layout_viewport)
            .unwrap_or([1280., 800.]);
        if event.phase == PointerPhase::Down {
            if epoch != self.epoch() || self.host.drag.is_some() {
                return Ok(None);
            }
            if event.position[1] < 104. || event.position[1] > viewport[1] - 80. {
                return Ok(None);
            }
            let action = self.map_action_at(event.position)?;
            // Plus/ellipsis retain ordinary activation and never start a drag.
            if matches!(action, Some(HostAction::Settings(..))) {
                return Ok(None);
            }
            let kind = match &action {
                Some(HostAction::MapSpace(s)) if self.host.editing && self.map_admin(s) => {
                    DragKind::Space(s.clone())
                }
                Some(HostAction::MapApp(s, a)) if self.host.editing && self.map_admin(s) => {
                    DragKind::App(s.clone(), a.clone())
                }
                Some(HostAction::Resize(s)) if self.host.editing && self.map_admin(s) => {
                    DragKind::Resize(s.clone())
                }
                _ => DragKind::Pan,
            };
            self.host.drag = Some(Drag {
                pointer: event.id,
                epoch,
                generation: self.spaces.snapshot().map_or(0, |s| s.generation),
                kind,
                start: event.position,
                position: event.position,
                scroll: self.host.scroll,
                moving: false,
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
        if event.phase == PointerPhase::Cancel {
            self.build_host(true)?;
            return Ok(Some(true));
        }
        if event.phase == PointerPhase::Move {
            if drag.moving && matches!(drag.kind, DragKind::Pan) {
                self.host.scroll = (drag.scroll - delta[1]).clamp(0., self.host.scroll_limit);
                self.host.drag = Some(drag);
                self.build_host(true)?;
            } else {
                self.host.drag = Some(drag);
                self.build_host(true)?;
            }
            return Ok(Some(true));
        }
        if event.phase != PointerPhase::Up {
            return Ok(Some(false));
        }
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
        let target = self.map_action_at(event.position)?;
        let change = match drag.kind {
            DragKind::Pan => return Ok(Some(true)),
            DragKind::Space(space) => {
                let before = match target {
                    Some(
                        HostAction::MapSpace(s)
                        | HostAction::SpaceBody(s)
                        | HostAction::MapApp(s, _)
                        | HostAction::Settings(s, _),
                    ) if s != space => Some(s),
                    _ => None,
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
                let before = match target {
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
                        | HostAction::Settings(s, _),
                    ) if s == space => None,
                    _ => {
                        self.host.message =
                            "Moving apps between Spaces is not available yet.".into();
                        self.build_host(true)?;
                        return Ok(Some(true));
                    }
                };
                Change::PlaceApp { space, app, before }
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
