// SPDX-License-Identifier: LGPL-3.0-only
//! Shared first responder and modal scopes. Hosts only mirror this state.
use super::*;
use crate::dashboard::input::focus::{State, Target};

impl Client {
    fn focus_context(&self, scene: &Scene) -> (String, Vec<u32>, Option<u32>) {
        let Some(m) = &self.active else {
            return (String::new(), vec![], None);
        };
        let domain = if self.host_active {
            "host".into()
        } else {
            format!("{}:{}", m.ui.ui_ref, m.state_ref)
        };
        let domain = format!("{domain}:{:?}", self.selected_space);
        let mut scopes = vec![];
        let mut container = None;
        if m.host_backdrop.is_some()
            && let Some(panel) = &m.panel_scroll
        {
            scopes.push(panel.node());
            container = Some(panel.node());
        }
        if m.chat.as_ref().is_some_and(|chat| chat.info().modal) {
            scopes.push(super::super::chat::MODAL_BARRIER);
            // Chat clears background semantics after composing the breadcrumb.
            container = None;
        }
        if scene.path(oib_workspace::MODAL_ROOT).is_some() {
            scopes.push(oib_workspace::MODAL_ROOT);
            container = Some(oib_workspace::MODAL_ROOT);
        }
        if m.breadcrumbs
            .as_ref()
            .is_some_and(|trail| trail.account_open)
        {
            scopes.push(breadcrumbs::BASE + 305);
            container = Some(breadcrumbs::BASE + 305);
        }
        (domain, scopes, container)
    }
    pub(super) fn focus_targets(&self) -> Result<Vec<Target>> {
        let scene = self.scene()?;
        let (_, _, container) = self.focus_context(&scene);
        let mut targets = scene
            .controls()
            .into_iter()
            .filter(|owner| {
                container.is_none_or(|id| {
                    scene
                        .path(owner.node.id)
                        .is_some_and(|path| path.iter().any(|n| n.node.id == id))
                })
            })
            .filter_map(|owner| {
                let m = self.active.as_ref()?;
                let key = match owner.node.responder {
                    Some(TouchResponder::Activate(Action::KeyboardKey(key))) => key != u16::MAX,
                    Some(TouchResponder::Activate(Action::AppEvent(control))) => m
                        .handler(control)
                        .is_some_and(|h| matches!(h.operation, Operation::Key(_))),
                    _ => false,
                } || m
                    .designer
                    .as_ref()
                    .is_some_and(|d| d.keyboard_key(owner.node.id).is_some());
                if key {
                    return None;
                }
                Some(Target {
                    id: owner.node.id,
                    label: owner.node.semantics.as_ref()?.label.clone(),
                    responder: owner.node.responder?,
                })
            })
            .collect::<Vec<_>>();
        if self.focus_context(&scene).1.is_empty()
            && let Some(chat) = self.active.as_ref().and_then(|m| m.chat.as_ref())
        {
            targets.extend(chat.replay_controls().iter().map(|c| Target {
                id: c.id,
                label: c.label.clone(),
                responder: TouchResponder::Activate(Action::KeyboardKey(u16::MAX)),
            }));
        }
        Ok(targets)
    }
    pub(super) fn focus_allows_activation(&self, id: u32) -> Result<bool> {
        let scene = self.scene()?;
        let (_, scopes, container) = self.focus_context(&scene);
        if scopes.is_empty() {
            return Ok(true);
        }
        let Some(path) = scene.path(id) else {
            return Ok(false);
        };
        if path
            .iter()
            .any(|n| n.node.responder == Some(TouchResponder::PassThrough))
        {
            return Ok(false);
        }
        let keyboard = self.active.as_ref().is_some_and(|m| {
            m.designer
                .as_ref()
                .is_some_and(|d| d.keyboard_key(id).is_some())
                || path.last().is_some_and(|n| match n.node.responder {
                    Some(TouchResponder::Activate(Action::KeyboardKey(key))) => key != u16::MAX,
                    Some(TouchResponder::Activate(Action::AppEvent(c))) => m
                        .handler(c)
                        .is_some_and(|h| matches!(h.operation, Operation::Key(_))),
                    _ => false,
                })
        });
        Ok(keyboard
            || if let Some(container) = container {
                path.iter().any(|n| n.node.id == container)
                    || (container == breadcrumbs::BASE + 305 && id == breadcrumbs::BASE + 300)
            } else {
                scene.controls().iter().any(|n| n.node.id == id)
            })
    }
    pub(super) fn sync_responder_focus(&self) -> Result<()> {
        if self.active.is_none() {
            self.responder_focus
                .lock()
                .unwrap()
                .reconcile("", &[], &[], None);
            return Ok(());
        }
        let scene = self.scene()?;
        let (domain, scopes, _) = self.focus_context(&scene);
        let targets = self.focus_targets()?;
        let preferred = self
            .text_inputs()?
            .iter()
            .find(|input| input.focused)
            .map(|input| input.id);
        self.responder_focus
            .lock()
            .unwrap()
            .reconcile(&domain, &scopes, &targets, preferred);
        Ok(())
    }
    /// Stable first-responder projection for accessibility and platform IME hosts.
    pub fn focus_state(&self) -> Result<State> {
        self.sync_responder_focus()?;
        Ok(self.responder_focus.lock().unwrap().state())
    }
    pub(super) fn select_responder_focus(&self, id: Option<u32>, visible: bool) -> Result<bool> {
        self.sync_responder_focus()?;
        let targets = self.focus_targets()?;
        Ok(self
            .responder_focus
            .lock()
            .unwrap()
            .select(&targets, id, visible))
    }
    pub fn focus_control(&mut self, epoch: u32, id: u32, visible: bool) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        if !self.focus_targets()?.iter().any(|t| t.id == id) {
            return Ok(false);
        }
        let old = self
            .text_inputs()?
            .into_iter()
            .find(|i| i.focused && i.id != id);
        let blurred = if let Some(old) = old {
            self.blur_text_input(epoch, old.id)?
        } else {
            false
        };
        Ok(self.select_responder_focus(Some(id), visible)? || blurred)
    }
    /// Tab follows shared semantic order and wraps within the current scope.
    pub fn focus_next(&mut self, epoch: u32, backward: bool) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        self.sync_responder_focus()?;
        let targets = self.focus_targets()?;
        let id = self
            .responder_focus
            .lock()
            .unwrap()
            .next(&targets, backward);
        let Some(id) = id else {
            return Ok(false);
        };
        self.focus_control(epoch, id, true)?;
        if self.text_inputs()?.iter().any(|i| i.id == id) {
            self.activate_inner(epoch, id)?;
        }
        Ok(true)
    }
    /// Non-text keys start at the focused responder; activation still follows
    /// the same target-to-ancestor delivery as touch and accessibility actions.
    pub fn focus_key(&mut self, epoch: u32, key: &str) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        let Some(id) = self.focus_state()?.id else {
            return Ok(false);
        };
        self.select_responder_focus(Some(id), true)?;
        match key {
            "Enter" | " " | "Space" => self.activate(epoch, id),
            "ArrowLeft" => self.adjust_control(id, -50, 0),
            "ArrowRight" => self.adjust_control(id, 50, 0),
            "ArrowUp" => self.adjust_control(id, 0, 50),
            "ArrowDown" => self.adjust_control(id, 0, -50),
            _ => Ok(false),
        }
    }
    pub(super) fn append_focus_vertices(&self, vertices: &mut Vec<f32>) -> Result<()> {
        let state = self.responder_focus.lock().unwrap().state();
        if !state.visible {
            return Ok(());
        }
        let Some(id) = state.id else {
            return Ok(());
        };
        let Some(bounds) = self.control_bounds(id)? else {
            return Ok(());
        };
        let color = self
            .active
            .as_ref()
            .unwrap()
            .shell_appearance
            .palette()
            .focus
            .map(|c| f32::from(c) / 255.);
        let x = bounds[0] + 2.;
        let y = bounds[1] + 2.;
        let w = (bounds[2] - 4.).max(0.);
        let h = (bounds[3] - 4.).max(0.);
        if w < 4. || h < 4. {
            return Ok(());
        }
        let radius = 10_f32.min(w / 2.).min(h / 2.);
        let point = |index: usize, inset: f32| {
            let corner = index / 8;
            let angle = (corner as f32 + (index % 8) as f32 / 8.) * std::f32::consts::FRAC_PI_2;
            let center = [
                if corner == 0 || corner == 3 {
                    x + w - radius
                } else {
                    x + radius
                },
                if corner < 2 {
                    y + h - radius
                } else {
                    y + radius
                },
            ];
            [
                center[0] + angle.cos() * (radius - inset),
                center[1] + angle.sin() * (radius - inset),
            ]
        };
        for index in 0..32 {
            let next = (index + 1) % 32;
            let a = point(index, 0.);
            let b = point(next, 0.);
            let c = point(index, 2.);
            let d = point(next, 2.);
            for [x, y] in [a, b, c, c, b, d] {
                vertices.extend([x, y, color[0], color[1], color[2], color[3]]);
            }
        }
        Ok(())
    }
}
