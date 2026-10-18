// SPDX-License-Identifier: Apache-2.0
//! Thin host adapter. The LGPL runtime owns state, snapshots, input and rendering.
use otrios_core::dashboard::{Replica, SyncSession, input::InputRouter, render};
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Dashboard {
    replica: Replica,
    session: SyncSession,
    input: InputRouter,
}
#[wasm_bindgen]
impl Dashboard {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Dashboard, String> {
        Ok(Self {
            replica: Replica::new()?,
            session: SyncSession::default(),
            input: InputRouter::default(),
        })
    }
    pub fn load(bytes: &[u8]) -> Result<Dashboard, String> {
        Ok(Self {
            replica: Replica::load(bytes)?,
            session: SyncSession::default(),
            input: InputRouter::default(),
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
    pub fn vertices(&self) -> Vec<f32> {
        render(self.replica.snapshot()).vertices
    }
    /// Assistive description derives from the same primitive scene as the pixels.
    pub fn scene_text(&self) -> String {
        fn gather(node: &otrios_interfaces::scene::SceneNode, values: &mut Vec<String>) {
            if let otrios_interfaces::scene::Primitive::Text { value, .. } = &node.primitive {
                values.push(value.clone());
            }
            for child in &node.children {
                gather(child, values);
            }
        }
        let mut values = Vec::new();
        gather(self.replica.snapshot().scene().root(), &mut values);
        values.join(". ")
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
    pub fn pointer(&mut self, phase: u8, id: u32, x: f32, y: f32) -> Result<bool, String> {
        let phase = match phase {
            0 => PointerPhase::Down,
            1 => PointerPhase::Move,
            2 => PointerPhase::Up,
            3 => PointerPhase::Cancel,
            _ => return Err("Invalid pointer phase".into()),
        };
        let result = self.input.dispatch(
            self.replica.snapshot().scene().clone(),
            PointerEvent {
                id,
                phase,
                position: [x, y],
            },
        );
        self.replica.apply_action(result.action)
    }
    pub fn activate(&mut self, id: u32) -> Result<bool, String> {
        let result = InputRouter::activate(self.replica.snapshot().scene(), id);
        self.replica.apply_action(result.action)
    }
    pub fn control_ids(&self) -> Vec<u32> {
        self.replica
            .snapshot()
            .scene()
            .controls()
            .iter()
            .map(|c| c.node.id)
            .collect()
    }
    pub fn control_bounds(&self, id: u32) -> Vec<f32> {
        self.replica
            .snapshot()
            .scene()
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .map(|c| {
                vec![
                    c.origin[0],
                    c.origin[1],
                    c.node.frame.width,
                    c.node.frame.height,
                ]
            })
            .unwrap_or_default()
    }
    pub fn control_label(&self, id: u32) -> String {
        self.replica
            .snapshot()
            .scene()
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .and_then(|c| c.node.semantics.as_ref())
            .map(|s| s.label.clone())
            .unwrap_or_default()
    }
    pub fn control_checked(&self, id: u32) -> bool {
        self.replica
            .snapshot()
            .scene()
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .and_then(|c| c.node.semantics.as_ref())
            .is_some_and(|s| s.checked)
    }
}
