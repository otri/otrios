// SPDX-License-Identifier: Apache-2.0
// Unified code, bootstrap and app authority. Trusted/public listeners stay loopback.
use axum::serve::ListenerExt;
use axum::{
    Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, Path as RoutePath, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};

use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock, Semaphore, watch};

#[path = "server_apps.rs"]
mod apps;
#[path = "server_auth.rs"]
mod auth;
#[path = "server_ink.rs"]
mod ink;
#[path = "server_session.rs"]
mod session;
#[path = "server_store.rs"]
mod store;

struct App {
    ink: ink::Room,
    auth: Option<Arc<auth::Sessions>>,
    apps: Mutex<Option<apps::Hub>>,
    state_available: watch::Sender<bool>,
    release_changed: watch::Sender<Value>,
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
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(&tmp)?;
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
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
    use std::io::Read;
    let root = root.as_ref()?;
    let mut bytes = Vec::new();
    fs::File::open(root.join("current.json"))
        .ok()?
        .take(8193)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let descriptor: otrios_interfaces::release::Release =
        serde_json::from_value(value.clone()).ok()?;
    if !descriptor.valid() {
        return None;
    }
    let mut raw = Vec::new();
    fs::File::open(root.join(&descriptor.id).join("manifest.json"))
        .ok()?
        .take(65537)
        .read_to_end(&mut raw)
        .ok()?;
    if raw.len() > 65536
        || otrios_core::canvas::hex(&otrios_core::canvas::hash(&raw)) != descriptor.id
    {
        return None;
    }
    let manifest: Value = serde_json::from_slice(&raw).ok()?;
    (manifest["sync_protocol"] == descriptor.sync_protocol).then_some(value)
}
async fn refresh_release(app: &App) -> bool {
    // Serialize read/capture with other hello/activation calls. No state lock.
    let mut current = app.release.write().await;
    let Some(next) = read_release(&app.releases) else {
        return false;
    };
    if *current != next {
        *current = next.clone();
        app.release_changed.send_replace(next);
        announce(app).await;
    }
    true
}
async fn activate(State(app): State<Arc<App>>) -> StatusCode {
    if !refresh_release(&app).await {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    // Catalog admission is deliberately outside release-control handling.
    let copy = app.clone();
    tokio::spawn(async move {
        let release = copy.release.read().await.clone();
        if let Some(hub) = copy.apps.lock().await.as_mut()
            && let Err(e) = hub.refresh(&copy.releases, &release)
        {
            eprintln!("Catalog activation: {e}");
        }
    });
    StatusCode::NO_CONTENT
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
async fn retired() -> Response {
    failure(StatusCode::GONE, "Use the app-scoped /apps/stream protocol")
}
async fn get_presentation(State(app): State<Arc<App>>) -> Response {
    let guard = app.apps.lock().await;
    match guard.as_ref() {
        Some(hub) => binary("application/octet-stream", hub.view().to_vec(), false),
        None => failure(StatusCode::SERVICE_UNAVAILABLE, "State unavailable"),
    }
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
    let mut guard = app.apps.lock().await;
    let Some(hub) = guard.as_mut() else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "State unavailable");
    };
    // Validate before persisting, then publish this replaceable aggregate atomically.
    if !bytes.is_empty() {
        let checked = otrios_core::dashboard::layout::Layout::from_bytes(&bytes).and_then(|l| {
            l.resolve_slots(&std::collections::BTreeMap::new())
                .map(|_| ())
        });
        if let Err(e) = checked {
            return failure(
                StatusCode::BAD_REQUEST,
                &format!("Invalid presentation: {e}"),
            );
        }
    }
    if let Some(path) = &app.presentation_file
        && let Err(e) = persist(path, &bytes)
    {
        eprintln!("Presentation persistence: {e}");
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "Persistence failed");
    }
    let changed = hub.set_view(bytes.to_vec()).expect("validated view");
    drop(guard);
    if changed {
        announce(&app).await;
    }
    binary(
        "text/plain; charset=utf-8",
        otrios_core::canvas::hex(&otrios_core::canvas::hash(&bytes)).into_bytes(),
        false,
    )
}
async fn public_health(State(app): State<Arc<App>>) -> Response {
    let available = *app.state_available.borrow();
    (if available{StatusCode::OK}else{StatusCode::SERVICE_UNAVAILABLE},binary("application/json",json!({"status":if available{"ok"}else{"state-unavailable"},"state_available":available,"release":app.release.read().await["id"]}).to_string().into_bytes(),false)).into_response()
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
    let guard = app.apps.lock().await;
    let Some(hub) = guard.as_ref() else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "State unavailable");
    };
    let (catalogs, roots) = hub.counts();
    let presentation = !hub.view().is_empty();
    drop(guard);
    binary("application/json",json!({"status":"ok","catalogs":catalogs,"app_roots":roots,"presentation":presentation,"release":app.release.read().await["id"]}).to_string().into_bytes(),false)
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
        | "session.mjs"
        | "web/session.mjs"
        | "gpu-scene.mjs"
        | "web/gpu-scene.mjs"
        | "slug-shaders.mjs"
        | "web/slug-shaders.mjs"
        | "font-resource.mjs"
        | "web/font-resource.mjs"
        | "font-reference.mjs"
        | "web/font-reference.mjs"
        | "pkg/otrios_dashboard.js"
        | "web/pkg/otrios_dashboard.js"
        | "pkg/host.js"
        | "web/pkg/host.js"
        | "app-catalog.mjs"
        | "web/app-catalog.mjs" => Some("text/javascript"),
        "style.css" | "web/style.css" => Some("text/css"),
        "Slug-LICENSE.txt" | "web/Slug-LICENSE.txt" => Some("text/plain; charset=utf-8"),
        "fonts/Roboto-Regular.otq.zst" | "web/fonts/Roboto-Regular.otq.zst" => {
            Some("application/octet-stream")
        }
        "pkg/otrios_dashboard_bg.wasm"
        | "web/pkg/otrios_dashboard_bg.wasm"
        | "pkg/host_bg.wasm"
        | "web/pkg/host_bg.wasm" => Some("application/wasm"),
        "apps/catalog.flex" => Some("application/octet-stream"),
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
    if path == "apps/catalog.flex" {
        return match app
            .apps
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| "State unavailable".to_string())
            .and_then(|hub| hub.catalog_bytes())
        {
            Ok(bytes) => binary("application/octet-stream", bytes, false),
            Err(_) => failure(StatusCode::NOT_FOUND, "Not found"),
        };
    }
    match asset_type(&path) {
        Some(kind) if !path.starts_with("native/") => {
            let mut file = app.web.join(&path);
            if !file.exists()
                && matches!(
                    path.as_str(),
                    "artifact-cache-worker.js"
                        | "artifact-cache.mjs"
                        | "release-gate.mjs"
                        | "session.mjs"
                        | "monitoring-session.mjs"
                        | "app-catalog.mjs"
                        | "Slug-LICENSE.txt"
                        | "gpu-scene.mjs"
                        | "slug-shaders.mjs"
                        | "font-resource.mjs"
                        | "font-reference.mjs"
                )
            {
                file = app.web.join("../shared").join(&path);
            }
            if !file.exists() && path == "fonts/Roboto-Regular.otq.zst" {
                file = app.web.join("../dashboard").join(&path);
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
    let prototype = env!("CARGO_BIN_NAME") == "canvas-server";
    let mut bind = if prototype {
        "127.0.0.1:8894"
    } else {
        "127.0.0.1:8876"
    }
    .to_string();
    let mut public_bind = None;
    let mut auth_file = None;
    let mut session_seconds = 8 * 60 * 60;
    let mut state_file: Option<PathBuf> = None;
    let mut app_state = None;
    let mut releases = None;
    let mut origins = vec![];
    let mut web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/canvas");
    let mut source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
        "--bind"=>bind=args.next().ok_or("Missing bind")?,
        "--state"=>state_file=Some(PathBuf::from(args.next().ok_or("Missing state file")?)),
        "--app-state"=>app_state=Some(PathBuf::from(args.next().ok_or("Missing app state directory")?)),
        "--apps"=>source=PathBuf::from(args.next().ok_or("Missing app source directory")?),
        "--auth-file"=>auth_file=Some(PathBuf::from(args.next().ok_or("Missing auth file")?)),
        "--session-seconds"=>session_seconds=args.next().ok_or("Missing session duration")?.parse::<u64>()?,
        "--public-bind"=>public_bind=Some(args.next().ok_or("Missing public bind")?),
        "--releases"=>releases=Some(PathBuf::from(args.next().ok_or("Missing release directory")?)),
        "--web-dir"=>web=PathBuf::from(args.next().ok_or("Missing web directory")?),
        "--origin"=>origins.push(args.next().ok_or("Missing origin")?),
        _=>return Err("Usage: dashboard-server [--bind ADDRESS] [--public-bind LOOPBACK_ADDRESS] [--auth-file PRIVATE_JSON] [--session-seconds SECONDS] [--state LEGACY_FILENAME_PREFIX] [--app-state DIRECTORY] [--apps DIRECTORY] [--releases DIRECTORY] [--web-dir DIRECTORY] [--origin HTTPS_ORIGIN]".into())
    }
    }
    if !bind.parse::<std::net::SocketAddr>()?.ip().is_loopback() {
        return Err("Development authority must bind loopback".into());
    }
    let app_state = app_state.or_else(|| state_file.as_ref().map(|p| p.with_extension("apps")));
    let presentation_file = state_file
        .as_ref()
        .map(|p| p.with_extension("presentation.flex"));
    let release = read_release(&releases).unwrap_or_else(|| json!({"id":"development"}));
    if !(1..=86400).contains(&session_seconds) {
        return Err("Session duration must be between 1 and 86400 seconds".into());
    }
    let auth = auth_file.and_then(|p| {
        match auth::Sessions::load(&p, Duration::from_secs(session_seconds)) {
            Ok(a) => Some(Arc::new(a)),
            Err(e) => {
                eprintln!("Monitoring auth unavailable (closed): {e}");
                None
            }
        }
    });
    let (changed, _) = watch::channel(0);
    let (state_available, _) = watch::channel(false);
    let (release_changed, _) = watch::channel(release.clone());
    let app = Arc::new(App {
        ink: ink::Room::new()?,
        auth,
        apps: Mutex::new(None),
        state_available,
        release_changed,
        presentation_file,
        web,
        releases,
        release: RwLock::new(release),
        changed,
        origins,
        peers: Arc::new(Semaphore::new(64)),
        public_peers: Arc::new(Semaphore::new(32)),
    });
    let recovering = app.clone();
    tokio::spawn(async move {
        let release = recovering.release.read().await.clone();
        let releases = recovering.releases.clone();
        let presentation = recovering.presentation_file.clone();
        let recovered = tokio::task::spawn_blocking(move || -> Result<apps::Hub, String> {
            let mut hub = apps::Hub::new(source, app_state)?;
            hub.refresh(&releases, &release)?;
            if let Some(path) = presentation.filter(|p| p.exists()) {
                hub.set_view(fs::read(path).map_err(|e| e.to_string())?)?;
            }
            Ok(hub)
        })
        .await;
        match recovered {
            Ok(Ok(hub)) => {
                *recovering.apps.lock().await = Some(hub);
                recovering.state_available.send_replace(true);
                announce(&recovering).await;
            }
            other => eprintln!(
                "State recovery unavailable: {}",
                match other {
                    Ok(Err(e)) => e,
                    Err(e) => e.to_string(),
                    _ => unreachable!(),
                }
            ),
        }
    });
    let observer = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        loop {
            tick.tick().await;
            observer.ink.expire().await;
        }
    });
    if let Some(public_bind) = public_bind {
        let address: std::net::SocketAddr = public_bind.parse()?;
        if !address.ip().is_loopback() {
            return Err("Public adapter must bind loopback behind the module host".into());
        }
        let listener = tokio::net::TcpListener::bind(address)
            .await?
            .tap_io(interactive_socket);
        let routes = Router::new()
            .route("/session", get(session::public))
            .route("/release/events", get(session::public_control))
            .route("/", get(root))
            .route("/release", get(current_release))
            .route("/releases/{id}/{*path}", get(release_asset))
            .route("/demo/ink", get(retired))
            .route("/apps/stream", get(retired))
            .route("/stream", get(retired))
            .route("/demo/stream", get(retired))
            .route("/monitoring/stream", get(retired))
            .route(
                "/auth/session",
                get(auth::session)
                    .post(auth::login)
                    .delete(auth::logout)
                    .layer(DefaultBodyLimit::max(2048)),
            )
            .route("/monitoring/presentation", get(auth::presentation))
            .route("/health", get(public_health))
            .fallback(|| async { failure(StatusCode::NOT_FOUND, "Not found") })
            .with_state(app.clone());
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, routes).await {
                eprintln!("Public adapter stopped: {e}");
                std::process::exit(1);
            }
        });
    }
    let routes = Router::new()
        .route("/session", get(session::trusted))
        .route("/release/events", get(session::trusted_control))
        .route("/admin/activate", axum::routing::post(activate))
        .route("/demo/ink", get(retired))
        .route("/", get(root))
        .route("/health", get(health))
        .route("/release", get(current_release))
        .route("/apps/stream", get(retired))
        .route("/sync", get(retired).post(retired))
        .route("/layout", get(retired).post(retired))
        .route("/stream", get(retired))
        .route("/demo/stream", get(retired))
        .route("/monitoring/stream", get(retired))
        .route(
            "/presentation",
            get(get_presentation)
                .post(put_presentation)
                .layer(DefaultBodyLimit::max(
                    otrios_core::dashboard::layout::MAX_LAYOUT_BYTES,
                )),
        )
        .route(
            "/auth/session",
            get(auth::session)
                .post(auth::login)
                .delete(auth::logout)
                .layer(DefaultBodyLimit::max(2048)),
        )
        .route("/monitoring/presentation", get(auth::presentation))
        .route("/releases/{id}/{*path}", get(release_asset))
        .route("/{*path}", get(source_asset))
        .layer(DefaultBodyLimit::max(otrios_core::canvas::MAX_BYTES))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await?
        .tap_io(interactive_socket);
    println!("OTriOS app authority: http://{bind}/");
    axum::serve(listener, routes).await?;
    Ok(())
}

/// Runtime batches and durability define when a packet is ready. Avoid adding
/// TCP small-packet buffering to interactive mount, write and acknowledgment traffic.
fn interactive_socket(socket: &mut tokio::net::TcpStream) {
    if let Err(error) = socket.set_nodelay(true) {
        eprintln!("Could not disable interactive socket buffering: {error}");
    }
}
