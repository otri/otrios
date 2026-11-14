// SPDX-License-Identifier: Apache-2.0
//! Collection-owned transport adapters. Core owns profiles, journals and windows.
use super::*;
use otrios_core::{canvas::chat as room, collection as col};
use otrios_interfaces::{chat as wire, spaces::Operation};
use std::sync::Mutex as StdMutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc};

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
struct Input {
    channel: u8,
    bytes: Vec<u8>,
    _credit: OwnedSemaphorePermit,
}
pub(super) struct Peer {
    pub active: apps::Activation,
    input: mpsc::Sender<Input>,
    credit: Arc<Semaphore>,
    pub output: mpsc::Receiver<Packet>,
    worker: tokio::task::JoinHandle<()>,
}
impl Peer {
    pub(super) fn send(&self, channel: u8, bytes: Vec<u8>) -> Result<(), ()> {
        let credit = self
            .credit
            .clone()
            .try_acquire_many_owned(bytes.len().try_into().map_err(|_| ())?)
            .map_err(|_| ())?;
        self.input
            .try_send(Input {
                channel,
                bytes,
                _credit: credit,
            })
            .map_err(|_| ())
    }
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
    let credit = Arc::new(Semaphore::new(
        wire::MAX_COMMAND_BYTES + 8 * col::MAX_COMMAND_BYTES,
    ));
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
        credit,
        output,
        worker,
    }
}
enum Work {
    Input(Option<Input>),
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
    mut input: mpsc::Receiver<Input>,
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
        let mut guard = app.apps.lock().await;
        let scope = spaces::Scope {
            space: space.clone(),
            identity: spaces::identity(&app, &key).await,
        };
        let Some(authority) = guard.as_mut().filter(|hub| {
            live.has_changed().is_ok()
                && live.borrow().as_ref() == Some(&active)
                && hub.grant_current(&scope, &active.binding, &active.grant)
        }) else {
            break;
        };
        let contribute = authority.allowed(&scope, &active.binding, Operation::Edit)
            || authority.allowed(&scope, &active.binding, Operation::Comment);
        let moderate = authority.allowed(&scope, &active.binding, Operation::Moderate);
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
                    let nonce = if let wire::Command::Send { nonce, .. }
                    | wire::Command::SendAsset { nonce, .. }
                    | wire::Command::Publish { nonce, .. }
                    | wire::Command::Redact { nonce, .. }
                    | wire::Command::ReadAsset { nonce, .. } = &command
                    {
                        Some(nonce.clone())
                    } else {
                        None
                    };
                    let permitted = authority.chat_allowed(&scope, &active.binding, &command);
                    if permitted
                        && let wire::Command::ReadAsset {
                            nonce: request_nonce,
                            sequence,
                            revision,
                            index,
                        } = command
                    {
                        // Large private payloads go directly to this bounded peer
                        // queue, never the room-wide ephemeral broadcast history.
                        let result =
                            if !otrios_core::canvas::chat::valid_request_nonce(&request_nonce) {
                                Err("Invalid attachment request nonce".into())
                            } else {
                                authority.owned_assets(&active.binding).and_then(|assets| {
                                    hub.chat.run_inline(|room| {
                                        room.read_attachment(sequence, revision, index, &assets)
                                    })
                                })
                            };
                        let event = match result {
                            Ok((asset, bytes)) => wire::Event::Asset {
                                nonce: request_nonce.clone(),
                                sequence,
                                revision,
                                asset,
                                bytes,
                            },
                            Err(_) => wire::Event::Error {
                                message: "Dispatch attachment is unavailable".into(),
                                nonce: Some(request_nonce),
                            },
                        };
                        let Ok(bytes) = room::encode_event(&event) else {
                            break;
                        };
                        Some(Packet {
                            channel: wire::CHANNEL,
                            bytes,
                        })
                    } else {
                        let identity = scope.identity.clone();
                        let result = if !permitted {
                            Err("This Space does not permit that chat action".into())
                        } else if identity.is_none()
                            && matches!(command, wire::Command::Join { credential: None })
                            && !hub.chat.admit_guest()
                        {
                            Err("Guest provisioning rate limit".into())
                        } else if matches!(command, wire::Command::Publish { .. }) {
                            authority
                                .publish_chat(&scope, &active, &hub, &command, time)
                                .map(|sequence| {
                                    vec![room::Delivery {
                                        target: Some(id),
                                        event: wire::Event::Published {
                                            nonce: nonce.clone().expect("publication nonce"),
                                            sequence,
                                        },
                                    }]
                                })
                        } else if matches!(command, wire::Command::SendAsset { .. }) {
                            authority.owned_assets(&active.binding).and_then(|assets| {
                                hub.chat.run_inline(|room| {
                                    let mut events = room.set_admin(id, moderate);
                                    events
                                        .extend(room.send_attachment(id, command, &assets, time)?);
                                    Ok(events)
                                })
                            })
                        } else {
                            hub.chat.run_inline(move |room| {
                                if matches!(command, wire::Command::Leave) {
                                    return Ok(room.abandon(id, time));
                                }
                                let mut events = room.set_admin(id, moderate);
                                events.extend(match command {
                                    wire::Command::Join { credential: None } => {
                                        if let Some(identity) = identity.as_deref() {
                                            room.join_admitted(id, identity, moderate, time)?
                                        } else {
                                            // The current Space Edit/Comment grant admitted
                                            // this temporary, non-moderating guest already.
                                            room.join(id, None, false, time, false)?
                                        }
                                    }
                                    wire::Command::Join { .. }
                                    | wire::Command::Authenticate { .. }
                                    | wire::Command::SignOut => {
                                        return Err("Use the host's Space identity controls".into());
                                    }
                                    command => room.handle(id, command, time)?,
                                });
                                Ok(events)
                            })
                        };
                        match result {
                            Ok(events) => hub.chat.publish(events),
                            Err(message) => hub.chat.publish(vec![room::Delivery {
                                target: Some(id),
                                event: wire::Event::Error { message, nonce },
                            }]),
                        }
                        None
                    }
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
                if packet.target.is_some_and(|target| target != id) {
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
                if contribute {
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
