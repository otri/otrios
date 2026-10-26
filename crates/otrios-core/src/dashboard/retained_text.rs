// SPDX-License-Identifier: LGPL-3.0-only
//! Retained text strips: immutable local glyphs, moving viewport, conservative culling.
use super::{
    font::Font,
    render::{Clip, Mesh},
    scene::Scene,
};
use otrios_interfaces::scene::{Primitive, SceneNode};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub struct TextPresentation {
    key: Option<SceneNode>,
    font: String,
    mesh: Arc<Mesh>,
    runs: Vec<Run>,
    generation: u32,
}
struct Run {
    draw: usize,
    id: u32,
    right: Vec<f32>,
    left: Vec<f32>,
    top: f32,
    bottom: f32,
}
pub struct TextFrame {
    pub mesh: Arc<Mesh>,
    /// Eight floats per draw: visible first/count, translation xy, clip l/t/r/b.
    /// Geometry entries are ignored by hosts. No glyph vertices change on scrolling.
    pub views: Vec<f32>,
    pub generation: u32,
}
impl TextPresentation {
    pub fn compose(&mut self, scene: &Scene, font: &Font) -> TextFrame {
        let mut key = scene.root().clone();
        let mut positions = BTreeMap::new();
        let extent = [key.frame.width, key.frame.height];
        let mut glyphs = 0;
        normalize(
            &mut key,
            [0., 0.],
            1.,
            [0., 0., extent[0], extent[1]],
            &mut positions,
            &mut glyphs,
        );
        if self.key.as_ref() != Some(&key) || self.font != font.hash() {
            let mut mesh = Mesh::default();
            let mut runs = Vec::new();
            // Oversized documents retain the existing visible-only bounded path.
            // Input fields are limited to 256 bytes; normal strips fit comfortably.
            if glyphs <= 16384 {
                mesh.paint(
                    scene.root(),
                    [0., 0.],
                    1.,
                    Clip {
                        left: 0.,
                        top: 0.,
                        right: extent[0],
                        bottom: extent[1],
                    },
                    Some(font),
                    Some(&mut runs),
                );
            } else {
                mesh = super::render_scene_with_font(scene, font);
                self.key = None;
            }
            self.runs = runs
                .into_iter()
                .map(|(draw, id)| {
                    let first = mesh.draws[draw * 3 + 1] as usize;
                    let count = mesh.draws[draw * 3 + 2] as usize;
                    let mut right = Vec::new();
                    let mut left = Vec::new();
                    let mut top = f32::INFINITY;
                    let mut bottom = f32::NEG_INFINITY;
                    let mut maximum = f32::NEG_INFINITY;
                    for q in mesh.glyph_vertices[first * 16..(first + count) * 16].chunks_exact(64)
                    {
                        maximum = maximum.max(q[16]);
                        right.push(maximum);
                        left.push(q[0]);
                        top = top.min(q[1]);
                        bottom = bottom.max(q[33]);
                    }
                    let mut minimum = f32::INFINITY;
                    for x in left.iter_mut().rev() {
                        minimum = minimum.min(*x);
                        *x = minimum;
                    }
                    Run {
                        draw,
                        id,
                        right,
                        left,
                        top,
                        bottom,
                    }
                })
                .collect();
            self.mesh = Arc::new(mesh);
            self.font = font.hash().into();
            self.key = (glyphs <= 16384).then_some(key);
            self.generation = self.generation.wrapping_add(1);
        }
        let mut views = vec![0.; self.mesh.draws.len() / 3 * 8];
        for run in &self.runs {
            let Some(&[x, y, l, t, r, b]) = positions.get(&run.id) else {
                continue;
            };
            let start = run.right.partition_point(|right| *right <= l - x);
            let end = run.left.partition_point(|left| *left < r - x);
            let count = if r > l && b > t && run.bottom + y > t && run.top + y < b {
                end.saturating_sub(start) * 4
            } else {
                0
            };
            views[run.draw * 8..run.draw * 8 + 8].copy_from_slice(&[
                self.mesh.draws[run.draw * 3 + 1] as f32 + (start * 4) as f32,
                count as f32,
                x,
                y,
                l,
                t,
                r,
                b,
            ]);
        }
        if self.runs.is_empty() {
            views.clear();
        }
        TextFrame {
            mesh: self.mesh.clone(),
            views,
            generation: self.generation,
        }
    }
}
fn normalize(
    node: &mut SceneNode,
    parent: [f32; 2],
    factor: f32,
    clip: [f32; 4],
    out: &mut BTreeMap<u32, [f32; 6]>,
    glyphs: &mut usize,
) {
    let f = node.frame;
    let x = parent[0] + f.x * factor;
    let y = parent[1] + f.y * factor;
    let mut clip = clip;
    let mut factor = factor;
    match &node.primitive {
        Primitive::Text { value, .. } => {
            *glyphs += value.chars().count();
            out.insert(node.id, [x, y, clip[0], clip[1], clip[2], clip[3]]);
            node.frame = otrios_interfaces::scene::Rect::new(0., 0., 0., 0.);
        }
        Primitive::Canvas { clip: true } | Primitive::Viewport { .. } => {
            clip = [
                clip[0].max(x),
                clip[1].max(y),
                clip[2].min(x + f.width * factor),
                clip[3].min(y + f.height * factor),
            ];
            if let Primitive::Viewport { scale } = node.primitive {
                factor *= scale;
            }
        }
        _ => {}
    }
    for child in &mut node.children {
        normalize(child, [x, y], factor, clip, out, glyphs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::components::{WHITE, canvas, text};
    use otrios_interfaces::scene::Rect;
    fn scene(offset: f32, y: f32, value: &str) -> Scene {
        Scene::new(canvas(
            0,
            Rect::new(0., 0., 200., 100.),
            vec![canvas(
                1,
                Rect::new(10., 10., 40., 24.),
                vec![text(2, Rect::new(-offset, y, 1000., 24.), value, 2, WHITE)],
            )],
        ))
        .unwrap()
    }
    #[test]
    fn scrolling_retains_vertices_and_culls_glyph_ranges_and_whole_runs() {
        let font = super::super::font::test_font();
        let mut renderer = TextPresentation::default();
        let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let a = renderer.compose(&scene(0., 0., text), &font);
        let b = renderer.compose(&scene(30.25, 0., text), &font);
        assert!(Arc::ptr_eq(&a.mesh, &b.mesh));
        assert_eq!(a.generation, b.generation);
        assert_eq!(&b.views[2..8], &[-20.25, 10., 10., 10., 50., 34.]);
        assert!(b.views[0] > a.views[0]);
        assert!(b.views[1] > 0. && b.views[1] < b.mesh.glyph_vertices.len() as f32 / 16.);
        let off = renderer.compose(&scene(30.25, 100., text), &font);
        assert!(Arc::ptr_eq(&b.mesh, &off.mesh));
        assert_eq!(off.views[1], 0.);
        let changed = renderer.compose(&scene(30.25, 0., "New contents"), &font);
        assert!(!Arc::ptr_eq(&changed.mesh, &b.mesh));
    }
}
