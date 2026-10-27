// SPDX-License-Identifier: LGPL-3.0-only
//! CPU generation qualification, not presentation FPS. No sleeps/driver timing.
use otrios_core::{
    canvas::document::{CPU_WORKING_BYTES, Reader, Source},
    dashboard::{font::Font, font_reference},
};
use std::{sync::Arc, time::Instant};
fn percentile(values: &mut [u64], q: f64) -> u64 {
    values.sort_unstable();
    values[((values.len() - 1) as f64 * q).round() as usize]
}
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("Supply UTF-8 corpus file")?;
    let profiled = std::env::args().any(|a| a == "--profile");
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let bytes = raw.len();
    let start = Instant::now();
    let source = Source::new(raw)?;
    let source_us = start.elapsed().as_micros();
    let font = Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?);
    let mut reader = Reader::new(source, font, 2., [239, 244, 236, 255], 1184., 600.)?;
    reader.set_profiling(profiled);
    let start = Instant::now();
    let mut first_page_us = None;
    while !reader.extent().1 {
        reader.work(16)?;
        if first_page_us.is_none() && !reader.frame().loading {
            first_page_us = Some(start.elapsed().as_micros());
        }
    }
    let heights_us = start.elapsed().as_micros();
    let start = Instant::now();
    let mut peak = 0;
    let mut maximum_pages = 0;
    let mut loading = 0;
    let mut turns = 0;
    let mut samples = Vec::new();
    let extent = reader.extent().0;
    let mut offset = 0.;
    let mut anchor = reader.anchor();
    while offset < extent - 600. {
        let before = Instant::now();
        reader.scroll_to(offset);
        reader.work(16)?;
        let frame = reader.frame();
        samples.push(before.elapsed().as_micros() as u64);
        loading += usize::from(frame.loading);
        let stats = reader.stats();
        peak = peak.max(stats.working_bytes);
        maximum_pages = maximum_pages.max(stats.resident_pages);
        if stats.working_bytes > CPU_WORKING_BYTES {
            return Err("Document CPU budget exceeded".into());
        }
        turns += 1;
        offset += 4000. / 60.;
        if offset >= extent * 0.5 && anchor.source_byte == 0 {
            anchor = reader.anchor();
        }
    }
    let generation_us = start.elapsed().as_micros();
    let stats = reader.stats();
    let mut warm = Vec::new();
    for i in 0..1000 {
        let before = Instant::now();
        reader.scroll_to((extent - 900. + f64::from(i % 8) * 0.125).max(0.));
        reader.frame();
        warm.push(before.elapsed().as_micros() as u64);
    }
    let start = Instant::now();
    reader.jump_to(anchor)?;
    reader.resize(704., 600.)?;
    let mut reflow_turns = 0;
    while reader.frame().loading {
        reader.work(16)?;
        reflow_turns += 1;
        if reflow_turns > 100_000 {
            return Err("Reflow did not complete".into());
        }
    }
    let reflow_us = start.elapsed().as_micros();
    println!(
        "{{\"source_bytes\":{bytes},\"source_admission_us\":{source_us},\"first_page_us\":{},\"exact_heights_us\":{heights_us},\"generation_us\":{generation_us},\"viewport_turns\":{turns},\"loading_turns\":{loading},\"viewport_p95_us\":{},\"warm_p95_us\":{},\"working_peak_bytes\":{peak},\"index_bytes\":{},\"max_resident_pages\":{maximum_pages},\"measured_clusters\":{},\"broken_clusters\":{},\"packed_glyphs\":{},\"lines\":{},\"work_ns\":{},\"max_work_ns\":{},\"measurement_ns\":{},\"breaking_ns\":{},\"packing_ns\":{},\"reflow_us\":{reflow_us}}}",
        first_page_us.unwrap_or(heights_us),
        percentile(&mut samples, 0.95),
        percentile(&mut warm, 0.95),
        stats.source_index_bytes,
        stats.measured_clusters,
        stats.broken_clusters,
        stats.packed_glyphs,
        stats.laid_out_lines,
        stats.work_ns,
        stats.max_work_ns,
        stats.measurement_ns,
        stats.breaking_ns,
        stats.packing_ns
    );
    Ok(())
}
