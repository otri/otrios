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
            .edit_oib(&[(9999, Some(Value::Text("unknown".into())))])
            .is_err()
    );
    assert_eq!(doc.root_hash(), before);
    let mut changed = ui.clone();
    changed.title = "Wrong seed".into();
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
