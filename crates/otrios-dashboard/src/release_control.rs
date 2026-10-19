// SPDX-License-Identifier: Apache-2.0
//! Independent code-host control process. No app/state protocol is loaded here.
use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use otrios_interfaces::release::{ClientMessage, MAX_BYTES, Release, ServerMessage, VERSION};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, watch};
struct Hub {
    root: PathBuf,
    changed: watch::Sender<Release>,
    peers: Arc<Semaphore>,
    origins: Vec<String>,
}
fn bounded(path: PathBuf, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![];
    File::open(path)
        .map_err(|e| e.to_string())?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Oversized release metadata".into());
    }
    Ok(bytes)
}
impl Hub {
    fn read(&self) -> Result<Release, String> {
        let r: Release =
            serde_json::from_slice(&bounded(self.root.join("current.json"), MAX_BYTES)?)
                .map_err(|e| e.to_string())?;
        if !r.valid() {
            return Err("Invalid release pointer".into());
        }
        let raw = bounded(self.root.join(&r.id).join("manifest.json"), 65536)?;
        let hash = Sha256::digest(&raw)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let manifest: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if hash != r.id || manifest["sync_protocol"].as_str() != Some(&r.sync_protocol) {
            return Err("Pointer/manifest mismatch".into());
        }
        Ok(r)
    }
    fn refresh(&self) -> Result<(), String> {
        let next = self.read()?;
        self.changed.send_if_modified(|old| {
            if *old != next {
                *old = next;
                true
            } else {
                false
            }
        });
        Ok(())
    }
}
async fn publish(State(hub): State<Arc<Hub>>) -> StatusCode {
    match hub.refresh() {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            eprintln!("Release notification rejected: {e}");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
async fn stream(State(hub): State<Arc<Hub>>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if let Some(origin) = headers.get("origin")
        && !origin
            .to_str()
            .is_ok_and(|v| hub.origins.iter().any(|o| o == v))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(permit) = hub.peers.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(MAX_BYTES)
        .max_frame_size(MAX_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            session(hub, socket).await;
        })
}
async fn session(hub: Arc<Hub>, mut ws: WebSocket) {
    let Ok(Some(Ok(Message::Text(bytes)))) =
        tokio::time::timeout(Duration::from_secs(5), ws.recv()).await
    else {
        return;
    };
    let Ok(ClientMessage::Hello {
        version,
        release,
        state_protocol,
    }) = serde_json::from_str(&bytes)
    else {
        return;
    };
    if release
        .as_ref()
        .is_some_and(|id| !otrios_interfaces::release::valid_hash(id))
        || state_protocol.len() > 128
    {
        return;
    }
    // Reconnect also recovers a publication whose explicit notification was lost.
    if let Err(e) = hub.refresh() {
        eprintln!("Release reconnect kept last valid pointer: {e}");
    }
    let mut changed = hub.changed.subscribe();
    let current = changed.borrow_and_update().clone();
    let refresh = version != VERSION || state_protocol != current.sync_protocol;
    let hello = ServerMessage::Hello {
        version: VERSION,
        release: current,
        refresh,
    };
    if send(&mut ws, &hello).await.is_err() {
        return;
    }
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.tick().await;
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            event=changed.changed()=>{
                if event.is_err(){return;}
                let release=changed.borrow_and_update().clone();
                let refresh=version!=VERSION || state_protocol!=release.sync_protocol;
                if send(&mut ws,&ServerMessage::Upgrade{version:VERSION,release,refresh}).await.is_err(){return;}
            }
            _=heartbeat.tick()=>{
                if heard.elapsed()>Duration::from_secs(60) || ws.send(Message::Ping(vec![].into())).await.is_err(){return;}
            }
            event=ws.recv()=>match event{
                Some(Ok(Message::Pong(_)|Message::Ping(_)))=>heard=Instant::now(),
                _=>return,
            }
        }
    }
}
async fn send(ws: &mut WebSocket, event: &ServerMessage) -> Result<(), ()> {
    let text = serde_json::to_string(event).map_err(|_| ())?;
    tokio::time::timeout(Duration::from_secs(5), ws.send(Message::Text(text.into())))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bind = "127.0.0.1:8879".to_string();
    let mut root = None;
    let mut origins = vec![];
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or("Missing release-control option")?;
        match flag.as_str() {
            "--bind" => bind = value,
            "--releases" => root = Some(PathBuf::from(value)),
            "--origin" => origins.push(value),
            _ => return Err("Unknown release-control option".into()),
        }
    }
    let address: std::net::SocketAddr = bind.parse()?;
    if !address.ip().is_loopback() {
        return Err("Control publisher must bind loopback".into());
    }
    let root = root.ok_or("--releases required")?;
    let initial: Release = serde_json::from_slice(&bounded(root.join("current.json"), MAX_BYTES)?)?;
    let (changed, _) = watch::channel(initial);
    let hub = Arc::new(Hub {
        root,
        changed,
        peers: Arc::new(Semaphore::new(64)),
        origins,
    });
    hub.refresh()?;
    let routes = Router::new()
        .route("/release/events", get(stream))
        .route("/publish", post(publish))
        .route("/health", get(|| async { StatusCode::OK }))
        .with_state(hub);
    println!("Release control v1: {bind}");
    axum::serve(tokio::net::TcpListener::bind(address).await?, routes).await?;
    Ok(())
}
