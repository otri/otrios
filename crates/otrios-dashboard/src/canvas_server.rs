// SPDX-License-Identifier: Apache-2.0
//! Isolated loopback prototype. Deliberately separate from the deployed authority.
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use otrios_core::canvas::*;
use otrios_interfaces::canvas::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
struct Package {
    app: App,
    ui: Vec<u8>,
    code: Vec<u8>,
    folder: PathBuf,
    schema: StateSchema,
}
struct Room {
    doc: Document,
    file: PathBuf,
}
struct Hub {
    packages: BTreeMap<String, Package>,
    rooms: BTreeMap<String, Room>,
}
#[derive(Clone)]
struct Server {
    hub: Arc<Mutex<Hub>>,
    changed: broadcast::Sender<String>,
    web: PathBuf,
    origin: String,
}
fn io(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn persist(file: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let tmp = file.with_extension("pending");
    let mut f = std::fs::File::create(&tmp).map_err(io)?;
    f.write_all(bytes).map_err(io)?;
    f.sync_all().map_err(io)?;
    std::fs::rename(tmp, file).map_err(io)?;
    std::fs::File::open(file.parent().ok_or("No parent")?)
        .and_then(|d| d.sync_all())
        .map_err(io)
}
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let option = |name: &str, default: &str| {
        args.windows(2)
            .find(|a| a[0] == name)
            .map(|a| a[1].clone())
            .unwrap_or_else(|| default.into())
    };
    let bind = option("--bind", "127.0.0.1:8894");
    let address: std::net::SocketAddr = bind.parse().map_err(io)?;
    if !address.ip().is_loopback() {
        return Err("Canvas prototype must bind loopback".into());
    }
    let root = PathBuf::from(option("--apps", "apps"));
    let state = PathBuf::from(option("--state", "target/canvas-state"));
    std::fs::create_dir_all(&state).map_err(io)?;
    let mut hub = Hub {
        packages: BTreeMap::new(),
        rooms: BTreeMap::new(),
    };
    for entry in std::fs::read_dir(root).map_err(io)? {
        let folder = entry.map_err(io)?.path().join("package");
        if !folder.join("app.flex").is_file() {
            continue;
        }
        let app = decode_app(&std::fs::read(folder.join("app.flex")).map_err(io)?)?;
        if app.status != "canvas-prototype" {
            continue;
        }
        if app.id.is_empty() || !app.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
            return Err("Invalid app ID".into());
        }
        let bytes = std::fs::read(folder.join("ui.flex")).map_err(io)?;
        let ui = decode_ui(&bytes)?;
        let code = if let Some(r) = &ui.behavior {
            let code = std::fs::read(folder.join("behavior.wasm")).map_err(io)?;
            Behavior::load(&code, r)?;
            code
        } else {
            vec![]
        };
        let file = state.join(format!("{}.automerge", app.id));
        let mut doc = if file.exists() {
            Document::load(
                ui.state_schema.clone(),
                &app.state_ref,
                &std::fs::read(&file).map_err(io)?,
            )?
        } else {
            Document::new(ui.state_schema.clone(), &app.state_ref)?
        };
        persist(&file, &doc.save())?;
        hub.rooms.insert(app.state_ref.clone(), Room { doc, file });
        hub.packages.insert(
            app.ui_ref.clone(),
            Package {
                app,
                ui: bytes,
                code,
                folder,
                schema: ui.state_schema,
            },
        );
    }
    if hub.packages.len() != 3 {
        return Err("Build the three Canvas app packages first".into());
    }
    let (changed, _) = broadcast::channel(64);
    let server = Server {
        hub: Arc::new(Mutex::new(hub)),
        changed,
        web: PathBuf::from(option("--web-dir", "web/canvas")),
        origin: option("--origin", &format!("http://{bind}")),
    };
    let router = Router::new()
        .route("/stream", get(upgrade))
        .route("/", get(index))
        .route("/{*path}", get(asset))
        .with_state(server);
    println!("Canvas prototype http://{bind} (isolated app state)");
    let listener = tokio::net::TcpListener::bind(address).await.map_err(io)?;
    axum::serve(listener, router).await.map_err(io)
}
async fn index(State(s): State<Server>) -> Response {
    file(&s, "index.html").await
}
async fn asset(
    State(s): State<Server>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    file(&s, &path).await
}
async fn file(s: &Server, path: &str) -> Response {
    let mime = match path {
        "index.html" => "text/html; charset=utf-8",
        "app.js" | "state-worker.js" | "pkg/host.js" => "text/javascript; charset=utf-8",
        "style.css" => "text/css; charset=utf-8",
        "pkg/host_bg.wasm" => "application/wasm",
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match tokio::fs::read(s.web.join(path)).await {
        Ok(b) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            b,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn upgrade(State(s): State<Server>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if headers
        .get(header::ORIGIN)
        .is_some_and(|v| v.to_str().ok() != Some(s.origin.as_str()))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.max_message_size(MAX_BYTES)
        .max_frame_size(MAX_BYTES)
        .on_upgrade(move |socket| run(socket, s))
}
async fn send(socket: &mut WebSocket, message: ServerMessage) -> Result<()> {
    socket
        .send(Message::Binary(encode_server(&message)?.into()))
        .await
        .map_err(io)
}
struct Lease {
    epoch: u32,
    state_ref: String,
    sync: Session,
}
fn process(
    s: &Server,
    lease: &mut Option<Lease>,
    message: ClientMessage,
) -> Result<Vec<ServerMessage>> {
    let mut hub = s.hub.lock().map_err(io)?;
    let mut response = Vec::new();
    match message {
        ClientMessage::Mount {
            epoch,
            ui_ref,
            state_ref,
            known_code,
        } => {
            if epoch == 0
                || lease.as_ref().is_some_and(|l| epoch <= l.epoch)
                || known_code.len() > 8
            {
                return Err("Invalid mount generation/cache".into());
            }
            let package = hub
                .packages
                .get_mut(&ui_ref)
                .ok_or("Unknown UI reference")?;
            // Reopen qualifies fresh immutable UI/code, preserving the durable State root.
            let ui_bytes = std::fs::read(package.folder.join("ui.flex")).map_err(io)?;
            let ui = decode_ui(&ui_bytes)?;
            if ui.ui_ref != ui_ref || ui.state_schema != package.schema {
                return Err("App update needs a schema migration".into());
            }
            let code = if let Some(r) = &ui.behavior {
                let b = std::fs::read(package.folder.join("behavior.wasm")).map_err(io)?;
                Behavior::load(&b, r)?;
                b
            } else {
                vec![]
            };
            package.ui = ui_bytes;
            package.code = code;
            if package.app.state_ref != state_ref {
                return Err("UI/State pair not granted by registry".into());
            }
            let code = if known_code.contains(&hash(&package.code)) {
                vec![]
            } else {
                package.code.clone()
            };
            response.push(ServerMessage::Mounted {
                epoch,
                ui: package.ui.clone(),
                ui_hash: hash(&package.ui),
                code,
            });
            *lease = Some(Lease {
                epoch,
                state_ref,
                sync: Session::default(),
            });
        }
        ClientMessage::Sync { epoch, bytes } => {
            let l = lease.as_mut().ok_or("Sync without Canvas")?;
            if l.epoch != epoch {
                return Ok(vec![]);
            }
            let room = hub.rooms.get_mut(&l.state_ref).ok_or("Missing State")?;
            let mut doc = room.doc.clone();
            let mut next = l.sync.clone();
            if doc.receive(&mut next, &bytes)? {
                persist(&room.file, &doc.save())?;
                let _ = s.changed.send(l.state_ref.clone());
            }
            room.doc = doc;
            l.sync = next;
        }
        ClientMessage::Leave { epoch } => {
            if lease.as_ref().is_some_and(|l| l.epoch == epoch) {
                *lease = None;
            }
            return Ok(vec![]);
        }
    }
    if let Some(l) = lease
        && let Some(bytes) = hub
            .rooms
            .get_mut(&l.state_ref)
            .ok_or("Missing State")?
            .doc
            .generate(&mut l.sync)?
    {
        response.push(ServerMessage::Sync {
            epoch: l.epoch,
            bytes,
        });
    }
    Ok(response)
}
async fn run(mut socket: WebSocket, s: Server) {
    let mut lease = None;
    let mut changes = s.changed.subscribe();
    loop {
        tokio::select! {
            msg=socket.recv()=>{let Some(Ok(Message::Binary(bytes)))=msg else{break};let result=decode_client(&bytes).and_then(|m|process(&s,&mut lease,m));match result{
                Ok(messages)=>{for m in messages{if send(&mut socket,m).await.is_err(){return}}},
                Err(message)=>{let epoch=lease.as_ref().map(|l|l.epoch).unwrap_or(1);let _=send(&mut socket,ServerMessage::Error{epoch,message}).await;break}
            }},
            notice=changes.recv()=>{
                if notice.as_ref().is_ok_and(|r|lease.as_ref().is_none_or(|l|&l.state_ref!=r)){continue}
                let message=(||->Result<Option<ServerMessage>>{let Some(l)=&mut lease else{return Ok(None)};let mut hub=s.hub.lock().map_err(io)?;Ok(hub.rooms.get_mut(&l.state_ref).ok_or("Missing State")?.doc.generate(&mut l.sync)?.map(|bytes|ServerMessage::Sync{epoch:l.epoch,bytes}))})();
                if let Ok(Some(m))=message&&send(&mut socket,m).await.is_err(){return}
            }
        }
    }
}
