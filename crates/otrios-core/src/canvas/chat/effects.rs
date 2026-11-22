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
        for i in 0..16 {
            let point = |i: usize| {
                let a = i as f32 / 16. * TAU;
                [at[0] + a.cos() * r[0], at[1] + a.sin() * r[1]]
            };
            self.triangle(at, point(i), point(i + 1), color);
        }
    }
    fn ring(&mut self, at: [f32; 2], r: f32, width: f32, color: Color) {
        for i in 0..24 {
            let point = |i: usize| {
                let a = i as f32 / 24. * TAU;
                [at[0] + a.cos() * r, at[1] + a.sin() * r]
            };
            self.line(point(i), point(i + 1), width, color);
        }
    }
}

pub(super) fn duration(event: &MessageEvent) -> u64 {
    match event {
        MessageEvent::Steam { .. } => 2300,
        MessageEvent::Boop { .. } => 1400,
        MessageEvent::Salute { .. } => 1800,
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
fn steam(canvas: &mut Canvas, seed: u32, t: f32, at: [f32; 2], dark: bool, quiet: bool) {
    let mut random = Random(seed);
    let count = if quiet { 5 } else { 13 };
    for _ in 0..count {
        let delay = random.range(0., 0.65);
        let age = t - delay;
        let spread = random.range(-26., 26.);
        let drift = random.range(-18., 18.);
        let phase = random.range(0., TAU);
        if !(0. ..1.65).contains(&age) {
            continue;
        }
        let f = age / 1.65;
        let r = 8. + f * 24.;
        let p = [
            at[0] + spread + drift * f + (f * 7. + phase).sin() * 7.,
            at[1] - age * if quiet { 24. } else { 48. },
        ];
        let alpha = (smooth(f / 0.18) * (1. - f).powi(2)) * 0.43;
        let shade = if dark { 0.83 } else { 0.35 };
        for (factor, opacity) in [(1.35, 0.18), (1., 0.35), (0.72, 0.47)] {
            canvas.ellipse(
                p,
                [r * factor, r * factor * 0.82],
                [shade, shade + 0.015, shade + 0.025, alpha * opacity],
            );
        }
    }
}
fn boop(canvas: &mut Canvas, seed: u32, t: f32, at: [f32; 2], dark: bool, quiet: bool) {
    let fade = (1. - (t - 0.85).max(0.) / 0.55).clamp(0., 1.);
    let travel = (t / 0.45).clamp(0., 1.);
    let side = if seed & 1 == 0 { -1. } else { 1. };
    let height = if quiet { 10. } else { 36. };
    let p = [
        at[0] + side * 48. * (1. - smooth(travel)),
        at[1] - (travel * PI).sin() * height,
    ];
    let spring = if t > 0.4 {
        ((t - 0.4) * 27.).sin() * (-(t - 0.4) * 7.).exp() * 0.23
    } else {
        0.
    };
    let alpha = fade * 0.95;
    canvas.ellipse(
        p,
        [8. * (1. + spring), 8. * (1. - spring)],
        [0.85, 0.66, 0.34, alpha],
    );
    canvas.ellipse(
        [p[0] - 2., p[1] - 2.],
        [2., 2.],
        [1., 0.92, 0.66, alpha * 0.8],
    );
    for i in 0..if quiet { 1 } else { 3 } {
        let a = (t - 0.38 - i as f32 * 0.1).max(0.);
        if a > 0. && a < 0.65 {
            let f = a / 0.65;
            canvas.ring(
                at,
                10. + f * 38.,
                2. * (1. - f) + 0.5,
                [0.72, 0.48, 0.3, (1. - f) * fade * 0.8],
            );
        }
    }
    if dark {
        canvas.ring(at, 11. + spring * 8., 0.8, [0.9, 0.78, 0.55, alpha * 0.25]);
    }
}
fn salute(canvas: &mut Canvas, seed: u32, t: f32, at: [f32; 2], dark: bool, quiet: bool) {
    let fade = (1. - (t - 1.15).max(0.) / 0.65).clamp(0., 1.);
    let a = (t / 1.4 * PI).sin().max(0.);
    let angle = if quiet { -0.12 * a } else { -0.5 * a };
    let lift = a * if quiet { 8. } else { 23. };
    let point = |x: f32, y: f32| {
        [
            at[0] + x * angle.cos() - y * angle.sin(),
            at[1] - lift + x * angle.sin() + y * angle.cos(),
        ]
    };
    let fill = if dark {
        [0.82, 0.65, 0.36, fade]
    } else {
        [0.34, 0.28, 0.18, fade]
    };
    let corners = [
        point(-14., -4.),
        point(-11., -26.),
        point(11., -26.),
        point(14., -4.),
    ];
    canvas.triangle(corners[0], corners[1], corners[2], fill);
    canvas.triangle(corners[0], corners[2], corners[3], fill);
    canvas.line(point(-21., -3.), point(21., -3.), 5., fill);
    canvas.line(
        point(-12., -9.),
        point(12., -9.),
        4.,
        [0.72, 0.43, 0.26, fade],
    );
    for i in 0..if quiet { 2 } else { 5 } {
        let angle = i as f32 / 5. * TAU + seed as f32 * 0.001;
        let distance = 25. + a * 13.;
        let p = [
            at[0] + angle.cos() * distance,
            at[1] - lift - 15. + angle.sin() * distance,
        ];
        canvas.line(
            [p[0] - 2., p[1]],
            [p[0] + 2., p[1]],
            1.5,
            [0.85, 0.67, 0.36, fade * a],
        );
        canvas.line(
            [p[0], p[1] - 2.],
            [p[0], p[1] + 2.],
            1.5,
            [0.85, 0.67, 0.36, fade * a],
        );
    }
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
    let mut p = [
        random.range(-bound, bound),
        random.range(2.4, 3.9),
        random.range(-0.7, 0.7),
    ];
    let mut v = [
        random.range(2.8, 5.4) * if index & 1 == 0 { 1. } else { -1. },
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
        if p[0].abs() > bound {
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
    let axis = norm([
        random.range(-1., 1.),
        random.range(-1., 1.),
        random.range(-1., 1.),
    ]);
    let spin = Q::axis(axis, time * if quiet { 4. } else { 11. }).product(initial);
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
