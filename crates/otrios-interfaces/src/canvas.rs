// SPDX-License-Identifier: Apache-2.0
//! Opt-in Canvas prototype contracts. FlexBuffers transport; not a stable ABI.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: u16,
    pub name: String,
    pub default: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSchema {
    pub id: String,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    Number { scale: f32, offset: f32 },
    Percent,
    EnabledText,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    pub local: u16,
    pub field: u16,
    pub projection: Projection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GestureKind {
    Slider,
    Dial,
    Pad,
    Activate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handler {
    pub control: u8,
    pub event: u16,
    pub kind: GestureKind,
    pub fields: Vec<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Navigation {
    pub action: u8,
    pub ui_ref: String,
    pub state_ref: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorRef {
    pub sha256: Vec<u8>,
    pub abi: u32,
    pub writes: Vec<u16>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    pub format: String,
    pub ui_ref: String,
    pub title: String,
    pub state_schema: StateSchema,
    pub layout: Vec<u8>,
    pub selectors: Vec<Selector>,
    pub handlers: Vec<Handler>,
    pub navigation: Vec<Navigation>,
    pub behavior: Option<BehaviorRef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct App {
    pub id: String,
    pub title: String,
    pub ui_ref: String,
    pub state_ref: String,
    pub status: String,
    pub dependencies: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Mount {
        epoch: u32,
        ui_ref: String,
        state_ref: String,
        known_code: Vec<Vec<u8>>,
    },
    Sync {
        epoch: u32,
        bytes: Vec<u8>,
    },
    Leave {
        epoch: u32,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    Mounted {
        epoch: u32,
        ui: Vec<u8>,
        ui_hash: Vec<u8>,
        code: Vec<u8>,
    },
    Sync {
        epoch: u32,
        bytes: Vec<u8>,
    },
    Error {
        epoch: u32,
        message: String,
    },
}
