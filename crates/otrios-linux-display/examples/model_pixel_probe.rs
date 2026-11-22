// SPDX-License-Identifier: Apache-2.0
//! Manual GLES2 shader/texture/depth acceptance on Mesa, including ARM emulation.
//! EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 cargo run -p otrios-linux-display
//! --features platform --example model_pixel_probe -- /tmp/relay-model
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use otrios_core::canvas::model::{Camera, Frame, IDENTITY, Instance, Model, multiply};
    use otrios_linux_display::Display;
    use std::{collections::BTreeSet, io::Write};
    let model = Model::from_glb(include_bytes!("../../../assets/models/relay-hat.glb"))?;
    let bounds = model.bounds();
    let size = (0..3)
        .map(|i| bounds[1][i] - bounds[0][i])
        .fold(0., f32::max);
    let mut normalize = IDENTITY;
    for i in 0..3 {
        normalize[i * 5] = 2. / size;
        normalize[12 + i] = -(bounds[0][i] + bounds[1][i]) / size;
    }
    let mut display = Display::headless([512, 512], false)?;
    display.upload(2, &[])?;
    let projection = [2. / 512., 0., 0., 0., -2. / 512., 0., -1., 1., 1.];
    let prefix = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/relay-model".into());
    for (label, pitch) in [("rest", 0.22_f32), ("tip", 0.64_f32)] {
        let (s, c) = pitch.sin_cos();
        let (a, b) = (-0.2_f32).sin_cos();
        let rotation = multiply(
            [1., 0., 0., 0., 0., c, s, 0., 0., -s, c, 0., 0., 0., 0., 1.],
            [b, 0., -a, 0., 0., 1., 0., 0., a, 0., b, 0., 0., 0., 0., 1.],
        );
        let frame = Frame::project(
            &[Instance {
                model: &model,
                pose: multiply(rotation, normalize),
                opacity: 1.,
            }],
            Camera {
                center: [256., 256.],
                pixels_per_unit: 180.,
                distance: 4.5,
                near: 1.,
                far: 8.,
            },
        )?;
        let mut ids = vec![];
        let mut draws = vec![];
        for batch in &frame.batches {
            let t = &batch.texture;
            let slot = ids.iter().position(|id| id == &t.id).unwrap_or_else(|| {
                ids.push(t.id);
                ids.len() - 1
            });
            display.model_texture(slot as u32, t.id, [t.width, t.height], t.repeat, &t.rgba)?;
            draws.extend([
                batch.first,
                batch.count,
                slot as u32,
                u32::from(batch.double_sided),
            ]);
        }
        display.upload_model(&frame.vertices, &draws, frame.depth)?;
        display.draw_slots(projection, &[2])?;
        let pixels = display.pixels()?;
        let background = &pixels[..4];
        let mut colors = BTreeSet::new();
        let mut painted = 0;
        for pixel in pixels.chunks_exact(4) {
            if pixel != background {
                painted += 1;
                colors.insert([pixel[0], pixel[1], pixel[2]]);
            }
        }
        if painted < 10_000 || colors.len() < 100 {
            return Err(format!(
                "Model pixels missing: {painted} pixels, {} colors",
                colors.len()
            )
            .into());
        }
        let path = format!("{prefix}-{label}.ppm");
        let mut file = std::fs::File::create(&path)?;
        write!(file, "P6\n512 512\n255\n")?;
        for row in pixels.chunks_exact(512 * 4).rev() {
            for p in row.chunks_exact(4) {
                file.write_all(&p[..3])?;
            }
        }
        println!(
            "{label}: {} triangles, {painted} foreground pixels, {} colors, linear={}, output={path}",
            frame.vertices.len() / 36,
            colors.len(),
            display.linear_scene_enabled()
        );
    }
    // Retiring a pose clears geometry while the immutable texture remains reusable.
    display.upload_model(&[], &[], [1., 0.])?;
    display.draw_slots(projection, &[2])?;
    let clear = display.pixels()?;
    assert!(clear.chunks_exact(4).all(|p| p == &clear[..4]));
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This manual probe requires Linux EGL/GLES2.");
}
