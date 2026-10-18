// SPDX-License-Identifier: Apache-2.0
//! Experimental retained scene contract. Components expand into these primitives.
pub type NodeId = u32;
pub type Color = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    /// Child frames use this canvas's local coordinates. Clip is rectangular.
    Canvas {
        clip: bool,
    },
    Text {
        value: String,
        scale: u32,
        color: Color,
    },
    /// Rectangle with superelliptic corners; exponent 2 is circular rounding.
    Superellipse {
        radius: f32,
        exponent: f32,
        color: Color,
    },
    Circle {
        color: Color,
    },
    Path {
        points: Vec<[f32; 2]>,
        width: f32,
        color: Color,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    ToggleFocus,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchResponder {
    Bubble,
    Consume,
    /// Capture on press; emit only on release inside this responder's subtree.
    Activate(Action),
}
#[derive(Clone, Debug, PartialEq)]
pub struct SwitchSemantics {
    pub label: String,
    pub checked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    pub id: NodeId,
    pub frame: Rect,
    pub primitive: Primitive,
    pub children: Vec<SceneNode>,
    pub responder: Option<TouchResponder>,
    pub semantics: Option<SwitchSemantics>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub id: u32,
    pub phase: PointerPhase,
    /// Root canvas coordinates, including positions outside the canvas during capture.
    pub position: [f32; 2],
}
