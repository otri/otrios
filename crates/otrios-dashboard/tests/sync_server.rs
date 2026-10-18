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
    start_with_releases(state, None)
}
fn start_with_releases(state: &std::path::Path, releases: Option<&std::path::Path>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let mut command = Command::new(env!("CARGO_BIN_EXE_dashboard-server"));
    command.args(["--bind", &address, "--state", state.to_str().unwrap()]);
    if let Some(releases) = releases {
        command.args(["--releases", releases.to_str().unwrap()]);
    }
    let child = command.stdout(Stdio::null()).spawn().unwrap();
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

#[test]
fn code_release_is_pushed_to_existing_peer_and_preserves_state_and_old_assets() {
    use tungstenite::client::IntoClientRequest;
    let root = std::env::temp_dir().join(format!("otrios-release-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let state = root.join("state.automerge");
    let first = "a".repeat(64);
    let second = "b".repeat(64);
    let publish = |id: &str, html: &str| {
        let dir = root.join(id).join("web");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), html).unwrap();
        let pointer =
            serde_json::json!({"id":id,"web_entry":format!("/releases/{id}/web/index.html")});
        let temp = root.join("pending.json");
        std::fs::write(&temp, pointer.to_string()).unwrap();
        std::fs::rename(temp, root.join("current.json")).unwrap();
    };
    publish(&first, "first code");
    let server = start_with_releases(&state, Some(&root));
    let mut request = format!("ws://{}/stream", server.address)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", "https://unrelated.example".parse().unwrap());
    assert!(
        matches!(tungstenite::connect(request), Err(tungstenite::Error::Http(response)) if response.status().as_u16()==403)
    );
    let mut peer = Peer::connect(&server, Replica::new().unwrap());
    peer.replica.set_enabled(false).unwrap();
    for _ in 0..8 {
        peer.pump();
    }
    let before = Replica::load(&std::fs::read(&state).unwrap())
        .unwrap()
        .snapshot()
        .id();
    publish(&second, "second code");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match peer.socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                let notice: serde_json::Value = serde_json::from_str(&text).unwrap();
                if notice["release"]["id"] == second {
                    break;
                }
            }
            Ok(tungstenite::Message::Ping(_)) => {
                peer.socket.flush().unwrap();
            }
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            other => panic!("Unexpected release stream message: {other:?}"),
        }
        assert!(Instant::now() < deadline, "release was not pushed");
    }
    let get = |path: &str| {
        let mut stream = TcpStream::connect(&server.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.0\r\nHost: {}\r\n\r\n",
            server.address
        )
        .unwrap();
        let mut bytes = String::new();
        stream.read_to_string(&mut bytes).unwrap();
        bytes
    };
    assert!(get("/").contains(&format!("/releases/{second}/web/index.html")));
    let old = get(&format!("/releases/{first}/web/index.html"));
    assert!(old.ends_with("first code"));
    assert!(old.contains("max-age=31536000, immutable"));
    assert!(get("/releases/invalid/web/index.html").starts_with("HTTP/1.0 404"));
    assert_eq!(
        Replica::load(&std::fs::read(&state).unwrap())
            .unwrap()
            .snapshot()
            .id(),
        before
    );
    drop(peer);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
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
    let _ = stream.write_all(body);
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
        413
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

struct Peer {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    replica: Replica,
    session: otrios_core::dashboard::SyncSession,
}
impl Peer {
    fn connect(server: &Server, replica: Replica) -> Self {
        let (mut socket, _) =
            tungstenite::connect(format!("ws://{}/stream", server.address).as_str()).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
            tcp.set_read_timeout(Some(Duration::from_millis(30)))
                .unwrap();
        }
        Self {
            socket,
            replica,
            session: Default::default(),
        }
    }
    fn pump(&mut self) {
        if let Some(bytes) = self.replica.generate_sync(&mut self.session).unwrap() {
            self.socket
                .send(tungstenite::Message::Binary(bytes.into()))
                .unwrap();
        }
        match self.socket.read() {
            Ok(tungstenite::Message::Binary(bytes)) => {
                self.replica
                    .receive_sync(&mut self.session, &bytes)
                    .unwrap();
            }
            Ok(tungstenite::Message::Ping(_)) => {
                self.socket.flush().unwrap();
            }
            Ok(tungstenite::Message::Text(_) | tungstenite::Message::Pong(_)) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            other => panic!("Unexpected stream result: {other:?}"),
        }
    }
}
#[test]
fn websocket_push_reconnect_and_concurrent_offline_changes() {
    let state =
        std::env::temp_dir().join(format!("otrios-stream-{}.automerge", std::process::id()));
    let server = start(&state);
    let mut a = Peer::connect(&server, Replica::new().unwrap());
    let mut b = Peer::connect(&server, Replica::new().unwrap());
    for _ in 0..8 {
        a.pump();
        b.pump();
    }
    a.replica.set_enabled(false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while b.replica.snapshot().id() != a.replica.snapshot().id() {
        a.pump();
        b.pump();
        assert!(Instant::now() < deadline, "push timed out");
    }
    // The subscriber has never posted a full document or polled an HTTP state endpoint.
    a.socket.close(None).unwrap();
    let mut offline = a.replica.clone();
    drop(a);
    offline.set_enabled(true).unwrap();
    b.replica.set_enabled(false).unwrap();
    for _ in 0..5 {
        b.pump();
    }
    let mut a = Peer::connect(&server, offline);
    let deadline = Instant::now() + Duration::from_secs(5);
    while a.replica.snapshot().id() != b.replica.snapshot().id() {
        a.pump();
        b.pump();
        assert!(Instant::now() < deadline, "reconnect timed out");
    }
    assert!(
        a.replica
            .snapshot()
            .view()
            .nodes
            .iter()
            .any(|n| n.content == otrios_interfaces::ViewContent::Label("3 touches".into()))
    );
    let stored = Replica::load(&std::fs::read(&state).unwrap()).unwrap();
    assert_eq!(stored.snapshot().id(), a.replica.snapshot().id());
    drop(a);
    drop(b);
    drop(server);
    std::fs::remove_file(state).unwrap();
}

#[test]
fn binary_layout_cli_publish_push_cas_and_restart_preserve_app_state() {
    use otrios_core::dashboard::layout::{DEFAULT_LAYOUT, Layout, MAX_LAYOUT_BYTES};
    use otrios_interfaces::layout::{Expr, Property, Value};
    let root = std::env::temp_dir().join(format!("otrios-layout-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let state = root.join("state");
    let file = root.join("draft.flex");
    let server = start(&state);
    let url = format!("http://{}/layout", server.address);
    let cli = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_dashboard-layout"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    let mut peer = Peer::connect(&server, Replica::new().unwrap());
    peer.replica.set_enabled(false).unwrap();
    for _ in 0..8 {
        peer.pump();
    }
    cli(&["fetch", &url, file.to_str().unwrap()]);
    assert_eq!(std::fs::read(&file).unwrap(), DEFAULT_LAYOUT);
    cli(&[
        "frame",
        file.to_str().unwrap(),
        "2",
        "72",
        "160",
        "440",
        "240",
    ]);
    cli(&["text", file.to_str().unwrap(), "2", "1", "Quiet mode"]);
    cli(&["publish", file.to_str().unwrap(), &url]);
    let layout = Layout::from_bytes(&std::fs::read(&file).unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while peer.replica.snapshot().layout().hex_id() != layout.hex_id() {
        peer.pump();
        assert!(Instant::now() < deadline, "Layout was not pushed");
    }
    assert_eq!(
        peer.replica.snapshot().scene().controls()[0].origin,
        [72., 160.]
    );
    assert!(
        !peer.replica.snapshot().scene().controls()[0]
            .node
            .semantics
            .as_ref()
            .unwrap()
            .checked
    );
    assert!(
        peer.replica
            .snapshot()
            .view()
            .nodes
            .iter()
            .any(|n| n.content == otrios_interfaces::ViewContent::Label("1 touches".into()))
    );
    fn post_layout(address: &str, body: &[u8], expected: &str, origin: &str) -> u16 {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream,"POST /layout HTTP/1.0\r\nHost: {address}\r\nOrigin: {origin}\r\nContent-Type: application/octet-stream\r\nIf-Match: \"{expected}\"\r\nContent-Length: {}\r\n\r\n",body.len()).unwrap();
        let _ = stream.write_all(body);
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).unwrap();
        std::str::from_utf8(&reply)
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }
    let expected = layout.hex_id();
    let origin = format!("http://{}", server.address);
    assert_eq!(
        post_layout(&server.address, b"bad", &expected, &origin),
        400
    );
    assert_eq!(
        post_layout(
            &server.address,
            layout.bytes(),
            &expected,
            "https://unrelated.example"
        ),
        403
    );
    assert_eq!(
        post_layout(
            &server.address,
            &vec![0; MAX_LAYOUT_BYTES + 1],
            &expected,
            &origin
        ),
        413
    );
    let make = |label: &str| {
        let mut b = layout.blueprint().clone();
        let i = b.root.children.iter_mut().find(|i| i.id == 2).unwrap();
        i.properties.retain(|p| p.key != 1);
        i.properties.push(Property {
            key: 1,
            value: Expr::Literal(Value::Text(label.into())),
        });
        Layout::encode(&b).unwrap()
    };
    let a = make("Draft A");
    let b = make("Draft B");
    let mut statuses = std::thread::scope(|scope| {
        let x = scope.spawn(|| post_layout(&server.address, &a, &expected, &origin));
        let y = scope.spawn(|| post_layout(&server.address, &b, &expected, &origin));
        vec![x.join().unwrap(), y.join().unwrap()]
    });
    statuses.sort();
    assert_eq!(statuses, [200, 412]);
    let persisted = Replica::load(&std::fs::read(&state).unwrap()).unwrap();
    let winner = persisted.snapshot().layout().bytes().to_vec();
    assert!(winner == a || winner == b);
    drop(peer);
    drop(server);
    let restarted = start(&state);
    let url = format!("http://{}/layout", restarted.address);
    cli(&["fetch", &url, file.to_str().unwrap()]);
    assert_eq!(std::fs::read(&file).unwrap(), winner);
    drop(restarted);
    std::fs::remove_dir_all(root).unwrap();
}
