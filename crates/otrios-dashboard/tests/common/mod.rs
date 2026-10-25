// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use otrios_core::{canvas, dashboard::layout::Layout};
use otrios_interfaces::{canvas::*, layout::Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{Message, client::IntoClientRequest};
#[path = "../../src/session_client.rs"]
pub mod session_client;
pub const ORIGIN: &str = "https://otrios.otri.ca";
pub type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;
pub struct Room {
    pub child: Child,
    pub private: String,
    pub public: String,
    pub root: PathBuf,
    pub catalog: Vec<u8>,
    pub schema: StateSchema,
}
impl Drop for Room {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
pub fn port() -> String {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .to_string()
}
pub fn fixture(root: &std::path::Path) -> (Vec<u8>, StateSchema) {
    let schema = StateSchema {
        id: "test-demo-v2".into(),
        fields: vec![Field {
            id: 1,
            name: "level".into(),
            default: Value::Number(0.),
            writable: true,
        }],
    };
    let mut entries = vec![];
    for id in ["demo", "monitoring"] {
        let app = App {
            id: id.into(),
            title: id.into(),
            ui_ref: format!("app:{id}/ui"),
            state_ref: format!("app:{id}/state"),
            status: "canvas-app".into(),
            dependencies: vec![],
        };
        let mut fields = schema.clone();
        if id == "monitoring" {
            fields.fields[0].writable = false;
        }
        let ui = Ui {
            format: "otrios-canvas-v2".into(),
            ui_ref: app.ui_ref.clone(),
            title: id.into(),
            state_schema: fields,
            layout: empty_layout().bytes().to_vec(),
            // Tests read/edit this field; declare it as a view dependency so
            // selective replication does not intentionally omit it.
            selectors: vec![Selector {
                local: 1,
                field: 1,
                projection: Projection::Identity,
            }],
            handlers: vec![],
            navigation: vec![],
            behavior: None,
            presentation: None,
        };
        let bytes = canvas::encode_ui(&ui).unwrap();
        let folder = root.join(id).join("package");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("ui.flex"), &bytes).unwrap();
        std::fs::write(folder.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
        entries.push(CatalogEntry {
            path: id.into(),
            app,
            ui_hash: canvas::hash(&bytes),
            private: false,
        });
    }
    let bytes = canvas::encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: entries,
    })
    .unwrap();
    std::fs::write(root.join("catalog.flex"), &bytes).unwrap();
    (bytes, schema)
}
pub fn room(auth: bool, seconds: u64) -> Room {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "otrios-app-test-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));

    std::fs::create_dir_all(&root).unwrap();
    let (catalog, schema) = fixture(&root.join("apps"));
    // Binding port 0 in the child keeps ownership with the actual listener.
    // A bind/drop/rebind "free port" probe races other parallel fixtures.
    let private = "127.0.0.1:0";
    let public = "127.0.0.1:0";
    let mut command = Command::new(env!("CARGO_BIN_EXE_dashboard-server"));
    command.args([
        "--bind",
        private,
        "--public-bind",
        public,
        "--apps",
        root.join("apps").to_str().unwrap(),
        "--state",
        root.join("state.merkle").to_str().unwrap(),
        "--origin",
        ORIGIN,
    ]);
    if auth {
        use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
        let salt = SaltString::encode_b64(b"test-only-otrios-salt").unwrap();
        let hash = Argon2::default()
            .hash_password(b"test-only-password", &salt)
            .unwrap()
            .to_string();
        let path = root.join("auth.json");
        std::fs::write(
            &path,
            serde_json::json!({"username":"test-user","password_hash":hash}).to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        command.args([
            "--auth-file",
            path.to_str().unwrap(),
            "--session-seconds",
            &seconds.to_string(),
        ]);
    }
    let mut child = command.stdout(Stdio::piped()).spawn().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut addresses = Vec::new();
    for label in [
        "OTriOS public listener: http://",
        "OTriOS app authority: http://",
    ] {
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "authority exited before binding"
        );
        addresses.push(
            line.trim()
                .strip_prefix(label)
                .expect("listener announcement")
                .trim_end_matches('/')
                .to_owned(),
        );
    }
    child.stdout = Some(output.into_inner());
    let public = addresses.remove(0);
    let private = addresses.remove(0);
    assert_ne!(public, private);
    let mut room = Room {
        child,
        private,
        public,
        root,
        catalog,
        schema,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(&room.public).is_ok()
            && request(&room.public, "GET", "/health", &[], &[]).0 == 200
        {
            break;
        }
        assert!(room.child.try_wait().unwrap().is_none(), "authority exited");
        assert!(Instant::now() < deadline, "authority startup timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    room
}
pub fn response(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<u8>, String) {
    let mut tcp = TcpStream::connect(address).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        tcp,
        "{method} {path} HTTP/1.0\r\nHost: {address}\r\nContent-Length: {}\r\n",
        body.len()
    )
    .unwrap();
    for (name, value) in headers {
        write!(tcp, "{name}: {value}\r\n").unwrap();
    }
    tcp.write_all(b"\r\n").unwrap();
    tcp.write_all(body).unwrap();
    let mut bytes = vec![];
    tcp.read_to_end(&mut bytes).unwrap();
    let split = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let headers = String::from_utf8(bytes[..split].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, bytes[split + 4..].to_vec(), headers)
}
pub fn request(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<u8>) {
    let (s, b, _) = response(address, method, path, headers, body);
    (s, b)
}
pub fn socket(address: &str, cookie: Option<&str>) -> Socket {
    let mut request = format!("ws://{address}/session")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("Cookie", cookie.parse().unwrap());
    }
    let (mut socket, _) = tungstenite::connect(request).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(t) = socket.get_mut() {
        t.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    }
    session_client::negotiate(&mut socket).unwrap();
    socket
}
pub fn send(s: &mut Socket, m: ClientMessage) {
    s.send(Message::Binary(
        otrios_interfaces::session::frame(1, canvas::encode_client(&m).unwrap()).into(),
    ))
    .unwrap();
}
pub fn read(s: &mut Socket) -> ServerMessage {
    loop {
        match s.read().unwrap() {
            Message::Binary(b) => {
                return canvas::decode_server(b.strip_prefix(&[1]).expect("state channel"))
                    .unwrap();
            }
            Message::Text(_) | Message::Pong(_) => {}
            Message::Ping(_) => s.flush().unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
}
pub fn mount(s: &mut Socket, room: &Room, id: &str, epoch: u32) {
    send(
        s,
        ClientMessage::Mount {
            epoch,
            ui_ref: format!("app:{id}/ui"),
            state_ref: format!("app:{id}/state"),
            catalog_hash: canvas::hash(&room.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![9; 16],
            resume: 0,
        },
    );
}
pub fn login(room: &Room, old: Option<&str>) -> (u16, String) {
    let body = br#"{"username":"test-user","password":"test-only-password"}"#;
    let mut headers = vec![
        ("Origin", ORIGIN),
        ("X-OTriOS-Request", "session"),
        ("Content-Type", "application/json"),
    ];
    if let Some(cookie) = old {
        headers.push(("Cookie", cookie));
    }
    let (status, _, headers) = response(&room.public, "POST", "/auth/session", &headers, body);
    (status, headers)
}
pub fn cookie(room: &Room) -> String {
    let (status, headers) = login(room, None);
    assert_eq!(status, 200);
    let line = headers
        .lines()
        .find(|s| s.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap();
    for attribute in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(line.contains(attribute));
    }
    line.split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}
fn empty_layout() -> Layout {
    use otrios_interfaces::layout::*;
    let number = |n| Expr::Literal(Value::Number(n));
    let blueprint = Blueprint {
        composition: None,
        schema: "otrios-layout-v2".into(),
        templates: vec![Template {
            id: 1,
            name: "Test canvas".into(),
            parameters: vec![],
            root: TemplateNode {
                id: 0,
                frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                visible: Expr::Literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                resize: None,
                responder: None,
                semantics: None,
                children: vec![],
            },
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    };
    Layout::from_bytes(&Layout::encode(&blueprint).unwrap()).unwrap()
}
pub fn view(room: &Room) -> Vec<u8> {
    let mut blueprint = empty_layout().blueprint().clone();
    blueprint.root.children.clear();
    let bytes = Layout::encode(&blueprint).unwrap();
    assert_eq!(
        request(
            &room.private,
            "POST",
            "/presentation",
            &[
                ("Content-Type", "application/octet-stream"),
                ("Origin", ORIGIN)
            ],
            &bytes
        )
        .0,
        200
    );
    bytes
}

pub fn state_file(room: &Room, id: &str) -> PathBuf {
    state_file_ref(room, id, &format!("app:{id}/state"))
}
pub fn state_file_ref(room: &Room, id: &str, state_ref: &str) -> PathBuf {
    room.root.join("state.apps").join(format!(
        "{id}-{}.merkle",
        canvas::hex(&canvas::hash(state_ref.as_bytes()))
    ))
}
pub fn accept(s: &mut Socket, epoch: u32) {
    send(s, ClientMessage::AcceptMount { epoch });
}

pub fn stored(room: &Room, schema: StateSchema, scope: &str, id: &str) -> canvas::Document {
    let file = state_file_ref(room, id, scope);
    stored_file(&file, schema, scope)
}
pub fn stored_file(file: &std::path::Path, schema: StateSchema, scope: &str) -> canvas::Document {
    let mut doc = canvas::Document::load(schema, scope, &std::fs::read(file).unwrap()).unwrap();
    let path = file.with_extension("merkle.log");
    if let Ok(bytes) = std::fs::read(path) {
        let mut at = 0;
        while at < bytes.len() {
            if bytes.len() - at < 36 {
                break;
            }
            let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            if at + 36 + len > bytes.len() {
                break;
            }
            doc.replay(&bytes[at + 36..at + 36 + len]).unwrap();
            at += 36 + len;
        }
    }
    doc
}
