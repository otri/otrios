// SPDX-License-Identifier: LGPL-3.0-only
//! Checked hierarchy coordinates. Q32 retains small offsets through cancellation;
//! float attributes are produced only after the complete ancestor transform.
use super::{Result, fixed::Scalar, render::Clip};
use otrios_interfaces::scene::{Primitive, Rect, SceneNode};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) struct Placement {
    pub frame: [Scalar; 4],
    pub clip: [Scalar; 4],
}
impl Placement {
    pub fn rect(self) -> Rect {
        let [x, y, w, h] = self.frame.map(Scalar::to_f32);
        Rect::new(x, y, w, h)
    }
    pub fn clip(self) -> Clip {
        let [left, top, right, bottom] = self.clip.map(Scalar::to_f32);
        Clip {
            left,
            top,
            right,
            bottom,
        }
    }
}
#[derive(Clone, Copy)]
struct Space {
    origin: [Scalar; 2],
    scale: Scalar,
    clip: [Scalar; 4],
}
impl Space {
    fn place(self, node: &SceneNode) -> Result<(Placement, Self)> {
        let f = node.frame;
        let x = self.origin[0].checked_add(Scalar::from_f32(f.x)?.checked_mul(self.scale)?)?;
        let y = self.origin[1].checked_add(Scalar::from_f32(f.y)?.checked_mul(self.scale)?)?;
        let width = Scalar::from_f32(f.width)?.checked_mul(self.scale)?;
        let height = Scalar::from_f32(f.height)?.checked_mul(self.scale)?;
        let mut next = Self {
            origin: [x, y],
            ..self
        };
        if matches!(
            node.primitive,
            Primitive::Canvas { clip: true } | Primitive::Viewport { .. }
        ) {
            next.clip = [
                self.clip[0].max(x),
                self.clip[1].max(y),
                self.clip[2].min(x.checked_add(width)?),
                self.clip[3].min(y.checked_add(height)?),
            ];
        }
        if let Primitive::Viewport { scale } = node.primitive {
            next.scale = self.scale.checked_mul(Scalar::from_f32(scale)?)?;
        }
        Ok((
            Placement {
                frame: [x, y, width, height],
                clip: self.clip,
            },
            next,
        ))
    }
}
/// Failure falls back for the entire frame; no partially transformed hierarchy
/// is published and existing float contract admission remains unchanged.
pub(super) fn placements(root: &SceneNode) -> Result<BTreeMap<u32, Placement>> {
    fn walk(node: &SceneNode, space: Space, out: &mut BTreeMap<u32, Placement>) -> Result<()> {
        let (placement, next) = space.place(node)?;
        out.insert(node.id, placement);
        for child in &node.children {
            walk(child, next, out)?;
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(
        root,
        Space {
            origin: [Scalar::ZERO; 2],
            scale: Scalar::ONE,
            clip: [
                Scalar::ZERO,
                Scalar::ZERO,
                Scalar::from_f32(root.frame.width)?,
                Scalar::from_f32(root.frame.height)?,
            ],
        },
        &mut out,
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::components::{WHITE, canvas, text};
    #[test]
    fn cancelling_large_ancestors_preserve_a_small_local_offset() {
        let mut inner = canvas(
            2,
            Rect::new(0.0001, 0., 100., 100.),
            vec![text(3, Rect::new(-99984., 8., 100., 20.), "ABC", 2, WHITE)],
        );
        inner.primitive = Primitive::Canvas { clip: false };
        let mut outer = canvas(1, Rect::new(100000., 0., 100., 100.), vec![inner]);
        outer.primitive = Primitive::Canvas { clip: false };
        let root = canvas(0, Rect::new(0., 0., 200., 100.), vec![outer]);
        let all = placements(&root).unwrap();
        assert!((all[&3].rect().x - 16.0001).abs() < 0.000002);
        assert_eq!(100000_f32 + 0.0001 - 99984., 16.);
    }
    #[test]
    fn nested_fractional_scales_intersect_all_clip_edges_in_integer_space() {
        let mut inner = canvas(
            2,
            Rect::new(-2.25, 3.5, 80., 50.),
            vec![text(3, Rect::new(-4., -5., 500., 20.), "jyABC", 2, WHITE)],
        );
        inner.primitive = Primitive::Viewport { scale: 0.33 };
        let mut outer = canvas(1, Rect::new(10.125, 4.75, 60., 40.), vec![inner]);
        outer.primitive = Primitive::Viewport { scale: 1.25 };
        let root = canvas(0, Rect::new(0., 0., 200., 100.), vec![outer]);
        let all = placements(&root).unwrap();
        let p = all[&3];
        assert_eq!(p.clip.map(Scalar::to_f32), [10.125, 9.125, 70.125, 44.75]);
        let expected = 10.125_f64 - 2.25 * 1.25 - 4. * 1.25 * f64::from(0.33_f32);
        assert!((f64::from(p.rect().x) - expected).abs() < 0.000001);
    }
}
