// SPDX-License-Identifier: LGPL-3.0-only
//! Chat choreography around a reusable, immutable static model resource.
use crate::canvas::model::{Camera, Frame, IDENTITY, Instance, Matrix, Model, multiply};
use std::sync::OnceLock;
pub(super) const HAT_DURATION: u64 = 2_100;
const HAT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/models/relay-hat.glb"
));
fn model() -> &'static Result<Model, String> {
    static MODEL: OnceLock<Result<Model, String>> = OnceLock::new();
    MODEL.get_or_init(|| Model::from_glb(HAT))
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn rotation(pitch: f32, yaw: f32) -> Matrix {
    let (s, c) = pitch.sin_cos();
    let (a, b) = yaw.sin_cos();
    multiply(
        [1., 0., 0., 0., 0., c, s, 0., 0., -s, c, 0., 0., 0., 0., 1.],
        [b, 0., -a, 0., 0., 1., 0., 0., a, 0., b, 0., 0., 0., 0., 1.],
    )
}
pub(super) fn hat_frame(
    elapsed_ms: u64,
    viewport: [f32; 2],
    quiet: bool,
) -> Result<Option<Frame>, String> {
    if elapsed_ms >= HAT_DURATION {
        return Ok(None);
    }
    if viewport
        .iter()
        .any(|n| !n.is_finite() || *n <= 0. || *n > 8192.)
    {
        return Err("Invalid model viewport".into());
    }
    let model = model().as_ref().map_err(Clone::clone)?;
    let t = elapsed_ms as f32 / 1000.;
    let tip =
        (smooth((t - 0.28) / 0.45) - smooth((t - 0.95) / 0.55)) * if quiet { 0.35 } else { 1. };
    let opacity = smooth(t / 0.12) * (1. - smooth((t - 1.75) / 0.35));
    let bounds = model.bounds();
    let extent = (0..3)
        .map(|i| bounds[1][i] - bounds[0][i])
        .fold(0., f32::max);
    let scale = 2. / extent;
    let mut normalize = IDENTITY;
    for i in 0..3 {
        normalize[i * 5] = scale;
        normalize[12 + i] = -(bounds[0][i] + bounds[1][i]) * 0.5 * scale;
    }
    let mut pose = multiply(rotation(0.12 + 0.52 * tip, -0.24), normalize);
    pose[13] += 0.18 * tip;
    let camera = Camera {
        center: [viewport[0] * 0.5, viewport[1] * 0.46],
        pixels_per_unit: (viewport[0] * 0.34).min(viewport[1] * 0.27),
        distance: 4.5,
        near: 1.,
        far: 8.,
    };
    Frame::project(
        &[Instance {
            model,
            pose,
            opacity,
        }],
        camera,
    )
    .map(Some)
}
