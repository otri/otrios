// SPDX-License-Identifier: Apache-2.0
#![cfg(all(feature = "canvas", feature = "server", feature = "host-sync"))]
//! Real TCP/WebSocket boundary, persistence/restart, app scoping and native guest execution.
mod common;
use common::session_client;
use otrios_core::canvas::*;
use otrios_interfaces::canvas::*;
use std::{
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};
struct Server {
    child: Child,
    root: PathBuf,
    port: u16,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Server {
    fn start() -> Self {
        let root =
            std::env::temp_dir().join(format!("otrios-canvas-process-{}", std::process::id()));
        std::fs::create_dir_all(root.join("apps")).unwrap();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        // Isolated package outputs: never race the user's package builder/live source watcher.
        for app in [
            "home",
            "test-playground",
            "layout-document",
            "layout-text-scroll",
            "component-playground",
            "layout-lab",
            "input-lab",
            "animation-lab",
            "three-d-lab",
            "monitoring",
        ] {
            let source = repo.join("apps").join(app);
            let dest = root.join("apps").join(app);
            copy_sources(&source, &dest);
        }
        assert!(
            Command::new(env!("CARGO_BIN_EXE_canvas-pack"))
                .arg(root.join("apps"))
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        // Add an editable child exclusively to this disposable fixture. The
        // shipped read-only scroll demo has no State fields to exercise here.
        let mut catalog =
            decode_catalog(&std::fs::read(root.join("apps/catalog.flex")).unwrap()).unwrap();
        assert_eq!(catalog.apps.len(), 10);
        assert_eq!(
            catalog
                .apps
                .iter()
                .filter(|entry| entry.path.contains('/'))
                .map(|entry| entry.path.as_str())
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "test-playground/components",
                "test-playground/layouts",
                "test-playground/text-input",
                "test-playground/animations-two-d",
                "test-playground/animations-three-d",
                "test-playground/long-document",
                "test-playground/text-panning"
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        );
        let parent = catalog
            .apps
            .iter()
            .find(|entry| entry.app.id == "layout-lab")
            .unwrap()
            .clone();
        let mut child =
            decode_ui(&std::fs::read(root.join("apps/layout-lab/package/ui.flex")).unwrap())
                .unwrap();
        child.ui_ref = "app:layout-lab/test-detail/ui".into();
        let mut app = parent.app;
        app.id = "layout-lab/test-detail".into();
        app.ui_ref = child.ui_ref.clone();
        let bytes = encode_ui(&child).unwrap();
        let folder = root.join("apps/layout-lab/test-detail/package");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("ui.flex"), &bytes).unwrap();
        std::fs::write(folder.join("app.flex"), encode_app(&app).unwrap()).unwrap();
        catalog.apps.push(CatalogEntry {
            app,
            path: "test-playground/layouts/test-detail".into(),
            ui_hash: hash(&bytes),
            private: false,
        });
        std::fs::write(
            root.join("apps/catalog.flex"),
            encode_catalog(&catalog).unwrap(),
        )
        .unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let child = spawn(&root, port);
        let s = Self { child, root, port };
        s.ready();
        s
    }
    fn ready(&self) {
        let start = Instant::now();
        while TcpStream::connect(("127.0.0.1", self.port)).is_err() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn restart(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        self.child = spawn(&self.root, self.port);
        self.ready();
    }
    fn peer(&self, ui: &str, state: &str) -> Peer {
        let (mut ws, _) =
            tungstenite::connect(format!("ws://127.0.0.1:{}/session", self.port)).unwrap();
        if let MaybeTlsStream::Plain(tcp) = ws.get_mut() {
            tcp.set_read_timeout(Some(Duration::from_millis(30)))
                .unwrap();
        }
        session_client::negotiate(&mut ws).unwrap();
        let mut c = Client::default();
        c.admit_catalog(&std::fs::read(self.root.join("apps/catalog.flex")).unwrap())
            .unwrap();
        c.open(ui, state);
        let mut p = Peer { ws, c };
        p.settle();
        p
    }
}
fn copy_sources(src: &std::path::Path, dest: &std::path::Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.file_name().unwrap() == "package" {
            continue;
        }
        let out = dest.join(entry.file_name());
        if path.is_dir() {
            copy_sources(&path, &out)
        } else {
            std::fs::copy(path, out).unwrap();
        }
    }
}
fn spawn(root: &std::path::Path, port: u16) -> Child {
    Command::new(env!("CARGO_BIN_EXE_canvas-server"))
        .args(["--bind", &format!("127.0.0.1:{port}"), "--apps"])
        .arg(root.join("apps"))
        .arg("--app-state")
        .arg(root.join("state"))
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}
fn edit(c: &mut Client, label: &str, value: u16) {
    let scene = c.scene().unwrap();
    let node = scene
        .controls()
        .iter()
        .find(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == label))
        .unwrap()
        .node
        .id;
    c.event_for_node(c.epoch(), node, value, 0).unwrap();
}
struct Peer {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    c: Client,
}
impl Peer {
    fn tick(&mut self) -> bool {
        self.c
            .tick(
                1000 + std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                0,
            )
            .unwrap();
        while let Some(b) = self.c.outgoing().unwrap() {
            self.ws
                .send(Message::Binary(
                    otrios_interfaces::session::frame(1, b).into(),
                ))
                .unwrap();
        }
        match self.ws.read() {
            Ok(Message::Text(_)) => true,
            Ok(Message::Ping(b)) => {
                self.ws.send(Message::Pong(b)).unwrap();
                true
            }
            Ok(Message::Binary(b)) => {
                self.c.receive(b.strip_prefix(&[1]).unwrap()).unwrap();
                true
            }
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                false
            }
            other => panic!("unexpected socket result {other:?}"),
        }
    }
    fn settle(&mut self) {
        let now = Instant::now();
        loop {
            let received = self.tick();
            if !received && !self.c.ui_ref().is_empty() && self.c.confirmed() {
                return;
            }
            assert!(
                now.elapsed() < Duration::from_secs(10),
                "Canvas did not settle"
            );
        }
    }
}
#[test]
fn real_process_scoping_persistence_reload_and_native_probe() {
    let mut server = Server::start();
    let mut a = server.peer(
        "app:component-playground/ui",
        "app:component-playground/state",
    );
    let mut b = server.peer(
        "app:component-playground/ui",
        "app:component-playground/state",
    );
    assert!(
        !a.tick(),
        "another writer's durable admission receipt must not enter this lease"
    );
    let mut layout = server.peer("app:layout-lab/ui", "app:layout-lab/state");
    edit(&mut a.c, "Linear slider", 810);
    a.settle();
    b.settle();
    assert_eq!(b.c.value(1).unwrap(), 800);
    assert!(
        !layout.tick(),
        "Inactive app must receive zero messages for Playground edits"
    );
    assert_eq!(layout.c.value(1).unwrap(), 500);
    let read_app = |id: &str| {
        let state_ref = format!("app:{id}/state");
        let ui = decode_ui(
            &std::fs::read(server.root.join(format!("apps/{id}/package/ui.flex"))).unwrap(),
        )
        .unwrap();
        let file = server
            .root
            .join("state")
            .join(format!("{id}-{}.merkle", hex(&hash(state_ref.as_bytes()))));
        common::stored_file(&file, ui.state_schema, &state_ref)
    };
    let wall = otrios_core::dashboard::clock::wall_time_seconds();
    assert!(
        read_app("component-playground")
            .signal(1, wall, 300)
            .is_empty()
    );
    edit(&mut layout.c, "Toggle", 0);
    layout.settle();
    let wall = otrios_core::dashboard::clock::wall_time_seconds();
    let history = read_app("layout-lab");
    assert!(
        !history.signal(3, wall + 1, 300).is_empty(),
        "The admitted Toggle signal must retain samples"
    );
    assert!(
        history.signal(1, wall + 1, 300).is_empty(),
        "Panel width does not request history"
    );

    // Nested screens mount their own UI against the same admitted durable State.
    let mut cards = server.peer("app:layout-lab/test-detail/ui", "app:layout-lab/state");
    assert_eq!(cards.c.path(), "test-playground/layouts/test-detail");
    edit(&mut cards.c, "Panel width", 720);
    cards.settle();
    layout.settle();
    assert_eq!(layout.c.value(1).unwrap(), 720);
    cards.c.back().unwrap();
    cards.settle();
    assert_eq!(cards.c.path(), "test-playground/layouts");
    assert_eq!(cards.c.value(1).unwrap(), 720);
    drop(cards);

    // Same durable root, changed executable: no host rebuild or authority restart.
    let source = server
        .root
        .join("apps/component-playground/behavior/main.wat");
    let text = std::fs::read_to_string(&source).unwrap();
    std::fs::write(source, {
        let start = text.find(";; linear-value-begin").unwrap() + ";; linear-value-begin".len();
        let end = text.find(";; linear-value-end").unwrap();
        format!("{}\n i32.const 123\n {}", &text[..start], &text[end..])
    })
    .unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_canvas-pack"))
            .arg(server.root.join("apps"))
            .stdout(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    a.c.admit_catalog(&std::fs::read(server.root.join("apps/catalog.flex")).unwrap())
        .unwrap();
    a.c.open(
        "app:component-playground/ui",
        "app:component-playground/state",
    );
    a.settle();
    edit(&mut a.c, "Linear slider", 999);
    a.settle();
    assert_eq!(a.c.value(1).unwrap(), 123, "New guest bytes must execute");
    drop(a);
    drop(b);
    drop(layout);
    server.restart();
    let c = server.peer(
        "app:component-playground/ui",
        "app:component-playground/state",
    );
    assert_eq!(c.c.value(1).unwrap(), 123);
    drop(c);
    let probe = Command::new(env!("CARGO_BIN_EXE_canvas-native"))
        .arg(format!("ws://127.0.0.1:{}/session", server.port))
        .arg(server.root.join("apps/catalog.flex"))
        .output()
        .unwrap();
    assert!(
        probe.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    assert!(String::from_utf8_lossy(&probe.stdout).contains("acknowledged=true"));
    // Unknown pair is denied by server policy before any UI/State data.
    let (mut bad, _) =
        tungstenite::connect(format!("ws://127.0.0.1:{}/session", server.port)).unwrap();
    session_client::negotiate(&mut bad).unwrap();
    bad.send(Message::Binary(
        otrios_interfaces::session::frame(
            1,
            encode_client(&ClientMessage::Mount {
                epoch: 1,
                ui_ref: "app:layout-lab/ui".into(),
                state_ref: "app:component-playground/state".into(),
                known_ui: None,
                known_code: vec![],
                writer: vec![9; 16],
                resume: 0,

                catalog_hash: hash(&std::fs::read(server.root.join("apps/catalog.flex")).unwrap()),
            })
            .unwrap(),
        )
        .into(),
    ))
    .unwrap();
    let bytes = loop {
        match bad.read().unwrap() {
            Message::Binary(b) => break b,
            Message::Text(_) | Message::Pong(_) => {}
            Message::Ping(bytes) => bad.send(Message::Pong(bytes)).unwrap(),
            other => panic!("Unexpected {other:?}"),
        }
    };
    assert!(matches!(
        decode_server(bytes.strip_prefix(&[1]).unwrap()).unwrap(),
        ServerMessage::Error { .. }
    ));
    // Reject browser origins outside the exact loopback viewer.
    use tungstenite::client::IntoClientRequest;
    let mut request = format!("ws://127.0.0.1:{}/session", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", "https://other.example".parse().unwrap());
    assert!(
        matches!(tungstenite::connect(request),Err(tungstenite::Error::Http(r)) if r.status()==403)
    );
}
