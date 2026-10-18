// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "server")]
mod common;
use common::*;
use otrios_core::canvas::{self, Document, Session};
use otrios_interfaces::canvas::{ClientMessage, ServerMessage};
#[test]
fn retired_global_writer_and_scope_forgery_cannot_mutate_app_roots() {
    let room = room(false, 28800);
    for path in [
        "/stream",
        "/demo/stream",
        "/monitoring/stream",
        "/layout",
        "/sync",
    ] {
        assert_eq!(request(&room.private, "GET", path, &[], &[]).0, 410);
    }
    assert!(!room.root.join("state.automerge").exists());
    let before = std::fs::read(state_file(&room, "demo")).unwrap();
    let mut s = socket(&room.public, None);
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: "app:demo/ui".into(),
            state_ref: "app:monitoring/state".into(),
            catalog_hash: canvas::hash(&room.catalog),
            known_code: vec![],
        },
    );
    assert!(matches!(read(&mut s), ServerMessage::Error { .. }));
    assert_eq!(std::fs::read(state_file(&room, "demo")).unwrap(), before);
    let mut s = socket(&room.public, None);
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: "app:demo/ui".into(),
            state_ref: "app:demo/state".into(),
            catalog_hash: vec![0; 32],
            known_code: vec![],
        },
    );
    assert!(matches!(read(&mut s), ServerMessage::Error { .. }));
    assert_eq!(std::fs::read(state_file(&room, "demo")).unwrap(), before);
}
#[test]
fn public_app_mount_is_atomic_and_accepted_edits_are_persisted_separately() {
    let room = room(false, 28800);
    let mut s = socket(&room.public, None);
    mount(&mut s, &room, "demo", 1);
    let ServerMessage::Mounted {
        state, ui, ui_hash, ..
    } = read(&mut s)
    else {
        panic!("missing staged mount")
    };
    assert_eq!(canvas::hash(&ui), ui_hash);
    accept(&mut s, 1);
    let mut doc = Document::load(room.schema.clone(), "app:demo/state", &state).unwrap();
    doc.set(&[(1, 731)]).unwrap();
    let mut session = Session::default();
    for _ in 0..20 {
        if let Some(bytes) = doc.generate(&mut session).unwrap() {
            send(&mut s, ClientMessage::Sync { epoch: 1, bytes });
        }
        let ServerMessage::Sync { bytes, .. } = read(&mut s) else {
            panic!("missing sync")
        };
        doc.receive_authority(&mut session, &bytes).unwrap();
        let bytes = std::fs::read(state_file(&room, "demo")).unwrap();
        let saved = Document::load(room.schema.clone(), "app:demo/state", &bytes).unwrap();
        if saved.value(1).unwrap() == 731 {
            return;
        }
    }
    panic!("edit was not durably accepted");
}
#[test]
fn immutable_authority_payloads_have_no_static_download_route() {
    let room = room(false, 28800);
    for path in [
        "/authority/apps/monitoring/ui.flex",
        "/apps/monitoring/ui.flex",
        "/apps/demo/package/ui.flex",
    ] {
        assert_eq!(request(&room.private, "GET", path, &[], &[]).0, 404);
    }
    assert_eq!(
        request(&room.private, "GET", "/apps/catalog.flex", &[], &[]).1,
        room.catalog
    );
}

fn read_epoch(socket: &mut Socket, wanted: u32) -> ServerMessage {
    loop {
        let m = read(socket);
        let epoch = match &m {
            ServerMessage::Mounted { epoch, .. }
            | ServerMessage::Sync { epoch, .. }
            | ServerMessage::View { epoch, .. }
            | ServerMessage::Snapshot { epoch, .. }
            | ServerMessage::AccessRequired { epoch }
            | ServerMessage::Error { epoch, .. } => *epoch,
        };
        if epoch == wanted {
            return m;
        }
    }
}
fn persisted_edit(
    room: &Room,
    socket: &mut Socket,
    doc: &mut Document,
    session: &mut Session,
    epoch: u32,
    value: u16,
) {
    doc.set(&[(1, value)]).unwrap();
    for _ in 0..20 {
        if let Some(bytes) = doc.generate(session).unwrap() {
            send(socket, ClientMessage::Sync { epoch, bytes });
        }
        if let ServerMessage::Sync { bytes, .. } = read_epoch(socket, epoch) {
            doc.receive_authority(session, &bytes).unwrap();
        }
        let saved = Document::load(
            room.schema.clone(),
            "app:demo/state",
            &std::fs::read(state_file(room, "demo")).unwrap(),
        )
        .unwrap();
        if saved.value(1).unwrap() == value {
            return;
        }
    }
    panic!("accepted old app edit did not persist");
}
#[test]
fn canceled_and_rejected_candidates_keep_the_previous_lease_writable() {
    let room = room(false, 28800);
    let mut s = socket(&room.public, None);
    mount(&mut s, &room, "demo", 1);
    let ServerMessage::Mounted { state, .. } = read_epoch(&mut s, 1) else {
        panic!("no candidate")
    };
    accept(&mut s, 1);
    let mut doc = Document::load(room.schema.clone(), "app:demo/state", &state).unwrap();
    let mut session = Session::default();
    persisted_edit(&room, &mut s, &mut doc, &mut session, 1, 100);
    // A valid candidate can be rejected by its host after dependency qualification.
    mount(&mut s, &room, "demo", 2);
    assert!(matches!(
        read_epoch(&mut s, 2),
        ServerMessage::Mounted { .. }
    ));
    // The old activation can still complete edits before a decision.
    persisted_edit(&room, &mut s, &mut doc, &mut session, 1, 200);
    send(&mut s, ClientMessage::CancelMount { epoch: 2 });
    persisted_edit(&room, &mut s, &mut doc, &mut session, 1, 300);
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 3,
            ui_ref: "app:demo/ui".into(),
            state_ref: "wrong-root".into(),
            catalog_hash: canvas::hash(&room.catalog),
            known_code: vec![],
        },
    );
    assert!(matches!(
        read_epoch(&mut s, 3),
        ServerMessage::Error { epoch: 3, .. }
    ));
    persisted_edit(&room, &mut s, &mut doc, &mut session, 1, 400);
    mount(&mut s, &room, "monitoring", 4);
    assert!(matches!(
        read_epoch(&mut s, 4),
        ServerMessage::AccessRequired { epoch: 4 }
    ));
    persisted_edit(&room, &mut s, &mut doc, &mut session, 1, 500);
}
#[test]
fn accepting_a_new_state_reference_switches_subscription_and_preserves_both_files() {
    let mut room = room(false, 28800);
    let mut s = socket(&room.public, None);
    mount(&mut s, &room, "demo", 1);
    let ServerMessage::Mounted { state, .. } = read_epoch(&mut s, 1) else {
        panic!("no first candidate")
    };
    accept(&mut s, 1);
    let mut doc = Document::load(room.schema.clone(), "app:demo/state", &state).unwrap();
    let mut session = Session::default();
    persisted_edit(&room, &mut s, &mut doc, &mut session, 1, 731);
    let old_file = state_file(&room, "demo");
    let before = std::fs::read(&old_file).unwrap();
    let mut catalog = canvas::decode_catalog(&room.catalog).unwrap();
    let entry = catalog
        .apps
        .iter_mut()
        .find(|e| e.app.id == "demo")
        .unwrap();
    entry.app.state_ref = "app:demo/state-v2".into();
    let path = room.root.join("apps/demo/package/ui.flex");
    let mut ui = canvas::decode_ui(&std::fs::read(&path).unwrap()).unwrap();
    ui.state_schema.id = "test-demo-v3".into();
    let bytes = canvas::encode_ui(&ui).unwrap();
    entry.ui_hash = canvas::hash(&bytes);
    std::fs::write(path, &bytes).unwrap();
    room.catalog = canvas::encode_catalog(&catalog).unwrap();
    std::fs::write(room.root.join("apps/catalog.flex"), &room.catalog).unwrap();
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 2,
            ui_ref: "app:demo/ui".into(),
            state_ref: "app:demo/state-v2".into(),
            catalog_hash: canvas::hash(&room.catalog),
            known_code: vec![],
        },
    );
    let ServerMessage::Mounted { state, .. } = read_epoch(&mut s, 2) else {
        panic!("no new state candidate")
    };
    let fresh = Document::load(ui.state_schema, "app:demo/state-v2", &state).unwrap();
    assert_eq!(fresh.value(1).unwrap(), 0);
    let new_file = state_file_ref(&room, "demo", "app:demo/state-v2");
    assert_ne!(old_file, new_file);
    assert!(new_file.is_file());
    accept(&mut s, 2);
    assert!(matches!(
        read_epoch(&mut s, 2),
        ServerMessage::Sync { epoch: 2, .. }
    ));
    // A packet from the previous activation cannot change either root after commit.
    doc.set(&[(1, 900)]).unwrap();
    if let Some(bytes) = doc.generate(&mut session).unwrap() {
        send(&mut s, ClientMessage::Sync { epoch: 1, bytes });
    }
    // A fresh mount is an ordered response barrier after the ignored packet.
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 3,
            ui_ref: "app:demo/ui".into(),
            state_ref: "app:demo/state-v2".into(),
            catalog_hash: canvas::hash(&room.catalog),
            known_code: vec![],
        },
    );
    assert!(matches!(
        read_epoch(&mut s, 3),
        ServerMessage::Mounted { epoch: 3, .. }
    ));
    assert_eq!(std::fs::read(old_file).unwrap(), before);
    let saved = Document::load(
        fresh.schema().clone(),
        "app:demo/state-v2",
        &std::fs::read(new_file).unwrap(),
    )
    .unwrap();
    assert_eq!(saved.value(1).unwrap(), 0);
}
