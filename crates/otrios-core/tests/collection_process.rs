// SPDX-License-Identifier: LGPL-3.0-only
//! Real child-process collection transfer over bounded TCP frames. This is not
//! Linux shared-memory IPC or qualification of the dashboard WebSocket adapter.
#![cfg(feature = "merkle")]
use otrios_core::{
    collection::*,
    merkle::{Edit, Snapshot},
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command as Process, Stdio},
    time::{Duration, Instant},
};

fn schema() -> Schema {
    Schema {
        id: [24; 32],
        fields: BTreeMap::from([(1, [1; 32]), (2, [2; 32])]),
    }
}
fn row(value: &[u8]) -> Snapshot {
    Snapshot::empty()
        .edit(
            [1; 16],
            &[
                Edit {
                    path: vec![1],
                    schema: [1; 32],
                    value: Some(value.to_vec()),
                },
                Edit {
                    path: vec![2],
                    schema: [2; 32],
                    value: Some(b"private-canary".to_vec()),
                },
            ],
        )
        .unwrap()
        .0
}
fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut size = [0; 4];
    stream.read_exact(&mut size).unwrap();
    let size = u32::from_le_bytes(size) as usize;
    assert!(size <= 1024 * 1024 + 256);
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).unwrap();
    bytes
}
fn write_frame(stream: &mut TcpStream, bytes: &[u8]) {
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(bytes).unwrap();
}
struct Peer {
    stream: TcpStream,
    child: ChildGuard,
    session: Session,
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn peer(mode: &str) -> Peer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let child = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "collection_peer", "--nocapture"])
        .env(
            "OTRIOS_COLLECTION_ADDRESS",
            listener.local_addr().unwrap().to_string(),
        )
        .env("OTRIOS_COLLECTION_MODE", mode)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let child = ChildGuard(child);
    let start = Instant::now();
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "collection peer did not connect"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    };
    let peer = Peer {
        stream,
        child,
        session: Session::default(),
    };
    peer.stream.set_nonblocking(false).unwrap();
    peer.stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    peer.stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    peer
}

#[test]
fn collection_peer() {
    let Ok(address) = std::env::var("OTRIOS_COLLECTION_ADDRESS") else {
        return;
    };
    let tail = std::env::var("OTRIOS_COLLECTION_MODE").unwrap() == "tail";
    let plan = Selection {
        collection: 24,
        schema: [24; 32],
        anchor: if tail {
            Anchor::Tail
        } else {
            Anchor::Around(15_000)
        },
        record_limit: 16,
        byte_limit: 8192,
        fields: vec![1],
    };
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut replica = Replica::new(71, 24, schema());
    replica.begin_request(1, plan.clone()).unwrap();
    write_frame(
        &mut stream,
        &encode_command(&Command::Select {
            generation: 1,
            plan,
        })
        .unwrap(),
    );
    let mut pinned = None;
    for iteration in 0..3 {
        let bytes = read_frame(&mut stream);
        if !tail && iteration > 0 {
            assert!(bytes.is_empty());
            continue;
        }
        assert!(!bytes.windows(14).any(|b| b == b"private-canary"));
        let offer = Offer::from_bytes(&bytes).unwrap();
        replica.apply(&offer).unwrap();
        let window = replica.window().unwrap();
        assert_eq!(window.keys.len(), 16);
        assert!(window.snapshot().canonical_bytes() < 16 * 1024);
        if iteration == 0 {
            pinned = Some(window.clone());
            assert_eq!(
                window.keys.last(),
                Some(if tail { &30_000 } else { &15_021 })
            );
        } else {
            assert_eq!(window.keys.last(), Some(&(30_000 + iteration * 3)));
            assert_eq!(pinned.as_ref().unwrap().keys.last(), Some(&30_000));
        }
        write_frame(
            &mut stream,
            &encode_command(&Command::Acknowledge {
                generation: 1,
                cursor: offer.transfer.cursor,
                root: offer.transfer.root,
            })
            .unwrap(),
        );
    }
}

#[test]
fn two_processes_qualify_tail_and_sparse_anchored_windows() {
    let mut collection = Collection::new(24, schema(), Budget::default()).unwrap();
    let record = row(b"initial-record");
    for id in 1..=10_000 {
        collection = collection.append(id * 3, record.clone()).unwrap();
    }
    let mut authority = Authority::new(71, collection);
    let grant = Grant {
        collection: 24,
        fields: vec![1],
    };
    let mut peers = [peer("tail"), peer("anchored")];
    let mut transfers = [0; 2];
    for (i, peer) in peers.iter_mut().enumerate() {
        let command = decode_command(&read_frame(&mut peer.stream)).unwrap();
        let offer = authority
            .handle(&mut peer.session, grant.clone(), command)
            .unwrap()
            .unwrap();
        let bytes = offer.to_bytes().unwrap();
        transfers[i] += bytes.len();
        write_frame(&mut peer.stream, &bytes);
        let ack = decode_command(&read_frame(&mut peer.stream)).unwrap();
        assert!(
            authority
                .handle(&mut peer.session, grant.clone(), ack)
                .unwrap()
                .is_none()
        );
    }
    for iteration in 1..=2 {
        let next = authority
            .collection()
            .append(
                30_000 + iteration * 3,
                row(format!("record-{iteration}").as_bytes()),
            )
            .unwrap();
        authority
            .publish(authority.collection().revision(), next)
            .unwrap();
        for (i, peer) in peers.iter_mut().enumerate() {
            let offer = authority.poll(&mut peer.session).unwrap();
            let bytes = offer
                .as_ref()
                .map_or_else(Vec::new, |o| o.to_bytes().unwrap());
            assert_eq!(offer.is_some(), i == 0);
            transfers[i] += bytes.len();
            write_frame(&mut peer.stream, &bytes);
            if offer.is_some() {
                let ack = decode_command(&read_frame(&mut peer.stream)).unwrap();
                assert!(
                    authority
                        .handle(&mut peer.session, grant.clone(), ack)
                        .unwrap()
                        .is_none()
                );
            }
        }
    }
    for peer in &mut peers {
        let start = Instant::now();
        loop {
            if let Some(status) = peer.child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "collection peer did not exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(
        transfers[0] < 32 * 1024 && transfers[1] < 16 * 1024,
        "wire bytes={transfers:?}"
    );
    println!("10,002 records; tail/anchored wire bytes={transfers:?}");
}
