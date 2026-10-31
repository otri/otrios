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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::mpsc;
pub(super) async fn public(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    uri: axum::extract::OriginalUri,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, uri, ws, true, false).await
}
pub(super) async fn trusted(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    uri: axum::extract::OriginalUri,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, uri, ws, false, false).await
}
pub(super) async fn public_control(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    uri: axum::extract::OriginalUri,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, uri, ws, true, true).await
}
pub(super) async fn trusted_control(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    uri: axum::extract::OriginalUri,
    ws: WebSocketUpgrade,
) -> Response {
    open(app, headers, uri, ws, false, true).await
}
async fn open(
    app: Arc<App>,
    headers: HeaderMap,
    uri: axum::extract::OriginalUri,
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
    let space = match spaces::selected(&uri.0) {
        Ok(space) => space,
        Err(status) => return failure(status, "Invalid Space selection"),
    };
    let key = if !control_only {
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
            peer(app, socket, public, key, control_only, guest, space).await
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
#[derive(Default)]
struct ManagedInk {
    wanted: bool,
    active: Option<apps::Activation>,
    room: Option<Arc<ink::Room>>,
}
impl ManagedInk {
    fn current(
        &self,
        live: &watch::Receiver<Option<apps::Activation>>,
    ) -> Option<&apps::Activation> {
        if live.has_changed().is_err() {
            return None;
        }
        self.active
            .as_ref()
            .filter(|active| live.borrow().as_ref() == Some(*active))
    }
    fn clear(&mut self, subscription: &mut Option<ink::Subscription>) {
        *subscription = None;
        self.room = None;
    }
    async fn refresh(
        &mut self,
        app: &App,
        key: &Option<String>,
        space: &Option<String>,
        live: &watch::Receiver<Option<apps::Activation>>,
        subscription: &mut Option<ink::Subscription>,
        output: &mut SplitSink<WebSocket, Message>,
    ) -> bool {
        let mut guard = app.apps.lock().await;
        let scope = spaces::Scope {
            space: space.clone(),
            identity: spaces::identity(app, key).await,
        };
        let active = self.current(live).cloned();
        let Some(active) = active.filter(|active| self.wanted && active.ink) else {
            self.clear(subscription);
            return true;
        };
        let Some(hub) = guard.as_mut().filter(|hub| {
            hub.allowed(
                &scope,
                &active.binding,
                otrios_interfaces::spaces::Operation::Read,
            )
        }) else {
            self.clear(subscription);
            return true;
        };
        if self.room.is_some() && subscription.is_some() {
            return true;
        }
        let Ok(room) = hub.ink_room(&active, app.peer_limits.iter().sum()) else {
            return false;
        };
        let Ok((peer, bytes)) = room.subscribe().await else {
            return false;
        };
        if !send(
            output,
            Message::Binary(wire::frame(wire::INK, wire::scoped(active.epoch, bytes)).into()),
        )
        .await
        {
            return false;
        }
        self.room = Some(room);
        *subscription = Some(peer);
        true
    }
}
enum Event {
    Release,
    Availability,
    Wire(Option<Result<Message, axum::Error>>),
    State(Option<apps::Packet>),
    Spaces(Option<spaces::Packet>),
    Collection(Option<collections::Packet>),
    Ink(Result<Vec<ink::Event>, tokio::sync::broadcast::error::RecvError>),
    Chat(Result<Arc<chat::Packet>, tokio::sync::broadcast::error::RecvError>),
    ManagedChat(Option<space_chat::Packet>),
    Heartbeat,
    Activation,
}
async fn peer(
    app: Arc<App>,
    socket: WebSocket,
    public: bool,
    key: Option<String>,
    control_only: bool,
    guest: Option<String>,
    space: Option<String>,
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
    let (activation, mut live) = watch::channel(None);
    let worker = state_enabled.then(|| {
        tokio::spawn(apps::worker(
            app.clone(),
            public,
            key.clone(),
            space.clone(),
            state_input,
            state_output,
            activation,
        ))
    });
    let mut ink: Option<ink::Subscription> = None;
    let managed = app.state_config.spaces.is_some();
    let mut managed_ink = ManagedInk::default();
    let mut lease_open = state_enabled;
    let mut collection_peer: Option<collections::Peer> = None;
    let mut spaces_peer: Option<spaces::Peer> = None;
    let mut admission = Arc::new(AtomicBool::new(false));
    let mut chat_peer: Option<chat::Peer> = None;
    let mut managed_chat: Option<space_chat::Peer> = None;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.tick().await;
    let mut heard = Instant::now();
    let mut window = Instant::now();
    let mut commands = 0_u16;
    let mut collection_rate = (Instant::now(), 0_u16);
    let mut decode_budget = otrios_core::canvas::DecodeBudget::default();
    let decode_started = Instant::now();
    loop {
        let event = tokio::select! {biased;
            result=releases.changed()=>{if result.is_err(){break;}Event::Release},
            result=availability.changed(),if state_enabled=>{if result.is_err(){break;}Event::Availability},
            result=live.changed(),if lease_open=>{if result.is_err(){lease_open=false;}Event::Activation},
            _=heartbeat.tick()=>Event::Heartbeat,
            value=input.next()=>Event::Wire(value),
            value=state_rx.recv(),if state_enabled=>Event::State(value),
            value=async{match spaces_peer.as_mut(){Some(peer)=>peer.output.recv().await,None=>std::future::pending().await}}=>Event::Spaces(value),
            value=async{match collection_peer.as_mut(){Some(peer)=>peer.output.recv().await,None=>std::future::pending().await}}=>Event::Collection(value),
            value=async{match ink.as_mut(){Some(sub)=>sub.updates.recv().await,None=>std::future::pending().await}}=>Event::Ink(value),
            value=async{match chat_peer.as_mut(){Some(sub)=>sub.updates.recv().await,None=>std::future::pending().await}}=>Event::Chat(value),
            value=async{match managed_chat.as_mut(){Some(peer)=>peer.output.recv().await,None=>std::future::pending().await}}=>Event::ManagedChat(value),
        };
        match event {
            Event::Activation => {
                let current = live.borrow_and_update().clone();
                if managed && (!lease_open || managed_ink.active != current) {
                    managed_ink.clear(&mut ink);
                    managed_ink.active = None;
                }
                if !lease_open
                    || managed_chat
                        .as_ref()
                        .is_some_and(|peer| current.as_ref() != Some(&peer.active))
                {
                    managed_chat = None;
                }
            }
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
                        // Auxiliary lanes cannot fall back to global rooms.
                        if managed
                            && !matches!(
                                bytes.first(),
                                Some(&wire::STATE)
                                    | Some(&otrios_interfaces::spaces::CHANNEL)
                                    | Some(&wire::INK)
                                    | Some(&otrios_interfaces::chat::CHANNEL)
                                    | Some(&otrios_interfaces::collection::CHANNEL)
                            )
                        {
                            break;
                        }
                        if managed
                            && matches!(
                                bytes.first(),
                                Some(&otrios_interfaces::chat::CHANNEL)
                                    | Some(&otrios_interfaces::collection::CHANNEL)
                            )
                        {
                            let (channel, payload) = bytes.split_first().expect("channel");
                            let Ok((epoch, payload)) = wire::scope(payload) else {
                                break;
                            };
                            let bound = if *channel == otrios_interfaces::chat::CHANNEL {
                                otrios_interfaces::chat::MAX_COMMAND_BYTES
                            } else {
                                otrios_interfaces::collection::MAX_COMMAND_BYTES
                            };
                            if payload.is_empty() || payload.len() > bound {
                                break;
                            }
                            let guard = app.apps.lock().await;
                            let scope = spaces::Scope {
                                space: space.clone(),
                                identity: spaces::identity(&app, &key).await,
                            };
                            let Some(active) = managed_ink
                                .current(&live)
                                .filter(|active| active.epoch == epoch && active.chat)
                                .cloned()
                            else {
                                continue;
                            };
                            let Some(hub) = guard.as_ref().filter(|hub| {
                                hub.allowed(
                                    &scope,
                                    &active.binding,
                                    otrios_interfaces::spaces::Operation::Read,
                                )
                            }) else {
                                continue;
                            };
                            if managed_chat
                                .as_ref()
                                .is_some_and(|peer| peer.active != active)
                            {
                                managed_chat = None;
                            }
                            if managed_chat.is_none() {
                                let Ok(room) = hub.chat_room(&active) else {
                                    break;
                                };
                                managed_chat = Some(space_chat::subscribe(
                                    app.clone(),
                                    room,
                                    active,
                                    live.clone(),
                                    key.clone(),
                                    space.clone(),
                                ));
                            }
                            if managed_chat
                                .as_ref()
                                .expect("subscribed room")
                                .input
                                .try_send(space_chat::Packet {
                                    channel: *channel,
                                    bytes: payload.to_vec(),
                                })
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                        match bytes.split_first() {
                            Some((&otrios_interfaces::spaces::CHANNEL, payload))
                                if !payload.is_empty()
                                    && payload.len()
                                        <= otrios_interfaces::spaces::MAX_COMMAND_BYTES =>
                            {
                                let peer = spaces_peer.get_or_insert_with(|| {
                                    spaces::subscribe(app.clone(), key.clone())
                                });
                                if peer.input.try_send(payload.to_vec()).is_err() {
                                    spaces_peer = None;
                                    let bytes = otrios_core::spaces::encode_event(
                                        &otrios_interfaces::spaces::Event::ResyncRequired {
                                            version: 1,
                                        },
                                    )
                                    .expect("bounded status");
                                    if !send(
                                        &mut output,
                                        Message::Binary(
                                            wire::frame(otrios_interfaces::spaces::CHANNEL, bytes)
                                                .into(),
                                        ),
                                    )
                                    .await
                                    {
                                        break;
                                    }
                                }
                            }
                            Some((&otrios_interfaces::collection::CHANNEL, payload))
                                if !payload.is_empty()
                                    && payload.len()
                                        <= otrios_interfaces::collection::MAX_COMMAND_BYTES =>
                            {
                                if collection_rate.0.elapsed() >= Duration::from_secs(1) {
                                    collection_rate = (Instant::now(), 0);
                                }
                                collection_rate.1 += 1;
                                if collection_rate.1 > 192 {
                                    break;
                                }
                                let peer = collection_peer.get_or_insert_with(|| {
                                    collections::subscribe(
                                        app.clone(),
                                        public,
                                        key.clone(),
                                        admission.clone(),
                                    )
                                });
                                if peer.input.try_send(payload.to_vec()).is_err() {
                                    collection_peer = None;
                                    let bytes = otrios_core::collection::Event::Rejected {
                                        generation: 0,
                                        status: otrios_core::collection::Status::ResyncRequired,
                                    }
                                    .to_bytes()
                                    .expect("bounded status");
                                    if !send(
                                        &mut output,
                                        Message::Binary(
                                            wire::frame(
                                                otrios_interfaces::collection::CHANNEL,
                                                bytes,
                                            )
                                            .into(),
                                        ),
                                    )
                                    .await
                                    {
                                        break;
                                    }
                                }
                            }
                            Some((&otrios_interfaces::chat::CHANNEL, payload))
                                if payload.len() <= otrios_interfaces::chat::MAX_COMMAND_BYTES =>
                            {
                                let peer = chat_peer.get_or_insert_with(|| {
                                    app.chat.subscribe(
                                        public,
                                        guest.clone(),
                                        key.clone(),
                                        app.auth.clone(),
                                        admission.clone(),
                                    )
                                });
                                if peer.input.try_send(payload.to_vec()).is_err() {
                                    // Only the room resynchronizes; release discovery and
                                    // unrelated subscriptions retain their shared socket.
                                    chat_peer = None;
                                    admission.store(false, Ordering::Release);
                                    admission = Arc::new(AtomicBool::new(false));
                                    collection_peer = None;
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
                                if managed {
                                    let Ok((epoch, payload)) = wire::scope(payload) else {
                                        break;
                                    };
                                    let guard = app.apps.lock().await;
                                    let scope = spaces::Scope {
                                        space: space.clone(),
                                        identity: spaces::identity(&app, &key).await,
                                    };
                                    let Some(active) = managed_ink
                                        .current(&live)
                                        .filter(|active| active.epoch == epoch)
                                    else {
                                        continue;
                                    };
                                    if guard.as_ref().is_none_or(|hub| {
                                        !hub.allowed(
                                            &scope,
                                            &active.binding,
                                            otrios_interfaces::spaces::Operation::Edit,
                                        )
                                    }) {
                                        continue;
                                    }
                                    let (Some(room), Some(peer)) =
                                        (&managed_ink.room, ink.as_mut())
                                    else {
                                        continue;
                                    };
                                    if room.receive(peer, payload).await.is_err() {
                                        break;
                                    }
                                    continue;
                                }
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
                            Ok(ClientControl::Ink { enabled }) if managed => {
                                managed_ink.wanted = enabled;
                                if !managed_ink
                                    .refresh(&app, &key, &space, &live, &mut ink, &mut output)
                                    .await
                                {
                                    break;
                                }
                            }
                            Ok(ClientControl::Ink { enabled: false }) => {
                                ink = None;
                            }
                            Ok(ClientControl::Ink { enabled: true }) if ink.is_none() => {
                                if app.state_config.spaces.is_some() {
                                    break;
                                }
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
                // Hold the same lock as management through the bounded send:
                // revocation cannot acknowledge ahead of an older queued payload.
                let guard = if packet.binding.is_some() {
                    Some(app.apps.lock().await)
                } else {
                    None
                };
                let scope = spaces::Scope {
                    space: space.clone(),
                    identity: spaces::identity(&app, &key).await,
                };
                if let Some(binding) = &packet.binding
                    && guard.as_ref().and_then(|g| g.as_ref()).is_none_or(|hub| {
                        !hub.allowed(&scope, binding, otrios_interfaces::spaces::Operation::Read)
                    })
                {
                    continue;
                }
                if packet.private && !apps::private_allowed(&app, public, &key).await {
                    continue;
                }
                if packet
                    .activated
                    .as_deref()
                    .is_some_and(|active| live.borrow().as_ref() != Some(active))
                {
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
                drop(guard);
                if let Some(active) = packet.activated {
                    managed_ink.clear(&mut ink);
                    managed_ink.active = Some(*active);
                    if !managed_ink
                        .refresh(&app, &key, &space, &live, &mut ink, &mut output)
                        .await
                    {
                        break;
                    }
                }
            }
            Event::State(None) => break,
            Event::ManagedChat(Some(packet)) => {
                let guard = app.apps.lock().await;
                let scope = spaces::Scope {
                    space: space.clone(),
                    identity: spaces::identity(&app, &key).await,
                };
                let Some(peer) = managed_chat.as_ref() else {
                    continue;
                };
                if managed_ink.current(&live) != Some(&peer.active)
                    || guard.as_ref().is_none_or(|hub| {
                        !hub.allowed(
                            &scope,
                            &peer.active.binding,
                            otrios_interfaces::spaces::Operation::Read,
                        )
                    })
                {
                    continue;
                }
                if !send(
                    &mut output,
                    Message::Binary(
                        wire::frame(
                            packet.channel,
                            wire::scoped(peer.active.epoch, packet.bytes),
                        )
                        .into(),
                    ),
                )
                .await
                {
                    break;
                }
            }
            Event::ManagedChat(None) => break,
            Event::Spaces(Some(packet)) => {
                let guard = app.apps.lock().await;
                let principal = spaces::identity(&app, &key).await;
                if principal != packet.identity
                    || packet.cursor.as_ref().is_some_and(|cursor| {
                        guard
                            .as_ref()
                            .is_none_or(|hub| hub.discovery_cursor() != cursor)
                    })
                {
                    continue;
                }
                if !send(
                    &mut output,
                    Message::Binary(
                        wire::frame(otrios_interfaces::spaces::CHANNEL, packet.bytes).into(),
                    ),
                )
                .await
                {
                    break;
                }
                drop(guard);
            }
            Event::Spaces(None) => break,
            Event::Collection(Some(packet)) => {
                // Never flush a private view after the admission that produced
                // it expires, including while renderer/socket work was blocked.
                let bytes = if (packet.required.authenticated
                    && !apps::private_allowed(&app, public, &key).await)
                    || (packet.required.admitted && !admission.load(Ordering::Acquire))
                {
                    packet.revoked
                } else {
                    packet.bytes
                };
                if !send(
                    &mut output,
                    Message::Binary(
                        wire::frame(otrios_interfaces::collection::CHANNEL, bytes).into(),
                    ),
                )
                .await
                {
                    break;
                }
            }
            Event::Collection(None) => {
                collection_peer = None;
                let bytes = otrios_core::collection::Event::Rejected {
                    generation: 0,
                    status: otrios_core::collection::Status::Unavailable,
                }
                .to_bytes()
                .expect("bounded status");
                if !send(
                    &mut output,
                    Message::Binary(
                        wire::frame(otrios_interfaces::collection::CHANNEL, bytes).into(),
                    ),
                )
                .await
                {
                    break;
                }
            }
            Event::Ink(Ok(events)) => {
                let bytes = ink::encode(&events);
                if managed {
                    let guard = app.apps.lock().await;
                    let scope = spaces::Scope {
                        space: space.clone(),
                        identity: spaces::identity(&app, &key).await,
                    };
                    let Some(active) = managed_ink.current(&live) else {
                        continue;
                    };
                    if guard.as_ref().is_none_or(|hub| {
                        !hub.allowed(
                            &scope,
                            &active.binding,
                            otrios_interfaces::spaces::Operation::Read,
                        )
                    }) {
                        continue;
                    }
                    if !bytes.is_empty()
                        && !send(
                            &mut output,
                            Message::Binary(
                                wire::frame(wire::INK, wire::scoped(active.epoch, bytes)).into(),
                            ),
                        )
                        .await
                    {
                        break;
                    }
                    continue;
                }
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
                admission.store(false, Ordering::Release);
                admission = Arc::new(AtomicBool::new(false));
                collection_peer = None;
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
                admission.store(false, Ordering::Release);
                admission = Arc::new(AtomicBool::new(false));
                collection_peer = None;
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
        let collections = collections::Hub::new().unwrap();
        let chat = chat::Hub::new(None, collections.core.clone()).unwrap();
        let app = Arc::new(App {
            collections,
            chat,
            ink: ink::Room::new(8).unwrap(),
            auth: None,
            apps: Mutex::new(None),
            state_config: StateConfig {
                bundle_key: PathBuf::from("unconfigured-test-key"),
                source: PathBuf::new(),
                state: None,
                presentation: None,
                spaces: None,
                provisioning: false,
                collector: None,
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
