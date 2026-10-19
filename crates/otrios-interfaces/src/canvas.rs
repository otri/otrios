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
    /// Local browser session controls; never an authorization grant or shared state.
    SessionControls,
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
    /// Request local cookie-session revocation. Never creates a state mutation.
    SignOut,
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
        /// RAM-only nonce for durable deduplication; never an access grant.
        writer: Vec<u8>,
        resume: u64,
        /// Opt in to Blob response fields. Missing/false preserves legacy vectors.
        #[serde(default, skip_serializing_if = "is_false")]
        binary: bool,
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
        #[serde(deserialize_with = "opaque_bytes")]
        bytes: Vec<u8>,
    },
    Mounted {
        epoch: u32,
        #[serde(deserialize_with = "opaque_bytes")]
        ui: Vec<u8>,
        #[serde(deserialize_with = "opaque_bytes")]
        ui_hash: Vec<u8>,
        #[serde(deserialize_with = "opaque_bytes")]
        code: Vec<u8>,
        /// Complete authoritative state used to qualify a candidate before activation.
        #[serde(deserialize_with = "opaque_bytes")]
        state: Vec<u8>,
    },
    Sync {
        epoch: u32,
        #[serde(deserialize_with = "opaque_bytes")]
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

/// Versioned inner state protocol. Client clocks/trees are never accepted as writes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StateRequest {
    Hello {
        writer: Vec<u8>,
        resume: u64,
    },
    Write {
        sequence: u64,
        values: Vec<(u16, Value)>,
    },
    Ack {
        authority: u64,
        lease: u64,
        sequence: u64,
        root: Vec<u8>,
    },
    Resync,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StateResponse {
    Mount {
        authority: u64,
        lease: u64,
        #[serde(deserialize_with = "opaque_bytes")]
        frame: Vec<u8>,
    },
    Update {
        #[serde(deserialize_with = "opaque_bytes")]
        frame: Vec<u8>,
    },
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Accept both legacy integer vectors and negotiated opaque binary fields.
fn opaque_bytes<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl<'de> serde::de::Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("binary bytes or a legacy byte vector")
        }
        fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Vec<u8>, E> {
            Ok(value.to_vec())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut result = Vec::new();
            while let Some(byte) = seq.next_element::<u8>()? {
                result.push(byte);
            }
            Ok(result)
        }
    }
    de.deserialize_any(Bytes)
}
