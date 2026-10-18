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
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ValueType {
    Number,
    Boolean,
    Text,
    Color,
    Series,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Binding {
    Enabled,
    Title,
    TouchLabel,
    Samples,
    HasSamples,
    LastSample,
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
    DragControl(u8),
    Bubble,
    Consume,
    ToggleFocus,
    ResetControls,
    LayoutLab,
    Motion(u8),
    Stress,
    Open(crate::scene::Screen),
    Back,
    Home,
    FocusField(u8),
    KeyboardKey(u16),
    KeyParameter(u16),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Semantics {
    pub label: Expr,
    pub checked: Expr,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateNode {
    pub id: u16,
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
    pub templates: Vec<Template>,
    pub root: Instance,
}
