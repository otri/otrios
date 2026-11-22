// SPDX-License-Identifier: LGPL-3.0-only
//! Local, bounded procedural effects. A recorded result is never rolled again.
use otrios_interfaces::chat::MessageEvent;
use std::{
    f32::consts::{PI, TAU},
    sync::OnceLock,
};

type V3 = [f32; 3];
type Color = [f32; 4];
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> V3 {
    mul(a, dot(a, a).sqrt().max(1e-6).recip())
}
fn sub(a: V3, b: V3) -> V3 {
    add(a, mul(b, -1.))
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
#[derive(Clone, Copy)]
struct Q([f32; 4]);
impl Q {
    fn normalized(self) -> Self {
        let m = self.0.iter().map(|n| n * n).sum::<f32>().sqrt().max(1e-6);
        Self(self.0.map(|n| n / m))
    }
    fn axis(axis: V3, angle: f32) -> Self {
        let s = (angle * 0.5).sin();
        let a = mul(norm(axis), s);
        Self([a[0], a[1], a[2], (angle * 0.5).cos()])
    }
    fn product(self, b: Self) -> Self {
        let [x, y, z, w] = self.0;
        let [a, b, c, d] = b.0;
        Self([
            w * a + x * d + y * c - z * b,
            w * b - x * c + y * d + z * a,
            w * c + x * b - y * a + z * d,
            w * d - x * a - y * b - z * c,
        ])
    }
    fn rotate(self, p: V3) -> V3 {
        let q = [self.0[0], self.0[1], self.0[2]];
        let t = mul(cross(q, p), 2.);
        add(p, add(mul(t, self.0[3]), cross(q, t)))
    }
    fn toward(a: V3, b: V3) -> Self {
        let d = dot(a, b);
        if d < -0.9999 {
            Self::axis([1., 0., 0.], PI)
        } else {
            let c = cross(a, b);
            Self([c[0], c[1], c[2], 1. + d]).normalized()
        }
    }
    fn blend(self, mut b: Self, t: f32) -> Self {
        if self.0.iter().zip(b.0).map(|(a, b)| a * b).sum::<f32>() < 0. {
            b.0 = b.0.map(|n| -n);
        }
        Self(std::array::from_fn(|i| mix(self.0[i], b.0[i], t))).normalized()
    }
}
#[derive(Clone)]
struct Face {
    points: Vec<V3>,
    normal: V3,
    center: V3,
    u: V3,
    v: V3,
    value: u8,
}
impl Face {
    fn new(mut points: Vec<V3>, value: u8) -> Self {
        let center = mul(
            points.iter().copied().fold([0.; 3], add),
            1. / points.len() as f32,
        );
        let mut normal = norm(cross(sub(points[1], points[0]), sub(points[2], points[0])));
        if dot(normal, center) < 0. {
            points.reverse();
            normal = mul(normal, -1.);
        }
        let u = norm(sub(points[1], points[0]));
        let v = norm(cross(normal, u));
        Self {
            points,
            normal,
            center,
            u,
            v,
            value,
        }
    }
}
fn cube() -> &'static Vec<Face> {
    static CUBE: OnceLock<Vec<Face>> = OnceLock::new();
    CUBE.get_or_init(|| {
        let p = 0.62;
        vec![
            Face::new(vec![[-p, -p, p], [p, -p, p], [p, p, p], [-p, p, p]], 1),
            Face::new(vec![[-p, p, -p], [-p, p, p], [p, p, p], [p, p, -p]], 2),
            Face::new(vec![[p, -p, -p], [p, p, -p], [p, p, p], [p, -p, p]], 3),
            Face::new(vec![[-p, -p, p], [-p, p, p], [-p, p, -p], [-p, -p, -p]], 4),
            Face::new(vec![[-p, -p, -p], [p, -p, -p], [p, -p, p], [-p, -p, p]], 5),
            Face::new(vec![[p, -p, -p], [-p, -p, -p], [-p, p, -p], [p, p, -p]], 6),
        ]
    })
}
fn d20() -> &'static Vec<Face> {
    static D20: OnceLock<Vec<Face>> = OnceLock::new();
    D20.get_or_init(|| {
        // Hull triangles of the twelve golden-rectangle vertices. Construction
        // happens once; all face orderings and labels are deterministic.
        let g = (1. + 5_f32.sqrt()) / 2.;
        let mut vertices = vec![];
        for a in [-1., 1.] {
            for b in [-g, g] {
                vertices.extend([[0., a, b], [a, b, 0.], [b, 0., a]]);
            }
        }
        let vertices: Vec<_> = vertices.into_iter().map(|p| mul(norm(p), 0.9)).collect();
        let mut faces = vec![];
        for a in 0..12 {
            for b in a + 1..12 {
                for c in b + 1..12 {
                    let normal =
                        cross(sub(vertices[b], vertices[a]), sub(vertices[c], vertices[a]));
                    let mut positive = false;
                    let mut negative = false;
                    for (i, p) in vertices.iter().enumerate() {
                        if [a, b, c].contains(&i) {
                            continue;
                        }
                        let d = dot(normal, sub(*p, vertices[a]));
                        positive |= d > 1e-4;
                        negative |= d < -1e-4;
                    }
                    if !(positive && negative) {
                        faces.push(Face::new(
                            vec![vertices[a], vertices[b], vertices[c]],
                            faces.len() as u8 + 1,
                        ));
                    }
                }
            }
        }
        faces
    })
}
fn resting(mesh: &[Face], value: u8) -> Q {
    let face = mesh.iter().find(|f| f.value == value).unwrap_or(&mesh[0]);
    let q = Q::toward(face.normal, [0., 0., 1.]);
    let u = q.rotate(face.u);
    // Keep the recorded face dominant while retaining visible side faces at rest.
    Q::axis([1., 0., 0.], -0.18)
        .product(Q::axis([0., 1., 0.], 0.23))
        .product(Q::axis([0., 0., 1.], -u[1].atan2(u[0])).product(q))
}
struct Random(u32);
impl Random {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b9);
        let mut n = self.0;
        n = (n ^ (n >> 16)).wrapping_mul(0x21f0aaad);
        n = (n ^ (n >> 15)).wrapping_mul(0x735a2d97);
        ((n ^ (n >> 15)) >> 8) as f32 / 16_777_216.
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        mix(a, b, self.unit())
    }
}
struct Canvas {
    vertices: Vec<f32>,
    viewport: [f32; 2],
}
impl Canvas {
    fn triangle(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2], color: Color) {
        // Sutherland-Hodgman clipping also bounds every retained vertex. Input
        // overlays remain decorative; no scene node or responder is allocated.
        let mut polygon = vec![a, b, c];
        for (axis, bound, lower) in [
            (0, 0., true),
            (0, self.viewport[0], false),
            (1, 0., true),
            (1, self.viewport[1], false),
        ] {
            if polygon.is_empty() {
                return;
            }
            let mut clipped = Vec::with_capacity(7);
            let mut previous = *polygon.last().unwrap();
            let mut previous_inside = if lower {
                previous[axis] >= bound
            } else {
                previous[axis] <= bound
            };
            for point in polygon {
                let inside = if lower {
                    point[axis] >= bound
                } else {
                    point[axis] <= bound
                };
                if inside != previous_inside {
                    let t = (bound - previous[axis]) / (point[axis] - previous[axis]);
                    clipped.push([mix(previous[0], point[0], t), mix(previous[1], point[1], t)]);
                }
                if inside {
                    clipped.push(point);
                }
                previous = point;
                previous_inside = inside;
            }
            polygon = clipped;
        }
        for i in 1..polygon.len().saturating_sub(1) {
            for p in [polygon[0], polygon[i], polygon[i + 1]] {
                self.vertices
                    .extend([p[0], p[1], color[0], color[1], color[2], color[3]]);
            }
        }
    }
    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: Color) {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0].hypot(d[1]).max(1e-5);
        let n = [-d[1] / len * width * 0.5, d[0] / len * width * 0.5];
        let p = |a: [f32; 2], s: f32| [a[0] + n[0] * s, a[1] + n[1] * s];
        self.triangle(p(a, 1.), p(b, 1.), p(a, -1.), color);
        self.triangle(p(a, -1.), p(b, 1.), p(b, -1.), color);
    }
    fn ellipse(&mut self, at: [f32; 2], r: [f32; 2], color: Color) {
        self.ellipse_steps(at, r, color, 16);
    }
    fn ellipse_steps(&mut self, at: [f32; 2], r: [f32; 2], color: Color, steps: usize) {
        for i in 0..steps {
            let point = |i: usize| {
                let a = i as f32 / steps as f32 * TAU;
                [at[0] + a.cos() * r[0], at[1] + a.sin() * r[1]]
            };
            self.triangle(at, point(i), point(i + 1), color);
        }
    }
}

pub(super) fn duration(event: &MessageEvent) -> u64 {
    match event {
        MessageEvent::Steam { .. } => 4800,
        MessageEvent::Boop { .. } => 5600,
        MessageEvent::Salute { .. } => 4000,
        MessageEvent::Dice { .. } => 4300,
    }
}

/// Straight RGBA triangle vertices, consumed by the shared effects layer.
pub(super) fn vertices(
    event: &MessageEvent,
    elapsed_ms: u64,
    viewport: [f32; 2],
    origin: [f32; 2],
    dark: bool,
    quiet: bool,
) -> Vec<f32> {
    if elapsed_ms >= duration(event)
        || viewport
            .iter()
            .any(|n| !n.is_finite() || *n <= 0. || *n > 8192.)
        || origin.iter().any(|n| !n.is_finite())
    {
        return vec![];
    }
    let mut canvas = Canvas {
        vertices: Vec::new(),
        viewport,
    };
    let t = elapsed_ms as f32 / 1000.;
    let origin = [
        origin[0].clamp(24., (viewport[0] - 24.).max(24.)),
        origin[1].clamp(24., (viewport[1] - 24.).max(24.)),
    ];
    match event {
        MessageEvent::Steam { seed } => steam(&mut canvas, *seed, t, origin, dark, quiet),
        MessageEvent::Boop { seed, .. } => boop(&mut canvas, *seed, t, origin, dark, quiet),
        MessageEvent::Salute { seed, .. } => salute(&mut canvas, *seed, t, origin, dark, quiet),
        MessageEvent::Dice { seed, sides, rolls } => {
            dice(&mut canvas, *seed, *sides, rolls, t, origin, dark, quiet)
        }
    }
    canvas.vertices
}
fn steam(canvas: &mut Canvas, seed: u32, t: f32, _at: [f32; 2], dark: bool, quiet: bool) {
    let [w, h] = canvas.viewport;
    let mut random = Random(seed);
    for _ in 0..if quiet { 8 } else { 20 } {
        let delay = random.range(0., 1.05);
        let age = t - delay;
        let spread = random.range(-0.18, 1.18);
        let drift = random.range(-0.22, 0.22);
        let phase = random.range(0., TAU);
        let size = random.range(0.12, 0.25);
        if !(0. ..3.7).contains(&age) {
            continue;
        }
        let f = age / 3.7;
        let radius = w * (size + f * if quiet { 0.32 } else { 0.53 });
        let p = [
            w * (spread + drift * f + (f * 5. + phase).sin() * 0.035),
            h + radius * 0.35 - f * (h + radius * 2.5),
        ];
        let alpha =
            smooth(f / 0.12) * (1. - smooth((f - 0.55) / 0.45)) * if quiet { 0.55 } else { 1.15 };
        let shade = if dark { 0.87 } else { 0.58 } + random.range(-0.035, 0.035);
        // Wide translucent shells soften the cloud edge without textures,
        // blur passes, screen-size render targets or unbounded particle state.
        for (factor, opacity) in [
            (1.45, 0.045),
            (1.20, 0.065),
            (1., 0.10),
            (0.75, 0.13),
            (0.48, 0.18),
        ] {
            canvas.ellipse_steps(
                p,
                [radius * factor, radius * factor * 0.84],
                [shade, shade + 0.012, shade + 0.022, alpha * opacity],
                32,
            );
        }
    }
}

struct SolidFace {
    points: Vec<V3>,
    normal: V3,
    center: V3,
    color: [f32; 3],
}
impl SolidFace {
    fn new(mut points: Vec<V3>, color: [f32; 3], outward: V3) -> Self {
        let mut normal = norm(cross(sub(points[1], points[0]), sub(points[2], points[0])));
        if dot(normal, outward) < 0. {
            points.reverse();
            normal = mul(normal, -1.);
        }
        let center = mul(
            points.iter().copied().fold([0.; 3], add),
            1. / points.len() as f32,
        );
        Self {
            points,
            normal,
            center,
            color,
        }
    }
}
struct Stage {
    center: [f32; 2],
    scale: f32,
    rotation: Q,
}
impl Stage {
    fn project(&self, p: V3) -> [f32; 2] {
        let p = self.rotation.rotate(p);
        let depth = 8. / (8. - p[2]).max(2.);
        [
            self.center[0] + p[0] * self.scale * depth,
            self.center[1] - p[1] * self.scale * depth,
        ]
    }
}
fn solid(canvas: &mut Canvas, mesh: &[SolidFace], stage: &Stage, alpha: f32, edge: Color) {
    if alpha <= 0. {
        return;
    }
    let light = norm([-0.5, 0.8, 1.]);
    let half = norm(add(light, [0., 0., 1.]));
    let mut faces = vec![];
    for face in mesh {
        let center = stage.rotation.rotate(face.center);
        let normal = stage.rotation.rotate(face.normal);
        if dot(normal, [-center[0], -center[1], 8. - center[2]]) > 0. {
            faces.push((center[2], face, normal));
        }
    }
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, face, normal) in faces {
        let shade = 0.30 + 0.70 * dot(normal, light).max(0.);
        let shine = dot(normal, half).max(0.).powi(24) * 0.16;
        let color = [
            (face.color[0] * shade + shine).min(1.),
            (face.color[1] * shade + shine).min(1.),
            (face.color[2] * shade + shine).min(1.),
            alpha,
        ];
        let points: Vec<_> = face.points.iter().map(|p| stage.project(*p)).collect();
        for i in 1..points.len() - 1 {
            canvas.triangle(points[0], points[i], points[i + 1], color);
        }
        if edge[3] > 0. {
            for i in 0..points.len() {
                canvas.line(
                    points[i],
                    points[(i + 1) % points.len()],
                    (stage.scale * 0.005).clamp(0.6, 1.3),
                    [edge[0], edge[1], edge[2], edge[3] * alpha],
                );
            }
        }
    }
}
fn soccer_ball() -> &'static Vec<SolidFace> {
    static BALL: OnceLock<Vec<SolidFace>> = OnceLock::new();
    BALL.get_or_init(|| {
        // Truncate each directed edge of an icosahedron by one third: twelve
        // black pentagons and twenty white hexagons make the classic ball.
        let mut vertices: Vec<V3> = vec![];
        for face in d20() {
            for p in &face.points {
                if !vertices
                    .iter()
                    .any(|q| dot(sub(*p, *q), sub(*p, *q)) < 1e-6)
                {
                    vertices.push(*p);
                }
            }
        }
        let edge = |a: V3, b: V3| norm(add(mul(a, 2.), b));
        let mut result = vec![];
        for a in &vertices {
            let mut neighbors: Vec<V3> = vec![];
            for face in d20().iter().filter(|f| f.points.contains(a)) {
                for b in &face.points {
                    if b != a && !neighbors.contains(b) {
                        neighbors.push(*b);
                    }
                }
            }
            let normal = norm(*a);
            let u = norm(cross(
                normal,
                if normal[1].abs() < 0.9 {
                    [0., 1., 0.]
                } else {
                    [1., 0., 0.]
                },
            ));
            let v = cross(normal, u);
            neighbors.sort_by(|b, c| {
                dot(*b, v)
                    .atan2(dot(*b, u))
                    .total_cmp(&dot(*c, v).atan2(dot(*c, u)))
            });
            result.push(SolidFace::new(
                neighbors.into_iter().map(|b| edge(*a, b)).collect(),
                [0.055, 0.065, 0.08],
                normal,
            ));
        }
        for face in d20() {
            let [a, b, c] = [face.points[0], face.points[1], face.points[2]];
            result.push(SolidFace::new(
                vec![
                    edge(a, b),
                    edge(b, a),
                    edge(b, c),
                    edge(c, b),
                    edge(c, a),
                    edge(a, c),
                ],
                [0.94, 0.945, 0.91],
                face.normal,
            ));
        }
        result
    })
}
fn ripple_point(origin: [f32; 2], viewport: [f32; 2], radius: f32, angle: f32) -> [f32; 2] {
    let x = angle.cos() * radius;
    let z = angle.sin() * radius;
    let depth = 8. / (8. + z).max(1.8);
    let horizon = [
        viewport[0] * 0.52,
        (origin[1] - viewport[1] * 0.45).max(viewport[1] * 0.14),
    ];
    [
        horizon[0] + (origin[0] - horizon[0] + x * viewport[0] * 0.13) * depth,
        horizon[1] + (origin[1] - horizon[1]) * depth,
    ]
}
fn boop(canvas: &mut Canvas, seed: u32, t: f32, at: [f32; 2], dark: bool, quiet: bool) {
    let [w, h] = canvas.viewport;
    let impact = 1.55;
    if t < 2.45 {
        let travel = smooth(t / impact);
        let bounce = ((t - impact) / 0.9).clamp(0., 1.);
        let radius = if t <= impact {
            mix(w * if quiet { 0.34 } else { 0.56 }, 18., travel)
        } else {
            mix(18., 8., bounce)
        };
        let center = if t <= impact {
            [
                mix(w + w * 0.24, at[0], travel),
                mix(h * 0.53, at[1], travel)
                    - (t / impact * PI).sin() * h * if quiet { 0.025 } else { 0.10 },
            ]
        } else {
            [
                at[0] + bounce * if quiet { 42. } else { 100. },
                at[1] - (bounce * PI).sin() * if quiet { 20. } else { 56. },
            ]
        };
        let rotation = Q::axis([0., 0., 1.], t * if quiet { 3.5 } else { 7.5 })
            .product(Q::axis(
                [0., 1., 0.],
                t * 1.2 + (seed % 4096) as f32 / 4096. * TAU,
            ))
            .product(Q::axis([1., 0., 0.], 0.25));
        let alpha = 1. - smooth(bounce);
        canvas.ellipse(
            [center[0], center[1] + radius * 0.95],
            [radius * 0.85, radius * 0.16],
            [0., 0., 0., alpha * 0.17],
        );
        solid(
            canvas,
            soccer_ball(),
            &Stage {
                center,
                scale: radius,
                rotation,
            },
            alpha,
            [0.08, 0.10, 0.11, 0.75],
        );
    }
    for i in 0..if quiet { 2 } else { 4 } {
        let age = t - impact - i as f32 * 0.25;
        if !(0. ..3.1).contains(&age) {
            continue;
        }
        let radius = 0.12 + age * if quiet { 3.4 } else { 5.2 };
        let width = 0.055 + age * 0.018;
        let alpha = smooth(age / 0.12) * (1. - age / 3.1).powf(1.2);
        let color = if dark {
            [0.85, 0.65, 0.37, alpha * 0.85]
        } else {
            [0.48, 0.29, 0.16, alpha * 0.8]
        };
        for j in 0..72 {
            let a = j as f32 / 72. * TAU;
            let b = (j + 1) as f32 / 72. * TAU;
            let p = |r, a| ripple_point(at, canvas.viewport, r, a);
            let [p0, p1, p2, p3] = [
                p(radius - width, a),
                p(radius - width, b),
                p(radius + width, a),
                p(radius + width, b),
            ];
            canvas.triangle(p0, p1, p2, color);
            canvas.triangle(p2, p1, p3, color);
        }
    }
}
fn top_hat() -> &'static Vec<SolidFace> {
    static HAT: OnceLock<Vec<SolidFace>> = OnceLock::new();
    HAT.get_or_init(|| {
        let mut faces = vec![];
        let point = |radius: [f32; 2], y: f32, angle: f32| {
            [angle.cos() * radius[0], y, angle.sin() * radius[1]]
        };
        let mut cylinder = |bottom: [f32; 2], top: [f32; 2], y0: f32, y1: f32, color: [f32; 3]| {
            for i in 0..40 {
                let a = i as f32 / 40. * TAU;
                let b = (i + 1) as f32 / 40. * TAU;
                faces.push(SolidFace::new(
                    vec![
                        point(bottom, y0, a),
                        point(bottom, y0, b),
                        point(top, y1, b),
                        point(top, y1, a),
                    ],
                    color,
                    [((a + b) * 0.5).cos(), 0., ((a + b) * 0.5).sin()],
                ));
            }
        };
        cylinder([1.66, 1.35], [1.66, 1.35], -1.15, -0.99, [0.27, 0.24, 0.18]);
        // Split the crown at the band boundaries. Separate overlapping
        // full-height cylinders would defeat painter-order occlusion while tipped.
        cylinder(
            [0.78, 0.78],
            [0.7833, 0.7833],
            -0.99,
            -0.94,
            [0.19, 0.21, 0.245],
        );
        cylinder(
            [0.7833, 0.7833],
            [0.8188, 0.8188],
            -0.94,
            -0.41,
            [0.74, 0.46, 0.24],
        );
        cylinder(
            [0.8188, 0.8188],
            [0.95, 0.95],
            -0.41,
            1.55,
            [0.19, 0.21, 0.245],
        );
        for i in 0..40 {
            let a = i as f32 / 40. * TAU;
            let b = (i + 1) as f32 / 40. * TAU;
            faces.push(SolidFace::new(
                vec![
                    [0., 1.55, 0.],
                    point([0.95, 0.95], 1.55, a),
                    point([0.95, 0.95], 1.55, b),
                ],
                [0.24, 0.265, 0.30],
                [0., 1., 0.],
            ));
            faces.push(SolidFace::new(
                vec![
                    point([0.78, 0.78], -0.99, a),
                    point([1.66, 1.35], -0.99, a),
                    point([1.66, 1.35], -0.99, b),
                    point([0.78, 0.78], -0.99, b),
                ],
                [0.22, 0.24, 0.27],
                [0., 1., 0.],
            ));
            faces.push(SolidFace::new(
                vec![
                    [0., -1.15, 0.],
                    point([1.66, 1.35], -1.15, a),
                    point([1.66, 1.35], -1.15, b),
                ],
                [0.11, 0.12, 0.14],
                [0., -1., 0.],
            ));
        }
        faces
    })
}
fn salute(canvas: &mut Canvas, seed: u32, t: f32, _at: [f32; 2], _dark: bool, quiet: bool) {
    let [w, h] = canvas.viewport;
    let enter = smooth(t / 0.55);
    let leave = smooth((t - 2.7) / 1.25);
    let tip = ((t - 0.45) / 2.3 * PI).sin().max(0.);
    let scale = w.min(h * 0.85) / 3.25
        * (1.08 + tip * if quiet { 0.03 } else { 0.13 })
        * (1. - leave * 0.25);
    let center = [
        w * 0.5 + (t * 2.).sin() * w * if quiet { 0.005 } else { 0.025 },
        if quiet {
            h * 0.53 - leave * h * 0.4
        } else {
            mix(h + scale * 1.7, h * 0.53, enter) - leave * h * 0.5
        },
    ];
    let rotation = Q::axis([0., 0., 1.], -tip * if quiet { 0.10 } else { 0.32 })
        .product(Q::axis(
            [1., 0., 0.],
            0.30 + tip * if quiet { 0.12 } else { 0.55 },
        ))
        .product(Q::axis(
            [0., 1., 0.],
            (t * 1.8 + (seed % 4096) as f32 / 4096. * TAU).sin() * if quiet { 0.08 } else { 0.35 },
        ));
    let alpha = smooth(t / 0.2) * (1. - leave);
    solid(
        canvas,
        top_hat(),
        &Stage {
            center,
            scale,
            rotation,
        },
        alpha,
        [0.66, 0.53, 0.34, 0.],
    );
}

struct Body {
    center: V3,
    rotation: Q,
    floor: f32,
    scale: f32,
    alpha: f32,
}
fn body(
    seed: u32,
    index: usize,
    count: usize,
    mesh: &[Face],
    value: u8,
    t: f32,
    viewport: [f32; 2],
    origin: [f32; 2],
    quiet: bool,
) -> Body {
    let mut random = Random(seed.wrapping_add(index as u32 * 7919));
    let columns = count.min(5);
    let rows = count.div_ceil(columns);
    let scale = (viewport[0] / (columns as f32 + 1.) * 0.5).clamp(18., 46.);
    let row = index / columns;
    let col = index % columns;
    let target_x = (col as f32 - (columns as f32 - 1.) * 0.5) * 2.1;
    let floor = origin[1].clamp(
        (rows as f32 * scale * 2.2 + 90.).min(viewport[1] * 0.7),
        (viewport[1] - 58.).max(100.),
    ) - (rows - 1 - row) as f32 * scale * 2.25;
    let bound = (viewport[0] / (2. * scale) - 1.1).max(0.1);
    let launch_x = -viewport[0] / (2. * scale) - 1.6 - index as f32 * 0.18;
    let mut p = [
        launch_x,
        random.range(if quiet { 1.6 } else { 2.4 }, if quiet { 2.2 } else { 3.9 }),
        random.range(-0.7, 0.7),
    ];
    let mut v = [
        random.range(if quiet { 3.8 } else { 4.8 }, if quiet { 4.6 } else { 6.2 }),
        random.range(2., 4.6),
        random.range(-1.3, 1.3),
    ];
    let dt = 1. / 120.;
    let time = t.min(2.8);
    let steps = (time / dt).floor() as usize;
    for step in 0..steps {
        v[1] -= if quiet { 15. } else { 11. } * dt;
        for axis in 0..3 {
            p[axis] += v[axis] * dt;
        }
        if p[1] < 1. {
            p[1] = 1.;
            v[1] = v[1].abs() * 0.53;
            v[0] *= 0.78;
            v[2] *= 0.78;
        }
        // A die may begin outside the left edge while entering. The left
        // wall applies only once it is travelling back toward that edge.
        if p[0] > bound || (p[0] < -bound && v[0] < 0.) {
            p[0] = p[0].clamp(-bound, bound);
            v[0] *= -0.72;
        }
        if p[2].abs() > 1.2 {
            p[2] = p[2].clamp(-1.2, 1.2);
            v[2] *= -0.6;
        }
        if step > 240 {
            v[0] *= 0.99;
            v[2] *= 0.99;
        }
    }
    let initial = Q::axis(
        norm([
            random.range(-1., 1.),
            random.range(-1., 1.),
            random.range(-1., 1.),
        ]),
        random.range(0., TAU),
    );
    // World +Y projects upward. A negative Z roll therefore moves a top
    // marking toward screen-right: clockwise, matching left-to-right travel.
    // The small oblique components give it a genuine three-dimensional tumble.
    let travel = p[0] - launch_x;
    let spin = Q::axis(norm([0.12, 0.17, -1.]), travel / 0.72).product(initial);
    let final_rotation = resting(mesh, value);
    let settle = smooth((t - 1.85) / 0.95);
    let rest_height = mesh
        .iter()
        .flat_map(|f| &f.points)
        .map(|p| -final_rotation.rotate(*p)[1])
        .fold(0_f32, f32::max);
    p[0] = mix(p[0], target_x.clamp(-bound, bound), settle);
    p[1] = mix(p[1], rest_height, settle);
    p[2] = mix(p[2], 0., settle);
    Body {
        center: p,
        rotation: spin.blend(final_rotation, settle),
        floor,
        scale,
        alpha: (1. - (t - 3.55).max(0.) / 0.75).clamp(0., 1.),
    }
}
fn project(body: &Body, p: V3, width: f32) -> [f32; 2] {
    let p = add(body.rotation.rotate(p), body.center);
    let depth = 12. / (12. - p[2]).max(4.);
    [
        width * 0.5 + p[0] * body.scale * depth,
        body.floor - p[1] * body.scale * depth,
    ]
}
fn face_point(face: &Face, x: f32, y: f32) -> V3 {
    add(
        add(mul(face.normal, 0.004), face.center),
        add(mul(face.u, x), mul(face.v, y)),
    )
}
fn digit_lines(digit: u8) -> &'static [(u8, u8)] {
    // Seven-segment strokes in a face-local plane, not screen-space labels.
    const A: (u8, u8) = (0, 1);
    const B: (u8, u8) = (1, 2);
    const C: (u8, u8) = (2, 3);
    const D: (u8, u8) = (3, 4);
    const E: (u8, u8) = (4, 5);
    const F: (u8, u8) = (5, 0);
    const G: (u8, u8) = (5, 2);
    match digit {
        0 => &[A, B, C, D, E, F],
        1 => &[B, C],
        2 => &[A, B, G, E, D],
        3 => &[A, B, G, C, D],
        4 => &[F, G, B, C],
        5 => &[A, F, G, C, D],
        6 => &[A, F, G, E, C, D],
        7 => &[A, B, C],
        8 => &[A, B, C, D, E, F, G],
        _ => &[A, B, C, D, F, G],
    }
}
fn face_number(canvas: &mut Canvas, body: &Body, face: &Face, sides: u8, alpha: f32) {
    let ink = [0.13, 0.15, 0.15, alpha];
    let w = canvas.viewport[0];
    if sides == 6 {
        let positions: &[[f32; 2]] = match face.value {
            1 => &[[0., 0.]],
            2 => &[[-0.25, -0.25], [0.25, 0.25]],
            3 => &[[-0.25, -0.25], [0., 0.], [0.25, 0.25]],
            4 => &[[-0.25, -0.25], [0.25, -0.25], [-0.25, 0.25], [0.25, 0.25]],
            5 => &[
                [-0.25, -0.25],
                [0.25, -0.25],
                [0., 0.],
                [-0.25, 0.25],
                [0.25, 0.25],
            ],
            _ => &[
                [-0.25, -0.28],
                [0.25, -0.28],
                [-0.25, 0.],
                [0.25, 0.],
                [-0.25, 0.28],
                [0.25, 0.28],
            ],
        };
        for [x, y] in positions {
            let c = face_point(face, *x, *y);
            for i in 0..8 {
                let point = |i: usize| {
                    let a = i as f32 / 8. * TAU;
                    face_point(face, x + a.cos() * 0.065, y + a.sin() * 0.065)
                };
                canvas.triangle(
                    project(body, c, w),
                    project(body, point(i), w),
                    project(body, point(i + 1), w),
                    ink,
                );
            }
        }
    } else {
        let digits: Vec<_> = face
            .value
            .to_string()
            .bytes()
            .map(|digit| digit - b'0')
            .collect();
        let points = [[0., 1.], [1., 1.], [1., 0.5], [1., 0.], [0., 0.], [0., 0.5]];
        for (column, digit) in digits.iter().enumerate() {
            let offset = (column as f32 - (digits.len() as f32 - 1.) * 0.5) * 0.17;
            for (a, b) in digit_lines(*digit) {
                let local = |i: u8| {
                    face_point(
                        face,
                        (points[i as usize][0] - 0.5) * 0.115 + offset,
                        (points[i as usize][1] - 0.5) * 0.25,
                    )
                };
                canvas.line(
                    project(body, local(*a), w),
                    project(body, local(*b), w),
                    (body.scale * 0.023).clamp(0.65, 1.4),
                    ink,
                );
            }
        }
    }
}
fn dice(
    canvas: &mut Canvas,
    seed: u32,
    sides: u8,
    rolls: &[u8],
    t: f32,
    origin: [f32; 2],
    dark: bool,
    quiet: bool,
) {
    if rolls.is_empty()
        || rolls.len() > 10
        || !(2..=100).contains(&sides)
        || rolls.iter().any(|n| *n == 0 || *n > sides)
    {
        return;
    }
    let mesh = if sides == 20 { d20() } else { cube() };
    let mut faces = vec![];
    let bodies: Vec<_> = rolls
        .iter()
        .enumerate()
        .map(|(i, n)| {
            body(
                seed,
                i,
                rolls.len(),
                mesh,
                if sides == 6 || sides == 20 { *n } else { 1 },
                t,
                canvas.viewport,
                origin,
                quiet,
            )
        })
        .collect();
    for body in &bodies {
        let depth = 12. / (12. - body.center[2]).max(4.);
        let at = [
            canvas.viewport[0] * 0.5 + body.center[0] * body.scale * depth,
            body.floor + 2.,
        ];
        canvas.ellipse(
            at,
            [body.scale * 0.83, body.scale * 0.19],
            [
                0.,
                0.,
                0.,
                body.alpha * (if dark { 0.33 } else { 0.16 }) / (1. + body.center[1] * 0.2),
            ],
        );
    }
    for (index, body) in bodies.iter().enumerate() {
        for face in mesh {
            let normal = body.rotation.rotate(face.normal);
            if normal[2] > 0.025 {
                let z = body.rotation.rotate(face.center)[2] + body.center[2];
                faces.push((z, index, face, normal));
            }
        }
    }
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    let light = norm([-0.4, 0.75, 1.]);
    for (_, index, face, normal) in faces {
        let body = &bodies[index];
        let shade = 0.38 + 0.62 * dot(normal, light).max(0.);
        let palette = if index % 2 == 0 {
            [0.9, 0.73, 0.43]
        } else {
            [0.78, 0.52, 0.34]
        };
        let color = [
            palette[0] * shade,
            palette[1] * shade,
            palette[2] * shade,
            body.alpha,
        ];
        let points: Vec<_> = face
            .points
            .iter()
            .map(|p| project(body, *p, canvas.viewport[0]))
            .collect();
        for i in 1..points.len() - 1 {
            canvas.triangle(points[0], points[i], points[i + 1], color);
        }
        for i in 0..points.len() {
            canvas.line(
                points[i],
                points[(i + 1) % points.len()],
                0.75,
                [0.2, 0.18, 0.14, body.alpha * 0.65],
            );
        }
        if sides == 6 || sides == 20 {
            face_number(canvas, body, face, sides, body.alpha);
        } else if face.value == 1 {
            // Non-standard sizes retain their exact authoritative value. A
            // numbered result block avoids pretending it is a physical dN.
            let mut label = face.clone();
            label.value = rolls[index];
            face_number(canvas, body, &label, 20, body.alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dice_meshes_settle_to_the_recorded_face() {
        assert_eq!(d20().len(), 20);
        for (mesh, sides) in [(cube(), 6), (d20(), 20)] {
            for value in 1..=sides {
                let body = body(99, 0, 1, mesh, value, 3., [390., 720.], [195., 600.], false);
                let q = body.rotation;
                let bottom = mesh
                    .iter()
                    .flat_map(|face| &face.points)
                    .map(|p| q.rotate(*p)[1] + body.center[1])
                    .fold(f32::INFINITY, f32::min);
                assert!(bottom.abs() < 1e-4, "resting die meets its floor");
                let front = mesh
                    .iter()
                    .max_by(|a, b| q.rotate(a.normal)[2].total_cmp(&q.rotate(b.normal)[2]))
                    .unwrap();
                assert_eq!(front.value, value);
                assert!(q.rotate(front.normal)[2] > 0.95);
            }
        }
    }
    #[test]
    fn left_entering_dice_roll_clockwise_with_their_forward_travel() {
        for seed in [1, 19, 77, 4_294_967_295] {
            let a = body(seed, 0, 1, cube(), 6, 0., [390., 720.], [195., 550.], false);
            let b = body(
                seed,
                0,
                1,
                cube(),
                6,
                0.12,
                [390., 720.],
                [195., 550.],
                false,
            );
            assert!(
                project(&a, [0.; 3], 390.)[0] < 0.,
                "die starts off the left edge"
            );
            assert!(
                b.center[0] > a.center[0],
                "entry travels toward screen-right"
            );
            let [x, y, z, w] = a.rotation.0;
            let local_top = Q([-x, -y, -z, w]).rotate([0., 1., 0.]);
            let next_top = b.rotation.rotate(local_top);
            assert!(
                next_top[0] > 0. && next_top[1] > 0.,
                "a top marking advances rightward, clockwise on screen"
            );
        }
    }
    #[test]
    fn effects_are_deterministic_bounded_and_finite_across_phone_and_frame_sizes() {
        let target = otrios_interfaces::chat::Profile {
            id: "1".repeat(32),
            name: "Pilot Piston".into(),
            avatar: 0,
        };
        let events = [
            MessageEvent::Steam { seed: 19 },
            MessageEvent::Boop {
                seed: 29,
                target: target.clone(),
            },
            MessageEvent::Salute { seed: 39, target },
            MessageEvent::Dice {
                seed: 42,
                sides: 6,
                rolls: vec![1, 6, 4, 3, 2, 5, 6, 1, 2, 3],
            },
            MessageEvent::Dice {
                seed: 77,
                sides: 20,
                rolls: vec![1, 20, 11, 7],
            },
            MessageEvent::Dice {
                seed: 91,
                sides: 100,
                rolls: vec![100, 9],
            },
        ];
        for viewport in [[320., 568.], [390., 720.], [1280., 800.]] {
            for event in &events {
                for ms in [0, 250, 1000, 2300, 2800, 4000] {
                    let a = vertices(
                        event,
                        ms,
                        viewport,
                        [viewport[0] * 0.5, viewport[1] - 100.],
                        true,
                        false,
                    );
                    assert_eq!(
                        a,
                        vertices(
                            event,
                            ms,
                            viewport,
                            [viewport[0] * 0.5, viewport[1] - 100.],
                            true,
                            false
                        )
                    );
                    assert!(a.len() < 200_000);
                    assert_eq!(a.len() % 18, 0);
                    for vertex in a.chunks_exact(6) {
                        assert!(vertex.iter().all(|n| n.is_finite()));
                        assert!((-0.001..=viewport[0] + 0.001).contains(&vertex[0]));
                        assert!((-0.001..=viewport[1] + 0.001).contains(&vertex[1]));
                        assert!((0. ..=1.).contains(&vertex[5]));
                    }
                }
                assert!(
                    vertices(event, duration(event), viewport, [20., 30.], false, false).is_empty()
                );
            }
        }
    }
}
