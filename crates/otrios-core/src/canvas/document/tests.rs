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
fn keyed(first: u64, count: u64) -> Source {
    let inline = Inline::new(
        vec![
            0., 0., 1., 0., 0., 1., 12., 0., 1., 0., 0., 1., 0., 12., 1., 0., 0., 1.,
        ],
        [4., -4.],
    )
    .unwrap();
    Source::from_blocks((first..first+count).map(|id| Block::new(
        BlockId { object: id, part: 0 },
        format!("Sender {id}\nA paragraph with an explicit hard break.\nSecond paragraph {}\n\n", "wrapped text ".repeat(6)),
        24., Some(inline.clone()),
    ).unwrap()).collect()).unwrap()
}

#[test]
fn keyed_windows_preserve_anchor_and_chunk_identity_across_prepend_append_and_reflow() {
    let mut r = Reader::new(
        keyed(20, 32),
        Arc::new(test_font()),
        2.,
        [255; 4],
        480.,
        160.,
    )
    .unwrap();
    settle(&mut r);
    r.jump_to(Anchor {
        block: 7,
        source_byte: 0,
        viewport_y: -3.5,
    })
    .unwrap();
    settle(&mut r);
    let anchor = r.anchor();
    let identity = r.source.block_id(anchor.block).unwrap();
    assert_eq!(identity.object, 27);
    let before = r.frame();
    let revision = r.stats().anchor_revision;
    let packed = r.stats().packed_glyphs;
    r.replace_blocks(keyed(10, 64)).unwrap();
    settle(&mut r);
    assert_eq!(r.source.block_id(r.anchor().block), Some(identity));
    assert_eq!(r.anchor().source_byte, anchor.source_byte);
    assert!((r.anchor().viewport_y - anchor.viewport_y).abs() < 1e-8);
    assert_eq!(r.stats().anchor_revision, revision);
    let after = r.frame();
    for old in &before.draws {
        let retained = after
            .draws
            .iter()
            .find(|d| d.chunk.version == old.chunk.version)
            .unwrap();
        assert!(Arc::ptr_eq(&old.chunk, &retained.chunk));
        assert!((old.view[3] - retained.view[3]).abs() < 1e-5);
    }
    assert_eq!(
        r.stats().packed_glyphs,
        packed,
        "unrelated new source blocks do not repack a warm visible window"
    );
    assert!(
        after
            .draws
            .iter()
            .any(|d| !d.chunk.mesh.vertices.is_empty())
    );
    for draw in after
        .draws
        .iter()
        .filter(|d| !d.chunk.mesh.vertices.is_empty())
    {
        let views = draw.views();
        assert_eq!(
            &views[2..8],
            &views[10..16],
            "inline art and text share translation/clip"
        );
        assert_eq!(draw.chunk.mesh.draws[0], 0);
        assert_eq!(draw.chunk.mesh.draws[3], 1);
    }
    let old_bytes = before.draws[0].chunk.mesh.glyph_vertices.clone();
    r.resize(260., 160.).unwrap();
    settle(&mut r);
    assert_eq!(r.source.block_id(r.anchor().block), Some(identity));
    let line = lines(&r, r.anchor().block as usize)
        .into_iter()
        .find(|l| l.source.start <= anchor.source_byte && anchor.source_byte < l.source.end)
        .unwrap();
    let y = r.heights.prefix(r.anchor().block as usize) + f64::from(line.line) * r.line_height
        - r.offset;
    assert!((y - anchor.viewport_y).abs() < 1e-8);
    assert_eq!(
        before.draws[0].chunk.mesh.glyph_vertices, old_bytes,
        "pinned old-width geometry remains immutable"
    );
}

#[test]
fn keyed_revisions_invalidate_only_changed_content_and_window_traversal_stays_bounded() {
    let mut r = Reader::new(
        keyed(1, 64),
        Arc::new(test_font()),
        2.,
        [255; 4],
        480.,
        160.,
    )
    .unwrap();
    settle(&mut r);
    let old = r.frame();
    let unaffected = r.pages[&(1, 0)].chunks[0].clone();
    let mut source = keyed(1, 64);
    let changed = Block::new(
        BlockId { object: 1, part: 0 },
        "A changed sender\nNew content\n\n".into(),
        24.,
        source.inline(0).cloned(),
    )
    .unwrap();
    let mut blocks = (0..64)
        .map(|i| {
            if i == 0 {
                changed.clone()
            } else {
                Block::new(
                    source.key(i).0,
                    source.block(i as u32).unwrap().to_owned(),
                    24.,
                    source.inline(i).cloned(),
                )
                .unwrap()
            }
        })
        .collect::<Vec<_>>();
    source = Source::from_blocks(std::mem::take(&mut blocks)).unwrap();
    r.replace_blocks(source).unwrap();
    settle(&mut r);
    assert!(
        r.frame()
            .draws
            .iter()
            .any(|d| d.chunk.version > old.draws.last().unwrap().chunk.version)
    );
    assert!(
        Arc::ptr_eq(&unaffected, &r.pages[&(1, 0)].chunks[0]),
        "a neighboring unchanged record keeps its cached layout"
    );
    for first in (1..=1025).step_by(32) {
        r.replace_blocks(keyed(first, 64)).unwrap();
        settle(&mut r);
        let stats = r.stats();
        assert_eq!(stats.blocks, 64);
        assert!(stats.source_bytes < 512 * 1024);
        assert!(stats.source_index_bytes < source::MAX_INDEX_BYTES);
        assert!(stats.working_bytes <= CPU_WORKING_BYTES);
        assert!(stats.resident_pages <= MAX_RESIDENT_PAGES);
        assert!(
            r.frame()
                .draws
                .iter()
                .map(|d| d.chunk.gpu_bytes())
                .sum::<usize>()
                <= GPU_GEOMETRY_BYTES
        );
    }
}

#[test]
fn prefix_labels_preserve_the_message_anchor_when_older_coverage_arrives() {
    let labeled = |first| {
        let source = keyed(first, 32);
        let target = BlockId {
            object: first,
            part: 0,
        };
        let mut blocks = vec![
            Block::label(
                BlockId {
                    object: first,
                    part: 2,
                },
                String::new(),
                24.,
                target,
            )
            .unwrap(),
            Block::label(
                BlockId {
                    object: first,
                    part: 1,
                },
                "Today".into(),
                24.,
                target,
            )
            .unwrap(),
        ];
        blocks.extend((0..32).map(|i| {
            Block::new(
                source.key(i).0,
                source.block(i as u32).unwrap().into(),
                24.,
                source.inline(i).cloned(),
            )
            .unwrap()
        }));
        Source::from_blocks(blocks).unwrap()
    };
    let mut r = Reader::new(labeled(20), Arc::new(test_font()), 2., [255; 4], 480., 160.).unwrap();
    settle(&mut r);
    let anchor = r.anchor();
    assert_eq!(
        r.source.block_id(anchor.block),
        Some(BlockId {
            object: 20,
            part: 0
        })
    );
    assert!(anchor.viewport_y > 0.);
    r.replace_blocks(labeled(10)).unwrap();
    settle(&mut r);
    assert_eq!(
        r.source.block_id(r.anchor().block),
        Some(BlockId {
            object: 20,
            part: 0
        })
    );
    assert!((r.anchor().viewport_y - anchor.viewport_y).abs() < 1e-8);
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
    assert_eq!(r.source.original_text().unwrap(), "aa aa\n\nÉ👩‍💻b\r\nz\tz");
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
    assert_eq!(
        summary.cursor as usize,
        r.source.original_text().unwrap().len()
    );
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
        assert!(
            scan.step(&source, 0, 10_000, 100., &mut measure, 64)
                .is_none()
        );
    }
    let mut line = None;
    for _ in 0..2 {
        line = scan.step(&source, 0, 10_000, 100., &mut measure, 64);
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

#[test]
fn appended_paragraphs_preserve_existing_geometry_and_anchor() {
    let mut r = reader("first paragraph\nsecond paragraph", 600., 120.);
    settle(&mut r);
    let before = r.frame();
    let anchor = r.anchor();
    let original = r.source.original_text().unwrap().to_owned();
    r.append_blocks(&["third paragraph".into(), "fourth paragraph".into()])
        .unwrap();
    settle(&mut r);
    assert_eq!(r.anchor(), anchor);
    assert!(r.source.original_text().unwrap().starts_with(&original));
    assert_eq!(r.source.block(2), Some("third paragraph"));
    for old in before.draws {
        let retained = r
            .frame()
            .draws
            .into_iter()
            .find(|d| d.chunk.version == old.chunk.version)
            .unwrap();
        assert!(Arc::ptr_eq(&retained.chunk, &old.chunk));
    }
    let snapshot = r.source.original_text().unwrap().to_owned();
    assert!(r.append_blocks(&["bad\nparagraph".into()]).is_err());
    assert_eq!(r.source.original_text().unwrap(), snapshot);
}
