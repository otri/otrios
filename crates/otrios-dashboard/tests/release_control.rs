// SPDX-License-Identifier: Apache-2.0
#![cfg(all(feature = "server", feature = "host-sync"))]
use otrios_interfaces::release::{Release, ServerMessage, VERSION};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tungstenite::{Message, client::IntoClientRequest};
struct Rig {
    child: Child,
    root: PathBuf,
    address: String,
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
struct Watcher(Child);
impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn publish_file(root: &std::path::Path, protocol: &str, nonce: u8) -> Release {
    let raw =
        serde_json::to_vec(&serde_json::json!({"sync_protocol":protocol,"nonce":nonce})).unwrap();
    let id = Sha256::digest(&raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let folder = root.join(&id);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("manifest.json"), raw).unwrap();
    let release = Release {
        id: id.clone(),
        web_entry: format!("/releases/{id}/web/index.html"),
        sync_protocol: protocol.into(),
    };
    fs::write(
        root.join("pending.json"),
        serde_json::to_vec(&release).unwrap(),
    )
    .unwrap();
    fs::rename(root.join("pending.json"), root.join("current.json")).unwrap();
    release
}
fn post(address: &str) -> u16 {
    let mut tcp = TcpStream::connect(address).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(tcp,"POST /publish HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    let mut reply = String::new();
    tcp.read_to_string(&mut reply).unwrap();
    reply.split_whitespace().nth(1).unwrap().parse().unwrap()
}
fn socket(address: &str) -> tungstenite::WebSocket<TcpStream> {
    let tcp = TcpStream::connect(address).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tungstenite::client(format!("ws://{address}/release/events"), tcp)
        .unwrap()
        .0
}
fn hello(socket: &mut tungstenite::WebSocket<TcpStream>, version: u32, protocol: &str) {
    socket.send(Message::Text(serde_json::json!({"type":"hello","version":version,"release":null,"state_protocol":protocol,"future_optional_field":true}).to_string().into())).unwrap();
}
fn event(socket: &mut tungstenite::WebSocket<TcpStream>) -> ServerMessage {
    loop {
        match socket.read().unwrap() {
            Message::Text(s) => return serde_json::from_str(&s).unwrap(),
            Message::Ping(_) => socket.flush().unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
}
#[test]
fn independent_hello_upgrade_and_reconnect_survive_future_state_protocols() {
    let root = std::env::temp_dir().join(format!(
        "otrios-release-control-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let a = publish_file(&root, "otrios-canvas-sync-v2", 1);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let child = Command::new(env!("CARGO_BIN_EXE_release-control"))
        .args([
            "--bind",
            &address,
            "--releases",
            root.to_str().unwrap(),
            "--origin",
            "https://otrios.example",
        ])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut rig = Rig {
        child,
        root,
        address,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(&rig.address).is_err() {
        assert!(rig.child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut first = socket(&rig.address);
    hello(&mut first, VERSION, "otrios-canvas-sync-v2");
    match event(&mut first) {
        ServerMessage::Hello {
            version,
            release,
            refresh,
        } => {
            assert_eq!(version, 1);
            assert_eq!(release, a);
            assert!(!refresh)
        }
        other => panic!("{other:?}"),
    }
    let mut pending = socket(&rig.address); // Subscribe/hello racing publication must see the latest hash.
    let b = publish_file(&rig.root, "entirely-new-crdt-v99", 2);
    assert_eq!(post(&rig.address), 204);
    match event(&mut first) {
        ServerMessage::Upgrade {
            release, refresh, ..
        } => {
            assert_eq!(release, b);
            assert!(refresh)
        }
        other => panic!("{other:?}"),
    }
    hello(&mut pending, 999, "unknown-crdt");
    match event(&mut pending) {
        ServerMessage::Hello {
            version,
            release,
            refresh,
        } => {
            assert_eq!(version, 1);
            assert_eq!(release, b);
            assert!(refresh)
        }
        other => panic!("{other:?}"),
    }
    // No notification: reconnect must still read the active pointer and recover the gap.
    let c = publish_file(&rig.root, "third-state-protocol", 3);
    let mut reconnect = socket(&rig.address);
    hello(&mut reconnect, 1, "old-protocol");
    assert_eq!(event(&mut reconnect).release(), &c);
    fs::write(rig.root.join("current.json"), b"invalid").unwrap();
    assert_eq!(post(&rig.address), 503);
    assert_eq!(event(&mut first).release(), &c); // The recovered current hash, never invalid bytes.
    publish_file(&rig.root, "third-state-protocol", 3);
    let tcp = TcpStream::connect(&rig.address).unwrap();
    let mut bad = format!("ws://{}/release/events", rig.address)
        .into_client_request()
        .unwrap();
    bad.headers_mut()
        .insert("Origin", "https://unrelated.example".parse().unwrap());
    assert!(
        matches!(tungstenite::client(bad,tcp),Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r))) if r.status()==403)
    );
    // Exercise the actual native bootstrap companion against a future data protocol.
    let mut child = Command::new(env!("CARGO_BIN_EXE_release-watch"))
        .args([
            &format!("ws://{}/release/events", rig.address),
            "",
            "unsupported-client-crdt",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let _watcher = Watcher(child);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let _ = tx.send(line);
    });
    let message: ServerMessage =
        serde_json::from_str(&rx.recv_timeout(Duration::from_secs(5)).unwrap()).unwrap();
    assert_eq!(message.release(), &c);
}
