// SPDX-License-Identifier: LGPL-3.0-only
//! Primitive construction helpers for tests and native callers.
//! Live components are defined by the FlexBuffers layout asset.
use otrios_interfaces::scene::*;
pub const BG: Color = [16, 28, 41, 255];
pub const CARD: Color = [27, 44, 61, 255];
pub const WHITE: Color = [239, 244, 236, 255];
pub const MUTED: Color = [150, 171, 185, 255];
pub const ACCENT: Color = [185, 238, 143, 255];
pub fn node(id: NodeId, frame: Rect, primitive: Primitive) -> SceneNode {
    SceneNode {
        id,
        frame,
        primitive,
        children: vec![],
        responder: None,
        semantics: None,
    }
}
pub fn canvas(id: NodeId, frame: Rect, children: Vec<SceneNode>) -> SceneNode {
    let mut n = node(id, frame, Primitive::Canvas { clip: true });
    n.children = children;
    n
}
pub fn background(id: NodeId, w: f32, h: f32, color: Color, radius: f32) -> SceneNode {
    node(
        id,
        Rect::new(0., 0., w, h),
        Primitive::Superellipse {
            color,
            radius,
            exponent: 4.,
        },
    )
}
pub fn text(id: NodeId, frame: Rect, value: &str, scale: u32, color: Color) -> SceneNode {
    node(
        id,
        frame,
        Primitive::Text {
            value: value.into(),
            scale,
            color,
        },
    )
}
/// Reserve id..id+999 for one component; descendants stay stable across state edits.
pub fn panel(id: NodeId, frame: Rect, color: Color, children: Vec<SceneNode>) -> SceneNode {
    let mut all = vec![background(id + 1, frame.width, frame.height, color, 16.)];
    all.extend(children);
    canvas(id, frame, all)
}
