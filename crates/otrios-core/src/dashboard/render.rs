// SPDX-License-Identifier: LGPL-3.0-only
use super::{ViewSnapshot, scene::Scene};
#[path = "font.rs"]
mod font;
use otrios_interfaces::scene::{Color, Primitive, Rect, SceneNode};
/// Roboto copyright and license notice, embedded with the glyph data in every host.
pub fn font_notice() -> &'static str {
    font::notice()
}

/// Proportional text advance in logical units, using the same metrics as rendering.
pub fn text_width(value: &str, scale: f32) -> f32 {
    value
        .chars()
        .map(|ch| font::glyph(ch).advance * scale)
        .sum()
}

/// Triangle list: logical x/y and RGBA. Both GPU hosts consume this geometry.
pub struct Mesh {
    pub vertices: Vec<f32>,
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
pub fn render_scene(scene: &Scene) -> Mesh {
    let mut mesh = Mesh { vertices: vec![] };
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
    );
    mesh
}
impl Mesh {
    fn vertex(&mut self, p: [f32; 2], c: Color) {
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
    fn text(&mut self, r: Rect, value: &str, scale: f32, c: Color, clip: Clip) {
        let clip = clip.intersect(r);
        let mut x = r.x;
        for ch in value.chars() {
            let glyph = font::glyph(ch);
            let mut points = glyph.points();
            while let (Some(a), Some(b), Some(d)) = (points.next(), points.next(), points.next()) {
                self.triangle(
                    [a, b, d].map(|p| [x + p[0] * scale, r.y + p[1] * scale]),
                    c,
                    clip,
                );
            }
            x += glyph.advance * scale;
            if x >= clip.right {
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
    fn paint(&mut self, node: &SceneNode, parent: [f32; 2], factor: f32, clip: Clip) {
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
                        self.paint(child, [r.x, r.y], factor, clip);
                    }
                }
            }
            Primitive::Viewport { scale } => {
                let clip = clip.intersect(r);
                if !clip.empty() {
                    for child in &node.children {
                        self.paint(child, [r.x, r.y], factor * scale, clip);
                    }
                }
            }
            Primitive::Text {
                value,
                scale,
                color,
            } => self.text(r, value, *scale as f32 * factor, *color, clip),
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
    fn vector_text_preserves_holes_and_fractional_clipping() {
        let mut mesh = Mesh { vertices: vec![] };
        let clip = Clip {
            left: 0.,
            top: 0.,
            right: 100.,
            bottom: 100.,
        };
        mesh.text(Rect::new(0., 0., 100., 100.), "O", 4., [255; 4], clip);
        let covered = |p: [f32; 2]| {
            mesh.vertices.chunks_exact(18).any(|t| {
                let cross = |a: usize, b: usize| {
                    (t[b] - t[a]) * (p[1] - t[a + 1]) - (t[b + 1] - t[a + 1]) * (p[0] - t[a])
                };
                let signs = [cross(0, 6), cross(6, 12), cross(12, 0)];
                signs.iter().all(|s| *s >= 0.) || signs.iter().all(|s| *s <= 0.)
            })
        };
        assert!(!mesh.vertices.is_empty());
        assert!(
            !covered([font::glyph('O').advance * 2., 16.]),
            "O counter must be empty"
        );
        assert!(covered([3., 16.]), "O stem must be filled");
        let clip = Clip {
            left: 3.25,
            top: 1.25,
            right: 73.75,
            bottom: 7.75,
        };
        mesh.vertices.clear();
        mesh.text(
            Rect::new(0., 0., 1280., 800.),
            "ABCxyz09?",
            1.,
            [255; 4],
            clip,
        );
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.vertices.len() % 18, 0);
        for p in mesh.vertices.chunks_exact(6) {
            assert!(p.iter().all(|v| v.is_finite()));
            assert!((clip.left..=clip.right).contains(&p[0]));
            assert!((clip.top..=clip.bottom).contains(&p[1]));
        }
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
