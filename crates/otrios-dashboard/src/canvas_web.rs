// SPDX-License-Identifier: Apache-2.0
//! Browser adapter; Canvas state/guest memory live only in the worker's RAM.
use otrios_core::{canvas::Client, dashboard::render_scene};
use otrios_interfaces::scene::{PointerEvent, PointerPhase, Primitive};
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
#[derive(Default)]
pub struct CanvasApp {
    inner: Client,
}
#[wasm_bindgen]
impl CanvasApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn open(&mut self, ui: &str, state: &str) {
        self.inner.open(ui, state)
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.inner.outgoing()
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool, String> {
        self.inner.receive(bytes)
    }
    pub fn epoch(&self) -> u32 {
        self.inner.epoch()
    }
    pub fn ui_ref(&self) -> String {
        self.inner.ui_ref()
    }
    pub fn state_ref(&self) -> String {
        self.inner.state_ref()
    }
    pub fn title(&self) -> String {
        self.inner.title()
    }
    pub fn confirmed(&mut self) -> bool {
        self.inner.confirmed()
    }
    pub fn vertices(&self) -> Result<Vec<f32>, String> {
        Ok(render_scene(self.inner.scene()?.as_ref()).vertices)
    }
    pub fn text(&self) -> Result<String, String> {
        fn visit(n: &otrios_interfaces::scene::SceneNode, out: &mut Vec<String>) {
            if let Primitive::Text { value, .. } = &n.primitive {
                out.push(value.clone())
            }
            for c in &n.children {
                visit(c, out)
            }
        }
        let mut out = vec![];
        visit(self.inner.scene()?.root(), &mut out);
        Ok(out.join(". "))
    }
    pub fn controls(&self) -> Result<Vec<u32>, String> {
        Ok(self
            .inner
            .scene()?
            .controls()
            .iter()
            .map(|c| c.node.id)
            .collect())
    }
    pub fn label(&self, id: u32) -> Result<String, String> {
        Ok(self
            .inner
            .scene()?
            .controls()
            .iter()
            .find(|c| c.node.id == id)
            .and_then(|c| c.node.semantics.as_ref())
            .map(|s| s.label.clone())
            .unwrap_or_default())
    }
    pub fn bounds(&self, id: u32) -> Result<Vec<f32>, String> {
        Ok(self
            .inner
            .scene()?
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
    pub fn activate(&mut self, epoch: u32, id: u32) -> Result<bool, String> {
        self.inner.activate(epoch, id)
    }
    pub fn pointer(
        &mut self,
        epoch: u32,
        phase: u8,
        id: u32,
        x: f32,
        y: f32,
    ) -> Result<bool, String> {
        let phase = match phase {
            0 => PointerPhase::Down,
            1 => PointerPhase::Move,
            2 => PointerPhase::Up,
            3 => PointerPhase::Cancel,
            _ => return Err("Invalid pointer phase".into()),
        };
        self.inner.pointer(
            epoch,
            PointerEvent {
                id,
                phase,
                position: [x, y],
            },
        )
    }
}
