// SPDX-License-Identifier: Apache-2.0
//! Reusable, constraint-free layout policy. Fixed lengths and Free coordinates
//! use existing frame expressions; they are not duplicated here.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sizing {
    #[default]
    Fixed,
    Hug,
    Fill,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisSizing {
    pub mode: Sizing,
    pub min: f32,
    pub max: Option<f32>,
}
impl Default for AxisSizing {
    fn default() -> Self {
        Self {
            mode: Sizing::Fixed,
            min: 0.,
            max: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    #[default]
    InFlow,
    Free,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Horizontal,
    #[default]
    Vertical,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Packing {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossAlignment {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoLayout {
    pub direction: Direction,
    /// Left, top, right, bottom in logical units.
    pub padding: [f32; 4],
    pub gap: f32,
    pub packing: Packing,
    pub alignment: CrossAlignment,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLayout {
    pub width: AxisSizing,
    pub height: AxisSizing,
    pub placement: Placement,
    pub container: Option<AutoLayout>,
}
