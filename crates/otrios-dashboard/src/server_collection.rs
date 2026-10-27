// SPDX-License-Identifier: Apache-2.0
//! Thin authenticated socket/mailbox adapter for the covered collection lane.
use super::*;
use otrios_core::collection as core;
use std::{
    collections::VecDeque,
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::mpsc;

pub(super) struct Hub {
    pub core: Arc<StdMutex<core::Channel>>,
    changed: watch::Sender<u64>,
}
pub(super) struct Packet {
    pub bytes: Vec<u8>,
    pub required: core::Access,
    pub revoked: Vec<u8>,
}
pub(super) struct Peer {
    pub input: mpsc::Sender<Vec<u8>>,
    pub output: mpsc::Receiver<Packet>,
    worker: tokio::task::JoinHandle<()>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
pub(super) fn subscribe(
    app: Arc<App>,
    public: bool,
    key: Option<String>,
    admission: Arc<AtomicBool>,
) -> Peer {
    let (input, receiver) = mpsc::channel(8);
    let (sender, output) = mpsc::channel(1);
    let worker = tokio::spawn(worker(app, public, key, admission, receiver, sender));
    Peer {
        input,
        output,
        worker,
    }
}
impl Hub {
    pub fn new() -> Result<Arc<Self>, String> {
        let mut epoch = [0; 8];
        getrandom::fill(&mut epoch).map_err(|e| e.to_string())?;
        let (changed, _) = watch::channel(0_u64);
        let notify = changed.clone();
        let mut core = core::Channel::new(u64::from_le_bytes(epoch).max(1));
        core.set_notifier(move || notify.send_modify(|version| *version = version.wrapping_add(1)));
        Ok(Arc::new(Self {
            core: Arc::new(StdMutex::new(core)),
            changed,
        }))
    }
    pub fn fixture(self: &Arc<Self>, path: PathBuf) {
        let hub = self.clone();
        tokio::spawn(async move {
            let fixture = Arc::new(StdMutex::new(core::Synthetic::default()));
            let mut clock = tokio::time::interval(Duration::from_millis(100));
            loop {
                clock.tick().await;
                let channel = hub.core.clone();
                let fixture = fixture.clone();
                let path = path.clone();
                let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    fs::File::open(path)
                        .map_err(|e| e.to_string())?
                        .take(1025)
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    if bytes.len() > 1024 {
                        return Err("Collection fixture limit".into());
                    }
                    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    let records = value["records"]
                        .as_u64()
                        .and_then(|v| u32::try_from(v).ok())
                        .ok_or("Fixture record count")?;
                    let retain = value["retain_from"]
                        .as_u64()
                        .ok_or("Fixture retention key")?;
                    fixture
                        .lock()
                        .map_err(|_| "Fixture lock")?
                        .update(
                            &mut *channel.lock().map_err(|_| "Collection lock")?,
                            records,
                            retain,
                        )
                        .map_err(|e| e.to_string())
                })
                .await;
                if let Ok(Err(error)) = result {
                    eprintln!("Collection fixture: {error}");
                }
            }
        });
    }
    async fn step(
        &self,
        session: Arc<StdMutex<core::ChannelSession>>,
        access: core::Access,
        command: Option<core::Command>,
        poll_data: bool,
    ) -> Result<Option<Packet>, String> {
        let channel = self.core.clone();
        tokio::task::spawn_blocking(move || {
            let mut channel = channel.lock().map_err(|_| "Collection lock")?;
            let mut session = session.lock().map_err(|_| "Collection session lock")?;
            let event = if let Some(command) = command {
                channel.handle(&mut session, access, command)
            } else if poll_data {
                channel.poll(&mut session, access)
            } else {
                channel.authorize(&mut session, access)
            };
            event
                .map(|event| {
                    let required = if matches!(event, core::Event::View(_)) {
                        channel.required_access(&session, access)
                    } else {
                        core::Access::default()
                    };
                    let revoked = if let core::Event::View(offer) = &event {
                        core::Event::Revoked {
                            epoch: offer.epoch,
                            generation: offer.generation,
                            collection: session.collection().ok_or("Collection scope")?,
                        }
                        .to_bytes()
                        .map_err(|e| e.to_string())?
                    } else {
                        Vec::new()
                    };
                    Ok(Packet {
                        bytes: event.to_bytes().map_err(|e| e.to_string())?,
                        required,
                        revoked,
                    })
                })
                .transpose()
        })
        .await
        .map_err(|_| "Collection task failed".to_string())?
    }
}

pub(super) async fn worker(
    app: Arc<App>,
    public: bool,
    key: Option<String>,
    admission: Arc<AtomicBool>,
    mut input: mpsc::Receiver<Vec<u8>>,
    output: mpsc::Sender<Packet>,
) {
    let session = Arc::new(StdMutex::new(core::ChannelSession::default()));
    let mut notice = app.collections.changed.subscribe();
    let mut clock = tokio::time::interval(Duration::from_millis(100));
    let mut pending = VecDeque::new();
    let mut window = std::time::Instant::now();
    let mut count = 0_u16;
    loop {
        let (incoming, poll_data) = if let Some(command) = pending.pop_front() {
            (Some(command), true)
        } else {
            tokio::select! {
                bytes = input.recv() => match bytes { Some(bytes) => (Some(bytes), true), None => return },
                result = notice.changed() => { if result.is_err() { return; } (None, true) },
                _ = clock.tick() => (None, false),
            }
        };
        let access = core::Access {
            authenticated: apps::private_allowed(&app, public, &key).await,
            admitted: admission.load(Ordering::Acquire),
        };
        let command = if let Some(bytes) = incoming {
            if window.elapsed() >= Duration::from_secs(1) {
                window = std::time::Instant::now();
                count = 0;
            }
            count += 1;
            if count > 192 {
                return;
            }
            let mut command = match core::decode_command(&bytes) {
                Ok(command) => command,
                Err(_) => {
                    let bytes = core::Event::Rejected {
                        generation: 0,
                        status: core::Status::Invalid,
                    }
                    .to_bytes()
                    .expect("bounded status");
                    if output
                        .send(Packet {
                            bytes,
                            required: core::Access::default(),
                            revoked: vec![],
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    continue;
                }
            };
            // Adjacent queued selections supersede older selections before any
            // record transfer is assembled. Preserve ACK/cancel command order.
            if matches!(command, core::Command::Select { .. }) {
                for _ in 0..8 {
                    let Ok(bytes) = input.try_recv() else {
                        break;
                    };
                    count += 1;
                    if count > 192 {
                        return;
                    }
                    if let Ok(next @ core::Command::Select { .. }) = core::decode_command(&bytes) {
                        if next.generation() > command.generation() {
                            command = next;
                        }
                    } else {
                        pending.push_back(bytes);
                        break;
                    }
                }
            }
            Some(command)
        } else {
            None
        };
        match app
            .collections
            .step(session.clone(), access, command, poll_data)
            .await
        {
            Ok(Some(packet)) => {
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(2), output.send(packet)).await,
                    Ok(Ok(()))
                ) {
                    return;
                }
            }
            Ok(None) => (),
            Err(error) => {
                eprintln!("Collection session: {error}");
                return;
            }
        }
    }
}
