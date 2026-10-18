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
        // On Linux a bare connect to an unused ephemeral port can self-connect.
        // Require our persisted state and a real HTTP response, not just an open socket.
        if state.exists()
            && let Ok(mut probe) = TcpStream::connect(&server.address)
        {
            probe
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            probe
                .set_write_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let request = format!("GET /health HTTP/1.0\r\nHost: {}\r\n\r\n", server.address);
            if probe.write_all(request.as_bytes()).is_ok() {
                let mut response = String::new();
                if probe.take(16384).read_to_string(&mut response).is_ok()
                    && response.starts_with("HTTP/1.0 200")
                    && response.contains("\"status\":\"ok\"")
                {
                    return server;
                }
            }
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
        std::fs::write(dir.join("state-worker.js"), "// worker asset").unwrap();
        std::fs::write(dir.join("release-gate.mjs"), "// module handoff fixture").unwrap();
        let recovery = root.join(id).join("native/armv7");
        std::fs::create_dir_all(&recovery).unwrap();
        std::fs::write(recovery.join("dashboard-sdl"), b"signed-recovery-fixture").unwrap();
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
    // Release notices and presentation frames can precede sync acknowledgements.
    // Wait for the durable boundary rather than counting transport messages.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !peer.replica.is_confirmed(&peer.session) {
        peer.pump();
        assert!(
            Instant::now() < deadline,
            "Initial edit was not acknowledged"
        );
    }
    let before = peer.replica.snapshot().id();
    assert_eq!(
        Replica::load(&std::fs::read(&state).unwrap())
            .unwrap()
            .snapshot()
            .id(),
        before,
        "Acknowledged state was not persisted"
    );
    drop(peer);
    let mut peer = Peer::connect(
        &server,
        Replica::load(&std::fs::read(&state).unwrap()).unwrap(),
    );
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = peer.socket.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    }
    let tungstenite::Message::Text(initial) = peer.socket.read().unwrap() else {
        panic!("missing initial release notice");
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&initial).unwrap()["release"]["id"],
        first
    );
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = peer.socket.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
    }
    // Leave the initial binary state frames unread, forcing their delivery
    // ahead of the next code notice rather than relying on scheduler timing.
    publish(&second, "second code");
    let mut state_messages = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(bytes) = peer.replica.generate_sync(&mut peer.session).unwrap() {
            peer.socket
                .send(tungstenite::Message::Binary(bytes.into()))
                .unwrap();
        }
        match peer.socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                let notice: serde_json::Value = serde_json::from_str(&text).unwrap();
                if notice["release"]["id"] == second {
                    break;
                }
            }
            // Code notices share the stream with state and acknowledgements;
            // a fixed number of earlier pumps cannot establish their order.
            Ok(tungstenite::Message::Binary(bytes)) if bytes.starts_with(b"OTML1") => {
                state_messages += 1;
                peer.replica
                    .set_presentation(if bytes.len() == 5 {
                        None
                    } else {
                        Some(&bytes[5..])
                    })
                    .unwrap();
            }
            Ok(tungstenite::Message::Binary(bytes)) => {
                state_messages += 1;
                peer.replica
                    .receive_sync(&mut peer.session, &bytes)
                    .unwrap();
            }
            Ok(tungstenite::Message::Ping(_)) => {
                peer.socket.flush().unwrap();
            }
            Ok(tungstenite::Message::Pong(_)) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            other => panic!("Unexpected release stream message: {other:?}"),
        }
        assert!(Instant::now() < deadline, "release was not pushed");
    }
    assert!(
        state_messages > 0,
        "interleaved state frames were not exercised"
    );
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
    assert!(get(&format!("/releases/{second}/web/state-worker.js")).ends_with("// worker asset"));
    assert!(old.contains("max-age=31536000, immutable"));
    assert!(
        get(&format!("/releases/{second}/web/release-gate.mjs"))
            .ends_with("// module handoff fixture")
    );
    assert!(
        get(&format!("/releases/{second}/native/armv7/dashboard-sdl"))
            .ends_with("signed-recovery-fixture")
    );
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
    a.set_motion(otrios_core::dashboard::motion::Intent {
        playing: false,
        speed: 2.,
        count: 64,
    })
    .unwrap();
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
    assert_eq!(recovered.motion().unwrap(), a.motion().unwrap());
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
            Ok(tungstenite::Message::Binary(bytes)) if bytes.starts_with(b"OTML1") => {
                self.replica
                    .set_presentation(if bytes.len() == 5 {
                        None
                    } else {
                        Some(&bytes[5..])
                    })
                    .unwrap();
            }
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
            .any(|n| n.content == otrios_interfaces::ViewContent::Label("1 touch".into()))
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

#[test]
fn monitoring_presentation_push_restart_and_clear_preserve_the_collaborative_document() {
    use otrios_core::dashboard::layout::{DEFAULT_LAYOUT, Layout};
    let root = std::env::temp_dir().join(format!("otrios-monitoring-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let state = root.join("state.automerge");
    let server = start(&state);
    let before = std::fs::read(&state).unwrap();
    let mut peer = Peer::connect(&server, Replica::new().unwrap());
    for _ in 0..8 {
        peer.pump();
    }
    let mut blueprint = Layout::bootstrap().unwrap().blueprint().clone();
    blueprint.root.children[0].frame[0] = 90.;
    let bytes = Layout::encode(&blueprint).unwrap();
    let post = |server: &Server, bytes: &[u8], origin: &str| {
        let mut stream = TcpStream::connect(&server.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream,"POST /presentation HTTP/1.0\r\nHost: {}\r\nOrigin: {origin}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",server.address,bytes.len()).unwrap();
        let _ = stream.write_all(bytes);
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap()
    };
    let origin = format!("http://{}", server.address);
    assert_eq!(post(&server, &bytes, "https://unrelated.example"), 403);
    assert_eq!(post(&server, &bytes, &origin), 200);
    assert_eq!(post(&server, b"broken", &origin), 400);
    let deadline = Instant::now() + Duration::from_secs(5);
    while peer.replica.presentation().is_none() {
        peer.pump();
        assert!(Instant::now() < deadline);
    }
    assert_eq!(peer.replica.presentation().unwrap().bytes(), bytes);
    assert_eq!(std::fs::read(&state).unwrap(), before);
    assert_eq!(
        std::fs::read(state.with_extension("presentation.flex")).unwrap(),
        bytes
    );
    drop(peer);
    drop(server);
    let server = start(&state);
    let mut peer = Peer::connect(&server, Replica::new().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while peer.replica.presentation().is_none() {
        peer.pump();
        assert!(Instant::now() < deadline);
    }
    assert_eq!(peer.replica.presentation().unwrap().bytes(), bytes);
    assert_eq!(
        post(&server, b"", &format!("http://{}", server.address)),
        200
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while peer.replica.presentation().is_some() {
        peer.pump();
        assert!(Instant::now() < deadline);
    }
    assert_eq!(peer.replica.snapshot().layout().bytes(), DEFAULT_LAYOUT);
    assert_eq!(std::fs::read(&state).unwrap(), before);
    drop(peer);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}
