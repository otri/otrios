// SPDX-License-Identifier: Apache-2.0
//! Real kernel-presented document scrolling, with the same safe resource adapter
//! as the native app. Use an exclusive-display controller that restores services.
#[path = "../../otrios-dashboard/src/document_gpu.rs"]
mod document_gpu;
use otrios_core::{
    canvas::document::{Reader, Source},
    dashboard::{display::Surface, font::Font, font_reference},
};
use otrios_linux_display::Display;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
fn p(values: &mut [u64], q: f64) -> u64 {
    values.sort_unstable();
    values[((values.len() - 1) as f64 * q).round() as usize]
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .ok_or("Supply UTF-8 corpus, seconds, speed, rotation, optional capture")?;
    let seconds = args
        .next()
        .unwrap_or("30".into())
        .parse::<f64>()
        .map_err(|e| e.to_string())?;
    let speed = args
        .next()
        .unwrap_or("4000".into())
        .parse::<f64>()
        .map_err(|e| e.to_string())?;
    let rotation = args
        .next()
        .unwrap_or("90".into())
        .parse::<u16>()
        .map_err(|e| e.to_string())?;
    let capture = args.next();
    if !seconds.is_finite()
        || !(1. ..=120.).contains(&seconds)
        || !speed.is_finite()
        || !(0. ..=4000.).contains(&speed)
    {
        return Err("Invalid probe bounds".into());
    }
    let logical_size = if rotation % 180 == 90 {
        [1280, 800]
    } else {
        [800, 1280]
    };
    let surface = Surface::with_size(rotation, logical_size)?;
    let extent = logical_size.map(|n| n as f32);
    let start = Instant::now();
    let source = Source::new(std::fs::read_to_string(source).map_err(|e| e.to_string())?)?;
    let source_bytes = source.source_bytes();
    let source_us = start.elapsed().as_micros();
    let font = Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?);
    let width = f64::from(extent[0]) - 96.;
    let height = f64::from(extent[1]) - 200.;
    let mut reader = Reader::new(
        source,
        font.clone(),
        2.,
        [239, 244, 236, 255],
        width,
        height,
    )?;
    let start = Instant::now();
    while reader.frame().loading {
        reader.work(16)?;
    }
    let first_visible_us = start.elapsed().as_micros();
    let mut display = Display::open(
        "/dev/dri/card0",
        "/dev/input/by-path/platform-2005a000.i2c-event",
        [800, 1280],
        true,
    )?;
    let atlas = font.atlas();
    display.set_font(
        atlas.curve_width,
        atlas.curve_height,
        &atlas.curves,
        atlas.band_width,
        atlas.band_height,
        &atlas.bands,
    )?;
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let warmed = Arc::new(AtomicBool::new(false));
    let worker_warmed = warmed.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    let started = Instant::now();
    let worker = thread::spawn(move || -> Result<_, String> {
        let mut work = Vec::new();
        let mut peak = 0;
        let mut peak_index = 0;
        let mut pending = None;
        let mut motion_started: Option<Instant> = None;
        while !worker_stop.load(Ordering::Acquire) {
            let elapsed = if worker_warmed.load(Ordering::Acquire) {
                motion_started
                    .get_or_insert_with(Instant::now)
                    .elapsed()
                    .as_secs_f64()
                    .min(seconds)
            } else {
                0.
            };
            let offset = if elapsed < seconds / 2. {
                elapsed * speed
            } else {
                (seconds - elapsed) * speed
            };
            reader.scroll_to(offset);
            let before = Instant::now();
            reader.work(16)?;
            let mut frame = reader.frame();
            work.push(before.elapsed().as_micros() as u64);
            let stats = reader.stats();
            peak = peak.max(stats.working_bytes);
            peak_index = peak_index.max(stats.source_index_bytes);
            for draw in &mut frame.draws {
                draw.view[2] += 48.;
                draw.view[3] += 100.;
                draw.view[4..].copy_from_slice(&[
                    48.,
                    100.,
                    48. + width as f32,
                    100. + height as f32,
                ]);
            }
            // Superseded metadata is replaced; the channel and pending frame
            // retain immutable Arc resources until the display releases them.
            drop(pending.take());
            match tx.try_send(frame) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(frame)) => pending = Some(frame),
                Err(_) => break,
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok((reader.stats(), peak, peak_index, work))
    });
    let mut cache = document_gpu::Cache::default();
    let mut flips: Vec<otrios_linux_display::Flip> = Vec::new();
    let mut startup_us = 0;
    let mut draw_us = Vec::new();
    let mut loading = 0;
    let mut cold_loading = 0;
    let mut cold = true;
    let mut model = None;
    let result = (|| -> Result<(), String> {
        while flips
            .last()
            .is_none_or(|last| last.usec - flips[0].usec < (seconds * 1_000_000.) as u64)
        {
            while let Ok(frame) = rx.try_recv() {
                model = Some(frame);
            }
            let Some(frame) = &model else {
                thread::sleep(Duration::from_millis(1));
                continue;
            };
            let before = Instant::now();
            let (slots, uploading) = cache.update(&mut display, Some(frame))?;
            if frame.loading || uploading {
                if cold {
                    cold_loading += 1;
                } else {
                    loading += 1;
                }
            } else {
                cold = false;
            }
            display.draw_slots(surface.projection(), &slots)?;
            draw_us.push(before.elapsed().as_micros() as u64);
            let flip = display.present()?;
            // Initial modesetting has no page-flip event/timestamp.
            if flip.usec != 0 {
                if flips.is_empty() {
                    startup_us = started.elapsed().as_micros();
                    warmed.store(true, Ordering::Release);
                }
                flips.push(flip);
            }
        }
        if let Some(path) = capture {
            let rgba = display.pixels()?;
            let mut ppm = b"P6\n800 1280\n255\n".to_vec();
            for row in rgba.chunks_exact(800 * 4).rev() {
                for pixel in row.chunks_exact(4) {
                    ppm.extend_from_slice(&pixel[..3]);
                }
            }
            std::fs::write(path, ppm).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    stop.store(true, Ordering::Release);
    let (stats, peak, peak_index, mut work) =
        worker.join().map_err(|_| "Document worker panicked")??;
    result?;
    if flips.len() < 2 || flips.first().unwrap().usec == 0 {
        return Err("Probe requires real kernel flip timestamps".into());
    }
    let span = flips.last().unwrap().usec - flips.first().unwrap().usec;
    let fps = (flips.len() - 1) as f64 * 1_000_000. / span as f64;
    let mut intervals: Vec<_> = flips.windows(2).map(|w| w[1].usec - w[0].usec).collect();
    let missed: u64 = flips
        .windows(2)
        .map(|w| u64::from(w[1].sequence.wrapping_sub(w[0].sequence).saturating_sub(1)))
        .sum();
    println!(
        "{{\"source_bytes\":{source_bytes},\"source_admission_us\":{source_us},\"first_visible_us\":{first_visible_us},\"seconds\":{seconds},\"startup_us\":{startup_us},\"kernel_span_us\":{span},\"speed\":{speed},\"rotation\":{rotation},\"kernel_fps\":{fps},\"frames\":{},\"interval_p50_us\":{},\"interval_p95_us\":{},\"interval_p99_us\":{},\"missed_vblanks\":{missed},\"loading_frames\":{loading},\"cold_loading_frames\":{cold_loading},\"draw_p95_us\":{},\"worker_p95_us\":{},\"working_peak_bytes\":{peak},\"index_peak_bytes\":{peak_index},\"packed_glyphs\":{},\"measured_clusters\":{},\"uploaded_bytes\":{},\"chunk_uploads\":{},\"gpu_evictions\":{}}}",
        flips.len(),
        p(&mut intervals, 0.5),
        p(&mut intervals, 0.95),
        p(&mut intervals, 0.99),
        p(&mut draw_us, 0.95),
        p(&mut work, 0.95),
        stats.packed_glyphs,
        stats.measured_clusters,
        cache.uploaded_bytes,
        cache.uploads,
        cache.evictions
    );
    Ok(())
}
