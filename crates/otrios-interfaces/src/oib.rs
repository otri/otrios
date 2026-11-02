// SPDX-License-Identifier: Apache-2.0
//! RFC 0029 O1: immutable edit surface and causal property operations.
use crate::layout::Value;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "otrios-oib-v1";
pub const FEATURE: &str = "otrios-oib-v1";
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
