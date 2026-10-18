// SPDX-License-Identifier: Apache-2.0
//! Durable development room. Bind loopback and put Tailscale Serve in front of it.
use axum::{
    Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, Path as RoutePath, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use otrios_core::dashboard::{MAX_DOCUMENT_BYTES, Replica, SyncSession};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock, Semaphore, watch};

#[path = "server_auth.rs"]
mod auth;
#[path = "server_ink.rs"]
mod ink;

struct App {
    ink: ink::Room,
    auth: Option<Arc<auth::Sessions>>,
    replica: Mutex<Replica>,
    state_file: Option<PathBuf>,
    presentation_file: Option<PathBuf>,
    web: PathBuf,
    releases: Option<PathBuf>,
    release: RwLock<Value>,
    changed: watch::Sender<u64>,
    origins: Vec<String>,
    peers: Arc<Semaphore>,
    public_peers: Arc<Semaphore>,
}
fn persist(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = path.with_extension(format!("{}.{nonce}.tmp", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        if let Some(parent) = path.parent()
            && let Ok(dir) = fs::File::open(parent)
        {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
fn valid_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
fn read_release(root: &Option<PathBuf>) -> Option<Value> {
    let bytes = fs::read(root.as_ref()?.join("current.json")).ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    if !valid_id(value.get("id")?.as_str()?) {
        return None;
    }
    Some(value)
}
fn origin_ok(app: &App, headers: &HeaderMap) -> bool {
    headers.get(header::ORIGIN).is_none_or(|origin| {
        let origin = origin.to_str().unwrap_or("");
        app.origins.iter().any(|v| v == origin)
            || headers
                .get(header::HOST)
                .is_some_and(|h| origin == format!("http://{}", h.to_str().unwrap_or("")))
    })
}
fn failure(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}
fn binary(kind: &str, bytes: Vec<u8>, immutable: bool) -> Response {
    (
        [
            (header::CONTENT_TYPE, kind),
            (
                header::CACHE_CONTROL,
                if immutable {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-store"
                },
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::HeaderName::from_static("service-worker-allowed"),
                "/",
            ),
        ],
        bytes,
    )
        .into_response()
}
async fn announce(app: &App) {
    app.changed.send_modify(|v| *v = v.wrapping_add(1));
}
async fn commit(app: &App, candidate: &mut Replica) -> Result<(), String> {
    if let Some(path) = &app.state_file {
        persist(path, &candidate.save()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
async fn sync_http(State(app): State<Arc<App>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if !origin_ok(&app, &headers) {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let mut replica = app.replica.lock().await;
    let mut candidate = replica.clone();
    let changed = match candidate.merge(&bytes) {
        Ok(v) => v,
        Err(_) => return failure(StatusCode::BAD_REQUEST, "Invalid dashboard document"),
    };
    if let Err(e) = commit(&app, &mut candidate).await {
        eprintln!("Persistence: {e}");
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "Persistence failed");
    }
    let bytes = candidate.save();
    *replica = candidate;
    drop(replica);
    if changed {
        announce(&app).await;
    }
    binary("application/octet-stream", bytes, false)
}
async fn get_layout(State(app): State<Arc<App>>) -> Response {
    let replica = app.replica.lock().await;
    let layout = replica.base_layout().expect("validated base layout");
    (
        [
            (header::ETAG, format!("\"{}\"", layout.hex_id())),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=dashboard.flex".to_string(),
            ),
        ],
        binary("application/octet-stream", layout.bytes().to_vec(), false),
    )
        .into_response()
}
async fn put_layout(State(app): State<Arc<App>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if !origin_ok(&app, &headers) {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        != Some("application/octet-stream")
    {
        return failure(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected binary FlexBuffers",
        );
    }
    let Some(expected) = headers.get(header::IF_MATCH).and_then(|h| h.to_str().ok()) else {
        return failure(
            StatusCode::PRECONDITION_REQUIRED,
            "If-Match layout revision is required",
        );
    };
    let mut replica = app.replica.lock().await;
    if expected.trim_matches('"')
        != replica
            .base_layout()
            .expect("validated base layout")
            .hex_id()
    {
        return failure(
            StatusCode::PRECONDITION_FAILED,
            "Layout changed; fetch and reapply edits",
        );
    }
    let mut candidate = replica.clone();
    let changed = match candidate.set_layout(&bytes) {
        Ok(value) => value,
        Err(e) => return failure(StatusCode::BAD_REQUEST, &format!("Invalid layout: {e}")),
    };
    if let Err(e) = commit(&app, &mut candidate).await {
        eprintln!("Persistence: {e}");
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "Persistence failed");
    }
    let id = candidate
        .base_layout()
        .expect("validated base layout")
        .hex_id();
    *replica = candidate;
    drop(replica);
    if changed {
        announce(&app).await;
    }
    binary("text/plain; charset=utf-8", id.into_bytes(), false)
}
// Trusted tailnet editors use the same origin policy as /layout. No query URLs or secrets accepted.
async fn get_presentation(State(app): State<Arc<App>>) -> Response {
    let replica = app.replica.lock().await;
    binary(
        "application/octet-stream",
        replica
            .presentation()
            .map(|p| p.bytes().to_vec())
            .unwrap_or_default(),
        false,
    )
}
async fn put_presentation(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    if !origin_ok(&app, &headers) {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        != Some("application/octet-stream")
    {
        return failure(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected binary FlexBuffers",
        );
    }
    let mut replica = app.replica.lock().await;
    let mut candidate = replica.clone();
    let changed =
        match candidate.set_presentation(if bytes.is_empty() { None } else { Some(&bytes) }) {
            Ok(value) => value,
            Err(e) => {
                return failure(
                    StatusCode::BAD_REQUEST,
                    &format!("Invalid presentation: {e}"),
                );
            }
        };
    if let Some(path) = &app.presentation_file
        && let Err(e) = persist(path, &bytes)
    {
        eprintln!("Presentation persistence: {e}");
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "Persistence failed");
    }
    let id = candidate.snapshot().layout().hex_id();
    *replica = candidate;
    drop(replica);
    if changed {
        announce(&app).await;
    }
    binary("text/plain; charset=utf-8", id.into_bytes(), false)
}
async fn stream(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_ok(&app, &headers) {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let permit = match app.peers.clone().try_acquire_owned() {
        Ok(v) => v,
        Err(_) => {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many connected screens",
            );
        }
    };
    upgrade
        .max_message_size(MAX_DOCUMENT_BYTES)
        .max_frame_size(MAX_DOCUMENT_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            peer(app, socket, false).await
        })
        .into_response()
}
async fn peer(app: Arc<App>, socket: WebSocket, public: bool) {
    let (mut output, mut input) = socket.split();
    let mut session = SyncSession::default();
    let mut updates = app.changed.subscribe();
    let mut release_id = String::new();
    let mut presentation_id: Option<String> = None;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    let mut rate_window = std::time::Instant::now();
    let mut received = 0u32;
    loop {
        let release = app.release.read().await.clone();
        let id = release["id"].as_str().unwrap_or("development");
        if id != release_id {
            if output
                .send(Message::Text(
                    json!({"type":"release","release":release})
                        .to_string()
                        .into(),
                ))
                .await
                .is_err()
            {
                break;
            }
            release_id = id.to_string();
        }
        let presentation = if public {
            None
        } else {
            let replica = app.replica.lock().await;
            let id = replica
                .presentation()
                .map(|p| p.hex_id())
                .unwrap_or_default();
            if presentation_id.as_ref() != Some(&id) {
                let mut bytes = b"OTML1".to_vec();
                if let Some(p) = replica.presentation() {
                    bytes.extend_from_slice(p.bytes());
                }
                presentation_id = Some(id);
                Some(bytes)
            } else {
                None
            }
        };
        if let Some(bytes) = presentation
            && output.send(Message::Binary(bytes.into())).await.is_err()
        {
            break;
        }
        let outgoing = app.replica.lock().await.generate_sync(&mut session);
        match outgoing {
            Ok(Some(bytes)) => {
                if output.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("Sync generate: {e}");
                break;
            }
        }
        tokio::select! {
            incoming=input.next()=>match incoming{
                Some(Ok(Message::Binary(bytes)))=>{
                    if public {
                        if rate_window.elapsed() >= Duration::from_secs(1) { rate_window = std::time::Instant::now(); received = 0; }
                        received += 1;
                        if received > 64 { break; }
                    }
                    let mut replica=app.replica.lock().await;let mut candidate=replica.clone();let mut next=session.clone();
                    let result=if public { candidate.receive_demo_sync(&mut next,&bytes) } else { candidate.receive_sync(&mut next,&bytes) };
                    let changed=match result{Ok(v)=>v,Err(e)=>{eprintln!("Rejected sync: {e}");break;}};
                    if changed && let Err(e)=commit(&app,&mut candidate).await{eprintln!("Persistence: {e}");break;}
                    *replica=candidate;session=next;drop(replica);if changed{announce(&app).await;}
                },
                Some(Ok(Message::Ping(data)))=>{if output.send(Message::Pong(data)).await.is_err(){break;}},
                Some(Ok(Message::Pong(_)))=>{},Some(Ok(Message::Close(_)))|None=>break,
                Some(Ok(Message::Text(_)))=>break,Some(Err(_))=>break,
            },
            result=updates.changed()=>{if result.is_err(){break;}},
            _=heartbeat.tick()=>{if output.send(Message::Ping(Bytes::new())).await.is_err(){break;}},
        }
    }
    let _ = output.close().await;
}
// This listener exposes one shared demo document, never the private feed or
// administrative routes. Native/trusted clients retain the original listener.
async fn public_stream(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .is_none_or(|origin| !app.origins.iter().any(|allowed| allowed == origin))
    {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let permit = match app.public_peers.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Too many screens"),
    };
    upgrade
        .max_message_size(MAX_DOCUMENT_BYTES)
        .max_frame_size(MAX_DOCUMENT_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            peer(app, socket, true).await;
        })
        .into_response()
}
async fn public_health(State(app): State<Arc<App>>) -> Response {
    binary(
        "application/json",
        json!({"status":"ok","release":app.release.read().await["id"]})
            .to_string()
            .into_bytes(),
        false,
    )
}
async fn root(State(app): State<Arc<App>>) -> Response {
    let release = app.release.read().await;
    if let Some(entry) = release["web_entry"].as_str() {
        return (
            [(header::CACHE_CONTROL, "no-store")],
            Redirect::temporary(entry),
        )
            .into_response();
    }
    drop(release);
    serve_file(
        app.web.join("index.html"),
        "text/html; charset=utf-8",
        false,
    )
    .await
}
async fn current_release(State(app): State<Arc<App>>) -> Response {
    binary(
        "application/json",
        app.release.read().await.to_string().into_bytes(),
        false,
    )
}
async fn health(State(app): State<Arc<App>>) -> Response {
    let (id, layout, presentation) = {
        let r = app.replica.lock().await;
        (
            r.snapshot().hex_id(),
            r.snapshot().layout().hex_id(),
            r.presentation().is_some(),
        )
    };
    binary(
        "application/json",
        json!({"status":"ok","snapshot":id,"layout":layout,"presentation":presentation,"release":app.release.read().await["id"]})
            .to_string()
            .into_bytes(),
        false,
    )
}
fn asset_type(path: &str) -> Option<&'static str> {
    match path {
        "index.html" | "web/index.html" => Some("text/html; charset=utf-8"),
        "app.js"
        | "web/app.js"
        | "artifact-cache-worker.js"
        | "web/artifact-cache-worker.js"
        | "artifact-cache.mjs"
        | "web/artifact-cache.mjs"
        | "state-worker.js"
        | "web/state-worker.js"
        | "monitoring-session.mjs"
        | "web/monitoring-session.mjs"
        | "release-gate.mjs"
        | "web/release-gate.mjs"
        | "gpu-scene.mjs"
        | "web/gpu-scene.mjs"
        | "slug-shaders.mjs"
        | "web/slug-shaders.mjs"
        | "font-resource.mjs"
        | "web/font-resource.mjs"
        | "font-reference.mjs"
        | "web/font-reference.mjs"
        | "pkg/otrios_dashboard.js"
        | "web/pkg/otrios_dashboard.js" => Some("text/javascript"),
        "style.css" | "web/style.css" => Some("text/css"),
        "fonts/Roboto-Regular.otq.zst" | "web/fonts/Roboto-Regular.otq.zst" => {
            Some("application/octet-stream")
        }
        "pkg/otrios_dashboard_bg.wasm" | "web/pkg/otrios_dashboard_bg.wasm" => {
            Some("application/wasm")
        }
        "manifest.json" => Some("application/json"),
        "manifest.sig" | "native/armv7/dashboard-native" | "native/armv7/dashboard-sdl" => {
            Some("application/octet-stream")
        }
        _ => None,
    }
}
async fn serve_file(path: PathBuf, kind: &str, immutable: bool) -> Response {
    match tokio::fs::read(&path).await {
        Ok(data) if data.len() <= 32 * 1024 * 1024 => binary(kind, data, immutable),
        _ => failure(StatusCode::NOT_FOUND, "Not found"),
    }
}
async fn source_asset(State(app): State<Arc<App>>, RoutePath(path): RoutePath<String>) -> Response {
    match asset_type(&path) {
        Some(kind) if !path.starts_with("native/") => {
            let mut file = app.web.join(&path);
            if !file.exists()
                && matches!(
                    path.as_str(),
                    "gpu-scene.mjs"
                        | "slug-shaders.mjs"
                        | "font-resource.mjs"
                        | "font-reference.mjs"
                )
            {
                file = app.web.join("../shared").join(&path);
            }
            serve_file(file, kind, false).await
        }
        _ => failure(StatusCode::NOT_FOUND, "Not found"),
    }
}
async fn release_asset(
    State(app): State<Arc<App>>,
    RoutePath((id, path)): RoutePath<(String, String)>,
) -> Response {
    if !valid_id(&id) {
        return failure(StatusCode::NOT_FOUND, "Not found");
    }
    match (&app.releases, asset_type(&path)) {
        (Some(root), Some(kind)) => serve_file(root.join(id).join(path), kind, true).await,
        _ => failure(StatusCode::NOT_FOUND, "Not found"),
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut bind = "127.0.0.1:8876".to_string();
    let mut public_bind = None;
    let mut auth_file = None;
    let mut session_seconds = 8 * 60 * 60;
    let mut state_file = None;
    let mut releases = None;
    let mut origins = Vec::new();
    let mut web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dashboard");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str(){
        "--bind"=>bind=args.next().ok_or("Missing bind")?,"--state"=>state_file=Some(PathBuf::from(args.next().ok_or("Missing state file")?)),
        "--auth-file"=>auth_file=Some(PathBuf::from(args.next().ok_or("Missing auth file")?)),
        "--session-seconds"=>session_seconds=args.next().ok_or("Missing session duration")?.parse::<u64>()?,
        "--public-bind"=>public_bind=Some(args.next().ok_or("Missing public bind")?),
        "--releases"=>releases=Some(PathBuf::from(args.next().ok_or("Missing release folder")?)),
        "--web-dir"=>web=PathBuf::from(args.next().ok_or("Missing web folder")?),"--origin"=>origins.push(args.next().ok_or("Missing origin")?),
        _=>return Err("Usage: dashboard-server [--bind ADDRESS] [--public-bind LOOPBACK_ADDRESS] [--auth-file PRIVATE_JSON] [--session-seconds SECONDS] [--state FILE] [--releases DIRECTORY] [--web-dir DIRECTORY] [--origin HTTPS_ORIGIN]".into())
    }
    }
    let mut replica = if let Some(path) = &state_file {
        if path.exists() {
            Replica::load(&fs::read(path)?)?
        } else {
            Replica::new()?
        }
    } else {
        Replica::new()?
    };
    if let Some(path) = &state_file {
        persist(path, &replica.save())?;
    }
    let presentation_file = state_file
        .as_ref()
        .map(|p| p.with_extension("presentation.flex"));
    if let Some(path) = &presentation_file
        && path.exists()
    {
        let bytes = fs::read(path)?;
        if !bytes.is_empty() {
            replica.set_presentation(Some(&bytes))?;
        }
    }
    let release = read_release(&releases).unwrap_or_else(|| json!({"id":"development"}));
    let (changed, _) = watch::channel(0);
    if !(1..=86400).contains(&session_seconds) {
        return Err("Session duration must be between 1 and 86400 seconds".into());
    }
    let auth = auth_file
        .map(|path| auth::Sessions::load(&path, Duration::from_secs(session_seconds)).map(Arc::new))
        .transpose()?;
    let app = Arc::new(App {
        ink: ink::Room::new()?,
        auth,
        replica: Mutex::new(replica),
        state_file,
        presentation_file,
        web,
        releases,
        release: RwLock::new(release),
        changed,
        origins,
        peers: Arc::new(Semaphore::new(64)),
        public_peers: Arc::new(Semaphore::new(32)),
    });
    let observer = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        loop {
            tick.tick().await;
            observer.ink.expire().await;
            if let Some(next) = read_release(&observer.releases) {
                let mut current = observer.release.write().await;
                if *current != next {
                    *current = next;
                    drop(current);
                    announce(&observer).await;
                }
            }
        }
    });
    if let Some(public_bind) = public_bind {
        let address: std::net::SocketAddr = public_bind.parse()?;
        if !address.ip().is_loopback() {
            return Err("Public adapter must bind loopback behind the module host".into());
        }
        let listener = tokio::net::TcpListener::bind(address).await?;
        let public_routes = Router::new()
            .route("/demo/ink", get(ink::public))
            .route("/stream", get(public_stream))
            .route("/demo/stream", get(public_stream))
            .route(
                "/auth/session",
                get(auth::session)
                    .post(auth::login)
                    .delete(auth::logout)
                    .layer(DefaultBodyLimit::max(2048)),
            )
            .route("/monitoring/presentation", get(auth::presentation))
            .route("/monitoring/stream", get(auth::stream))
            .route("/health", get(public_health))
            .fallback(|| async { failure(StatusCode::NOT_FOUND, "Not found") })
            .with_state(app.clone());
        tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, public_routes).await {
                eprintln!("Public adapter stopped: {error}");
                std::process::exit(1);
            }
        });
    }
    let routes = Router::new()
        .route("/demo/ink", get(ink::trusted))
        .route("/", get(root))
        .route("/health", get(health))
        .route("/release", get(current_release))
        .route("/sync", post(sync_http))
        .route(
            "/layout",
            get(get_layout)
                .post(put_layout)
                .layer(DefaultBodyLimit::max(
                    otrios_core::dashboard::layout::MAX_LAYOUT_BYTES,
                )),
        )
        .route(
            "/presentation",
            get(get_presentation)
                .post(put_presentation)
                .layer(DefaultBodyLimit::max(
                    otrios_core::dashboard::layout::MAX_LAYOUT_BYTES,
                )),
        )
        .route("/stream", get(stream))
        .route("/demo/stream", get(public_stream))
        .route(
            "/auth/session",
            get(auth::session)
                .post(auth::login)
                .delete(auth::logout)
                .layer(DefaultBodyLimit::max(2048)),
        )
        .route("/monitoring/presentation", get(auth::presentation))
        .route("/monitoring/stream", get(auth::stream))
        .route("/releases/{id}/{*path}", get(release_asset))
        .route("/{*path}", get(source_asset))
        .layer(DefaultBodyLimit::max(MAX_DOCUMENT_BYTES))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("OTriOS durable room: http://{bind}/");
    axum::serve(listener, routes).await?;
    Ok(())
}
