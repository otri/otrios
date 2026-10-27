// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/layout-text-scroll/app.rs"]
mod demo;
use otrios_core::{
    canvas::*,
    dashboard::{font::Font, font_reference, retained_text::TextPresentation},
};
use otrios_interfaces::{
    canvas::Presentation,
    scene::{PointerEvent, PointerPhase},
};
use std::sync::Arc;
#[test]
fn full_page_drag_coasts_culls_and_keeps_geometry_and_state_immutable() {
    let ui = demo::ui(&[]);
    let app = demo::manifest();
    let mut client = Client::default();
    client
        .admit_catalog(
            &encode_catalog(&otrios_interfaces::canvas::Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![otrios_interfaces::canvas::CatalogEntry {
                    path: String::new(),
                    private: false,
                    ui_hash: hash(&encode_ui(&ui).unwrap()),
                    app: app.clone(),
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client
        .mount_local(&encode_ui(&ui).unwrap(), &app.state_ref, &[], &[])
        .unwrap();
    let font = Arc::new(
        Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    );
    client.set_font(font.clone());
    client.tick(0, 0).unwrap();
    let Presentation::TextScroll { viewport, .. } = ui.presentation.unwrap() else {
        panic!("scroll presentation")
    };
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(viewport)).unwrap();
    let p = path.last().unwrap();
    let start = [
        p.origin[0] + p.node.frame.width * 0.8,
        p.origin[1] + p.node.frame.height * 0.8,
    ];
    let mut presentation = TextPresentation::default();
    let first = presentation.compose(&scene, &font);
    let original = first.mesh.glyph_vertices.clone();
    let total = first.mesh.glyph_vertices.len() / 64;
    assert!(total > 10000 && total < 16384);
    let visible: usize = first.views.chunks_exact(8).map(|v| v[1] as usize / 4).sum();
    assert!(visible > 1000 && visible < total);
    let input = |client: &mut Client, now, phase, position| {
        client.tick(now, 0).unwrap();
        client
            .pointer(
                client.epoch(),
                PointerEvent {
                    id: 7,
                    phase,
                    position,
                },
            )
            .unwrap();
    };
    input(&mut client, 0, PointerPhase::Down, start);
    assert!(client.input_busy());
    input(
        &mut client,
        50,
        PointerPhase::Move,
        [start[0] - 150., start[1] - 200.],
    );
    let moved = client.text_scroll_info().unwrap();
    assert_eq!(moved.offset, [150., 200.]);
    input(
        &mut client,
        60,
        PointerPhase::Up,
        [start[0] - 150., start[1] - 200.],
    );
    assert!(!client.input_busy());
    client.tick(110, 0).unwrap();
    let coast = client.text_scroll_info().unwrap();
    assert!(coast.offset[0] > 150. && coast.offset[1] > 200.);
    let second = presentation.compose(&client.scene().unwrap(), &font);
    assert!(
        Arc::ptr_eq(&first.mesh, &second.mesh),
        "Viewport moves must not rebuild text"
    );
    assert!(Arc::ptr_eq(&first.styles, &second.styles));
    assert_eq!(first.mesh.glyph_vertices, original);
    assert_eq!(
        client.pending_count(),
        0,
        "Panning never writes shared state"
    );
    input(&mut client, 120, PointerPhase::Down, start);
    input(&mut client, 170, PointerPhase::Move, [-10000., -10000.]);
    let end = client.text_scroll_info().unwrap();
    assert_eq!(end.offset, end.limit);
    input(&mut client, 180, PointerPhase::Move, [-9990., -9990.]);
    let reversed = client.text_scroll_info().unwrap();
    assert_eq!(reversed.offset, [end.limit[0] - 10., end.limit[1] - 10.]);
    client.cancel().unwrap();
    client.tick(1000, 0).unwrap();
    assert!(!client.text_scroll_info().unwrap().active);
    client.set_viewport([800., 1280.]).unwrap();
    client.tick(1100, 0).unwrap();
    let resized = client.text_scroll_info().unwrap();
    assert!(
        resized
            .offset
            .iter()
            .zip(resized.limit)
            .all(|(o, l)| *o >= 0. && *o <= l)
    );
}
#[test]
fn scroll_ui_is_explicitly_versioned_and_rejects_invalid_bounds() {
    let mut ui = demo::ui(&[]);
    assert_eq!(ui.format, "otrios-canvas-v3");
    ui.format = "otrios-canvas-v2".into();
    assert!(validate_ui_format(&ui).is_err());
    ui.format = "otrios-canvas-v3".into();
    if let Some(Presentation::TextScroll { content, .. }) = &mut ui.presentation {
        content[0] = f32::INFINITY;
    }
    let mut c = Client::default();
    c.admit_catalog(
        &encode_catalog(&otrios_interfaces::canvas::Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: vec![otrios_interfaces::canvas::CatalogEntry {
                path: String::new(),
                private: false,
                ui_hash: hash(&encode_ui(&ui).unwrap()),
                app: demo::manifest(),
            }],
        })
        .unwrap(),
    )
    .unwrap();
    assert!(
        c.mount_local(
            &encode_ui(&ui).unwrap(),
            &demo::manifest().state_ref,
            &[],
            &[]
        )
        .is_err()
    );
}
