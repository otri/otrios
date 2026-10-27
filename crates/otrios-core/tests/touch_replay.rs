// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/layout-document/app.rs"]
mod document;
#[allow(dead_code)]
#[path = "../../../apps/component-playground/app.rs"]
mod playground;
#[path = "../../../apps/layout-text-scroll/app.rs"]
mod scrolling;
use otrios_core::canvas::touch_trace::{Point, Trace};
use otrios_core::{
    canvas::*,
    dashboard::{font::Font, font_reference},
};
use otrios_interfaces::canvas::{App, Catalog, CatalogEntry, Ui};
use std::sync::Arc;
fn client(app: App, ui: Ui, code: &[u8]) -> Client {
    let mut c = Client::default();
    c.admit_catalog(
        &encode_catalog(&Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: vec![CatalogEntry {
                path: "test".into(),
                private: false,
                ui_hash: hash(&encode_ui(&ui).unwrap()),
                app: app.clone(),
            }],
        })
        .unwrap(),
    )
    .unwrap();
    c.mount_local(&encode_ui(&ui).unwrap(), &app.state_ref, code, &[])
        .unwrap();
    c.set_font(Arc::new(
        Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    ));
    for _ in 0..100_000 {
        c.tick(0, 0).unwrap();
        if c.resource_usage()
            .text_document
            .is_none_or(|s| s.extent_exact && !s.loading)
        {
            break;
        }
    }
    c
}
fn trace(control: &str, points: Vec<Point>) -> Trace {
    Trace {
        format: "otrios-touch-trace-v1".into(),
        source: "synthetic-golden".into(),
        screen: "test".into(),
        control: control.into(),
        viewport: [1280., 800.],
        mapping: None,
        points,
    }
}
fn point(at_ms: u64, phase: u8, position: [f32; 2]) -> Point {
    Point {
        at_ms,
        phase,
        id: 1,
        position,
    }
}
#[test]
fn scroll_replay_rejects_noisy_slow_release_without_changing_retained_geometry() {
    let mut c = client(scrolling::manifest(), scrolling::ui(&[]), &[]);
    let original = c.scene().unwrap();
    let t: Trace = fixture(include_str!("fixtures/touch/scroll-slow-lift.json"));
    let report = t.replay(&mut c, 0).unwrap();
    let release = report.releases[0].velocity.unwrap()[0];
    assert!(release > 0. && release < 0.15, "{report:?}");
    assert!(report.final_view.offset.unwrap()[0] < 45., "{report:?}");
    assert!(original.root().children.len() == c.scene().unwrap().root().children.len());
}
#[test]
fn long_document_repeated_strokes_exceed_single_stroke_speed_with_bounded_loading() {
    let mut c = client(document::manifest(), document::ui(&[]), &[]);
    let t: Trace = fixture(include_str!("fixtures/touch/document-repeat.json"));
    let report = t.replay(&mut c, 0).unwrap();
    assert!(
        (report.releases[0].velocity.unwrap()[1] - 4.).abs() < 0.01,
        "{report:?}"
    );
    assert!(
        report.releases.last().unwrap().velocity.unwrap()[1] > 8.,
        "{report:?}"
    );
    assert!(report.releases.iter().all(|r| !r.loading));
    assert!(report.final_view.offset.unwrap()[1] > 2000.);
    assert!(
        c.resource_usage().text_document.unwrap().working_bytes
            <= otrios_core::canvas::document::CPU_WORKING_BYTES
    );
}
#[test]
fn slider_pad_and_dial_replay_filter_lift_without_inertial_value_changes() {
    for batch in [0, 16, 64] {
        let (mut c, t) = control_fixture(include_str!("fixtures/touch/slider-lift.json"));
        let report = t.replay(&mut c, batch).unwrap();
        assert!(
            (i32::from(c.value(1).unwrap()) - 700).abs() < 20,
            "batch={batch} {report:?}"
        );
        let (mut c, t) = control_fixture(include_str!("fixtures/touch/pad-lift.json"));
        let report = t.replay(&mut c, batch).unwrap();
        assert!(
            (i32::from(c.value(3).unwrap()) - 700).abs() < 20,
            "{report:?}"
        );
        assert!(
            (i32::from(c.value(4).unwrap()) - 300).abs() < 20,
            "{report:?}"
        );
        let (mut c, t) = control_fixture(include_str!("fixtures/touch/dial-lift.json"));
        let report = t.replay(&mut c, batch).unwrap();
        assert!(
            (i32::from(c.value(2).unwrap()) - 375).abs() < 25,
            "{report:?}"
        );
    }
}
#[test]
fn malformed_trace_fails_before_mutating_the_client() {
    let mut c = client(scrolling::manifest(), scrolling::ui(&[]), &[]);
    let mut t = trace(
        "invalid",
        vec![point(5, 0, [900., 500.]), point(4, 2, [800., 500.])],
    );
    assert!(t.replay(&mut c, 0).is_err());
    assert_eq!(c.text_scroll_info().unwrap().offset, [0., 0.]);
    t.points[1].at_ms = 10;
    t.points[1].position[0] = f32::NAN;
    assert!(t.validate().is_err());
    t.points[1].position = [800., 500.];
    t.points[1].phase = 1;
    assert!(t.validate().is_err());
}

fn control_fixture(json: &str) -> (Client, Trace) {
    let code = wat::parse_str(include_str!(
        "../../../apps/component-playground/behavior/main.wat"
    ))
    .unwrap();
    (
        client(playground::manifest(), playground::ui(&code), &code),
        fixture(json),
    )
}
#[test]
fn paused_capture_replays_without_lift_off_motion() {
    for batch in [0, 16, 64] {
        let mut c = client(scrolling::manifest(), scrolling::ui(&[]), &[]);
        let t: Trace = fixture(include_str!("fixtures/touch/scroll-paused-lift.json"));
        let r = t.replay(&mut c, batch).unwrap();
        assert_eq!(r.releases[0].velocity.unwrap(), [0., 0.]);
        assert!((r.final_view.offset.unwrap()[0] - 400.).abs() < 0.01);
    }
}

fn fixture(json: &str) -> Trace {
    let mut trace: Trace = serde_json::from_str(json).unwrap();
    // Unit fixtures mount one real app at /test. The CLI uses the complete
    // shipped catalog and checks the original recorded route before replay.
    trace.screen = "test".into();
    trace
}

#[test]
fn batched_repeated_strokes_keep_the_same_launch_and_reading_anchor() {
    let t = fixture(include_str!("fixtures/touch/document-repeat.json"));
    let mut speeds = vec![];
    for batch in [0, 16, 64] {
        let mut c = client(document::manifest(), document::ui(&[]), &[]);
        let r = t.replay(&mut c, batch).unwrap();
        speeds.push(r.releases.last().unwrap().velocity.unwrap()[1]);
        let before = c.resource_usage().text_document.unwrap().anchor.unwrap();
        c.set_viewport([800., 1280.]).unwrap();
        for i in 1..1000 {
            c.tick(r.final_view.at_ms + i * 16, 0).unwrap();
            if !c.resource_usage().text_document.unwrap().loading {
                break;
            }
        }
        let after = c.resource_usage().text_document.unwrap().anchor.unwrap();
        assert_eq!(after.block, before.block);
        assert!(
            (after.viewport_y - before.viewport_y).abs() < 1e-7,
            "{before:?} {after:?}"
        );
    }
    assert!(
        speeds.iter().all(|v| (v - speeds[0]).abs() < 1e-9),
        "{speeds:?}"
    );
}
#[test]
fn captured_device_coordinates_use_the_same_runtime_calibration_mapping() {
    let (mut c, mut t) = control_fixture(include_str!("fixtures/touch/slider-lift.json"));
    for p in &mut t.points {
        p.position = [1. - p.position[1] / 800., p.position[0] / 1280.];
    }
    t.mapping = Some(otrios_core::canvas::touch_trace::Mapping {
        rotation: 90,
        surface: [1280, 800],
        calibration: [1., 0., 0., 0., 1., 0.],
    });
    t.replay(&mut c, 16).unwrap();
    assert!((i32::from(c.value(1).unwrap()) - 700).abs() < 20);
}

#[test]
fn recorded_evdev_strokes_replay_with_identical_launch_across_delivery_batches() {
    let t = fixture(include_str!("fixtures/touch/frame-repeat-capture.json"));
    let mut expected: Option<Vec<f64>> = None;
    for batch in [0, 16, 64] {
        let mut c = client(document::manifest(), document::ui(&[]), &[]);
        let r = t.replay(&mut c, batch).unwrap();
        let launches: Vec<_> = r.releases.iter().map(|v| v.velocity.unwrap()[1]).collect();
        assert!(launches.iter().any(|v| *v > 4.));
        assert!(r.releases.iter().all(|v| !v.loading));
        if let Some(previous) = &expected {
            for (a, b) in launches.iter().zip(previous) {
                assert!((a - b).abs() < 1e-8);
            }
        }
        expected = Some(launches);
    }
}
