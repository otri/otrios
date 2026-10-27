// SPDX-License-Identifier: Apache-2.0
//! Real authority-process qualification of the host's cookie/auth/socket adapter.
#![cfg(all(feature = "server", feature = "canvas", feature = "host-sync"))]
mod common;
use common::*;
use otrios_core::canvas::chat::{decode_event, encode_command};
use otrios_interfaces::{chat::*, session::frame};
use std::{
    fs,
    process::{Command as Process, Stdio},
    time::{Duration, Instant},
};
use tungstenite::Message as Wire;
fn send_chat(socket: &mut Socket, command: Command) {
    socket
        .send(Wire::Binary(
            frame(CHANNEL, encode_command(&command).unwrap()).into(),
        ))
        .unwrap();
}
fn until(socket: &mut Socket, wanted: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "chat event deadline");
        match socket.read().unwrap() {
            Wire::Binary(bytes) if bytes.first() == Some(&CHANNEL) => {
                let event = decode_event(&bytes[1..]).unwrap();
                if wanted(&event) {
                    return event;
                }
            }
            Wire::Ping(_) => socket.flush().unwrap(),
            Wire::Close(_) => panic!("shared socket unexpectedly closed"),
            _ => {}
        }
    }
}
fn join(socket: &mut Socket, credential: Option<String>) -> (Member, Option<String>, u64) {
    send_chat(socket, Command::Join { credential });
    match until(socket, |e| matches!(e, Event::Welcome { .. })) {
        Event::Welcome {
            you,
            credential,
            epoch,
            ..
        } => (you, credential, epoch),
        _ => unreachable!(),
    }
}
fn restart(room: &mut Room) {
    if room.child.try_wait().unwrap().is_none() {
        room.child.kill().unwrap();
        room.child.wait().unwrap();
    }
    room.child = Process::new(env!("CARGO_BIN_EXE_dashboard-server"))
        .args([
            "--bind",
            &room.private,
            "--public-bind",
            &room.public,
            "--origin",
            ORIGIN,
            "--apps",
            room.root.join("apps").to_str().unwrap(),
            "--state",
            room.root.join("state.merkle").to_str().unwrap(),
            "--auth-file",
            room.root.join("auth.json").to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::net::TcpStream::connect(&room.public).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn admitted_collection_owns_sent_snapshots_receipts_and_all_tab_kick_revocation() {
    use otrios_core::collection as col;
    let mut room = room(true, 60);
    let mut writer = socket(&room.private, None);
    let (author, resume, epoch) = join(&mut writer, None);
    let mut reader = socket(&room.private, None);
    let (target, target_resume, _) = join(&mut reader, None);
    fn send_col(s: &mut Socket, command: col::Command) {
        s.send(Wire::Binary(
            frame(col::CHANNEL, col::encode_command(&command).unwrap()).into(),
        ))
        .unwrap();
    }
    fn next_col(s: &mut Socket) -> col::Event {
        loop {
            match s.read().unwrap() {
                Wire::Binary(bytes) if bytes.first() == Some(&col::CHANNEL) => {
                    return col::Event::from_bytes(&bytes[1..]).unwrap();
                }
                Wire::Ping(_) => s.flush().unwrap(),
                Wire::Close(_) => panic!("collection socket closed"),
                _ => (),
            }
        }
    }
    fn denied_after_revocation(s: &mut Socket, plan: col::Selection) {
        // The send boundary can suppress a just-revoked queued view before the
        // worker observes the same revocation. Controls are idempotent and take
        // priority over an in-flight request; retry only after consuming them.
        for _ in 0..3 {
            send_col(
                s,
                col::Command::Select {
                    generation: 2,
                    plan: plan.clone(),
                },
            );
            match next_col(s) {
                col::Event::Revoked {
                    collection: COLLECTION,
                    ..
                } => (),
                col::Event::Rejected {
                    status: col::Status::Forbidden,
                    ..
                } => return,
                other => panic!("data escaped revoked admission: {other:?}"),
            }
        }
        panic!("revoked selection never reported its denied admission");
    }
    let plan = otrios_core::canvas::chat::message_selection(col::Anchor::Tail);
    let mut stranger = socket(&room.private, None);
    send_col(
        &mut stranger,
        col::Command::Select {
            generation: 1,
            plan: plan.clone(),
        },
    );
    assert!(matches!(
        next_col(&mut stranger),
        col::Event::Rejected {
            status: col::Status::Forbidden,
            ..
        }
    ));
    send_col(
        &mut reader,
        col::Command::Select {
            generation: 1,
            plan: plan.clone(),
        },
    );
    let col::Event::View(initial) = next_col(&mut reader) else {
        panic!("view");
    };
    let mut replica = col::Replica::new(
        initial.epoch,
        COLLECTION,
        otrios_core::canvas::chat::message_schema(),
    );
    replica.begin_request(1, plan.clone()).unwrap();
    replica.apply(&initial).unwrap();
    send_col(
        &mut reader,
        col::Command::Acknowledge {
            generation: 1,
            cursor: initial.transfer.cursor,
            root: initial.transfer.root,
        },
    );
    let nonce = "1".repeat(32);
    send_chat(
        &mut writer,
        Command::Send {
            epoch,
            nonce: nonce.clone(),
            text: "Immutable dispatch".into(),
        },
    );
    assert!(matches!(
        until(&mut writer, |e| matches!(e, Event::Receipt { .. })),
        Event::Receipt { sequence: 1, .. }
    ));
    let col::Event::View(sent) = next_col(&mut reader) else {
        panic!("sent view");
    };
    replica.apply(&sent).unwrap();
    let pinned = replica.window().unwrap().clone();
    assert_eq!(pinned.keys, vec![1]);
    let message = otrios_core::canvas::chat::read_message(&pinned.record(1).unwrap()).unwrap();
    assert_eq!(message.sender, author.profile);
    assert_eq!(message.text, "Immutable dispatch");
    send_col(
        &mut reader,
        col::Command::Acknowledge {
            generation: 1,
            cursor: sent.transfer.cursor,
            root: sent.transfer.root,
        },
    );
    let mut second_tab = socket(&room.private, None);
    let (same, _, _) = join(&mut second_tab, target_resume);
    assert_eq!(same.profile.id, target.profile.id);
    send_col(
        &mut second_tab,
        col::Command::Select {
            generation: 1,
            plan: plan.clone(),
        },
    );
    let col::Event::View(unacknowledged) = next_col(&mut second_tab) else {
        panic!("second tab view");
    };
    // Leave this tab's complete private view unacknowledged through the kick.
    assert_eq!(unacknowledged.generation, 1);
    send_chat(
        &mut writer,
        Command::Profile {
            name: "Admiral Newname".into(),
            avatar: 42,
        },
    );
    until(
        &mut writer,
        |e| matches!(e, Event::Members { members } if members.iter().any(|m| m.profile.name == "Admiral Newname")),
    );
    send_chat(
        &mut writer,
        Command::Authenticate {
            username: "test-user".into(),
            password: "test-only-password".into(),
        },
    );
    until(&mut writer, |e| {
        matches!(e, Event::Permissions { admin: true })
    });
    send_chat(
        &mut writer,
        Command::Kick {
            participant: target.profile.id,
            generation: target.generation,
            reason: "Test cooldown".into(),
        },
    );
    loop {
        match next_col(&mut reader) {
            col::Event::Revoked {
                collection: COLLECTION,
                ..
            } => break,
            col::Event::View(offer) => {
                replica.apply(&offer).unwrap();
            }
            other => panic!("{other:?}"),
        }
    }
    replica.revoke();
    assert!(replica.window().is_none());
    denied_after_revocation(&mut reader, plan.clone());
    assert!(matches!(
        next_col(&mut second_tab),
        col::Event::Revoked {
            collection: COLLECTION,
            ..
        }
    ));
    denied_after_revocation(&mut second_tab, plan.clone());
    assert_eq!(
        otrios_core::canvas::chat::read_message(&pinned.record(1).unwrap())
            .unwrap()
            .sender,
        author.profile
    );
    restart(&mut room);
    let mut recovered = socket(&room.private, None);
    join(&mut recovered, resume);
    send_chat(
        &mut recovered,
        Command::Send {
            epoch,
            nonce,
            text: "Immutable dispatch".into(),
        },
    );
    assert!(matches!(
        until(&mut recovered, |e| matches!(e, Event::Receipt { .. })),
        Event::Receipt { sequence: 1, .. }
    ));
    send_col(
        &mut recovered,
        col::Command::Select {
            generation: 1,
            plan: plan.clone(),
        },
    );
    let col::Event::View(after) = next_col(&mut recovered) else {
        panic!("recovered view");
    };
    let mut new_replica = col::Replica::new(
        after.epoch,
        COLLECTION,
        otrios_core::canvas::chat::message_schema(),
    );
    new_replica.begin_request(1, plan).unwrap();
    new_replica.apply(&after).unwrap();
    assert_eq!(new_replica.window().unwrap().keys, vec![1, 2]);
    assert_eq!(
        new_replica.window().unwrap().record(1).unwrap().hash(),
        pinned.record(1).unwrap().hash()
    );
}
#[test]
fn room_adapter_handles_identity_live_drafts_native_auth_all_tab_kick_and_recovery() {
    let mut room = room(true, 60);
    let headers = [("Origin", ORIGIN), ("x-otrios-request", "session")];
    assert_eq!(
        request(&room.public, "POST", "/chat/identity", &[], &[]).0,
        403
    );
    let (status, profile, response) =
        response(&room.public, "POST", "/chat/identity", &headers, &[]);
    assert_eq!(status, 200);
    let cookie = response
        .lines()
        .find(|s| s.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap()
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(
        response.contains("HttpOnly")
            && response.contains("Secure")
            && response.contains("SameSite=Strict")
    );
    let profile: serde_json::Value = serde_json::from_slice(&profile).unwrap();
    let (_, again, _) = common::response(
        &room.public,
        "POST",
        "/chat/identity",
        &[
            ("Origin", ORIGIN),
            ("x-otrios-request", "session"),
            ("Cookie", &cookie),
        ],
        &[],
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&again).unwrap(),
        profile
    );
    let mut browser = socket(&room.public, Some(&cookie));
    let (guest, secret, _) = join(&mut browser, None);
    assert!(secret.is_none() && !guest.admin);
    let mut second_tab = socket(&room.public, Some(&cookie));
    let (same, _, _) = join(&mut second_tab, None);
    assert_eq!(same.profile.id, guest.profile.id);
    let mut native = socket(&room.private, None);
    let (admin, resume, epoch) = join(&mut native, None);
    assert!(!admin.admin, "trusted transport is not a chat admin");
    send_chat(
        &mut native,
        Command::Authenticate {
            username: "test-user".into(),
            password: "test-only-password".into(),
        },
    );
    until(&mut native, |e| {
        matches!(e, Event::Permissions { admin: true })
    });
    // The same native guest credential in another connection has no grant.
    let mut copied = socket(&room.private, None);
    assert!(!join(&mut copied, resume.clone()).0.admin);
    let lease = match until(&mut second_tab, |e| matches!(e, Event::DraftLease { .. })) {
        Event::DraftLease { generation } => generation,
        _ => unreachable!(),
    };
    send_chat(
        &mut second_tab,
        Command::Draft {
            generation: lease,
            revision: 1,
            text: "An unfinished pigeon".into(),
        },
    );
    until(
        &mut native,
        |e| matches!(e,Event::Draft{text,..} if text=="An unfinished pigeon"),
    );
    let send = Command::Send {
        epoch,
        nonce: "1".repeat(32),
        text: "A durable dispatch".into(),
    };
    send_chat(&mut native, send.clone());
    let sequence = match until(&mut native, |e| matches!(e, Event::Receipt { .. })) {
        Event::Receipt { sequence, .. } => sequence,
        _ => unreachable!(),
    };
    until(
        &mut browser,
        |e| matches!(e,Event::Message{message} if message.sequence==sequence),
    );
    send_chat(
        &mut native,
        Command::Kick {
            participant: guest.profile.id.clone(),
            generation: guest.generation,
            reason: "Tea break".into(),
        },
    );
    until(&mut browser, |e| matches!(e, Event::Kicked { .. }));
    until(&mut second_tab, |e| matches!(e, Event::Kicked { .. }));
    // Kick leaves the shared transport alive: state still mounts on that socket.
    mount(&mut browser, &room, "demo", 1);
    loop {
        if let Wire::Binary(bytes) = browser.read().unwrap()
            && bytes.first() == Some(&1)
        {
            assert!(matches!(
                otrios_core::canvas::decode_server(&bytes[1..]).unwrap(),
                otrios_interfaces::canvas::ServerMessage::Mounted { .. }
            ));
            break;
        }
    }
    drop((browser, second_tab, native, copied));
    restart(&mut room);
    let mut native = socket(&room.private, None);
    join(&mut native, resume);
    send_chat(&mut native, send);
    until(
        &mut native,
        |e| matches!(e,Event::Receipt{sequence:s,..} if *s==sequence),
    );
    let mut banned = socket(&room.public, Some(&cookie));
    send_chat(&mut banned, Command::Join { credential: None });
    until(&mut banned, |e| matches!(e, Event::Kicked { .. }));
    drop((native, banned));
    room.child.kill().unwrap();
    room.child.wait().unwrap();
    let journal = room.root.join("state.apps/brass-relay.journal");
    let mut bytes = fs::read(&journal).unwrap();
    bytes[0] ^= 1;
    fs::write(&journal, bytes).unwrap();
    restart(&mut room);
    // Successful negotiation proves release/control and state survived bad chat storage.
    let mut healthy = socket(&room.public, Some(&cookie));
    send_chat(&mut healthy, Command::Join { credential: None });
    until(
        &mut healthy,
        |e| matches!(e,Event::Error{message,..} if message.contains("history unavailable")),
    );
}
