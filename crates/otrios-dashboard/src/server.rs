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
    response::{IntoResponse, Response},
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
#[path = "server_chat.rs"]
mod chat;
#[path = "server_collection.rs"]
mod collections;
#[path = "server_ink.rs"]
mod ink;
#[path = "server_session.rs"]
mod session;
#[path = "server_store.rs"]
mod store;

#[derive(Clone)]
struct StateConfig {
    source: PathBuf,
    state: Option<PathBuf>,
    presentation: Option<PathBuf>,
}
struct App {
    collections: Arc<collections::Hub>,
    chat: Arc<chat::Hub>,
    ink: ink::Room,
    auth: Option<Arc<auth::Sessions>>,
    apps: Mutex<Option<apps::Hub>>,
    state_config: StateConfig,
    recovery: Mutex<()>,
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
    peer_limits: [usize; 2],
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
    // Recovery/catalog admission is outside release-control handling.
    let copy = app.clone();
    tokio::spawn(async move {
        let release = copy.release.read().await.clone();
        let mut guard = copy.apps.lock().await;
        if let Some(hub) = guard.as_mut() {
            if let Err(e) = hub.refresh(&copy.releases, &release) {
                eprintln!("Catalog activation: {e}");
            }
        } else {
            drop(guard);
            recover_state(copy).await;
        }
    });
    StatusCode::NO_CONTENT
}
async fn recover_state(app: Arc<App>) {
    let Ok(_exclusive) = app.recovery.try_lock() else {
        return;
    };
    if app.apps.lock().await.is_some() {
        return;
    }
    // A publication racing failed recovery gets one catch-up attempt. Work stays bounded.
    for _ in 0..2 {
        let release = app.release.read().await.clone();
        let wanted = release.clone();
        let releases = app.releases.clone();
        let config = app.state_config.clone();
        let recovered = tokio::task::spawn_blocking(move || -> Result<apps::Hub, String> {
            if releases.is_some() && release["id"] == "development" {
                return Err("No valid active release".into());
            }
            let mut hub = apps::Hub::new(config.source, config.state)?;
            hub.refresh(&releases, &release)?;
            if let Some(path) = config.presentation.filter(|p| p.exists()) {
                hub.set_view(fs::read(path).map_err(|e| e.to_string())?)?;
            }
            Ok(hub)
        })
        .await;
        match recovered {
            Ok(Ok(hub)) => {
                *app.apps.lock().await = Some(hub);
                app.state_available.send_replace(true);
                announce(&app).await;
                return;
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
        if *app.release.read().await == wanted {
            return;
        }
    }
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
async fn response_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("same-origin"),
    );
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(header::HeaderValue::from_static("no-store"));
    response
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
/// Serve a mutable navigation entry, pinning every executable/style URL to one release.
/// Reading code metadata here never waits for app-state recovery or admission.
async fn app_entry(app: &App, route: Option<&str>) -> Response {
    refresh_release(app).await;
    let release = app.release.read().await.clone();
    let (index, catalog, prefix) = if let (Some(root), Some(id)) = (
        &app.releases,
        release["id"].as_str().filter(|id| valid_id(id)),
    ) {
        (
            root.join(id).join("web/index.html"),
            root.join(id).join("apps/catalog.flex"),
            format!("/releases/{id}/web/"),
        )
    } else if app.releases.is_some() {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "No valid active release");
    } else {
        (
            app.web.join("index.html"),
            app.state_config.source.join("catalog.flex"),
            "/".into(),
        )
    };
    let title = if let Some(path) = route {
        let screen = path.trim_end_matches('/');
        if !otrios_interfaces::canvas::valid_destination(screen) {
            return failure(StatusCode::NOT_FOUND, "Not found");
        }
        let bytes = match tokio::fs::read(catalog).await {
            Ok(bytes) if bytes.len() <= 512 * 1024 => bytes,
            _ => return failure(StatusCode::SERVICE_UNAVAILABLE, "App catalog unavailable"),
        };
        let catalog = match otrios_core::canvas::decode_catalog(&bytes) {
            Ok(catalog) => catalog,
            Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "App catalog unavailable"),
        };
        let Some(entry) = catalog.apps.iter().find(|e| e.path == screen) else {
            return failure(StatusCode::NOT_FOUND, "Not found");
        };
        entry.app.title.clone()
    } else {
        "OTriOS".into()
    };
    let bytes = match tokio::fs::read(index).await {
        Ok(bytes) if bytes.len() <= 1024 * 1024 => bytes,
        _ => return failure(StatusCode::SERVICE_UNAVAILABLE, "App entry unavailable"),
    };
    let mut html = match String::from_utf8(bytes) {
        Ok(html) => html,
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "App entry unavailable"),
    };
    // Link preview fetchers do not run JavaScript. Use only the public catalog
    // title, independent of private UI/state admission or recovery.
    let escaped = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    let page_title = if route.is_some() {
        format!("OTriOS · {escaped}")
    } else {
        escaped.clone()
    };
    html = html
        .replace(
            "<title>OTriOS</title>",
            &format!("<title>{page_title}</title>"),
        )
        .replace(
            "property=\"og:title\" content=\"OTriOS\"",
            &format!("property=\"og:title\" content=\"{escaped}\""),
        );
    for (attribute, asset) in [("href", "style.css"), ("src", "app.js")] {
        for source in [format!("/{asset}"), format!("./{asset}")] {
            html = html.replace(
                &format!("{attribute}=\"{source}\""),
                &format!("{attribute}=\"{prefix}{asset}\""),
            );
        }
    }
    binary("text/html; charset=utf-8", html.into_bytes(), false)
}
async fn root(State(app): State<Arc<App>>) -> Response {
    app_entry(&app, None).await
}
async fn navigation_entry(
    State(app): State<Arc<App>>,
    request: axum::extract::Request,
) -> Response {
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        return failure(StatusCode::NOT_FOUND, "Not found");
    }
    let mut response = app_entry(&app, Some(request.uri().path().trim_start_matches('/'))).await;
    if request.method() == axum::http::Method::HEAD {
        *response.body_mut() = axum::body::Body::empty();
    }
    response
}
async fn current_release(State(app): State<Arc<App>>) -> Response {
    refresh_release(&app).await;
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
    binary("application/json",json!({"status":"ok","catalogs":catalogs,"app_roots":roots,"presentation":presentation,"public_sessions":app.peer_limits[1]-app.public_peers.available_permits(),"trusted_sessions":app.peer_limits[0]-app.peers.available_permits(),"peer_limits":app.peer_limits,"ink_subscribers":app.ink.subscribers(),"ink_limit":app.peer_limits.iter().sum::<usize>(),"release":app.release.read().await["id"]}).to_string().into_bytes(),false)
}
fn asset_type(path: &str) -> Option<&'static str> {
    if let Some(source) = path.strip_suffix(".gz")
        && source.starts_with("web/")
        && !source.ends_with(".gz")
        && asset_type(source).is_some()
    {
        return Some("application/gzip");
    }
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
        | "document-transfer.mjs"
        | "touch-input.mjs"
        | "web/touch-input.mjs"
        | "web/document-transfer.mjs"
        | "monitoring-session.mjs"
        | "web/monitoring-session.mjs"
        | "release-gate.mjs"
        | "web/release-gate.mjs"
        | "session.mjs"
        | "web/session.mjs"
        | "navigation-timings.mjs"
        | "web/navigation-timings.mjs"
        | "ui-update.mjs"
        | "web/ui-update.mjs"
        | "viewport.mjs"
        | "web/viewport.mjs"
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
        | "app-catalog.mjs"
        | "web/app-catalog.mjs" => Some("text/javascript"),
        "text-input.mjs" | "web/text-input.mjs" => Some("text/javascript"),
        "fonts/Roboto.ttf" | "web/fonts/Roboto.ttf" => Some("font/ttf"),
        "fonts/OFL.txt" | "web/fonts/OFL.txt" => Some("text/plain; charset=utf-8"),
        "style.css" | "web/style.css" => Some("text/css"),
        "Slug-LICENSE.txt" | "web/Slug-LICENSE.txt" => Some("text/plain; charset=utf-8"),
        "fonts/Roboto-Regular.otq.zst" | "web/fonts/Roboto-Regular.otq.zst" => {
            Some("application/octet-stream")
        }
        "pkg/otrios_dashboard_bg.wasm" | "web/pkg/otrios_dashboard_bg.wasm" => {
            Some("application/wasm")
        }
        "apps/catalog.flex" => Some("application/octet-stream"),
        "manifest.json" => Some("application/json"),
        "manifest.sig" | "native/armv7/dashboard-native" => Some("application/octet-stream"),
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
    if asset_type(&path).is_none() {
        return app_entry(&app, Some(&path)).await;
    }
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
                        | "document-transfer.mjs"
                        | "touch-input.mjs"
                        | "text-input.mjs"
                        | "app-catalog.mjs"
                        | "Slug-LICENSE.txt"
                        | "navigation-timings.mjs"
                        | "ui-update.mjs"
                        | "viewport.mjs"
                        | "gpu-scene.mjs"
                        | "slug-shaders.mjs"
                        | "font-resource.mjs"
                        | "font-reference.mjs"
                )
            {
                file = app.web.join("../shared").join(&path);
            }
            if !file.exists() && path == "fonts/Roboto-Regular.otq.zst" {
                file = app.web.join("../shared").join(&path);
            }
            if !file.exists() && matches!(path.as_str(), "fonts/Roboto.ttf" | "fonts/OFL.txt") {
                file = app
                    .web
                    .join("../../assets/fonts/roboto")
                    .join(Path::new(&path).file_name().unwrap_or_default());
            }
            serve_file(file, kind, false).await
        }
        _ => failure(StatusCode::NOT_FOUND, "Not found"),
    }
}
async fn release_asset(
    State(app): State<Arc<App>>,
    RoutePath((id, path)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if !valid_id(&id) {
        return failure(StatusCode::NOT_FOUND, "Not found");
    }
    match (&app.releases, asset_type(&path)) {
        (Some(root), Some(kind)) => {
            let folder = root.join(id);
            if path.starts_with("web/") && !path.ends_with(".gz") {
                if accepts_gzip(&headers) {
                    let encoded = folder.join(format!("{path}.gz"));
                    if let Ok(data) = tokio::fs::read(encoded).await
                        && data.len() <= 32 * 1024 * 1024
                    {
                        let mut response = binary(kind, data, true);
                        let h = response.headers_mut();
                        h.insert(
                            header::CONTENT_ENCODING,
                            header::HeaderValue::from_static("gzip"),
                        );
                        h.insert(
                            header::VARY,
                            header::HeaderValue::from_static("Accept-Encoding"),
                        );
                        // The edge may normalize Accept-Encoding on the origin
                        // request. Allow it to adapt gzip for the actual client.
                        return response;
                    }
                }
                let mut response = serve_file(folder.join(path), kind, true).await;
                response.headers_mut().insert(
                    header::VARY,
                    header::HeaderValue::from_static("Accept-Encoding"),
                );
                response
            } else {
                serve_file(folder.join(path), kind, true).await
            }
        }
        _ => failure(StatusCode::NOT_FOUND, "Not found"),
    }
}
fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .any(|value| {
            value.to_str().unwrap_or("").split(',').any(|coding| {
                let mut fields = coding.trim().split(';');
                if !fields
                    .next()
                    .unwrap_or("")
                    .trim()
                    .eq_ignore_ascii_case("gzip")
                {
                    return false;
                }
                fields.all(|parameter| {
                    let Some((key, value)) = parameter.trim().split_once('=') else {
                        return false;
                    };
                    key.trim().eq_ignore_ascii_case("q")
                        && value
                            .trim()
                            .parse::<f32>()
                            .is_ok_and(|q| q > 0.0 && q <= 1.0)
                })
            })
        })
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
    let mut trusted_peers = 64;
    let mut public_peers = 32;
    let mut state_file: Option<PathBuf> = None;
    let mut app_state = None;
    let mut collection_fixture = None;
    let mut explicit_presentation = None;
    let mut releases = None;
    let mut origins = vec![];
    let mut web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/canvas");
    let mut source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
        "--bind"=>bind=args.next().ok_or("Missing bind")?,
        "--state"=>state_file=Some(PathBuf::from(args.next().ok_or("Missing state file")?)),
        "--presentation-file"=>explicit_presentation=Some(PathBuf::from(args.next().ok_or("Missing presentation file")?)),
        "--app-state"=>app_state=Some(PathBuf::from(args.next().ok_or("Missing app state directory")?)),
        "--collection-fixture"=>collection_fixture=Some(PathBuf::from(args.next().ok_or("Missing collection fixture file")?)),
        "--apps"=>source=PathBuf::from(args.next().ok_or("Missing app source directory")?),
        "--auth-file"=>auth_file=Some(PathBuf::from(args.next().ok_or("Missing auth file")?)),
        "--session-seconds"=>session_seconds=args.next().ok_or("Missing session duration")?.parse::<u64>()?,
        "--trusted-peers"=>trusted_peers=args.next().ok_or("Missing trusted peer limit")?.parse::<usize>()?,
        "--public-peers"=>public_peers=args.next().ok_or("Missing public peer limit")?.parse::<usize>()?,
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
    if !(1..=64).contains(&trusted_peers) || !(1..=4096).contains(&public_peers) {
        return Err("Peer limits must be 1..=64 trusted and 1..=4096 public".into());
    }
    let app_state = app_state.or_else(|| state_file.as_ref().map(|p| p.with_extension("apps")));
    let presentation_file = explicit_presentation.or_else(|| {
        state_file
            .as_ref()
            .map(|p| p.with_extension("presentation.flex"))
    });
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
        collections: collections::Hub::new()?,
        chat: chat::Hub::new(app_state.as_ref().map(|p| p.join("brass-relay.journal")))?,
        ink: ink::Room::new(trusted_peers + public_peers)?,
        auth,
        apps: Mutex::new(None),
        state_config: StateConfig {
            source,
            state: app_state,
            presentation: presentation_file.clone(),
        },
        recovery: Mutex::new(()),
        state_available,
        release_changed,
        presentation_file,
        web,
        releases,
        release: RwLock::new(release),
        changed,
        origins,
        peers: Arc::new(Semaphore::new(trusted_peers)),
        public_peers: Arc::new(Semaphore::new(public_peers)),
        peer_limits: [trusted_peers, public_peers],
    });
    tokio::spawn(recover_state(app.clone()));
    if let Some(path) = collection_fixture {
        app.collections.fixture(path);
    }
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
        let listener = tokio::net::TcpListener::bind(address).await?;
        println!("OTriOS public listener: http://{}/", listener.local_addr()?);
        std::io::stdout().flush()?;
        let listener = listener.tap_io(interactive_socket);
        let routes = Router::new()
            .route("/session", get(session::public))
            .route("/release/events", get(session::public_control))
            .route("/", get(root))
            .route("/release", get(current_release))
            .route("/releases/{id}/{*path}", get(release_asset))
            .route("/demo/ink", get(retired))
            .route("/apps/stream", get(retired))
            .route(
                "/auth/session",
                get(auth::session)
                    .post(auth::login)
                    .delete(auth::logout)
                    .layer(DefaultBodyLimit::max(2048)),
            )
            .route("/chat/identity", axum::routing::post(chat::identity))
            .route("/monitoring/presentation", get(auth::presentation))
            .route("/health", get(public_health))
            .fallback(navigation_entry)
            .layer(axum::middleware::from_fn(response_headers))
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
        .route("/chat/identity", axum::routing::post(chat::identity))
        .route("/monitoring/presentation", get(auth::presentation))
        .route("/releases/{id}/{*path}", get(release_asset))
        .route("/{*path}", get(source_asset))
        .layer(DefaultBodyLimit::max(otrios_core::canvas::MAX_BYTES))
        .layer(axum::middleware::from_fn(response_headers))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("OTriOS app authority: http://{}/", listener.local_addr()?);
    std::io::stdout().flush()?;
    let listener = listener.tap_io(interactive_socket);
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
