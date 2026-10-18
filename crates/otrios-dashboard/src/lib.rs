// SPDX-License-Identifier: Apache-2.0
//! Thin host adapter. The LGPL runtime owns state, snapshots, input and rendering.
use otrios_core::dashboard::{Replica, SyncSession, render_scene, shell::Shell};
use otrios_interfaces::keyboard::Edit;
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
use wasm_bindgen::prelude::*;

fn browser_shell() -> Shell {
    let mut shell = Shell::default();
    shell.set_monitoring_access(false);
    shell
}
#[wasm_bindgen]
pub struct Dashboard {
    replica: Replica,
    session: SyncSession,
    shell: Shell,
    stress: otrios_core::dashboard::motion::Stress,
}
#[wasm_bindgen]
impl Dashboard {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Dashboard, String> {
        Ok(Self {
            replica: Replica::new()?,
            session: SyncSession::default(),
            shell: browser_shell(),
            stress: otrios_core::dashboard::motion::Stress::new()?,
        })
    }
    pub fn load(bytes: &[u8]) -> Result<Dashboard, String> {
        Ok(Self {
            replica: Replica::load(bytes)?,
            session: SyncSession::default(),
            shell: browser_shell(),
            stress: otrios_core::dashboard::motion::Stress::new()?,
        })
    }
    pub fn document(&mut self) -> Vec<u8> {
        self.replica.save()
    }
    pub fn merge(&mut self, bytes: &[u8]) -> Result<bool, String> {
        self.replica.merge(bytes)
    }
    pub fn reset_sync(&mut self) {
        self.session = SyncSession::default();
    }
    pub fn state_confirmed(&self) -> bool {
        self.replica.is_confirmed(&self.session)
    }
    pub fn input_busy(&self) -> bool {
        self.shell.input_busy()
    }
    pub fn sync_message(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.replica.generate_sync(&mut self.session)
    }
    pub fn receive_sync(&mut self, bytes: &[u8]) -> Result<bool, String> {
        self.replica.receive_sync(&mut self.session, bytes)
    }
    pub fn present(&mut self, bytes: &[u8]) -> Result<bool, String> {
        self.replica
            .set_presentation(if bytes.is_empty() { None } else { Some(bytes) })
    }
    pub fn touch(&mut self, x: u32, y: u32) -> Result<bool, String> {
        self.replica.touch(x, y)
    }
    pub fn set_enabled(&mut self, value: bool) -> Result<bool, String> {
        self.replica.set_enabled(value)
    }
    pub fn vertices(&mut self) -> Result<Vec<f32>, String> {
        Ok(render_scene(self.shell.scene(&self.replica)?.as_ref()).vertices)
    }
    pub fn motion_intent(&self) -> Result<Vec<f32>, String> {
        let i = self.replica.motion()?;
        Ok(vec![
            if i.playing { 1. } else { 0. },
            i.speed,
            f32::from(i.count),
        ])
    }
    pub fn stress_tick(&mut self, now: f64) -> Result<bool, String> {
        self.stress.tick(now, self.shell.stress_hz())
    }
    pub fn stress_stall_ms(&self) -> f64 {
        self.shell.stress_stall_ms() as f64
    }
    pub fn stress_updates(&self) -> f64 {
        self.stress.updates as f64
    }
    pub fn keyboard_visible(&self) -> bool {
        self.shell.keyboard_visible()
    }
    pub fn show_keyboard(&mut self) {
        self.shell.show_keyboard();
    }
    pub fn set_monitoring_access(&mut self, allowed: bool) -> Result<bool, String> {
        let mut changed = self.shell.set_monitoring_access(allowed);
        if !allowed {
            changed |= self.replica.set_presentation(None)?;
        }
        Ok(changed)
    }
    pub fn screen(&self) -> String {
        self.shell.screen_name().into()
    }
    pub fn controls_layout(&self) -> bool {
        self.shell.controls_layout()
    }
    pub fn show_controls_layout(&mut self, value: bool) {
        self.shell.show_controls_layout(value);
    }
    pub fn navigate(&mut self, name: &str) -> bool {
        self.shell.open_name(name)
    }
    pub fn back(&mut self) -> bool {
        self.shell.back()
    }
    pub fn editing(&self) -> bool {
        self.shell.editing()
    }
    pub fn tick(&mut self, now: f64) -> Result<bool, String> {
        let now = now.max(0.) as u64;
        Ok(self.shell.tick(now) | self.shell.flush_controls(&mut self.replica, now)?)
    }
    pub fn ink_outgoing(&mut self, now: f64) -> Vec<u8> {
        self.shell.ink_outgoing(now.max(0.) as u64)
    }
    pub fn receive_ink(&mut self, bytes: &[u8], now: f64) -> Result<bool, String> {
        self.shell.receive_ink(bytes, now.max(0.) as u64)
    }
    pub fn adjust_control(&mut self, id: u32, dx: i16, dy: i16) -> Result<bool, String> {
        self.shell.adjust_control(&mut self.replica, id, dx, dy)
    }
    pub fn control_draggable(&mut self, id: u32) -> Result<bool, String> {
        Ok(self.shell.scene(&self.replica)?.controls().iter().any(|c| {
            c.node.id == id
                && matches!(
                    c.node.responder,
                    Some(otrios_interfaces::scene::TouchResponder::Drag(_))
                )
        }))
    }
    pub fn control_value(&mut self, id: u32) -> Result<f64, String> {
        Ok(self
            .shell
            .control_value(&self.replica, id)?
            .unwrap_or(f64::NAN))
    }
    pub fn cancel_input(&mut self) {
        self.shell.cancel();
    }
    pub fn type_text(&mut self, text: &str) -> bool {
        self.shell.hardware_edit(&Edit::Commit(text.into()))
    }
    pub fn key(&mut self, name: &str, select: bool) -> bool {
        if name == "Escape" {
            return self.shell.back();
        }
        let edit = match name {
            "Backspace" => Edit::Backspace,
            "ArrowLeft" => Edit::Left { select },
            "ArrowRight" => Edit::Right { select },
            "SelectAll" => Edit::SelectAll,
            "Tab" => Edit::Tab { backward: select },
            "Enter" => Edit::Enter,
            _ => return false,
        };
        self.shell.hardware_edit(&edit)
    }
    /// Assistive description derives from the same primitive scene as the pixels.
    pub fn scene_text(&mut self) -> Result<String, String> {
        fn gather(node: &otrios_interfaces::scene::SceneNode, values: &mut Vec<String>) {
            if let otrios_interfaces::scene::Primitive::Text { value, .. } = &node.primitive {
                values.push(value.clone());
            }
            for child in &node.children {
                gather(child, values);
            }
        }
        let mut values = Vec::new();
        gather(self.shell.scene(&self.replica)?.root(), &mut values);
        Ok(values.join(". "))
    }
    pub fn snapshot(&self) -> Vec<u8> {
        self.replica.snapshot().bytes().to_vec()
    }
    pub fn layout(&self) -> Vec<u8> {
        self.replica.snapshot().layout().bytes().to_vec()
    }
    pub fn layout_id(&self) -> String {
        self.replica.snapshot().layout().hex_id()
    }
    pub fn snapshot_id(&self) -> String {
        self.replica.snapshot().hex_id()
    }
    pub fn pointer(
        &mut self,
        phase: u8,
        id: u32,
        x: f32,
        y: f32,
        now: f64,
    ) -> Result<bool, String> {
        let phase = match phase {
            0 => PointerPhase::Down,
            1 => PointerPhase::Move,
            2 => PointerPhase::Up,
            3 => PointerPhase::Cancel,
            _ => return Err("Invalid pointer phase".into()),
        };
        self.shell.pointer(
            &mut self.replica,
            PointerEvent {
                id,
                phase,
                position: [x, y],
            },
            now.max(0.) as u64,
        )
    }
    pub fn activate(&mut self, id: u32) -> Result<bool, String> {
        self.shell.activate(&mut self.replica, id)
    }
    pub fn control_ids(&mut self) -> Result<Vec<u32>, String> {
        Ok(self
            .shell
            .scene(&self.replica)?
            .controls()
            .iter()
            .map(|c| c.node.id)
            .collect())
    }
    pub fn control_bounds(&mut self, id: u32) -> Result<Vec<f32>, String> {
        Ok(self
            .shell
            .scene(&self.replica)?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .map(|c| {
                vec![
                    c.origin[0],
                    c.origin[1],
                    c.node.frame.width * c.scale,
                    c.node.frame.height * c.scale,
                ]
            })
            .unwrap_or_default())
    }
    pub fn control_label(&mut self, id: u32) -> Result<String, String> {
        let scene = self.shell.scene(&self.replica)?;
        Ok(scene
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .map(|c| {
                use otrios_interfaces::scene::{Action, TouchResponder};
                match c.node.responder {
                    Some(TouchResponder::Activate(Action::KeyboardKey(10))) => "Backspace".into(),
                    Some(TouchResponder::Activate(Action::KeyboardKey(35))) => {
                        "Move caret left".into()
                    }
                    Some(TouchResponder::Activate(Action::KeyboardKey(36))) => {
                        "Move caret right".into()
                    }
                    Some(TouchResponder::Activate(Action::KeyboardKey(37))) => {
                        "Hide keyboard".into()
                    }
                    _ => c
                        .node
                        .semantics
                        .as_ref()
                        .map(|s| s.label.clone())
                        .unwrap_or_default(),
                }
            })
            .unwrap_or_default())
    }
    pub fn control_disabled(&mut self, id: u32) -> Result<bool, String> {
        Ok(
            self.shell.screen() == otrios_interfaces::scene::Screen::Home
                && self
                    .shell
                    .scene(&self.replica)?
                    .controls()
                    .iter()
                    .find(|c| c.node.id == id)
                    .is_some_and(|c| {
                        matches!(
                            c.node.responder,
                            Some(otrios_interfaces::scene::TouchResponder::Activate(
                                otrios_interfaces::scene::Action::Back
                                    | otrios_interfaces::scene::Action::Home
                            ))
                        )
                    }),
        )
    }
    pub fn control_role(&mut self, id: u32) -> Result<String, String> {
        if self.shell.control_value(&self.replica, id)?.is_some() {
            return Ok("slider".into());
        }
        Ok(if self
            .shell
            .scene(&self.replica)?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .is_some_and(|c| {
                matches!(
                    c.node.responder,
                    Some(otrios_interfaces::scene::TouchResponder::Activate(
                        otrios_interfaces::scene::Action::ToggleFocus
                    ))
                )
            })
        {
            "switch"
        } else {
            "button"
        }
        .into())
    }
    pub fn control_checked(&mut self, id: u32) -> Result<bool, String> {
        Ok(self
            .shell
            .scene(&self.replica)?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .and_then(|c| c.node.semantics.as_ref())
            .is_some_and(|s| s.checked))
    }
}

/// Has no CRDT, networking, storage, navigation or app-owned text.
#[wasm_bindgen]
#[derive(Default)]
pub struct MotionPresenter {
    inner: otrios_core::dashboard::motion::Presenter,
}
#[wasm_bindgen]
impl MotionPresenter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn vertices(
        &mut self,
        three_d: bool,
        playing: bool,
        speed: f32,
        count: u16,
        now: f64,
        updates: f64,
    ) -> Result<Vec<f32>, String> {
        let i = otrios_core::dashboard::motion::Intent {
            playing,
            speed,
            count,
        };
        i.validate()?;
        Ok(self
            .inner
            .mesh(three_d, i, now, updates.max(0.) as u64)
            .vertices)
    }
    pub fn suspend(&mut self) {
        self.inner.suspend();
    }
    pub fn hud_revision(&self) -> f64 {
        self.inner.hud_revision() as f64
    }
    pub fn hud(&self) -> Vec<f32> {
        self.inner.hud().to_vec()
    }
    pub fn stats(&self) -> String {
        self.inner.stats()
    }
}

#[cfg(feature = "canvas")]
mod canvas_web;
