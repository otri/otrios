// SPDX-License-Identifier: Apache-2.0
//! Same-pipeline native reference/output/difference comparison. Not an FPS test.
mod common;
use otrios_core::dashboard::{display::Surface, font::Font, font_reference};
use otrios_linux_display::Display;
use std::path::Path;
fn ppm(path: &Path, pixels: &[u8], w: usize, h: usize) -> Result<(), String> {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for row in pixels.chunks_exact(w * 4).rev() {
        for p in row.chunks_exact(4) {
            out.extend_from_slice(&p[..3]);
        }
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("Supply reference.mesh candidate.mesh OUTPUT_DIRECTORY".into());
    }
    let reference = common::load(&args[0])?;
    let candidate = common::load(&args[1])?;
    if reference.len() != candidate.len() {
        return Err("Frame count mismatch".into());
    }
    let output = Path::new(&args[2]);
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let font = Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?;
    let atlas = font.atlas();
    let mut passed = true;
    for rotation in [0, 90] {
        let surface = Surface::new(rotation)?;
        let [w, h] = surface.extent();
        let mut display = Display::headless([w, h], false)?;
        display.set_font(
            atlas.curve_width,
            atlas.curve_height,
            &atlas.curves,
            atlas.band_width,
            atlas.band_height,
            &atlas.bands,
        )?;
        let mut worst = 0;
        for i in [0, 1, 17, 59, reference.len() - 1] {
            let mut pixels = vec![];
            for frame in [&reference[i], &candidate[i]] {
                display.upload_scene(
                    0,
                    &frame.mesh.vertices,
                    &frame.mesh.glyphs,
                    &frame.mesh.draws,
                )?;
                display.text_views(0, &frame.views)?;
                display.draw(surface.projection(), false)?;
                pixels.push(display.pixels()?);
            }
            let [a, b] = pixels.as_slice() else {
                return Err("Missing comparison".into());
            };
            let bg = &a[..3];
            let mut active = 0_u64;
            let mut changed = 0_u64;
            let mut above_one = 0_u64;
            let mut maximum = 0;
            let mut sum = 0_u64;
            let mut diff = vec![0; a.len()];
            for ((a, b), out) in a
                .chunks_exact(4)
                .zip(b.chunks_exact(4))
                .zip(diff.chunks_exact_mut(4))
            {
                let mut pixel_max = 0;
                if a[..3] != *bg || b[..3] != *bg {
                    active += 1;
                }
                for j in 0..3 {
                    let e = a[j].abs_diff(b[j]);
                    pixel_max = pixel_max.max(e);
                    sum += u64::from(e);
                    out[j] = e.saturating_mul(8);
                }
                out[3] = 255;
                maximum = maximum.max(pixel_max);
                changed += u64::from(pixel_max != 0);
                above_one += u64::from(pixel_max > 1);
            }
            let name = format!("rotation-{rotation}-frame-{i}");
            passed &= maximum <= 8;
            if i == 17 {
                ppm(
                    &output.join(format!("{name}-reference.ppm")),
                    a,
                    w as usize,
                    h as usize,
                )?;
                ppm(
                    &output.join(format!("{name}-candidate.ppm")),
                    b,
                    w as usize,
                    h as usize,
                )?;
                ppm(
                    &output.join(format!("{name}-difference-8x.ppm")),
                    &diff,
                    w as usize,
                    h as usize,
                )?;
            }
            if maximum >= worst {
                worst = maximum;
                for (kind, bytes) in [("reference", a), ("candidate", b), ("difference-8x", &diff)]
                {
                    ppm(
                        &output.join(format!("rotation-{rotation}-worst-{kind}.ppm")),
                        bytes,
                        w as usize,
                        h as usize,
                    )?;
                }
                std::fs::write(
                    output.join(format!("rotation-{rotation}-worst-frame.txt")),
                    i.to_string(),
                )
                .map_err(|e| e.to_string())?;
            }
            println!(
                "{{\"rotation\":{rotation},\"frame\":{i},\"pixels\":{},\"active_pixels\":{active},\"changed_pixels\":{changed},\"above_one_pixels\":{above_one},\"max_byte_error\":{maximum},\"mean_active_byte_error\":{}}}",
                w * h,
                if active == 0 {
                    0.
                } else {
                    sum as f64 / (active * 3) as f64
                }
            );
        }
    }
    if passed {
        Ok(())
    } else {
        Err("Fixed/reference exceeds 8-byte comparison gate".into())
    }
}
