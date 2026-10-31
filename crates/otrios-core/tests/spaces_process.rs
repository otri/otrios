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
    http_path(address, "/spaces", method, token, body)
}
fn http_path(
    address: &str,
    path: &str,
    method: &str,
    token: Option<char>,
    body: Option<&Request>,
) -> (u16, Json) {
    let bytes = body
        .map(|v| serde_json::to_vec(v).unwrap())
        .unwrap_or_default();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket, "{method} {path} HTTP/1.0\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\nX-OTriOS-Request: spaces\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", bytes.len()).unwrap();
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
    fn stop(&mut self) {
        self.server.0.kill().unwrap();
        self.server.0.wait().unwrap();
    }
    fn restart(&mut self, binary: &str, managed: bool, ready: bool) {
        let mut command = Command::new(binary);
        command
            .args([
                "--bind",
                &self.native,
                "--public-bind",
                &self.public,
                "--origin",
                ORIGIN,
            ])
            .arg("--apps")
            .arg(self.root.join("apps"))
            .arg("--app-state")
            .arg(self.root.join("state"))
            .arg("--auth-file")
            .arg(self.root.join("auth.json"));
        if managed {
            command
                .arg("--spaces-registry")
                .arg(self.root.join("spaces.flex"));
            if self.root.join("collector-binding.json").exists() {
                command
                    .arg("--collector-binding")
                    .arg(self.root.join("collector-binding.json"));
            }
        }
        self.server = Guard(command.stdout(Stdio::null()).spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if TcpStream::connect(&self.public).is_ok()
                && TcpStream::connect(&self.native).is_ok()
                && (!ready || http(&self.public, "GET", Some('a'), None).0 == 200)
            {
                break;
            }
            assert!(Instant::now() < deadline);
            assert!(self.server.0.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn provision(&self, binary: &str, manifest: &Path) -> std::process::Output {
        Command::new(binary)
            .arg("--apps")
            .arg(self.root.join("apps"))
            .arg("--app-state")
            .arg(self.root.join("state"))
            .arg("--spaces-registry")
            .arg(self.root.join("spaces.flex"))
            .arg("--provision-spaces")
            .arg(manifest)
            .output()
            .unwrap()
    }
    fn start(binary: &str) -> Self {
        Self::start_mode(binary, true)
    }
    fn start_mode(binary: &str, managed: bool) -> Self {
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
        let mut command = Command::new(binary);
        command
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
            .arg("--auth-file")
            .arg(root.join("auth.json"));
        if managed {
            command
                .arg("--spaces-registry")
                .arg(root.join("spaces.flex"));
        }
        let child = command.stdout(Stdio::piped()).spawn().unwrap();
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
        while managed && http(&rig.public, "GET", Some('a'), None).0 != 200 {
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

fn private_json(path: &Path, value: &Json) {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .unwrap()
        .write_all(value.to_string().as_bytes())
        .unwrap();
}

fn presentation(address: &str, token: Option<char>, payload: &[u8]) -> u16 {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket,"POST /presentation HTTP/1.0\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n",payload.len()).unwrap();
    if let Some(c) = token {
        write!(
            socket,
            "Authorization: Bearer {}\r\n",
            c.to_string().repeat(64)
        )
        .unwrap();
    }
    socket.write_all(b"\r\n").unwrap();
    socket.write_all(payload).unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn monitoring_collector_has_an_explicit_collection_grant_and_no_human_membership() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    let mut ui =
        canvas::decode_ui(&fs::read(rig.root.join("apps/counter/package/ui.flex")).unwrap())
            .unwrap();
    let view = ui.layout.clone();
    ui.ui_ref = "app:monitoring/ui".into();
    ui.title = "Monitoring".into();
    ui.state_schema.fields[0].writable = false;
    ui.presentation = Some(Presentation::View {
        frame: [0., 0., 1280., 800.],
    });
    let package = rig.root.join("apps/monitoring/package");
    fs::create_dir_all(&package).unwrap();
    let ui = canvas::encode_ui(&ui).unwrap();
    fs::write(package.join("ui.flex"), &ui).unwrap();
    let app = App {
        id: "monitoring".into(),
        title: "Monitoring".into(),
        ui_ref: "app:monitoring/ui".into(),
        state_ref: "app:monitoring/state".into(),
        status: "canvas-app".into(),
        dependencies: vec![],
    };
    fs::write(package.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps.push(CatalogEntry {
        app,
        path: "monitoring".into(),
        ui_hash: canvas::hash(&ui),
        private: true,
    });
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    let auth = rig.root.join("auth.json");
    let mut config: Json = serde_json::from_slice(&fs::read(&auth).unwrap()).unwrap();
    config["native"].as_array_mut().unwrap().push(json!({"identity":"collector","token_sha256":canvas::hex(&canvas::hash("d".repeat(64).as_bytes()))}));
    fs::write(auth, config.to_string()).unwrap();
    let manifest = rig.root.join("monitoring-provision.json");
    private_json(
        &manifest,
        &json!({"version":1,"spaces":[
        {"name":"Operations","admins":["alice"],"members":[],"apps":[{"id":"monitoring"}]},
        {"name":"Other","admins":["alice"],"members":[],"apps":[{"id":"monitoring"}]}
    ],"services":[{"identity":"collector","space":"Operations","app":"monitoring","output":rig.root.join("collector-binding.json")}]}),
    );
    let result = rig.provision(&binary, &manifest);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let record: Json = serde_json::from_slice(&result.stdout).unwrap();
    let operations = record["spaces"][0]["space"].as_str().unwrap();
    let other = record["spaces"][1]["space"].as_str().unwrap();
    rig.restart(&binary, true, true);
    let (_, state) = http(&rig.public, "GET", Some('a'), None);
    let create = Request {
        version: 1,
        expected_generation: state["generation"].as_u64().unwrap(),
        change: Change::Create {
            name: "Collector must not become an Admin".into(),
        },
    };
    assert_eq!(http(&rig.public, "POST", Some('d'), Some(&create)).0, 403);
    assert_eq!(presentation(&rig.native, None, &view), 403);
    assert_eq!(presentation(&rig.native, Some('a'), &view), 403);
    assert_eq!(presentation(&rig.native, Some('d'), &view), 200);
    let mount_monitoring = |socket: &mut Socket| {
        send(
            socket,
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: "app:monitoring/ui".into(),
                state_ref: "app:monitoring/state".into(),
                catalog_hash: canvas::hash(&rig.catalog),
                known_ui: None,
                known_code: vec![],
                writer: vec![1; 16],
                resume: 0,

                collection: None,
            },
        )
    };
    let mut human = socket(&rig.public, operations, Some('a'));
    mount_monitoring(&mut human);
    let ServerMessage::Mounted {
        binding: Some(_),
        initial_view: Some(initial_view),
        ..
    } = read(&mut human)
    else {
        panic!("complete managed mount")
    };
    assert_eq!(initial_view, view);
    send(&mut human, ClientMessage::AcceptMount { epoch: 1 });
    assert!(matches!(
        read(&mut human),
        ServerMessage::Activated { epoch: 1 }
    ));
    loop {
        if let ServerMessage::View { bytes, .. } = read(&mut human) {
            assert_eq!(bytes, view);
            break;
        }
    }
    let mut producer = socket(&rig.native, operations, Some('d'));
    send(
        &mut human,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 1,
                values: vec![(1, Value::Number(100.))],
            })
            .unwrap(),
        },
    );
    assert!(matches!(read(&mut human), ServerMessage::Error { .. }));
    mount_monitoring(&mut producer);
    assert!(matches!(
        read(&mut producer),
        ServerMessage::AccessRequired { .. }
    ));
    let mut isolated = socket(&rig.public, other, Some('a'));
    mount_monitoring(&mut isolated);
    assert!(matches!(read(&mut isolated), ServerMessage::Error { .. }));
    rig.change(
        'a',
        Change::Membership {
            space: operations.into(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    assert_eq!(presentation(&rig.native, Some('d'), &view), 200);
    let grant: otrios_interfaces::spaces::ServiceBinding =
        serde_json::from_slice(&fs::read(rig.root.join("collector-binding.json")).unwrap())
            .unwrap();
    assert_eq!(
        fs::read(rig.root.join(format!(
            "state/collection-{}.presentation.flex",
            grant.binding.collection
        )))
        .unwrap(),
        view
    );
}

#[test]
fn offline_provisioning_preserves_confirmed_legacy_data_and_does_not_regrant_removed_members() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start_mode(&binary, false);
    rig.child(&rig.public, &"a".repeat(32), Some('a'), "write");
    rig.stop();
    let manifest = rig.root.join("provision.json");
    private_json(
        &manifest,
        &json!({"version":1,"spaces":[
            {"name":"Household","admins":["alice","bob"],"members":["frame"],
                "apps":[{"id":"counter","legacy_state_ref":"app:counter/state"}]},
            {"name":"Work","admins":["alice"],"members":[],"apps":[{"id":"counter"}]}
        ]}),
    );
    let result = rig.provision(&binary, &manifest);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let record: Json = serde_json::from_slice(&result.stdout).unwrap();
    let household = record["spaces"][0]["space"].as_str().unwrap().to_string();
    let work = record["spaces"][1]["space"].as_str().unwrap().to_string();
    assert!(
        record["spaces"][0]["imports"][0]["source_root"]
            .as_str()
            .is_some_and(|s| s.len() == 64)
    );
    rig.restart(&binary, true, true);
    rig.child(&rig.public, &household, Some('a'), "42");
    rig.child(&rig.native, &household, Some('c'), "42");
    rig.child(&rig.public, &work, Some('a'), "0");
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "frame".into(),
            role: None,
        },
    );
    rig.stop();
    assert!(rig.provision(&binary, &manifest).status.success());
    // Removing a CLI flag cannot reopen the legacy listener after migration.
    rig.restart(&binary, false, true);
    rig.child(&rig.native, &household, Some('c'), "deny");
    rig.child(&rig.public, &household, Some('a'), "42");
    rig.stop();
    // A failed initial import leaves admission closed, never an empty replacement.
    let broken = rig.root.join("broken-provision.json");
    private_json(
        &broken,
        &json!({"version":1,"spaces":[{"name":"Incomplete","admins":["alice"],"members":[],
        "apps":[{"id":"counter","legacy_state_ref":"app:unrelated/state"}]}]}),
    );
    assert!(!rig.provision(&binary, &broken).status.success());
    rig.restart(&binary, false, false);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if http(&rig.public, "GET", Some('a'), None).0 == 503 {
            break;
        }
        assert!(Instant::now() < deadline);
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
    socket.send(Message::Text(json!({"type":"hello","version":1,"release":null,"state_protocol":"otrios-canvas-sync-v13","state":true}).to_string().into())).unwrap();
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
fn send_spaces(socket: &mut Socket, command: otrios_interfaces::spaces::Command) {
    let mut bytes = otrios_core::spaces::encode_command(&command).unwrap();
    bytes.insert(0, otrios_interfaces::spaces::CHANNEL);
    socket.send(Message::Binary(bytes.into())).unwrap();
}
fn read_spaces(socket: &mut Socket) -> otrios_interfaces::spaces::Event {
    loop {
        match socket.read().unwrap() {
            Message::Binary(bytes) if bytes[0] == otrios_interfaces::spaces::CHANNEL => {
                return otrios_core::spaces::decode_event(&bytes[1..]).unwrap();
            }
            Message::Ping(_) => socket.flush().unwrap(),
            _ => (),
        }
    }
}

#[test]
fn shared_session_discovery_observes_racing_changes_and_rechecks_management_and_roster_access() {
    use otrios_interfaces::spaces::{Command as SpaceCommand, Event, ManagementStatus};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let rig = Rig::start(&binary);
    let space = rig.space("Discovery");
    let mut admin = socket(&rig.public, &space, Some('a'));
    send_spaces(
        &mut admin,
        SpaceCommand::Watch {
            observation: 1,
            version: 1,
            selected: Some(space.clone()),
            cursor: None,
        },
    );
    // Race publication against initial capture. Either initial snapshot is valid;
    // observation must reach the committed generation without another request.
    let target = rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    )["generation"]
        .as_u64()
        .unwrap();
    let mut snapshot = loop {
        if let Event::Snapshot { snapshot, .. } = read_spaces(&mut admin)
            && snapshot.generation >= target
        {
            break snapshot;
        }
    };
    let detail = snapshot.selected.as_ref().unwrap();
    assert_eq!(detail.apps.len(), 1);
    assert!(detail.apps[0].launch.is_some());
    assert_eq!(detail.members.as_ref().unwrap().len(), 1);
    let collection = detail.apps[0].collection.clone();
    send_spaces(
        &mut admin,
        SpaceCommand::Manage {
            version: 1,
            id: 1,
            request: Request {
                version: 1,
                expected_generation: snapshot.generation,
                change: Change::Membership {
                    space: space.clone(),
                    identity: "alice".into(),
                    role: None,
                },
            },
        },
    );
    assert!(matches!(
        read_spaces(&mut admin),
        Event::Result {
            id: 1,
            status: ManagementStatus::LastAdmin,
            ..
        }
    ));
    let Event::Snapshot {
        snapshot: unchanged,
        ..
    } = read_spaces(&mut admin)
    else {
        panic!("snapshot")
    };
    assert_eq!(unchanged.generation, snapshot.generation);
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    let Event::Snapshot {
        snapshot: updated, ..
    } = read_spaces(&mut admin)
    else {
        panic!("observation")
    };
    snapshot = updated;
    let mut member = socket(&rig.public, &space, Some('b'));
    send_spaces(
        &mut member,
        SpaceCommand::Watch {
            observation: 1,
            version: 1,
            selected: Some(space.clone()),
            cursor: None,
        },
    );
    let Event::Snapshot {
        snapshot: member_snapshot,
        ..
    } = read_spaces(&mut member)
    else {
        panic!("member snapshot")
    };
    let detail = member_snapshot.selected.as_ref().unwrap();
    assert!(detail.members.is_none());
    assert!(detail.installed.is_empty());
    assert!(detail.apps[0].stored_bytes.is_none());
    assert_eq!(detail.apps[0].collection, collection);
    send_spaces(
        &mut member,
        SpaceCommand::Manage {
            version: 1,
            id: 2,
            request: Request {
                version: 1,
                expected_generation: snapshot.generation,
                change: Change::Membership {
                    space: space.clone(),
                    identity: "bob".into(),
                    role: Some(Role::Admin),
                },
            },
        },
    );
    assert!(matches!(
        read_spaces(&mut member),
        Event::Result {
            id: 2,
            status: ManagementStatus::Denied,
            ..
        }
    ));
    assert!(matches!(read_spaces(&mut member), Event::Snapshot { .. }));
    let mut stranger = socket(&rig.public, &space, None);
    send_spaces(
        &mut stranger,
        SpaceCommand::Watch {
            observation: 1,
            version: 1,
            selected: Some(space.clone()),
            cursor: None,
        },
    );
    let Event::Snapshot {
        snapshot: hidden, ..
    } = read_spaces(&mut stranger)
    else {
        panic!("anonymous snapshot")
    };
    assert!(hidden.spaces.is_empty());
    assert!(hidden.selected.is_none());
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: None,
        },
    );
    let Event::Snapshot {
        snapshot: revoked, ..
    } = read_spaces(&mut member)
    else {
        panic!("revocation")
    };
    assert!(revoked.spaces.is_empty());
    assert!(revoked.selected.is_none());
    // A returning observer from a previous authority incarnation receives a full
    // explicit resync even when its numeric revision happens to match.
    let mut stale = snapshot.cursor.clone();
    stale.authority = "0".repeat(32);
    send_spaces(
        &mut stranger,
        SpaceCommand::Watch {
            observation: 1,
            version: 1,
            selected: Some(space),
            cursor: Some(stale),
        },
    );
    loop {
        if let Event::Snapshot {
            resync: true,
            snapshot,
            ..
        } = read_spaces(&mut stranger)
        {
            assert!(snapshot.selected.is_none());
            break;
        }
    }
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

            collection: None,
        },
    );
}
fn document_session(
    rig: &Rig,
    space: &str,
    id: &str,
    schema: StateSchema,
    value: Option<u16>,
) -> (u16, Option<String>) {
    let mut socket = socket(&rig.public, space, Some('a'));
    let state_ref = format!("app:{id}/state");
    send(
        &mut socket,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: format!("app:{id}/ui"),
            state_ref: state_ref.clone(),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![7; 16],
            resume: 0,
            collection: None,
        },
    );
    let ServerMessage::Mounted { state, binding, .. } = read(&mut socket) else {
        panic!("document mount")
    };
    let mut replica = canvas::StateAdapter::new(schema, &state_ref, &state, [7; 16]).unwrap();
    send(&mut socket, ClientMessage::AcceptMount { epoch: 1 });
    if binding.is_some() {
        assert!(matches!(
            read(&mut socket),
            ServerMessage::Activated { epoch: 1 }
        ));
    }
    if let Some(value) = value {
        replica.set(&[(1, value)]).unwrap();
        while let Some(bytes) = replica.generate().unwrap() {
            send(&mut socket, ClientMessage::Sync { epoch: 1, bytes });
        }
        while !replica.confirmed() {
            if let ServerMessage::Sync { bytes, .. } = read(&mut socket) {
                replica.receive(&bytes).unwrap();
            }
        }
        assert_eq!(replica.value(1).unwrap(), value);
    }
    send(&mut socket, ClientMessage::Leave { epoch: 1 });
    (replica.value(1).unwrap(), binding.map(|b| b.collection))
}

fn ink_control(socket: &mut Socket, enabled: bool) {
    socket
        .send(Message::Text(
            json!({"type":"ink","enabled":enabled}).to_string().into(),
        ))
        .unwrap();
}
fn chat_fixture(rig: &mut Rig) {
    use otrios_core::authoring as a;
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    for id in ["relay", "relay/other", "sibling"] {
        let ui = a::AppUi::new(id, id, a::schema("chat-empty-v1", &[]))
            .with_presentation(Presentation::Chat)
            .build(id, vec![]);
        let app = App {
            id: id.into(),
            title: id.into(),
            ui_ref: ui.ui_ref.clone(),
            state_ref: format!("app:{id}/state"),
            status: "canvas-app".into(),
            dependencies: vec![],
        };
        let bytes = canvas::encode_ui(&ui).unwrap();
        let package = rig.root.join("apps").join(id).join("package");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
        fs::write(package.join("ui.flex"), &bytes).unwrap();
        catalog.apps.push(CatalogEntry {
            app,
            path: id.into(),
            private: false,
            ui_hash: canvas::hash(&bytes),
        });
    }
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
}
fn chat_state(socket: &mut Socket) -> ServerMessage {
    loop {
        match socket.read().unwrap() {
            Message::Binary(bytes) if bytes.first() == Some(&1) => {
                return canvas::decode_server(&bytes[1..]).unwrap();
            }
            Message::Ping(_) => socket.flush().unwrap(),
            _ => (),
        }
    }
}
fn chat_mount(
    rig: &Rig,
    socket: &mut Socket,
    id: &str,
    epoch: u32,
) -> (String, otrios_interfaces::chat::Initial) {
    send(
        socket,
        ClientMessage::Mount {
            epoch,
            ui_ref: format!("app:{id}/ui"),
            state_ref: format!("app:{id}/state"),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![9; 16],
            resume: 0,
            collection: None,
        },
    );
    loop {
        match chat_state(socket) {
            ServerMessage::Mounted {
                epoch: found,
                binding: Some(binding),
                initial_view: Some(view),
                ..
            } => {
                assert_eq!(found, epoch);
                let initial = flexbuffers::from_slice(&view).unwrap();
                send(socket, ClientMessage::AcceptMount { epoch });
                loop {
                    match chat_state(socket) {
                        ServerMessage::Activated { epoch: found } => {
                            assert_eq!(found, epoch);
                            return (binding.collection, initial);
                        }
                        ServerMessage::Sync { .. } => (),
                        other => panic!("chat activation: {other:?}"),
                    }
                }
            }
            ServerMessage::Sync { .. } => (),
            other => panic!("chat mount: {other:?}"),
        }
    }
}
fn chat_command(socket: &mut Socket, epoch: u32, command: otrios_interfaces::chat::Command) {
    use otrios_interfaces::channels::{frame, scoped};
    socket
        .send(Message::Binary(
            frame(
                3,
                scoped(epoch, canvas::chat::encode_command(&command).unwrap()),
            )
            .into(),
        ))
        .unwrap();
}
fn chat_aux(socket: &mut Socket, channel: u8, epoch: u32) -> Vec<u8> {
    loop {
        match socket.read().unwrap() {
            Message::Binary(bytes) if bytes.first() == Some(&channel) => {
                let (scope, bytes) = otrios_interfaces::channels::scope(&bytes[1..]).unwrap();
                assert_eq!(scope, epoch);
                return bytes.to_vec();
            }
            Message::Ping(_) => socket.flush().unwrap(),
            Message::Close(_) => panic!("chat session closed"),
            _ => (),
        }
    }
}
fn chat_event(
    socket: &mut Socket,
    epoch: u32,
    wanted: impl Fn(&otrios_interfaces::chat::Event) -> bool,
) -> otrios_interfaces::chat::Event {
    loop {
        let event = canvas::chat::decode_event(&chat_aux(socket, 3, epoch)).unwrap();
        if wanted(&event) {
            return event;
        }
        assert!(
            !matches!(event, otrios_interfaces::chat::Event::Error { .. }),
            "{event:?}"
        );
    }
}
fn chat_join(socket: &mut Socket, epoch: u32) -> (String, u64) {
    use otrios_interfaces::chat::{Command, Event};
    chat_command(socket, epoch, Command::Join { credential: None });
    let Event::Welcome {
        you,
        credential,
        temporary,
        epoch,
        ..
    } = chat_event(socket, epoch, |e| matches!(e, Event::Welcome { .. }))
    else {
        unreachable!()
    };
    assert!(credential.is_none());
    assert!(!temporary);
    assert!(you.admin, "a Space Member may moderate app content");
    (you.profile.id, epoch)
}
fn chat_send(socket: &mut Socket, mount: u32, epoch: u64, nonce: u64, text: &str) -> u64 {
    use otrios_interfaces::chat::{Command, Event};
    chat_command(
        socket,
        mount,
        Command::Send {
            epoch,
            nonce: format!("{nonce:032x}"),
            text: text.into(),
        },
    );
    let Event::Receipt { sequence, .. } =
        chat_event(socket, mount, |e| matches!(e, Event::Receipt { .. }))
    else {
        unreachable!()
    };
    sequence
}
fn chat_window(
    socket: &mut Socket,
    epoch: u32,
    generation: u64,
) -> Vec<otrios_interfaces::chat::Message> {
    chat_select(
        socket,
        epoch,
        generation,
        otrios_core::collection::Anchor::Tail,
    )
    .0
}
fn chat_select(
    socket: &mut Socket,
    epoch: u32,
    generation: u64,
    anchor: otrios_core::collection::Anchor,
) -> (Vec<otrios_interfaces::chat::Message>, u64) {
    use otrios_core::collection as col;
    use otrios_interfaces::channels::{frame, scoped};
    let mut client = col::Client::new(canvas::chat::COLLECTION, canvas::chat::message_schema());
    client
        .select(canvas::chat::message_selection(anchor))
        .unwrap();
    let mut counter = generation - 1;
    let mut authority_epoch = None;
    loop {
        while let Some(command) = client.outgoing(&mut counter).unwrap() {
            socket
                .send(Message::Binary(
                    frame(4, scoped(epoch, col::encode_command(&command).unwrap())).into(),
                ))
                .unwrap();
        }
        if let Some(window) = client.window() {
            return (
                window
                    .keys
                    .iter()
                    .map(|key| canvas::chat::read_message(&window.record(*key).unwrap()).unwrap())
                    .collect(),
                authority_epoch.expect("selected authority epoch"),
            );
        }
        let event = col::Event::from_bytes(&chat_aux(socket, 4, epoch)).unwrap();
        if let col::Event::View(offer) = &event {
            authority_epoch = Some(offer.epoch);
        }
        client.receive(event).unwrap();
    }
}

fn legacy_chat(rig: &Rig, count: u64) -> (canvas::chat::Room, String) {
    use otrios_interfaces::chat::Command as C;
    let now = 1_000_000;
    let mut room =
        canvas::chat::Room::open(Some(rig.root.join("state/brass-relay.journal")), now).unwrap();
    let (profile, credential) = room.identity(None, now).unwrap();
    room.join(1, Some(&credential), false, now, true).unwrap();
    for sequence in 1..=count {
        let at = now + sequence * 1000;
        room.handle(
            1,
            C::Send {
                epoch: at / 600_000,
                nonce: format!("{sequence:032x}"),
                text: format!("Confirmed legacy dispatch {sequence}"),
            },
            at,
        )
        .unwrap();
    }
    (room, profile.id)
}

#[test]
fn offline_chat_import_preserves_full_history_and_never_replays_over_later_confirmed_data() {
    use otrios_core::collection as col;
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start_mode(&binary, false);
    rig.stop();
    chat_fixture(&mut rig);
    let (source_owner, author) = legacy_chat(&rig, 70);
    let source = rig.root.join("state/brass-relay.journal");
    let original = fs::read(&source).unwrap();
    let manifest = rig.root.join("provision-chat.json");
    let config = json!({"version":2,"spaces":[
        {"name":"Migrated chat","admins":["alice"],"members":["frame"],"apps":[{"id":"relay","legacy_chat_root":"app:relay/other/state"}]},
        {"name":"Fresh chat","admins":["alice"],"members":[],"apps":[{"id":"relay"}]}
    ]});
    let old_manifest = rig.root.join("provision-chat-v1.json");
    let mut old_config = config.clone();
    old_config["version"] = json!(1);
    private_json(&old_manifest, &old_config);
    let old_result = rig.provision(&binary, &old_manifest);
    assert!(!old_result.status.success());
    assert!(String::from_utf8_lossy(&old_result.stderr).contains("version 2"));
    assert!(
        !rig.root.join("spaces.provisioning").exists(),
        "reject the unsupported plan before beginning provisioning"
    );
    private_json(&manifest, &config);
    let locked = rig.provision(&binary, &manifest);
    assert!(
        !locked.status.success(),
        "a live source writer must prevent import"
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(rig.root.join("spaces.provisioning").exists());
    drop(source_owner);
    // Confirmed complete records survive recovery of an incomplete final write.
    let mut torn = fs::OpenOptions::new().append(true).open(&source).unwrap();
    torn.write_all(b"tail").unwrap();
    torn.sync_all().unwrap();
    drop(torn);
    let completed = rig.provision(&binary, &manifest);
    assert!(
        completed.status.success(),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    let record: Json = serde_json::from_slice(&completed.stdout).unwrap();
    let household = record["spaces"][0]["space"].as_str().unwrap().to_string();
    let work = record["spaces"][1]["space"].as_str().unwrap().to_string();
    let import = &record["spaces"][0]["imports"][0];
    assert_eq!(import["kind"], "legacy-chat");
    assert_eq!(
        import["source_sha256"],
        canvas::hex(&canvas::hash(&original))
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    let collection = import["collection"].as_str().unwrap();
    let target = rig.root.join(format!(
        "state/collection-{collection}.data/chat-{}.journal",
        canvas::hex(&canvas::hash(b"app:relay/other/state"))
    ));
    assert_eq!(
        fs::read(&target).unwrap(),
        original,
        "copy all retained history, not just its first window"
    );
    assert!(!rig.root.join("spaces.provisioning").exists());
    rig.restart(&binary, true, true);
    let mut frame = socket(&rig.native, &household, Some('c'));
    let (_, initial) = chat_mount(&rig, &mut frame, "relay/other", 1);
    assert_eq!(initial.messages.len(), 64);
    assert_eq!(initial.messages[0].message.sequence, 7);
    assert_eq!(initial.messages[63].message.sequence, 70);
    assert!(initial.older);
    let (tail, epoch) = chat_select(&mut frame, 1, 1, col::Anchor::Tail);
    assert_eq!(tail.len(), 64);
    let (older, _) = chat_select(
        &mut frame,
        1,
        2,
        col::Anchor::Before(col::Cursor {
            epoch,
            collection: canvas::chat::COLLECTION,
            key: 7,
        }),
    );
    assert_eq!(
        older.iter().map(|m| m.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    assert!(older.iter().all(|m| m.sender.id == author));
    let (frame_profile, epoch) = chat_join(&mut frame, 1);
    assert_ne!(
        frame_profile, author,
        "admission cannot steal the legacy guest profile"
    );
    assert_eq!(
        chat_send(&mut frame, 1, epoch, 71, "Confirmed after migration"),
        71
    );
    assert_eq!(
        fs::read(&source).unwrap(),
        original,
        "new writes never return to the global source"
    );
    for (space, root) in [(&household, "relay"), (&work, "relay/other")] {
        let mut fresh = socket(&rig.public, space, Some('a'));
        assert!(chat_mount(&rig, &mut fresh, root, 1).1.messages.is_empty());
    }
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "frame".into(),
            role: None,
        },
    );
    drop(frame);
    rig.stop();
    let before = fs::read(&target).unwrap();
    assert!(rig.provision(&binary, &manifest).status.success());
    assert_eq!(fs::read(&target).unwrap(), before);
    // A new provisioning intent can retain the completed import declaration;
    // it validates the destination without needing or replaying the old source.
    fs::remove_file(&source).unwrap();
    let revised = rig.root.join("provision-chat-revised.json");
    let mut revised_config = config;
    revised_config["spaces"][0]["members"] = json!([]);
    private_json(&revised, &revised_config);
    let result = rig.provision(&binary, &revised);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), before);
    rig.restart(&binary, true, true);
    let (_, state) = http(&rig.public, "GET", Some('a'), None);
    assert!(
        !state["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == household)
            .unwrap()["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["identity"] == "frame")
    );
    let mut alice = socket(&rig.public, &household, Some('a'));
    let (_, recovered) = chat_mount(&rig, &mut alice, "relay/other", 1);
    assert_eq!(recovered.messages.last().unwrap().message.sequence, 71);
    assert_eq!(
        recovered.messages.last().unwrap().message.sender.id,
        frame_profile
    );
    drop(alice);
    rig.stop();
    fs::remove_file(&target).unwrap();
    revised_config["spaces"][1]["name"] = json!("Another provisioning intent");
    let missing_target = rig.root.join("provision-missing-target.json");
    private_json(&missing_target, &revised_config);
    assert!(!rig.provision(&binary, &missing_target).status.success());
    assert!(
        !target.exists(),
        "a completed import cannot recreate missing confirmed data"
    );
    assert!(!source.exists());
}

#[test]
fn offline_chat_import_rejects_missing_corrupt_and_conflicting_data_without_empty_replacement() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    for failure in ["missing", "corrupt", "conflict"] {
        let mut rig = Rig::start(&binary);
        rig.stop();
        chat_fixture(&mut rig);
        let (owner, _) = legacy_chat(&rig, 1);
        drop(owner);
        let source = rig.root.join("state/brass-relay.journal");
        rig.restart(&binary, true, true);
        let space = rig.space("Import destination");
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "relay".into(),
            },
        );
        let (_, registry) = http(&rig.public, "GET", Some('a'), None);
        let collection = registry["spaces"][0]["apps"][0]["collection"]
            .as_str()
            .unwrap();
        let target = rig.root.join(format!(
            "state/collection-{collection}.data/chat-primary.journal"
        ));
        if failure == "conflict" {
            let mut client = socket(&rig.public, &space, Some('a'));
            chat_mount(&rig, &mut client, "relay", 1);
            let (_, epoch) = chat_join(&mut client, 1);
            assert_eq!(
                chat_send(
                    &mut client,
                    1,
                    epoch,
                    1,
                    "Different confirmed destination content"
                ),
                1
            );
        }
        rig.stop();
        let target_before = fs::read(&target).ok();
        if failure == "missing" {
            fs::remove_file(&source).unwrap();
        }
        if failure == "corrupt" {
            let mut bytes = fs::read(&source).unwrap();
            bytes[20] ^= 1;
            fs::write(&source, bytes).unwrap();
        }
        let source_before = fs::read(&source).ok();
        let manifest = rig.root.join("provision-conflict.json");
        private_json(
            &manifest,
            &json!({"version":2,"spaces":[
                {"name":"Import destination","admins":["alice"],"members":[],"apps":[{"id":"relay","legacy_chat_root":"app:relay/state"}]}
            ]}),
        );
        let result = rig.provision(&binary, &manifest);
        assert!(!result.status.success(), "must reject {failure}");
        assert_eq!(
            fs::read(&source).ok(),
            source_before,
            "source stays intact on {failure}"
        );
        assert_eq!(
            fs::read(&target).ok(),
            target_before,
            "destination stays intact on {failure}"
        );
        assert!(rig.root.join("spaces.provisioning").exists());
        rig.restart(&binary, true, false);
        assert_eq!(
            http(&rig.public, "GET", Some('a'), None).0,
            503,
            "partial provisioning cannot serve empty data"
        );
    }
}

#[test]
fn managed_chat_isolates_owned_transcripts_and_preserves_them_after_quit_revocation_and_reattachment()
 {
    use otrios_interfaces::chat::{Command as C, Event as E};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    rig.restart(&binary, true, true);
    let household = rig.space("Chat household");
    let work = rig.space("Chat work");
    for (space, app) in [
        (&household, "relay"),
        (&household, "sibling"),
        (&work, "relay"),
    ] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: app.into(),
            },
        );
    }
    for identity in ["bob", "frame"] {
        rig.change(
            'a',
            Change::Membership {
                space: household.clone(),
                identity: identity.into(),
                role: Some(Role::Member),
            },
        );
    }
    let mut alice = socket(&rig.public, &household, Some('a'));
    let (collection, initial) = chat_mount(&rig, &mut alice, "relay", 1);
    assert!(initial.messages.is_empty());
    chat_command(
        &mut alice,
        1,
        C::Join {
            credential: Some("f".repeat(64)),
        },
    );
    assert!(matches!(
        chat_event(&mut alice, 1, |e| matches!(e, E::Error { .. })),
        E::Error { .. }
    ));
    let (alice_profile, epoch) = chat_join(&mut alice, 1);
    let mut bob = socket(&rig.public, &household, Some('b'));
    chat_mount(&rig, &mut bob, "relay", 1);
    chat_join(&mut bob, 1);
    let mut frame = socket(&rig.native, &household, Some('c'));
    chat_mount(&rig, &mut frame, "relay", 1);
    let (frame_profile, _) = chat_join(&mut frame, 1);
    assert_ne!(frame_profile, alice_profile);
    assert_eq!(chat_send(&mut alice, 1, epoch, 1, "Household dispatch"), 1);
    assert_eq!(chat_window(&mut bob, 1, 1)[0].text, "Household dispatch");
    assert_eq!(chat_window(&mut frame, 1, 1)[0].text, "Household dispatch");
    for (space, id) in [
        (&work, "relay"),
        (&household, "relay/other"),
        (&household, "sibling"),
    ] {
        let mut peer = socket(&rig.public, space, Some('a'));
        let (_, initial) = chat_mount(&rig, &mut peer, id, 1);
        assert!(initial.messages.is_empty());
        assert_ne!(chat_join(&mut peer, 1).0, alice_profile);
        assert!(
            chat_window(&mut peer, 1, 1).is_empty(),
            "collection 22 cannot select a sibling transcript"
        );
    }
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "bob".into(),
            role: None,
        },
    );
    loop {
        if matches!(
            chat_state(&mut bob),
            ServerMessage::AccessRequired { epoch: 1 }
        ) {
            break;
        }
    }
    chat_command(
        &mut bob,
        1,
        C::Send {
            epoch,
            nonce: format!("{:032x}", 99),
            text: "Revoked dispatch".into(),
        },
    );
    send(&mut alice, ClientMessage::Leave { epoch: 1 });
    assert_eq!(
        chat_send(&mut frame, 1, epoch, 2, "Frame continues after Quit"),
        2
    );
    let (same, initial) = chat_mount(&rig, &mut alice, "relay", 2);
    assert_eq!(same, collection);
    assert_eq!(initial.messages.len(), 2);
    assert_eq!(chat_join(&mut alice, 2).0, alice_profile);
    rig.change(
        'a',
        Change::Remove {
            space: household.clone(),
            app: "relay".into(),
        },
    );
    loop {
        if matches!(
            chat_state(&mut frame),
            ServerMessage::AccessRequired { epoch: 1 }
        ) {
            break;
        }
    }
    drop(alice);
    drop(bob);
    drop(frame);
    rig.stop();
    rig.restart(&binary, true, true);
    rig.change(
        'a',
        Change::Attach {
            space: household.clone(),
            app: "relay".into(),
        },
    );
    let mut reopened = socket(&rig.native, &household, Some('c'));
    let (same, initial) = chat_mount(&rig, &mut reopened, "relay", 1);
    assert_eq!(same, collection);
    assert_eq!(initial.messages.len(), 2);
    assert_eq!(
        initial.messages[1].message.text,
        "Frame continues after Quit"
    );
    assert_eq!(chat_join(&mut reopened, 1).0, frame_profile);
    assert_eq!(chat_window(&mut reopened, 1, 1).len(), 2);
    drop(reopened);
    rig.stop();
    let damaged = rig.root.join(format!(
        "state/collection-{collection}.data/chat-{}.journal",
        canvas::hex(&canvas::hash(b"app:relay/other/state"))
    ));
    fs::write(&damaged, b"invalid saved transcript").unwrap();
    rig.restart(&binary, true, true);
    let mut survivor = socket(&rig.native, &household, Some('c'));
    chat_mount(&rig, &mut survivor, "relay", 1);
    let (_, epoch) = chat_join(&mut survivor, 1);
    send(
        &mut survivor,
        ClientMessage::Mount {
            epoch: 2,
            ui_ref: "app:relay/other/ui".into(),
            state_ref: "app:relay/other/state".into(),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![9; 16],
            resume: 0,
            collection: Some(collection),
        },
    );
    loop {
        match chat_state(&mut survivor) {
            ServerMessage::Error { epoch: 2, .. } => break,
            ServerMessage::Sync { .. } => (),
            other => panic!("unreadable chat candidate: {other:?}"),
        }
    }
    chat_command(
        &mut survivor,
        2,
        C::Send {
            epoch,
            nonce: format!("{:032x}", 100),
            text: "Unaccepted candidate dispatch".into(),
        },
    );
    assert_eq!(
        chat_send(&mut survivor, 1, epoch, 3, "Previous app remains usable"),
        3
    );
    assert_eq!(fs::read(damaged).unwrap(), b"invalid saved transcript");
}
fn ink_points(socket: &mut Socket, epoch: u32) -> Vec<otrios_interfaces::ink::Point> {
    use otrios_interfaces::{channels as session, ink};
    loop {
        match socket.read().unwrap() {
            Message::Binary(bytes) if bytes.first() == Some(&session::INK) => {
                let (received, payload) = session::scope(&bytes[1..]).unwrap();
                assert_eq!(received, epoch);
                assert!(payload.len().is_multiple_of(ink::SERVER_BYTES));
                return payload
                    .chunks_exact(ink::SERVER_BYTES)
                    .filter_map(|bytes| ink::decode_server(bytes).unwrap().2)
                    .collect();
            }
            Message::Binary(bytes) if bytes.first() == Some(&session::STATE) => {
                assert!(matches!(
                    canvas::decode_server(&bytes[1..]).unwrap(),
                    ServerMessage::Sync { .. }
                ));
            }
            Message::Ping(_) => socket.flush().unwrap(),
            other => panic!("unexpected ink event: {other:?}"),
        }
    }
}
fn ink_send(socket: &mut Socket, epoch: u32, point: otrios_interfaces::ink::Point) {
    use otrios_interfaces::channels as session;
    socket
        .send(Message::Binary(
            session::frame(session::INK, session::scoped(epoch, point.encode())).into(),
        ))
        .unwrap();
}
fn ink_mount(rig: &Rig, socket: &mut Socket, id: &str, epoch: u32) {
    send(
        socket,
        ClientMessage::Mount {
            epoch,
            ui_ref: format!("app:{id}/ui"),
            state_ref: format!("app:{id}/state"),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![epoch as u8 + 10; 16],
            resume: 0,
            collection: None,
        },
    );
    loop {
        match read(socket) {
            ServerMessage::Mounted {
                epoch: received,
                binding: Some(_),
                ..
            } => {
                assert_eq!(received, epoch);
                return;
            }
            ServerMessage::Sync { .. } => (),
            other => panic!("unexpected mount: {other:?}"),
        }
    }
}
fn ink_accept(socket: &mut Socket, epoch: u32) {
    send(socket, ClientMessage::AcceptMount { epoch });
    loop {
        match read(socket) {
            ServerMessage::Activated { epoch: received } => {
                assert_eq!(received, epoch);
                return;
            }
            ServerMessage::Sync { .. } => (),
            other => panic!("unexpected activation: {other:?}"),
        }
    }
}
fn ink_barrier(socket: &mut Socket) {
    socket
        .send(Message::Ping(b"no-ink-before-activation".to_vec().into()))
        .unwrap();
    loop {
        match socket.read().unwrap() {
            Message::Pong(bytes) => {
                assert_eq!(&bytes[..], b"no-ink-before-activation");
                return;
            }
            Message::Ping(_) => socket.flush().unwrap(),
            Message::Binary(bytes) if bytes.first() == Some(&1) => {
                assert!(matches!(
                    canvas::decode_server(&bytes[1..]).unwrap(),
                    ServerMessage::Sync { .. }
                ));
            }
            other => panic!("unexpected data without ink admission: {other:?}"),
        }
    }
}

#[test]
fn managed_ink_follows_accepted_owned_documents_and_releases_revoked_or_quit_subscriptions() {
    use otrios_core::authoring as a;
    use otrios_interfaces::ink::Point;
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    let mut entries = vec![];
    for id in ["counter", "counter/other", "sibling"] {
        let x = a::StateField::normalized(1, "x", 0);
        let y = a::StateField::normalized(2, "y", 0);
        let ui = a::AppUi::new(id, id, a::schema("ink-v1", &[x, y]))
            .with_touch_pad("pad")
            .build(
                id,
                vec![
                    a::canvas(vec![a::surface(a::CARD, 10.).pin_edges(0.)])
                        .key("pad")
                        .frame([24., 160., 1200., 600.])
                        .on_drag("Touch pad", a::Event::pad(x, y)),
                ],
            );
        let bytes = canvas::encode_ui(&ui).unwrap();
        let app = App {
            id: id.into(),
            title: id.into(),
            ui_ref: ui.ui_ref,
            state_ref: format!("app:{id}/state"),
            status: "canvas-app".into(),
            dependencies: vec![],
        };
        let package = rig.root.join("apps").join(id).join("package");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("ui.flex"), &bytes).unwrap();
        fs::write(package.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
        entries.push(CatalogEntry {
            app,
            path: id.into(),
            ui_hash: canvas::hash(&bytes),
            private: false,
        });
    }
    rig.catalog = canvas::encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: entries,
    })
    .unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    let household = rig.space("Ink household");
    let work = rig.space("Ink work");
    for (space, app) in [
        (&household, "counter"),
        (&household, "sibling"),
        (&work, "counter"),
    ] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: app.into(),
            },
        );
    }
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    let mut alice = socket(&rig.public, &household, Some('a'));
    ink_control(&mut alice, true);
    ink_barrier(&mut alice);
    ink_mount(&rig, &mut alice, "counter", 1);
    ink_barrier(&mut alice);
    ink_accept(&mut alice, 1);
    assert!(ink_points(&mut alice, 1).is_empty());
    let mut peers = [
        (socket(&rig.public, &household, Some('b')), "counter"),
        (socket(&rig.public, &work, Some('a')), "counter"),
        (socket(&rig.public, &household, Some('a')), "counter/other"),
        (socket(&rig.public, &household, Some('a')), "sibling"),
    ];
    for (peer, id) in &mut peers {
        ink_mount(&rig, peer, id, 1);
        ink_accept(peer, 1);
        ink_control(peer, true);
        assert!(ink_points(peer, 1).is_empty());
    }
    let root = Point {
        phase: 0,
        stroke: 1,
        position: [321, 654],
    };
    ink_send(&mut alice, 1, root);
    assert_eq!(ink_points(&mut alice, 1), vec![root]);
    assert_eq!(ink_points(&mut peers[0].0, 1), vec![root]);
    for (peer, _) in &mut peers[1..] {
        ink_control(peer, false);
        ink_control(peer, true);
        assert!(
            ink_points(peer, 1).is_empty(),
            "Space, document and app rooms must be isolated"
        );
    }
    ink_mount(&rig, &mut alice, "counter/other", 2);
    let staged = Point {
        stroke: 2,
        position: [200, 300],
        ..root
    };
    ink_send(&mut alice, 1, staged);
    assert_eq!(ink_points(&mut alice, 1), vec![staged]);
    assert_eq!(ink_points(&mut peers[0].0, 1), vec![staged]);
    ink_accept(&mut alice, 2);
    assert!(ink_points(&mut alice, 2).is_empty());
    let child = Point {
        stroke: 3,
        position: [800, 900],
        ..root
    };
    ink_send(&mut alice, 1, Point { stroke: 99, ..root }); // late old-app traffic
    ink_send(&mut alice, 2, child);
    assert_eq!(ink_points(&mut alice, 2), vec![child]);
    assert_eq!(ink_points(&mut peers[2].0, 1), vec![child]);
    ink_control(&mut peers[0].0, false);
    ink_control(&mut peers[0].0, true);
    assert_eq!(ink_points(&mut peers[0].0, 1), vec![root, staged]);
    send(&mut alice, ClientMessage::Leave { epoch: 2 });
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if http_path(&rig.native, "/health", "GET", None, None).1["ink_subscribers"] == 4 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Quit did not release its relay subscription"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    ink_send(&mut alice, 2, Point { stroke: 98, ..root });
    ink_control(&mut peers[2].0, false);
    ink_control(&mut peers[2].0, true);
    assert_eq!(ink_points(&mut peers[2].0, 1), vec![child]);
    ink_mount(&rig, &mut alice, "counter", 3);
    ink_accept(&mut alice, 3);
    assert_eq!(ink_points(&mut alice, 3), vec![root, staged]);
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "bob".into(),
            role: None,
        },
    );
    assert!(matches!(
        read(&mut peers[0].0),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    ink_send(&mut peers[0].0, 1, Point { stroke: 97, ..root });
    ink_control(&mut alice, false);
    ink_control(&mut alice, true);
    assert_eq!(ink_points(&mut alice, 3), vec![root, staged]);
    rig.change(
        'a',
        Change::Remove {
            space: household.clone(),
            app: "counter".into(),
        },
    );
    assert!(matches!(
        read(&mut alice),
        ServerMessage::AccessRequired { epoch: 3 }
    ));
    assert!(matches!(
        read(&mut peers[2].0),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    ink_send(&mut peers[3].0, 1, child);
    assert_eq!(
        ink_points(&mut peers[3].0, 1),
        vec![child],
        "sibling app continues"
    );
    rig.change(
        'a',
        Change::Attach {
            space: household,
            app: "counter".into(),
        },
    );
    ink_mount(&rig, &mut alice, "counter", 4);
    ink_accept(&mut alice, 4);
    assert!(
        ink_points(&mut alice, 4).is_empty(),
        "removed attachments do not retain ephemeral trails"
    );
}

#[test]
fn quit_releases_durable_room_capacity_without_evicting_live_or_poisoned_participants() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    let ui = canvas::decode_ui(&fs::read(rig.root.join("apps/counter/package/ui.flex")).unwrap())
        .unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    let mut ids = vec!["counter".to_owned()];
    for index in 1_u8..12 {
        let id = format!("counter/document-{}", char::from(b'a' + index - 1));
        let mut ui = ui.clone();
        ui.ui_ref = format!("app:{id}/ui");
        let bytes = canvas::encode_ui(&ui).unwrap();
        let app = App {
            id: id.clone(),
            title: format!("Document {index}"),
            ui_ref: ui.ui_ref,
            state_ref: format!("app:{id}/state"),
            status: "canvas-app".into(),
            dependencies: vec![],
        };
        let package = rig.root.join("apps").join(&id).join("package");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("ui.flex"), &bytes).unwrap();
        fs::write(package.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
        catalog.apps.push(CatalogEntry {
            app,
            path: id.clone(),
            ui_hash: canvas::hash(&bytes),
            private: false,
        });
        ids.push(id);
    }
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    let spaces = (0..22)
        .map(|index| {
            let space = rig.space(&format!("Room budget {index}"));
            rig.change(
                'a',
                Change::Attach {
                    space: space.clone(),
                    app: "counter".into(),
                },
            );
            space
        })
        .collect::<Vec<_>>();
    document_session(&rig, &spaces[0], &ids[0], schema(), Some(41));
    document_session(&rig, &spaces[0], &ids[1], schema(), Some(73));
    let open = |space: &str, id: &str| {
        let mut socket = socket(&rig.public, space, Some('a'));
        let state_ref = format!("app:{id}/state");
        send(
            &mut socket,
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: format!("app:{id}/ui"),
                state_ref: state_ref.clone(),
                catalog_hash: canvas::hash(&rig.catalog),
                known_ui: None,
                known_code: vec![],
                writer: vec![9; 16],
                resume: 0,
                collection: None,
            },
        );
        let ServerMessage::Mounted {
            state,
            binding: Some(binding),
            ..
        } = read(&mut socket)
        else {
            panic!("mount")
        };
        let replica = canvas::StateAdapter::new(schema(), &state_ref, &state, [9; 16]).unwrap();
        send(&mut socket, ClientMessage::AcceptMount { epoch: 1 });
        assert!(matches!(
            read(&mut socket),
            ServerMessage::Activated { epoch: 1 }
        ));
        (socket, replica, binding.collection)
    };
    let (mut active, mut replica, _) = open(&spaces[0], &ids[0]);
    let (mut poisoned, _, collection) = open(&spaces[0], &ids[1]);
    let journal = rig.root.join(format!(
        "state/collection-{collection}.data/doc-{}.merkle.log",
        canvas::hex(&canvas::hash(format!("app:{}/state", ids[1]).as_bytes()))
    ));
    let saved = journal.with_extension("saved");
    fs::rename(&journal, &saved).unwrap();
    fs::create_dir(&journal).unwrap();
    send(
        &mut poisoned,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 1,
                values: vec![(1, Value::Number(999.))],
            })
            .unwrap(),
        },
    );
    assert!(matches!(read(&mut poisoned), ServerMessage::Error { .. }));
    fs::remove_dir(&journal).unwrap();
    fs::rename(saved, &journal).unwrap();
    drop(poisoned);
    for (index, space) in spaces.iter().enumerate() {
        for (document, id) in ids.iter().enumerate() {
            if index == 0 && document < 2 {
                continue;
            }
            let value = (index * ids.len() + document + 1) as u16;
            assert_eq!(
                document_session(&rig, space, id, schema(), Some(value)).0,
                value
            );
        }
    }
    let (status, health) = http_path(&rig.native, "/health", "GET", None, None);
    assert_eq!(status, 200);
    assert_eq!(
        health["app_roots"], 256,
        "264 distinct documents fit a bounded cache through idle eviction"
    );
    // Revisit every healthy document, including those evicted from RAM.
    for (index, space) in spaces.iter().enumerate() {
        for (document, id) in ids.iter().enumerate() {
            if index == 0 && document < 2 {
                continue;
            }
            assert_eq!(
                document_session(&rig, space, id, schema(), None).0,
                (index * ids.len() + document + 1) as u16
            );
        }
    }
    let mut rejected = socket(&rig.public, &spaces[0], Some('a'));
    send(
        &mut rejected,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: format!("app:{}/ui", ids[1]),
            state_ref: format!("app:{}/state", ids[1]),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![8; 16],
            resume: 0,
            collection: None,
        },
    );
    assert!(
        matches!(read(&mut rejected), ServerMessage::Error { message, .. } if message.contains("uncertain"))
    );
    assert_eq!(replica.value(1).unwrap(), 41);
    replica.set(&[(1, 42)]).unwrap();
    while let Some(bytes) = replica.generate().unwrap() {
        send(&mut active, ClientMessage::Sync { epoch: 1, bytes });
    }
    while !replica.confirmed() {
        if let ServerMessage::Sync { bytes, .. } = read(&mut active) {
            replica.receive(&bytes).unwrap();
        }
    }
    drop(active);
    drop(rejected);
    rig.stop();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &spaces[0], &ids[0], schema(), None).0,
        42
    );
    assert_eq!(
        document_session(&rig, &spaces[0], &ids[1], schema(), None).0,
        73
    );
}

#[test]
fn owned_documents_share_one_collection_without_aliasing_and_migrate_all_confirmed_roots() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start_mode(&binary, false);
    rig.stop();
    let mut child_ui =
        canvas::decode_ui(&fs::read(rig.root.join("apps/counter/package/ui.flex")).unwrap())
            .unwrap();
    child_ui.ui_ref = "app:counter/details/ui".into();
    child_ui.state_schema.id = "counter-details-v1".into();
    let child_schema = child_ui.state_schema.clone();
    let child_ui = canvas::encode_ui(&child_ui).unwrap();
    let app = App {
        id: "counter/details".into(),
        title: "Details".into(),
        ui_ref: "app:counter/details/ui".into(),
        state_ref: "app:counter/details/state".into(),
        status: "canvas-app".into(),
        dependencies: vec![],
    };
    let package = rig.root.join("apps/counter/details/package");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("ui.flex"), &child_ui).unwrap();
    fs::write(package.join("app.flex"), canvas::encode_app(&app).unwrap()).unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps.push(CatalogEntry {
        app,
        path: "counter/details".into(),
        ui_hash: canvas::hash(&child_ui),
        private: false,
    });
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, false, false);
    let legacy_space = "a".repeat(32);
    assert_eq!(
        document_session(&rig, &legacy_space, "counter", schema(), Some(41)).0,
        41
    );
    assert_eq!(
        document_session(
            &rig,
            &legacy_space,
            "counter/details",
            child_schema.clone(),
            Some(73)
        )
        .0,
        73
    );
    rig.stop();
    let manifest = rig.root.join("multi-root-provision.json");
    private_json(
        &manifest,
        &json!({"version":1,"spaces":[
            {"name":"Household","admins":["alice"],"members":[],"apps":[{"id":"counter","legacy_roots":["app:counter/state","app:counter/details/state"]}]},
            {"name":"Work","admins":["alice"],"members":[],"apps":[{"id":"counter"}]}
        ]}),
    );
    let provisioned = rig.provision(&binary, &manifest);
    assert!(
        provisioned.status.success(),
        "{}",
        String::from_utf8_lossy(&provisioned.stderr)
    );
    let record: Json = serde_json::from_slice(&provisioned.stdout).unwrap();
    assert_eq!(record["spaces"][0]["imports"].as_array().unwrap().len(), 2);
    let household = record["spaces"][0]["space"].as_str().unwrap();
    let work = record["spaces"][1]["space"].as_str().unwrap();
    rig.restart(&binary, true, true);
    let root = document_session(&rig, household, "counter", schema(), None);
    let child = document_session(
        &rig,
        household,
        "counter/details",
        child_schema.clone(),
        None,
    );
    assert_eq!(root.0, 41);
    assert_eq!(child.0, 73);
    assert_eq!(root.1, child.1);
    let other_root = document_session(&rig, work, "counter", schema(), None);
    let other_child = document_session(&rig, work, "counter/details", child_schema.clone(), None);
    assert_eq!(other_root.0, 0);
    assert_eq!(other_child.0, 0);
    assert_eq!(other_root.1, other_child.1);
    assert_ne!(root.1, other_root.1);
    rig.stop();
    // The state-directory marker recovers all owned documents without the flag.
    rig.restart(&binary, false, true);
    assert_eq!(
        document_session(&rig, household, "counter", schema(), None).0,
        41
    );
    assert_eq!(
        document_session(
            &rig,
            household,
            "counter/details",
            child_schema.clone(),
            None
        )
        .0,
        73
    );
    rig.stop();
    let collection = root.1.unwrap();
    let checkpoint = rig
        .root
        .join(format!("state/collection-{collection}.merkle"));
    let child_checkpoint = rig.root.join(format!(
        "state/collection-{collection}.data/doc-{}.merkle",
        canvas::hex(&canvas::hash(b"app:counter/details/state"))
    ));
    let before = [
        canvas::hash(&fs::read(&checkpoint).unwrap()),
        canvas::hash(&fs::read(&child_checkpoint).unwrap()),
    ];
    let good_catalog = rig.catalog.clone();
    let mut incompatible = canvas::decode_ui(&child_ui).unwrap();
    incompatible.state_schema.id = "counter-details-v2".into();
    let incompatible = canvas::encode_ui(&incompatible).unwrap();
    fs::write(package.join("ui.flex"), &incompatible).unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps.last_mut().unwrap().ui_hash = canvas::hash(&incompatible);
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    let mut rejected = socket(&rig.public, household, Some('a'));
    mount(&mut rejected, &rig.catalog);
    assert!(matches!(
        read(&mut rejected),
        ServerMessage::AccessRequired { .. }
    ));
    let (_, registry) = http(&rig.public, "GET", Some('a'), None);
    let request = Request {
        version: 1,
        expected_generation: registry["generation"].as_u64().unwrap(),
        change: Change::Attach {
            space: household.into(),
            app: "counter".into(),
        },
    };
    assert_eq!(http(&rig.public, "POST", Some('a'), Some(&request)).0, 409);
    assert_eq!(canvas::hash(&fs::read(&checkpoint).unwrap()), before[0]);
    assert_eq!(
        canvas::hash(&fs::read(&child_checkpoint).unwrap()),
        before[1]
    );
    rig.stop();
    // Restoring compatible code reopens both original documents and identities.
    rig.catalog = good_catalog;
    fs::write(package.join("ui.flex"), &child_ui).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, household, "counter", schema(), None),
        (41, Some(collection.clone()))
    );
    assert_eq!(
        document_session(&rig, household, "counter/details", child_schema, None),
        (73, Some(collection))
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
    let ServerMessage::Mounted { state, binding, .. } = message else {
        panic!("mount rejected: {message:?}");
    };
    let mut replica =
        canvas::StateAdapter::new(schema(), "app:counter/state", &[], [9; 16]).unwrap();
    replica.receive(&state).unwrap();
    if mode == "write" {
        send(&mut socket, ClientMessage::AcceptMount { epoch: 1 });
        if binding.is_some() {
            assert!(matches!(
                read(&mut socket),
                ServerMessage::Activated { epoch: 1 }
            ));
        }
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
fn removing_an_attachment_revokes_sessions_retains_admin_visible_data_and_reattaches_after_restart()
{
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
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
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    rig.child(&rig.public, &household, Some('a'), "write");
    let mut active = socket(&rig.public, &household, Some('b'));
    mount(&mut active, &rig.catalog);
    let ServerMessage::Mounted {
        binding: Some(binding),
        ..
    } = read(&mut active)
    else {
        panic!("mount")
    };
    send(&mut active, ClientMessage::AcceptMount { epoch: 1 });
    assert!(matches!(
        read(&mut active),
        ServerMessage::Activated { epoch: 1 }
    ));
    rig.change(
        'a',
        Change::Remove {
            space: household.clone(),
            app: "counter".into(),
        },
    );
    assert!(matches!(
        read(&mut active),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    send(
        &mut active,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 2,
                values: vec![(1, Value::Number(99.))],
            })
            .unwrap(),
        },
    );
    assert!(matches!(read(&mut active), ServerMessage::Error { .. }));
    let (_, admin) = http(&rig.public, "GET", Some('a'), None);
    let retained = admin["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == household)
        .unwrap()["apps"][0]
        .clone();
    assert_eq!(retained["collection"], binding.collection);
    assert_eq!(retained["attached"], false);
    let (_, member) = http(&rig.public, "GET", Some('b'), None);
    assert!(member["spaces"][0]["apps"].as_array().unwrap().is_empty());
    rig.child(&rig.public, &work, Some('a'), "0");
    rig.stop();
    rig.restart(&binary, true, true);
    rig.change(
        'a',
        Change::Attach {
            space: household.clone(),
            app: "counter".into(),
        },
    );
    rig.child(&rig.public, &household, Some('a'), "42");
    rig.child(&rig.public, &work, Some('a'), "0");
    let (_, admin) = http(&rig.public, "GET", Some('a'), None);
    let restored = admin["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == household)
        .unwrap()["apps"][0]
        .clone();
    assert_eq!(restored["collection"], binding.collection);
    assert_eq!(restored["attached"], true);
}

#[test]
fn revocation_between_qualification_and_acceptance_cannot_activate_a_staged_collection() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let rig = Rig::start(&binary);
    let space = rig.space("Launch race");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    let mut candidate = socket(&rig.public, &space, Some('b'));
    mount(&mut candidate, &rig.catalog);
    assert!(matches!(
        read(&mut candidate),
        ServerMessage::Mounted {
            binding: Some(_),
            ..
        }
    ));
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: None,
        },
    );
    send(&mut candidate, ClientMessage::AcceptMount { epoch: 1 });
    assert!(matches!(
        read(&mut candidate),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    send(
        &mut candidate,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 1,
                values: vec![(1, Value::Number(99.))],
            })
            .unwrap(),
        },
    );
    // An activation ack queued ahead of this error would fail the assertion.
    assert!(matches!(read(&mut candidate), ServerMessage::Error { .. }));
    rig.child(&rig.public, &space, Some('a'), "0");
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
