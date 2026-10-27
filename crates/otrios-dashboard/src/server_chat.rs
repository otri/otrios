// SPDX-License-Identifier: Apache-2.0
//! Host adapter: bounded session queues, HTTP cookies and the covered room authority.
use super::*;
use otrios_core::canvas::chat::{self as core, Command, Delivery, Event};
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{broadcast, mpsc};
const COOKIE: &str = "__Host-otrios_guest";
pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
pub(super) struct Packet {
    pub target: Option<u64>,
    pub bytes: Vec<u8>,
    pub joined: Option<bool>,
}
pub(super) struct Hub {
    room: Arc<StdMutex<RoomState>>,
    bus: broadcast::Sender<Arc<Packet>>,
    next: AtomicU64,
    provisions: StdMutex<(u64, u16)>,
}
enum RoomState {
    Unopened(Option<PathBuf>),
    Ready(Box<core::Room>),
    Failed,
}
pub(super) struct Peer {
    pub id: u64,
    pub input: mpsc::Sender<Vec<u8>>,
    pub updates: broadcast::Receiver<Arc<Packet>>,
    pub joined: bool,
}
impl Hub {
    pub fn new(path: Option<PathBuf>) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            // Recovery belongs to the room, never to the release-control bootstrap.
            room: Arc::new(StdMutex::new(RoomState::Unopened(path))),
            bus: broadcast::channel(128).0,
            next: AtomicU64::new(1),
            provisions: StdMutex::new((0, 0)),
        }))
    }
    fn admit_guest(&self) -> bool {
        let Ok(mut window) = self.provisions.lock() else {
            return false;
        };
        let time = now();
        if time.saturating_sub(window.0) >= 60_000 {
            *window = (time, 0);
        }
        window.1 = window.1.saturating_add(1);
        window.1 <= 128
    }
    fn publish(&self, events: Vec<Delivery>) {
        for event in events {
            let joined = match event.event {
                Event::Welcome { .. } => Some(true),
                Event::Kicked { .. } | Event::Left => Some(false),
                _ => None,
            };
            match core::encode_event(&event.event) {
                Ok(bytes) => {
                    let _ = self.bus.send(Arc::new(Packet {
                        target: event.target,
                        bytes,
                        joined,
                    }));
                }
                Err(e) => eprintln!("Room encoding failed: {e}"),
            }
        }
    }
    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut core::Room) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let room = self.room.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = room.lock().map_err(|_| "Room lock unavailable")?;
            if let RoomState::Unopened(path) = &mut *guard {
                *guard = match core::Room::open(path.take(), now()) {
                    Ok(room) => RoomState::Ready(Box::new(room)),
                    Err(error) => {
                        eprintln!("Chat recovery failed: {error}");
                        RoomState::Failed
                    }
                };
            }
            match &mut *guard {
                RoomState::Ready(room) => work(room),
                _ => Err("Room history unavailable; operator recovery required".into()),
            }
        })
        .await
        .map_err(|_| "Room task failed")?
    }
    pub fn subscribe(
        self: &Arc<Self>,
        public: bool,
        cookie: Option<String>,
        key: Option<String>,
        auth: Option<Arc<auth::Sessions>>,
    ) -> Peer {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (input, mut commands) = mpsc::channel::<Vec<u8>>(8);
        let updates = self.bus.subscribe();
        let hub = self.clone();
        tokio::spawn(async move {
            let mut auth_key = key;
            let mut clock = tokio::time::interval(Duration::from_millis(500));
            let mut rate = (now(), 0_u16);
            loop {
                let bytes = tokio::select! {value=commands.recv()=>match value{Some(bytes)=>Some(bytes),None=>break},_=clock.tick()=>None};
                let admin = match (&auth, &auth_key) {
                    (Some(auth), Some(key)) => auth.valid(key).await,
                    _ => false,
                };
                let events = hub
                    .run(move |room| {
                        let mut events = room.set_admin(id, admin);
                        events.extend(room.tick(now())?);
                        Ok(events)
                    })
                    .await;
                if let Ok(events) = events {
                    hub.publish(events);
                }
                let Some(bytes) = bytes else { continue };
                if now().saturating_sub(rate.0) >= 1000 {
                    rate = (now(), 0);
                }
                rate.1 += 1;
                if rate.1 > 40 {
                    break;
                }
                let command = match core::decode_command(&bytes) {
                    Ok(command) => command,
                    Err(_) => break,
                };
                let nonce = if let Command::Send { nonce, .. } = &command {
                    Some(nonce.clone())
                } else {
                    None
                };
                let result=match command {
                    Command::Join{credential}=>{
                        if public&&credential.is_some(){Err("Browser identity must use its guest cookie".into())}
                        else {let token=if public{cookie.clone()}else{credential};if token.is_none() && !hub.admit_guest() {Err("Guest provisioning rate limit".into())}else{hub.run(move|room|room.join(id,token.as_deref(),admin,now(),!public)).await}}
                    }
                    Command::Authenticate{username,password} if !public=>{
                        match &auth{Some(auth)=>match auth.authenticate(&username,&password).await{Ok((_,key))=>{auth_key=Some(key);hub.run(move|room|Ok(room.set_admin(id,true))).await},Err(_)=>Err("Monitoring sign-in failed; check credentials or wait before retrying".into())},None=>Err("Monitoring sign-in is unavailable".into())}
                    }
                    Command::Authenticate{..}=>Err("Use the Monitoring sign-in form".into()),
                    Command::SignOut=>{if let (Some(auth),Some(key))=(&auth,auth_key.take()){auth.revoke(&key).await;}hub.run(move|room|Ok(room.set_admin(id,false))).await},
                    command=>hub.run(move|room|room.handle(id,command,now())).await,
                };
                match result {
                    Ok(events) => hub.publish(events),
                    Err(message) => hub.publish(vec![Delivery {
                        target: Some(id),
                        event: Event::Error { message, nonce },
                    }]),
                }
            }
            if let Ok(events) = hub.run(move |room| room.leave(id, now())).await {
                hub.publish(events);
            }
        });
        Peer {
            id,
            input,
            updates,
            joined: false,
        }
    }
}
pub(super) fn cookie(headers: &HeaderMap) -> Option<String> {
    let mut result = None;
    for header in headers.get_all(header::COOKIE) {
        for part in header.to_str().ok()?.split(';') {
            if let Some((name, value)) = part.trim().split_once('=')
                && name == COOKIE
            {
                if result.is_some()
                    || value.len() != 64
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                {
                    return None;
                }
                result = Some(value.into());
            }
        }
    }
    result
}
pub(super) async fn identity(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| app.origins.iter().any(|o| o == v))
        || headers
            .get("x-otrios-request")
            .is_none_or(|v| v != "session")
    {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let credential = cookie(&headers);
    let candidate = credential.clone();
    let valid = match app
        .chat
        .run(move |room| {
            Ok(candidate
                .as_ref()
                .is_some_and(|token| room.valid_identity(token)))
        })
        .await
    {
        Ok(valid) => valid,
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Room history unavailable"),
    };
    if !valid && !app.chat.admit_guest() {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Guest provisioning rate limit",
        );
    }
    let result = app
        .chat
        .run(move |room| room.identity(credential.as_deref().filter(|_| valid), now()))
        .await;
    match result {
        Ok((profile, token)) => {
            let mut response = binary(
                "application/json",
                json!({"participant":profile.id,"name":profile.name,"avatar":profile.avatar})
                    .to_string()
                    .into_bytes(),
                false,
            );
            response.headers_mut().insert(
                header::SET_COOKIE,
                format!(
                    "{COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=2592000"
                )
                .parse()
                .unwrap(),
            );
            response
        }
        Err(_) => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Guest identity temporarily unavailable",
        ),
    }
}
