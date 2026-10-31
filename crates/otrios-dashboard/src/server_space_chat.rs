// SPDX-License-Identifier: Apache-2.0
//! Collection-owned transport adapters. Core owns profiles, journals and windows.
use super::*;
use otrios_core::{canvas::chat as room, collection as col};
use otrios_interfaces::{chat as wire, spaces::Operation};
use std::sync::Mutex as StdMutex;
use tokio::sync::{broadcast, mpsc};

pub(super) struct Hub {
    pub chat: Arc<chat::Hub>,
    collections: Arc<collections::Hub>,
}
impl Hub {
    pub fn new(path: PathBuf) -> Result<Arc<Self>, String> {
        let collections = collections::Hub::new()?;
        let chat = chat::Hub::new(Some(path), collections.core.clone())?;
        Ok(Arc::new(Self { chat, collections }))
    }
    pub fn healthy(&self) -> bool {
        self.chat.healthy()
    }
}
pub(super) struct Packet {
    pub channel: u8,
    pub bytes: Vec<u8>,
}
pub(super) struct Peer {
    pub active: apps::Activation,
    pub input: mpsc::Sender<Packet>,
    pub output: mpsc::Receiver<Packet>,
    worker: tokio::task::JoinHandle<()>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
struct Presence {
    hub: Arc<chat::Hub>,
    id: u64,
}
impl Drop for Presence {
    fn drop(&mut self) {
        // No durable write, including after deletion or an aborted socket task.
        self.hub.abandon(self.id);
    }
}
pub(super) fn subscribe(
    app: Arc<App>,
    hub: Arc<Hub>,
    active: apps::Activation,
    live: watch::Receiver<Option<apps::Activation>>,
    key: Option<String>,
    space: Option<String>,
) -> Peer {
    let (input, receiver) = mpsc::channel(8);
    let (sender, output) = mpsc::channel(1);
    let worker = tokio::spawn(worker(
        app,
        hub,
        active.clone(),
        live,
        key,
        space,
        receiver,
        sender,
    ));
    Peer {
        active,
        input,
        output,
        worker,
    }
}
enum Work {
    Input(Option<Packet>),
    Chat(Result<Arc<chat::Packet>, broadcast::error::RecvError>),
    Collection,
    Tick,
    Lease,
}
#[allow(clippy::too_many_arguments)]
async fn worker(
    app: Arc<App>,
    hub: Arc<Hub>,
    active: apps::Activation,
    mut live: watch::Receiver<Option<apps::Activation>>,
    key: Option<String>,
    space: Option<String>,
    mut input: mpsc::Receiver<Packet>,
    output: mpsc::Sender<Packet>,
) {
    let (id, mut events) = hub.chat.observe();
    let _presence = Presence {
        hub: hub.chat.clone(),
        id,
    };
    let session = Arc::new(StdMutex::new(col::ChannelSession::default()));
    let mut collection = hub.collections.changed.subscribe();
    let mut clock = tokio::time::interval(Duration::from_millis(500));
    let mut joined = false;
    let mut rate = (chat::now(), 0_u16, 0_u16);
    loop {
        let work = tokio::select! {biased;
            _ = live.changed() => Work::Lease,
            value = input.recv() => Work::Input(value),
            value = events.recv() => Work::Chat(value),
            _ = collection.changed() => Work::Collection,
            _ = clock.tick() => Work::Tick,
        };
        let guard = app.apps.lock().await;
        let scope = spaces::Scope {
            space: space.clone(),
            identity: spaces::identity(&app, &key).await,
        };
        let Some(authority) = guard.as_ref().filter(|hub| {
            live.has_changed().is_ok()
                && live.borrow().as_ref() == Some(&active)
                && hub.allowed(&scope, &active.binding, Operation::Read)
        }) else {
            break;
        };
        let edit = authority.allowed(&scope, &active.binding, Operation::Edit);
        // Room moderation is an ordinary collaborative chat action. It does
        // not grant Space administration or admit public comment visitors.
        let moderate = edit;
        let packet = match work {
            Work::Input(None) => break,
            Work::Lease => continue,
            Work::Input(Some(packet)) => {
                let time = chat::now();
                if time.saturating_sub(rate.0) >= 1000 {
                    rate = (time, 0, 0);
                }
                if packet.channel == wire::CHANNEL {
                    rate.1 += 1;
                    if rate.1 > 40 {
                        break;
                    }
                    let Ok(command) = room::decode_command(&packet.bytes) else {
                        break;
                    };
                    let nonce = if let wire::Command::Send { nonce, .. } = &command {
                        Some(nonce.clone())
                    } else {
                        None
                    };
                    let identity = scope.identity.clone();
                    let result = hub.chat.run_inline(move |room| {
                        if matches!(command, wire::Command::Leave) {
                            return Ok(room.abandon(id, time));
                        }
                        if !edit {
                            return Err("Space membership is required for collaboration".into());
                        }
                        let mut events = room.set_admin(id, moderate);
                        events.extend(match command {
                            wire::Command::Join { credential: None } => room.join_admitted(
                                id,
                                identity.as_deref().ok_or("Sign in first")?,
                                moderate,
                                time,
                            )?,
                            wire::Command::Join { .. }
                            | wire::Command::Authenticate { .. }
                            | wire::Command::SignOut => {
                                return Err("Use the host's Space identity controls".into());
                            }
                            command => room.handle(id, command, time)?,
                        });
                        Ok(events)
                    });
                    match result {
                        Ok(events) => hub.chat.publish(events),
                        Err(message) => hub.chat.publish(vec![room::Delivery {
                            target: Some(id),
                            event: wire::Event::Error { message, nonce },
                        }]),
                    }
                    None
                } else if packet.channel == col::CHANNEL {
                    rate.2 += 1;
                    if rate.2 > 192 {
                        break;
                    }
                    let Ok(command) = col::decode_command(&packet.bytes) else {
                        break;
                    };
                    match collections::step_sync(
                        &hub.collections.core,
                        &session,
                        col::Access {
                            admitted: true,
                            authenticated: false,
                        },
                        Some(command),
                        true,
                    ) {
                        Ok(packet) => packet.map(|packet| Packet {
                            channel: col::CHANNEL,
                            bytes: packet.bytes,
                        }),
                        Err(_) => break,
                    }
                } else {
                    break;
                }
            }
            Work::Chat(Ok(packet)) => {
                if packet.target.is_some_and(|target| target != id)
                    || packet.target.is_none() && !joined
                {
                    continue;
                }
                if let Some(value) = packet.joined {
                    joined = value;
                }
                Some(Packet {
                    channel: wire::CHANNEL,
                    bytes: packet.bytes.clone(),
                })
            }
            Work::Chat(Err(_)) => break,
            Work::Collection => {
                match collections::step_sync(
                    &hub.collections.core,
                    &session,
                    col::Access {
                        admitted: true,
                        authenticated: false,
                    },
                    None,
                    true,
                ) {
                    Ok(packet) => packet.map(|packet| Packet {
                        channel: col::CHANNEL,
                        bytes: packet.bytes,
                    }),
                    Err(_) => break,
                }
            }
            Work::Tick => {
                if edit {
                    match hub.chat.run_inline(move |room| {
                        let mut events = room.set_admin(id, moderate);
                        events.extend(room.tick(chat::now())?);
                        Ok(events)
                    }) {
                        Ok(events) => hub.chat.publish(events),
                        Err(_) => break,
                    }
                } else if joined {
                    hub.chat.abandon(id);
                    joined = false;
                }
                None
            }
        };
        drop(guard);
        if let Some(packet) = packet
            && !matches!(
                tokio::time::timeout(Duration::from_secs(2), output.send(packet)).await,
                Ok(Ok(()))
            )
        {
            break;
        }
    }
}
