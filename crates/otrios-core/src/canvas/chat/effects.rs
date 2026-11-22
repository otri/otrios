// SPDX-License-Identifier: LGPL-3.0-only
//! Local, bounded procedural effects. A recorded result is never rolled again.
use otrios_interfaces::chat::MessageEvent;
use std::{
    f32::consts::{PI, TAU},
    sync::OnceLock,
};

#[path = "effects/locomotive.rs"]
mod locomotive;

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
        self.gradient_triangle([a, b, c], [color; 3]);
    }
    fn gradient_triangle(&mut self, points: [[f32; 2]; 3], colors: [Color; 3]) {
        // Clip both position and colour: feathered clouds remain smooth at the
        // viewport edge, and decorative triangles never allocate responders.
        let mut polygon: Vec<[f32; 6]> = points
            .into_iter()
            .zip(colors)
            .map(|(p, c)| [p[0], p[1], c[0], c[1], c[2], c[3]])
            .collect();
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
            let inside = |p: [f32; 6]| {
                if lower {
                    p[axis] >= bound
                } else {
                    p[axis] <= bound
                }
            };
            for point in polygon {
                if inside(point) != inside(previous) {
                    let t = (bound - previous[axis]) / (point[axis] - previous[axis]);
                    clipped.push(std::array::from_fn(|i| mix(previous[i], point[i], t)));
                }
                if inside(point) {
                    clipped.push(point);
                }
                previous = point;
            }
            polygon = clipped;
        }
        for i in 1..polygon.len().saturating_sub(1) {
            for p in [polygon[0], polygon[i], polygon[i + 1]] {
                self.vertices.extend(p);
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
        MessageEvent::Steam { .. } => 6800,
        MessageEvent::Boop { .. } => 5600,
        MessageEvent::Salute { .. } => 2100,
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
        MessageEvent::Salute { .. } => {}
        MessageEvent::Dice { seed, sides, rolls } => {
            dice(&mut canvas, *seed, *sides, rolls, t, origin, dark, quiet)
        }
    }
    canvas.vertices
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Wisp {
    birth: f32,
    phase: f32,
    drift: f32,
    size: f32,
    shade: f32,
}
fn wisp(seed: u32, index: u32) -> Wisp {
    // Independent per-wisp random streams never depend on which wisps happen
    // to be visible. The former shared stream skipped shade samples before a
    // birth and reassigned later particles' parameters, causing visible jumps.
    let mut random = Random(seed.wrapping_add(index.wrapping_mul(7919)));
    Wisp {
        birth: 0.9 + index as f32 * 0.14,
        phase: random.range(0., TAU),
        drift: random.range(-0.20, -0.06),
        size: random.range(0.065, 0.11),
        shade: random.range(-0.025, 0.025),
    }
}
fn steam(canvas: &mut Canvas, seed: u32, t: f32, _at: [f32; 2], dark: bool, quiet: bool) {
    locomotive::draw(canvas, t, quiet);
    let [w, h] = canvas.viewport;
    for i in 0..if quiet { 10 } else { 20 } {
        let wisp = wisp(seed, if quiet { i * 2 } else { i });
        let age = t - wisp.birth;
        if !(0. ..3.2).contains(&age) {
            continue;
        }
        let f = age / 3.2;
        let source =
            locomotive::stage(wisp.birth, canvas.viewport, quiet).project([1.15, 1.84, 0.]);
        let radius = w * (wisp.size + smooth(f) * if quiet { 0.50 } else { 0.85 });
        let curl = (f * 4. + wisp.phase).sin() - wisp.phase.sin();
        let center = [
            source[0] + w * (wisp.drift * f + curl * f * 0.10),
            source[1] - h * f * 0.55,
        ];
        let alpha =
            smooth(age / 0.28) * (1. - smooth((f - 0.50) / 0.50)) * if quiet { 0.28 } else { 0.42 };
        let shade = if dark { 0.87 } else { 0.63 } + wisp.shade;
        // Radial vertex-alpha ramps replace concentric hard-edged shells.
        // A coherent low-frequency curl deforms the whole wisp continuously.
        let point = |r: f32, angle: f32| {
            let shape = 1. + 0.08 * (angle * 3. + wisp.phase + f * 2.).sin();
            [
                center[0] + angle.cos() * radius * r * shape,
                center[1] + angle.sin() * radius * r * shape * 0.80,
            ]
        };
        let color = |r: f32| {
            [
                shade,
                shade + 0.012,
                shade + 0.022,
                alpha * (1. - smooth(r)),
            ]
        };
        for ring in 0..4 {
            let r0 = ring as f32 / 4.;
            let r1 = (ring + 1) as f32 / 4.;
            for j in 0..24 {
                let a = j as f32 / 24. * TAU;
                let b = (j + 1) as f32 / 24. * TAU;
                canvas.gradient_triangle(
                    [point(r0, a), point(r1, a), point(r1, b)],
                    [color(r0), color(r1), color(r1)],
                );
                if ring > 0 {
                    canvas.gradient_triangle(
                        [point(r0, a), point(r1, b), point(r0, b)],
                        [color(r0), color(r1), color(r0)],
                    );
                }
            }
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

struct Body {
    center: V3,
    rotation: Q,
    floor: f32,
    scale: f32,
    alpha: f32,
}
#[derive(Clone)]
struct Particle {
    p: V3,
    v: V3,
    initial: Q,
    roll: f32,
    entered: bool,
}
fn contacts(particles: &mut [Particle], radius: f32, restitution: f32) {
    for _ in 0..8 {
        for i in 0..particles.len() {
            let (left, right) = particles.split_at_mut(i + 1);
            let a = &mut left[i];
            for b in right {
                let delta = sub(b.p, a.p);
                let distance = dot(delta, delta).sqrt();
                let diameter = 2. * radius;
                if distance >= diameter {
                    continue;
                }
                let normal = if distance > 1e-5 {
                    mul(delta, 1. / distance)
                } else {
                    [1., 0., 0.]
                };
                let correction = mul(normal, (diameter - distance + 0.0001) * 0.5);
                a.p = sub(a.p, correction);
                b.p = add(b.p, correction);
                let closing = dot(sub(b.v, a.v), normal);
                if restitution >= 0. && closing < 0. {
                    // Equal masses, bounded restitution and a little tangential
                    // friction. Collision proxies enclose every rotating vertex.
                    let impulse = mul(normal, -closing * (1. + restitution) * 0.5);
                    a.v = sub(a.v, impulse);
                    b.v = add(b.v, impulse);
                    let tangent = sub(sub(b.v, a.v), mul(normal, dot(sub(b.v, a.v), normal)));
                    a.v = add(a.v, mul(tangent, 0.035));
                    b.v = sub(b.v, mul(tangent, 0.035));
                }
            }
        }
    }
}
fn bodies(
    seed: u32,
    values: &[u8],
    mesh: &[Face],
    t: f32,
    viewport: [f32; 2],
    origin: [f32; 2],
    quiet: bool,
) -> Vec<Body> {
    let count = values.len();
    if count == 0 || count > 10 {
        return vec![];
    }
    let columns = count.min(5);
    let rows = count.div_ceil(columns);
    let scale = (viewport[0] / (columns as f32 + 1.) * 0.5).clamp(18., 46.);
    let radius = mesh
        .iter()
        .flat_map(|f| &f.points)
        .map(|p| dot(*p, *p).sqrt())
        .fold(0_f32, f32::max)
        + 0.015;
    let floor = origin[1].clamp(
        (rows as f32 * scale * 2.4 + 90.).min(viewport[1] * 0.7),
        (viewport[1] - 58.).max(100.),
    );
    let bound = (viewport[0] / (2. * scale) - radius).max(radius);
    let mut particles: Vec<_> = (0..count)
        .map(|index| {
            let mut random = Random(seed.wrapping_add(index as u32 * 7919));
            let p = [
                -viewport[0] / (2. * scale) - 1.6 - (index % columns) as f32 * 2.35,
                random.range(if quiet { 1.6 } else { 2.4 }, if quiet { 2.2 } else { 3.9 })
                    + (index / columns) as f32 * 2.4,
                random.range(-0.7, 0.7),
            ];
            let v = [
                random.range(if quiet { 5.0 } else { 6.4 }, if quiet { 6.2 } else { 8.6 }),
                random.range(2., 4.6),
                random.range(-1.3, 1.3),
            ];
            let initial = Q::axis(
                norm([
                    random.range(-1., 1.),
                    random.range(-1., 1.),
                    random.range(-1., 1.),
                ]),
                random.range(0., TAU),
            );
            Particle {
                p,
                v,
                initial,
                roll: 0.,
                entered: false,
            }
        })
        .collect();
    contacts(&mut particles, radius, 0.62);
    let rotations: Vec<_> = values.iter().map(|n| resting(mesh, *n)).collect();
    let heights: Vec<_> = rotations
        .iter()
        .map(|q| {
            mesh.iter()
                .flat_map(|f| &f.points)
                .map(|p| -q.rotate(*p)[1])
                .fold(0_f32, f32::max)
        })
        .collect();
    let mut targets = vec![[0.; 3]; count];
    let dt = 1_f32 / 120.;
    let roll_end = 1.85;
    let roll_steps = (roll_end / dt).round() as usize;
    let sample = t.min(3.3) / dt;
    let steps = sample.floor() as usize;
    let fraction = sample - steps as f32;
    let mut before = particles.clone();
    for step in 0..=steps {
        if step == roll_steps {
            // Preserve spatial order instead of pulling the front die through
            // the following one to reach an index-based result slot.
            let mut order: Vec<_> = (0..count).collect();
            order.sort_by(|a, b| {
                particles[*a].p[1]
                    .total_cmp(&particles[*b].p[1])
                    .then(a.cmp(b))
            });
            for (row, group) in order.chunks_mut(columns).enumerate() {
                group.sort_by(|a, b| {
                    particles[*a].p[0]
                        .total_cmp(&particles[*b].p[0])
                        .then(a.cmp(b))
                });
                let n = group.len();
                for (col, i) in group.iter().copied().enumerate() {
                    targets[i] = [
                        ((col as f32 - (n as f32 - 1.) * 0.5) * 2.35).clamp(-bound, bound),
                        heights[i] + row as f32 * 2.7,
                        0.,
                    ];
                }
            }
        }
        if step == steps {
            before = particles.clone();
        }
        let time = (step + 1) as f32 * dt;
        let settling = smooth((time - roll_end) / 0.90);
        let drive = smooth((time - roll_end) / 0.22);
        for (i, p) in particles.iter_mut().enumerate() {
            p.v[1] -= if quiet { 15. } else { 11. } * dt * (1. - drive);
            if step >= roll_steps {
                let omega = 18.;
                let acceleration = sub(
                    mul(sub(targets[i], p.p), omega * omega),
                    mul(p.v, 2. * omega),
                );
                p.v = add(p.v, mul(acceleration, drive * dt));
                let speed = dot(p.v, p.v).sqrt();
                if speed > 12. {
                    p.v = mul(p.v, 12. / speed);
                }
            }
            p.p = add(p.p, mul(p.v, dt));
            // Lock the source orientation once. Blending toward a fixed result
            // from a still-spinning quaternion can flip its shortest-arc sign.
            if step < roll_steps {
                p.roll += p.v[0] * dt / 0.72;
            }
            let support = mix(radius, heights[i], settling);
            if p.p[1] < support {
                p.p[1] = support;
                if p.v[1] < 0. {
                    p.v[1] = -p.v[1] * 0.53 * (1. - settling);
                }
                p.v[0] *= mix(0.78, 1., drive);
                p.v[2] *= mix(0.78, 1., drive);
            }
            // A collision may reverse an entering die while it is still off
            // screen. Do not teleport it to a wall it has not crossed yet.
            p.entered |= p.p[0] >= -bound;
            if p.p[0] > bound || (p.entered && p.p[0] < -bound) {
                p.p[0] = p.p[0].clamp(-bound, bound);
                p.v[0] *= -0.72 * (1. - settling);
            }
            if p.p[2].abs() > 2.2 {
                p.p[2] = p.p[2].clamp(-2.2, 2.2);
                p.v[2] *= -0.6;
            }
        }
        // Sleep only after subpixel convergence; this removes residual spring
        // drift without snapping the visible collision or settling trajectory.
        if time > 2.8 {
            for (i, p) in particles.iter_mut().enumerate() {
                let error = sub(p.p, targets[i]);
                if dot(error, error) < 0.0005_f32.powi(2) && dot(p.v, p.v) < 0.01_f32.powi(2) {
                    p.p = targets[i];
                    p.v = [0.; 3];
                }
            }
        }
        // Attraction, collision impulses and separation share one fixed-step
        // state. There is no later position animation overriding the solver.
        contacts(&mut particles, radius, 0.62 * (1. - settling));
    }
    let settle = smooth((t - roll_end) / 0.90);
    before
        .into_iter()
        .zip(particles)
        .enumerate()
        .map(|(i, (a, b))| {
            let center = std::array::from_fn(|j| mix(a.p[j], b.p[j], fraction));
            let roll = mix(a.roll, b.roll, fraction);
            Body {
                center,
                rotation: Q::axis(norm([0.12, 0.17, -1.]), roll)
                    .product(a.initial)
                    .blend(rotations[i], settle),
                floor,
                scale,
                alpha: (1. - (t - 3.55).max(0.) / 0.75).clamp(0., 1.),
            }
        })
        .collect()
}
#[cfg(test)]
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
    bodies(seed, &vec![value; count], mesh, t, viewport, origin, quiet).remove(index)
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
fn facing_camera(body: &Body, face: &Face) -> bool {
    let center = add(body.rotation.rotate(face.center), body.center);
    dot(
        body.rotation.rotate(face.normal),
        sub([0., 0., 12.], center),
    ) > 0.
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
    let values: Vec<_> = rolls
        .iter()
        .map(|n| if sides == 6 || sides == 20 { *n } else { 1 })
        .collect();
    let bodies = bodies(seed, &values, mesh, t, canvas.viewport, origin, quiet);
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
            if facing_camera(body, face) {
                let z = body.rotation.rotate(face.center)[2] + body.center[2];
                faces.push((z, index, face, normal));
            }
        }
    }
    // Equal enclosing collision spheres are disjoint. Their projected overlap
    // has a consistent near/far order, so paint each convex die as a whole.
    // Interleaving individual faces by average Z can swap patches during a
    // close pass even when the solids never intersect.
    faces.sort_by(|a, b| {
        let distance = |i: usize| {
            let d = sub([0., 0., 12.], bodies[i].center);
            dot(d, d)
        };
        distance(b.1)
            .total_cmp(&distance(a.1))
            .then(a.1.cmp(&b.1))
            .then(a.0.total_cmp(&b.0))
    });
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
    fn polygon_area(points: &[[f32; 2]]) -> f32 {
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
            .sum::<f32>()
            .abs()
            * 0.5
    }
    fn hull(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
        points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        points.dedup();
        let mut result: Vec<[f32; 2]> = vec![];
        for pass in 0..2 {
            let start = result.len();
            for p in if pass == 0 {
                points.clone()
            } else {
                points.iter().rev().copied().collect()
            } {
                while result.len() >= start + 2 {
                    let a = result[result.len() - 2];
                    let b = result[result.len() - 1];
                    if (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) > 0. {
                        break;
                    }
                    result.pop();
                }
                result.push(p);
            }
            result.pop();
        }
        result
    }
    #[test]
    fn perspective_dice_faces_cover_their_silhouette() {
        for mesh in [cube(), d20()] {
            for seed in [1, 19, 42, 77, 99] {
                for ms in [100, 600, 1200, 1800, 2300, 2800] {
                    for body in bodies(
                        seed,
                        &[1, 6],
                        mesh,
                        ms as f32 / 1000.,
                        [390., 720.],
                        [195., 550.],
                        false,
                    ) {
                        let points = mesh
                            .iter()
                            .flat_map(|f| f.points.iter().map(|p| project(&body, *p, 390.)))
                            .collect();
                        let expected = polygon_area(&hull(points));
                        let actual = mesh
                            .iter()
                            .filter(|f| facing_camera(&body, f))
                            .map(|f| {
                                polygon_area(
                                    &f.points
                                        .iter()
                                        .map(|p| project(&body, *p, 390.))
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .sum::<f32>();
                        assert!(
                            (actual - expected).abs() < expected * 0.0002 + 0.01,
                            "face gap/overdraw: seed={seed} time={ms} actual={actual} silhouette={expected}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn dice_contact_and_settling_are_temporally_continuous() {
        for seed in [1, 19, 42, 77, 99, 123, 291, 4294967295] {
            for mesh in [cube(), d20()] {
                for count in [2, 10] {
                    let values: Vec<_> =
                        (0..count).map(|i| if i % 2 == 0 { 1 } else { 6 }).collect();
                    let mut previous =
                        bodies(seed, &values, mesh, 1.6, [390., 720.], [195., 550.], false);
                    for step in 193..=384 {
                        let current = bodies(
                            seed,
                            &values,
                            mesh,
                            step as f32 / 120.,
                            [390., 720.],
                            [195., 550.],
                            false,
                        );
                        for (i, (a, b)) in previous.iter().zip(&current).enumerate() {
                            let d = sub(a.center, b.center);
                            let cosine = a
                                .rotation
                                .0
                                .iter()
                                .zip(b.rotation.0)
                                .map(|(a, b)| a * b)
                                .sum::<f32>()
                                .abs()
                                .min(1.);
                            let angle = 2. * cosine.acos();
                            assert!(
                                dot(d, d).sqrt() < 0.22,
                                "position pop: seed={seed} step={step} die={i} displacement={}",
                                dot(d, d).sqrt()
                            );
                            assert!(
                                angle < 0.35,
                                "rotation pop: seed={seed} step={step} die={i} radians={angle}"
                            );
                        }
                        previous = current;
                    }
                }
            }
        }
    }
    #[test]
    fn dice_exchange_impulses_and_remain_separated_through_settling() {
        let particle = |x, velocity| Particle {
            p: [x, 2., 0.],
            v: [velocity, 0., 0.],
            initial: Q::axis([0., 0., 1.], 0.),
            roll: 0.,
            entered: true,
        };
        let mut pair = [particle(-0.7, 2.), particle(0.7, -2.)];
        contacts(&mut pair, 1., 0.62);
        assert!(
            pair[0].v[0] < 0. && pair[1].v[0] > 0.,
            "a head-on collision rebounds"
        );
        for mesh in [cube(), d20()] {
            let diameter = 2.
                * (mesh
                    .iter()
                    .flat_map(|f| &f.points)
                    .map(|p| dot(*p, *p).sqrt())
                    .fold(0_f32, f32::max)
                    + 0.015);
            for count in [2, 10] {
                for seed in [19, 42, 77] {
                    for step in 0..=28 {
                        let bodies = bodies(
                            seed,
                            &vec![1; count],
                            mesh,
                            step as f32 / 10.,
                            [390., 720.],
                            [195., 550.],
                            false,
                        );
                        for i in 0..count {
                            for b in &bodies[i + 1..] {
                                let d = sub(b.center, bodies[i].center);
                                assert!(
                                    dot(d, d).sqrt() >= diameter - 0.025,
                                    "rotating die bounds intersect: count={count} seed={seed} step={step}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
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
                assert!(
                    bottom.abs() < 1e-4,
                    "resting die meets its floor: value={value} bottom={bottom}"
                );
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
