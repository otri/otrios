// SPDX-License-Identifier: Apache-2.0
//! Exclusive-display replay of synthetic core-produced meshes. Run via a restoring controller.
use otrios_core::dashboard::{display::Surface, font::Font, font_reference};
use otrios_linux_display::Display;
use std::{sync::Arc, time::Instant};
mod common;
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("Supply mesh stream")?;
    let mode = args.next().unwrap_or("scroll".into());
    let capture = args.next();
    let meshes = common::load(&input)?;
    let font = Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?;
    let atlas = font.atlas();
    let mut display = Display::open(
        "/dev/dri/card0",
        "/dev/input/by-path/platform-2005a000.i2c-event",
        [800, 1280],
        true,
    )?;
    display.set_font(
        atlas.curve_width,
        atlas.curve_height,
        &atlas.curves,
        atlas.band_width,
        atlas.band_height,
        &atlas.bands,
    )?;
    let projection = Surface::new(90)?.projection();
    if std::env::var_os("OTRIOS_TEXT_PROFILE").is_some() {
        let start = Instant::now();
        for i in 0..120 {
            let m = &meshes[i % meshes.len()].mesh;
            otrios_linux_display::validate_text_mesh(&m.glyphs, &m.draws, m.vertices.len() / 6)?;
        }
        println!(
            "Glyph validation mean={} us",
            start.elapsed().as_micros() / 120
        );
    }
    let mut draw_us = vec![];
    let mut present_us = vec![];
    let mut upload_us = vec![];
    let mut submit_us = vec![];
    let mut flips = vec![];
    let started = Instant::now();
    let mut previous = None;
    for i in 0..360 {
        let frame = &meshes[if mode == "static" {
            0
        } else {
            i % meshes.len()
        }];
        let mesh = &frame.mesh;
        let before = Instant::now();
        if previous.as_ref().is_none_or(|old| !Arc::ptr_eq(old, mesh)) {
            display.upload_scene(0, &mesh.vertices, &mesh.glyphs, &mesh.draws)?;
            display.text_styles(0, &mesh.styles)?;
        }
        previous = Some(mesh.clone());
        display.text_views(0, &frame.views)?;
        let upload = before.elapsed().as_micros();
        let draw_start = Instant::now();
        display.draw(projection, false)?;
        let submit = draw_start.elapsed().as_micros();
        let compose = before.elapsed().as_micros();
        let before = Instant::now();
        let flip = display.present()?;
        if i >= 120 {
            draw_us.push(compose);
            upload_us.push(upload);
            submit_us.push(submit);
            present_us.push(before.elapsed().as_micros());
            flips.push(flip);
        }
    }
    if let Some(path) = capture {
        let pixels = display.pixels()?;
        let mut ppm = b"P6\n800 1280\n255\n".to_vec();
        for row in pixels.chunks_exact(800 * 4).rev() {
            for p in row.chunks_exact(4) {
                ppm.extend_from_slice(&p[..3]);
            }
        }
        std::fs::write(path, ppm).map_err(|e| e.to_string())?;
    }
    let stats = display.glyph_cache_stats();
    upload_us.sort_unstable();
    submit_us.sort_unstable();
    println!(
        "Upload p50={} us; draw/cache submit p50={} us",
        upload_us[120], submit_us[120]
    );
    draw_us.sort_unstable();
    present_us.sort_unstable();
    let seconds = (flips.last().unwrap().usec - flips.first().unwrap().usec) as f64 / 1e6;
    let missed: u64 = flips
        .windows(2)
        .map(|w| u64::from(w[1].sequence.wrapping_sub(w[0].sequence).saturating_sub(1)))
        .sum();
    println!(
        "{{\"mode\":\"{mode}\",\"seconds\":{},\"kernel_fps\":{},\"missed_vblanks\":{missed},\"compose_p50_us\":{},\"compose_p95_us\":{},\"present_p50_us\":{},\"cache_hits\":{},\"cache_misses\":{},\"atlas_bytes\":{}}}",
        started.elapsed().as_secs_f64(),
        239. / seconds,
        draw_us[120],
        draw_us[228],
        present_us[120],
        stats[0],
        stats[1],
        stats[2]
    );
    Ok(())
}
