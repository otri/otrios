// SPDX-License-Identifier: Apache-2.0
//! Versioned FlexBuffers layout contract. Serde connects typed Rust values directly to binary.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Number(f32),
    Boolean(bool),
    Text(String),
    Color([u8; 4]),
    Series(Vec<f32>),
    /// Explicit normalized time x (0..1) and signal value y (0..100).
    TimedSeries(Vec<[f32; 2]>),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ValueType {
    Number,
    Boolean,
    Text,
    Color,
    Series,
    TimedSeries,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Binding {
    Local(u16),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(Value),
    Parameter(u16),
    State(Binding),
    Width,
    Height,
    Add(Box<Expr>, Box<Expr>),
    Subtract(Box<Expr>, Box<Expr>),
    Multiply(Box<Expr>, Box<Expr>),
    Select {
        condition: Box<Expr>,
        yes: Box<Expr>,
        no: Box<Expr>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub key: u16,
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub key: u16,
    pub name: String,
    pub value_type: ValueType,
    pub default: Expr,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Canvas {
        clip: bool,
    },
    Text {
        value: Expr,
        scale: Expr,
        color: Expr,
    },
    /// Rounded rectangle with local superelliptic corner arcs; see scene::Primitive.
    Superellipse {
        radius: Expr,
        exponent: f32,
        color: Expr,
    },
    Circle {
        color: Expr,
    },
    SeriesPath {
        series: Expr,
        width: f32,
        color: Expr,
    },
    /// Time-spaced points; the horizontal axis is independent of event count.
    TimedSeriesPath {
        series: Expr,
        width: f32,
        color: Expr,
    },
    Slot,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Responder {
    /// App-scoped event/navigation indices, resolved by the Canvas mount.
    AppEvent {
        control: u8,
        drag: bool,
    },
    AppNavigate(u8),
    Bubble,
    Consume,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Semantics {
    pub label: Expr,
    pub checked: Expr,
}
/// One axis of a component's parent-relative layout, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AxisResize {
    Fixed,
    /// One pin preserves length; two pins stretch between the edge gaps.
    Pins {
        leading: Option<f32>,
        trailing: Option<f32>,
    },
    Center {
        offset: f32,
    },
    /// Distribute parent delta among the selected design segments.
    Springs {
        leading: bool,
        length: bool,
        trailing: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeRule {
    pub reference_size: [f32; 2],
    pub horizontal: AxisResize,
    pub vertical: AxisResize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateNode {
    pub id: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resize: Option<ResizeRule>,
    pub frame: [Expr; 4],
    pub visible: Expr,
    pub shape: Shape,
    pub responder: Option<Responder>,
    pub semantics: Option<Semantics>,
    pub children: Vec<TemplateNode>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub id: u16,
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub root: TemplateNode,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: u32,
    pub template: u16,
    pub frame: [f32; 4],
    pub properties: Vec<Property>,
    pub children: Vec<Instance>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<Composition>,
    pub templates: Vec<Template>,
    pub root: Instance,
}

/// Device-local arrangement of the same component instances. No replicated state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub wide_min_width: f32,
    pub wide: FlowPlan,
    pub tall: FlowPlan,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowPlan {
    /// Left, top, right, bottom in logical pixels.
    pub insets: [f32; 4],
    pub content: Flow,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Alignment {
    Start,
    Center,
    End,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    /// Flexible axes may grow or shrink from preferred size down to minimum size.
    pub flexible: [bool; 2],
    pub kind: FlowKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FlowKind {
    Item {
        node: u16,
        preferred: [f32; 2],
        minimum: [f32; 2],
    },
    Group {
        /// 0 = row, 1 = column. Children retain their compiled tree order/identity.
        axis: u8,
        gap: f32,
        alignment: Alignment,
        children: Vec<Flow>,
    },
}
