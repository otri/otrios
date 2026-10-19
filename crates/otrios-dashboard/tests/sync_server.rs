// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "server")]
mod common;
use common::*;
use otrios_core::canvas::{self, Document, StateAdapter};
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
            writer: vec![9; 16],
            resume: 0,
            binary: false,
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
            writer: vec![9; 16],
            resume: 0,
            binary: false,
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
    let mut doc =
        StateAdapter::new(room.schema.clone(), "app:demo/state", &state, [9; 16]).unwrap();
    doc.set(&[(1, 731)]).unwrap();
    for _ in 0..20 {
        while let Some(bytes) = doc.generate().unwrap() {
            send(&mut s, ClientMessage::Sync { epoch: 1, bytes });
        }
        let ServerMessage::Sync { bytes, .. } = read(&mut s) else {
            panic!("missing sync")
        };
        doc.receive(&bytes).unwrap();
        let saved = stored(&room, room.schema.clone(), "app:demo/state", "demo");
        if saved.value(1).unwrap() == 731 && doc.confirmed() {
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
    doc: &mut StateAdapter,
    epoch: u32,
    value: u16,
) {
    doc.set(&[(1, value)]).unwrap();
    for _ in 0..20 {
        while let Some(bytes) = doc.generate().unwrap() {
            send(socket, ClientMessage::Sync { epoch, bytes });
        }
        if let ServerMessage::Sync { bytes, .. } = read_epoch(socket, epoch) {
            doc.receive(&bytes).unwrap();
        }
        let saved = stored(room, room.schema.clone(), "app:demo/state", "demo");
        if saved.value(1).unwrap() == value && doc.confirmed() {
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
    let mut doc =
        StateAdapter::new(room.schema.clone(), "app:demo/state", &state, [9; 16]).unwrap();
    persisted_edit(&room, &mut s, &mut doc, 1, 100);
    // A valid candidate can be rejected by its host after dependency qualification.
    mount(&mut s, &room, "demo", 2);
    assert!(matches!(
        read_epoch(&mut s, 2),
        ServerMessage::Mounted { .. }
    ));
    // The old activation can still complete edits before a decision.
    persisted_edit(&room, &mut s, &mut doc, 1, 200);
    send(&mut s, ClientMessage::CancelMount { epoch: 2 });
    persisted_edit(&room, &mut s, &mut doc, 1, 300);
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 3,
            ui_ref: "app:demo/ui".into(),
            state_ref: "wrong-root".into(),
            catalog_hash: canvas::hash(&room.catalog),
            known_code: vec![],
            writer: vec![9; 16],
            resume: 0,
            binary: false,
        },
    );
    assert!(matches!(
        read_epoch(&mut s, 3),
        ServerMessage::Error { epoch: 3, .. }
    ));
    persisted_edit(&room, &mut s, &mut doc, 1, 400);
    mount(&mut s, &room, "monitoring", 4);
    assert!(matches!(
        read_epoch(&mut s, 4),
        ServerMessage::AccessRequired { epoch: 4 }
    ));
    persisted_edit(&room, &mut s, &mut doc, 1, 500);
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
    let mut doc =
        StateAdapter::new(room.schema.clone(), "app:demo/state", &state, [9; 16]).unwrap();
    persisted_edit(&room, &mut s, &mut doc, 1, 731);
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
            writer: vec![9; 16],
            resume: 0,
            binary: false,
        },
    );
    let ServerMessage::Mounted { state, .. } = read_epoch(&mut s, 2) else {
        panic!("no new state candidate")
    };
    let fresh = StateAdapter::new(ui.state_schema, "app:demo/state-v2", &state, [9; 16]).unwrap();
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
    while let Some(bytes) = doc.generate().unwrap() {
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
            writer: vec![9; 16],
            resume: 0,
            binary: false,
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

#[test]
fn journal_failure_never_acknowledges_and_pending_intent_survives_restart() {
    let mut room = room(false, 28800);
    let mut s = socket(&room.public, None);
    mount(&mut s, &room, "demo", 1);
    let ServerMessage::Mounted { state, .. } = read_epoch(&mut s, 1) else {
        panic!("no mount")
    };
    let mut client =
        StateAdapter::new(room.schema.clone(), "app:demo/state", &state, [9; 16]).unwrap();
    accept(&mut s, 1);
    let ServerMessage::Sync { bytes, .. } = read_epoch(&mut s, 1) else {
        panic!("no activation")
    };
    client.receive(&bytes).unwrap();
    while let Some(bytes) = client.generate().unwrap() {
        send(&mut s, ClientMessage::Sync { epoch: 1, bytes });
    }
    let file = state_file(&room, "demo");
    let log = file.with_extension("merkle.log");
    let backup = log.with_extension("backup");
    std::fs::rename(&log, &backup).unwrap();
    std::fs::create_dir(&log).unwrap();
    client.set(&[(1, 731)]).unwrap();
    let bytes = client.generate().unwrap().unwrap();
    send(&mut s, ClientMessage::Sync { epoch: 1, bytes });
    assert!(matches!(read_epoch(&mut s, 1), ServerMessage::Error { .. }));
    assert!(!client.confirmed());
    assert_eq!(client.value(1).unwrap(), 731);
    room.child.kill().unwrap();
    room.child.wait().unwrap();
    std::fs::remove_dir(&log).unwrap();
    std::fs::rename(&backup, &log).unwrap();
    room.child = std::process::Command::new(env!("CARGO_BIN_EXE_dashboard-server"))
        .args([
            "--bind",
            &room.private,
            "--public-bind",
            &room.public,
            "--app-state",
            room.root.join("state.apps").to_str().unwrap(),
            "--apps",
            room.root.join("apps").to_str().unwrap(),
            "--origin",
            ORIGIN,
        ])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::net::TcpStream::connect(&room.public).is_err() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        stored(&room, room.schema.clone(), "app:demo/state", "demo")
            .value(1)
            .unwrap(),
        0
    );
    let mut s = socket(&room.public, None);
    send(
        &mut s,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: "app:demo/ui".into(),
            state_ref: "app:demo/state".into(),
            catalog_hash: canvas::hash(&room.catalog),
            known_code: vec![],
            writer: vec![9; 16],
            resume: client.resume_sequence(),
            binary: false,
        },
    );
    let ServerMessage::Mounted { state, .. } = read_epoch(&mut s, 1) else {
        panic!("no resumed mount")
    };
    let mut resumed =
        StateAdapter::new(room.schema.clone(), "app:demo/state", &state, [9; 16]).unwrap();
    resumed.restore_pending(&client).unwrap();
    accept(&mut s, 1);
    persisted_edit(&room, &mut s, &mut resumed, 1, 731);
    assert!(resumed.confirmed());
}

#[test]
fn negotiated_blob_and_legacy_mounts_both_confirm_durable_edits() {
    let room = room(false, 28800);
    for binary in [false, true] {
        let mut socket = socket(&room.public, None);
        send(
            &mut socket,
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: "app:demo/ui".into(),
                state_ref: "app:demo/state".into(),
                catalog_hash: canvas::hash(&room.catalog),
                known_code: vec![],
                writer: vec![if binary { 12 } else { 11 }; 16],
                resume: 0,
                binary,
            },
        );
        let ServerMessage::Mounted { state, .. } = read(&mut socket) else {
            panic!("no mount")
        };
        let mut client = StateAdapter::new(
            room.schema.clone(),
            "app:demo/state",
            &state,
            [if binary { 12 } else { 11 }; 16],
        )
        .unwrap();
        accept(&mut socket, 1);
        persisted_edit(
            &room,
            &mut socket,
            &mut client,
            1,
            if binary { 882 } else { 881 },
        );
        assert!(client.confirmed());
    }
}
