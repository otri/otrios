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
    let before = std::fs::read(room.root.join("state.apps/demo.automerge")).unwrap();
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
    assert_eq!(
        std::fs::read(room.root.join("state.apps/demo.automerge")).unwrap(),
        before
    );
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
    assert_eq!(
        std::fs::read(room.root.join("state.apps/demo.automerge")).unwrap(),
        before
    );
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
        let bytes = std::fs::read(room.root.join("state.apps/demo.automerge")).unwrap();
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
