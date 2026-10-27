// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "server")]
mod common;
use common::{ORIGIN, port, request};
use otrios_interfaces::session::{ClientControl, ServerControl};
use std::{
    fs,
    io::Write,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tungstenite::{Message, client::IntoClientRequest};
struct Rig {
    child: Child,
    root: PathBuf,
    public: String,
    private: String,
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn publish(root: &Path, nonce: u8) -> serde_json::Value {
    let content = format!(
        r#"<title>OTriOS</title><meta property="og:title" content="OTriOS"><meta property="og:site_name" content="OTriOS"><link rel="stylesheet" href="./style.css"><script type="module" src="./app.js"></script><p>signed-store fixture {nonce}</p>"#
    );
    let (catalog, _) = common::fixture(&root.join("catalog-source"));
    let mut decoded = otrios_core::canvas::decode_catalog(&catalog).unwrap();
    let mut child = decoded.apps[0].clone();
    child.app.id = "demo/child".into();
    child.path = "demo/child".into();
    child.app.ui_ref = "app:demo/child/ui".into();
    child.app.title = "Steam & \"Tea\" <Relay>".into();
    decoded.apps.push(child);
    let catalog = otrios_core::canvas::encode_catalog(&decoded).unwrap();
    let touch = include_bytes!("../../../web/canvas/touch-input.mjs");
    let asset = |bytes: &[u8]| serde_json::json!({"bytes":bytes.len(),"sha256":otrios_core::canvas::hex(&otrios_core::canvas::hash(bytes))});
    let raw=serde_json::to_vec(&serde_json::json!({"format":"otrios-release-v1","state_schema":"otrios-canvas-merkle-state-v1","sync_protocol":"otrios-canvas-sync-v7","assets":{"web/index.html":asset(content.as_bytes()),"web/touch-input.mjs":asset(touch),"apps/catalog.flex":asset(&catalog)}})).unwrap();
    let id = otrios_core::canvas::hex(&otrios_core::canvas::hash(&raw));
    let folder = root.join(&id);
    fs::create_dir_all(folder.join("web")).unwrap();
    fs::write(folder.join("manifest.json"), raw).unwrap();
    fs::write(folder.join("web/index.html"), content).unwrap();
    fs::write(folder.join("web/touch-input.mjs"), touch).unwrap();
    fs::create_dir_all(folder.join("apps")).unwrap();
    fs::write(folder.join("apps/catalog.flex"), catalog).unwrap();
    let r = serde_json::json!({"id":id,"web_entry":format!("/releases/{id}/web/index.html"),"sync_protocol":"otrios-canvas-sync-v7"});
    let mut f = fs::File::create(root.join("pending.json")).unwrap();
    f.write_all(r.to_string().as_bytes()).unwrap();
    drop(f);
    fs::rename(root.join("pending.json"), root.join("current.json")).unwrap();
    r
}
fn complete_release(root: &Path) -> Vec<u8> {
    let source = root.join("source");
    let (catalog, _) = common::fixture(&source);
    let mut files = std::collections::BTreeMap::new();
    files.insert("web/index.html".to_string(), b"complete fixture".to_vec());
    files.insert("apps/catalog.flex".to_string(), catalog.clone());
    for app in ["demo", "monitoring"] {
        for name in ["ui.flex", "app.flex"] {
            files.insert(
                format!("authority/apps/{app}/{name}"),
                fs::read(source.join(app).join("package").join(name)).unwrap(),
            );
        }
    }
    let mut assets = serde_json::Map::new();
    let mut authority = serde_json::Map::new();
    for (name, bytes) in &files {
        let item = serde_json::json!({"bytes":bytes.len(),"sha256":otrios_core::canvas::hex(&otrios_core::canvas::hash(bytes))});
        if name.starts_with("authority/") {
            authority.insert(name.clone(), item);
        } else {
            assets.insert(name.clone(), item);
        }
    }
    let raw=serde_json::json!({"format":"otrios-release-v1","state_schema":"otrios-canvas-merkle-state-v1","sync_protocol":"otrios-canvas-sync-v7","assets":assets,"authority":authority}).to_string().into_bytes();
    let id = otrios_core::canvas::hex(&otrios_core::canvas::hash(&raw));
    let folder = root.join(&id);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("manifest.json"), raw).unwrap();
    for (name, bytes) in files {
        let path = folder.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let r = serde_json::json!({"id":id,"web_entry":format!("/releases/{id}/web/index.html"),"sync_protocol":"otrios-canvas-sync-v7"});
    fs::write(root.join("pending.json"), r.to_string()).unwrap();
    fs::rename(root.join("pending.json"), root.join("current.json")).unwrap();
    catalog
}
fn socket(address: &str, path: &str) -> common::Socket {
    let mut req = format!("ws://{address}{path}")
        .into_client_request()
        .unwrap();
    req.headers_mut().insert("Origin", ORIGIN.parse().unwrap());
    let (mut s, _) = tungstenite::connect(req).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(t) = s.get_mut() {
        t.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    }
    s
}
fn hello(s: &mut common::Socket, version: u32, protocol: &str, state: bool) {
    s.send(Message::Text(
        serde_json::to_string(&ClientControl::Hello {
            version,
            release: None,
            state_protocol: protocol.into(),
            state,
        })
        .unwrap()
        .into(),
    ))
    .unwrap();
}
fn event(s: &mut common::Socket) -> ServerControl {
    loop {
        match s.read().unwrap() {
            Message::Text(t) => return serde_json::from_str(&t).unwrap(),
            Message::Ping(_) => s.flush().unwrap(),
            other => panic!("state before compatible negotiation: {other:?}"),
        }
    }
}
#[test]
fn code_and_hello_survive_failed_state_recovery_and_obsolete_codecs() {
    let root = std::env::temp_dir().join(format!(
        "otrios-unified-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let a = publish(&root, 1);
    let public = port();
    let private = port();
    // The installed store intentionally lacks its state catalog: recovery fails.
    let child = Command::new(env!("CARGO_BIN_EXE_dashboard-server"))
        .args([
            "--bind",
            &private,
            "--public-bind",
            &public,
            "--origin",
            ORIGIN,
            "--releases",
            root.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let rig = Rig {
        child,
        root,
        public,
        private,
    };
    let until = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(&rig.public).is_err() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    for address in [&rig.public, &rig.private] {
        let path = format!(
            "/releases/{}/web/touch-input.mjs",
            a["id"].as_str().unwrap()
        );
        let (status, bytes) = request(address, "GET", &path, &[], &[]);
        assert_eq!(status, 200);
        assert_eq!(bytes, include_bytes!("../../../web/canvas/touch-input.mjs"));
    }
    assert_eq!(
        request(
            &rig.public,
            "GET",
            &format!("/releases/{}/web/index.html", a["id"].as_str().unwrap()),
            &[],
            &[]
        )
        .0,
        200
    );
    // Navigation is a no-store shell even while app-state recovery is failing.
    let (status, entry, headers) = common::response(&rig.public, "GET", "/", &[], &[]);
    assert_eq!(status, 200);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("cache-control: no-store")
    );
    let entry_text = String::from_utf8(entry.clone()).unwrap();
    assert!(entry_text.contains("<title>OTriOS</title>"));
    assert!(entry_text.contains(&format!(
        "src=\"/releases/{}/web/app.js\"",
        a["id"].as_str().unwrap()
    )));
    assert!(entry_text.contains(&format!(
        "href=\"/releases/{}/web/style.css\"",
        a["id"].as_str().unwrap()
    )));
    for screen in ["demo", "demo/child", "monitoring"] {
        let name = if screen == "demo/child" {
            "Steam &amp; &quot;Tea&quot; &lt;Relay&gt;"
        } else {
            screen
        };
        for address in [&rig.public, &rig.private] {
            let (status, bytes) = request(address, "GET", &format!("/{screen}"), &[], &[]);
            assert_eq!(status, 200);
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.contains(&format!("<title>OTriOS · {name}</title>")));
            assert!(text.contains(&format!("property=\"og:title\" content=\"{name}\"")));
            assert!(text.contains("property=\"og:site_name\" content=\"OTriOS\""));
        }
        assert_eq!(
            request(&rig.public, "HEAD", &format!("/{screen}"), &[], &[]),
            (200, vec![])
        );
    }
    assert_eq!(request(&rig.public, "GET", "/not-an-app", &[], &[]).0, 404);
    assert_eq!(request(&rig.public, "GET", "/demo/input", &[], &[]).0, 404);
    assert_eq!(request(&rig.public, "POST", "/demo", &[], &[]).0, 404);
    let (status, original, headers) = common::response(
        &rig.public,
        "GET",
        a["web_entry"].as_str().unwrap(),
        &[],
        &[],
    );
    assert_eq!(status, 200);
    assert!(
        String::from_utf8(original)
            .unwrap()
            .contains("src=\"./app.js\"")
    );
    assert!(headers.contains("immutable"));
    assert_eq!(
        request(&rig.public, "GET", "/admin/activate", &[], &[]).0,
        404
    );
    assert_eq!(
        request(&rig.public, "POST", "/admin/activate", &[], &[]).0,
        404
    );
    let mut old = socket(&rig.public, "/session");
    hello(&mut old, 999, "retired-crdt-v0", true);
    match event(&mut old) {
        ServerControl::Hello {
            version,
            release,
            compatible,
            state_available,
            refresh,
        } => {
            assert_eq!(version, 1);
            assert_eq!(release.unwrap().id, a["id"]);
            assert!(!compatible);
            assert!(!state_available);
            assert!(refresh)
        }
        other => panic!("{other:?}"),
    }
    // Opening control v1 still supplies the new release to the previous live
    // state codec. Its state stream remains blocked until the client upgrades.
    let mut previous_state = socket(&rig.public, "/session");
    hello(&mut previous_state, 1, "otrios-canvas-sync-v4", true);
    assert!(matches!(
        event(&mut previous_state),
        ServerControl::Hello {
            version: 1,
            release: Some(_),
            compatible: false,
            refresh: true,
            ..
        }
    ));
    let b = publish(&rig.root, 2);
    assert_eq!(
        request(&rig.private, "POST", "/admin/activate", &[], &[]).0,
        204
    );
    assert_eq!(event(&mut old).release().unwrap().id, b["id"]);
    // Lost explicit notification: reconnect rereads the current pointer.
    let c = publish(&rig.root, 3);
    let mut reconnect = socket(&rig.public, "/session");
    hello(&mut reconnect, 1, "unknown", true);
    assert_eq!(event(&mut reconnect).release().unwrap().id, c["id"]);
    assert_eq!(event(&mut old).release().unwrap().id, c["id"]);
    fs::write(rig.root.join("current.json"), b"invalid").unwrap();
    assert_eq!(
        request(&rig.private, "POST", "/admin/activate", &[], &[]).0,
        503
    );
    old.send(Message::Binary(vec![1, 0].into())).unwrap();
    assert!(!matches!(old.read(), Ok(Message::Binary(_))));
    #[cfg(feature = "host-sync")]
    {
        publish(&rig.root, 3);
        let result = Command::new(env!("CARGO_BIN_EXE_session-bootstrap"))
            .args([&format!("ws://{}/session", rig.private), ""])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["release"]["id"], c["id"]);
    }
    // Stable alias is metadata only even when a caller asks for state.
    let mut alias = socket(&rig.public, "/release/events");
    hello(&mut alias, 1, "otrios-canvas-sync-v7", true);
    assert!(matches!(
        event(&mut alias),
        ServerControl::Hello {
            compatible: true,
            ..
        }
    ));
    alias.send(Message::Binary(vec![1, 0].into())).unwrap();
    assert!(!matches!(alias.read(), Ok(Message::Binary(_))));
    let mut early = socket(&rig.public, "/session");
    early.send(Message::Binary(vec![1, 0].into())).unwrap();
    assert!(!matches!(early.read(), Ok(Message::Binary(_))));
    // A later complete publication recovers state without replacing the process/socket.
    let mut waiting = socket(&rig.public, "/session");
    hello(&mut waiting, 1, "otrios-canvas-sync-v7", true);
    assert!(matches!(
        event(&mut waiting),
        ServerControl::Hello {
            state_available: false,
            ..
        }
    ));
    let catalog = complete_release(&rig.root);
    assert_eq!(
        request(&rig.private, "POST", "/admin/activate", &[], &[]).0,
        204
    );
    loop {
        if matches!(
            event(&mut waiting),
            ServerControl::StateStatus {
                available: true,
                ..
            }
        ) {
            break;
        }
    }
    common::send(
        &mut waiting,
        otrios_interfaces::canvas::ClientMessage::Mount {
            epoch: 1,
            ui_ref: "app:demo/ui".into(),
            state_ref: "app:demo/state".into(),
            catalog_hash: otrios_core::canvas::hash(&catalog),
            writer: vec![7; 16],
            resume: 0,
            known_ui: None,
            known_code: vec![],
        },
    );
    assert!(matches!(
        common::read(&mut waiting),
        otrios_interfaces::canvas::ServerMessage::Mounted { epoch: 1, .. }
    ));
    assert_eq!(request(&rig.public, "GET", "/apps/stream", &[], &[]).0, 410);
    assert_eq!(request(&rig.public, "GET", "/demo/ink", &[], &[]).0, 410);
}

#[test]
fn ink_subscriptions_follow_peer_admission_and_release_their_slots() {
    let room = common::room(false, 60);
    let mut peers = Vec::new();
    for _ in 0..24 {
        let mut peer = socket(&room.public, "/session");
        common::session_client::negotiate(&mut peer).unwrap();
        peer.send(Message::Text(
            serde_json::to_string(&ClientControl::Ink { enabled: true })
                .unwrap()
                .into(),
        ))
        .unwrap();
        let Message::Binary(bytes) = peer.read().unwrap() else {
            panic!("Every admitted session must receive its ink welcome, including peers after 16")
        };
        let payload = bytes
            .strip_prefix(&[otrios_interfaces::session::INK])
            .unwrap();
        assert!(
            otrios_interfaces::ink::decode_server(payload)
                .unwrap()
                .2
                .is_none()
        );
        peers.push(peer);
    }
    let health = || {
        serde_json::from_slice::<serde_json::Value>(
            &request(&room.private, "GET", "/health", &[], &[]).1,
        )
        .unwrap()
    };
    assert_eq!(health()["ink_limit"], 96);
    assert_eq!(health()["ink_subscribers"], 24);
    let point = otrios_interfaces::ink::Point {
        phase: 0,
        stroke: 1,
        position: [123, 456],
    };
    peers[0]
        .send(Message::Binary(
            otrios_interfaces::session::frame(otrios_interfaces::session::INK, point.encode())
                .into(),
        ))
        .unwrap();
    for peer in &mut peers {
        let Message::Binary(bytes) = peer.read().unwrap() else {
            panic!("Ink must reach every subscriber")
        };
        let payload = bytes
            .strip_prefix(&[otrios_interfaces::session::INK])
            .unwrap();
        assert_eq!(
            otrios_interfaces::ink::decode_server(payload).unwrap().2,
            Some(point)
        );
    }
    peers[0]
        .send(Message::Text(
            serde_json::to_string(&ClientControl::Ink { enabled: false })
                .unwrap()
                .into(),
        ))
        .unwrap();
    peers[0].send(Message::Ping(vec![7].into())).unwrap();
    assert!(matches!(peers[0].read().unwrap(), Message::Pong(_)));
    assert_eq!(health()["ink_subscribers"], 23);
    drop(peers);
    let until = Instant::now() + Duration::from_secs(3);
    while health()["ink_subscribers"] != 0 {
        assert!(
            Instant::now() < until,
            "Disconnected subscriptions must release permits"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
