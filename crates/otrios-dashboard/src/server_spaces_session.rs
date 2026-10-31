// SPDX-License-Identifier: Apache-2.0
//! Shared-session adapter for the covered Space management/discovery contract.
use super::*;
use tokio::sync::mpsc;

pub(crate) struct Packet {
    pub bytes: Vec<u8>,
    pub identity: Option<String>,
    pub cursor: Option<contract::DiscoveryCursor>,
}
pub(crate) struct Peer {
    pub input: mpsc::Sender<Vec<u8>>,
    pub output: mpsc::Receiver<Packet>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub(crate) fn subscribe(app: Arc<App>, key: Option<String>) -> Peer {
    let (input, commands) = mpsc::channel(4);
    let (events, output) = mpsc::channel(1);
    let task = tokio::spawn(worker(app, key, commands, events));
    Peer {
        input,
        output,
        task,
    }
}
async fn emit(
    output: &mpsc::Sender<Packet>,
    event: contract::Event,
    principal: Option<String>,
) -> bool {
    let cursor = match &event {
        contract::Event::Snapshot { snapshot, .. } => Some(snapshot.cursor.clone()),
        _ => None,
    };
    let Ok(bytes) = core::encode_event(&event) else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            output.send(Packet {
                bytes,
                identity: principal,
                cursor
            })
        )
        .await,
        Ok(Ok(()))
    )
}
async fn worker(
    app: Arc<App>,
    key: Option<String>,
    mut input: mpsc::Receiver<Vec<u8>>,
    output: mpsc::Sender<Packet>,
) {
    // Register observation before capturing any snapshot. Mutations and catalog
    // publication update the cursor under the same lock used for capture.
    let mut notice = app.changed.subscribe();
    let mut expiry = tokio::time::interval(Duration::from_secs(1));
    let mut selected: Option<Option<String>> = None;
    let mut last: Option<(contract::DiscoveryCursor, Option<String>)> = None;
    let mut window = std::time::Instant::now();
    let mut commands = 0_u16;
    loop {
        let incoming = tokio::select! {
            value=input.recv()=>match value { Some(value)=>Some(value),None=>return },
            changed=notice.changed()=>{ if changed.is_err() { return; } None },
            _=expiry.tick()=>None,
        };
        let principal = identity(&app, &key).await;
        let mut force = false;
        let mut resync_requested = false;
        let mut since = None;
        if let Some(bytes) = incoming {
            if window.elapsed() >= Duration::from_secs(1) {
                window = std::time::Instant::now();
                commands = 0;
            }
            commands += 1;
            if commands > 16 {
                return;
            }
            let Ok(command) = core::decode_command(&bytes) else {
                return;
            };
            match command {
                contract::Command::Watch {
                    selected: wanted,
                    cursor,
                    ..
                } => {
                    selected = Some(wanted);
                    force = true;
                    resync_requested = true;
                    since = cursor;
                }
                contract::Command::Manage { id, request, .. } => {
                    let result = {
                        let mut guard = app.apps.lock().await;
                        let current = identity(&app, &key).await;
                        match (guard.as_mut(), current.as_deref()) {
                            (Some(hub), Some(actor)) => {
                                manage_request(hub, actor, &request).map(|_| {
                                    if matches!(request.change, Change::Create { .. }) {
                                        hub.registry()
                                            .ok()
                                            .and_then(|r| r.spaces.last().map(|s| s.id.clone()))
                                    } else {
                                        None
                                    }
                                })
                            }
                            (Some(_), None) => Err(core::Error::Denied),
                            _ => Err(core::Error::Unavailable),
                        }
                    };
                    if result.is_ok() {
                        announce(&app).await;
                    }
                    let (status, created) = match result {
                        Ok(created) => (contract::ManagementStatus::Applied, created),
                        Err(error) => (core::management_status(&error), None),
                    };
                    if !emit(
                        &output,
                        contract::Event::Result {
                            version: contract::VERSION,
                            id,
                            status,
                            created,
                        },
                        principal.clone(),
                    )
                    .await
                    {
                        return;
                    }
                    force = true;
                }
            }
        }
        let Some(wanted) = &selected else {
            continue;
        };
        let snapshot = {
            let guard = app.apps.lock().await;
            match guard.as_ref() {
                Some(hub)
                    if !force
                        && last.as_ref().is_some_and(|(cursor, identity)| {
                            cursor == hub.discovery_cursor() && identity == &principal
                        }) =>
                {
                    continue;
                }
                Some(hub) => hub.discovery(principal.as_deref(), wanted.as_deref()),
                None => Err(core::Error::Unavailable),
            }
        };
        let event = match snapshot {
            Ok(snapshot) => {
                let prior = since
                    .as_ref()
                    .or_else(|| last.as_ref().map(|(cursor, _)| cursor));
                let resync = resync_requested
                    || prior.is_none_or(|cursor| {
                        cursor.authority != snapshot.cursor.authority
                            || (cursor.revision != snapshot.cursor.revision
                                && cursor.revision.checked_add(1) != Some(snapshot.cursor.revision))
                    });
                last = Some((snapshot.cursor.clone(), principal.clone()));
                contract::Event::Snapshot { snapshot, resync }
            }
            Err(_) => {
                last = None;
                contract::Event::Unavailable {
                    version: contract::VERSION,
                }
            }
        };
        if !emit(&output, event, principal).await {
            return;
        }
    }
}
