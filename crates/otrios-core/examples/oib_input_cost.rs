// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded CPU probe for Live UI input, scrolling and native frame diagnostics.
//! No network, persistent app data or GPU; run the ARM build on Frame.
#[path = "../../../apps/live-ui/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::{presentation::Composer, *};
use otrios_core::dashboard::font::Font;
use otrios_interfaces::{
    canvas::*,
    scene::{PointerEvent, PointerPhase, TouchResponder},
};
use std::{sync::Arc, time::Instant};

fn main() -> Result<()> {
    let ui = app::ui(&[]);
    let mut doc = Document::new(ui.state_schema.clone(), "oib:cost")?;
    doc.configure_oib(&ui)?;
    let bytes = encode_ui(&ui)?;
    let mut manifest = app::manifest();
    manifest.state_ref = "oib:cost".into();
    let mut client = Client::default();
    client.set_viewport([1280., 800.])?;
    let font = Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )?);
    client.set_font(font.clone());
    client.admit_catalog(&encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![CatalogEntry {
            app: manifest,
            path: "live-ui".into(),
            ui_hash: hash(&bytes),
            private: false,
        }],
    })?)?;
    client.open(&ui.ui_ref, "oib:cost");
    let ClientMessage::Mount { epoch, writer, .. } =
        decode_client(&client.outgoing()?.ok_or("Mount missing")?)?
    else {
        return Err("Mount missing".into());
    };
    let (mut session, _) = doc.open_session(&writer, 0)?;
    session.set_oib_editable(true);
    session.set_selection(StateSelection::from_ui(&ui)?)?;
    client.receive(&encode_server(&ServerMessage::Mounted {
        epoch,
        ui: bytes.clone(),
        ui_hash: hash(&bytes),
        code: vec![],
        state: doc.generate(&mut session)?.ok_or("Initial state missing")?,
        binding: None,
        initial_view: None,
    })?)?;
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = client.outgoing()? {
            if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes)? {
                doc.receive(&mut session, &bytes)?;
                progress = true;
            }
        }
        if let Some(bytes) = doc.generate(&mut session)? {
            client.receive(&encode_server(&ServerMessage::Sync { epoch, bytes })?)?;
            progress = true;
        }
        if !progress {
            break;
        }
    }
    assert!(client.confirmed());
    let mut composer = Composer::default();
    composer.compose(&mut client, &font)?;
    let measure = |samples: &mut Vec<f64>, start: Instant| {
        samples.push(start.elapsed().as_secs_f64() * 1000.)
    };
    let mut tick_ms = vec![];
    let mut diagnostics_ms = vec![];
    for i in 0..100 {
        let start = Instant::now();
        client.tick(1000 + i * 16, 0)?;
        measure(&mut tick_ms, start);
        if i < 24 {
            let start = Instant::now();
            std::hint::black_box(client.resource_usage());
            measure(&mut diagnostics_ms, start);
        }
    }
    let scene = client.scene()?;
    let slider = scene
        .controls()
        .into_iter()
        .find(|c| matches!(c.node.responder, Some(TouchResponder::Drag(_))))
        .ok_or("Slider missing")?;
    let origin = slider.origin;
    let mut slider_pointer_ms = vec![];
    let mut slider_compose_ms = vec![];
    for i in 0..24 {
        let start = Instant::now();
        client.pointer_at(
            epoch,
            PointerEvent {
                id: 1,
                phase: if i == 0 {
                    PointerPhase::Down
                } else if i == 23 {
                    PointerPhase::Up
                } else {
                    PointerPhase::Move
                },
                position: [origin[0] + 40. + i as f32 * 8., origin[1] + 20.],
            },
            3000 + i * 33,
        )?;
        measure(&mut slider_pointer_ms, start);
        let start = Instant::now();
        composer.compose(&mut client, &font)?;
        measure(&mut slider_compose_ms, start);
    }
    let mut panel_pointer_ms = vec![];
    let mut panel_compose_ms = vec![];
    for i in 0..24 {
        let start = Instant::now();
        client.pointer_at(
            epoch,
            PointerEvent {
                id: 2,
                phase: if i == 0 {
                    PointerPhase::Down
                } else if i == 23 {
                    PointerPhase::Up
                } else {
                    PointerPhase::Move
                },
                position: [1200., 560. - i as f32 * 14.],
            },
            5000 + i * 33,
        )?;
        measure(&mut panel_pointer_ms, start);
        let start = Instant::now();
        composer.compose(&mut client, &font)?;
        measure(&mut panel_compose_ms, start);
    }
    assert!(
        client
            .resource_usage()
            .panel_scroll
            .ok_or("Panel missing")?
            .offset[1]
            > 0.
    );
    println!(
        "{}",
        serde_json::json!({"tick_ms":tick_ms,"diagnostics_ms":diagnostics_ms,"slider_pointer_ms":slider_pointer_ms,"slider_compose_ms":slider_compose_ms,"panel_pointer_ms":panel_pointer_ms,"panel_compose_ms":panel_compose_ms})
    );
    Ok(())
}
