// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "server")]
use otrios_core::dashboard::{MAX_DOCUMENT_BYTES, Replica};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Server {
    child: Child,
    address: String,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn start(state: &std::path::Path) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let child = Command::new(env!("CARGO_BIN_EXE_dashboard-server"))
        .args(["--bind", &address, "--state", state.to_str().unwrap()])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut server = Server { child, address };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(&server.address).is_ok() {
            return server;
        }
        assert!(server.child.try_wait().unwrap().is_none(), "server exited");
        assert!(Instant::now() < deadline, "server startup timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn post(server: &Server, body: &[u8], origin: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(&server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "POST /sync HTTP/1.0\r\nHost: {}\r\nOrigin: {}\r\nContent-Length: {}\r\n\r\n",
        server.address,
        origin,
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let split = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
    let status = std::str::from_utf8(&bytes[..split])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, bytes[split + 4..].to_vec())
}
#[test]
fn real_process_sync_rejects_invalid_updates_and_recovers_persisted_history() {
    let state = std::env::temp_dir().join(format!(
        "otrios-sync-{}-{}.automerge",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let server = start(&state);
    let origin = format!("http://{}", server.address);
    let mut a = Replica::new().unwrap();
    let mut b = Replica::new().unwrap();
    a.set_enabled(false).unwrap();
    b.set_enabled(true).unwrap();
    let (status, bytes) = post(&server, &a.save(), &origin);
    assert_eq!(status, 200);
    a.merge(&bytes).unwrap();
    let (status, bytes) = post(&server, &b.save(), &origin);
    assert_eq!(status, 200);
    b.merge(&bytes).unwrap();
    let (status, bytes) = post(&server, &a.save(), &origin);
    assert_eq!(status, 200);
    a.merge(&bytes).unwrap();
    assert_eq!(a.snapshot().bytes(), b.snapshot().bytes());
    assert_eq!(post(&server, b"invalid", &origin).0, 400);
    assert_eq!(
        post(&server, &vec![0; MAX_DOCUMENT_BYTES + 1], &origin).0,
        400
    );
    assert_eq!(post(&server, &a.save(), "https://unrelated.example").0, 403);
    drop(server);
    let restarted = start(&state);
    let origin = format!("http://{}", restarted.address);
    let (_, bytes) = post(&restarted, &Replica::new().unwrap().save(), &origin);
    let recovered = Replica::load(&bytes).unwrap();
    assert_eq!(recovered.snapshot().id(), a.snapshot().id());
    drop(restarted);
    std::fs::remove_file(state).unwrap();
}
