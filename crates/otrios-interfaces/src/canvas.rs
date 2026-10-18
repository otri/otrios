// SPDX-License-Identifier: Apache-2.0
//! Opt-in Canvas prototype contracts. FlexBuffers transport; not a stable ABI.
use crate::layout::Value;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: u16,
    pub name: String,
    /// The variant defines this field's fixed type; runtime bounds apply to its payload.
    pub default: Value,
    /// Client command/sync permission. The authority's admitted schema controls this flag.
    pub writable: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSchema {
    pub id: String,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    /// Preserve the declared typed value for text, booleans, colors and bounded series.
    Identity,
    NumberText {
        prefix: String,
        suffix: String,
        scale: f32,
        offset: f32,
        precision: u8,
    },
    ChooseText {
        zero: String,
        nonzero: String,
    },
    ChooseColor {
        zero: [u8; 4],
        nonzero: [u8; 4],
    },
    /// Normalized register 0..1000 is one clockwise turn, starting at twelve o'clock.
    TurnX {
        center: f32,
        radius: f32,
    },
    TurnY {
        center: f32,
        radius: f32,
    },
    /// Local input-method label. Its selector uses reserved field 0.
    KeyboardVisible,
    KeyboardLabel {
        key: u16,
    },
    WindowSignal {
        seconds: u32,
    },
    Number {
        scale: f32,
        offset: f32,
    },
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
pub enum Operation {
    Set,
    Toggle,
    Cycle(Vec<u16>),
    Focus,
    Key(u16),
    Back,
    Stress,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handler {
    pub control: u8,
    pub event: u16,
    pub kind: GestureKind,
    pub operation: Operation,
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
pub enum Presentation {
    View {
        frame: [f32; 4],
    },
    Motion {
        three_d: bool,
        load: u16,
        playing: u16,
        speed: u16,
        count: u16,
    },
    Editor {
        fields: Vec<u16>,
    },
    TouchPad {
        control: u8,
    },
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
    pub presentation: Option<Presentation>,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub app: App,
    pub ui_hash: Vec<u8>,
    pub private: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: String,
    pub apps: Vec<CatalogEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Mount {
        epoch: u32,
        ui_ref: String,
        state_ref: String,
        catalog_hash: Vec<u8>,
        known_code: Vec<Vec<u8>>,
    },
    /// Activate an already-qualified candidate; no new app Sync precedes this.
    AcceptMount {
        epoch: u32,
    },
    /// Reject a staged candidate while retaining the previous active lease.
    CancelMount {
        epoch: u32,
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
    View {
        epoch: u32,
        bytes: Vec<u8>,
    },
    Mounted {
        epoch: u32,
        ui: Vec<u8>,
        ui_hash: Vec<u8>,
        code: Vec<u8>,
        /// Complete authoritative state used to qualify a candidate before activation.
        state: Vec<u8>,
    },
    Sync {
        epoch: u32,
        bytes: Vec<u8>,
    },
    Snapshot {
        epoch: u32,
        values: Vec<(u16, Value)>,
    },
    AccessRequired {
        epoch: u32,
    },
    Error {
        epoch: u32,
        message: String,
    },
}
