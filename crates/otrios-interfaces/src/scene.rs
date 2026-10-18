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
    /// Uniformly scale child logical coordinates inside a clipped frame.
    Viewport {
        scale: f32,
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
    /// Projected triangle geometry; bounded and clipped like other primitives.
    Triangles {
        points: Vec<[f32; 2]>,
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
    AppEvent(u8),
    AppNavigate(u8),
    ToggleFocus,
    ResetControls,
    LayoutLab,
    Control(u8),
    Motion(u8),
    Stress,
    Open(Screen),
    Back,
    Home,
    FocusField(u8),
    KeyboardKey(u16),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "layout", derive(serde::Serialize, serde::Deserialize))]
pub enum Screen {
    Home,
    Monitoring,
    Controls,
    Keyboard,
    Animation,
    ThreeD,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchResponder {
    Bubble,
    Consume,
    /// Capture on press; emit only on release inside this responder's subtree.
    Activate(Action),
    /// Capture and deliver local positions throughout the gesture, including cancel.
    Drag(Action),
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
