// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/layout-document/app.rs"]
mod demo;
#[path = "../examples/common/document_reference.rs"]
mod reference;
use otrios_core::{
    canvas::*,
    dashboard::{font::Font, font_reference},
};
use otrios_interfaces::{
    canvas::{Catalog, CatalogEntry, Presentation},
    scene::{PointerEvent, PointerPhase},
};
use std::sync::Arc;
#[test]
fn paged_lines_match_an_independent_unpaged_scalar_oracle_at_every_boundary() {
    let text = "jgyÉ aa aa-aaaa\t words  words 👩‍💻 tail.\r\n\n".repeat(40);
    let font = Arc::new(
        Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    );
    for width in [110., 267.25, 704.] {
        let reference = reference::Reference::new(&text, &font, 2, width);
        assert!(
            !reference
                .scene(width as f32, 200., 0.)
                .root()
                .children
                .is_empty()
        );
        let mut reader = document::Reader::new(
            document::Source::new(text.clone()).unwrap(),
            font.clone(),
            2.,
            [239, 244, 236, 255],
            width,
            200.,
        )
        .unwrap();
        let mut offset = 0.;
        let mut seen = std::collections::BTreeSet::new();
        while offset < reference.height {
            reader.scroll_to(offset);
            for _ in 0..1000 {
                reader.work(16).unwrap();
                if !reader.frame().loading {
                    break;
                }
            }
            assert!(!reader.frame().loading);
            for (block, line) in reader.resident_lines() {
                seen.insert((block, line.line));
                let expected = reference
                    .lines
                    .iter()
                    .find(|l| l.block == block && l.number == line.line)
                    .unwrap();
                assert_eq!(
                    (line.source.start, line.source.end),
                    (expected.start, expected.end)
                );
            }
            offset += 117.125;
        }
        assert_eq!(
            seen.len(),
            reference.lines.len(),
            "every reference line was compared"
        );
        for _ in 0..10000 {
            reader.work(16).unwrap();
            if reader.extent().1 {
                break;
            }
        }
        assert!(
            (reader.extent().0 - reference.height).abs() < 1e-8,
            "stale estimates cannot append phantom blank lines"
        );
    }
}
fn mount(ui: &otrios_interfaces::canvas::Ui) -> Result<Client> {
    let mut client = Client::default();
    let bytes = encode_ui(ui)?;
    let app = demo::manifest();
    client.admit_catalog(&encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![CatalogEntry {
            app: app.clone(),
            path: String::new(),
            private: true,
            ui_hash: hash(&bytes),
        }],
    })?)?;
    client.set_font(Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?));
    client.mount_local(&bytes, &app.state_ref, &[], &[])?;
    Ok(client)
}
#[test]
fn admitted_reader_keeps_ui_and_state_static_while_scrolling_and_clears_private_resources() {
    let ui = demo::ui(&[]);
    assert_eq!(ui.format, "otrios-canvas-v4");
    assert!(encode_ui(&ui).unwrap().len() < MAX_BYTES);
    let mut client = mount(&ui).unwrap();
    for now in 0..200 {
        client.tick(now * 8, 0).unwrap();
        if !client.document_frame().unwrap().loading {
            break;
        }
    }
    let before = client.document_frame().unwrap();
    assert!(!before.loading);
    let scene = client.scene().unwrap();
    let Presentation::TextDocument { viewport, .. } = client.presentation().unwrap() else {
        panic!("document")
    };
    let path = scene.path(u32::from(*viewport)).unwrap();
    let p = path.last().unwrap();
    let x = p.origin[0] + 50.;
    let y = p.origin[1] + 200.;
    let pointer = |client: &mut Client, phase, y| {
        client
            .pointer(
                client.epoch(),
                PointerEvent {
                    id: 7,
                    phase,
                    position: [x, y],
                },
            )
            .unwrap()
    };
    pointer(&mut client, PointerPhase::Down, y);
    assert!(client.input_busy());
    client.tick(2000, 0).unwrap();
    pointer(&mut client, PointerPhase::Move, y - 100.);
    assert_eq!(client.resource_usage().text_document.unwrap().offset, 100.);
    client.tick(2025, 0).unwrap();
    pointer(&mut client, PointerPhase::Up, y - 100.);
    assert!(!client.input_busy());
    client.tick(2050, 0).unwrap();
    assert!(client.resource_usage().text_document.unwrap().offset > 100.);
    assert!(
        Arc::ptr_eq(&scene, &client.scene().unwrap()),
        "document pans never rebuild the UI tree"
    );
    assert_eq!(client.pending_count(), 0);
    assert!(!before.draws.is_empty());
    let held = before.draws[0].chunk.mesh.glyph_vertices.clone();
    client.set_viewport([800., 1280.]).unwrap();
    for now in 0..1000 {
        client.tick(3000 + now * 8, 0).unwrap();
        if !client.document_frame().unwrap().loading {
            break;
        }
    }
    assert!(!client.document_frame().unwrap().loading);
    assert_eq!(held, before.draws[0].chunk.mesh.glyph_vertices);
    client.clear_private();
    assert!(client.document_frame().is_none());
    assert!(client.resource_usage().text_document.is_none());
    assert_eq!(
        held, before.draws[0].chunk.mesh.glyph_vertices,
        "held immutable snapshot survives owner teardown"
    );
}
#[test]
fn old_ui_formats_and_corrupt_resource_identity_cannot_admit_documents() {
    let mut ui = demo::ui(&[]);
    ui.format = "otrios-canvas-v3".into();
    assert!(mount(&ui).is_err());
    ui.format = "otrios-canvas-v4".into();
    if let Some(Presentation::TextDocument { source, .. }) = &mut ui.presentation {
        source.sha256[0] ^= 1;
    }
    assert!(mount(&ui).is_err());
}
