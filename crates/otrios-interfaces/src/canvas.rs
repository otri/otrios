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
    /// Mount-local reorder label, independent of the document.
    ReorderLabel,
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
    /// Local shared-room UI command. The server independently authorizes wire commands.
    Chat(u16),
    Set,
    Toggle,
    Cycle(Vec<u16>),
    Focus,
    Key(u16),
    Back,
    Stress,
    /// Request local cookie-session revocation. Never creates a state mutation.
    SignOut,
    /// Toggle this mount's reorder interaction; never writes state.
    EditOrder,
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
    /// A named child edge owned by this UI; ordinary links leave it unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub ui_ref: String,
    /// Empty resolves the destination app's admitted State reference; nonempty is an explicit binding.
    pub state_ref: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorRef {
    pub sha256: Vec<u8>,
    pub abi: u32,
    pub writes: Vec<u16>,
}
/// Stable field identity mapped to a node within this UI activation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderItem {
    pub node: u16,
    pub field: u16,
}
/// Immutable UTF-8 resource embedded in an admitted UI. LZ4 block bytes, without
/// a size prefix; the declared size is checked before decoding. SHA-256 covers
/// the original UTF-8 bytes. UI v4, independent of replicated state protocols.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextResource {
    pub decoded_bytes: u32,
    pub sha256: Vec<u8>,
    pub lz4: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Presentation {
    /// Dynamic shared-room blocks and local composer, backed by session channel 3.
    Chat,
    /// Read-only, wrapped document; source is not stored in scene text nodes.
    TextDocument {
        viewport: u16,
        source: TextResource,
        scale: u8,
        color: [u8; 4],
    },
    /// Mount-local text-only viewport. UI format v3; offsets are never state.
    TextScroll {
        viewport: u16,
        content: [f32; 2],
    },
    Reorder {
        items: Vec<OrderItem>,
    },
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
/// Canonical app/screen path, without a leading slash. Every segment is app-owned.
pub fn valid_destination(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 64
        && path.split('/').all(|segment| {
            !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
        })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct App {
    /// Stable package/screen identity, independent of its navigation path.
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
    /// Resolved from app-owned named navigation edges; the entry screen uses "".
    pub path: String,
    pub ui_hash: Vec<u8>,
    pub private: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: String,
    pub apps: Vec<CatalogEntry>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    Mount {
        epoch: u32,
        ui_ref: String,
        state_ref: String,
        catalog_hash: Vec<u8>,
        /// One cached UI digest for this target. Never an access grant.
        known_ui: Option<Vec<u8>>,
        known_code: Vec<Vec<u8>>,
        /// RAM-only nonce for durable deduplication; never an access grant.
        writer: Vec<u8>,
        resume: u64,
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
#[derive(Clone, Debug, PartialEq)]
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
    AccessRequired {
        epoch: u32,
    },
    Error {
        epoch: u32,
        message: String,
    },
}

/// Versioned inner state protocol. Client clocks/trees are never accepted as writes.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub enum StateResponse {
    Mount {
        authority: u64,
        lease: u64,
        frame: Vec<u8>,
    },
    Update {
        frame: Vec<u8>,
    },
}
