// SPDX-License-Identifier: LGPL-3.0-only
//! Independent process ownership of the durable room journal. No shared-memory
//! IPC is implemented or claimed by these filesystem/process checks.
#![cfg(feature = "canvas")]
use otrios_core::canvas::chat::Room;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn journal_owner_child() {
    let Ok(path) = std::env::var("OTRIOS_JOURNAL_OWNER_PATH") else {
        return;
    };
    let _room = Room::open(Some(path.into()), 1_000_000).unwrap();
    println!("OTRIOS_JOURNAL_OWNER_READY");
    std::io::stdout().flush().unwrap();
    let mut stop = [0];
    std::io::stdin().read_exact(&mut stop).unwrap();
}
#[test]
fn only_one_process_can_recover_or_write_a_room_journal() {
    let path = std::env::temp_dir().join(format!(
        "otrios-owner-{}-{}.journal",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "journal_owner_child", "--nocapture"])
            .env("OTRIOS_JOURNAL_OWNER_PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let output = child.0.stdout.take().unwrap();
    let (ready, received) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut sent = false;
        for line in BufReader::new(output).lines() {
            let line = line.unwrap();
            if line == "OTRIOS_JOURNAL_OWNER_READY" && !sent {
                ready.send(()).unwrap();
                sent = true;
            }
        }
    });
    received
        .recv_timeout(Duration::from_secs(10))
        .expect("journal owner readiness");
    assert!(Room::open(Some(path.clone()), 1_000_000).is_err());
    child.0.stdin.as_mut().unwrap().write_all(&[1]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    reader.join().unwrap();
    let room = Room::open(Some(path.clone()), 1_000_001).unwrap();
    drop(room);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("journal.owner.lock")).unwrap();
}
