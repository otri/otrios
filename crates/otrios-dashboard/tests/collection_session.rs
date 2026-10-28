// SPDX-License-Identifier: Apache-2.0
//! Real shared-WebSocket adapter against a large synthetic runtime collection.
#![cfg(all(feature = "server", feature = "canvas", feature = "host-sync"))]
mod common;
use common::*;
use otrios_core::collection::{self as core, *};
use std::{
    fs,
    time::{Duration, Instant},
};
use tungstenite::Message;

fn plan(anchor: Anchor, fields: Vec<u64>) -> Selection {
    Selection {
        collection: 24,
        schema: [24; 32],
        anchor,
        record_limit: 16,
        byte_limit: 8192,
        fields,
    }
}
fn send(s: &mut Socket, command: Command) {
    s.send(Message::Binary(
        otrios_interfaces::session::frame(CHANNEL, core::encode_command(&command).unwrap()).into(),
    ))
    .unwrap();
}
fn event(s: &mut Socket) -> Event {
    loop {
        match s.read().unwrap() {
            Message::Binary(bytes) if bytes.first() == Some(&CHANNEL) => {
                return Event::from_bytes(&bytes[1..]).unwrap();
            }
            Message::Ping(_) => s.flush().unwrap(),
            other => panic!("unexpected collection session frame: {other:?}"),
        }
    }
}
fn select(s: &mut Socket, generation: u64, plan: Selection) -> Offer {
    send(s, Command::Select { generation, plan });
    match event(s) {
        Event::View(offer) => offer,
        other => panic!("{other:?}"),
    }
}
fn ack(s: &mut Socket, offer: &Offer) {
    send(
        s,
        Command::Acknowledge {
            generation: offer.generation,
            cursor: offer.transfer.cursor,
            root: offer.transfer.root,
        },
    );
}
fn update(room: &Room, records: u32, retain: u64) {
    let path = room.root.join("collection-fixture.json");
    let next = path.with_extension("next");
    fs::write(
        &next,
        serde_json::json!({"records":records,"retain_from":retain}).to_string(),
    )
    .unwrap();
    fs::rename(next, path).unwrap();
}
fn quiet(s: &mut Socket) {
    // A pong orders all preceding socket traffic without requiring a sleep or
    // conflating an observation timeout with a failed authority process.
    s.send(Message::Ping(vec![24].into())).unwrap();
    loop {
        match s.read().unwrap() {
            Message::Pong(bytes) => {
                assert_eq!(&bytes[..], &[24]);
                return;
            }
            Message::Ping(_) => s.flush().unwrap(),
            other => panic!("unexpected transfer while selection should be quiet: {other:?}"),
        }
    }
}

#[test]
fn shared_session_qualifies_large_sparse_windows_coalesces_slow_readers_and_recovers() {
    let room = room_with_collection(false, 60, Some(65_536));
    let mut anchored = socket(&room.public, None);
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = anchored.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(25)))
            .unwrap();
    }
    let mut generation = 1;
    let p = plan(Anchor::Around(192), vec![1]);
    let deadline = Instant::now() + Duration::from_secs(20);
    let initial = loop {
        send(
            &mut anchored,
            Command::Select {
                generation,
                plan: p.clone(),
            },
        );
        match event(&mut anchored) {
            Event::View(offer) => break offer,
            Event::Rejected {
                status: Status::Unavailable,
                ..
            } => {
                assert!(Instant::now() < deadline);
                generation += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            other => panic!("{other:?}"),
        }
    };
    let mut replica = Replica::new(initial.epoch, 24, synthetic_schema());
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = anchored.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
    }
    replica.begin_request(generation, p.clone()).unwrap();
    replica.apply(&initial).unwrap();
    ack(&mut anchored, &initial);
    let pinned = replica.window().unwrap().clone();
    assert_eq!(pinned.keys, (56..72).map(|i| i * 3).collect::<Vec<_>>());
    assert_eq!(pinned.work.records_considered, 0); // Client receives only the selected DAG.
    assert!(initial.transfer.bytes().len() < 16 * 1024);
    assert!(
        !initial
            .transfer
            .bytes()
            .windows(14)
            .any(|bytes| bytes == b"private-canary")
    );
    let mut tail = socket(&room.public, None);
    let tail_plan = plan(Anchor::Tail, vec![1]);
    let first_tail = select(&mut tail, 1, tail_plan.clone());
    let mut tail_replica = Replica::new(first_tail.epoch, 24, synthetic_schema());
    tail_replica.begin_request(1, tail_plan).unwrap();
    tail_replica.apply(&first_tail).unwrap();
    let mut observer = socket(&room.public, None);
    let observed = select(&mut observer, 1, plan(Anchor::Tail, vec![1]));
    let mut observer_replica = Replica::new(observed.epoch, 24, synthetic_schema());
    observer_replica
        .begin_request(1, plan(Anchor::Tail, vec![1]))
        .unwrap();
    observer_replica.apply(&observed).unwrap();
    ack(&mut observer, &observed);
    // An independent fast reader confirms four distinct publications while the
    // slow reader deliberately withholds its first ACK throughout all four.
    for records in 65_537..=65_540 {
        update(&room, records, 1);
        let Event::View(offer) = event(&mut observer) else {
            panic!("observer update");
        };
        observer_replica.apply(&offer).unwrap();
        assert_eq!(
            observer_replica.window().unwrap().keys.last(),
            Some(&(u64::from(records) * 3))
        );
        ack(&mut observer, &offer);
    }
    // The anchored selection remains unchanged; an outstanding tail ACK holds
    // one transfer even as the source publishes a later complete revision.
    quiet(&mut anchored);
    quiet(&mut tail);
    ack(&mut tail, &first_tail);
    let deadline = Instant::now() + Duration::from_secs(10);
    let latest = loop {
        assert!(Instant::now() < deadline);
        let Event::View(offer) = event(&mut tail) else {
            panic!("tail update");
        };
        tail_replica.apply(&offer).unwrap();
        ack(&mut tail, &offer);
        if tail_replica.window().unwrap().keys.last() == Some(&(65_540 * 3)) {
            break offer;
        }
    };
    assert!(latest.transfer.bytes().len() < 16 * 1024);
    quiet(&mut anchored);
    assert_eq!(
        pinned.snapshot().hash(),
        replica.window().unwrap().snapshot().hash()
    );
    // Superseded response generations cannot replace the final requested view.
    for n in 1..=4 {
        send(
            &mut anchored,
            Command::Select {
                generation: generation + n,
                plan: plan(Anchor::Around(300 + n * 3), vec![1]),
            },
        );
    }
    generation += 4;
    replica
        .begin_request(generation, plan(Anchor::Around(312), vec![1]))
        .unwrap();
    let replacement = loop {
        let Event::View(offer) = event(&mut anchored) else {
            panic!("replacement");
        };
        if offer.generation < generation {
            assert_eq!(replica.apply(&offer), Err(core::Error::Stale));
        } else {
            replica.apply(&offer).unwrap();
            break offer;
        }
    };
    ack(&mut anchored, &replacement);
    assert!(replica.window().unwrap().keys.contains(&312));
    // Retention invalidates old cursors explicitly; the previous qualified
    // window is retained by the client until it chooses a new anchor.
    update(&room, 65_540, 1000);
    let expiry_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < expiry_deadline,
            "retention publication deadline"
        );
        generation += 1;
        send(
            &mut anchored,
            Command::Select {
                generation,
                plan: plan(
                    Anchor::Before(Cursor {
                        epoch: initial.epoch,
                        collection: 24,
                        key: 192,
                    }),
                    vec![1],
                ),
            },
        );
        match event(&mut anchored) {
            Event::AnchorExpired {
                neighbor: Some(1002),
                ..
            } => break,
            Event::View(offer) => {
                ack(&mut anchored, &offer);
                std::thread::sleep(Duration::from_millis(20));
            }
            other => panic!("{other:?}"),
        }
    }
    let mut reconnect = socket(&room.public, None);
    let fresh = select(&mut reconnect, 1, plan(Anchor::Tail, vec![1]));
    assert_eq!(fresh.epoch, initial.epoch); // Same live authority, fresh RAM lease.
    assert_ne!(fresh.lease, first_tail.lease);
    let mut recovered = Replica::new(fresh.epoch, 24, synthetic_schema());
    recovered
        .begin_request(1, plan(Anchor::Tail, vec![1]))
        .unwrap();
    recovered.apply(&fresh).unwrap();
    assert_eq!(
        recovered.window().unwrap().snapshot().hash(),
        tail_replica.window().unwrap().snapshot().hash()
    );
    println!(
        "shared socket: 65,540 records; initial wire={} latest tail wire={}",
        initial.to_bytes().unwrap().len(),
        latest.to_bytes().unwrap().len()
    );
}

#[test]
fn authority_restart_rejects_an_old_epoch_cursor_after_a_new_hello() {
    let mut room = room_with_collection(false, 60, Some(256));
    let mut first = socket(&room.public, None);
    let old = select(&mut first, 1, plan(Anchor::Tail, vec![1]));
    room.child.kill().unwrap();
    room.child.wait().unwrap();
    room.child = std::process::Command::new(env!("CARGO_BIN_EXE_dashboard-server"))
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
            "--collection-fixture",
            room.root.join("collection-fixture.json").to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::net::TcpStream::connect(&room.public).is_err() {
        assert!(Instant::now() < deadline && room.child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut next = socket(&room.public, None);
    let current = select(&mut next, 1, plan(Anchor::Tail, vec![1]));
    assert_ne!(old.epoch, current.epoch);
    send(
        &mut next,
        Command::Select {
            generation: 2,
            plan: plan(
                Anchor::Before(Cursor {
                    epoch: old.epoch,
                    collection: 24,
                    key: 768,
                }),
                vec![1],
            ),
        },
    );
    assert!(matches!(
        event(&mut next),
        Event::Rejected {
            generation: 2,
            status: Status::Stale
        }
    ));
    quiet(&mut next);
}

#[test]
fn private_collection_is_revoked_while_an_offer_is_unacknowledged_and_socket_stays_usable() {
    let room = room_with_collection(true, 1, Some(256));
    let secret = cookie(&room);
    let mut s = socket(&room.public, Some(&secret));
    let offer = select(&mut s, 1, plan(Anchor::Tail, vec![1, 2]));
    let mut replica = Replica::new(offer.epoch, 24, synthetic_schema());
    replica
        .begin_request(1, plan(Anchor::Tail, vec![1, 2]))
        .unwrap();
    replica.apply(&offer).unwrap();
    assert!(replica.window().is_some());
    match event(&mut s) {
        Event::Revoked {
            epoch,
            generation: 1,
            collection: 24,
        } => assert_eq!(epoch, offer.epoch),
        other => panic!("expected unacknowledged private coverage revocation, got {other:?}"),
    }
    replica.revoke();
    assert!(replica.window().is_none());
    assert_eq!(replica.apply(&offer), Err(core::Error::Forbidden));
    send(
        &mut s,
        Command::Select {
            generation: 2,
            plan: plan(Anchor::Tail, vec![2]),
        },
    );
    assert!(matches!(
        event(&mut s),
        Event::Rejected {
            generation: 2,
            status: Status::Forbidden
        }
    ));
    let public = select(&mut s, 3, plan(Anchor::Tail, vec![1]));
    ack(&mut s, &public);
    s.send(Message::Binary(
        otrios_interfaces::session::frame(CHANNEL, vec![0]).into(),
    ))
    .unwrap();
    assert!(matches!(
        event(&mut s),
        Event::Rejected {
            generation: 0,
            status: Status::Invalid
        }
    ));
    quiet(&mut s);
}
