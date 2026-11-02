// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../apps/live-ui/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::*;
use otrios_interfaces::{canvas::*, layout::Value};

fn setup() -> (Ui, Document) {
    assert_eq!(app::manifest().id, "live-ui");
    let ui = app::ui(&[]);
    let mut doc = Document::new(ui.state_schema.clone(), "oib:test").unwrap();
    assert!(doc.configure_oib(&ui).unwrap());
    (ui, doc)
}
fn compact_changes() -> Vec<(u64, Option<Value>)> {
    let mut changes: Vec<_> = [
        (104, 10.),
        (105, 128.),
        (106, 144.),
        (107, 0.),
        (108, 288.),
        (109, 10.),
        (110, 76.),
        (111, 0.),
        (112, 64.),
    ]
    .into_iter()
    .map(|(id, v)| (id, Some(Value::Number(v))))
    .collect();
    changes.extend([
        (113, Some(Value::Boolean(false))),
        (200, Some(Value::Text("volume".into()))),
        (300, Some(Value::Text("brightness".into()))),
    ]);
    changes
}

#[test]
fn live_geometry_batches_compact_rows_preserve_values_and_style_new_instances() {
    use otrios_interfaces::scene::TouchResponder;
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    client.inspect_oib(30).unwrap();
    let epoch = client.epoch();
    let panel = client.resource_usage().panel_scroll.unwrap().offset;
    admin.edit_oib(&compact_changes()).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(client.epoch(), epoch);
    assert_eq!(client.resource_usage().panel_scroll.unwrap().offset, panel);
    assert_eq!(
        client.resource_usage().oib.unwrap().selected_instance,
        Some(30)
    );
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(400.));
    assert_eq!(doc.typed_value(2).unwrap(), Value::Number(700.));
    let scene = client.scene().unwrap();
    let controls = scene.controls();
    let tracks: Vec<_> = controls
        .iter()
        .filter(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .collect();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].node.frame.width, 288.);
    assert_eq!(tracks[1].origin[1] - tracks[0].origin[1], 64.);
    assert_eq!(tracks[0].origin, [192., 140.]);
    let label = controls
        .iter()
        .find(|p| {
            p.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Inspect volume")
        })
        .unwrap();
    assert_eq!(label.origin, [48., 150.]);
    assert_eq!(label.node.frame.width, 128.);
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    for (destination, expected) in [(0., 0.), (1000., 1000.)] {
        for (phase, x) in [
            (PointerPhase::Down, 330.),
            (PointerPhase::Move, destination),
            (PointerPhase::Up, destination),
        ] {
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 72,
                        phase,
                        position: [x, 164.],
                    },
                    300,
                )
                .unwrap();
        }
        pump_client(&mut client, &mut doc, &mut session);
        assert_eq!(doc.typed_value(1).unwrap(), Value::Number(expected));
    }
    admin.duplicate_oib(20, Some("copy".into())).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    let scene = client.scene().unwrap();
    let tracks: Vec<_> = scene
        .controls()
        .into_iter()
        .filter(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .collect();
    assert_eq!(tracks.len(), 3);
    assert!(tracks.iter().all(|p| p.node.frame.width == 288.));
    assert_eq!(tracks[2].origin[1] - tracks[1].origin[1], 64.);
    let root = doc.oib_status().unwrap().active_root;
    for (id, value) in [(112, 32.), (106, 900.), (108, 20.)] {
        admin.edit_oib(&[(id, Some(Value::Number(value)))]).unwrap();
        sync(&mut doc, &mut sa, &mut admin);
        assert!(doc.oib_status().unwrap().error.is_some());
        assert_eq!(doc.oib_status().unwrap().active_root, root);
        admin.edit_oib(&compact_changes()).unwrap();
        sync(&mut doc, &mut sa, &mut admin);
        assert!(doc.oib_status().unwrap().error.is_none());
    }
    let saved = doc.save().unwrap();
    let mut restored = Document::load(ui.state_schema.clone(), "oib:test", &saved).unwrap();
    assert!(!restored.configure_oib(&ui).unwrap());
    assert_eq!(restored.oib_status().unwrap().active_root, root);
}

#[test]
fn live_geometry_activation_waits_for_a_held_slider_without_remounting() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let track = scene
        .controls()
        .into_iter()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let point = [track.origin[0] + 200., track.origin[1] + 24.];
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 91,
                phase: PointerPhase::Down,
                position: point,
            },
            100,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    let old = client.resource_usage().oib.unwrap().presented_generation;
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 9, true);
    admin.edit_oib(&compact_changes()).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(
        client.resource_usage().oib.unwrap().presented_generation,
        old
    );
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 91,
                phase: PointerPhase::Up,
                position: point,
            },
            200,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    let info = client.resource_usage().oib.unwrap();
    assert_eq!(info.active_generation, info.presented_generation);
    assert!(info.presented_generation > old);
    assert_eq!(client.epoch(), epoch);
}

#[test]
fn identical_layout_seed_presentation_updates_keep_active_roots() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    admin.edit_oib(&compact_changes()).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let root = doc.oib_status().unwrap().active_root;
    let mut next = ui.clone();
    next.title = "Presentation update".into();
    assert!(doc.configure_oib(&next).unwrap());
    assert_eq!(doc.oib_status().unwrap().active_root, root);
}
fn adapter(ui: &Ui, doc: &mut Document, actor: u8, edit: bool) -> (StateAdapter, Session) {
    let (mut session, _) = doc.open_session(&[actor; 16], 0).unwrap();
    session.set_oib_editable(edit);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(ui, "oib:test", &[], [actor; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(doc, &mut session, &mut client);
    (client, session)
}
fn sync(doc: &mut Document, session: &mut Session, client: &mut StateAdapter) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = client.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            client.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("OIB sync did not settle");
}
fn issue(client: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = client.generate().unwrap() {
        if matches!(
            decode_state_request(&bytes).unwrap(),
            StateRequest::OibWrite { .. }
        ) {
            return bytes;
        }
    }
    panic!("Missing OIB operation");
}
#[test]
fn inherited_properties_overrides_and_causal_clear_survive_checkpoint_and_journal() {
    let (ui, mut doc) = setup();
    let before = doc.clone();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    let accent = Value::Color([255, 136, 85, 255]);
    assert!(a.edit_oib(&[(101, Some(accent.clone()))]).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    let seed = oib::seed(&ui).unwrap();
    for id in [201, 301] {
        assert_eq!(
            oib::effective(seed, &b.oib_active().unwrap(), id).unwrap(),
            accent
        );
    }
    assert!(
        b.edit_oib(&[(201, Some(Value::Color([1, 2, 3, 255])))])
            .unwrap()
    );
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    assert_ne!(
        oib::effective(seed, &a.oib_active().unwrap(), 201).unwrap(),
        accent
    );
    assert_eq!(
        oib::effective(seed, &a.oib_active().unwrap(), 301).unwrap(),
        accent
    );
    assert!(a.edit_oib(&[(201, None)]).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(
        oib::effective(seed, &a.oib_active().unwrap(), 201).unwrap(),
        accent
    );
    let mut restored =
        Document::load(ui.state_schema.clone(), "oib:test", &doc.save().unwrap()).unwrap();
    assert!(!restored.configure_oib(&ui).unwrap());
    assert_eq!(restored.oib_values(2).unwrap(), doc.oib_values(2).unwrap());
    let mut replayed = before.clone();
    replayed
        .replay(&doc.journal_delta(&before).unwrap())
        .unwrap();
    assert_eq!(replayed.root_hash(), doc.root_hash());
    assert_eq!(
        before.oib_values(2).unwrap()[&101],
        Some(Value::Color([85, 136, 255, 255]))
    );
    assert!(restored.portable_checkpoint().is_err());
}
#[test]
fn concurrent_values_remain_draft_conflicts_until_an_observing_resolution() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.edit_oib(&[(100, Some(Value::Text("Alice".into())))])
        .unwrap();
    b.edit_oib(&[(100, Some(Value::Text("Bob".into())))])
        .unwrap();
    let a_bytes = issue(&mut a);
    let b_bytes = issue(&mut b);
    doc.receive(&mut sa, &a_bytes).unwrap();
    let active = doc.oib_values(2).unwrap();
    let generation = doc.oib_status().unwrap().generation;
    doc.receive(&mut sb, &b_bytes).unwrap();
    assert_eq!(doc.oib_status().unwrap().conflicts, vec![100]);
    assert_eq!(doc.oib_values(2).unwrap(), active);
    assert_eq!(doc.oib_status().unwrap().generation, generation);
    assert!(
        !doc.receive(&mut sb, &b_bytes).unwrap(),
        "duplicate receipt"
    );
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    assert_eq!(a.oib_conflicts(), vec![100]);
    b.edit_oib(&[(100, Some(Value::Text("Resolved".into())))])
        .unwrap();
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    assert!(a.oib_conflicts().is_empty());
    assert_eq!(
        a.oib_active().unwrap()[&100],
        Some(Value::Text("Resolved".into()))
    );
    assert_eq!(a.oib_active().unwrap(), b.oib_active().unwrap());
}
#[test]
fn disjoint_changes_merge_and_clear_conflicts_with_a_concurrent_override() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.edit_oib(&[(101, Some(Value::Color([1, 2, 3, 255])))])
        .unwrap();
    b.edit_oib(&[(100, Some(Value::Text("Level B".into())))])
        .unwrap();
    doc.receive(&mut sa, &issue(&mut a)).unwrap();
    doc.receive(&mut sb, &issue(&mut b)).unwrap();
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    a.edit_oib(&[(201, None)]).unwrap();
    b.edit_oib(&[(201, Some(Value::Color([9, 8, 7, 255])))])
        .unwrap();
    doc.receive(&mut sa, &issue(&mut a)).unwrap();
    doc.receive(&mut sb, &issue(&mut b)).unwrap();
    assert_eq!(doc.oib_status().unwrap().conflicts, vec![201]);
}
#[test]
fn invalid_and_unauthorized_edits_keep_the_active_ui_and_receipts_atomic() {
    let (ui, mut doc) = setup();
    let (mut member, mut sm) = adapter(&ui, &mut doc, 1, false);
    // The viewer never receives draft leaves, and the authority still rejects forged writes.
    assert!(
        member
            .edit_oib(&[(100, Some(Value::Text("forged".into())))])
            .is_err()
    );
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    admin
        .edit_oib(&[(100, Some(Value::Text("forged".into())))])
        .unwrap();
    let request = issue(&mut admin);
    let before = doc.root_hash();
    assert!(doc.receive(&mut sm, &request).is_err());
    assert_eq!(doc.root_hash(), before);
    doc.receive(&mut sa, &request).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let active = doc.oib_values(2).unwrap();
    let generation = admin.oib_generation();
    admin.edit_oib(&[(102, Some(Value::Number(1.)))]).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_some());
    assert_eq!(doc.oib_values(2).unwrap(), active);
    assert_eq!(admin.oib_generation(), generation);
    admin.edit_oib(&[(102, Some(Value::Number(400.)))]).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
    let before = doc.root_hash();
    assert!(admin.edit_oib(&[(100, Some(Value::Number(1.)))]).is_err());
    assert!(
        admin
            .edit_oib(&[
                (100, Some(Value::Text("one".into()))),
                (100, Some(Value::Text("two".into()))),
            ])
            .is_err()
    );
    assert!(
        admin.confirmed(),
        "Rejected local intents cannot issue a flight"
    );
    assert!(
        admin
            .edit_oib(&[(9999, Some(Value::Text("unknown".into())))])
            .is_err()
    );
    assert_eq!(doc.root_hash(), before);
    let mut changed = ui.clone();
    if let Some(Presentation::Oib(seed)) = &mut changed.presentation {
        seed.cells[0].default = Value::Text("Wrong seed".into());
    }
    assert!(doc.configure_oib(&changed).is_err());
}
#[test]
fn observed_register_join_converges_without_erasing_concurrent_assignments() {
    use otrios_core::merkle::{Edit, Snapshot};
    let initial = Snapshot::empty()
        .edit(
            [0; 16],
            &[Edit {
                path: vec![1],
                schema: [7; 32],
                value: Some(vec![0]),
            }],
        )
        .unwrap()
        .0;
    let observed = initial.subtree(&[1]).unwrap();
    let a = initial
        .edit_observed(
            [1; 16],
            &Edit {
                path: vec![1],
                schema: [7; 32],
                value: Some(vec![1]),
            },
            &observed,
        )
        .unwrap();
    let b = initial
        .edit_observed(
            [2; 16],
            &Edit {
                path: vec![1],
                schema: [7; 32],
                value: None,
            },
            &observed,
        )
        .unwrap();
    let merged = a.merge(&b).unwrap().0;
    assert_eq!(merged.hash(), b.merge(&a).unwrap().0.hash());
    assert_eq!(merged.hash(), merged.merge(&merged).unwrap().0.hash());
    assert_eq!(
        merged
            .subtree(&[1])
            .unwrap()
            .register()
            .unwrap()
            .values()
            .count(),
        2
    );
    assert_eq!(
        merged.merge(&initial).unwrap().0.hash(),
        initial.merge(&merged).unwrap().0.hash()
    );
}

fn mounted_client(ui: &Ui, doc: &mut Document) -> (Client, Session) {
    let mut client = Client::default();
    let bytes = encode_ui(ui).unwrap();
    let mut app = app::manifest();
    app.state_ref = "oib:test".into();
    client
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app,
                    path: "live-ui".into(),
                    ui_hash: hash(&bytes),
                    private: false,
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client.open(&ui.ui_ref, "oib:test");
    let ClientMessage::Mount { epoch, writer, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount")
    };
    let (mut session, _) = doc.open_session(&writer, 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui: bytes.clone(),
                ui_hash: hash(&bytes),
                code: vec![],
                state: doc.generate(&mut session).unwrap().unwrap(),
                binding: None,
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::AcceptMount { .. }
    ));
    pump_client(&mut client, doc, &mut session);
    (client, session)
}
fn pump_client(client: &mut Client, doc: &mut Document, session: &mut Session) {
    for i in 0..30 {
        let mut progress = false;
        client.tick(1000 + i * 33, 0).unwrap();
        while let Some(bytes) = client.outgoing().unwrap() {
            if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes).unwrap() {
                doc.receive(session, &bytes).unwrap();
                progress = true;
            }
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            client
                .receive(
                    &encode_server(&ServerMessage::Sync {
                        epoch: client.epoch(),
                        bytes,
                    })
                    .unwrap(),
                )
                .unwrap();
            progress = true;
        }
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("Client did not settle");
}
#[test]
fn shared_scene_edits_keep_mount_identity_and_defer_geometry_until_slider_release() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, Primitive, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = mounted_client(&ui, &mut doc);
    let (mut b, mut sb) = mounted_client(&ui, &mut doc);
    let epoch = a.epoch();
    let scene = a.scene().unwrap();
    let slider = scene
        .controls()
        .into_iter()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let id = slider.node.id;
    let position = [slider.origin[0] + 40., slider.origin[1] + 20.];
    a.pointer(
        epoch,
        PointerEvent {
            id: 1,
            phase: PointerPhase::Down,
            position,
        },
    )
    .unwrap();
    b.edit_oib(&[
        (102, Some(Value::Number(400.))),
        (100, Some(Value::Text("New label".into()))),
    ])
    .unwrap();
    pump_client(&mut b, &mut doc, &mut sb);
    pump_client(&mut a, &mut doc, &mut sa);
    let info = a.resource_usage().oib.unwrap();
    assert!(info.active_generation > info.presented_generation);
    assert_eq!(
        a.scene()
            .unwrap()
            .path(id)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        520.
    );
    a.pointer(
        epoch,
        PointerEvent {
            id: 1,
            phase: PointerPhase::Up,
            position,
        },
    )
    .unwrap();
    pump_client(&mut a, &mut doc, &mut sa);
    let info = a.resource_usage().oib.unwrap();
    assert_eq!(info.active_generation, info.presented_generation);
    assert_eq!(a.epoch(), epoch);
    assert_eq!(
        a.scene()
            .unwrap()
            .path(id)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        400.
    );
    let has_label = |scene: &otrios_core::dashboard::scene::Scene| {
        fn find(n: &otrios_interfaces::scene::SceneNode) -> bool {
            matches!(&n.primitive,Primitive::Text {value,..} if value=="New label")
                || n.children.iter().any(find)
        }
        find(scene.root())
    };
    assert!(has_label(&a.scene().unwrap()));
    assert!(has_label(&b.scene().unwrap()));
    assert_eq!(a.typed_value(1).unwrap(), doc.typed_value(1).unwrap());
    assert_eq!(a.typed_value(2).unwrap(), Value::Number(700.));
}
#[test]
fn inspector_scroll_is_local_and_visible_buttons_remain_clickable() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let (ui, mut doc) = setup();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    let epoch = client.epoch();
    let generation = client.resource_usage().oib.unwrap().active_generation;
    let scene = client.scene().unwrap();
    let button = scene
        .controls()
        .into_iter()
        .find(|p| {
            p.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Component: LabeledSlider Accent change")
        })
        .unwrap();
    let position = [button.origin[0] + 20., button.origin[1] + 20.];
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        client
            .pointer(
                epoch,
                PointerEvent {
                    id: 9,
                    phase,
                    position,
                },
            )
            .unwrap();
    }
    pump_client(&mut client, &mut doc, &mut session);
    assert!(client.resource_usage().oib.unwrap().active_generation > generation);
    let before = doc.root_hash();
    assert!(client.scroll_map(epoch, 64.).unwrap());
    assert_eq!(doc.root_hash(), before);
    for (phase, y) in [
        (PointerPhase::Down, 550.),
        (PointerPhase::Move, 270.),
        (PointerPhase::Up, 270.),
    ] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 10,
                    phase,
                    position: [1200., y],
                },
                2_000_000_000,
            )
            .unwrap();
    }
    assert!(client.resource_usage().panel_scroll.unwrap().offset[1] > 0.);
    assert_eq!(doc.root_hash(), before, "panning cannot mutate shared UI");
    client.set_viewport([800., 1280.]).unwrap();
    assert_eq!(client.epoch(), epoch);
    assert!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .all(|p| p.origin[1] >= 0.)
    );
}

#[test]
fn inspector_panning_retains_uploaded_content_including_initially_hidden_buttons() {
    use std::sync::Arc;
    let (ui, mut doc) = setup();
    let (mut client, _) = mounted_client(&ui, &mut doc);
    assert!(client.inspect_oib(30).unwrap());
    client.scroll_map(client.epoch(), -10_000.).unwrap();
    let font = otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut composer = presentation::Composer::default();
    let before = composer.compose(&mut client, &font).unwrap().clone();
    let vertices = before.base.mesh.vertices.clone();
    let glyphs = before.base.mesh.glyph_vertices.clone();
    let info = client.resource_usage().oib.unwrap();
    for _ in 0..50 {
        client.scroll_map(client.epoch(), 20.).unwrap();
        let frame = composer.compose(&mut client, &font).unwrap();
        assert!(
            Arc::ptr_eq(&before.base.mesh, &frame.base.mesh),
            "Panning must only update placements and clips"
        );
        assert_eq!(before.revisions.content, frame.revisions.content);
        assert!(Arc::ptr_eq(
            &before.overlay.as_ref().unwrap().mesh,
            &frame.overlay.as_ref().unwrap().mesh
        ));
        assert_eq!(
            client.resource_usage().oib.unwrap().properties,
            info.properties
        );
    }
    let after = composer.frame().unwrap();
    assert_ne!(
        before.overlay.as_ref().unwrap().views,
        after.overlay.as_ref().unwrap().views
    );
    assert_eq!(
        before.base.mesh.vertices, vertices,
        "Pinned frames stay immutable"
    );
    assert_eq!(before.base.mesh.glyph_vertices, glyphs);
    // At the lower bound the previously hidden brightness controls are drawn,
    // clipped and interactive, without another content generation.
    let scene = client.scene().unwrap();
    assert!(scene.controls().iter().any(|c| {
        c.node
            .semantics
            .as_ref()
            .is_some_and(|s| s.label == "Instance: brightness Spacing change")
    }));
    assert!(
        after
            .base
            .views
            .chunks_exact(8)
            .any(|v| v[1] > 0. && v[6] > v[4] && v[7] > v[5])
    );
}

#[test]
fn slider_feedback_keeps_inspector_content_and_saving_status_unchanged() {
    use std::sync::Arc;
    let (ui, mut doc) = setup();
    let (mut client, _) = mounted_client(&ui, &mut doc);
    assert!(client.inspect_oib(30).unwrap());
    client.scroll_map(client.epoch(), -10_000.).unwrap();
    let font = otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut composer = presentation::Composer::default();
    let before = composer.compose(&mut client, &font).unwrap().clone();
    let status = client.resource_usage().oib.unwrap().status;
    let control = ui
        .handlers
        .iter()
        .find(|h| h.fields == [1])
        .unwrap()
        .control;
    client.event(client.epoch(), control, 650, 0).unwrap();
    let after = composer.compose(&mut client, &font).unwrap();
    assert!(!Arc::ptr_eq(&before.base.mesh, &after.base.mesh));
    assert!(Arc::ptr_eq(
        &before.overlay.as_ref().unwrap().mesh,
        &after.overlay.as_ref().unwrap().mesh
    ));
    assert_eq!(client.resource_usage().oib.unwrap().status, status);
    assert_eq!(client.typed_value(1).unwrap(), Value::Number(650.));
}

#[test]
fn slider_captures_reach_both_endpoints_even_outside_the_track() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    for viewport in [[1280., 800.], [800., 1280.]] {
        let (ui, mut doc) = setup();
        let (mut client, mut session) = mounted_client(&ui, &mut doc);
        client.set_viewport(viewport).unwrap();
        for field in [1, 2] {
            let control = ui
                .handlers
                .iter()
                .find(|h| h.fields == [field])
                .unwrap()
                .control;
            let scene = client.scene().unwrap();
            let track = scene
                .controls()
                .into_iter()
                .find(|c| {
                    c.node.responder
                        == Some(TouchResponder::Drag(
                            otrios_interfaces::scene::Action::AppEvent(control),
                        ))
                })
                .unwrap();
            let origin = track.origin;
            let width = track.node.frame.width * track.scale;
            let y = origin[1] + track.node.frame.height * track.scale / 2.;
            let epoch = client.epoch();
            let mut at = 20_000;
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 11,
                        phase: PointerPhase::Down,
                        position: [origin[0] + width / 2., y],
                    },
                    at,
                )
                .unwrap();
            for (fraction, expected) in [
                (0.15, 150.),
                (0.05, 50.),
                (0., 0.),
                (-0.05, 0.),
                (1., 1000.),
                (1.05, 1000.),
            ] {
                at += 50;
                client
                    .pointer_at(
                        epoch,
                        PointerEvent {
                            id: 11,
                            phase: PointerPhase::Move,
                            position: [origin[0] + width * fraction, y],
                        },
                        at,
                    )
                    .unwrap();
                assert_eq!(client.typed_value(field).unwrap(), Value::Number(expected));
                client.scene().unwrap();
            }
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 11,
                        phase: PointerPhase::Up,
                        position: [origin[0] + width * 1.05, y],
                    },
                    at + 100,
                )
                .unwrap();
            pump_client(&mut client, &mut doc, &mut session);
            assert_eq!(doc.typed_value(field).unwrap(), Value::Number(1000.));
        }
    }
}

fn issue_instance(client: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = client.generate().unwrap() {
        if matches!(
            decode_state_request(&bytes).unwrap(),
            StateRequest::OibInstance { .. }
        ) {
            return bytes;
        }
    }
    panic!("Missing instance intent")
}
#[test]
fn instance_creation_duplication_inheritance_and_removal_share_typed_operations() {
    use otrios_interfaces::oib::InstanceAction;
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    let before = doc.clone();
    assert!(a.instantiate_oib(10, Some("gain".into())).unwrap());
    let request = issue_instance(&mut a);
    doc.receive(&mut sa, &request).unwrap();
    assert!(
        !doc.receive(&mut sa, &request).unwrap(),
        "retry cannot allocate a second object"
    );
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let allocations = doc.oib_allocations().unwrap();
    let gain = allocations[2].id;
    assert_eq!(allocations[2].name, "gain");
    let seed = oib::seed(&ui).unwrap();
    let label = oib::instances::cell_id(seed, &allocations, gain, 100).unwrap();
    let accent = oib::instances::cell_id(seed, &allocations, gain, 101).unwrap();
    b.edit_oib_instance_property(gain, "label", Some(Value::Text("Gain".into())))
        .unwrap();
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    a.edit_oib(&[(101, Some(Value::Color([1, 2, 3, 255])))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    let expanded = oib::instances::expanded_seed(seed, &allocations).unwrap();
    assert_eq!(
        oib::effective(&expanded, &b.oib_active().unwrap(), accent).unwrap(),
        Value::Color([1, 2, 3, 255])
    );
    assert_eq!(
        oib::effective(&expanded, &b.oib_active().unwrap(), label).unwrap(),
        Value::Text("Gain".into())
    );
    assert!(
        a.oib_instance_action(gain, InstanceAction::Duplicate)
            .unwrap()
    );
    sync(&mut doc, &mut sa, &mut a);
    let all = doc.oib_allocations().unwrap();
    let copy = all[3].id;
    let copied_label = oib::instances::cell_id(seed, &all, copy, 100).unwrap();
    assert_eq!(
        doc.oib_values(2).unwrap()[&copied_label],
        Some(Value::Text("Gain".into()))
    );
    let (resolved, _) = doc.oib_compiled(2).unwrap();
    assert_eq!(
        resolved
            .handlers
            .iter()
            .filter(|h| h.kind == GestureKind::Slider)
            .count(),
        4
    );
    assert!(a.remove_oib(gain).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    assert!(!doc.oib_live(2).unwrap().contains(&gain));
    assert!(doc.oib_live(2).unwrap().contains(&copy));
    assert_eq!(
        doc.oib_allocations().unwrap().len(),
        4,
        "deleted allocation is retained"
    );
    let mut restored =
        Document::load(ui.state_schema.clone(), "oib:test", &doc.save().unwrap()).unwrap();
    assert!(!restored.configure_oib(&ui).unwrap());
    assert_eq!(restored.oib_live(2).unwrap(), doc.oib_live(2).unwrap());
    let mut replayed = before;
    replayed
        .replay(&doc.journal_delta(&replayed).unwrap())
        .unwrap();
    assert_eq!(replayed.root_hash(), doc.root_hash());
}
#[test]
fn removing_an_instance_wins_over_a_concurrent_property_edit_without_reusing_ids() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.remove_oib(20).unwrap();
    b.edit_oib(&[(200, Some(Value::Text("stale edit".into())))])
        .unwrap();
    let remove = issue_instance(&mut a);
    let edit = issue(&mut b);
    doc.receive(&mut sa, &remove).unwrap();
    doc.receive(&mut sb, &edit).unwrap();
    assert!(!doc.oib_live(2).unwrap().contains(&20));
    assert!(doc.oib_status().unwrap().error.is_none());
    sync(&mut doc, &mut sa, &mut a);
    a.instantiate_oib(10, Some("replacement".into())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let all = doc.oib_allocations().unwrap();
    assert_ne!(all.last().unwrap().id, 20);
    let (resolved, _) = doc.oib_compiled(2).unwrap();
    let old_control = ui
        .handlers
        .iter()
        .find(|h| h.kind == GestureKind::Slider && h.fields == [1])
        .unwrap()
        .control;
    assert!(!resolved.handlers.iter().any(|h| h.control == old_control));
    assert!(
        resolved
            .handlers
            .iter()
            .any(|h| h.kind == GestureKind::Slider && h.fields == [1])
    );
}
#[test]
fn instance_name_conflicts_retain_the_active_graph_and_can_be_removed() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.instantiate_oib(10, Some("gain".into())).unwrap();
    b.instantiate_oib(10, Some("gain".into())).unwrap();
    doc.receive(&mut sa, &issue_instance(&mut a)).unwrap();
    let active = doc.oib_live(2).unwrap();
    let active_root = doc.oib_status().unwrap().active_root;
    doc.receive(&mut sb, &issue_instance(&mut b)).unwrap();
    assert_eq!(
        doc.oib_status().unwrap().active_root,
        active_root,
        "invalid graph must retain the exact active closure"
    );
    assert!(
        doc.oib_status()
            .unwrap()
            .error
            .unwrap()
            .contains("Duplicate instance name")
    );
    assert_eq!(doc.oib_live(2).unwrap(), active);
    sync(&mut doc, &mut sa, &mut a);
    let pending = doc.oib_allocations().unwrap().last().unwrap().id;
    a.remove_oib(pending).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert!(doc.oib_status().unwrap().error.is_none());
    assert_eq!(doc.oib_live(2).unwrap(), active);
}
#[test]
fn instance_requests_are_authorized_and_budgeted_before_allocation() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (_, mut member) = adapter(&ui, &mut doc, 2, false);
    admin.instantiate_oib(10, None).unwrap();
    let wire = issue_instance(&mut admin);
    let before = doc.root_hash();
    assert!(doc.receive(&mut member, &wire).is_err());
    assert_eq!(doc.root_hash(), before);
    let request = decode_state_request(&wire).unwrap();
    assert_eq!(
        decode_state_request(&encode_state_request(&request).unwrap()).unwrap(),
        request
    );
    doc.receive(&mut sa, &wire).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    while doc.oib_live(2).unwrap().len() < otrios_interfaces::oib::MAX_LIVE_INSTANCES {
        assert!(admin.instantiate_oib(10, None).unwrap());
        sync(&mut doc, &mut sa, &mut admin);
    }
    assert!(!admin.instantiate_oib(10, None).unwrap());
    for id in doc.oib_live(2).unwrap() {
        doc.oib_compiled_selected(2, Some(id)).unwrap();
    }
}

#[test]
fn exact_signed_o1_upgrade_preserves_properties_and_journal_recovery() {
    use sha2::{Digest, Sha256};
    let bytes =
        lz4_flex::decompress_size_prepended(include_bytes!("fixtures/oib-o1-ui.lz4")).unwrap();
    let old = decode_ui(&bytes).unwrap();
    let new = app::ui(&[]);
    assert_eq!(
        Sha256::digest(&bytes).as_slice(),
        oib::seed(&new)
            .unwrap()
            .instance_surface
            .as_ref()
            .unwrap()
            .previous_ui_sha256
    );
    let mut doc = Document::new(old.state_schema.clone(), "oib:test").unwrap();
    doc.configure_oib(&old).unwrap();
    let (mut a, mut sa) = adapter(&old, &mut doc, 1, true);
    a.edit_oib(&[(201, Some(Value::Color([7, 8, 9, 255])))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut a);
    doc.set_typed(&[(1, Value::Number(730.))]).unwrap();
    let checkpoint = doc.save().unwrap();
    let before = doc.clone();
    assert!(doc.configure_oib(&new).unwrap());
    assert_eq!(
        doc.oib_values(2).unwrap()[&201],
        Some(Value::Color([7, 8, 9, 255]))
    );
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(730.));
    let mut restored = Document::load(old.state_schema.clone(), "oib:test", &checkpoint).unwrap();
    restored
        .replay(&doc.journal_delta(&before).unwrap())
        .unwrap();
    assert!(!restored.configure_oib(&new).unwrap());
    assert_eq!(restored.root_hash(), doc.root_hash());
    let mut incompatible = new.clone();
    if let Some(Presentation::Oib(seed)) = &mut incompatible.presentation {
        seed.cells[0].default = Value::Text("changed contract".into());
    }
    let mut original = before;
    assert!(original.configure_oib(&incompatible).is_err());
}

#[test]
fn signed_layout_surface_upgrade_keeps_retained_instances_and_properties() {
    let bytes =
        lz4_flex::decompress_size_prepended(include_bytes!("fixtures/oib-o2-ui.lz4")).unwrap();
    let old = decode_ui(&bytes).unwrap();
    let new = app::ui(&[]);
    assert!(oib::seed(&new).unwrap().cells.len() > oib::seed(&old).unwrap().cells.len());
    assert_ne!(old.layout, new.layout);
    let mut doc = Document::new(old.state_schema.clone(), "oib:test").unwrap();
    doc.configure_oib(&old).unwrap();
    let (mut a, mut sa) = adapter(&old, &mut doc, 1, true);
    for name in ["retained", "removed"] {
        a.instantiate_oib(10, Some(name.into())).unwrap();
        sync(&mut doc, &mut sa, &mut a);
    }
    a.edit_oib_instance_property(65538, "label", Some(Value::Text("Keep".into())))
        .unwrap();
    sync(&mut doc, &mut sa, &mut a);
    a.remove_oib(65539).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    doc.set_typed(&[(1, Value::Number(730.))]).unwrap();
    let checkpoint = doc.save().unwrap();
    let before = doc.clone();
    assert!(doc.configure_oib(&new).unwrap());
    assert_eq!(
        doc.oib_allocations().unwrap(),
        before.oib_allocations().unwrap()
    );
    for branch in [1, 2] {
        assert_eq!(
            doc.oib_live(branch).unwrap(),
            before.oib_live(branch).unwrap()
        );
        let values = doc.oib_values(branch).unwrap();
        for (id, value) in before.oib_values(branch).unwrap() {
            assert_eq!(values[&id], value);
        }
    }
    assert!(doc.oib_values(2).unwrap().contains_key(&112));
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(730.));
    let mut restored = Document::load(old.state_schema.clone(), "oib:test", &checkpoint).unwrap();
    restored
        .replay(&doc.journal_delta(&before).unwrap())
        .unwrap();
    assert!(!restored.configure_oib(&new).unwrap());
    assert_eq!(restored.root_hash(), doc.root_hash());

    let (mut client, mut session) = mounted_client(&new, &mut doc);
    pump_client(&mut client, &mut doc, &mut session);
    let scene = client.scene().unwrap();
    let seed = oib::seed(&new).unwrap();
    let surface = seed.instance_surface.as_ref().unwrap();
    for (viewport, inset) in [
        (surface.preview_viewport, 16.),
        (seed.inspector_viewport, 16.),
    ] {
        let path = scene.path(u32::from(viewport)).unwrap();
        let p = path.last().unwrap();
        assert_eq!(p.origin[1] + p.node.frame.height * p.scale + inset, 776.);
    }
    client.set_viewport([800., 1280.]).unwrap();
    let tall = client.scene().unwrap();
    let path = tall.path(u32::from(seed.inspector_viewport)).unwrap();
    let p = path.last().unwrap();
    assert_eq!(p.origin[1] + p.node.frame.height * p.scale + 16., 1256.);
    let mut incompatible = new.clone();
    if let Some(Presentation::Oib(seed)) = &mut incompatible.presentation {
        seed.cells[0].default = Value::Text("changed contract".into());
    }
    assert!(doc.configure_oib(&incompatible).is_err());
}

#[test]
fn structural_activation_waits_for_a_held_slider_and_preserves_local_inspection() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    assert!(client.inspect_oib(30).unwrap());
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let slider = scene
        .controls()
        .into_iter()
        .find(|c| matches!(c.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let start = [
        slider.origin[0] + slider.node.frame.width * slider.scale / 2.,
        slider.origin[1] + 24. * slider.scale,
    ];
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 21,
                phase: PointerPhase::Down,
                position: start,
            },
            100,
        )
        .unwrap();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 9, true);
    editor.instantiate_oib(10, Some("gain".into())).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    pump_client(&mut client, &mut doc, &mut session);
    let info = client.resource_usage().oib.unwrap();
    assert!(info.active_generation > info.presented_generation);
    assert_eq!(info.selected_instance, Some(30));
    assert_eq!(client.epoch(), epoch);
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 21,
                phase: PointerPhase::Move,
                position: [start[0] + 40., start[1]],
            },
            200,
        )
        .unwrap();
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 21,
                phase: PointerPhase::Up,
                position: [start[0] + 40., start[1]],
            },
            250,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    client.tick(500, 0).unwrap();
    let info = client.resource_usage().oib.unwrap();
    assert_eq!(info.active_generation, info.presented_generation);
    assert_eq!(info.selected_instance, Some(30));
    assert_eq!(client.epoch(), epoch);
    assert!(!client.input_busy());
    assert_eq!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .filter(|c| matches!(c.node.responder, Some(TouchResponder::Drag(_))))
            .count(),
        3,
        "the taller preview exposes the third slider after activation"
    );
}
#[test]
fn local_inspector_selection_and_preview_scrolling_do_not_write_shared_state() {
    let (ui, mut doc) = setup();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 2, true);
    editor.instantiate_oib(10, Some("gain".into())).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    pump_client(&mut client, &mut doc, &mut session);
    let root = doc.root_hash();
    assert!(client.inspect_oib(30).unwrap());
    assert_eq!(doc.root_hash(), root);
    let viewport = oib::seed(&ui)
        .unwrap()
        .instance_surface
        .as_ref()
        .unwrap()
        .preview_viewport;
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(viewport)).unwrap();
    let p = path.last().unwrap();
    let position = [
        p.origin[0] + p.node.frame.width * p.scale / 2.,
        p.origin[1] + p.node.frame.height * p.scale / 2.,
    ];
    let inspector = client.resource_usage().panel_scroll.unwrap().offset;
    assert!(client.scroll_at(client.epoch(), 200., position).unwrap());
    let usage = client.resource_usage();
    assert!(usage.oib_preview_scroll.unwrap().offset[1] > 0.);
    assert_eq!(usage.panel_scroll.unwrap().offset, inspector);
    assert_eq!(usage.oib.unwrap().selected_instance, Some(30));
    assert_eq!(doc.root_hash(), root);
    assert!(client.outgoing().unwrap().is_none());

    // Preview card background has no responder in the viewport, but must start a touch
    // pan. A wheel-only check cannot exercise this routing decision.
    let background = [
        p.origin[0] + (p.node.frame.width - 4.) * p.scale,
        p.origin[1] + (p.node.frame.height - 8.) * p.scale,
    ];
    let background_scene = client.scene().unwrap();
    let hit = background_scene.hit_path(background);
    assert!(!hit.iter().any(|p| p.node.id == u32::from(viewport)));
    assert!(hit.iter().all(|p| p.node.responder.is_none()));
    client.scroll_at(client.epoch(), -1000., position).unwrap();
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    for (phase, y, at) in [
        (PointerPhase::Down, background[1], 100),
        (PointerPhase::Move, background[1] - 100., 200),
        (PointerPhase::Up, background[1] - 100., 220),
    ] {
        assert!(
            client
                .pointer_at(
                    client.epoch(),
                    PointerEvent {
                        id: 71,
                        phase,
                        position: [background[0], y],
                    },
                    at,
                )
                .unwrap()
        );
    }
    let usage = client.resource_usage();
    assert!(usage.oib_preview_scroll.unwrap().offset[1] > 0.);
    assert_eq!(usage.panel_scroll.unwrap().offset, inspector);
    assert_eq!(usage.oib.unwrap().selected_instance, Some(30));
    assert_eq!(doc.root_hash(), root);
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn allocation_history_is_bounded_and_an_empty_preview_can_be_repopulated() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    for id in [20, 30] {
        a.remove_oib(id).unwrap();
        sync(&mut doc, &mut sa, &mut a);
    }
    assert!(doc.oib_live(2).unwrap().is_empty());
    doc.oib_compiled(2).unwrap();
    while doc.oib_allocations().unwrap().len() < otrios_interfaces::oib::MAX_ALLOCATIONS {
        assert!(a.instantiate_oib(10, None).unwrap());
        sync(&mut doc, &mut sa, &mut a);
        assert!(
            doc.oib_status().unwrap().error.is_none(),
            "{} allocations: {:?}",
            doc.oib_allocations().unwrap().len(),
            doc.oib_status().unwrap().error
        );
        let id = doc.oib_allocations().unwrap().last().unwrap().id;
        doc.oib_compiled_selected(2, Some(id)).unwrap();
        a.remove_oib(id).unwrap();
        sync(&mut doc, &mut sa, &mut a);
    }
    let before = doc.root_hash();
    assert!(!a.instantiate_oib(10, None).unwrap());
    assert_eq!(doc.root_hash(), before);
    assert!(doc.oib_live(2).unwrap().is_empty());
    assert_eq!(
        doc.oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        otrios_interfaces::oib::MAX_ALLOCATIONS
    );
}

#[test]
fn reconnect_keeps_local_inspector_selection_and_both_scroll_offsets() {
    let (ui, mut doc) = setup();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 2, true);
    editor.instantiate_oib(10, None).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    pump_client(&mut client, &mut doc, &mut session);
    client.inspect_oib(30).unwrap();
    let surface = oib::seed(&ui).unwrap().instance_surface.as_ref().unwrap();
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(surface.preview_viewport)).unwrap();
    let p = path.last().unwrap();
    client
        .scroll_at(
            client.epoch(),
            150.,
            [p.origin[0] + 40., p.origin[1] + 100.],
        )
        .unwrap();
    let before = client.resource_usage();
    let inspector = before.panel_scroll.unwrap().offset;
    let preview = before.oib_preview_scroll.unwrap().offset;
    client.reset_connection();
    let ClientMessage::Mount {
        epoch,
        writer,
        resume,
        ..
    } = decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount");
    };
    let (mut next, _) = doc.open_session(&writer, resume).unwrap();
    next.set_oib_editable(true);
    next.set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let bytes = encode_ui(&ui).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: doc.generate(&mut next).unwrap().unwrap(),
                binding: None,
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::AcceptMount { .. }
    ));
    pump_client(&mut client, &mut doc, &mut next);
    let after = client.resource_usage();
    assert_eq!(after.oib.unwrap().selected_instance, Some(30));
    assert_eq!(after.panel_scroll.unwrap().offset, inspector);
    assert_eq!(after.oib_preview_scroll.unwrap().offset, preview);
}

#[test]
fn sparse_layout_events_are_versioned_and_keep_the_host_reservation() {
    use otrios_core::dashboard::layout::Layout;
    use otrios_interfaces::layout::{Responder, TemplateNode};
    fn control(root: &mut TemplateNode, value: u8) -> bool {
        if let Some(Responder::AppEvent { control, .. }) = &mut root.responder {
            *control = value;
            return true;
        }
        root.children.iter_mut().any(|c| control(c, value))
    }
    let (mut ui, _) = setup();
    let mut blueprint = Layout::from_bytes(&ui.layout).unwrap().blueprint().clone();
    assert!(control(&mut blueprint.templates[0].root, 200));
    ui.layout = Layout::encode(&blueprint).unwrap();
    assert!(oib::compile(&ui, &Default::default()).is_err());
    blueprint.schema = otrios_interfaces::layout::SPARSE_EVENTS_FORMAT.into();
    ui.layout = Layout::encode(&blueprint).unwrap();
    oib::compile(&ui, &Default::default()).unwrap();
    assert!(control(&mut blueprint.templates[0].root, 255));
    ui.layout = Layout::encode(&blueprint).unwrap();
    assert!(oib::compile(&ui, &Default::default()).is_err());
}
