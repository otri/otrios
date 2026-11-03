// SPDX-License-Identifier: Apache-2.0
//! RFC 0029 O1: immutable edit surface and causal property operations.
use crate::layout::Value;
use serde::{Deserialize, Serialize};

/// Additive RFC 0035 D1 proposal; not attached to the v26 seed or state wire.
pub mod designer;

pub const FORMAT: &str = "otrios-oib-v1";
pub const FEATURE: &str = "otrios-oib-v1";
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
    pub main_oib: u64,
    /// O1 uses the existing declarative event interpreter; no guest imports.
    pub main_class: String,
    pub components: Vec<Object>,
    pub instances: Vec<Object>,
    pub cells: Vec<Cell>,
    pub inspector_viewport: u16,
    pub inspector_content: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_surface: Option<InstanceSurface>,
}
/// Signed factories reuse admitted nodes, bindings, properties and event contracts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceSurface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutSurface>,
    /// Explicit one-time upgrade from the exact O1 UI; not a general seed rebase.
    pub previous_ui_sha256: Vec<u8>,
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
    /// Exact admitted predecessors permitted to append component layout cells.
    pub previous_ui_sha256: Vec<Vec<u8>>,
    /// Component numeric property controlling the preview row pitch/content extent.
    pub preview_pitch_cell: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prototype {
    pub instance: u64,
    pub preview_root: u16,
    pub inspector_root: u16,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstanceOperation {
    /// Restore authored factories/bindings, giving deleted objects fresh IDs.
    /// Optionally reset all UI properties.
    /// This explicit owner command observes current heads at authority acceptance.
    Restore {
        defaults: bool,
    },
    Instantiate {
        component: u64,
        name: Option<String>,
    },
    Duplicate {
        instance: u64,
        name: Option<String>,
    },
    Remove {
        instance: u64,
        observed: Vec<u8>,
    },
}
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
    pub cell: u64,
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
