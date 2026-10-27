// SPDX-License-Identifier: Apache-2.0
//! Same-pipeline native reference/output/difference comparison. Not an FPS test.
mod common;
use otrios_core::dashboard::{display::Surface, font::Font, font_reference};
use otrios_linux_display::Display;
use std::path::Path;
fn edge(pixels: &[u8], index: usize, width: usize, height: usize) -> bool {
    let (x, y) = (index % width, index / width);
    let color = &pixels[index * 4..index * 4 + 3];
    [
        (x > 0).then(|| index - 1),
        (x + 1 < width).then_some(index + 1),
        (y > 0).then(|| index - width),
        (y + 1 < height).then_some(index + width),
    ]
    .into_iter()
    .flatten()
    .any(|n| &pixels[n * 4..n * 4 + 3] != color)
}
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
                display.text_styles(0, &frame.mesh.styles)?;
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
            let mut edge_pixels = 0_u64;
            let mut edge_sum = 0_u64;
            let mut diff = vec![0; a.len()];
            for (index, ((pa, pb), out)) in a
                .chunks_exact(4)
                .zip(b.chunks_exact(4))
                .zip(diff.chunks_exact_mut(4))
                .enumerate()
            {
                let mut pixel_max = 0;
                let foreground = pa[..3] != *bg || pb[..3] != *bg;
                let on_edge = foreground
                    && (edge(a, index, w as usize, h as usize)
                        || edge(b, index, w as usize, h as usize));
                if foreground {
                    active += 1;
                }
                edge_pixels += u64::from(on_edge);
                for j in 0..3 {
                    let e = pa[j].abs_diff(pb[j]);
                    pixel_max = pixel_max.max(e);
                    sum += u64::from(e);
                    if on_edge {
                        edge_sum += u64::from(e);
                    }
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
                "{{\"rotation\":{rotation},\"frame\":{i},\"pixels\":{},\"active_pixels\":{active},\"changed_pixels\":{changed},\"above_one_pixels\":{above_one},\"max_byte_error\":{maximum},\"edge_pixels\":{edge_pixels},\"mean_edge_byte_error\":{},\"mean_active_byte_error\":{}}}",
                w * h,
                if edge_pixels == 0 {
                    0.
                } else {
                    edge_sum as f64 / (edge_pixels * 3) as f64
                },
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
        Err("Candidate/reference exceeds 8-byte comparison gate".into())
    }
}
