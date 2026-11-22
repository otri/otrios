// SPDX-License-Identifier: LGPL-3.0-only
//! A small retained locomotive mesh; steam is emitted from its moving chimney.
use super::*;

fn cuboid(faces: &mut Vec<SolidFace>, min: V3, max: V3, color: [f32; 3]) {
    for (axis, a, b) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
        for upper in [false, true] {
            let mut points = vec![];
            for [u, v] in [[false, false], [true, false], [true, true], [false, true]] {
                let mut p = min;
                p[axis] = if upper { max[axis] } else { min[axis] };
                p[a] = if u { max[a] } else { min[a] };
                p[b] = if v { max[b] } else { min[b] };
                points.push(p);
            }
            let mut normal = [0.; 3];
            normal[axis] = if upper { 1. } else { -1. };
            faces.push(SolidFace::new(points, color, normal));
        }
    }
}
fn cylinder(
    faces: &mut Vec<SolidFace>,
    center: V3,
    axis: usize,
    length: f32,
    radius: f32,
    color: [f32; 3],
) {
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let point = |side: f32, angle: f32| {
        let mut p = center;
        p[axis] += side * length * 0.5;
        p[u] += angle.cos() * radius;
        p[v] += angle.sin() * radius;
        p
    };
    for i in 0..24 {
        let a = i as f32 / 24. * TAU;
        let b = (i + 1) as f32 / 24. * TAU;
        let mut outward = [0.; 3];
        outward[u] = ((a + b) * 0.5).cos();
        outward[v] = ((a + b) * 0.5).sin();
        faces.push(SolidFace::new(
            vec![point(-1., a), point(1., a), point(1., b), point(-1., b)],
            color,
            outward,
        ));
        for side in [-1., 1.] {
            let mut normal = [0.; 3];
            normal[axis] = side;
            let mut c = center;
            c[axis] += side * length * 0.5;
            faces.push(SolidFace::new(
                vec![c, point(side, a), point(side, b)],
                color,
                normal,
            ));
        }
    }
}
fn mesh() -> &'static Vec<SolidFace> {
    static ENGINE: OnceLock<Vec<SolidFace>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut f = vec![];
        let iron = [0.13, 0.17, 0.16];
        let brass = [0.63, 0.43, 0.22];
        let black = [0.075, 0.09, 0.10];
        cuboid(&mut f, [-2.15, -0.04, -0.56], [2.18, 0.22, 0.56], black);
        cylinder(&mut f, [0.35, 0.78, 0.], 0, 2.95, 0.58, iron);
        // Boiler bands, smoke-box door and copper latch.
        for x in [-0.75, 0.28, 1.55] {
            cylinder(&mut f, [x, 0.78, 0.], 0, 0.045, 0.595, brass);
        }
        cylinder(&mut f, [1.85, 0.78, 0.], 0, 0.16, 0.60, black);
        cylinder(&mut f, [1.95, 0.78, 0.], 0, 0.035, 0.47, iron);
        cuboid(&mut f, [1.975, 0.72, -0.19], [2.005, 0.76, 0.19], brass);
        // Cab, bright inset windows and slightly overhanging roof.
        cuboid(&mut f, [-1.98, 0.22, -0.59], [-1.10, 1.53, 0.59], iron);
        cuboid(
            &mut f,
            [-1.86, 0.86, 0.595],
            [-1.24, 1.38, 0.615],
            [0.33, 0.43, 0.43],
        );
        cuboid(
            &mut f,
            [-1.80, 0.91, 0.618],
            [-1.30, 1.32, 0.625],
            [0.10, 0.14, 0.17],
        );
        cuboid(&mut f, [-2.10, 1.51, -0.72], [-1.01, 1.65, 0.72], black);
        // Chimney, flared cap and brass steam dome.
        cylinder(&mut f, [1.15, 1.51, 0.], 1, 0.52, 0.17, black);
        cylinder(&mut f, [1.15, 1.79, 0.], 1, 0.09, 0.24, brass);
        cylinder(&mut f, [-0.20, 1.37, 0.], 1, 0.34, 0.23, brass);
        cuboid(
            &mut f,
            [1.70, -0.12, -0.76],
            [2.22, 0.03, 0.76],
            [0.32, 0.19, 0.13],
        );
        for z in [-0.57, 0.57] {
            cylinder(&mut f, [2.25, 0.07, z], 0, 0.20, 0.13, black);
        }
        f
    })
}
pub(super) fn stage(t: f32, viewport: [f32; 2], quiet: bool) -> Stage {
    let [w, h] = viewport;
    let scale = (w / 5.4).min(h * 0.25);
    Stage {
        center: [mix(-w * 0.62, w * 1.65, smooth(t / 4.8)), h * 0.74],
        scale,
        rotation: Q::axis([1., 0., 0.], 0.13)
            .product(Q::axis([0., 1., 0.], if quiet { -0.20 } else { -0.32 })),
    }
}
pub(super) fn draw(canvas: &mut Canvas, t: f32, quiet: bool) {
    let stage = stage(t, canvas.viewport, quiet);
    let alpha = smooth(t / 0.25) * (1. - smooth((t - 4.5) / 0.4));
    solid(canvas, mesh(), &stage, alpha, [0.; 4]);
    // Front wheels and connecting rod rotate with travelled distance.
    let travel = stage.center[0] / stage.scale;
    let angle = -travel / 0.40;
    let mut cranks = vec![];
    for x in [-1.10, -0.10, 0.90] {
        let center = [x, -0.07, 0.65];
        for (r, width, color) in [
            (0.41, 0.12, [0.05, 0.065, 0.075, alpha]),
            (0.33, 0.045, [0.63, 0.43, 0.23, alpha]),
        ] {
            for j in 0..32 {
                let p = |a: f32| stage.project(add(center, [a.cos() * r, a.sin() * r, 0.]));
                canvas.line(
                    p(j as f32 / 32. * TAU),
                    p((j + 1) as f32 / 32. * TAU),
                    stage.scale * width,
                    color,
                );
            }
        }
        for j in 0..8 {
            let a = angle + j as f32 / 8. * TAU;
            canvas.line(
                stage.project(center),
                stage.project(add(center, [a.cos() * 0.32, a.sin() * 0.32, 0.])),
                stage.scale * 0.035,
                [0.57, 0.44, 0.28, alpha],
            );
        }
        cranks.push(stage.project(add(center, [angle.cos() * 0.18, angle.sin() * 0.18, 0.015])));
    }
    canvas.line(
        cranks[0],
        cranks[2],
        stage.scale * 0.065,
        [0.68, 0.63, 0.50, alpha],
    );
}
