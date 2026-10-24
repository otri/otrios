// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded position registers and local retained reflow. No network/document access.
use super::Result;
use crate::dashboard::{Mesh, font::Font, render_scene_with_font, scene::Scene};
use otrios_interfaces::scene::{Rect, SceneNode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_POSITION: usize = 96;
pub fn valid_position(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_POSITION
        && s.bytes().all(|b| (33..=125).contains(&b))
        && s.as_bytes().last() != Some(&33)
}
/// Lexicographic fractional position, suffixed with the stable field identity.
/// Equal concurrent insertions of different cards therefore remain distinct.
pub fn between(left: Option<&str>, right: Option<&str>, item: u16) -> Result<String> {
    if left.is_some_and(|s| !valid_position(s))
        || right.is_some_and(|s| !valid_position(s))
        || matches!((left,right), (Some(a),Some(b)) if a >= b)
    {
        return Err("Invalid position interval".into());
    }
    let a = left.unwrap_or("").as_bytes();
    let b = right.unwrap_or("").as_bytes();
    let mut prefix = Vec::new();
    let mut upper = right.is_some();
    for i in 0..MAX_POSITION - 6 {
        let lo = a.get(i).copied().unwrap_or(33);
        let hi = if upper {
            b.get(i).copied().unwrap_or(126)
        } else {
            126
        };
        if lo + 1 < hi {
            prefix.push(lo + (hi - lo) / 2);
            let mut key = String::from_utf8(prefix).map_err(|e| e.to_string())?;
            key.push_str(&format!(":{item:04x}"));
            if left.is_none_or(|s| s < key.as_str()) && right.is_none_or(|s| key.as_str() < s) {
                return Ok(key);
            }
            return Err("Position interval exhausted".into());
        }
        prefix.push(lo);
        if lo < hi {
            upper = false;
        }
    }
    Err("Position interval exhausted".into())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DragView {
    pub node: u32,
    pub position: [f32; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub identity: String,
    pub root: SceneNode,
    pub nodes: Vec<u32>,
    pub editing: bool,
    pub drag: Option<DragView>,
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>> {
        flexbuffers::to_vec(self).map_err(|e| e.to_string())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 128 * 1024 {
            return Err("Reflow frame exceeds limit".into());
        }
        crate::flex_guard::validate(bytes, 48, 32768, 512)?;
        flexbuffers::from_slice(bytes).map_err(|e| e.to_string())
    }
}
struct Part {
    node: u32,
    mesh: Mesh,
    target: Rect,
    from: [f32; 2],
}
#[derive(Default)]
pub struct Presenter {
    identity: String,
    extent: [f32; 2],
    parts: Vec<Part>,
    editing: bool,
    drag: Option<DragView>,
    started: f64,
    transitioning: bool,
    root: Option<SceneNode>,
    nodes: Vec<u32>,
}
impl Presenter {
    fn position(&self, p: &Part, now: f64) -> [f32; 2] {
        if let Some(d) = &self.drag
            && d.node == p.node
        {
            return d.position;
        }
        let t = ((now - self.started) / 220.).clamp(0., 1.) as f32;
        let ease = 1. - (1. - t).powi(3);
        [
            p.from[0] + (p.target.x - p.from[0]) * ease,
            p.from[1] + (p.target.y - p.from[1]) * ease,
        ]
    }
    pub fn update(&mut self, frame: Frame, font: &Font, now: f64) -> Result<()> {
        if !now.is_finite() || frame.nodes.is_empty() || frame.nodes.len() > 32 {
            return Err("Invalid reflow frame".into());
        }
        let extent = [frame.root.frame.width, frame.root.frame.height];
        if self.identity == frame.identity
            && self.extent == extent
            && self.nodes == frame.nodes
            && self.root.as_ref() == Some(&frame.root)
        {
            if frame.drag.as_ref().is_some_and(|d| {
                !self.nodes.contains(&d.node)
                    || d.position.iter().any(|v| !v.is_finite() || v.abs() > 8192.)
            }) {
                return Err("Invalid reflow drag".into());
            }
            if self.drag.is_some() && frame.drag.is_none() {
                let positions: Vec<_> = self.parts.iter().map(|p| self.position(p, now)).collect();
                for (part, position) in self.parts.iter_mut().zip(positions) {
                    part.from = position;
                }
                self.started = now;
                self.transitioning = true;
            }
            self.editing = frame.editing;
            self.drag = frame.drag;
            return Ok(());
        }
        let scene = Scene::new(frame.root)?;
        let extent = [scene.root().frame.width, scene.root().frame.height];
        let retained = self.identity == frame.identity && self.extent == extent;
        let old: BTreeMap<_, _> = if retained {
            self.parts
                .iter()
                .map(|p| (p.node, self.position(p, now)))
                .collect()
        } else {
            BTreeMap::new()
        };
        let ids: std::collections::BTreeSet<_> = frame.nodes.iter().copied().collect();
        if ids.len() != frame.nodes.len()
            || !ids
                .iter()
                .all(|id| scene.root().children.iter().any(|n| n.id == *id))
        {
            return Err("Reflow nodes must be unique immediate components".into());
        }
        if frame.drag.as_ref().is_some_and(|d| {
            !ids.contains(&d.node) || d.position.iter().any(|v| !v.is_finite() || v.abs() > 8192.)
        }) {
            return Err("Invalid reflow drag".into());
        }
        let mut base = scene.root().clone();
        base.children.retain(|n| !ids.contains(&n.id));
        let mut parts = vec![Part {
            node: u32::MAX,
            mesh: render_scene_with_font(&Scene::new(base)?, font),
            target: Rect::new(0., 0., extent[0], extent[1]),
            from: [0., 0.],
        }];
        let mut transitioning = false;
        for node in &frame.nodes {
            let child = scene
                .root()
                .children
                .iter()
                .find(|n| n.id == *node)
                .ok_or("Missing reflow component")?;
            let mut root = scene.root().clone();
            root.children = vec![child.clone()];
            let from = old
                .get(node)
                .copied()
                .unwrap_or([child.frame.x, child.frame.y]);
            transitioning |= from != [child.frame.x, child.frame.y];
            parts.push(Part {
                node: *node,
                mesh: render_scene_with_font(&Scene::new(root)?, font),
                target: child.frame,
                from,
            });
        }
        self.root = Some(scene.root().clone());
        self.nodes = frame.nodes;
        self.identity = frame.identity;
        self.extent = extent;
        self.parts = parts;
        self.editing = frame.editing;
        self.drag = frame.drag;
        self.started = now;
        self.transitioning = transitioning;
        Ok(())
    }
    pub fn active(&self, _now: f64) -> bool {
        self.editing || self.transitioning
    }
    /// Cached glyph/shape geometry is translated and rotated; no layout/font work per tick.
    pub fn mesh(&mut self, now: f64) -> Mesh {
        if now - self.started >= 220. {
            self.transitioning = false;
        }
        let mut out = Mesh::default();
        let mut parts: Vec<_> = self.parts.iter().collect();
        if let Some(d) = &self.drag {
            parts.sort_by_key(|p| p.node == d.node);
        }
        for p in parts {
            let pos = if p.node == u32::MAX {
                [0., 0.]
            } else {
                self.position(p, now)
            };
            let angle = if self.editing
                && p.node != u32::MAX
                && self.drag.as_ref().is_none_or(|d| d.node != p.node)
            {
                ((now * 0.018 + p.node as f64).sin() * 0.012) as f32
            } else {
                0.
            };
            let (sin, cos) = angle.sin_cos();
            let center = [
                p.target.x + p.target.width / 2.,
                p.target.y + p.target.height / 2.,
            ];
            let translate = [pos[0] - p.target.x, pos[1] - p.target.y];
            let first = [out.vertices.len() / 6, out.glyph_vertices.len() / 16];
            for (input, output, stride) in [
                (&p.mesh.vertices, &mut out.vertices, 6),
                (&p.mesh.glyph_vertices, &mut out.glyph_vertices, 16),
            ] {
                for vertex in input.chunks_exact(stride) {
                    let start = output.len();
                    output.extend_from_slice(vertex);
                    let x = vertex[0] - center[0];
                    let y = vertex[1] - center[1];
                    output[start] = center[0] + x * cos - y * sin + translate[0];
                    output[start + 1] = center[1] + x * sin + y * cos + translate[1];
                }
            }
            for draw in p.mesh.draws.chunks_exact(3) {
                out.draws
                    .extend([draw[0], draw[1] + first[draw[0] as usize] as u32, draw[2]]);
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_positions_keep_identity_and_fit_between_adjacent_prefixes() {
        let mut right = "B:0001".to_string();
        for _ in 0..100 {
            let key = between(None, Some(&right), 2).unwrap();
            assert!(key < right);
            right = key;
        }
        assert!(between(Some("same"), Some("same"), 1).is_err());
        let a = between(Some("B:0001"), Some("J:0002"), 3).unwrap();
        let b = between(Some("B:0001"), Some("J:0002"), 4).unwrap();
        assert_ne!(a, b);
        let (lo, hi) = if a < b { (&a, &b) } else { (&b, &a) };
        assert!(between(Some(lo), Some(hi), 5).is_ok());
    }
}
