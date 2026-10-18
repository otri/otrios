// SPDX-License-Identifier: Apache-2.0
//! Thin host adapter. The LGPL runtime owns state, snapshots, input and rendering.
use otrios_core::dashboard::{Replica, render};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Dashboard {
    replica: Replica,
}
#[wasm_bindgen]
impl Dashboard {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Dashboard, String> {
        Ok(Self {
            replica: Replica::new()?,
        })
    }
    pub fn load(bytes: &[u8]) -> Result<Dashboard, String> {
        Ok(Self {
            replica: Replica::load(bytes)?,
        })
    }
    pub fn document(&mut self) -> Vec<u8> {
        self.replica.save()
    }
    pub fn merge(&mut self, bytes: &[u8]) -> Result<bool, String> {
        self.replica.merge(bytes)
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
    pub fn snapshot(&self) -> Vec<u8> {
        self.replica.snapshot().bytes().to_vec()
    }
    pub fn snapshot_id(&self) -> String {
        self.replica.snapshot().hex_id()
    }
    pub fn switch_bounds(&self) -> Vec<u32> {
        self.replica
            .snapshot()
            .view()
            .nodes
            .iter()
            .find_map(|n| match n.content {
                otrios_interfaces::ViewContent::Switch { .. } => Some(n.bounds.to_vec()),
                _ => None,
            })
            .unwrap_or_default()
    }
    pub fn enabled(&self) -> bool {
        self.replica
            .snapshot()
            .view()
            .nodes
            .iter()
            .find_map(|n| match n.content {
                otrios_interfaces::ViewContent::Switch { enabled, .. } => Some(enabled),
                _ => None,
            })
            .unwrap_or(false)
    }
}
