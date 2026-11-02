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
        if self.root.join("releases/current.json").is_file() {
            command
                .arg("--releases")
                .arg(self.root.join("releases"))
                .arg("--bundle-key")
                .arg(self.root.join("public.pem"));
        }
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
    fn storage_action(&self, binary: &str, action: &str) -> std::process::Output {
        Command::new(binary)
            .arg("--spaces-registry")
            .arg(self.root.join("spaces.flex"))
            .arg("--app-state")
            .arg(self.root.join("state"))
            .args(["--spaces-storage", action])
            .output()
            .unwrap()
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
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "otrios-spaces-process-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    private_file(path, value.to_string().as_bytes());
}

fn private_file(path: &Path, bytes: &[u8]) {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(bytes).unwrap();
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
    // A completed pre-retention import receipt repairs attribution without
    // replaying payloads or restoring historical admission.
    remove_legacy_attribution(&rig);
    let recovered = rig.provision(&binary, &manifest);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_legacy_attribution(&rig);
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
    // Completed historical receipts do not require currently installed code.
    let original_catalog = rig.catalog.clone();
    chat_fixture(&mut rig);
    let mut without_counter = canvas::decode_catalog(&rig.catalog).unwrap();
    without_counter
        .apps
        .retain(|entry| entry.app.id == "sibling");
    fs::write(
        rig.root.join("apps/catalog.flex"),
        canvas::encode_catalog(&without_counter).unwrap(),
    )
    .unwrap();
    rig.catalog = original_catalog;
    let repeated = rig.provision(&binary, &manifest);
    assert!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    let finalized = rig.storage_action(&binary, "finalize-legacy");
    assert!(
        finalized.status.success(),
        "{}",
        String::from_utf8_lossy(&finalized.stderr)
    );
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
    socket_auth(address, space, token, None)
}
fn socket_auth(address: &str, space: &str, token: Option<char>, cookie: Option<&str>) -> Socket {
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
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("Cookie", cookie.parse().unwrap());
    }
    let (mut socket, _) = tungstenite::connect(request).unwrap();
    if let MaybeTlsStream::Plain(t) = socket.get_mut() {
        t.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    }
    socket.send(Message::Text(json!({"type":"hello","version":1,"release":null,"state_protocol":"otrios-canvas-sync-v24","state":true}).to_string().into())).unwrap();
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

#[test]
fn offline_backup_restore_preserves_latest_revocation_and_older_writers_cannot_admit_restored_state()
 {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let space = rig.space("Backup household");
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
            identity: "frame".into(),
            role: Some(Role::Member),
        },
    );
    let (confirmed, collection) = document_session(&rig, &space, "counter", schema(), Some(42));
    assert_eq!(confirmed, 42);
    let collection = collection.unwrap();
    assert!(
        !rig.storage_action(&binary, "backup").status.success(),
        "live authority ownership must exclude offline snapshotting"
    );
    rig.stop();
    let copied = rig.storage_action(&binary, "backup");
    assert!(
        copied.status.success(),
        "{}",
        String::from_utf8_lossy(&copied.stderr)
    );
    let id = serde_json::from_slice::<Json>(&copied.stdout).unwrap()["backup"]
        .as_str()
        .unwrap()
        .to_owned();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), Some(73)).0,
        73
    );
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "frame".into(),
            role: None,
        },
    );
    let generation = http(&rig.public, "GET", Some('a'), None).1["generation"].clone();
    rig.stop();
    let restored = rig.storage_action(&binary, &format!("restore:{id}"));
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        42
    );
    assert_eq!(
        http(&rig.public, "GET", Some('a'), None).1["generation"],
        generation
    );
    rig.child(&rig.native, &space, Some('c'), "deny");
    rig.stop();
    let checkpoint = rig
        .root
        .join(format!("state/collection-{collection}.merkle"));
    let held = rig.root.join("held-checkpoint");
    fs::rename(&checkpoint, &held).unwrap();
    fs::create_dir(&checkpoint).unwrap();
    assert!(
        !rig.storage_action(&binary, &format!("restore:{id}"))
            .status
            .success()
    );
    assert!(rig.root.join("spaces.flex.restore").exists());
    rig.restart(&binary, true, false);
    assert_eq!(http(&rig.public, "GET", Some('a'), None).0, 503);
    rig.stop();
    fs::remove_dir(&checkpoint).unwrap();
    fs::rename(held, checkpoint).unwrap();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        42
    );
    assert!(!rig.root.join("spaces.flex.restore").exists());
    rig.child(&rig.native, &space, Some('c'), "deny");
    rig.stop();
    if let Ok(older) = std::env::var("OTRIOS_SPACES_OLD_SERVER") {
        let primary = fs::read(rig.root.join("spaces.flex")).unwrap();
        let latest = fs::read(rig.root.join("spaces.flex.latest")).unwrap();
        assert_eq!(&primary[..5], b"OTSP9");
        rig.restart(&older, true, false);
        assert_eq!(http(&rig.public, "GET", Some('a'), None).0, 503);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(http(&rig.public, "GET", Some('a'), None).0, 503);
        rig.stop();
        assert_eq!(fs::read(rig.root.join("spaces.flex")).unwrap(), primary);
        assert_eq!(
            fs::read(rig.root.join("spaces.flex.latest")).unwrap(),
            latest
        );
    }
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        42
    );
}

#[test]
fn latest_decisions_survive_old_primary_restore_and_missing_storage_cannot_become_empty_data() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let space = rig.space("Recovery household");
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
            identity: "frame".into(),
            role: Some(Role::Member),
        },
    );
    let primary = rig.root.join("spaces.flex");
    assert!(
        otrios_core::spaces::Store::open(primary.clone()).is_err(),
        "a running authority exclusively owns this root"
    );
    let old_primary = fs::read(&primary).unwrap();
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), Some(42)).0,
        42
    );
    rig.child(&rig.native, &space, Some('c'), "42");
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "frame".into(),
            role: None,
        },
    );
    rig.stop();
    fs::write(&primary, old_primary).unwrap();
    rig.restart(&binary, true, true);
    rig.child(&rig.native, &space, Some('c'), "deny");
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        42
    );
    assert_eq!(
        fs::read(&primary).unwrap(),
        fs::read(rig.root.join("spaces.flex.latest")).unwrap()
    );
    rig.stop();
    let unavailable = |rig: &Rig| {
        assert_eq!(http(&rig.public, "GET", Some('a'), None).0, 503);
        let mut request = format!("ws://{}/session", rig.public)
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Origin", ORIGIN.parse().unwrap());
        let (mut socket, _) = tungstenite::connect(request).unwrap();
        if let MaybeTlsStream::Plain(tcp) = socket.get_mut() {
            tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        }
        socket.send(Message::Text(json!({"type":"hello","version":1,"release":null,"state_protocol":"otrios-canvas-sync-v24","state":true}).to_string().into())).unwrap();
        let Message::Text(reply) = socket.read().unwrap() else {
            panic!("release hello remains independent of recovery");
        };
        let reply: Json = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["type"], "hello");
        assert_eq!(reply["compatible"], true);
        assert_eq!(reply["state_available"], false);
    };
    let saved = rig.root.join("saved-state");
    fs::rename(rig.root.join("state"), &saved).unwrap();
    rig.restart(&binary, true, false);
    unavailable(&rig);
    assert!(
        !rig.root.join("state/spaces.root").exists(),
        "missing initialized storage must not be rebound"
    );
    rig.stop();
    fs::remove_dir(rig.root.join("state")).unwrap();
    fs::rename(saved, rig.root.join("state")).unwrap();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        42
    );
    rig.stop();
    let latest = rig.root.join("spaces.flex.latest");
    let saved_latest = rig.root.join("saved-latest");
    fs::rename(&latest, &saved_latest).unwrap();
    rig.restart(&binary, true, false);
    unavailable(&rig);
    assert!(
        !latest.exists(),
        "missing latest ledger must not be reconstructed from an older primary"
    );
    rig.stop();
    fs::rename(saved_latest, latest).unwrap();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        42
    );
    rig.child(&rig.native, &space, Some('c'), "deny");
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
    assert!(snapshot.selected.as_ref().unwrap().cleanup.is_some());
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
    mount_epoch(socket, catalog, 1);
}
fn mount_epoch(socket: &mut Socket, catalog: &[u8], epoch: u32) {
    send(
        socket,
        ClientMessage::Mount {
            epoch,
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
    remove_legacy_attribution(&rig);
    let recovered = rig.provision(&binary, &manifest);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert_legacy_attribution(&rig);
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
    send_spaces(
        &mut bob,
        otrios_interfaces::spaces::Command::Watch {
            version: 1,
            observation: 1,
            selected: None,
            cursor: None,
        },
    );
    let otrios_interfaces::spaces::Event::Snapshot { snapshot, .. } = read_spaces(&mut bob) else {
        panic!("revoked participant can still refresh Home");
    };
    assert!(snapshot.spaces.is_empty());
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
    let (_, sibling) = chat_mount(&rig, &mut frame, "sibling", 2);
    assert!(
        sibling.messages.is_empty(),
        "revoked app cannot leak into the next activation"
    );
    chat_join(&mut frame, 2);
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
    mount_epoch(&mut active, &rig.catalog, 2);
    assert!(matches!(
        read(&mut active),
        ServerMessage::AccessRequired { epoch: 2 }
    ));
    send(&mut active, ClientMessage::Leave { epoch: 1 });
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
    // A queued activation or stale-write response would fail this barrier.
    mount_epoch(&mut candidate, &rig.catalog, 2);
    assert!(matches!(
        read(&mut candidate),
        ServerMessage::AccessRequired { epoch: 2 }
    ));
    send(&mut candidate, ClientMessage::Leave { epoch: 1 });
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

#[test]
fn an_explicit_missing_legacy_checkpoint_never_becomes_an_empty_import() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start_mode(&binary, false);
    rig.stop();
    let original = rig.root.join(format!(
        "state/counter-{}.merkle",
        canvas::hex(&canvas::hash(b"app:counter/state"))
    ));
    // Recovery runs asynchronously after listener startup. Stop it first, then
    // establish a genuinely absent source regardless of how far startup got.
    for path in [&original, &original.with_extension("merkle.log")] {
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => panic!("missing-source fixture: {error}"),
        }
    }
    let manifest = rig.root.join("provision-missing-checkpoint.json");
    private_json(
        &manifest,
        &json!({"version":1,"spaces":[{
        "name":"Missing original","admins":["alice"],"members":[],
        "apps":[{"id":"counter","legacy_state_ref":"app:counter/state"}]}]}),
    );
    let result = rig.provision(&binary, &manifest);
    assert!(!result.status.success());
    assert!(
        !original.exists(),
        "never create an empty replacement for the declared original"
    );
    assert!(rig.root.join("spaces.provisioning").exists());
    rig.restart(&binary, false, false);
    assert_eq!(http(&rig.public, "GET", Some('a'), None).0, 503);
}

fn remove_legacy_attribution(rig: &Rig) {
    let path = rig.root.join("spaces.flex");
    let bytes = fs::read(&path).unwrap();
    let mut record: Json = flexbuffers::from_slice(&bytes[37..]).unwrap();
    record.as_object_mut().unwrap().remove("legacy_sources");
    let payload = flexbuffers::to_vec(&record).unwrap();
    let mut old = b"OTSP4".to_vec();
    old.extend(canvas::hash(&payload));
    old.extend(payload);
    fs::write(path, &old).unwrap();
    fs::write(rig.root.join("spaces.flex.latest"), old).unwrap();
}
fn assert_legacy_attribution(rig: &Rig) {
    let bytes = fs::read(rig.root.join("spaces.flex")).unwrap();
    let record: Json = flexbuffers::from_slice(&bytes[37..]).unwrap();
    assert!(
        !record["legacy_sources"]["files"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn durable_deletion_orders_racing_writes_waits_for_disposal_and_cannot_be_undone_by_restore() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let household = rig.space("Delete household");
    let work = rig.space("Delete isolation");
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
    rig.change(
        'a',
        Change::Membership {
            space: household.clone(),
            identity: "frame".into(),
            role: Some(Role::Member),
        },
    );
    let (_, original) = document_session(&rig, &household, "counter", schema(), Some(42));
    let original = original.unwrap();
    document_session(&rig, &work, "counter", schema(), Some(73));
    rig.stop();
    let backed_up = rig.storage_action(&binary, "backup");
    assert!(
        backed_up.status.success(),
        "{}",
        String::from_utf8_lossy(&backed_up.stderr)
    );
    let backup: Json = serde_json::from_slice(&backed_up.stdout).unwrap();
    let backup = backup["backup"].as_str().unwrap().to_owned();
    let older_primary = fs::read(rig.root.join("spaces.flex")).unwrap();
    rig.restart(&binary, true, true);
    let mut bob = socket(&rig.public, &household, Some('b'));
    mount(&mut bob, &rig.catalog);
    let ServerMessage::Mounted { state, binding, .. } = read(&mut bob) else {
        panic!("old mount");
    };
    assert_eq!(binding.unwrap().collection, original);
    let old_replica =
        canvas::StateAdapter::new(schema(), "app:counter/state", &state, [9; 16]).unwrap();
    assert_eq!(old_replica.value(1).unwrap(), 42);
    send(&mut bob, ClientMessage::AcceptMount { epoch: 1 });
    assert!(matches!(
        read(&mut bob),
        ServerMessage::Activated { epoch: 1 }
    ));
    let (_, current) = http(&rig.public, "GET", Some('a'), None);
    let delete = Request {
        version: 1,
        expected_generation: current["generation"].as_u64().unwrap(),
        change: Change::DeleteData {
            space: household.clone(),
            app: "counter".into(),
            collection: original.clone(),
            remove: false,
        },
    };
    assert_eq!(http(&rig.public, "POST", Some('b'), Some(&delete)).0, 403);
    // Two separate TCP clients race an ordinary write against serialized deletion.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let writer_barrier = barrier.clone();
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        send(
            &mut bob,
            ClientMessage::Sync {
                epoch: 1,
                bytes: canvas::encode_state_request(&StateRequest::Write {
                    sequence: 1,
                    values: vec![(1, Value::Number(99.))],
                })
                .unwrap(),
            },
        );
        bob
    });
    barrier.wait();
    assert_eq!(http(&rig.public, "POST", Some('a'), Some(&delete)).0, 200);
    let mut bob = writer.join().unwrap();
    loop {
        match read(&mut bob) {
            ServerMessage::AccessRequired { epoch: 1 } => break,
            ServerMessage::Sync { .. } => (), // the write may commit before deletion
            other => panic!("revocation: {other:?}"),
        }
    }
    let old_path = rig.root.join(format!("state/collection-{original}.merkle"));
    assert!(
        old_path.exists(),
        "keep files until the reachable host confirms disposal"
    );
    let (_, state) = http(&rig.public, "GET", Some('a'), None);
    let space = state["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == household)
        .unwrap();
    assert_eq!(space["cleanup"]["local_pending"], 1);
    assert!(space["cleanup"]["backup_until_unix_ms"].as_u64().is_some());
    let replacement = space["apps"][0]["collection"].as_str().unwrap().to_owned();
    assert_ne!(replacement, original);
    let (_, member) = http(&rig.public, "GET", Some('b'), None);
    assert!(member["spaces"][0].get("cleanup").is_none());
    // A stale write cannot recreate the old collection or enter its successor.
    send(
        &mut bob,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 2,
                values: vec![(1, Value::Number(77.))],
            })
            .unwrap(),
        },
    );
    send(
        &mut bob,
        ClientMessage::Sync {
            epoch: 1,
            bytes: vec![0],
        },
    );
    send(
        &mut bob,
        ClientMessage::Mount {
            epoch: 2,
            ui_ref: "app:counter/ui".into(),
            state_ref: "app:counter/state".into(),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![9; 16],
            resume: 99,
            collection: Some(original.clone()),
        },
    );
    let ServerMessage::Mounted { state, binding, .. } = read(&mut bob) else {
        panic!("replacement mount");
    };
    assert_eq!(binding.unwrap().collection, replacement);
    let fresh = canvas::StateAdapter::new(schema(), "app:counter/state", &state, [9; 16]).unwrap();
    assert_eq!(fresh.value(1).unwrap(), 0);
    send(&mut bob, ClientMessage::AcceptMount { epoch: 2 });
    assert!(matches!(
        read(&mut bob),
        ServerMessage::Activated { epoch: 2 }
    ));
    drop(old_replica);
    send(&mut bob, ClientMessage::Leave { epoch: 1 });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (_, state) = http(&rig.public, "GET", Some('a'), None);
        let space = state["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == household)
            .unwrap();
        if space["cleanup"]["local_pending"] == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cleanup did not follow disposal acknowledgement"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!old_path.exists());
    rig.child(&rig.native, &household, Some('c'), "0");
    let (_, state) = http(&rig.public, "GET", Some('a'), None);
    let mut stale_confirmation = delete;
    stale_confirmation.expected_generation = state["generation"].as_u64().unwrap();
    assert_eq!(
        http(&rig.public, "POST", Some('a'), Some(&stale_confirmation)).0,
        409
    );
    assert_eq!(
        document_session(&rig, &work, "counter", schema(), None).0,
        73
    );
    document_session(&rig, &household, "counter", schema(), Some(5));
    drop(bob);
    rig.stop();
    // Neither an older primary nor an older payload snapshot can resurrect it.
    fs::write(rig.root.join("spaces.flex"), older_primary).unwrap();
    let restored = rig.storage_action(&binary, &format!("restore:{backup}"));
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &household, "counter", schema(), None),
        (5, Some(replacement))
    );
    assert_eq!(
        document_session(&rig, &work, "counter", schema(), None).0,
        73
    );
    assert!(!old_path.exists());
}

#[test]
fn deleting_chat_stops_all_owned_producers_and_retains_another_spaces_transcript() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    rig.restart(&binary, true, true);
    let household = rig.space("Chat deletion");
    let work = rig.space("Retained chat");
    for space in [&household, &work] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "relay".into(),
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
    let mut alice = socket(&rig.public, &household, Some('a'));
    let mut frame = socket(&rig.native, &household, Some('c'));
    let mut retained = socket(&rig.public, &work, Some('a'));
    chat_mount(&rig, &mut alice, "relay", 1);
    let (alice_profile, alice_epoch) = chat_join(&mut alice, 1);
    chat_send(
        &mut alice,
        1,
        alice_epoch,
        1,
        "Retired household transcript",
    );
    assert_eq!(chat_window(&mut alice, 1, 1).len(), 1);
    chat_mount(&rig, &mut frame, "relay/other", 1);
    let (frame_profile, frame_epoch) = chat_join(&mut frame, 1);
    chat_send(
        &mut frame,
        1,
        frame_epoch,
        2,
        "Retired subsidiary transcript",
    );
    chat_mount(&rig, &mut retained, "relay", 1);
    let (_, retained_epoch) = chat_join(&mut retained, 1);
    chat_send(
        &mut retained,
        1,
        retained_epoch,
        3,
        "Independent work transcript",
    );
    let (_, before) = http(&rig.public, "GET", Some('a'), None);
    let space = before["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == household)
        .unwrap();
    let old = space["apps"][0]["collection"].as_str().unwrap().to_owned();
    rig.change(
        'a',
        Change::DeleteData {
            space: household.clone(),
            app: "relay".into(),
            collection: old.clone(),
            remove: true,
        },
    );
    for peer in [&mut alice, &mut frame] {
        assert!(matches!(
            chat_state(peer),
            ServerMessage::AccessRequired { epoch: 1 }
        ));
    }
    let path = rig.root.join(format!("state/collection-{old}.data"));
    assert!(path.exists());
    // Old chat and collection-window packets are fenced before decoding.
    chat_command(
        &mut frame,
        1,
        otrios_interfaces::chat::Command::Draft {
            generation: 1,
            revision: 1,
            text: "stale draft".into(),
        },
    );
    for peer in [&mut alice, &mut frame] {
        send(peer, ClientMessage::Leave { epoch: 1 });
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (_, current) = http(&rig.public, "GET", Some('a'), None);
        let space = current["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == household)
            .unwrap();
        assert!(space["apps"].as_array().unwrap().is_empty());
        if space["cleanup"]["local_pending"] == 0 {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !path.exists(),
        "all owned journals, histories and caches were reclaimed"
    );
    chat_send(&mut retained, 1, retained_epoch, 4, "Work continues");
    let messages = chat_window(&mut retained, 1, 1);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].text, "Independent work transcript");
    rig.change(
        'a',
        Change::Attach {
            space: household.clone(),
            app: "relay".into(),
        },
    );
    let (_, first) = chat_mount(&rig, &mut alice, "relay", 2);
    assert!(first.messages.is_empty());
    let (new_alice, new_epoch) = chat_join(&mut alice, 2);
    assert_ne!(new_alice, alice_profile);
    assert_eq!(
        chat_send(&mut alice, 2, new_epoch, 5, "Fresh household transcript"),
        1
    );
    let (_, subsidiary) = chat_mount(&rig, &mut frame, "relay/other", 2);
    assert!(subsidiary.messages.is_empty());
    assert_ne!(chat_join(&mut frame, 2).0, frame_profile);
    assert!(
        !path.exists(),
        "late producers cannot recreate the tombstoned namespace"
    );
}

#[test]
fn missing_owned_chat_is_unavailable_until_verified_payload_restore() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    rig.restart(&binary, true, true);
    let space = rig.space("Chat storage repair");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "relay".into(),
        },
    );
    let mut alice = socket(&rig.public, &space, Some('a'));
    chat_mount(&rig, &mut alice, "relay", 1);
    let (profile, epoch) = chat_join(&mut alice, 1);
    chat_send(&mut alice, 1, epoch, 1, "Confirmed before storage repair");
    drop(alice);
    let (_, state) = http(&rig.public, "GET", Some('a'), None);
    let collection = state["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == space)
        .unwrap()["apps"][0]["collection"]
        .as_str()
        .unwrap()
        .to_owned();
    rig.stop();
    let backup = rig.storage_action(&binary, "backup");
    assert!(backup.status.success());
    let backup: Json = serde_json::from_slice(&backup.stdout).unwrap();
    let path = rig.root.join(format!(
        "state/collection-{collection}.data/chat-primary.journal"
    ));
    fs::remove_file(&path).unwrap();
    rig.restart(&binary, true, true);
    let mut rejected = socket(&rig.public, &space, Some('a'));
    send_spaces(
        &mut rejected,
        otrios_interfaces::spaces::Command::Watch {
            version: 1,
            observation: 1,
            selected: Some(space.clone()),
            cursor: None,
        },
    );
    let otrios_interfaces::spaces::Event::Snapshot { snapshot, .. } = read_spaces(&mut rejected)
    else {
        panic!("discovery");
    };
    let app = &snapshot.selected.unwrap().apps[0];
    assert_eq!(
        app.availability,
        otrios_interfaces::spaces::AppAvailability::Unavailable
    );
    assert!(app.launch.is_none());
    send(
        &mut rejected,
        ClientMessage::Mount {
            epoch: 1,
            ui_ref: "app:relay/ui".into(),
            state_ref: "app:relay/state".into(),
            catalog_hash: canvas::hash(&rig.catalog),
            known_ui: None,
            known_code: vec![],
            writer: vec![9; 16],
            resume: 0,
            collection: Some(collection),
        },
    );
    assert!(matches!(
        read(&mut rejected),
        ServerMessage::Error { epoch: 1, .. }
    ));
    assert!(
        !path.exists(),
        "never replace missing confirmed history with an empty room"
    );
    drop(rejected);
    rig.stop();
    let restored = rig.storage_action(
        &binary,
        &format!("restore:{}", backup["backup"].as_str().unwrap()),
    );
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    rig.restart(&binary, true, true);
    let mut alice = socket(&rig.public, &space, Some('a'));
    let (_, initial) = chat_mount(&rig, &mut alice, "relay", 1);
    assert_eq!(initial.messages.len(), 1);
    assert_eq!(
        initial.messages[0].message.text,
        "Confirmed before storage repair"
    );
    assert_eq!(chat_join(&mut alice, 1).0, profile);
}

#[test]
fn public_audiences_separate_read_comment_edit_and_moderation_across_clients() {
    use otrios_interfaces::{
        chat::{Command as Chat, Event as ChatEvent},
        spaces::Audience,
    };
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    // A real installed closed-only app, independent of the public app data.
    let closed_ui = otrios_core::authoring::AppUi::new(
        "monitoring",
        "Closed monitor",
        otrios_core::authoring::schema("closed-empty-v1", &[]),
    )
    .build("Closed monitor", vec![]);
    let closed_bytes = canvas::encode_ui(&closed_ui).unwrap();
    let closed_app = App {
        id: "monitoring".into(),
        title: "Closed monitor".into(),
        ui_ref: closed_ui.ui_ref,
        state_ref: "app:monitoring/state".into(),
        status: "canvas-app".into(),
        dependencies: vec![],
    };
    let package = rig.root.join("apps/monitoring/package");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("ui.flex"), &closed_bytes).unwrap();
    fs::write(
        package.join("app.flex"),
        canvas::encode_app(&closed_app).unwrap(),
    )
    .unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps.push(CatalogEntry {
        app: closed_app,
        path: "monitoring".into(),
        ui_hash: canvas::hash(&closed_bytes),
        private: true,
    });
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    let public = rig.space("Public conversation");
    let private = rig.space("Private operations");
    for app in ["relay", "counter"] {
        rig.change(
            'a',
            Change::Attach {
                space: public.clone(),
                app: app.into(),
            },
        );
    }
    rig.change(
        'a',
        Change::Attach {
            space: private.clone(),
            app: "monitoring".into(),
        },
    );
    document_session(&rig, &public, "counter", schema(), Some(42));
    let mut member = socket(&rig.public, &public, Some('a'));
    let (collection, _) = chat_mount(&rig, &mut member, "relay", 1);
    let (_, member_epoch) = chat_join(&mut member, 1);
    chat_send(&mut member, 1, member_epoch, 1, "Visible public content");
    rig.change(
        'a',
        Change::Audience {
            space: public.clone(),
            audience: Audience::PublicView,
        },
    );
    let journal = rig.root.join(format!(
        "state/collection-{collection}.data/chat-primary.journal"
    ));
    let before = fs::read(&journal).unwrap();
    let mut anonymous = socket(&rig.public, &public, None);
    let (_, initial) = chat_mount(&rig, &mut anonymous, "relay", 1);
    assert_eq!(initial.messages[0].message.text, "Visible public content");
    assert_eq!(chat_window(&mut anonymous, 1, 1).len(), 1);
    chat_command(&mut anonymous, 1, Chat::Join { credential: None });
    chat_event(&mut anonymous, 1, |event| {
        matches!(event, ChatEvent::Error { .. })
    });
    assert_eq!(
        fs::read(&journal).unwrap(),
        before,
        "read-only viewing cannot create a participant or writer receipt"
    );
    let mut readonly = socket(&rig.native, &public, Some('b'));
    mount(&mut readonly, &rig.catalog);
    let ServerMessage::Mounted {
        binding: Some(binding),
        ..
    } = read(&mut readonly)
    else {
        panic!("read-only mount");
    };
    assert!(!binding.editable && !binding.comments && !binding.moderate);
    send(&mut readonly, ClientMessage::AcceptMount { epoch: 1 });
    assert!(matches!(
        read(&mut readonly),
        ServerMessage::Activated { .. }
    ));
    send(
        &mut readonly,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 1,
                values: vec![(1, Value::Number(99.))],
            })
            .unwrap(),
        },
    );
    // A read-only state write closes the offending app transport, never commits.
    loop {
        match readonly.read() {
            Ok(Message::Ping(_)) => readonly.flush().unwrap(),
            Ok(Message::Binary(bytes)) if bytes.first() == Some(&1) => assert!(!matches!(
                canvas::decode_server(&bytes[1..]).unwrap(),
                ServerMessage::Sync { .. }
            )),
            Ok(Message::Close(_)) | Err(_) => break,
            _ => (),
        }
    }
    assert_eq!(
        document_session(&rig, &public, "counter", schema(), None).0,
        42
    );
    let (_, state) = http(&rig.public, "GET", Some('a'), None);
    let request = Request {
        version: 1,
        expected_generation: state["generation"].as_u64().unwrap(),
        change: Change::Audience {
            space: private.clone(),
            audience: Audience::PublicView,
        },
    };
    assert_eq!(http(&rig.public, "POST", Some('a'), Some(&request)).0, 409);
    let request = Request {
        version: 1,
        expected_generation: state["generation"].as_u64().unwrap(),
        change: Change::Attach {
            space: public.clone(),
            app: "monitoring".into(),
        },
    };
    assert_ne!(http(&rig.public, "POST", Some('a'), Some(&request)).0, 200);
    rig.change(
        'a',
        Change::Audience {
            space: public.clone(),
            audience: Audience::PublicComment,
        },
    );
    assert!(matches!(
        chat_state(&mut anonymous),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    drop(anonymous);
    let mut visitor = socket(&rig.public, &public, Some('b'));
    chat_mount(&rig, &mut visitor, "relay", 1);
    chat_command(&mut visitor, 1, Chat::Join { credential: None });
    let ChatEvent::Welcome {
        you,
        credential,
        epoch,
        ..
    } = chat_event(&mut visitor, 1, |e| matches!(e, ChatEvent::Welcome { .. }))
    else {
        unreachable!();
    };
    assert!(!you.admin && credential.is_none());
    assert_eq!(
        chat_send(&mut visitor, 1, epoch, 2, "Signed visitor comment"),
        2
    );
    chat_command(
        &mut visitor,
        1,
        Chat::Kick {
            participant: you.profile.id.clone(),
            generation: you.generation,
            reason: String::new(),
        },
    );
    chat_event(&mut visitor, 1, |e| matches!(e, ChatEvent::Error { .. }));
    chat_command(
        &mut visitor,
        1,
        Chat::Authenticate {
            username: "alice".into(),
            password: "irrelevant".into(),
        },
    );
    chat_event(&mut visitor, 1, |e| matches!(e, ChatEvent::Error { .. }));
    let messages = chat_window(&mut member, 1, 1);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].text, "Signed visitor comment");
    rig.change(
        'a',
        Change::Audience {
            space: public.clone(),
            audience: Audience::PublicView,
        },
    );
    assert!(matches!(
        chat_state(&mut visitor),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    drop(visitor);
    rig.change(
        'a',
        Change::Audience {
            space: public.clone(),
            audience: Audience::Closed,
        },
    );
    let mut denied = socket(&rig.native, &public, Some('b'));
    mount(&mut denied, &rig.catalog);
    assert!(matches!(
        read(&mut denied),
        ServerMessage::AccessRequired { .. }
    ));
    let mut denied = socket(&rig.public, &private, None);
    mount(&mut denied, &rig.catalog);
    assert!(matches!(
        read(&mut denied),
        ServerMessage::AccessRequired { .. }
    ));
}

fn browser_session(address: &str, method: &str, cookie: Option<&str>) -> Option<String> {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let body = if method == "POST" {
        r#"{"username":"visitor@spaces.test","password":"password"}"#
    } else {
        ""
    };
    write!(stream, "{method} /auth/session HTTP/1.1\r\nHost: spaces.test\r\nOrigin: {ORIGIN}\r\nX-OTriOS-Request: session\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", body.len()).unwrap();
    if let Some(cookie) = cookie {
        write!(stream, "Cookie: {cookie}\r\n").unwrap();
    }
    write!(stream, "\r\n{body}").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "session request failed"
    );
    response.lines().find_map(|line| {
        line.split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, value)| value.trim().split(';').next().unwrap().to_owned())
    })
}

#[test]
fn public_signout_revokes_the_old_identity_and_actions_even_when_read_access_remains() {
    use otrios_interfaces::{
        chat::{Command as Chat, Event as ChatEvent},
        spaces::Audience,
    };
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    let auth_path = rig.root.join("auth.json");
    let mut auth: Json = serde_json::from_slice(&fs::read(&auth_path).unwrap()).unwrap();
    auth["users"] = json!([{"username":"visitor@spaces.test","identity":"visitor","password_hash":"$argon2id$v=19$m=65536,t=2,p=1$c29tZXNhbHQ$CTFhFdXPJO1aFaMaO6Mm5c8y7cJHAph8ArZWb2GRPPc"}]);
    fs::write(&auth_path, serde_json::to_vec(&auth).unwrap()).unwrap();
    rig.restart(&binary, true, true);
    let space = rig.space("Public identity boundary");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "relay".into(),
        },
    );
    rig.change(
        'a',
        Change::Audience {
            space: space.clone(),
            audience: Audience::PublicComment,
        },
    );
    let cookie = browser_session(&rig.public, "POST", None).unwrap();
    let mut visitor = socket_auth(&rig.public, &space, None, Some(&cookie));
    let (collection, _) = chat_mount(&rig, &mut visitor, "relay", 1);
    chat_command(&mut visitor, 1, Chat::Join { credential: None });
    let ChatEvent::Welcome { epoch, you, .. } =
        chat_event(&mut visitor, 1, |e| matches!(e, ChatEvent::Welcome { .. }))
    else {
        unreachable!();
    };
    assert!(!you.admin);
    chat_send(&mut visitor, 1, epoch, 1, "Before sign-out");
    browser_session(&rig.public, "DELETE", Some(&cookie));
    assert!(matches!(
        chat_state(&mut visitor),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    // Read is still public, but this activation's identity and comment grant are gone.
    chat_command(
        &mut visitor,
        1,
        Chat::Send {
            epoch,
            nonce: format!("{:032x}", 2),
            text: "Must not post after sign-out".into(),
        },
    );
    send(&mut visitor, ClientMessage::Leave { epoch: 1 });
    let (_, initial) = chat_mount(&rig, &mut visitor, "relay", 2);
    assert_eq!(initial.messages.len(), 1);
    assert_eq!(initial.messages[0].message.text, "Before sign-out");
    assert_eq!(chat_window(&mut visitor, 2, 1).len(), 1);
    assert!(
        rig.root
            .join(format!(
                "state/collection-{collection}.data/chat-primary.journal"
            ))
            .exists()
    );
}

#[test]
fn publication_copies_confirmed_content_and_owned_assets_without_cross_space_access_or_deletion_links()
 {
    use otrios_interfaces::{
        chat::{Command as Chat, Event as ChatEvent},
        spaces::Audience,
    };
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    rig.restart(&binary, true, true);
    let source = rig.space("Private drafts");
    let destination = rig.space("Public copies");
    for space in [&source, &destination] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "relay".into(),
            },
        );
    }
    rig.change(
        'a',
        Change::Membership {
            space: source.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    rig.change(
        'a',
        Change::Audience {
            space: destination.clone(),
            audience: Audience::PublicComment,
        },
    );
    let mut author = socket(&rig.public, &source, Some('a'));
    let (source_collection, _) = chat_mount(&rig, &mut author, "relay", 1);
    let (source_profile, receipt_epoch) = chat_join(&mut author, 1);
    let payload = (0..=255).cycle().take(200_000).collect::<Vec<u8>>();
    chat_command(
        &mut author,
        1,
        Chat::SendAsset {
            epoch: receipt_epoch,
            nonce: format!("{:032x}", 1),
            text: "Selected confirmed draft with private bytes".into(),
            name: "diagram.bin".into(),
            bytes: payload.clone(),
        },
    );
    chat_event(&mut author, 1, |e| {
        matches!(e, ChatEvent::Receipt { sequence: 1, .. })
    });
    let mut reader = socket(&rig.public, &source, Some('b'));
    let (_, selected) = chat_mount(&rig, &mut reader, "relay", 1);
    let selection = selected.messages[0].clone();
    if let Ok(old) = std::env::var("OTRIOS_SPACES_OLD_CHAT_SERVER") {
        // Exercise the real pre-attachment writer against a private copy of the
        // new journal. Its failure must never truncate a complete new record.
        let rollback = rig.root.join("old-chat-writer");
        fs::create_dir_all(rollback.join("state")).unwrap();
        let bytes = fs::read(rig.root.join(format!(
            "state/collection-{source_collection}.data/chat-primary.journal"
        )))
        .unwrap();
        assert_eq!(&bytes[..8], b"OTCHAT02");
        let original = rollback.join("state/brass-relay.journal");
        private_file(&original, &bytes);
        let manifest = rollback.join("provision.json");
        private_json(
            &manifest,
            &json!({"version":2,"spaces":[{"name":"Rollback probe","admins":["alice"],"members":[],"apps":[{"id":"relay","legacy_chat_root":"app:relay/state"}]}]}),
        );
        let output = Command::new(old)
            .arg("--apps")
            .arg(rig.root.join("apps"))
            .arg("--app-state")
            .arg(rollback.join("state"))
            .arg("--spaces-registry")
            .arg(rollback.join("spaces.flex"))
            .arg("--provision-spaces")
            .arg(manifest)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Chat journal format"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(original).unwrap(), bytes);
    }
    assert_eq!(selection.message.assets.len(), 1);
    // A journal-only legacy manifest cannot silently strand owned blob files.
    let unsupported = rig.root.join("unsupported-asset-import");
    fs::create_dir_all(unsupported.join("state")).unwrap();
    let original = unsupported.join("state/brass-relay.journal");
    let original_bytes = fs::read(rig.root.join(format!(
        "state/collection-{source_collection}.data/chat-primary.journal"
    )))
    .unwrap();
    private_file(&original, &original_bytes);
    let manifest = unsupported.join("provision.json");
    private_json(
        &manifest,
        &json!({"version":2,"spaces":[{"name":"Unsupported asset import","admins":["alice"],"members":[],"apps":[{"id":"relay","legacy_chat_root":"app:relay/state"}]}]}),
    );
    let rejected = Command::new(&binary)
        .arg("--apps")
        .arg(rig.root.join("apps"))
        .arg("--app-state")
        .arg(unsupported.join("state"))
        .arg("--spaces-registry")
        .arg(unsupported.join("spaces.flex"))
        .arg("--provision-spaces")
        .arg(manifest)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("owned attachments"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    assert_eq!(fs::read(original).unwrap(), original_bytes);
    let mut public = socket(&rig.public, &destination, None);
    let (first_destination_collection, empty) = chat_mount(&rig, &mut public, "relay", 1);
    assert!(empty.messages.is_empty());
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: selection.revision,
            index: 0,
        },
    );
    chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Error { .. }));
    let generation = http(&rig.public, "GET", Some('a'), None).1["generation"]
        .as_u64()
        .unwrap();
    let publication = Chat::Publish {
        epoch: receipt_epoch,
        nonce: format!("{:032x}", 2),
        sequence: 1,
        revision: selection.revision,
        destination_space: destination.clone(),
        expected_generation: generation,
    };
    // Source membership and destination public-comment access do not confer Edit.
    chat_command(&mut reader, 1, publication.clone());
    chat_event(&mut reader, 1, |e| matches!(e, ChatEvent::Error { .. }));
    chat_command(&mut author, 1, publication.clone());
    chat_event(&mut author, 1, |e| {
        matches!(e, ChatEvent::Published { sequence: 1, .. })
    });
    chat_command(&mut author, 1, publication);
    chat_event(&mut author, 1, |e| {
        matches!(e, ChatEvent::Published { sequence: 1, .. })
    });
    drop(public);
    let mut public = socket(&rig.public, &destination, None);
    let (_, copied) = chat_mount(&rig, &mut public, "relay", 1);
    assert_eq!(copied.messages.len(), 1);
    let copy = copied.messages[0].clone();
    assert_eq!(copy.message.text, selection.message.text);
    assert_eq!(copy.message.assets, selection.message.assets);
    assert_ne!(copy.message.sender.id, source_profile);
    assert_ne!(copy.message.nonce, selection.message.nonce);
    let exposed = serde_json::to_string(&copied).unwrap();
    assert!(
        !exposed.contains(&source_collection)
            && !exposed.contains(&source_profile)
            && !exposed.contains("authority_identity")
    );
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
            index: 0,
        },
    );
    let ChatEvent::Asset { bytes, .. } =
        chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Asset { .. }))
    else {
        unreachable!()
    };
    assert_eq!(bytes, payload);
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: selection.revision,
            index: 0,
        },
    );
    chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Error { .. }));
    let mut visitor = socket(&rig.public, &destination, Some('b'));
    chat_mount(&rig, &mut visitor, "relay", 1);
    chat_command(&mut visitor, 1, Chat::Join { credential: None });
    let ChatEvent::Welcome {
        epoch: visitor_epoch,
        you,
        ..
    } = chat_event(&mut visitor, 1, |e| matches!(e, ChatEvent::Welcome { .. }))
    else {
        unreachable!()
    };
    assert!(!you.admin);
    chat_send(
        &mut visitor,
        1,
        visitor_epoch,
        3,
        "Comment belongs only to the public destination",
    );
    chat_command(
        &mut visitor,
        1,
        Chat::Redact {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
        },
    );
    chat_event(&mut visitor, 1, |e| matches!(e, ChatEvent::Error { .. }));
    let mut source_probe = socket(&rig.public, &source, Some('a'));
    let (_, unchanged) = chat_mount(&rig, &mut source_probe, "relay", 1);
    assert_eq!(unchanged.messages.len(), 1);
    assert_eq!(unchanged.messages[0], selection);
    drop(source_probe);
    let mut moderator = socket(&rig.public, &destination, Some('a'));
    let (_, moderation_view) = chat_mount(&rig, &mut moderator, "relay", 1);
    chat_join(&mut moderator, 1);
    for row in moderation_view.messages.iter().rev() {
        chat_command(
            &mut moderator,
            1,
            Chat::Redact {
                nonce: "0".repeat(32),
                sequence: row.message.sequence,
                revision: row.revision,
            },
        );
        chat_event(&mut moderator, 1, |event| {
            matches!(event, ChatEvent::Redacted { .. })
        });
    }
    // Knowing a former reference never grants access to unreferenced blobs.
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
            index: 0,
        },
    );
    chat_event(&mut public, 1, |event| {
        matches!(event, ChatEvent::Error { .. })
    });
    drop(moderator);
    // Deleting the published collection leaves its private source readable.
    rig.change(
        'a',
        Change::DeleteData {
            space: destination.clone(),
            app: "relay".into(),
            collection: first_destination_collection.clone(),
            remove: false,
        },
    );
    drop(public);
    drop(visitor);
    chat_command(
        &mut author,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: selection.revision,
            index: 0,
        },
    );
    let ChatEvent::Asset { bytes, .. } =
        chat_event(&mut author, 1, |e| matches!(e, ChatEvent::Asset { .. }))
    else {
        unreachable!()
    };
    assert_eq!(bytes, payload);
    // An old reviewed destination cannot target the replacement, even with the
    // same app/Space names and still-current source epoch.
    let stale = Chat::Publish {
        epoch: receipt_epoch,
        nonce: format!("{:032x}", 4),
        sequence: 1,
        revision: selection.revision,
        destination_space: destination.clone(),
        expected_generation: generation,
    };
    chat_command(&mut author, 1, stale);
    chat_event(&mut author, 1, |e| matches!(e, ChatEvent::Error { .. }));
    let generation = http(&rig.public, "GET", Some('a'), None).1["generation"]
        .as_u64()
        .unwrap();
    chat_command(
        &mut author,
        1,
        Chat::Publish {
            epoch: receipt_epoch,
            nonce: format!("{:032x}", 5),
            sequence: 1,
            revision: selection.revision,
            destination_space: destination.clone(),
            expected_generation: generation,
        },
    );
    chat_event(&mut author, 1, |e| {
        matches!(e, ChatEvent::Published { sequence: 1, .. })
    });
    let mut public = socket(&rig.public, &destination, None);
    let (destination_collection, new_copy) = chat_mount(&rig, &mut public, "relay", 1);
    assert_ne!(destination_collection, first_destination_collection);
    let copy = new_copy.messages[0].clone();
    assert_eq!(new_copy.messages.len(), 1);
    // Conversely, permanent source deletion never follows a link into the copy.
    rig.change(
        'a',
        Change::DeleteData {
            space: source.clone(),
            app: "relay".into(),
            collection: source_collection.clone(),
            remove: false,
        },
    );
    drop(author);
    drop(reader);
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
            index: 0,
        },
    );
    let ChatEvent::Asset { bytes, .. } =
        chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Asset { .. }))
    else {
        unreachable!()
    };
    assert_eq!(bytes, payload);
    let mut fresh_source = socket(&rig.public, &source, Some('a'));
    let (fresh_collection, empty) = chat_mount(&rig, &mut fresh_source, "relay", 1);
    assert_ne!(fresh_collection, source_collection);
    assert!(empty.messages.is_empty());
    let generation = http(&rig.public, "GET", Some('a'), None).1["generation"]
        .as_u64()
        .unwrap();
    chat_command(
        &mut fresh_source,
        1,
        Chat::Publish {
            epoch: receipt_epoch,
            nonce: format!("{:032x}", 6),
            sequence: 1,
            revision: selection.revision,
            destination_space: destination.clone(),
            expected_generation: generation,
        },
    );
    chat_event(&mut fresh_source, 1, |e| {
        matches!(e, ChatEvent::Error { .. })
    });
    drop(fresh_source);
    drop(public);
    let deadline = Instant::now() + Duration::from_secs(5);
    while rig
        .root
        .join(format!("state/collection-{source_collection}.data"))
        .exists()
    {
        assert!(
            Instant::now() < deadline,
            "deleted source namespace remained pinned"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    rig.stop();
    rig.restart(&binary, true, true);
    let mut public = socket(&rig.public, &destination, None);
    let (_, recovered) = chat_mount(&rig, &mut public, "relay", 1);
    assert_eq!(recovered.messages[0], copy);
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
            index: 0,
        },
    );
    let ChatEvent::Asset { bytes, .. } =
        chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Asset { .. }))
    else {
        unreachable!()
    };
    assert_eq!(bytes, payload);
    drop(public);
    rig.stop();
    let backup = rig.storage_action(&binary, "backup");
    assert!(
        backup.status.success(),
        "{}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let backup = serde_json::from_slice::<Json>(&backup.stdout).unwrap()["backup"]
        .as_str()
        .unwrap()
        .to_owned();
    let blob = rig.root.join(format!(
        "state/collection-{destination_collection}.data/asset-{}.blob",
        copy.message.assets[0].sha256
    ));
    assert_eq!(fs::read(&blob).unwrap(), payload);
    fs::remove_file(&blob).unwrap();
    let restored = rig.storage_action(&binary, &format!("restore:{backup}"));
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    rig.restart(&binary, true, true);
    let mut public = socket(&rig.public, &destination, None);
    let (_, restored_copy) = chat_mount(&rig, &mut public, "relay", 1);
    assert_eq!(restored_copy.messages[0], copy);
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
            index: 0,
        },
    );
    let ChatEvent::Asset { bytes, .. } =
        chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Asset { .. }))
    else {
        unreachable!()
    };
    assert_eq!(bytes, payload);
    rig.change(
        'a',
        Change::DeleteData {
            space: destination.clone(),
            app: "relay".into(),
            collection: destination_collection.clone(),
            remove: false,
        },
    );
    drop(public);
    let deadline = Instant::now() + Duration::from_secs(5);
    while blob.parent().unwrap().exists() {
        assert!(
            Instant::now() < deadline,
            "deleted destination remained pinned"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    rig.stop();
    let restored = rig.storage_action(&binary, &format!("restore:{backup}"));
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert!(
        !blob.parent().unwrap().exists(),
        "a payload backup cannot recreate a deleted asset namespace"
    );
    rig.restart(&binary, true, true);
    let mut public = socket(&rig.public, &destination, None);
    let (replacement, empty) = chat_mount(&rig, &mut public, "relay", 1);
    assert_ne!(replacement, destination_collection);
    assert!(empty.messages.is_empty());
    chat_command(
        &mut public,
        1,
        Chat::ReadAsset {
            nonce: "0".repeat(32),
            sequence: 1,
            revision: copy.revision,
            index: 0,
        },
    );
    chat_event(&mut public, 1, |e| matches!(e, ChatEvent::Error { .. }));
}

#[test]
fn retained_data_and_deleted_spaces_recover_real_confirmed_state_across_authority_restart() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let space = rig.space("Recovery Home");
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
    let (value, cid) = document_session(&rig, &space, "counter", schema(), Some(41));
    assert_eq!(value, 41);
    let cid = cid.unwrap();
    rig.change(
        'a',
        Change::Remove {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    let member = http(&rig.public, "GET", Some('b'), None).1;
    assert!(member["spaces"][0]["apps"].as_array().unwrap().is_empty());
    assert!(member["spaces"][0].get("retained").is_none());
    rig.stop();
    rig.restart(&binary, true, true);
    rig.change(
        'a',
        Change::RestoreDataset {
            space: space.clone(),
            app: "counter".into(),
            collection: cid.clone(),
        },
    );
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None),
        (41, Some(cid.clone()))
    );
    rig.change(
        'a',
        Change::DeleteSpace {
            space: space.clone(),
            remove_data: false,
        },
    );
    assert!(
        http(&rig.public, "GET", Some('b'), None).1["spaces"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        http(&rig.public, "GET", None, None).1["spaces"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        http(&rig.public, "GET", Some('a'), None).1["spaces"][0]["archived"],
        true
    );
    rig.stop();
    rig.restart(&binary, true, true);
    rig.child(&rig.public, &space, Some('b'), "deny");
    rig.change(
        'a',
        Change::RestoreSpace {
            space: space.clone(),
        },
    );
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None),
        (41, Some(cid.clone()))
    );
    rig.change(
        'a',
        Change::Remove {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    rig.change(
        'a',
        Change::DeleteDataset {
            space: space.clone(),
            app: "counter".into(),
            collection: cid.clone(),
        },
    );
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    let (value, fresh) = document_session(&rig, &space, "counter", schema(), None);
    assert_eq!(value, 0);
    assert_ne!(fresh.as_deref(), Some(cid.as_str()));
    rig.stop();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None),
        (0, fresh)
    );
}
#[test]
fn sealed_restore_uses_complete_durable_state_and_keeps_outgoing_data_after_restart() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let space = rig.space("Backup Home");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    let (_, old) = document_session(&rig, &space, "counter", schema(), Some(41));
    let old = old.unwrap();
    rig.stop();
    // Seed the immutable backup that the later replacement milestone will create.
    // No public management operation can inject a caller-selected collection ID.
    let source = "f".repeat(32);
    for entry in fs::read_dir(rig.root.join("state")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if let Some(tail) = name.strip_prefix(&format!("collection-{old}.")) {
            assert!(entry.metadata().unwrap().is_file());
            if !tail.ends_with("owner.lock") {
                fs::copy(
                    entry.path(),
                    rig.root.join(format!("state/collection-{source}.{tail}")),
                )
                .unwrap();
            }
        }
    }
    let raw = fs::read(rig.root.join("spaces.flex.latest")).unwrap();
    let mut record: Json = flexbuffers::from_slice(&raw[37..]).unwrap();
    let app = record["registry"]["spaces"][0]["apps"][0].clone();
    let generation = record["registry"]["generation"].clone();
    record["registry"]["retained"].as_array_mut().unwrap().push(json!({"space":space,"policy":app["policy"],"collection":source,"label":"Earlier Counter","origin":"before-replacement","generation":generation}));
    let payload = flexbuffers::to_vec(&record).unwrap();
    let mut raw = b"OTSP7".to_vec();
    raw.extend(canvas::hash(&payload));
    raw.extend(payload);
    for file in ["spaces.flex", "spaces.flex.latest"] {
        fs::write(rig.root.join(file), &raw).unwrap();
    }
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), Some(73)).0,
        73
    );
    let before = fs::read(rig.root.join(format!("state/collection-{old}.merkle"))).unwrap();
    rig.change(
        'a',
        Change::RestoreDataset {
            space: space.clone(),
            app: "counter".into(),
            collection: source.clone(),
        },
    );
    let (value, new) = document_session(&rig, &space, "counter", schema(), None);
    assert_eq!(value, 41);
    assert_ne!(new.as_deref(), Some(old.as_str()));
    assert_ne!(new.as_deref(), Some(source.as_str()));
    let state = http(&rig.public, "GET", Some('a'), None).1;
    let backups = state["spaces"][0]["retained"].as_array().unwrap();
    assert_eq!(backups.len(), 2);
    assert!(backups.iter().any(|d| d["collection"] == old));
    assert!(backups.iter().any(|d| d["collection"] == source));
    assert_eq!(
        fs::read(rig.root.join(format!("state/collection-{old}.merkle"))).unwrap(),
        before
    );
    rig.stop();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None),
        (41, new)
    );
}

#[test]
fn scoped_moves_preserve_confirmed_state_back_up_occupied_targets_and_resolve_after_restart() {
    use otrios_interfaces::spaces::{Command as SpaceCommand, Event, ManagementStatus, MoveMode};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    for (index, mode) in [MoveMode::AppAndData, MoveMode::AppOnly, MoveMode::DataOnly]
        .into_iter()
        .enumerate()
    {
        let source = rig.space(&format!("Move source {index}"));
        let destination = rig.space(&format!("Move destination {index}"));
        for space in [&source, &destination] {
            rig.change(
                'a',
                Change::Attach {
                    space: space.clone(),
                    app: "counter".into(),
                },
            );
        }
        let (_, original) = document_session(&rig, &source, "counter", schema(), Some(41));
        let original = original.unwrap();
        let (_, outgoing) = document_session(&rig, &destination, "counter", schema(), Some(73));
        let outgoing = outgoing.unwrap();
        let current = http(&rig.public, "GET", Some('a'), None).1;
        let operation = format!("{index:032x}");
        let request = Request {
            version: 1,
            expected_generation: current["generation"].as_u64().unwrap(),
            change: Change::Move {
                operation: operation.clone(),
                source: source.clone(),
                destination: destination.clone(),
                app: "counter".into(),
                collection: original.clone(),
                destination_collection: Some(outgoing.clone()),
                mode,
            },
        };
        assert_eq!(http(&rig.public, "POST", Some('b'), Some(&request)).0, 403);
        let mut acknowledged_race = false;
        if mode == MoveMode::DataOnly {
            let mut writer = socket(&rig.public, &source, Some('a'));
            mount(&mut writer, &rig.catalog);
            assert!(matches!(read(&mut writer), ServerMessage::Mounted { .. }));
            send(&mut writer, ClientMessage::AcceptMount { epoch: 1 });
            assert!(matches!(read(&mut writer), ServerMessage::Activated { .. }));
            let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
            let writer_gate = gate.clone();
            let writing = std::thread::spawn(move || {
                writer_gate.wait();
                send(
                    &mut writer,
                    ClientMessage::Sync {
                        epoch: 1,
                        bytes: canvas::encode_state_request(&StateRequest::Write {
                            sequence: 1,
                            values: vec![(1, Value::Number(99.))],
                        })
                        .unwrap(),
                    },
                );
                writer
            });
            gate.wait();
            assert_eq!(http(&rig.public, "POST", Some('a'), Some(&request)).0, 200);
            let mut writer = writing.join().unwrap();
            loop {
                match read(&mut writer) {
                    ServerMessage::Sync { .. } => acknowledged_race = true,
                    ServerMessage::AccessRequired { .. } => break,
                    other => panic!("move revocation: {other:?}"),
                }
            }
            send(
                &mut writer,
                ClientMessage::Sync {
                    epoch: 1,
                    bytes: canvas::encode_state_request(&StateRequest::Write {
                        sequence: 2,
                        values: vec![(1, Value::Number(88.))],
                    })
                    .unwrap(),
                },
            );
            send(&mut writer, ClientMessage::Leave { epoch: 1 });
        } else {
            assert_eq!(http(&rig.public, "POST", Some('a'), Some(&request)).0, 200);
        }
        let after = http(&rig.public, "GET", Some('a'), None).1;
        let generation = after["generation"].as_u64().unwrap();
        let target = after["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == destination)
            .unwrap();
        assert_eq!(target["apps"].as_array().unwrap().len(), 1);
        let (value, target_cid) = document_session(&rig, &destination, "counter", schema(), None);
        if mode == MoveMode::AppOnly {
            assert_eq!((value, target_cid), (73, Some(outgoing)));
            let source_state = after["spaces"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == source)
                .unwrap();
            assert_eq!(source_state["apps"][0]["attached"], false);
            assert_eq!(source_state["apps"][0]["collection"], original);
        } else {
            assert!(value == 41 || mode == MoveMode::DataOnly && value == 99);
            if acknowledged_race {
                assert_eq!(
                    value, 99,
                    "every acknowledged write must reach the destination"
                );
            }
            assert_ne!(target_cid.as_ref(), Some(&original));
            assert_ne!(target_cid.as_ref(), Some(&outgoing));
            assert!(
                target["retained"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["collection"] == outgoing && d["origin"] == "before-replacement")
            );
            if mode == MoveMode::DataOnly {
                let (value, replacement) =
                    document_session(&rig, &source, "counter", schema(), None);
                assert_eq!(value, 0);
                assert_ne!(replacement, Some(original));
            }
        }
        rig.stop();
        rig.restart(&binary, true, true);
        assert_eq!(
            http(&rig.public, "POST", Some('a'), Some(&request)).0,
            200,
            "known operation must not execute again"
        );
        assert_eq!(
            http(&rig.public, "GET", Some('a'), None).1["generation"],
            generation
        );
        let mut control = socket(&rig.public, &destination, Some('a'));
        send_spaces(
            &mut control,
            SpaceCommand::Watch {
                version: 1,
                observation: 1,
                selected: Some(destination.clone()),
                cursor: None,
            },
        );
        assert!(matches!(read_spaces(&mut control), Event::Snapshot { .. }));
        send_spaces(
            &mut control,
            SpaceCommand::MoveStatus {
                version: 1,
                id: 1,
                operation,
                source,
                destination: destination.clone(),
            },
        );
        assert!(
            matches!(read_spaces(&mut control), Event::MoveStatus { status: ManagementStatus::Applied, generation: Some(g), .. } if g == generation)
        );
        assert_eq!(
            document_session(&rig, &destination, "counter", schema(), None).0,
            value
        );
    }
}

#[test]
fn scoped_chat_move_copies_owned_assets_and_revokes_old_producers_without_mutable_links() {
    use otrios_interfaces::{
        chat::{Command as Chat, Event as ChatEvent},
        spaces::MoveMode,
    };
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    rig.restart(&binary, true, true);
    let source = rig.space("Move private transcript");
    let destination = rig.space("Move transcript destination");
    for space in [&source, &destination] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "relay".into(),
            },
        );
    }
    let mut producer = socket(&rig.public, &source, Some('a'));
    let (old, _) = chat_mount(&rig, &mut producer, "relay", 1);
    let (_, epoch) = chat_join(&mut producer, 1);
    let payload = vec![37; 8192];
    chat_command(
        &mut producer,
        1,
        Chat::SendAsset {
            epoch,
            nonce: "1".repeat(32),
            text: "Confirmed owned attachment".into(),
            name: "asset.bin".into(),
            bytes: payload.clone(),
        },
    );
    chat_event(&mut producer, 1, |e| {
        matches!(e, ChatEvent::Receipt { sequence: 1, .. })
    });
    let mut outgoing = socket(&rig.public, &destination, Some('a'));
    let (prior, _) = chat_mount(&rig, &mut outgoing, "relay", 1);
    let (_, prior_epoch) = chat_join(&mut outgoing, 1);
    chat_send(&mut outgoing, 1, prior_epoch, 2, "Destination backup");
    rig.change(
        'a',
        Change::Move {
            operation: "2".repeat(32),
            source: source.clone(),
            destination: destination.clone(),
            app: "relay".into(),
            collection: old.clone(),
            destination_collection: Some(prior),
            mode: MoveMode::AppAndData,
        },
    );
    loop {
        if matches!(
            chat_state(&mut producer),
            ServerMessage::AccessRequired { epoch: 1 }
        ) {
            break;
        }
    }
    // A delayed old producer cannot enter the moved dataset or resurrect its source.
    chat_command(
        &mut producer,
        1,
        Chat::Send {
            epoch,
            nonce: "3".repeat(32),
            text: "Delayed old write".into(),
        },
    );
    send(&mut producer, ClientMessage::Leave { epoch: 1 });
    send(&mut outgoing, ClientMessage::Leave { epoch: 1 });
    drop(producer);
    drop(outgoing);
    rig.stop();
    rig.restart(&binary, true, true);
    let mut reader = socket(&rig.public, &destination, Some('a'));
    let (fresh, initial) = chat_mount(&rig, &mut reader, "relay", 1);
    assert_ne!(fresh, old);
    assert_eq!(initial.messages.len(), 1);
    let message = &initial.messages[0];
    assert_eq!(message.message.text, "Confirmed owned attachment");
    chat_command(
        &mut reader,
        1,
        Chat::ReadAsset {
            nonce: "4".repeat(32),
            sequence: 1,
            revision: message.revision,
            index: 0,
        },
    );
    let event = chat_event(&mut reader, 1, |e| matches!(e, ChatEvent::Asset { .. }));
    let ChatEvent::Asset { bytes, .. } = event else {
        unreachable!()
    };
    assert_eq!(bytes, payload);
    let old_assets = rig.root.join(format!("state/collection-{old}.data"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while old_assets.exists() {
        assert!(
            Instant::now() < deadline,
            "old source cleanup must not retain writable files"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        rig.root
            .join(format!("state/collection-{fresh}.data"))
            .is_dir()
    );
}

#[test]
fn admin_usage_counts_only_accepted_current_mounts_deduplicates_people_and_counts_anonymous_connections()
 {
    use otrios_interfaces::spaces::{Audience, Command as SpaceCommand, Event};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let rig = Rig::start(&binary);
    let space = rig.space("Accepted participants");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    rig.change(
        'a',
        Change::Audience {
            space: space.clone(),
            audience: Audience::PublicView,
        },
    );
    let mut control = socket(&rig.public, &space, Some('a'));
    let mut observation = 0;
    let mut snapshot = |control: &mut Socket| {
        observation += 1;
        send_spaces(
            control,
            SpaceCommand::Watch {
                version: 1,
                observation,
                selected: Some(space.clone()),
                cursor: None,
            },
        );
        loop {
            if let Event::Snapshot {
                observation: actual,
                snapshot,
                ..
            } = read_spaces(control)
                && actual == observation
            {
                break snapshot;
            }
        }
    };
    let mut peers = vec![];
    for token in [Some('a'), Some('a'), Some('b'), None, None] {
        let mut peer = socket(&rig.public, &space, token);
        mount(&mut peer, &rig.catalog);
        assert!(matches!(read(&mut peer), ServerMessage::Mounted { .. }));
        let staged = snapshot(&mut control);
        assert_eq!(
            staged.selected.as_ref().unwrap().active_users,
            Some(match peers.len() {
                0 => 0,
                1 | 2 => 1,
                3 => 2,
                _ => 3,
            })
        );
        send(&mut peer, ClientMessage::AcceptMount { epoch: 1 });
        assert!(matches!(read(&mut peer), ServerMessage::Activated { .. }));
        peers.push(peer);
    }
    let active = snapshot(&mut control);
    let generation = active.generation;
    let detail = active.selected.as_ref().unwrap();
    assert_eq!(detail.active_users, Some(4));
    assert_eq!(detail.apps[0].active_users, Some(4));
    let mut visitor = socket(&rig.public, &space, Some('b'));
    send_spaces(
        &mut visitor,
        SpaceCommand::Watch {
            version: 1,
            observation: 1,
            selected: Some(space.clone()),
            cursor: None,
        },
    );
    let Event::Snapshot {
        snapshot: member, ..
    } = read_spaces(&mut visitor)
    else {
        panic!("visitor snapshot")
    };
    assert_eq!(member.selected.as_ref().unwrap().active_users, None);
    assert_eq!(member.selected.as_ref().unwrap().apps[0].active_users, None);
    drop(peers.pop());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = snapshot(&mut control);
        assert_eq!(
            current.generation, generation,
            "presence never changes registry decisions"
        );
        if current.selected.as_ref().unwrap().active_users == Some(3) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    rig.change(
        'a',
        Change::Audience {
            space: space.clone(),
            audience: Audience::Closed,
        },
    );
    assert_eq!(
        snapshot(&mut control)
            .selected
            .as_ref()
            .unwrap()
            .active_users,
        Some(1),
        "current admission excludes revoked visitors immediately"
    );
}

fn portable_download(
    address: &str,
    space: &str,
    app: &str,
    collection: &str,
    actor: Option<char>,
) -> (u16, String, Vec<u8>) {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket, "GET /transfers/data/{space}/{app}/{collection} HTTP/1.0\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\n").unwrap();
    if let Some(actor) = actor {
        write!(
            socket,
            "Authorization: Bearer {}\r\n",
            actor.to_string().repeat(64)
        )
        .unwrap();
    }
    socket.write_all(b"\r\n").unwrap();
    let mut bytes = vec![];
    socket.read_to_end(&mut bytes).unwrap();
    let boundary = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let headers = std::str::from_utf8(&bytes[..boundary]).unwrap().to_owned();
    let status = headers
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u16>()
        .unwrap();
    (status, headers, bytes[boundary + 4..].to_vec())
}

#[test]
fn portable_export_reads_confirmed_owned_data_and_denies_deleted_or_unadmitted_bindings() {
    use otrios_core::transfers::{self as transfer, RootPolicy, archive};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    // Frame learns only the configured public share origin on the trusted
    // listener. Loopback, an Origin header, and a browser-shaped request confer
    // no native admission; no Space/account metadata belongs in this response.
    let context = |address: &str, actor: Option<char>, origin: bool, requested: bool| {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(
            socket,
            "GET /transfers/context HTTP/1.0\r\nHost: {address}\r\n"
        )
        .unwrap();
        if let Some(actor) = actor {
            write!(
                socket,
                "Authorization: Bearer {}\r\n",
                actor.to_string().repeat(64)
            )
            .unwrap();
        }
        if origin {
            write!(socket, "Origin: {ORIGIN}\r\n").unwrap();
        }
        if requested {
            socket
                .write_all(b"X-OTriOS-Request: transfers\r\n")
                .unwrap();
        }
        socket.write_all(b"\r\n").unwrap();
        let mut bytes = vec![];
        socket.read_to_end(&mut bytes).unwrap();
        let at = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
        let status: u16 = std::str::from_utf8(&bytes[..at])
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        (
            status,
            serde_json::from_slice::<Json>(&bytes[at + 4..]).unwrap_or(Json::Null),
        )
    };
    assert_eq!(
        context(&rig.native, Some('a'), false, true),
        (200, json!({"origin":ORIGIN}))
    );
    assert_eq!(context(&rig.native, None, false, true).0, 403);
    assert_eq!(context(&rig.native, Some('a'), true, true).0, 403);
    assert_eq!(context(&rig.native, Some('a'), false, false).0, 403);
    assert_eq!(context(&rig.public, Some('a'), false, true).0, 404);
    let space = rig.space("Export source");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    let (_, collection) = document_session(&rig, &space, "counter", schema(), Some(42));
    let collection = collection.unwrap();
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    for (address, actor) in [
        (&rig.public, None),
        (&rig.native, None),
        (&rig.public, Some('b')),
        (&rig.native, Some('c')),
    ] {
        assert_eq!(
            portable_download(address, &space, "counter", &collection, actor).0,
            403
        );
    }
    let missing = "f".repeat(32);
    assert_eq!(
        portable_download(&rig.public, &space, "counter", &missing, Some('a')).0,
        403
    );
    let assert_payload = |payload: Vec<u8>| {
        let data = transfer::inspect_data(archive::decode(archive::Kind::Data, &payload).unwrap())
            .unwrap();
        assert_eq!(data.info().bundle_id, "counter");
        assert_eq!(data.info().codec, transfer::COLLECTION_CODEC);
        let schema_hash = data.info().schema_sha256.clone();
        let imported = transfer::QualifiedCollection::qualify(
            data,
            "counter",
            &schema_hash,
            &[RootPolicy {
                state_ref: "app:counter/state".into(),
                schema: schema(),
                chat: false,
            }],
        )
        .unwrap();
        let stage = rig.root.join("inspect-export");
        fs::create_dir(&stage).unwrap();
        imported
            .write_stage(&stage, "app:counter/state", &"e".repeat(32))
            .unwrap();
        let document = canvas::Document::load(
            schema(),
            "app:counter/state",
            &fs::read(stage.join(format!("collection-{}.merkle", "e".repeat(32)))).unwrap(),
        )
        .unwrap();
        assert_eq!(document.value(1).unwrap(), 42);
        fs::remove_dir_all(stage).unwrap();
    };
    let (status, headers, payload) =
        portable_download(&rig.public, &space, "counter", &collection, Some('a'));
    assert_eq!(status, 200, "{headers}");
    assert!(headers.contains("application/vnd.otrios.app-data+zip"));
    assert!(headers.contains("no-store"));
    assert!(headers.contains("counter.otdata"));
    assert_payload(payload);
    rig.change(
        'a',
        Change::Remove {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    let (status, _, payload) =
        portable_download(&rig.public, &space, "counter", &collection, Some('a'));
    assert_eq!(status, 200, "detaching retains exportable confirmed data");
    assert_payload(payload);
    let exported_before_restart =
        portable_download(&rig.public, &space, "counter", &collection, Some('a')).2;
    rig.stop();
    rig.restart(&binary, true, true);
    let (status, _, payload) =
        portable_download(&rig.native, &space, "counter", &collection, Some('a'));
    assert_eq!(status, 200);
    assert_eq!(payload, exported_before_restart);
    rig.change(
        'a',
        Change::DeleteData {
            space: space.clone(),
            app: "counter".into(),
            collection: collection.clone(),
            remove: true,
        },
    );
    assert_eq!(
        portable_download(&rig.public, &space, "counter", &collection, Some('a')).0,
        403
    );
}

#[test]
fn portable_export_keeps_history_beyond_the_view_window_all_roots_and_owned_assets() {
    use otrios_core::transfers::{self as transfer, RootPolicy, archive};
    use otrios_interfaces::chat::{Command as Chat, Event as ChatEvent};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    chat_fixture(&mut rig);
    rig.restart(&binary, true, true);
    let space = rig.space("Complete chat export");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "relay".into(),
        },
    );
    let mut writer = socket(&rig.public, &space, Some('a'));
    let (collection, _) = chat_mount(&rig, &mut writer, "relay", 1);
    let (_, epoch) = chat_join(&mut writer, 1);
    let owned = [vec![91; 200_000], vec![92; 200_000], vec![93; 200_000]];
    for (index, asset) in owned.iter().enumerate() {
        let sequence = index as u64 + 1;
        chat_command(
            &mut writer,
            1,
            Chat::SendAsset {
                epoch,
                nonce: format!("{sequence:032x}"),
                text: "Retained asset beyond the UI window".into(),
                name: "owned.bin".into(),
                bytes: asset.clone(),
            },
        );
        chat_event(
            &mut writer,
            1,
            |e| matches!(e, ChatEvent::Receipt { sequence: found, .. } if *found == sequence),
        );
    }
    // Seed additional confirmed journal entries while the authority is stopped;
    // synthetic time exercises full history without weakening live rate limits.
    drop(writer);
    rig.stop();
    let journal = rig.root.join(format!(
        "state/collection-{collection}.data/chat-primary.journal"
    ));
    let now = 1_000_000;
    let mut history = canvas::chat::Room::open(Some(journal), now).unwrap();
    let (_, credential) = history.identity(None, now).unwrap();
    history
        .join(1, Some(&credential), false, now, true)
        .unwrap();
    for sequence in 4..=66 {
        let at = now + sequence * 1000;
        history
            .handle(
                1,
                Chat::Send {
                    epoch: at / 600_000,
                    nonce: format!("{sequence:032x}"),
                    text: format!("Confirmed row {sequence}"),
                },
                at,
            )
            .unwrap();
    }
    drop(history);
    rig.restart(&binary, true, true);
    let mut writer = socket(&rig.public, &space, Some('a'));
    chat_mount(&rig, &mut writer, "relay/other", 1);
    let (_, other_epoch) = chat_join(&mut writer, 1);
    chat_send(
        &mut writer,
        1,
        other_epoch,
        100,
        "Independent child-root content",
    );
    let (status, _, bytes) =
        portable_download(&rig.public, &space, "relay", &collection, Some('a'));
    assert_eq!(status, 200);
    let data =
        transfer::inspect_data(archive::decode(archive::Kind::Data, &bytes).unwrap()).unwrap();
    assert_eq!(data.assets().len(), 3);
    for asset in &owned {
        assert_eq!(
            data.assets().get(&canvas::hex(&canvas::hash(asset))),
            Some(asset)
        );
    }
    let schema_hash = data.info().schema_sha256.clone();
    let policies = ["app:relay/state", "app:relay/other/state"].map(|state_ref| RootPolicy {
        state_ref: state_ref.into(),
        schema: otrios_core::authoring::schema("chat-empty-v1", &[]),
        chat: true,
    });
    let qualified =
        transfer::QualifiedCollection::qualify(data, "relay", &schema_hash, &policies).unwrap();
    let stage = rig.root.join("qualified-chat-export");
    fs::create_dir(&stage).unwrap();
    let fresh = "e".repeat(32);
    qualified
        .write_stage(&stage, "app:relay/state", &fresh)
        .unwrap();
    let directory = stage.join(format!("collection-{fresh}.data"));
    for (suffix, count) in [
        ("primary".into(), 66),
        (canvas::hex(&canvas::hash(b"app:relay/other/state")), 1),
    ] {
        let room = canvas::chat::Room::open(
            Some(directory.join(format!("chat-{suffix}.journal"))),
            1_000_000,
        )
        .unwrap();
        let pin = room.pin_portable_history().unwrap();
        let history = canvas::chat::QualifiedHistory::decode(&pin.encode().unwrap()).unwrap();
        assert_eq!(history.len(), count);
        if count == 66 {
            assert_eq!(
                history.assets().len(),
                3,
                "offscreen early attachment remains owned"
            );
        }
    }
}

fn portable_upload(
    address: &str,
    space: &str,
    app: &str,
    collection: &str,
    actor: Option<char>,
    payload: &[u8],
) -> (u16, Json) {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket, "POST /transfers/data/{space}/{app}/{collection} HTTP/1.0\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\nX-OTriOS-Request: transfers\r\nContent-Type: application/vnd.otrios.app-data+zip\r\nContent-Length: {}\r\n", payload.len()).unwrap();
    if let Some(actor) = actor {
        write!(
            socket,
            "Authorization: Bearer {}\r\n",
            actor.to_string().repeat(64)
        )
        .unwrap();
    }
    socket.write_all(b"\r\n").unwrap();
    socket.write_all(payload).unwrap();
    let mut bytes = vec![];
    socket.read_to_end(&mut bytes).unwrap();
    let boundary = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let status = std::str::from_utf8(&bytes[..boundary])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes[boundary + 4..]).unwrap_or(Json::Null),
    )
}

#[test]
fn portable_replacement_stages_then_commits_once_and_recovers_a_lost_reply_without_replay() {
    use otrios_interfaces::{
        spaces::{Command as Control, Event, ManagementStatus as Status},
        transfers::{DataReview, ReplacementOutcome},
    };
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let source = rig.space("Import source");
    let destination = rig.space("Import destination");
    for space in [&source, &destination] {
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "counter".into(),
            },
        );
    }
    let (_, source_cid) = document_session(&rig, &source, "counter", schema(), Some(42));
    let (_, destination_cid) = document_session(&rig, &destination, "counter", schema(), Some(7));
    let source_cid = source_cid.unwrap();
    let destination_cid = destination_cid.unwrap();
    let (status, _, payload) =
        portable_download(&rig.public, &source, "counter", &source_cid, Some('a'));
    assert_eq!(status, 200);
    rig.change(
        'a',
        Change::Membership {
            space: destination.clone(),
            identity: "bob".into(),
            role: Some(Role::Admin),
        },
    );
    assert_eq!(
        portable_upload(
            &rig.public,
            &destination,
            "counter",
            &destination_cid,
            None,
            &payload
        )
        .0,
        403
    );
    assert_eq!(
        portable_upload(
            &rig.public,
            &destination,
            "counter",
            &destination_cid,
            Some('c'),
            &payload
        )
        .0,
        403
    );
    let before = http(&rig.public, "GET", Some('a'), None).1;
    assert_eq!(
        portable_upload(
            &rig.public,
            &destination,
            "counter",
            &destination_cid,
            Some('a'),
            b"invalid container"
        )
        .0,
        422
    );
    assert_eq!(http(&rig.public, "GET", Some('a'), None).1, before);
    let (status, review) = portable_upload(
        &rig.public,
        &destination,
        "counter",
        &destination_cid,
        Some('a'),
        &payload,
    );
    assert_eq!(status, 200, "{review}");
    let review: DataReview = serde_json::from_value(review).unwrap();
    assert_eq!(
        http(&rig.public, "GET", Some('a'), None).1,
        before,
        "a staged upload exposes no replacement"
    );
    assert_eq!(
        portable_download(&rig.public, &source, "counter", &source_cid, Some('a')).0,
        429,
        "staged data retains the bounded transfer credit"
    );
    assert_eq!(
        document_session(&rig, &destination, "counter", schema(), None).0,
        7
    );
    let mut bob = socket(&rig.public, &destination, Some('b'));
    send_spaces(
        &mut bob,
        Control::ReplaceData {
            version: 1,
            id: 1,
            stage: review.stage.clone(),
        },
    );
    assert!(matches!(
        read_spaces(&mut bob),
        Event::Result {
            id: 1,
            status: Status::Denied,
            ..
        }
    ));
    let mut control = socket(&rig.public, &destination, Some('a'));
    send_spaces(
        &mut control,
        Control::CancelData {
            version: 1,
            id: 2,
            stage: review.stage,
        },
    );
    assert!(matches!(
        read_spaces(&mut control),
        Event::Result {
            id: 2,
            status: Status::Applied,
            ..
        }
    ));
    assert_eq!(
        portable_download(&rig.public, &source, "counter", &source_cid, Some('a')).0,
        200
    );
    let (_, stale_review) = portable_upload(
        &rig.public,
        &destination,
        "counter",
        &destination_cid,
        Some('a'),
        &payload,
    );
    let stale_review: DataReview = serde_json::from_value(stale_review).unwrap();
    rig.change(
        'b',
        Change::Membership {
            space: destination.clone(),
            identity: "alice".into(),
            role: Some(Role::Member),
        },
    );
    send_spaces(
        &mut control,
        Control::ReplaceData {
            version: 1,
            id: 3,
            stage: stale_review.stage.clone(),
        },
    );
    assert!(matches!(
        read_spaces(&mut control),
        Event::Result {
            id: 3,
            status: Status::Denied,
            ..
        }
    ));
    send_spaces(
        &mut control,
        Control::CancelData {
            version: 1,
            id: 4,
            stage: stale_review.stage,
        },
    );
    assert!(matches!(
        read_spaces(&mut control),
        Event::Result {
            id: 4,
            status: Status::Applied,
            ..
        }
    ));
    rig.change(
        'b',
        Change::Membership {
            space: destination.clone(),
            identity: "alice".into(),
            role: Some(Role::Admin),
        },
    );
    let (_, stale_review) = portable_upload(
        &rig.public,
        &destination,
        "counter",
        &destination_cid,
        Some('a'),
        &payload,
    );
    let stale_review: DataReview = serde_json::from_value(stale_review).unwrap();
    rig.change(
        'a',
        Change::LabelApp {
            space: destination.clone(),
            app: "counter".into(),
            name: "Reviewed destination".into(),
        },
    );
    send_spaces(
        &mut control,
        Control::ReplaceData {
            version: 1,
            id: 5,
            stage: stale_review.stage.clone(),
        },
    );
    assert!(matches!(
        read_spaces(&mut control),
        Event::Result {
            id: 5,
            status: Status::Conflict,
            ..
        }
    ));
    send_spaces(
        &mut control,
        Control::CancelData {
            version: 1,
            id: 6,
            stage: stale_review.stage,
        },
    );
    assert!(matches!(
        read_spaces(&mut control),
        Event::Result {
            id: 6,
            status: Status::Applied,
            ..
        }
    ));
    let mut stale_writer = socket(&rig.public, &destination, Some('a'));
    mount(&mut stale_writer, &rig.catalog);
    assert!(matches!(
        read(&mut stale_writer),
        ServerMessage::Mounted { .. }
    ));
    send(&mut stale_writer, ClientMessage::AcceptMount { epoch: 1 });
    assert!(matches!(
        read(&mut stale_writer),
        ServerMessage::Activated { epoch: 1 }
    ));
    let (status, review) = portable_upload(
        &rig.public,
        &destination,
        "counter",
        &destination_cid,
        Some('a'),
        &payload,
    );
    assert_eq!(status, 200, "{review}");
    let review: DataReview = serde_json::from_value(review).unwrap();
    send_spaces(
        &mut control,
        Control::ReplaceData {
            version: 1,
            id: 7,
            stage: review.stage.clone(),
        },
    );
    // Observe the committed binding independently, then discard the still unread
    // command reply. Reconnection must resolve its exact fresh identity by read.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = http(&rig.public, "GET", Some('a'), None).1;
        let current = state["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == destination)
            .unwrap()["apps"][0]["collection"]
            .as_str()
            .unwrap()
            .to_owned();
        if current == review.replacement.replacement {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "replacement did not commit: {state}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(control);
    assert!(matches!(
        read(&mut stale_writer),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    send(
        &mut stale_writer,
        ClientMessage::Sync {
            epoch: 1,
            bytes: canvas::encode_state_request(&StateRequest::Write {
                sequence: 99,
                values: vec![(1, Value::Number(99.))],
            })
            .unwrap(),
        },
    );
    send(&mut stale_writer, ClientMessage::Leave { epoch: 1 });
    let mut recovered = socket(&rig.public, &destination, Some('a'));
    send_spaces(
        &mut recovered,
        Control::ReplacementStatus {
            version: 1,
            id: 8,
            space: destination.clone(),
            replacement: review.replacement.replacement.clone(),
        },
    );
    assert!(matches!(
        read_spaces(&mut recovered),
        Event::ReplacementStatus {
            id: 8,
            status: Status::Applied,
            outcome: Some(ReplacementOutcome::Committed),
            ..
        }
    ));
    send_spaces(
        &mut recovered,
        Control::ReplaceData {
            version: 1,
            id: 9,
            stage: review.stage,
        },
    );
    assert!(matches!(
        read_spaces(&mut recovered),
        Event::Result {
            id: 9,
            status: Status::Denied,
            ..
        }
    ));
    assert_eq!(
        document_session(&rig, &destination, "counter", schema(), None).0,
        42
    );
    assert_eq!(
        document_session(&rig, &source, "counter", schema(), None).0,
        42
    );
    let (_, _, backup) = portable_download(
        &rig.public,
        &destination,
        "counter",
        &destination_cid,
        Some('a'),
    );
    assert_ne!(
        backup, payload,
        "outgoing data remains a separately exportable sealed backup"
    );
    assert!(!backup.is_empty());
    drop(recovered);
    drop(stale_writer);
    drop(bob);
    rig.stop();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &destination, "counter", schema(), None).0,
        42
    );
    let mut check = socket(&rig.native, &destination, Some('a'));
    send_spaces(
        &mut check,
        Control::ReplacementStatus {
            version: 1,
            id: 10,
            space: destination,
            replacement: review.replacement.replacement,
        },
    );
    assert!(matches!(
        read_spaces(&mut check),
        Event::ReplacementStatus {
            id: 10,
            status: Status::Applied,
            outcome: Some(ReplacementOutcome::Committed),
            ..
        }
    ));
}

fn signed_code_fixture(rig: &mut Rig) -> (otrios_core::bundle::BundleTree, String, String) {
    use otrios_core::bundle;
    let openssl = std::env::var("OTRIOS_TEST_OPENSSL").unwrap_or_else(|_| {
        if Path::new("/opt/homebrew/opt/openssl@3/bin/openssl").exists() {
            "/opt/homebrew/opt/openssl@3/bin/openssl".into()
        } else {
            "openssl".into()
        }
    });
    let run = |args: &[&str]| {
        let output = Command::new(&openssl)
            .args(args)
            .current_dir(&rig.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    run(&["genpkey", "-algorithm", "Ed25519", "-out", "private.pem"]);
    run(&[
        "pkey",
        "-in",
        "private.pem",
        "-pubout",
        "-out",
        "public.pem",
    ]);
    let key = bundle::public_key_der(&run(&[
        "pkey",
        "-in",
        "private.pem",
        "-pubout",
        "-outform",
        "DER",
    ]))
    .unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps[0].private = true;
    catalog.apps[0].app.title = "Code <Counter> & friends".into();
    let ui = fs::read(rig.root.join("apps/counter/package/ui.flex")).unwrap();
    let mut tree = bundle::pack(&catalog.apps[0], 1, &ui, None).unwrap();
    let digest = bundle::inspect(&tree).unwrap().digest;
    fs::write(
        rig.root.join("signature-input.bin"),
        bundle::signing_payload(&digest, &key).unwrap(),
    )
    .unwrap();
    run(&[
        "pkeyutl",
        "-sign",
        "-rawin",
        "-inkey",
        "private.pem",
        "-in",
        "signature-input.bin",
        "-out",
        "publisher.sig",
    ]);
    bundle::attach_signature(
        &mut tree,
        &key,
        &fs::read(rig.root.join("publisher.sig")).unwrap(),
    )
    .unwrap();
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    let mut files = std::collections::BTreeMap::from([
        ("web/index.html".to_owned(), b"signed code fixture".to_vec()),
        ("apps/catalog.flex".to_owned(), rig.catalog.clone()),
    ]);
    for (path, bytes) in &tree {
        files.insert(
            format!("authority/bundles/counter.otapp/{path}"),
            bytes.clone(),
        );
    }
    let mut assets = serde_json::Map::new();
    let mut authority = serde_json::Map::new();
    for (path, bytes) in &files {
        let entry = json!({"bytes":bytes.len(),"sha256":canvas::hex(&canvas::hash(bytes))});
        if path.starts_with("authority/") {
            authority.insert(path.clone(), entry);
        } else {
            assets.insert(path.clone(), entry);
        }
    }
    let manifest = json!({"format":"otrios-release-v1","bundle_abi":"otrios-app-bundle-v1","state_schema":"otrios-canvas-merkle-state-v1","sync_protocol":"otrios-canvas-sync-v24","assets":assets,"authority":authority}).to_string().into_bytes();
    let release = canvas::hex(&canvas::hash(&manifest));
    let root = rig.root.join("releases").join(&release);
    fs::create_dir_all(&root).unwrap();
    for (path, bytes) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fs::write(root.join("manifest.json"), &manifest).unwrap();
    fs::write(rig.root.join("release-manifest.json"), manifest).unwrap();
    run(&[
        "pkeyutl",
        "-sign",
        "-rawin",
        "-inkey",
        "private.pem",
        "-in",
        "release-manifest.json",
        "-out",
        "release-manifest.sig",
    ]);
    fs::copy(
        rig.root.join("release-manifest.sig"),
        root.join("manifest.sig"),
    )
    .unwrap();
    fs::write(rig.root.join("releases/current.json"), json!({"id":release,"web_entry":format!("/releases/{release}/web/index.html"),"sync_protocol":"otrios-canvas-sync-v24"}).to_string()).unwrap();
    (tree, digest, release)
}
fn public_get(address: &str, path: &str) -> (u16, String, Vec<u8>) {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(socket, "GET {path} HTTP/1.0\r\nHost: {address}\r\n\r\n").unwrap();
    let mut bytes = vec![];
    socket.read_to_end(&mut bytes).unwrap();
    let boundary = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let headers = std::str::from_utf8(&bytes[..boundary]).unwrap().to_owned();
    (
        headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
        headers,
        bytes[boundary + 4..].to_vec(),
    )
}
#[test]
fn signed_code_is_public_only_after_explicit_host_and_space_authorized_publication() {
    use otrios_interfaces::spaces::{Command as Control, Event, ManagementStatus as Status};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    let (tree, digest, release) = signed_code_fixture(&mut rig);
    let mut config: Json =
        serde_json::from_slice(&fs::read(rig.root.join("auth.json")).unwrap()).unwrap();
    config["operators"] = json!(["alice"]);
    fs::write(rig.root.join("auth.json"), config.to_string()).unwrap();
    rig.restart(&binary, true, true);
    let space = rig.space("Secret household label");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    rig.change(
        'a',
        Change::LabelApp {
            space: space.clone(),
            app: "counter".into(),
            name: "Secret Space app label".into(),
        },
    );
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Admin),
        },
    );
    let (_, collection) = document_session(&rig, &space, "counter", schema(), Some(42));
    let collection = collection.unwrap();
    let checkpoint = rig
        .root
        .join(format!("state/collection-{collection}.merkle"));
    let before = fs::read(&checkpoint).unwrap();
    let page_path = format!("/apps/counter/{digest}");
    assert_eq!(public_get(&rig.public, &page_path).0, 404);
    assert_eq!(
        public_get(&rig.public, &format!("{page_path}/download")).0,
        404
    );
    assert_eq!(
        public_get(
            &rig.public,
            &format!("/releases/{release}/authority/bundles/counter.otapp/Info.flex")
        )
        .0,
        404
    );
    let share = |id, digest: String| Control::ShareApp {
        version: 1,
        id,
        space: space.clone(),
        app: "counter".into(),
        digest,
        expected_generation: http(&rig.public, "GET", Some('a'), None).1["generation"]
            .as_u64()
            .unwrap(),
        catalog: canvas::hash(&rig.catalog),
    };
    let mut bob = socket(&rig.public, &space, Some('b'));
    send_spaces(&mut bob, share(1, digest.clone()));
    assert!(matches!(
        read_spaces(&mut bob),
        Event::AppShared {
            id: 1,
            status: Status::Denied,
            share: None,
            ..
        }
    ));
    rig.change(
        'b',
        Change::Membership {
            space: space.clone(),
            identity: "alice".into(),
            role: Some(Role::Member),
        },
    );
    let mut alice = socket(&rig.public, &space, Some('a'));
    send_spaces(&mut alice, share(2, digest.clone()));
    assert!(matches!(
        read_spaces(&mut alice),
        Event::AppShared {
            id: 2,
            status: Status::Denied,
            share: None,
            ..
        }
    ));
    rig.change(
        'b',
        Change::Membership {
            space: space.clone(),
            identity: "alice".into(),
            role: Some(Role::Admin),
        },
    );
    send_spaces(&mut alice, share(3, "f".repeat(64)));
    assert!(matches!(
        read_spaces(&mut alice),
        Event::AppShared {
            id: 3,
            status: Status::Conflict,
            share: None,
            ..
        }
    ));
    send_spaces(&mut alice, share(4, digest.clone()));
    let Event::AppShared {
        id: 4,
        status: Status::Applied,
        share: Some(shared),
        ..
    } = read_spaces(&mut alice)
    else {
        panic!("Explicit code-only share")
    };
    assert_eq!(shared.title, "Code <Counter> & friends");
    assert_eq!(shared.digest, digest);
    let (status, headers, html) = public_get(&rig.public, &page_path);
    assert_eq!(status, 200);
    assert!(headers.contains("text/html"));
    let html = String::from_utf8(html).unwrap();
    assert!(html.contains("Code &lt;Counter&gt; &amp; friends"));
    for label in [
        "Download App",
        "Get OTriOS",
        "Project source",
        "Hosting guide",
        "code only",
    ] {
        assert!(html.contains(label), "{label}");
    }
    assert!(!html.contains("Secret"));
    assert!(!html.contains("<Counter>"));
    let (status, headers, archive) = public_get(&rig.public, &format!("{page_path}/download"));
    assert_eq!(status, 200);
    assert!(headers.contains("application/vnd.otrios.app+zip"));
    assert!(headers.contains("counter.otapp.zip"));
    let decoded = otrios_core::transfers::archive::decode(
        otrios_core::transfers::archive::Kind::App,
        &archive,
    )
    .unwrap();
    assert_eq!(
        decoded, tree,
        "Every original signed file is retained, without mutable data or rewritten metadata"
    );
    assert!(otrios_core::bundle::inspect(&decoded).unwrap().info.private);
    assert_eq!(
        fs::read(&checkpoint).unwrap(),
        before,
        "Code sharing cannot rewrite confirmed state"
    );
    rig.change(
        'a',
        Change::Remove {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    assert_eq!(
        public_get(&rig.public, &format!("{page_path}/download")).2,
        archive
    );
    drop(alice);
    drop(bob);
    rig.stop();
    rig.restart(&binary, true, true);
    assert_eq!(
        public_get(&rig.native, &format!("{page_path}/download")).2,
        archive
    );
    assert_eq!(
        public_get(
            &rig.public,
            &format!("/apps/counter/{}/download", "0".repeat(64))
        )
        .0,
        404
    );
}

fn app_intake(
    address: &str,
    space: &str,
    actor: Option<char>,
    content_type: &str,
    payload: &[u8],
) -> (u16, Json) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(stream, "POST /transfers/apps/{space} HTTP/1.0\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\nX-OTriOS-Request: transfers\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n", payload.len()).unwrap();
    if let Some(actor) = actor {
        write!(
            stream,
            "Authorization: Bearer {}\r\n",
            actor.to_string().repeat(64)
        )
        .unwrap();
    }
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(payload).unwrap();
    let mut bytes = vec![];
    stream.read_to_end(&mut bytes).unwrap();
    let end = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    (
        std::str::from_utf8(&bytes[..end])
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap(),
        serde_json::from_slice(&bytes[end + 4..]).unwrap_or(Json::Null),
    )
}

#[test]
fn app_file_intake_is_actor_bound_and_url_intake_cannot_reach_private_destinations() {
    use otrios_core::transfers::{APP_MEDIA_TYPE, AppReview, archive};
    use otrios_interfaces::spaces::{Command as Control, Event, ManagementStatus as Status};
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    rig.stop();
    let (tree, digest, release) = signed_code_fixture(&mut rig);
    let mut config: Json =
        serde_json::from_slice(&fs::read(rig.root.join("auth.json")).unwrap()).unwrap();
    config["operators"] = json!(["alice"]);
    fs::write(rig.root.join("auth.json"), config.to_string()).unwrap();
    rig.restart(&binary, true, true);
    let space = rig.space("Intake destination");
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
            role: Some(Role::Admin),
        },
    );
    let (_, cid) = document_session(&rig, &space, "counter", schema(), Some(71));
    let checkpoint = rig
        .root
        .join(format!("state/collection-{}.merkle", cid.unwrap()));
    let before = fs::read(&checkpoint).unwrap();
    let archive = archive::encode(archive::Kind::App, &tree).unwrap();
    assert_eq!(
        app_intake(&rig.public, &space, None, APP_MEDIA_TYPE, &archive).0,
        403
    );
    for url in [
        format!("https://127.0.0.1/apps/counter/{digest}"),
        format!("https://169.254.169.254/apps/counter/{digest}"),
        format!("https://[::ffff:127.0.0.1]/apps/counter/{digest}"),
    ] {
        assert_eq!(
            app_intake(
                &rig.public,
                &space,
                Some('a'),
                "text/plain; charset=utf-8",
                url.as_bytes()
            )
            .0,
            422
        );
    }
    let bad_url = format!("http://localhost/apps/counter/{digest}");
    assert_eq!(
        app_intake(
            &rig.public,
            &space,
            Some('a'),
            "text/plain; charset=utf-8",
            bad_url.as_bytes()
        )
        .0,
        400
    );
    let mut unsigned = tree.clone();
    unsigned.remove(otrios_interfaces::bundle::SIGNATURE_PATH);
    let unsigned = archive::encode(archive::Kind::App, &unsigned).unwrap();
    assert_eq!(
        app_intake(&rig.public, &space, Some('a'), APP_MEDIA_TYPE, &unsigned).0,
        422
    );
    // Reusing verified installed code needs Space Admin, not host-operator rights.
    let (status, review) = app_intake(&rig.public, &space, Some('b'), APP_MEDIA_TYPE, &archive);
    assert_eq!(status, 200, "{review}");
    let review: AppReview = serde_json::from_value(review).unwrap();
    assert_eq!(review.app, "counter");
    assert_eq!(review.digest, digest);
    assert_eq!(review.release, release);
    assert_eq!(review.installed.as_deref(), Some(digest.as_str()));
    assert_eq!(review.affected_spaces, 0);
    assert_eq!(
        app_intake(&rig.public, &space, Some('a'), APP_MEDIA_TYPE, &archive).0,
        429
    );
    let mut alice = socket(&rig.public, &space, Some('a'));
    let mut bob = socket(&rig.public, &space, Some('b'));
    send_spaces(
        &mut alice,
        Control::CancelApp {
            version: 1,
            id: 1,
            stage: review.stage.clone(),
        },
    );
    assert!(matches!(
        read_spaces(&mut alice),
        Event::Result {
            id: 1,
            status: Status::Denied,
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
    // An owner may dispose of an uncommitted stage even after losing Admin.
    send_spaces(
        &mut bob,
        Control::CancelApp {
            version: 1,
            id: 2,
            stage: review.stage,
        },
    );
    assert!(matches!(
        read_spaces(&mut bob),
        Event::Result {
            id: 2,
            status: Status::Applied,
            ..
        }
    ));
    assert_eq!(
        app_intake(&rig.public, &space, Some('b'), APP_MEDIA_TYPE, &archive).0,
        403
    );
    assert_eq!(
        app_intake(&rig.public, &space, Some('a'), APP_MEDIA_TYPE, &archive).0,
        200
    );
    assert_eq!(
        fs::read(&checkpoint).unwrap(),
        before,
        "review never changes confirmed content"
    );
    assert_eq!(
        serde_json::from_slice::<Json>(&fs::read(rig.root.join("releases/current.json")).unwrap())
            .unwrap()["id"],
        release
    );
    drop(alice);
    drop(bob);
    rig.stop();
    rig.restart(&binary, true, true);
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), None).0,
        71
    );
    assert_eq!(
        app_intake(&rig.public, &space, Some('a'), APP_MEDIA_TYPE, &archive).0,
        200,
        "volatile stages never activate across recovery"
    );
}

#[test]
fn oib_admin_edits_replicate_and_recover_without_granting_members_ui_writes() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    use otrios_interfaces::oib::{Cell, FORMAT, Object, Seed};
    let mut rig = Rig::start(&binary);
    let space = rig.space("OIB process test");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), Some(431)).0,
        431
    );
    rig.stop();
    let package = rig.root.join("apps/counter/package/ui.flex");
    let mut ui = canvas::decode_ui(&fs::read(&package).unwrap()).unwrap();
    ui.format = "otrios-canvas-v8".into();
    ui.presentation = Some(Presentation::Oib(Seed {
        instance_surface: None,
        format: FORMAT.into(),
        main_oib: 1,
        main_class: "otrios.declarative-v1".into(),
        components: vec![Object {
            id: 10,
            name: "Label".into(),
            component: None,
        }],
        instances: vec![Object {
            id: 20,
            name: "label".into(),
            component: Some(10),
        }],
        cells: vec![Cell {
            id: 100,
            owner: 10,
            name: "label".into(),
            default: Value::Text("Original".into()),
            choices: vec![],
            inherit: None,
            targets: vec![],
        }],
        inspector_viewport: 0,
        inspector_content: [500., 500.],
    }));
    let bytes = canvas::encode_ui(&ui).unwrap();
    fs::write(&package, &bytes).unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps[0].ui_hash = canvas::hash(&bytes);
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    fn open(rig: &Rig, space: &str, ui: &Ui, actor: char) -> (Socket, canvas::StateAdapter, bool) {
        let mut socket = socket(&rig.public, space, Some(actor));
        send(
            &mut socket,
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: ui.ui_ref.clone(),
                state_ref: "app:counter/state".into(),
                catalog_hash: canvas::hash(&rig.catalog),
                known_ui: None,
                known_code: vec![],
                writer: vec![actor as u8; 16],
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
            panic!("OIB mount");
        };
        let replica =
            canvas::StateAdapter::for_ui(ui, "app:counter/state", &state, [actor as u8; 16])
                .unwrap();
        send(&mut socket, ClientMessage::AcceptMount { epoch: 1 });
        assert!(matches!(
            read(&mut socket),
            ServerMessage::Activated { epoch: 1 }
        ));
        (socket, replica, binding.ui_editable)
    }
    fn flush(socket: &mut Socket, replica: &mut canvas::StateAdapter) {
        while let Some(bytes) = replica.generate().unwrap() {
            send(socket, ClientMessage::Sync { epoch: 1, bytes });
        }
    }
    let (mut admin, mut a, allowed) = open(&rig, &space, &ui, 'a');
    assert!(allowed);
    assert_eq!(
        a.value(1).unwrap(),
        431,
        "seeding OIB retains existing app content"
    );
    let (mut member, mut b, allowed) = open(&rig, &space, &ui, 'b');
    assert!(!allowed);
    flush(&mut admin, &mut a);
    flush(&mut member, &mut b);
    a.edit_oib(&[(100, Some(Value::Text("Confirmed live UI".into())))])
        .unwrap();
    let request = a.generate().unwrap().unwrap();
    send(
        &mut member,
        ClientMessage::Sync {
            epoch: 1,
            bytes: request.clone(),
        },
    );
    assert!(matches!(read(&mut member), ServerMessage::Error { .. }));
    // Rejected mutations close that state session; reconnect for read-only observation.
    drop(member);
    let (next, replica, allowed) = open(&rig, &space, &ui, 'b');
    assert!(!allowed);
    member = next;
    b = replica;
    flush(&mut member, &mut b);
    send(
        &mut admin,
        ClientMessage::Sync {
            epoch: 1,
            bytes: request,
        },
    );
    while !a.confirmed() {
        if let ServerMessage::Sync { bytes, .. } = read(&mut admin) {
            a.receive(&bytes).unwrap();
            flush(&mut admin, &mut a);
        }
    }
    while b.oib_generation() != a.oib_generation() {
        if let ServerMessage::Sync { bytes, .. } = read(&mut member) {
            b.receive(&bytes).unwrap();
            flush(&mut member, &mut b);
        }
    }
    assert_eq!(
        a.oib_active().unwrap()[&100],
        Some(Value::Text("Confirmed live UI".into()))
    );
    assert_eq!(a.oib_active().unwrap(), b.oib_active().unwrap());
    drop(admin);
    drop(member);
    rig.stop();
    rig.restart(&binary, true, true);
    let (_, restored, _) = open(&rig, &space, &ui, 'a');
    assert_eq!(restored.oib_active().unwrap(), a.oib_active().unwrap());
}

#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/live-ui/app.rs"]
mod live_oib_app;

#[test]
fn oib_instances_recover_unknown_outcomes_and_retained_removals_across_authority_processes() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut rig = Rig::start(&binary);
    let space = rig.space("OIB instance process test");
    rig.change(
        'a',
        Change::Attach {
            space: space.clone(),
            app: "counter".into(),
        },
    );
    assert_eq!(
        document_session(&rig, &space, "counter", schema(), Some(431)).0,
        431
    );
    rig.stop();
    assert_eq!(live_oib_app::manifest().id, "live-ui");
    let mut ui = live_oib_app::ui(&[]);
    ui.ui_ref = "app:counter/ui".into();
    ui.state_schema = schema();
    for s in &mut ui.selectors {
        if s.field == 2 {
            s.field = 1;
        }
    }
    for h in &mut ui.handlers {
        for f in &mut h.fields {
            if *f == 2 {
                *f = 1;
            }
        }
    }
    let bytes = canvas::encode_ui(&ui).unwrap();
    fs::write(rig.root.join("apps/counter/package/ui.flex"), &bytes).unwrap();
    let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
    catalog.apps[0].ui_hash = canvas::hash(&bytes);
    rig.catalog = canvas::encode_catalog(&catalog).unwrap();
    fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
    rig.restart(&binary, true, true);
    rig.change(
        'a',
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    fn open(
        rig: &Rig,
        space: &str,
        ui: &Ui,
        actor: char,
        resume: u64,
    ) -> (Socket, canvas::StateAdapter, bool) {
        let mut s = socket(&rig.public, space, Some(actor));
        send(
            &mut s,
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: ui.ui_ref.clone(),
                state_ref: "app:counter/state".into(),
                catalog_hash: canvas::hash(&rig.catalog),
                known_ui: None,
                known_code: vec![],
                writer: vec![actor as u8; 16],
                resume,
                collection: None,
            },
        );
        let ServerMessage::Mounted {
            state,
            binding: Some(binding),
            ..
        } = read(&mut s)
        else {
            panic!("instance mount");
        };
        let replica =
            canvas::StateAdapter::for_ui(ui, "app:counter/state", &state, [actor as u8; 16])
                .unwrap();
        send(&mut s, ClientMessage::AcceptMount { epoch: 1 });
        assert!(matches!(
            read(&mut s),
            ServerMessage::Activated { epoch: 1 }
        ));
        (s, replica, binding.ui_editable)
    }
    fn flush(s: &mut Socket, r: &mut canvas::StateAdapter) {
        while let Some(bytes) = r.generate().unwrap() {
            send(s, ClientMessage::Sync { epoch: 1, bytes });
        }
    }
    fn settle(s: &mut Socket, r: &mut canvas::StateAdapter) {
        flush(s, r);
        while !r.confirmed() {
            if let ServerMessage::Sync { bytes, .. } = read(s) {
                r.receive(&bytes).unwrap();
                flush(s, r);
            }
        }
    }
    let (mut admin, mut a, allowed) = open(&rig, &space, &ui, 'a', 0);
    assert!(allowed);
    settle(&mut admin, &mut a);
    let (mut member, mut b, allowed) = open(&rig, &space, &ui, 'b', 0);
    assert!(!allowed);
    settle(&mut member, &mut b);
    a.instantiate_oib(10, Some("gain".into())).unwrap();
    let request = loop {
        let bytes = a.generate().unwrap().unwrap();
        if matches!(
            canvas::decode_state_request(&bytes).unwrap(),
            StateRequest::OibInstance { .. }
        ) {
            break bytes;
        }
        send(&mut admin, ClientMessage::Sync { epoch: 1, bytes });
    };
    send(
        &mut member,
        ClientMessage::Sync {
            epoch: 1,
            bytes: request.clone(),
        },
    );
    assert!(matches!(read(&mut member), ServerMessage::Error { .. }));
    drop(member);
    let (mut member, mut b, _) = open(&rig, &space, &ui, 'b', 0);
    settle(&mut member, &mut b);
    send(
        &mut admin,
        ClientMessage::Sync {
            epoch: 1,
            bytes: request.clone(),
        },
    );
    // Observe durable publication through the other process/session, without
    // consuming the writer's acknowledgement: its creation outcome is unknown.
    while b
        .oib_instances()
        .unwrap()
        .iter()
        .filter(|a| a.active)
        .count()
        != 3
    {
        if let ServerMessage::Sync { bytes, .. } = read(&mut member) {
            b.receive(&bytes).unwrap();
            flush(&mut member, &mut b);
        }
    }
    let created = b.oib_instances().unwrap().last().unwrap().id;
    drop(admin);
    drop(member);
    rig.stop();
    rig.restart(&binary, true, true);
    let (mut admin, mut restored, _) = open(&rig, &space, &ui, 'a', a.resume_sequence());
    restored.restore_pending(&a).unwrap();
    settle(&mut admin, &mut restored);
    assert_eq!(
        restored.oib_instances().unwrap().len(),
        3,
        "unknown outcome cannot allocate twice after recovery"
    );
    send(
        &mut admin,
        ClientMessage::Sync {
            epoch: 1,
            bytes: request,
        },
    );
    restored.set(&[(1, 632)]).unwrap();
    settle(&mut admin, &mut restored);
    assert_eq!(restored.oib_instances().unwrap().len(), 3);
    restored.remove_oib(created).unwrap();
    settle(&mut admin, &mut restored);
    restored
        .edit_oib(&[
            (104, Some(Value::Number(10.))),
            (105, Some(Value::Number(128.))),
            (113, Some(Value::Boolean(false))),
        ])
        .unwrap();
    settle(&mut admin, &mut restored);
    drop(admin);
    rig.stop();
    rig.restart(&binary, true, true);
    let (mut admin, mut recovered, _) = open(&rig, &space, &ui, 'a', 0);
    settle(&mut admin, &mut recovered);
    assert_eq!(recovered.value(1).unwrap(), 632);
    assert_eq!(recovered.oib_property_text(104), "label y: 10 (default)");
    assert_eq!(
        recovered.oib_property_text(105),
        "label width: 128 (default)"
    );
    assert_eq!(
        recovered.oib_property_text(113),
        "heading visible: false (default)"
    );
    assert!(
        !recovered
            .oib_instances()
            .unwrap()
            .iter()
            .find(|a| a.id == created)
            .unwrap()
            .active
    );
    recovered.instantiate_oib(10, None).unwrap();
    settle(&mut admin, &mut recovered);
    let all = recovered.oib_instances().unwrap();
    assert_eq!(all.len(), 4);
    assert_ne!(all.last().unwrap().id, created);
    recovered.restore_oib(false).unwrap();
    settle(&mut admin, &mut recovered);
    assert_eq!(
        recovered
            .oib_instances()
            .unwrap()
            .iter()
            .filter(|a| a.active)
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![20, 30]
    );
    assert_eq!(recovered.oib_property_text(104), "label y: 10 (default)");
    let resume = recovered.resume_sequence();
    recovered.restore_oib(true).unwrap();
    flush(&mut admin, &mut recovered);
    let (mut observer, mut view, _) = open(&rig, &space, &ui, 'b', 0);
    settle(&mut observer, &mut view);
    while view.oib_property_text(104) != "label y: 0 (default)" {
        if let ServerMessage::Sync { bytes, .. } = read(&mut observer) {
            view.receive(&bytes).unwrap();
            flush(&mut observer, &mut view);
        }
    }
    drop(observer);
    drop(admin);
    rig.stop();
    rig.restart(&binary, true, true);
    let (mut admin, mut final_state, _) = open(&rig, &space, &ui, 'a', resume);
    final_state.restore_pending(&recovered).unwrap();
    settle(&mut admin, &mut final_state);
    assert_eq!(final_state.value(1).unwrap(), 632);
    assert_eq!(final_state.oib_property_text(104), "label y: 0 (default)");
    assert_eq!(
        final_state.oib_property_text(113),
        "heading visible: true (default)"
    );
    assert_eq!(final_state.oib_instances().unwrap().len(), all.len());
}
