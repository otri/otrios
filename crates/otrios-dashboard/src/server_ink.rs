// SPDX-License-Identifier: Apache-2.0
//! Five-second drawing relay. No replica, private view, credential, or file access.
use super::{App, failure, origin_ok};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use otrios_interfaces::ink::{self, Point};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore, broadcast};
#[derive(Clone, Copy)]
struct Event {
    at: Instant,
    peer: u32,
    point: Point,
}
pub(super) struct Room {
    recent: Mutex<VecDeque<Event>>,
    bus: broadcast::Sender<Vec<Event>>,
    slots: Arc<Semaphore>,
    next: AtomicU32,
}
impl Room {
    pub(super) async fn expire(&self) {
        self.recent
            .lock()
            .await
            .retain(|e| e.at.elapsed() < Duration::from_millis(ink::LIFETIME_MS));
    }
    pub(super) fn new() -> Result<Self, String> {
        let mut seed = [0; 4];
        getrandom::fill(&mut seed).map_err(|_| "Ink session seed unavailable")?;
        Ok(Self {
            recent: Mutex::new(VecDeque::new()),
            bus: broadcast::channel(128).0,
            slots: Arc::new(Semaphore::new(16)),
            next: AtomicU32::new(u32::from_le_bytes(seed).max(1)),
        })
    }
}
pub(super) async fn public(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let allowed = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|o| app.origins.iter().any(|a| a == o));
    open(app, upgrade, allowed)
}
pub(super) async fn trusted(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let allowed = origin_ok(&app, &headers);
    open(app, upgrade, allowed)
}
fn open(app: Arc<App>, upgrade: WebSocketUpgrade, allowed: bool) -> Response {
    if !allowed {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let Ok(permit) = app.ink.slots.clone().try_acquire_owned() else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "Ink room full");
    };
    upgrade
        .max_message_size(32 * ink::CLIENT_BYTES)
        .max_frame_size(32 * ink::CLIENT_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            peer(app, socket).await;
        })
        .into_response()
}
fn encode(events: &[Event]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| {
            let age = e.at.elapsed().as_millis();
            (age < u128::from(ink::LIFETIME_MS)).then(|| ink::envelope(e.peer, age as u16, e.point))
        })
        .flatten()
        .collect()
}
async fn send(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    bytes: Vec<u8>,
) -> bool {
    bytes.is_empty()
        || matches!(
            tokio::time::timeout(
                Duration::from_secs(1),
                sink.send(Message::Binary(bytes.into()))
            )
            .await,
            Ok(Ok(()))
        )
}
async fn peer(app: Arc<App>, socket: WebSocket) {
    let room = &app.ink;
    let id = room.next.fetch_add(1, Ordering::Relaxed).max(1);
    let mut updates = room.bus.subscribe();
    let (mut sink, mut source) = socket.split();
    if !send(&mut sink, ink::welcome(id)).await {
        return;
    }
    let history = {
        let mut recent = room.recent.lock().await;
        recent.retain(|e| e.at.elapsed() < Duration::from_millis(ink::LIFETIME_MS));
        recent.iter().copied().collect::<Vec<_>>()
    };
    if !send(&mut sink, encode(&history)).await {
        return;
    }
    let mut window = Instant::now();
    let mut count = 0;
    loop {
        tokio::select! {
            incoming=source.next()=> {
                match incoming {
                    Some(Ok(Message::Binary(bytes))) => {
                        if bytes.is_empty() || bytes.len()%ink::CLIENT_BYTES!=0 {break;}
                        let points=bytes.chunks_exact(ink::CLIENT_BYTES).map(Point::decode).collect::<Result<Vec<_>,_>>();
                        let Ok(points)=points else{break;};
                        if window.elapsed()>=Duration::from_secs(1) {window=Instant::now();count=0;}
                        count+=points.len();if count>64 {break;}
                        let at=Instant::now();let events=points.into_iter().map(|point|Event{at,peer:id,point}).collect::<Vec<_>>();
                        {let mut recent=room.recent.lock().await;
                         recent.retain(|e|e.at.elapsed()<Duration::from_millis(ink::LIFETIME_MS));
                         for event in &events {if recent.len()>=1024 {recent.pop_front();}recent.push_back(*event);}}
                        let _=room.bus.send(events);
                    }
                    Some(Ok(Message::Ping(bytes))) => {if sink.send(Message::Pong(bytes)).await.is_err(){break;}},
                    Some(Ok(Message::Pong(_)))=>{},
                    _=>break,
                }
            },
            events=updates.recv()=> {
                match events {Ok(events)=>{if !send(&mut sink,encode(&events)).await {break;}},Err(_)=>break}
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), sink.close()).await;
}
