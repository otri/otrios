// SPDX-License-Identifier: LGPL-3.0-only
use super::{ViewSnapshot, font::Font, scene::Scene};
use otrios_interfaces::scene::{Color, Primitive, Rect, SceneNode};
/// Ordered geometry and Slug glyph runs, retained by both GPU hosts.
#[derive(Default)]
pub struct Mesh {
    /// Triangle list: logical xy and RGBA (six floats per vertex).
    pub vertices: Vec<f32>,
    /// Indexed quads TL,TR,BL,BR: xy, emxy, RGBA, bandTransform, glyphData.
    pub glyph_vertices: Vec<f32>,
    /// Triples (kind,firstVertex,countVertices): kind 0 triangles, kind 1 glyph quads.
    pub draws: Vec<u32>,
}
#[derive(Clone, Copy)]
struct Clip {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}
impl Clip {
    fn intersect(self, r: Rect) -> Self {
        Self {
            left: self.left.max(r.x),
            top: self.top.max(r.y),
            right: self.right.min(r.x + r.width),
            bottom: self.bottom.min(r.y + r.height),
        }
    }
    fn empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
}
pub fn render(snapshot: &ViewSnapshot) -> Mesh {
    render_scene(snapshot.scene())
}
/// Geometry-only projection for headless use; text requires an explicit admitted font.
pub fn render_scene(scene: &Scene) -> Mesh {
    render_optional_font(scene, None)
}
pub fn render_with_font(snapshot: &ViewSnapshot, font: &Font) -> Mesh {
    render_scene_with_font(snapshot.scene(), font)
}
pub fn render_scene_with_font(scene: &Scene, font: &Font) -> Mesh {
    render_optional_font(scene, Some(font))
}
fn render_optional_font(scene: &Scene, font: Option<&Font>) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.paint(
        scene.root(),
        [0., 0.],
        1.,
        Clip {
            left: 0.,
            top: 0.,
            right: 1280.,
            bottom: 800.,
        },
        font,
    );
    mesh
}
impl Mesh {
    fn run(&mut self, kind: u32, first: u32, count: u32) {
        let length = self.draws.len();
        if length >= 3 {
            let last = &mut self.draws[length - 3..];
            if last[0] == kind && last[1] + last[2] == first {
                last[2] += count;
                return;
            }
        }
        self.draws.extend([kind, first, count]);
    }
    fn vertex(&mut self, p: [f32; 2], c: Color) {
        self.run(0, (self.vertices.len() / 6) as u32, 1);
        self.vertices.extend([
            p[0],
            p[1],
            c[0] as f32 / 255.,
            c[1] as f32 / 255.,
            c[2] as f32 / 255.,
            c[3] as f32 / 255.,
        ]);
    }
    fn triangle(&mut self, points: [[f32; 2]; 3], c: Color, clip: Clip) {
        if clip.empty() {
            return;
        }
        if points.iter().all(|p| {
            p[0] >= clip.left && p[0] <= clip.right && p[1] >= clip.top && p[1] <= clip.bottom
        }) {
            for p in points {
                self.vertex(p, c);
            }
            return;
        }
        let mut polygon = points.to_vec();
        for (axis, bound, greater) in [
            (0, clip.left, true),
            (0, clip.right, false),
            (1, clip.top, true),
            (1, clip.bottom, false),
        ] {
            let input = std::mem::take(&mut polygon);
            if input.is_empty() {
                return;
            }
            let inside = |p: [f32; 2]| {
                if greater {
                    p[axis] >= bound
                } else {
                    p[axis] <= bound
                }
            };
            let mut a = *input.last().unwrap();
            for b in input {
                if inside(a) != inside(b) {
                    let t = (bound - a[axis]) / (b[axis] - a[axis]);
                    let mut p = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
                    p[axis] = bound;
                    polygon.push(p);
                }
                if inside(b) {
                    polygon.push(b);
                }
                a = b;
            }
        }
        for i in 1..polygon.len().saturating_sub(1) {
            for p in [polygon[0], polygon[i], polygon[i + 1]] {
                self.vertex(p, c);
            }
        }
    }
    fn rect(&mut self, r: Rect, c: Color, clip: Clip) {
        let a = [r.x, r.y];
        let b = [r.x + r.width, r.y];
        let d = [r.x, r.y + r.height];
        let e = [b[0], d[1]];
        self.triangle([a, b, d], c, clip);
        self.triangle([d, b, e], c, clip);
    }
    fn fan(&mut self, center: [f32; 2], outline: &[[f32; 2]], c: Color, clip: Clip) {
        for i in 0..outline.len() {
            self.triangle(
                [center, outline[i], outline[(i + 1) % outline.len()]],
                c,
                clip,
            );
        }
    }
    fn circle(&mut self, r: Rect, c: Color, clip: Clip) {
        let center = [r.x + r.width / 2., r.y + r.height / 2.];
        let points: Vec<_> = (0..32)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 32.;
                [
                    center[0] + a.cos() * r.width / 2.,
                    center[1] + a.sin() * r.height / 2.,
                ]
            })
            .collect();
        self.fan(center, &points, c, clip);
    }
    fn superellipse(&mut self, r: Rect, radius: f32, exponent: f32, c: Color, clip: Clip) {
        if radius == 0. {
            self.rect(r, c, clip);
            return;
        }
        let mut outline = Vec::new();
        for (corner, center) in [
            [r.x + r.width - radius, r.y + r.height - radius],
            [r.x + radius, r.y + r.height - radius],
            [r.x + radius, r.y + radius],
            [r.x + r.width - radius, r.y + radius],
        ]
        .iter()
        .enumerate()
        {
            for step in 0..=12 {
                let a = (corner as f32 + step as f32 / 12.) * std::f32::consts::FRAC_PI_2;
                let (s, t) = (a.sin(), a.cos());
                outline.push([
                    center[0] + radius * t.signum() * t.abs().powf(2. / exponent),
                    center[1] + radius * s.signum() * s.abs().powf(2. / exponent),
                ]);
            }
        }
        self.fan([r.x + r.width / 2., r.y + r.height / 2.], &outline, c, clip);
    }
    fn text(&mut self, r: Rect, value: &str, scale: f32, c: Color, clip: Clip, font: &Font) {
        // The scene's old height was an 8px bitmap box. It is not a glyph ink clip:
        // ascenders and descenders use font metrics, while ancestor canvases still clip.
        // Advance width controls where the logical run ends. It does not clip ink:
        // a negative side bearing or overhang may cross the text frame's x edges.
        // Explicit ancestor canvases provide field/panel clipping when required.
        if clip.empty() || r.width <= 0. {
            return;
        }
        let factor = font.metrics().scale(scale);
        let em_size = factor * f32::from(font.metrics().units_per_em);
        let baseline = r.y + font.metrics().baseline_from_top(scale);
        let mut pen = r.x;
        for ch in value.chars() {
            let glyph = font.glyph(ch);
            if glyph.curve_count != 0 && self.glyph_vertices.len() / 16 + 4 <= 65536 {
                let b = glyph.ink_bounds;
                let left = (pen + b[0] * factor - 1.).max(clip.left);
                let right = (pen + b[2] * factor + 1.).min(clip.right);
                let top = (baseline - b[3] * factor - 1.).max(clip.top);
                let bottom = (baseline - b[1] * factor + 1.).min(clip.bottom);
                if left < right && top < bottom {
                    let first = (self.glyph_vertices.len() / 16) as u32;
                    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
                        self.glyph_vertices.extend([
                            x,
                            y,
                            (x - pen) / em_size,
                            (baseline - y) / em_size,
                        ]);
                        self.glyph_vertices.extend(c.map(|v| f32::from(v) / 255.));
                        self.glyph_vertices.extend(glyph.band_transform);
                        self.glyph_vertices.extend(glyph.band_data());
                    }
                    self.run(1, first, 4);
                }
            }
            pen += glyph.advance * factor;
            if pen >= r.x + r.width {
                break;
            }
        }
    }

    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, c: Color, clip: Clip) {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0].hypot(d[1]);
        if len == 0. {
            return;
        }
        let n = [-d[1] / len * width / 2., d[0] / len * width / 2.];
        let p = [
            [a[0] + n[0], a[1] + n[1]],
            [b[0] + n[0], b[1] + n[1]],
            [a[0] - n[0], a[1] - n[1]],
            [b[0] - n[0], b[1] - n[1]],
        ];
        self.triangle([p[0], p[1], p[2]], c, clip);
        self.triangle([p[2], p[1], p[3]], c, clip);
    }
    fn paint(
        &mut self,
        node: &SceneNode,
        parent: [f32; 2],
        factor: f32,
        clip: Clip,
        font: Option<&Font>,
    ) {
        let f = node.frame;
        let r = Rect::new(
            parent[0] + f.x * factor,
            parent[1] + f.y * factor,
            f.width * factor,
            f.height * factor,
        );
        match &node.primitive {
            Primitive::Canvas { clip: enabled } => {
                let clip = if *enabled { clip.intersect(r) } else { clip };
                if !clip.empty() {
                    for child in &node.children {
                        self.paint(child, [r.x, r.y], factor, clip, font);
                    }
                }
            }
            Primitive::Viewport { scale } => {
                let clip = clip.intersect(r);
                if !clip.empty() {
                    for child in &node.children {
                        self.paint(child, [r.x, r.y], factor * scale, clip, font);
                    }
                }
            }
            Primitive::Text {
                value,
                scale,
                color,
            } => {
                if let Some(font) = font {
                    self.text(r, value, *scale as f32 * factor, *color, clip, font);
                }
            }
            Primitive::Superellipse {
                radius,
                exponent,
                color,
            } => self.superellipse(r, *radius * factor, *exponent, *color, clip),
            Primitive::Circle { color } => self.circle(r, *color, clip),
            Primitive::Triangles { points, color } => {
                for tri in points.chunks_exact(3) {
                    self.triangle(
                        std::array::from_fn(|i| {
                            [r.x + tri[i][0] * factor, r.y + tri[i][1] * factor]
                        }),
                        *color,
                        clip,
                    );
                }
            }
            Primitive::Path {
                points,
                width,
                color,
            } => {
                for p in points.windows(2) {
                    self.line(
                        [r.x + p[0][0] * factor, r.y + p[0][1] * factor],
                        [r.x + p[1][0] * factor, r.y + p[1][1] * factor],
                        *width * factor,
                        *color,
                        clip,
                    );
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glyph_quads_use_baselines_and_ancestor_clip_without_truncating_descenders() {
        let font = super::super::font::test_font();
        let mut mesh = Mesh::default();
        let clip = Clip {
            left: 0.,
            top: 0.,
            right: 100.,
            bottom: 100.,
        };
        mesh.text(
            Rect::new(0., 0., 100., 8.),
            "Égy",
            1.,
            [255; 4],
            clip,
            &font,
        );
        assert!(
            mesh.vertices.is_empty(),
            "Text must not be tessellated into geometry"
        );
        assert_eq!(mesh.glyph_vertices.len(), 3 * 4 * 16);
        let baseline = font.metrics().baseline_from_top(1.);
        let ys: Vec<_> = mesh.glyph_vertices.chunks_exact(16).map(|v| v[1]).collect();
        assert!(
            ys.iter().any(|y| *y > baseline),
            "descenders must extend below baseline"
        );
        assert!(
            ys.iter().any(|y| *y > 8.),
            "old bitmap height must not clip descenders"
        );
        assert_eq!(mesh.draws, [1, 0, 12]);
        mesh.glyph_vertices.clear();
        mesh.draws.clear();
        let clip = Clip {
            left: 3.25,
            top: 1.25,
            right: 20.75,
            bottom: 7.75,
        };
        mesh.text(
            Rect::new(0., 0., 1280., 8.),
            "ABCxyz09?",
            1.,
            [255; 4],
            clip,
            &font,
        );
        assert!(!mesh.glyph_vertices.is_empty());
        for v in mesh.glyph_vertices.chunks_exact(16) {
            assert!(v.iter().all(|v| v.is_finite()));
            assert!((clip.left..=clip.right).contains(&v[0]));
            assert!((clip.top..=clip.bottom).contains(&v[1]));
        }
    }
    #[test]
    fn glyph_ink_retains_negative_bearings_and_overhang_past_a_short_text_frame() {
        let font = super::super::font::test_font();
        let mut mesh = Mesh::default();
        let clip = Clip {
            left: 0.,
            top: 0.,
            right: 100.,
            bottom: 100.,
        };
        mesh.text(
            Rect::new(20., 0., 1., 8.),
            "jgyÉ",
            1.,
            [255; 4],
            clip,
            &font,
        );
        // One logical glyph is emitted before its advance reaches the run's width.
        assert_eq!(mesh.glyph_vertices.len(), 4 * 16);
        let left = mesh.glyph_vertices[0];
        let right = mesh.glyph_vertices[16];
        let expected = 20. + font.glyph('j').ink_bounds[0] * font.metrics().scale(1.) - 1.;
        assert_eq!(left, expected);
        assert!(
            left < 19.,
            "negative bearing must survive, beyond the AA margin"
        );
        assert!(
            right > 21.,
            "glyph ink must not be cut to the logical advance box"
        );
        assert!(
            mesh.glyph_vertices[2 * 16 + 1] > 8.,
            "descender must survive old bitmap height"
        );
    }
    #[test]
    fn geometry_and_text_draws_preserve_painter_order() {
        let font = super::super::font::test_font();
        let mut mesh = Mesh::default();
        let clip = Clip {
            left: 0.,
            top: 0.,
            right: 100.,
            bottom: 100.,
        };
        mesh.rect(Rect::new(0., 0., 10., 10.), [255; 4], clip);
        mesh.text(Rect::new(0., 0., 100., 8.), "Hi", 1., [255; 4], clip, &font);
        mesh.rect(Rect::new(0., 0., 10., 10.), [255; 4], clip);
        assert_eq!(mesh.draws, [0, 0, 6, 1, 0, 8, 0, 6, 6]);
    }
    #[test]
    fn same_snapshot_produces_same_mesh_and_touch_changes_it() {
        let mut r = crate::dashboard::Replica::new().unwrap();
        let a = render(r.snapshot());
        let b = render(&ViewSnapshot::from_bytes(r.snapshot().bytes()).unwrap());
        assert_eq!(a.vertices.len() % 18, 0);
        assert!(a.vertices.iter().all(|v| v.is_finite()));
        assert_eq!(a.vertices, b.vertices);
        r.touch(200, 300).unwrap();
        assert_ne!(a.vertices, render(r.snapshot()).vertices);
    }
    #[test]
    fn nested_canvas_clips_actual_triangles() {
        use crate::dashboard::components::*;
        let child = node(
            2,
            Rect::new(-10., -20., 80., 90.),
            Primitive::Circle { color: WHITE },
        );
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            vec![canvas(1, Rect::new(100., 200., 30., 40.), vec![child])],
        ))
        .unwrap_err();
        assert!(scene.contains("Circle"));
        let child = node(
            2,
            Rect::new(-10., -20., 80., 80.),
            Primitive::Circle { color: WHITE },
        );
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            vec![canvas(1, Rect::new(100., 200., 30., 40.), vec![child])],
        ))
        .unwrap();
        let mesh = render_scene(&scene);
        assert!(!mesh.vertices.is_empty());
        for v in mesh.vertices.chunks_exact(6) {
            assert!((100. ..=130.).contains(&v[0]));
            assert!((200. ..=240.).contains(&v[1]));
        }
    }
}
