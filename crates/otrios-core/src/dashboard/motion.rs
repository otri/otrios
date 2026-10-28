// SPDX-License-Identifier: LGPL-3.0-only
//! Declarative intent crosses the publication boundary. Phase, interpolation and
//! frame statistics belong exclusively to the presentation owner.
use super::{Mesh, Result, components::*, render_scene, scene::Scene};
use otrios_interfaces::scene::*;
use std::collections::VecDeque;

// Regular tetrahedron centered on the origin, with the same sqrt(3)
// circumradius as the former cube. The equilateral near face starts face-on.
const TETRAHEDRON_VERTICES: [[f32; 3]; 4] = [
    [0., -1.632_993_2, -0.577_350_26],
    [-std::f32::consts::SQRT_2, 0.816_496_6, -0.577_350_26],
    [std::f32::consts::SQRT_2, 0.816_496_6, -0.577_350_26],
    [0., 0., 1.732_050_8],
];
// Inward winding: visible faces have positive signed area after projection
// into the dashboard's downward-positive screen coordinates.
const TETRAHEDRON_FACES: [[usize; 3]; 4] = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    pub playing: bool,
    pub speed: f32,
    pub count: u16,
}
impl Default for Intent {
    fn default() -> Self {
        Self {
            playing: true,
            speed: 1.,
            count: 12,
        }
    }
}
impl Intent {
    pub fn validate(self) -> Result<()> {
        if !self.speed.is_finite()
            || !(0.25..=4.).contains(&self.speed)
            || !(1..=64).contains(&self.count)
        {
            return Err("Invalid motion intent".into());
        }
        Ok(())
    }
}
/// Presentation clock never resets when a new immutable app view arrives.
#[derive(Default)]
pub struct Presenter {
    last: Option<f64>,
    phase: f64,
    speed: f64,
    intervals: VecDeque<f64>,
    frames: u64,
    late: u64,
    hud: Option<(u64, Mesh)>,
}
impl Presenter {
    pub fn phase(&self) -> f64 {
        self.phase
    }
    pub fn advance(&mut self, now_ms: f64, intent: Intent) {
        if !now_ms.is_finite() || intent.validate().is_err() {
            return;
        }
        if let Some(last) = self.last {
            let dt = now_ms - last;
            if dt <= 0. {
                return;
            }
            // Count foreground hitches even when phase advancement is capped.
            if self.intervals.len() == 3600 {
                self.intervals.pop_front();
            }
            self.intervals.push_back(dt);
            self.frames += 1;
            if dt > 25. {
                self.late += 1;
            }
            if dt < 1000. {
                let step = dt.min(100.) / 1000.;
                let target = if intent.playing {
                    f64::from(intent.speed)
                } else {
                    0.
                };
                self.speed += (target - self.speed) * (1. - (-step * 12.).exp());
                self.phase = (self.phase + step * self.speed).rem_euclid(3600.);
            }
        } else {
            self.speed = if intent.playing {
                f64::from(intent.speed)
            } else {
                0.
            };
        }
        self.last = Some(now_ms);
    }
    pub fn suspend(&mut self) {
        self.last = None;
    }
    pub fn stats(&self) -> String {
        let mut v: Vec<_> = self.intervals.iter().copied().collect();
        v.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            v.get(((v.len().saturating_sub(1)) as f64 * p).round() as usize)
                .copied()
                .unwrap_or(0.)
        };
        let mean = if v.is_empty() {
            0.
        } else {
            v.iter().sum::<f64>() / v.len() as f64
        };
        format!(
            "{{\"frames\":{},\"samples\":{},\"fps\":{:.2},\"p50_ms\":{:.3},\"p95_ms\":{:.3},\"p99_ms\":{:.3},\"over_25ms\":{}}}",
            self.frames,
            v.len(),
            if mean > 0. { 1000. / mean } else { 0. },
            percentile(0.5),
            percentile(0.95),
            percentile(0.99),
            self.late
        )
    }
    /// Generate animation geometry without rebuilding telemetry text. A host may
    /// rasterize telemetry on a separate bounded presentation worker.
    pub fn geometry(&mut self, three_d: bool, intent: Intent, now: f64) -> Mesh {
        self.geometry_in(three_d, intent, now, [80., 280., 1120., 400.])
    }
    /// The admitted app owns its stage; both hosts render into these logical bounds.
    pub fn geometry_in(
        &mut self,
        three_d: bool,
        intent: Intent,
        now: f64,
        stage: [f32; 4],
    ) -> Mesh {
        if stage.iter().any(|v| !v.is_finite()) || stage[2] <= 0. || stage[3] <= 0. {
            return Mesh::default();
        }
        self.advance(now, intent);
        let [left, top, width, height] = stage;
        let mut children = Vec::new();
        let t = self.phase as f32;
        if three_d {
            // Bounded, nonintersecting tetrahedron cells. CPU projection and
            // painter sorting feed the shared GLES2/WebGL triangle rasterizers.
            for i in 0..intent.count {
                let columns = ((f32::from(intent.count) * (width / height).min(1.))
                    .sqrt()
                    .ceil() as u16)
                    .clamp(1, intent.count);
                let rows = intent.count.div_ceil(columns);
                let cw = width / f32::from(columns);
                let ch = height / f32::from(rows);
                let size = cw.min(ch) * 0.25;
                let center = [
                    left + cw * (f32::from(i % columns) + 0.5),
                    top + ch * (f32::from(i / columns) + 0.5),
                ];
                let f = f32::from(i);
                let a = t * (0.8 + f * 0.004) + f * 0.31;
                let b = t * (0.6 + f * 0.003) + f * 0.23;
                let (sa, ca) = a.sin_cos();
                let (sb, cb) = b.sin_cos();
                let vertices = TETRAHEDRON_VERTICES.map(|p| {
                    let x = p[0] * ca + p[2] * sa;
                    let z = -p[0] * sa + p[2] * ca;
                    [x, p[1] * cb - z * sb, p[1] * sb + z * cb]
                });
                let mut faces: Vec<_> = TETRAHEDRON_FACES
                    .into_iter()
                    .enumerate()
                    .map(|(face, idx)| {
                        let z = idx.iter().map(|j| vertices[*j][2]).sum::<f32>() / 3.;
                        (z, face, idx)
                    })
                    .collect();
                faces.sort_by(|a, b| b.0.total_cmp(&a.0));
                for (_, face, idx) in faces {
                    let project = |j: usize| {
                        let p = vertices[j];
                        let perspective = 4. / (4. + p[2]);
                        [
                            center[0] + p[0] * size * perspective,
                            center[1] + p[1] * size * perspective,
                        ]
                    };
                    // These closed, opaque convex cells never intersect. Their
                    // inward winding projects counterclockwise on hidden faces;
                    // skip those faces rather than rasterizing and covering them.
                    let [a, b, c] = [project(idx[0]), project(idx[1]), project(idx[2])];
                    if (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) <= 0. {
                        continue;
                    }
                    let points = idx.into_iter().map(project).collect();
                    let palette = [
                        otrios_interfaces::theme::BRASS,
                        otrios_interfaces::theme::COPPER,
                        [235, 204, 148, 255],
                        [143, 115, 79, 255],
                    ];
                    children.push(node(
                        10 + u32::from(i) * 4 + face as u32,
                        Rect::new(0., 0., 1280., 800.),
                        Primitive::Triangles {
                            points,
                            color: palette[face],
                        },
                    ));
                }
            }
        } else {
            for i in 0..intent.count {
                let f = f32::from(i);
                let a = t * (0.5 + f * 0.011) + f * 2.399963;
                let x = left + width * 0.5 + a.cos() * width * (0.2 + (f * 1.7).sin() * 0.17);
                let y = top + height * 0.5 + (a * 1.37).sin() * (height * 0.5 - 18.).max(0.);
                children.push(node(
                    10 + u32::from(i),
                    Rect::new(x - 12., y - 12., 24., 24.),
                    Primitive::Circle {
                        color: if i % 2 == 0 {
                            ACCENT
                        } else {
                            otrios_interfaces::theme::COPPER
                        },
                    },
                ));
            }
        }
        let scene = Scene::new(canvas(
            0,
            Rect::new(0., 0., (left + width).max(1280.), (top + height).max(800.)),
            children,
        ))
        .expect("bounded motion geometry");
        render_scene(&scene)
    }
    pub fn mesh(&mut self, three_d: bool, intent: Intent, now: f64, updates: u64) -> Mesh {
        let mesh = self.geometry(three_d, intent, now);
        let bucket = (now.max(0.) / 1000.) as u64;
        if self.hud.as_ref().is_none_or(|(old, _)| *old != bucket) {
            self.hud = Some((bucket, render_hud(&self.hud_label(updates))));
        }
        mesh
    }
    pub fn hud_label(&self, updates: u64) -> String {
        let mean = if self.intervals.is_empty() {
            0.
        } else {
            self.intervals.iter().sum::<f64>() / self.intervals.len() as f64
        };
        format!(
            "{:05.1} FPS / {} FRAMES >25ms / {} CRDT LOAD UPDATES",
            if mean > 0. { 1000. / mean } else { 0. },
            self.late,
            updates
        )
    }
    pub fn hud_revision(&self) -> u64 {
        self.hud.as_ref().map(|h| h.0).unwrap_or(u64::MAX)
    }
    pub fn hud(&self) -> &[f32] {
        self.hud
            .as_ref()
            .map(|h| h.1.vertices.as_slice())
            .unwrap_or_default()
    }
}
/// Telemetry remains a Text primitive. Hosts may render this immutable label
/// independently from the animation clock and the CRDT/state worker.
pub fn render_hud(label: &str) -> Mesh {
    render_hud_font(label, None)
}
pub fn render_hud_with_font(label: &str, font: &super::font::Font) -> Mesh {
    render_hud_font(label, Some(font))
}
fn render_hud_font(label: &str, font: Option<&super::font::Font>) -> Mesh {
    let scene = Scene::new(canvas(
        0,
        Rect::new(0., 0., 1280., 800.),
        vec![text(1, Rect::new(48., 748., 1184., 24.), label, 2, WHITE)],
    ))
    .unwrap();
    match font {
        Some(font) => super::render_scene_with_font(&scene, font),
        None => render_scene(&scene),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tetrahedron_is_regular_closed_and_inward_wound() {
        let mut edges = std::collections::BTreeMap::new();
        for (i, a) in TETRAHEDRON_VERTICES.iter().enumerate() {
            for b in &TETRAHEDRON_VERTICES[i + 1..] {
                let length_squared: f32 = a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum();
                assert!((length_squared - 8.).abs() < 0.00001);
            }
        }
        for [a, b, c] in TETRAHEDRON_FACES {
            let [a, b, c] = [a, b, c].map(|i| TETRAHEDRON_VERTICES[i]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let normal = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            assert!(normal.iter().zip(a).map(|(n, p)| n * p).sum::<f32>() < 0.);
        }
        for indices in TETRAHEDRON_FACES {
            for i in 0..3 {
                *edges.entry((indices[i], indices[(i + 1) % 3])).or_insert(0) += 1;
            }
        }
        assert_eq!(edges.len(), 12);
        for ((a, b), count) in &edges {
            assert_eq!(*count, 1);
            assert_eq!(edges.get(&(*b, *a)), Some(&1));
        }
    }
    #[test]
    fn face_on_tetrahedron_has_the_expected_triangular_silhouette() {
        let mesh = Presenter::default().geometry(
            true,
            Intent {
                count: 1,
                ..Intent::default()
            },
            0.,
        );
        // The far apex projects inside this single near equilateral face.
        assert_eq!(mesh.vertices.len(), 3 * 6);
        let perspective = 4. / (4. - 1. / 3f32.sqrt());
        let expected = [
            [640., 480. - 100. * (8f32 / 3.).sqrt() * perspective],
            [
                640. + 100. * 2f32.sqrt() * perspective,
                480. + 100. * (2f32 / 3.).sqrt() * perspective,
            ],
            [
                640. - 100. * 2f32.sqrt() * perspective,
                480. + 100. * (2f32 / 3.).sqrt() * perspective,
            ],
        ];
        for (v, xy) in mesh.vertices.chunks_exact(6).zip(expected) {
            assert!((v[0] - xy[0]).abs() < 0.001);
            assert!((v[1] - xy[1]).abs() < 0.001);
            assert_eq!(&v[2..], &[216. / 255., 183. / 255., 117. / 255., 1.]);
        }
    }
    #[test]
    fn rotating_tetrahedra_cull_hidden_faces_without_silhouette_holes() {
        fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        }
        fn inside(p: [f32; 2], points: &[[f32; 2]]) -> bool {
            let sides: Vec<_> = (0..points.len())
                .map(|i| cross(points[i], points[(i + 1) % points.len()], p))
                .collect();
            sides.iter().all(|s| *s >= -0.001) || sides.iter().all(|s| *s <= 0.001)
        }
        for step in 0..120 {
            let phase = step as f32 * 0.1;
            let mesh = Presenter {
                phase: f64::from(phase),
                ..Presenter::default()
            }
            .geometry(
                true,
                Intent {
                    count: 1,
                    ..Intent::default()
                },
                0.,
            );
            let triangles: Vec<Vec<[f32; 2]>> = mesh
                .vertices
                .chunks_exact(18)
                .map(|t| t.chunks_exact(6).map(|v| [v[0], v[1]]).collect())
                .collect();
            assert!((1..=3).contains(&triangles.len()));
            assert!(triangles.iter().all(|t| cross(t[0], t[1], t[2]) > 0.));
            // Build an independent 2D convex hull of all four projected points.
            // Its coverage must equal the union of the retained front faces.
            let (sa, ca) = (phase * 0.8).sin_cos();
            let (sb, cb) = (phase * 0.6).sin_cos();
            let mut points = TETRAHEDRON_VERTICES.map(|[x, y, z]| {
                let xx = x * ca + z * sa;
                let zz = -x * sa + z * ca;
                let yy = y * cb - zz * sb;
                let depth = y * sb + zz * cb;
                let perspective = 400. / (4. + depth);
                [640. + xx * perspective, 480. + yy * perspective]
            });
            points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            // Separate monotone chains keep the lower hull intact.
            let chain = |points: Vec<[f32; 2]>| {
                let mut h = Vec::new();
                for point in points {
                    while h.len() >= 2 && cross(h[h.len() - 2], h[h.len() - 1], point) <= 0. {
                        h.pop();
                    }
                    h.push(point);
                }
                h.pop();
                h
            };
            let mut hull = chain(points.to_vec());
            hull.extend(chain(points.into_iter().rev().collect()));
            assert!(
                hull.iter()
                    .all(|p| p[0] > 80. && p[0] < 1200. && p[1] > 280. && p[1] < 680.)
            );
            for x in (480..800).step_by(11) {
                for y in (300..660).step_by(11) {
                    let p = [x as f32 + 0.37, y as f32 + 0.19];
                    assert_eq!(
                        inside(p, &hull),
                        triangles.iter().any(|t| inside(p, t)),
                        "silhouette mismatch at phase {phase}, point {p:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn independently_rotating_tetrahedra_stay_inside_their_64_cells() {
        let intent = Intent {
            count: 64,
            ..Intent::default()
        };
        for step in 0..24 {
            let mesh = Presenter {
                phase: f64::from(step) * 0.5,
                ..Presenter::default()
            }
            .geometry(true, intent, 0.);
            let mut occupied = std::collections::BTreeSet::new();
            for triangle in mesh.vertices.chunks_exact(18) {
                let x = triangle.chunks_exact(6).map(|v| v[0]).sum::<f32>() / 3.;
                let y = triangle.chunks_exact(6).map(|v| v[1]).sum::<f32>() / 3.;
                let column = ((x - 80.) / 140.).floor() as u16;
                let row = ((y - 280.) / 50.).floor() as u16;
                assert!(column < 8 && row < 8);
                occupied.insert((column, row));
                for v in triangle.chunks_exact(6) {
                    assert!(
                        v[0] > 80. + f32::from(column) * 140.
                            && v[0] < 80. + f32::from(column + 1) * 140.
                    );
                    assert!(
                        v[1] > 280. + f32::from(row) * 50.
                            && v[1] < 280. + f32::from(row + 1) * 50.
                    );
                    assert_eq!(v[5], 1.);
                }
            }
            assert_eq!(occupied.len(), 64);
        }
    }
    #[test]
    fn new_intent_keeps_the_local_presentation_phase() {
        let mut presenter = Presenter::default();
        for i in 0..600 {
            presenter.mesh(false, Intent::default(), f64::from(i) * 16.6667, 0);
        }
        assert!(presenter.phase() > 9.9);
        let before = presenter.phase();
        presenter.advance(
            10000.,
            Intent {
                count: 64,
                ..Intent::default()
            },
        );
        assert!(presenter.phase() > before && presenter.phase() - before < 0.1);
        assert!(presenter.stats().contains("\"fps\":60.00"));
    }
    #[test]
    fn pause_resume_and_clock_discontinuity_are_bounded() {
        let mut p = Presenter::default();
        p.advance(0., Intent::default());
        p.advance(16., Intent::default());
        let before = p.phase();
        p.advance(10000., Intent::default());
        assert_eq!(p.phase(), before);
        p.advance(9999., Intent::default());
        assert_eq!(p.phase(), before);
        for i in 1..200 {
            p.advance(
                10000. + f64::from(i) * 16.,
                Intent {
                    playing: false,
                    ..Intent::default()
                },
            );
        }
        let paused = p.phase();
        p.advance(
            13200.,
            Intent {
                playing: false,
                ..Intent::default()
            },
        );
        assert!((p.phase() - paused).abs() < 0.001);
    }
    #[test]
    fn long_foreground_hitches_are_counted_but_explicit_background_suspension_is_not() {
        let mut p = Presenter::default();
        p.advance(0., Intent::default());
        p.advance(2000., Intent::default());
        assert!(p.stats().contains("\"p99_ms\":2000.000"));
        let before = p.phase();
        p.suspend();
        p.advance(20000., Intent::default());
        assert_eq!(p.phase(), before);
        assert!(p.stats().contains("\"frames\":1"));
    }
    #[test]
    fn intent_validation_and_both_mesh_limits() {
        assert!(
            Intent {
                speed: f32::NAN,
                ..Intent::default()
            }
            .validate()
            .is_err()
        );
        let mut p = Presenter::default();
        for mode in [false, true] {
            let m = p.mesh(
                mode,
                Intent {
                    count: 64,
                    ..Intent::default()
                },
                16.,
                0,
            );
            assert!(!m.vertices.is_empty());
            assert!(m.vertices.iter().all(|v| v.is_finite()));
            assert!(m.vertices.len() < 100_000);
        }
    }
}
