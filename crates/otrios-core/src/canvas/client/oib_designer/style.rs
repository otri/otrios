// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded decorative surfaces matching the authored 1.5-unit outline.
use crate::dashboard::components::{background, canvas};
use otrios_interfaces::scene::{Color, Primitive, Rect, SceneNode, TouchResponder};

const OUTLINE: f32 = 1.5;
const MAX_SIZE: f32 = 8192.;

fn dimension(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(1., MAX_SIZE)
    } else {
        1.
    }
}

fn radius(value: f32, width: f32, height: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0., width.min(height) / 2.)
    } else {
        0.
    }
}

fn inset(width: f32, height: f32) -> f32 {
    // Sub-stroke geometry still needs a positive fill frame for Scene admission.
    OUTLINE.min(width.min(height) / 4.)
}

fn inner_frame(width: f32, height: f32) -> Rect {
    let edge = inset(width, height);
    Rect::new(edge, edge, width - 2. * edge, height - 2. * edge)
}

/// IDs are supplied by the caller's namespace and must be distinct.
/// Construction normalizes invalid dimensions; fallible resizing rejects them.
pub(super) fn surface_with_outline(
    ids: [u32; 3],
    width: f32,
    height: f32,
    fill: Color,
    outline: Color,
    corner_radius: f32,
) -> SceneNode {
    let (width, height) = (dimension(width), dimension(height));
    let corner_radius = radius(corner_radius, width, height);
    let mut outer = background(ids[1], width, height, outline, corner_radius);
    let frame = inner_frame(width, height);
    let mut inner = background(
        ids[2],
        frame.width,
        frame.height,
        fill,
        radius(corner_radius - frame.x, frame.width, frame.height),
    );
    inner.frame = frame;
    outer.responder = Some(TouchResponder::PassThrough);
    inner.responder = Some(TouchResponder::PassThrough);
    let mut node = canvas(ids[0], Rect::new(0., 0., width, height), vec![outer, inner]);
    node.responder = Some(TouchResponder::PassThrough);
    node
}

fn valid_frame(frame: Rect) -> bool {
    [frame.x, frame.y, frame.width, frame.height]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= MAX_SIZE)
        && frame.width > 0.
        && frame.height > 0.
}

fn plain(node: &SceneNode) -> bool {
    valid_frame(node.frame)
        && node.children.is_empty()
        && matches!(node.primitive, Primitive::Superellipse { radius, exponent, .. }
            if radius.is_finite() && radius >= 0.
                && radius <= node.frame.width.min(node.frame.height) / 2.
                && exponent.is_finite() && (2. ..=8.).contains(&exponent))
}

/// Recognize only the generated two-layer shape or a leaf navigation background.
/// Validate the entire shape before taking any mutable references.
fn validate(node: &SceneNode) -> Result<(), String> {
    if plain(node) {
        return Ok(());
    }
    let [outer, inner] = node.children.as_slice() else {
        return Err("Expected a decorative surface or plain superellipse".into());
    };
    let frame = node.frame;
    let expected_inner = inner_frame(frame.width, frame.height);
    let decorative =
        |n: &SceneNode| n.responder == Some(TouchResponder::PassThrough) && n.semantics.is_none();
    if !valid_frame(frame)
        || node.primitive != (Primitive::Canvas { clip: true })
        || !decorative(node)
        || !plain(outer)
        || !plain(inner)
        || !decorative(outer)
        || !decorative(inner)
        || node.id == outer.id
        || node.id == inner.id
        || outer.id == inner.id
        || outer.frame != Rect::new(0., 0., frame.width, frame.height)
        || inner.frame != expected_inner
    {
        return Err("Unexpected decorative surface structure".into());
    }
    let Primitive::Superellipse {
        radius: outer_radius,
        exponent: 4.,
        color: _,
    } = outer.primitive
    else {
        return Err("Unexpected decorative surface outline".into());
    };
    if !matches!(inner.primitive, Primitive::Superellipse { radius: r, exponent: 4., .. }
        if r == radius(outer_radius - expected_inner.x, expected_inner.width, expected_inner.height))
    {
        return Err("Unexpected decorative surface inset".into());
    }
    Ok(())
}

pub(super) fn resize_surface(node: &mut SceneNode, width: f32, height: f32) -> Result<(), String> {
    if !valid_frame(Rect::new(0., 0., width, height)) {
        return Err("Surface dimensions must be finite, positive and at most 8192".into());
    }
    validate(node)?;
    node.frame.width = width;
    node.frame.height = height;
    if let Primitive::Superellipse { radius: r, .. } = &mut node.primitive {
        *r = radius(*r, width, height);
    } else {
        let edge = inset(width, height);
        let outer = &mut node.children[0];
        outer.frame = Rect::new(0., 0., width, height);
        let Primitive::Superellipse { radius: r, .. } = &mut outer.primitive else {
            unreachable!("validated outline")
        };
        *r = radius(*r, width, height);
        let outer_radius = *r;
        let inner = &mut node.children[1];
        inner.frame = inner_frame(width, height);
        let Primitive::Superellipse { radius: r, .. } = &mut inner.primitive else {
            unreachable!("validated fill")
        };
        *r = radius(outer_radius - edge, inner.frame.width, inner.frame.height);
    }
    Ok(())
}

pub(super) fn set_surface_fill(node: &mut SceneNode, fill: Color) -> Result<(), String> {
    validate(node)?;
    let target = if node.children.is_empty() {
        node
    } else {
        &mut node.children[1]
    };
    let Primitive::Superellipse { color, .. } = &mut target.primitive else {
        unreachable!("validated fill")
    };
    *color = fill;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_surface_keeps_outline_and_pass_through_when_resized_or_pressed() {
        let outline = [112, 112, 112, 255];
        let mut node =
            surface_with_outline([100, 101, 102], 80., 48., [244, 244, 244, 255], outline, 8.);
        resize_surface(&mut node, 144., 56.).unwrap();
        set_surface_fill(&mut node, [21, 21, 21, 255]).unwrap();
        assert!(
            matches!(node.children[0].primitive, Primitive::Superellipse { color, .. } if color == outline)
        );
        assert!(matches!(
            node.children[1].primitive,
            Primitive::Superellipse {
                color: [21, 21, 21, 255],
                ..
            }
        ));
        assert!(
            node.children
                .iter()
                .all(|child| child.responder == Some(TouchResponder::PassThrough))
        );
        assert_eq!(node.responder, Some(TouchResponder::PassThrough));
        let before = node.clone();
        assert!(resize_surface(&mut node, f32::NAN, 56.).is_err());
        assert_eq!(node, before);
    }
}
