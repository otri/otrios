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
    let content = format!("<p>signed-store fixture {nonce}</p>");
    let raw=serde_json::to_vec(&serde_json::json!({"format":"otrios-release-v1","state_schema":"otrios-canvas-merkle-state-v1","sync_protocol":"otrios-canvas-sync-v2","assets":{"web/index.html":{"bytes":content.len(),"sha256":otrios_core::canvas::hex(&otrios_core::canvas::hash(content.as_bytes()))}}})).unwrap();
    let id = otrios_core::canvas::hex(&otrios_core::canvas::hash(&raw));
    let folder = root.join(&id);
    fs::create_dir_all(folder.join("web")).unwrap();
    fs::write(folder.join("manifest.json"), raw).unwrap();
    fs::write(folder.join("web/index.html"), content).unwrap();
    let r = serde_json::json!({"id":id,"web_entry":format!("/releases/{id}/web/index.html"),"sync_protocol":"otrios-canvas-sync-v2"});
    let mut f = fs::File::create(root.join("pending.json")).unwrap();
    f.write_all(r.to_string().as_bytes()).unwrap();
    drop(f);
    fs::rename(root.join("pending.json"), root.join("current.json")).unwrap();
    r
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
    // Stable alias is metadata only even when a caller asks for state.
    let mut alias = socket(&rig.public, "/release/events");
    hello(&mut alias, 1, "otrios-canvas-sync-v2", true);
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
    assert_eq!(request(&rig.public, "GET", "/apps/stream", &[], &[]).0, 410);
    assert_eq!(request(&rig.public, "GET", "/demo/ink", &[], &[]).0, 410);
}
