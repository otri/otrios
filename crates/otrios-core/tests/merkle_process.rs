// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "merkle")]
use otrios_core::merkle::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn write_frame(stream: &mut TcpStream, bytes: &[u8]) {
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(bytes).unwrap();
}
fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut len = [0; 4];
    stream.read_exact(&mut len).unwrap();
    let size = u32::from_le_bytes(len) as usize;
    assert!(size < MAX_TREE_BYTES + 1024);
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).unwrap();
    bytes
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn connect(lease: u64, scope: u64) -> (TcpStream, ChildGuard) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "replica_child", "--nocapture"])
        .env(
            "OTRIOS_MERKLE_TEST_ADDRESS",
            listener.local_addr().unwrap().to_string(),
        )
        .env("OTRIOS_MERKLE_TEST_LEASE", lease.to_string())
        .env("OTRIOS_MERKLE_TEST_SCOPE", scope.to_string())
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let guard = ChildGuard(child);
    let start = Instant::now();
    let stream = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "receiver did not connect"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("{e}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    (stream, guard)
}
#[test]
fn replica_child() {
    let Ok(address) = std::env::var("OTRIOS_MERKLE_TEST_ADDRESS") else {
        return;
    };
    let lease = std::env::var("OTRIOS_MERKLE_TEST_LEASE")
        .unwrap()
        .parse()
        .unwrap();
    let scope: u64 = std::env::var("OTRIOS_MERKLE_TEST_SCOPE")
        .unwrap()
        .parse()
        .unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let path = if scope == 0 { vec![] } else { vec![scope] };
    let mut replica = Replica::for_lease(17, lease, path).unwrap();
    let mut pinned = None;
    for iteration in 0..3 {
        let bytes = read_frame(&mut stream);
        if scope != 2 {
            assert!(!bytes.windows(14).any(|b| b == b"private-canary"));
        }
        // The covered leaves have identical canonical bytes and share one
        // transferred record: one root plus one leaf, materialized at two keys.
        assert_eq!(Replica::transferred_nodes(&bytes).unwrap(), 2);
        replica.apply(&bytes).unwrap();
        let snapshot = replica.snapshot().unwrap();
        if iteration == 0 {
            pinned = Some(snapshot.clone());
        } else {
            assert_ne!(snapshot.hash(), pinned.as_ref().unwrap().hash());
        }
        if scope == 0 {
            assert!(snapshot.register().is_none());
            assert_eq!(snapshot.child_keys().collect::<Vec<_>>(), vec![1, 2]);
            for key in [1, 2] {
                let node = snapshot.subtree(&[key]).unwrap();
                let values = node.register().unwrap().values().collect::<Vec<_>>();
                assert_eq!(values.len(), 1);
                assert_eq!(values[0].2, Some([iteration as u8].as_slice()));
            }
        } else {
            assert_eq!(snapshot.child_keys().collect::<Vec<_>>(), vec![9]);
        }
        let cursor = replica.cursor().unwrap();
        let mut ack = cursor.sequence.to_le_bytes().to_vec();
        ack.extend(snapshot.hash());
        write_frame(&mut stream, &ack);
    }
}
fn deliver(authority: &Authority, sub: &mut Subscription, stream: &mut TcpStream) {
    let offer = authority.offer(sub).unwrap().unwrap();
    write_frame(stream, offer.bytes());
    let ack = read_frame(stream);
    assert_eq!(ack.len(), 40);
    assert_eq!(
        u64::from_le_bytes(ack[..8].try_into().unwrap()),
        offer.cursor.sequence
    );
    assert_eq!(&ack[8..], offer.root.as_ref().unwrap());
    sub.acknowledge(offer.cursor, offer.root).unwrap();
}
#[test]
fn two_real_processes_receive_only_their_scoped_incremental_revisions() {
    let tree = Snapshot::empty()
        .edit(
            [1; 16],
            &[
                Edit {
                    path: vec![1, 9],
                    schema: [7; 32],
                    value: Some(b"public".to_vec()),
                },
                Edit {
                    path: vec![2, 9],
                    schema: [7; 32],
                    value: Some(b"private-canary".to_vec()),
                },
            ],
        )
        .unwrap()
        .0;
    let mut authority = Authority::new(17, tree);
    let mut public = authority.subscribe(&[vec![1]], vec![1]).unwrap();
    let mut private = authority.subscribe(&[vec![2]], vec![2]).unwrap();
    let (mut ps, mut public_child) = connect(public.cursor().lease, 1);
    let (mut ss, mut private_child) = connect(private.cursor().lease, 2);
    deliver(&authority, &mut public, &mut ps);
    deliver(&authority, &mut private, &mut ss);
    for actor in [2, 3] {
        let tree = authority
            .snapshot()
            .edit(
                [actor; 16],
                &[Edit {
                    path: vec![1, 9],
                    schema: [7; 32],
                    value: Some(vec![actor; 1024]),
                }],
            )
            .unwrap()
            .0;
        authority
            .publish(authority.snapshot().hash(), tree)
            .unwrap();
        assert!(authority.offer(&mut private).unwrap().is_none());
        deliver(&authority, &mut public, &mut ps);
        let tree = authority
            .snapshot()
            .edit(
                [actor; 16],
                &[Edit {
                    path: vec![2, 9],
                    schema: [7; 32],
                    value: Some(b"private-canary".to_vec()),
                }],
            )
            .unwrap()
            .0;
        authority
            .publish(authority.snapshot().hash(), tree)
            .unwrap();
        assert!(authority.offer(&mut public).unwrap().is_none());
        deliver(&authority, &mut private, &mut ss);
    }
    for child in [&mut public_child, &mut private_child] {
        let start = Instant::now();
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "receiver did not exit"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn real_process_receives_one_coherent_covered_view_without_unrelated_payloads() {
    let edit = |path, value| Edit {
        path,
        schema: [7; 32],
        value: Some(value),
    };
    let tree = Snapshot::empty()
        .edit(
            [1; 16],
            &[
                edit(vec![], b"private-canary".to_vec()),
                edit(vec![1], vec![0]),
                edit(vec![2], vec![0]),
                edit(vec![3], b"private-canary".to_vec()),
            ],
        )
        .unwrap()
        .0;
    let mut authority = Authority::new(17, tree);
    let coverage = Coverage::new(&[vec![1], vec![2]], &[]).unwrap();
    let mut sub = authority
        .subscribe_coverage(&[vec![1], vec![2]], coverage)
        .unwrap();
    let (mut stream, mut child) = connect(sub.cursor().lease, 0);
    deliver(&authority, &mut sub, &mut stream);
    for number in [1, 2] {
        let next = authority
            .snapshot()
            .edit([1; 16], &[edit(vec![3], vec![number; 1024])])
            .unwrap()
            .0;
        authority
            .publish(authority.snapshot().hash(), next)
            .unwrap();
        assert!(authority.offer(&mut sub).unwrap().is_none());
        let next = authority
            .snapshot()
            .edit(
                [1; 16],
                &[edit(vec![1], vec![number]), edit(vec![2], vec![number])],
            )
            .unwrap()
            .0;
        authority
            .publish(authority.snapshot().hash(), next)
            .unwrap();
        deliver(&authority, &mut sub, &mut stream);
    }
    let start = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "covered receiver did not exit"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
