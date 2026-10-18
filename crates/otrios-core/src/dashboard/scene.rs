// SPDX-License-Identifier: LGPL-3.0-only
//! Validated immutable primitive tree shared by painting, hit testing and semantics.
use super::Result;
use otrios_interfaces::scene::{NodeId, Primitive, Rect, SceneNode, TouchResponder};
use std::{collections::BTreeSet, sync::Arc};
pub const MAX_NODES: usize = 512;
pub const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug)]
pub struct Scene {
    root: Arc<SceneNode>,
}
#[derive(Clone, Copy, Debug)]
pub struct Located<'a> {
    pub node: &'a SceneNode,
    pub origin: [f32; 2],
}

impl Scene {
    pub fn new(root: SceneNode) -> Result<Self> {
        if !matches!(root.primitive, Primitive::Canvas { clip: true })
            || root.frame != Rect::new(0., 0., 1280., 800.)
        {
            return Err("Scene root must be the clipped 1280×800 canvas".into());
        }
        fn validate(n: &SceneNode, depth: usize, ids: &mut BTreeSet<NodeId>) -> Result<()> {
            if depth > MAX_DEPTH || ids.len() >= MAX_NODES || !ids.insert(n.id) {
                return Err("Scene depth/count/identity invalid".into());
            }
            let r = n.frame;
            if ![r.x, r.y, r.width, r.height]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 8192.)
                || r.width <= 0.
                || r.height <= 0.
            {
                return Err("Invalid scene frame".into());
            }
            if !matches!(n.primitive, Primitive::Canvas { .. }) && !n.children.is_empty() {
                return Err("Only a canvas may contain children".into());
            }
            match &n.primitive {
                Primitive::Text { value, scale, .. }
                    if value.len() > 96 || !(1..=8).contains(scale) =>
                {
                    return Err("Invalid text primitive".into());
                }
                Primitive::Superellipse {
                    radius, exponent, ..
                } if !radius.is_finite()
                    || *radius < 0.
                    || *radius > r.width.min(r.height) / 2.
                    || !exponent.is_finite()
                    || !(2. ..=8.).contains(exponent) =>
                {
                    return Err("Invalid superellipse".into());
                }
                Primitive::Circle { .. } if r.width != r.height => {
                    return Err("Circle needs a square frame".into());
                }
                Primitive::Path { points, width, .. }
                    if points.len() > 256
                        || !width.is_finite()
                        || !(0.1..=16.).contains(width)
                        || points
                            .iter()
                            .flatten()
                            .any(|v| !v.is_finite() || v.abs() > 8192.) =>
                {
                    return Err("Invalid path".into());
                }
                _ => {}
            }
            if let Some(s) = &n.semantics
                && (s.label.len() > 96 || !matches!(n.responder, Some(TouchResponder::Activate(_))))
            {
                return Err("Switch semantics need an activating responder".into());
            }
            for child in &n.children {
                validate(child, depth + 1, ids)?;
            }
            Ok(())
        }
        validate(&root, 0, &mut BTreeSet::new())?;
        Ok(Self {
            root: Arc::new(root),
        })
    }
    pub fn root(&self) -> &SceneNode {
        &self.root
    }
    pub fn path(&self, id: NodeId) -> Option<Vec<Located<'_>>> {
        fn walk<'a>(
            n: &'a SceneNode,
            origin: [f32; 2],
            id: NodeId,
            path: &mut Vec<Located<'a>>,
        ) -> bool {
            let origin = [origin[0] + n.frame.x, origin[1] + n.frame.y];
            path.push(Located { node: n, origin });
            if n.id == id {
                return true;
            }
            for child in &n.children {
                if walk(child, origin, id, path) {
                    return true;
                }
            }
            path.pop();
            false
        }
        let mut path = Vec::new();
        if walk(self.root(), [0., 0.], id, &mut path) {
            Some(path)
        } else {
            None
        }
    }
    pub fn hit_path(&self, point: [f32; 2]) -> Vec<Located<'_>> {
        fn walk<'a>(
            n: &'a SceneNode,
            origin: [f32; 2],
            p: [f32; 2],
            path: &mut Vec<Located<'a>>,
        ) -> bool {
            let origin = [origin[0] + n.frame.x, origin[1] + n.frame.y];
            let local = [p[0] - origin[0], p[1] - origin[1]];
            if matches!(n.primitive, Primitive::Canvas { clip: true })
                && !rect_contains(n.frame, local)
            {
                return false;
            }
            path.push(Located { node: n, origin });
            for child in n.children.iter().rev() {
                if walk(child, origin, p, path) {
                    return true;
                }
            }
            if !matches!(n.primitive, Primitive::Canvas { .. }) && contains(n, local) {
                return true;
            }
            path.pop();
            false
        }
        if point.iter().any(|v| !v.is_finite()) {
            return vec![];
        }
        let mut path = Vec::new();
        walk(self.root(), [0., 0.], point, &mut path);
        path
    }
    pub fn controls(&self) -> Vec<Located<'_>> {
        fn walk<'a>(n: &'a SceneNode, origin: [f32; 2], v: &mut Vec<Located<'a>>) {
            let origin = [origin[0] + n.frame.x, origin[1] + n.frame.y];
            if n.semantics.is_some() {
                v.push(Located { node: n, origin });
            }
            for c in &n.children {
                walk(c, origin, v);
            }
        }
        let mut v = Vec::new();
        walk(self.root(), [0., 0.], &mut v);
        v
    }
}
fn rect_contains(r: Rect, p: [f32; 2]) -> bool {
    p[0] >= 0. && p[1] >= 0. && p[0] < r.width && p[1] < r.height
}
fn contains(n: &SceneNode, p: [f32; 2]) -> bool {
    let r = n.frame;
    if !rect_contains(r, p) {
        return false;
    }
    match &n.primitive {
        Primitive::Circle { .. } => {
            let a = r.width / 2.;
            ((p[0] - a) / a).powi(2) + ((p[1] - a) / a).powi(2) <= 1.
        }
        Primitive::Superellipse {
            radius, exponent, ..
        } if *radius > 0. => {
            let dx = (p[0] - r.width / 2.).abs() - (r.width / 2. - radius);
            let dy = (p[1] - r.height / 2.).abs() - (r.height / 2. - radius);
            dx <= 0.
                || dy <= 0.
                || (dx / radius).powf(*exponent) + (dy / radius).powf(*exponent) <= 1.
        }
        Primitive::Path { points, width, .. } => points.windows(2).any(|s| {
            let d = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
            let len = d[0] * d[0] + d[1] * d[1];
            let t = if len == 0. {
                0.
            } else {
                ((p[0] - s[0][0]) * d[0] + (p[1] - s[0][1]) * d[1]) / len
            }
            .clamp(0., 1.);
            (p[0] - s[0][0] - t * d[0]).hypot(p[1] - s[0][1] - t * d[1]) <= width / 2.
        }),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::components::*;
    #[test]
    fn clipping_and_superellipse_corners_match_hit_geometry() {
        let shape = node(
            2,
            Rect::new(-20., -20., 80., 80.),
            Primitive::Superellipse {
                radius: 20.,
                exponent: 4.,
                color: WHITE,
            },
        );
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            vec![canvas(1, Rect::new(100., 200., 40., 40.), vec![shape])],
        ))
        .unwrap();
        assert!(scene.hit_path([95., 205.]).is_empty());
        assert_eq!(scene.hit_path([110., 210.]).last().unwrap().node.id, 2);
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            vec![node(
                3,
                Rect::new(100., 200., 100., 100.),
                Primitive::Superellipse {
                    radius: 50.,
                    exponent: 4.,
                    color: WHITE,
                },
            )],
        ))
        .unwrap();
        assert!(scene.hit_path([100., 200.]).is_empty());
        assert_eq!(scene.hit_path([150., 250.]).last().unwrap().node.id, 3);
    }
    #[test]
    fn invalid_trees_are_rejected_before_publication() {
        let mut root = canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            vec![background(0, 10., 10., WHITE, 0.)],
        );
        assert!(Scene::new(root.clone()).is_err());
        root.children[0].id = 1;
        root.children[0].frame.x = f32::NAN;
        assert!(Scene::new(root.clone()).is_err());
        root.children.clear();
        let mut child = background(99, 10., 10., WHITE, 0.);
        for i in 1..=MAX_DEPTH + 1 {
            child = canvas(i as u32, Rect::new(0., 0., 10., 10.), vec![child]);
        }
        root.children.push(child);
        assert!(Scene::new(root).is_err());
    }
}
