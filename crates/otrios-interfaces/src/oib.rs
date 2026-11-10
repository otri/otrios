// SPDX-License-Identifier: Apache-2.0
//! RFC 0029 O1: immutable edit surface and causal property operations.
use crate::layout::Value;
use serde::{Deserialize, Serialize};

/// Bounded D2 authoring records; admitted through the existing instance request.
pub mod authoring;
/// RFC 0036 component behavior ABI records and canonical byte codec.
pub mod component_behavior;
/// RFC 0035 D1 signed metadata and stable occurrence/property identities.
pub mod designer;
pub mod graph;
pub mod library;

pub const FORMAT: &str = "otrios-oib-v2";
pub const FEATURE: &str = "otrios-oib-v2";
pub const INSTANCES_FEATURE: &str = "otrios-oib-instances-v1";
pub const LAYOUT_FEATURE: &str = "otrios-oib-layout-v1";
pub const RESTORE_FEATURE: &str = "otrios-oib-restore-v1";
pub const AUTHORED_DEFAULTS_FEATURE: &str = "otrios-oib-authored-defaults-v1";
/// Resident draft/active objects, independent of lifetime creation count.
pub const MAX_ALLOCATIONS: usize = 10;
pub const MAX_LIVE_INSTANCES: usize = 6;
pub const MAX_CELLS: usize = 64;
pub const MAX_CHANGES: usize = 16;
pub const MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub format: String,
    /// Canonical authored source. Everything below is disposable compiler output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<graph::Document>,
    /// Immutable verified import bodies carried with the exact app dependency
    /// lock. They never become app-authored local definition copies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_libraries: Vec<graph::ResolvedLibrary>,
    /// Disposable projection state; preset IDs and real instance IDs are
    /// independent domains even when their numeric values coincide.
    #[serde(skip)]
    pub expanded: bool,
    /// Signed hierarchy/inspector metadata for Designer applications.
    #[serde(skip)]
    pub designer: Option<designer::Document>,
    #[serde(skip)]
    pub main_oib: u64,
    /// O1 uses the existing declarative event interpreter; no guest imports.
    #[serde(skip)]
    pub main_class: String,
    #[serde(skip)]
    pub components: Vec<Object>,
    #[serde(skip)]
    pub instances: Vec<Object>,
    #[serde(skip)]
    pub cells: Vec<Cell>,
    #[serde(skip)]
    pub inspector_viewport: u16,
    #[serde(skip)]
    pub inspector_content: [f32; 2],
    #[serde(skip)]
    pub instance_surface: Option<InstanceSurface>,
}
/// Signed factories reuse admitted nodes, bindings, properties and event contracts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceSurface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutSurface>,
    pub preview_viewport: u16,
    pub preview_stride: f32,
    pub inspector_start: f32,
    pub inspector_stride: f32,
    pub prototypes: Vec<Prototype>,
    pub node_base: u16,
    pub node_stride: u16,
}
/// Component-owned geometry remains bounded by signed mappings and normal admission.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutSurface {
    /// Component numeric property controlling the preview row pitch/content extent.
    pub preview_pitch_cell: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prototype {
    pub instance: u64,
    /// Signed palette factory with no initial resident. Inserting always uses
    /// a fresh allocated identity; restoring defaults does not instantiate it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub palette_only: bool,
    pub preview_root: u16,
    pub inspector_root: Option<u16>,
    pub nodes: Vec<u16>,
}
/// Authority-owned resident descriptor. Removed objects are reclaimed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub id: u64,
    pub name: String,
    pub component: u64,
    /// Signed seed instance supplying the admitted data binding and template.
    pub prototype: u64,
    /// Reusable renderer slot, never used as an object/property identity.
    pub slot: u16,
}
/// Every structural state request carries this one bounded edit-batch contract.
pub type InstanceOperation = authoring::EditBatch;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceAction {
    Inspect,
    RemovePending,
    Add,
    Duplicate,
    Remove,
    Restore,
    ResetDefaults,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub id: u64,
    pub name: String,
    /// Components have no definition; instances refer to a component ID.
    pub component: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub id: u64,
    pub owner: u64,
    pub name: String,
    pub default: Value,
    /// App-authored instance override, revealed when a live override is cleared.
    /// Shared component properties still apply where this is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored_override: Option<Value>,
    /// App-authored inspector suggestions; the typed API also accepts other valid values.
    pub choices: Vec<Value>,
    /// An instance cell inherits this component cell until overridden.
    pub inherit: Option<u64>,
    pub targets: Vec<Target>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Property {
    Text,
    Color,
    Width,
    Y,
    SemanticLabel,
    X,
    Height,
    /// Raw frame width; legacy Width retains its 240-pixel container minimum.
    FrameWidth,
    /// Added after absolute Y assignments, independent of property ordering.
    OffsetY,
    Visible,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Admitted mapping from stable OIB cells to activation-local layout nodes.
    pub node: u16,
    pub property: Property,
    pub scale: f32,
    pub offset: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// Complete semantic identity. Activation-local cell indices never cross
    /// the state protocol or identify durable property registers.
    pub address: designer::PropertyAddress,
    /// None is a causal clear of an instance override.
    pub value: Option<Value>,
    /// Canonical leaf snapshot actually observed, including causal context.
    pub observed: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorAction {
    Cycle,
    Clear,
    Resolve,
}
