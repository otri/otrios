// SPDX-License-Identifier: Apache-2.0
//! Stable control stays in the socket owner; bounded state work is a separate task.
use super::*;
use futures_util::stream::SplitSink;
use otrios_interfaces::{
    release,
    session::{
        self as wire, ClientControl, MAX_CONTROL_BYTES, STATE_PROTOCOL, ServerControl, VERSION,
    },
};
use std::time::Instant;
use tokio::sync::mpsc;
pub(super) async fn public(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, ws, true, false).await
}
pub(super) async fn trusted(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, ws, false, false).await
}
pub(super) async fn public_control(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, ws, true, true).await
}
pub(super) async fn trusted_control(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, ws, false, true).await
}
async fn open(
    app: Arc<App>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    public: bool,
    control_only: bool,
) -> Response {
    let allowed = if public && !control_only {
        headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| app.origins.iter().any(|o| o == v))
    } else {
        origin_ok(&app, &headers)
    };
    if !allowed {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let slots = if public {
        app.public_peers.clone()
    } else {
        app.peers.clone()
    };
    let Ok(permit) = slots.try_acquire_owned() else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "Session limit");
    };
    let key = if public && !control_only {
        match &app.auth {
            Some(a) => a.key(&headers).await,
            None => None,
        }
    } else {
        None
    };
    let guest = chat::cookie(&headers);
    // Most session frames are small. Avoid reserving the library's 128 KiB
    // read buffer for every idle peer; complete messages retain the same bounds.
    ws.read_buffer_size(4096)
        .max_message_size(otrios_core::canvas::MAX_BYTES + 1)
        .max_frame_size(otrios_core::canvas::MAX_BYTES + 1)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            peer(app, socket, public, key, control_only, guest).await
        })
}
async fn send(output: &mut SplitSink<WebSocket, Message>, message: Message) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(2), output.send(message)).await,
        Ok(Ok(()))
    )
}
async fn control(output: &mut SplitSink<WebSocket, Message>, event: ServerControl) -> bool {
    let Ok(text) = serde_json::to_string(&event) else {
        return false;
    };
    send(output, Message::Text(text.into())).await
}
fn descriptor(value: &Value) -> Option<release::Release> {
    serde_json::from_value::<release::Release>(value.clone())
        .ok()
        .filter(|r| r.valid())
}
fn compatible(version: u32, protocol: &str) -> bool {
    version == VERSION && protocol == STATE_PROTOCOL
}
fn refresh(version: u32, protocol: &str, release: &Option<release::Release>) -> bool {
    version != VERSION
        || protocol
            != release
                .as_ref()
                .map_or(STATE_PROTOCOL, |r| r.sync_protocol.as_str())
}
enum Event {
    Release,
    Availability,
    Wire(Option<Result<Message, axum::Error>>),
    State(Option<apps::Packet>),
    Ink(Result<Vec<ink::Event>, tokio::sync::broadcast::error::RecvError>),
    Chat(Result<Arc<chat::Packet>, tokio::sync::broadcast::error::RecvError>),
    Heartbeat,
}
async fn peer(
    app: Arc<App>,
    socket: WebSocket,
    public: bool,
    key: Option<String>,
    control_only: bool,
    guest: Option<String>,
) {
    let (mut output, mut input) = socket.split();
    let Ok(Some(Ok(Message::Text(bytes)))) =
        tokio::time::timeout(Duration::from_secs(5), input.next()).await
    else {
        return;
    };
    if bytes.len() > MAX_CONTROL_BYTES {
        return;
    }
    let Ok(ClientControl::Hello {
        version,
        release,
        state_protocol,
        state,
    }) = serde_json::from_str(&bytes)
    else {
        return;
    };
    if release.as_ref().is_some_and(|id| !release::valid_hash(id)) || state_protocol.len() > 128 {
        return;
    }
    refresh_release(&app).await;
    let mut releases = app.release_changed.subscribe();
    let initial = releases.borrow_and_update().clone();
    let mut availability = app.state_available.subscribe();
    let available = *availability.borrow_and_update();
    let selected = compatible(version, &state_protocol);
    let state_enabled = state && selected && !control_only;
    let r = descriptor(&initial);
    if !control(
        &mut output,
        ServerControl::Hello {
            version: VERSION,
            refresh: refresh(version, &state_protocol, &r),
            release: r,
            compatible: selected,
            state_available: available,
        },
    )
    .await
    {
        return;
    }
    // Cover the browser's four output credits plus a mount/ACK burst.
    // Overflow still disconnects for resync; release control never awaits space.
    let (state_tx, state_input) = mpsc::channel(8);
    let (state_output, mut state_rx) = mpsc::channel(4);
    let worker = state_enabled.then(|| {
        tokio::spawn(apps::worker(
            app.clone(),
            public,
            key.clone(),
            state_input,
            state_output,
        ))
    });
    let mut ink: Option<ink::Subscription> = None;
    let mut chat_peer: Option<chat::Peer> = None;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.tick().await;
    let mut heard = Instant::now();
    let mut window = Instant::now();
    let mut commands = 0_u16;
    let mut decode_budget = otrios_core::canvas::DecodeBudget::default();
    let decode_started = Instant::now();
    loop {
        let event = tokio::select! {biased;
            result=releases.changed()=>{if result.is_err(){break;}Event::Release},
            result=availability.changed(),if state_enabled=>{if result.is_err(){break;}Event::Availability},
            _=heartbeat.tick()=>Event::Heartbeat,
            value=input.next()=>Event::Wire(value),
            value=state_rx.recv(),if state_enabled=>Event::State(value),
            value=async{match ink.as_mut(){Some(sub)=>sub.updates.recv().await,None=>std::future::pending().await}}=>Event::Ink(value),
            value=async{match chat_peer.as_mut(){Some(sub)=>sub.updates.recv().await,None=>std::future::pending().await}}=>Event::Chat(value),
        };
        match event {
            Event::Release => {
                let r = descriptor(&releases.borrow_and_update());
                let available = *availability.borrow();
                if !control(
                    &mut output,
                    ServerControl::Upgrade {
                        version: VERSION,
                        refresh: refresh(version, &state_protocol, &r),
                        release: r,
                        compatible: selected,
                        state_available: available,
                    },
                )
                .await
                {
                    break;
                }
            }
            Event::Availability => {
                let available = *availability.borrow_and_update();
                if !control(
                    &mut output,
                    ServerControl::StateStatus {
                        version: VERSION,
                        available,
                    },
                )
                .await
                {
                    break;
                }
            }
            Event::Heartbeat => {
                if heard.elapsed() > Duration::from_secs(60)
                    || !send(&mut output, Message::Ping(Bytes::new())).await
                {
                    break;
                }
            }
            Event::Wire(Some(Ok(message))) => {
                heard = Instant::now();
                match message {
                    Message::Binary(bytes) => {
                        if !state_enabled || !*availability.borrow() {
                            break;
                        }
                        match bytes.split_first() {
                            Some((&otrios_interfaces::chat::CHANNEL, payload))
                                if payload.len() <= otrios_interfaces::chat::MAX_COMMAND_BYTES =>
                            {
                                let peer = chat_peer.get_or_insert_with(|| {
                                    app.chat.subscribe(
                                        public,
                                        guest.clone(),
                                        key.clone(),
                                        app.auth.clone(),
                                    )
                                });
                                if peer.input.try_send(payload.to_vec()).is_err() {
                                    // Only the room resynchronizes; release discovery and
                                    // unrelated subscriptions retain their shared socket.
                                    chat_peer = None;
                                    let bytes = otrios_core::canvas::chat::encode_event(
                                        &otrios_interfaces::chat::Event::Resync,
                                    )
                                    .unwrap();
                                    if !send(
                                        &mut output,
                                        Message::Binary(
                                            wire::frame(otrios_interfaces::chat::CHANNEL, bytes)
                                                .into(),
                                        ),
                                    )
                                    .await
                                    {
                                        break;
                                    }
                                }
                            }
                            Some((&wire::STATE, payload)) if !payload.is_empty() => {
                                if decode_budget
                                    .admit(payload, decode_started.elapsed().as_millis() as u64)
                                    .is_err()
                                    || state_tx.try_send(payload.to_vec()).is_err()
                                {
                                    break;
                                }
                            }
                            Some((&wire::INK, payload)) => {
                                let Some(sub) = ink.as_mut() else {
                                    break;
                                };
                                if app.ink.receive(sub, payload).await.is_err() {
                                    break;
                                }
                            }
                            _ => break,
                        }
                    }
                    Message::Text(text) => {
                        if text.len() > MAX_CONTROL_BYTES || !state_enabled {
                            break;
                        }
                        if window.elapsed() >= Duration::from_secs(1) {
                            window = Instant::now();
                            commands = 0;
                        }
                        commands += 1;
                        if commands > 16 {
                            break;
                        }
                        match serde_json::from_str(&text) {
                            Ok(ClientControl::Ink { enabled: false }) => {
                                ink = None;
                            }
                            Ok(ClientControl::Ink { enabled: true }) if ink.is_none() => {
                                let Ok((subscription, bytes)) = app.ink.subscribe().await else {
                                    break;
                                };
                                ink = Some(subscription);
                                if !send(
                                    &mut output,
                                    Message::Binary(
                                        wire::frame(
                                            wire::INK,
                                            bytes[..otrios_interfaces::ink::SERVER_BYTES].to_vec(),
                                        )
                                        .into(),
                                    ),
                                )
                                .await
                                    || (bytes.len() > otrios_interfaces::ink::SERVER_BYTES
                                        && !send(
                                            &mut output,
                                            Message::Binary(
                                                wire::frame(
                                                    wire::INK,
                                                    bytes[otrios_interfaces::ink::SERVER_BYTES..]
                                                        .to_vec(),
                                                )
                                                .into(),
                                            ),
                                        )
                                        .await)
                                {
                                    break;
                                }
                            }
                            Ok(ClientControl::Ink { enabled: true }) => {}
                            _ => break,
                        }
                    }
                    Message::Ping(bytes) => {
                        if !send(&mut output, Message::Pong(bytes)).await {
                            break;
                        }
                    }
                    Message::Pong(_) => {}
                    _ => break,
                }
            }
            Event::Wire(_) => break,
            Event::State(Some(packet)) => {
                if packet.private && !apps::private_allowed(&app, public, &key).await {
                    continue;
                }
                if !send(
                    &mut output,
                    Message::Binary(wire::frame(wire::STATE, packet.bytes).into()),
                )
                .await
                {
                    break;
                }
            }
            Event::State(None) => break,
            Event::Ink(Ok(events)) => {
                let bytes = ink::encode(&events);
                if !bytes.is_empty()
                    && !send(
                        &mut output,
                        Message::Binary(wire::frame(wire::INK, bytes).into()),
                    )
                    .await
                {
                    break;
                }
            }
            Event::Ink(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Event::Ink(Err(_)) => break,
            Event::Chat(Ok(packet)) => {
                let Some(peer) = chat_peer.as_mut() else {
                    continue;
                };
                if packet.target.is_some_and(|id| id != peer.id)
                    || (packet.target.is_none() && !peer.joined)
                {
                    continue;
                }
                if let Some(joined) = packet.joined {
                    peer.joined = joined;
                }
                if !send(
                    &mut output,
                    Message::Binary(
                        wire::frame(otrios_interfaces::chat::CHANNEL, packet.bytes.clone()).into(),
                    ),
                )
                .await
                {
                    break;
                }
            }
            Event::Chat(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                chat_peer = None;
                let bytes = otrios_core::canvas::chat::encode_event(
                    &otrios_interfaces::chat::Event::Resync,
                )
                .expect("bounded event");
                if !send(
                    &mut output,
                    Message::Binary(wire::frame(otrios_interfaces::chat::CHANNEL, bytes).into()),
                )
                .await
                {
                    break;
                }
            }
            Event::Chat(Err(_)) => {
                chat_peer = None;
            }
        }
    }
    if let Some(worker) = worker {
        worker.abort();
    }
    let _ = send(&mut output, Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upgrade_is_delivered_while_the_state_authority_lock_is_held() {
        let release = |c: char| {
            let id = c.to_string().repeat(64);
            json!({"id":id,"web_entry":format!("/releases/{id}/web/index.html"),"sync_protocol":STATE_PROTOCOL})
        };
        let (changed, _) = watch::channel(0);
        let (state_available, _) = watch::channel(true);
        let (release_changed, _) = watch::channel(release('a'));
        let app = Arc::new(App {
            chat: chat::Hub::new(None).unwrap(),
            ink: ink::Room::new(8).unwrap(),
            auth: None,
            apps: Mutex::new(None),
            state_config: StateConfig {
                source: PathBuf::new(),
                state: None,
                presentation: None,
            },
            recovery: Mutex::new(()),
            state_available,
            release_changed,
            presentation_file: None,
            web: PathBuf::new(),
            releases: None,
            release: RwLock::new(release('a')),
            changed,
            origins: vec![],
            peers: Arc::new(Semaphore::new(4)),
            public_peers: Arc::new(Semaphore::new(4)),
            peer_limits: [4, 4],
        });
        let locked = app.apps.lock().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/session", get(trusted))
            .with_state(app.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let (sent, arrived) = tokio::sync::oneshot::channel();
        let client = tokio::task::spawn_blocking(move || {
            let tcp = std::net::TcpStream::connect(address).unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let (mut s, _) = tungstenite::client(format!("ws://{address}/session"), tcp).unwrap();
            s.send(tungstenite::Message::Text(json!({"type":"hello","version":1,"release":null,"state_protocol":STATE_PROTOCOL,"state":true}).to_string().into())).unwrap();
            assert!(matches!(s.read().unwrap(), tungstenite::Message::Text(_)));
            // A browser can send four credited state frames without waiting.
            // Keep that ordinary burst bounded without closing before a Ping/upgrade.
            for _ in 0..4 {
                s.send(tungstenite::Message::Binary(
                    wire::frame(
                        wire::STATE,
                        otrios_core::canvas::encode_client(
                            &otrios_interfaces::canvas::ClientMessage::Leave { epoch: 0 },
                        )
                        .unwrap(),
                    )
                    .into(),
                ))
                .unwrap();
            }
            s.send(tungstenite::Message::Ping(vec![17].into())).unwrap();
            assert!(matches!(s.read().unwrap(), tungstenite::Message::Pong(_)));
            sent.send(()).unwrap();
            loop {
                match s.read().unwrap() {
                    tungstenite::Message::Text(text) => {
                        let event: ServerControl = serde_json::from_str(&text).unwrap();
                        if matches!(event, ServerControl::Upgrade { .. }) {
                            assert_eq!(event.release().unwrap().id, "b".repeat(64));
                            break;
                        }
                    }
                    tungstenite::Message::Ping(_) => s.flush().unwrap(),
                    other => panic!("{other:?}"),
                }
            }
        });
        arrived.await.unwrap();
        app.release_changed.send_replace(release('b'));
        tokio::time::timeout(Duration::from_secs(3), client)
            .await
            .unwrap()
            .unwrap();
        drop(locked);
        server.abort();
    }
}
