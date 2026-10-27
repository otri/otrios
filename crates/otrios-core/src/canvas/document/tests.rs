// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::dashboard::font::test_font;

fn reader(text: &str, width: f64, height: f64) -> Reader {
    Reader::new(
        Source::new(text.to_owned()).unwrap(),
        Arc::new(test_font()),
        2.,
        [255; 4],
        width,
        height,
    )
    .unwrap()
}
fn settle(r: &mut Reader) {
    for _ in 0..100_000 {
        r.work(16).unwrap();
        if r.background == r.summaries.len() && !r.frame().loading && r.job.is_none() {
            return;
        }
    }
    panic!("reader did not settle: {:?}", r.stats());
}
fn lines(r: &Reader, block: usize) -> Vec<LineRange> {
    r.pages
        .iter()
        .filter(|(k, _)| k.0 == block)
        .flat_map(|(_, p)| p.lines.iter().copied())
        .collect()
}
#[test]
fn golden_wrapping_preserves_spaces_newlines_tabs_and_whole_missing_clusters() {
    let mut r = reader("aa aa\n\nÉ👩‍💻b\r\nz\tz", 100., 100.);
    let a = f64::from(r.measure.font.glyph('a').advance) * r.measure.factor;
    let space = f64::from(r.measure.font.glyph(' ').advance) * r.measure.factor;
    r.resize(2. * a + space + a * 0.5, 200.).unwrap();
    settle(&mut r);
    let l = lines(&r, 0);
    assert_eq!(
        l.iter()
            .map(|l| (l.source.start, l.source.end))
            .collect::<Vec<_>>(),
        [(0, 3), (3, 5)]
    );
    assert_eq!(lines(&r, 1)[0].source, ClusterRange { start: 6, end: 6 });
    let emoji_start = "aa aa\n\nÉ".len() as u32;
    let emoji_end = emoji_start + "👩‍💻".len() as u32;
    let clusters: Vec<_> = r
        .pages
        .values()
        .flat_map(|p| p.chunks.iter())
        .flat_map(|c| c.clusters.iter().copied())
        .collect();
    assert_eq!(
        clusters.iter().filter(|c| c.start == emoji_start).count(),
        1
    );
    assert!(clusters.contains(&ClusterRange {
        start: emoji_start,
        end: emoji_end
    }));
    assert!(
        !clusters
            .iter()
            .any(|c| c.start > emoji_start && c.start < emoji_end)
    );
    assert_eq!(r.source.text(), "aa aa\n\nÉ👩‍💻b\r\nz\tz");
}
#[test]
fn scroll_reuses_immutable_meshes_and_culls_outside_lines() {
    let mut r = reader(
        &"jgyÉ text with descenders and overhang. ".repeat(300),
        600.,
        180.,
    );
    settle(&mut r);
    let first = r.frame();
    assert!(!first.loading);
    let generated = r.stats().packed_glyphs;
    r.scroll_to(5.125);
    let next = r.frame();
    assert!(Arc::ptr_eq(&first.draws[0].chunk, &next.draws[0].chunk));
    assert_eq!(first.draws[0].view[3] - next.draws[0].view[3], 5.125);
    assert_eq!(generated, r.stats().packed_glyphs);
    assert!(next.draws.iter().all(|d| d.view[1] > 0.));
    assert!(
        first.draws[0].chunk.mesh.glyph_vertices[0] < 0.,
        "negative bearing remains unclipped until draw"
    );
    let held = first.draws[0].chunk.mesh.glyph_vertices.clone();
    r.resize(320., 180.).unwrap();
    settle(&mut r);
    assert_eq!(first.draws[0].chunk.mesh.glyph_vertices, held);
    assert!(
        r.frame()
            .draws
            .iter()
            .all(|d| d.chunk.version > first.draws.last().unwrap().chunk.version)
    );
}
#[test]
fn resize_anchors_source_instead_of_old_pixel_offset() {
    let mut r = reader(
        &"a line of text to wrap and reflow across widths. ".repeat(50),
        400.,
        150.,
    );
    settle(&mut r);
    r.scroll_to(300.25);
    for _ in 0..100 {
        r.work(16).unwrap();
    }
    let a = r.anchor();
    r.resize(240., 150.).unwrap();
    settle(&mut r);
    let page_line = r
        .pages
        .values()
        .flat_map(|p| p.lines.iter())
        .find(|l| l.source.start <= a.source_byte && a.source_byte < l.source.end)
        .unwrap();
    let viewport_y = f64::from(page_line.line) * r.line_height - r.offset;
    assert!((viewport_y - a.viewport_y).abs() < 1e-8);
}
#[test]
fn single_long_paragraph_is_paged_and_pinned_consumers_apply_backpressure() {
    let mut r = reader(&"abcdefgh ijklmnop ".repeat(6000), 640., 300.);
    settle(&mut r);
    assert!(r.summaries[0].checkpoints.len() > 32);
    let mut held = Vec::new();
    for i in 0..120 {
        r.scroll_to(f64::from(i) * 250.);
        for _ in 0..100 {
            r.work(16).unwrap();
            if !r.frame().loading {
                break;
            }
        }
        held.push(r.frame());
        assert!(r.stats().working_bytes <= CPU_WORKING_BYTES);
    }
    assert!(
        r.stats().backpressure > 0,
        "held consumer resources cannot be reused or ignored"
    );
    let before = r.stats().working_bytes;
    drop(held);
    for _ in 0..1000 {
        r.work(16).unwrap();
        if !r.frame().loading {
            break;
        }
    }
    assert!(!r.frame().loading);
    assert!(r.stats().working_bytes < before);
}
#[test]
fn resumable_long_tokens_and_hard_chunk_limits_do_not_truncate_source() {
    let mut r = reader(&"W".repeat(20_000), 8192., 64.);
    settle(&mut r);
    let summary = &r.summaries[0];
    assert!(summary.exact);
    assert_eq!(summary.cursor as usize, r.source.text().len());
    for page in r.pages.values() {
        assert!(
            page.chunks
                .iter()
                .all(|c| c.clusters.len() <= MAX_CHUNK_GLYPHS)
        );
        assert!(
            page.chunks
                .iter()
                .all(|c| c.mesh.glyph_vertices.iter().all(|v| v.is_finite()))
        );
    }
    let first = r.source.blocks[0].start;
    assert!(
        r.jump_to(Anchor {
            block: 0,
            source_byte: first + 19_999,
            viewport_y: 0.
        })
        .is_ok()
    );
    for _ in 0..1000 {
        r.work(16).unwrap();
        if !r.frame().loading {
            break;
        }
    }
    assert!(!r.frame().loading);
    assert!(r.offset() <= (r.extent().0 - r.height).max(0.));
}
#[test]
fn zero_advance_measured_runs_yield_and_progress_at_whole_cluster_limits() {
    let source = Source::new("W".repeat(10_000)).unwrap();
    let mut measure = layout::Measure::new(Arc::new(test_font()), 2.);
    // Exercise a shaper's legal zero-advance output independently of this
    // prototype font's ordinary positive advances.
    measure.factor = 0.;
    let mut scan = layout::Scan::new(0);
    for _ in 0..63 {
        assert!(scan.step(&source, 10_000, 100., &mut measure, 64).is_none());
    }
    let mut line = None;
    for _ in 0..2 {
        line = scan.step(&source, 10_000, 100., &mut measure, 64);
        if line.is_some() {
            break;
        }
    }
    let line = line.unwrap();
    assert_eq!(line.end, 4096);
    assert_eq!(line.glyphs.len(), 4096);
}
#[test]
fn far_document_offsets_keep_subpixel_precision_after_local_rebasing() {
    let mut r = Reader::new(
        Source::new("W".repeat(200_000)).unwrap(),
        Arc::new(test_font()),
        8.,
        [255; 4],
        1.,
        64.,
    )
    .unwrap();
    settle(&mut r);
    assert!(r.extent().0 > 16_777_216.);
    r.scroll_to(17_000_000.125);
    for _ in 0..1000 {
        r.work(16).unwrap();
        if !r.frame().loading {
            break;
        }
    }
    let first = r.frame();
    assert!(!first.loading);
    r.scroll_to(r.offset() + 0.125);
    let next = r.frame();
    assert!(Arc::ptr_eq(&first.draws[0].chunk, &next.draws[0].chunk));
    assert_eq!(first.draws[0].view[3] - next.draws[0].view[3], 0.125);
}
