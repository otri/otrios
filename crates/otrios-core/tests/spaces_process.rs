// SPDX-License-Identifier: LGPL-3.0-only
//! Real authority plus separately spawned clients. Run through scripts/check.sh,
//! which supplies its freshly built authority as OTRIOS_SPACES_SERVER.
#![cfg(feature = "canvas")]
use otrios_core::{canvas, dashboard::layout::Layout};
use otrios_interfaces::{
    canvas::*,
    layout::Value,
    spaces::{Change, Request, Role},
};
use serde_json::{Value as Json, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{Message, client::IntoClientRequest, stream::MaybeTlsStream};
const ORIGIN: &str = "https://spaces.test";
type Socket = tungstenite::WebSocket<MaybeTlsStream<TcpStream>>;
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Rig {
    server: Guard,
    root: PathBuf,
    public: String,
    native: String,
    catalog: Vec<u8>,
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.server.0.kill();
        let _ = self.server.0.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn schema() -> StateSchema {
    StateSchema {
        id: "spaces-process-v1".into(),
        fields: vec![Field {
            id: 1,
            name: "counter".into(),
            default: Value::Number(0.),
            writable: true,
        }],
    }
}
fn fixture(root: &Path) -> Vec<u8> {
    use otrios_interfaces::layout::*;
    let layout = Layout::encode(&Blueprint {
        schema: "otrios-layout-v2".into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "Counter".into(),
            parameters: vec![],
            root: TemplateNode {
                id: 0,
                frame: [
                    Expr::Literal(Value::Number(0.)),
                    Expr::Literal(Value::Number(0.)),
                    Expr::Width,
                    Expr::Height,
                ],
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
    })
    .unwrap();
    let app = App {
        id: "counter".into(),
        title: "Counter".into(),
        ui_ref: "app:counter/ui".into(),
        state_ref: "app:counter/state".into(),
        status: "canvas-app".into(),
        dependencies: vec![],
    };
    let ui = Ui {
        format: "otrios-canvas-v2".into(),
        ui_ref: app.ui_ref.clone(),
        title: app.title.clone(),
        state_schema: schema(),
        layout,
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
    let ui = canvas::encode_ui(&ui).unwrap();
    let package = root.join("counter/package");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("ui.flex"), &ui).unwrap();
    fs::write(package.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
    let catalog = canvas::encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![CatalogEntry {
            app,
            path: "counter".into(),
            private: false,
            ui_hash: canvas::hash(&ui),
        }],
    })
    .unwrap();
    fs::write(root.join("catalog.flex"), &catalog).unwrap();
    catalog
}
fn http(address: &str, method: &str, token: Option<char>, body: Option<&Request>) -> (u16, Json) {
    let bytes = body
        .map(|v| serde_json::to_vec(v).unwrap())
        .unwrap_or_default();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket, "{method} /spaces HTTP/1.0\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\nX-OTriOS-Request: spaces\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", bytes.len()).unwrap();
    if let Some(c) = token {
        write!(
            socket,
            "Authorization: Bearer {}\r\n",
            c.to_string().repeat(64)
        )
        .unwrap();
    }
    socket.write_all(b"\r\n").unwrap();
    socket.write_all(&bytes).unwrap();
    let mut bytes = vec![];
    socket.read_to_end(&mut bytes).unwrap();
    let at = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let status = std::str::from_utf8(&bytes[..at])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes[at + 4..]).unwrap_or(Json::Null),
    )
}
impl Rig {
    fn start(binary: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "otrios-spaces-process-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let catalog = fixture(&root.join("apps"));
        let config = json!({"version":1,"users":[],"native":[
            {"identity":"alice","token_sha256":canvas::hex(&canvas::hash("a".repeat(64).as_bytes()))},
            {"identity":"bob","token_sha256":canvas::hex(&canvas::hash("b".repeat(64).as_bytes()))},
            {"identity":"frame","token_sha256":canvas::hex(&canvas::hash("c".repeat(64).as_bytes()))}
        ]});
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(root.join("auth.json"))
            .unwrap()
            .write_all(config.to_string().as_bytes())
            .unwrap();
        let child = Command::new(binary)
            .args([
                "--bind",
                "127.0.0.1:0",
                "--public-bind",
                "127.0.0.1:0",
                "--origin",
                ORIGIN,
            ])
            .arg("--apps")
            .arg(root.join("apps"))
            .arg("--app-state")
            .arg(root.join("state"))
            .arg("--spaces-registry")
            .arg(root.join("spaces.flex"))
            .arg("--auth-file")
            .arg(root.join("auth.json"))
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut server = Guard(child);
        let mut output = BufReader::new(server.0.stdout.take().unwrap());
        let mut addresses = vec![];
        for _ in 0..2 {
            let mut line = String::new();
            assert!(output.read_line(&mut line).unwrap() > 0);
            addresses.push(
                line.split("http://")
                    .nth(1)
                    .unwrap()
                    .trim()
                    .trim_end_matches('/')
                    .to_owned(),
            );
        }
        server.0.stdout = Some(output.into_inner());
        let rig = Self {
            server,
            root,
            public: addresses[0].clone(),
            native: addresses[1].clone(),
            catalog,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while http(&rig.public, "GET", Some('a'), None).0 != 200 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        rig
    }
    fn change(&self, actor: char, change: Change) -> Json {
        let (status, state) = http(&self.public, "GET", Some(actor), None);
        assert_eq!(status, 200);
        let (status, result) = http(
            &self.public,
            "POST",
            Some(actor),
            Some(&Request {
                version: 1,
                expected_generation: state["generation"].as_u64().unwrap(),
                change,
            }),
        );
        assert_eq!(status, 200, "{result}");
        result
    }
    fn space(&self, name: &str) -> String {
        self.change('a', Change::Create { name: name.into() });
        let (_, state) = http(&self.public, "GET", Some('a'), None);
        state["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .into()
    }
    fn child(&self, address: &str, space: &str, token: Option<char>, mode: &str) {
        let mut child = Guard(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "space_client", "--nocapture"])
                .env("OTRIOS_SPACE_TEST_ADDRESS", address)
                .env("OTRIOS_SPACE_TEST_ID", space)
                .env(
                    "OTRIOS_SPACE_TEST_TOKEN",
                    token.map(|c| c.to_string()).unwrap_or_default(),
                )
                .env("OTRIOS_SPACE_TEST_MODE", mode)
                .env("OTRIOS_SPACE_TEST_ROOT", &self.root)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "client failed: {mode}");
                break;
            }
            assert!(Instant::now() < deadline, "client hung: {mode}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn socket(address: &str, space: &str, token: Option<char>) -> Socket {
    let mut request = format!("ws://{address}/session?space={space}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    if let Some(c) = token {
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", c.to_string().repeat(64))
                .parse()
                .unwrap(),
        );
    }
    let (mut socket, _) = tungstenite::connect(request).unwrap();
    if let MaybeTlsStream::Plain(t) = socket.get_mut() {
        t.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    }
    socket.send(Message::Text(json!({"type":"hello","version":1,"release":null,"state_protocol":"otrios-canvas-sync-v8","state":true}).to_string().into())).unwrap();
    loop {
        if let Message::Text(text) = socket.read().unwrap() {
            let event: Json = serde_json::from_str(&text).unwrap();
            if event["state_available"] == true || event["available"] == true {
                break;
            }
        }
    }
    socket
}
fn send(socket: &mut Socket, message: ClientMessage) {
    let mut bytes = canvas::encode_client(&message).unwrap();
    bytes.insert(0, 1);
    socket.send(Message::Binary(bytes.into())).unwrap();
}
fn read(socket: &mut Socket) -> ServerMessage {
    loop {
        match socket.read().unwrap() {
            Message::Binary(bytes) => {
                return canvas::decode_server(bytes.strip_prefix(&[1]).unwrap()).unwrap();
            }
            Message::Ping(_) => socket.flush().unwrap(),
            _ => (),
        }
    }
}
fn mount(socket: &mut Socket, catalog: &[u8]) {
    send(
        socket,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: "app:counter/ui".into(),
            state_ref: "app:counter/state".into(),
            catalog_hash: canvas::hash(catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![9; 16],
            resume: 0,
        },
    );
}
#[test]
fn space_client() {
    let Ok(address) = std::env::var("OTRIOS_SPACE_TEST_ADDRESS") else {
        return;
    };
    let space = std::env::var("OTRIOS_SPACE_TEST_ID").unwrap();
    let token = std::env::var("OTRIOS_SPACE_TEST_TOKEN")
        .unwrap()
        .chars()
        .next();
    let root = PathBuf::from(std::env::var("OTRIOS_SPACE_TEST_ROOT").unwrap());
    let mode = std::env::var("OTRIOS_SPACE_TEST_MODE").unwrap();
    let mut socket = socket(&address, &space, token);
    mount(
        &mut socket,
        &fs::read(root.join("apps/catalog.flex")).unwrap(),
    );
    let message = read(&mut socket);
    if mode == "deny" {
        assert!(matches!(
            message,
            ServerMessage::AccessRequired { epoch: 1 }
        ));
        send(
            &mut socket,
            ClientMessage::Sync {
                epoch: 1,
                bytes: canvas::encode_state_request(&StateRequest::Write {
                    sequence: 1,
                    values: vec![(1, Value::Number(1000.))],
                })
                .unwrap(),
            },
        );
        assert!(matches!(read(&mut socket), ServerMessage::Error { .. }));
        return;
    }
    let ServerMessage::Mounted { state, .. } = message else {
        panic!("mount rejected: {message:?}");
    };
    let mut replica =
        canvas::StateAdapter::new(schema(), "app:counter/state", &[], [9; 16]).unwrap();
    replica.receive(&state).unwrap();
    if mode == "write" {
        send(&mut socket, ClientMessage::AcceptMount { epoch: 1 });
        while let Some(bytes) = replica.generate().unwrap() {
            send(&mut socket, ClientMessage::Sync { epoch: 1, bytes });
        }
        send(
            &mut socket,
            ClientMessage::Sync {
                epoch: 1,
                bytes: canvas::encode_state_request(&StateRequest::Write {
                    sequence: 1,
                    values: vec![(1, Value::Number(42.))],
                })
                .unwrap(),
            },
        );
        loop {
            if let ServerMessage::Sync { bytes, .. } = read(&mut socket) {
                replica.receive(&bytes).unwrap();
                if replica.value(1).unwrap() == 42 {
                    break;
                }
            }
        }
    } else {
        assert_eq!(replica.value(1).unwrap(), mode.parse::<u16>().unwrap());
    }
}
#[test]
fn separate_clients_isolate_spaces_and_never_trust_native_loopback() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let rig = Rig::start(&binary);
    let household = rig.space("Household");
    let work = rig.space("Work");
    for space in [&household, &work] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "counter".into(),
            },
        );
    }
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "frame".into(),
            role: Some(Role::Member),
        },
    );
    rig.child(&rig.public, &household, Some('a'), "write");
    rig.child(&rig.public, &household, Some('a'), "42");
    rig.child(&rig.public, &work, Some('a'), "0");
    rig.child(&rig.public, &household, Some('b'), "deny");
    rig.child(&rig.native, &household, None, "deny");
    rig.child(&rig.native, &household, Some('c'), "42");
    rig.child(&rig.native, &work, Some('c'), "deny");
    rig.child(&rig.public, &household, Some('a'), "42");
    let (_, public) = http(&rig.public, "GET", None, None);
    assert_eq!(public["spaces"], json!([]));
    let (_, member) = http(&rig.public, "GET", Some('c'), None);
    assert!(member["spaces"][0].get("members").is_none());
    let denied = Request {
        version: 1,
        expected_generation: member["generation"].as_u64().unwrap(),
        change: Change::Membership {
            space: household.clone(),
            identity: "frame".into(),
            role: Some(Role::Admin),
        },
    };
    assert_eq!(http(&rig.public, "POST", Some('c'), Some(&denied)).0, 403);
    // Revoke an already-mounted client, then reject its reconnect.
    let mut active = socket(&rig.native, &household, Some('c'));
    mount(&mut active, &rig.catalog);
    assert!(matches!(read(&mut active), ServerMessage::Mounted { .. }));
    send(&mut active, ClientMessage::AcceptMount { epoch: 1 });
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "frame".into(),
            role: None,
        },
    );
    loop {
        if matches!(
            read(&mut active),
            ServerMessage::AccessRequired { epoch: 1 }
        ) {
            break;
        }
    }
    rig.child(&rig.native, &household, Some('c'), "deny");
    rig.change(
        'a',
        Change::Membership {
            space: work.clone(),
            identity: "bob".into(),
            role: Some(Role::Admin),
        },
    );
    let (_, snapshot) = http(&rig.public, "GET", Some('a'), None);
    let generation = snapshot["generation"].as_u64().unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let tasks = [('a', "bob"), ('b', "alice")].map(|(actor, target)| {
        let address = rig.public.clone();
        let space = work.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            (
                actor,
                http(
                    &address,
                    "POST",
                    Some(actor),
                    Some(&Request {
                        version: 1,
                        expected_generation: generation,
                        change: Change::Membership {
                            space,
                            identity: target.into(),
                            role: None,
                        },
                    }),
                )
                .0,
            )
        })
    });
    barrier.wait();
    let results = tasks.map(|t| t.join().unwrap());
    assert_eq!(
        results.iter().filter(|(_, status)| *status == 200).count(),
        1
    );
    assert_eq!(
        results.iter().filter(|(_, status)| *status == 409).count(),
        1
    );
    let admin = results.iter().find(|(_, status)| *status == 200).unwrap().0;
    let (_, snapshot) = http(&rig.public, "GET", Some(admin), None);
    assert_eq!(
        http(
            &rig.public,
            "POST",
            Some(admin),
            Some(&Request {
                version: 1,
                expected_generation: snapshot["generation"].as_u64().unwrap(),
                change: Change::Membership {
                    space: work,
                    identity: if admin == 'a' { "alice" } else { "bob" }.into(),
                    role: None
                },
            })
        )
        .0,
        409
    );
}
