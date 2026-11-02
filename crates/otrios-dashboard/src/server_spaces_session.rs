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
    let mut observation = 0;
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
                command @ (contract::Command::ImportApp { .. }
                | contract::Command::ImportStatus { .. }) => {
                    let (id, operation, commit) = match command {
                        contract::Command::ImportApp { id, stage, .. } => (id, stage, true),
                        contract::Command::ImportStatus { id, operation, .. } => {
                            (id, operation, false)
                        }
                        _ => unreachable!(),
                    };
                    let result = {
                        let mut guard = app.apps.lock().await;
                        let current = identity(&app, &key).await;
                        match (guard.as_mut(), current.as_deref(), app.auth.as_ref()) {
                            (Some(hub), Some(actor), Some(auth)) if commit => hub
                                .import_app_stage(actor, auth.host_policy(), &operation)
                                .map(Some),
                            (Some(hub), Some(actor), _) => hub.import_status(actor, &operation),
                            (Some(_), None, _) => Err(core::Error::Denied),
                            _ => Err(core::Error::Unavailable),
                        }
                    };
                    if commit && result.is_ok() {
                        announce(&app).await;
                    }
                    let (status, state) = result.map_or_else(
                        |e| (core::management_status(&e), None),
                        |s| (contract::ManagementStatus::Applied, s),
                    );
                    if !emit(
                        &output,
                        contract::Event::AppImport {
                            version: contract::VERSION,
                            id,
                            operation,
                            status,
                            state,
                        },
                        principal.clone(),
                    )
                    .await
                    {
                        return;
                    }
                    force = true;
                }
                command @ contract::Command::ShareApp { .. } => {
                    let contract::Command::ShareApp { id, .. } = &command else {
                        unreachable!()
                    };
                    let id = *id;
                    let result = {
                        let guard = app.apps.lock().await;
                        let current = identity(&app, &key).await;
                        match (
                            guard.as_ref(),
                            current.as_deref(),
                            app.auth.as_ref(),
                            app.releases.as_deref(),
                        ) {
                            (Some(hub), Some(actor), Some(auth), Some(releases)) => {
                                match hub.share_candidate(
                                    actor,
                                    auth.host_policy(),
                                    &command,
                                    releases,
                                ) {
                                    Ok(tree) => {
                                        match app.public_transfer_jobs.clone().try_acquire_owned() {
                                            Ok(credit) => super::super::transfers::publications(
                                                &app,
                                                move |store| {
                                                    let _credit = credit;
                                                    let summary = store.publish(tree)?;
                                                    Ok(otrios_interfaces::transfers::AppShare {
                                                        app: summary.info.app.id,
                                                        title: summary.info.app.title,
                                                        digest: summary.digest,
                                                    })
                                                },
                                            )
                                            .await
                                            .map_err(|_| core::Error::Unavailable),
                                            Err(_) => Err(core::Error::Capacity),
                                        }
                                    }
                                    Err(error) => Err(error),
                                }
                            }
                            (Some(_), None, ..) => Err(core::Error::Denied),
                            _ => Err(core::Error::Unavailable),
                        }
                    };
                    let (status, share) = match result {
                        Ok(share) => (contract::ManagementStatus::Applied, Some(share)),
                        Err(e) => (core::management_status(&e), None),
                    };
                    if !emit(
                        &output,
                        contract::Event::AppShared {
                            version: contract::VERSION,
                            id,
                            status,
                            share,
                        },
                        principal.clone(),
                    )
                    .await
                    {
                        return;
                    }
                }
                command @ (contract::Command::ReplaceData { .. }
                | contract::Command::CancelData { .. }
                | contract::Command::CancelApp { .. }) => {
                    let cancel_app = matches!(&command, contract::Command::CancelApp { .. });
                    let (id, stage, replace) = match command {
                        contract::Command::ReplaceData { id, stage, .. } => (id, stage, true),
                        contract::Command::CancelData { id, stage, .. } => (id, stage, false),
                        contract::Command::CancelApp { id, stage, .. } => (id, stage, false),
                        _ => unreachable!(),
                    };
                    let result = {
                        let mut guard = app.apps.lock().await;
                        let current = identity(&app, &key).await;
                        match (guard.as_mut(), current.as_deref()) {
                            (Some(hub), Some(actor)) if replace => {
                                hub.replace_data_stage(actor, &stage).map(|_| ())
                            }
                            (Some(hub), Some(actor)) if cancel_app => {
                                hub.cancel_app_stage(actor, &stage)
                            }
                            (Some(hub), Some(actor)) => hub.cancel_data_stage(actor, &stage),
                            (Some(_), None) => Err(core::Error::Denied),
                            _ => Err(core::Error::Unavailable),
                        }
                    };
                    if replace && result.is_ok() {
                        announce(&app).await;
                    }
                    let status = result.map_or_else(
                        |e| core::management_status(&e),
                        |_| contract::ManagementStatus::Applied,
                    );
                    if !emit(
                        &output,
                        contract::Event::Result {
                            version: contract::VERSION,
                            id,
                            status,
                            created: None,
                        },
                        principal.clone(),
                    )
                    .await
                    {
                        return;
                    }
                    force = true;
                }
                contract::Command::ReplacementStatus {
                    id,
                    space,
                    replacement,
                    ..
                } => {
                    let result = {
                        let guard = app.apps.lock().await;
                        let current = identity(&app, &key).await;
                        match (guard.as_ref(), current.as_deref()) {
                            (Some(hub), Some(actor)) => {
                                hub.replacement_outcome(actor, &space, &replacement)
                            }
                            (Some(_), None) => Err(core::Error::Denied),
                            _ => Err(core::Error::Unavailable),
                        }
                    };
                    let (status, outcome) = match result {
                        Ok(outcome) => (contract::ManagementStatus::Applied, Some(outcome)),
                        Err(e) => (core::management_status(&e), None),
                    };
                    if !emit(
                        &output,
                        contract::Event::ReplacementStatus {
                            version: contract::VERSION,
                            id,
                            replacement,
                            status,
                            outcome,
                        },
                        principal.clone(),
                    )
                    .await
                    {
                        return;
                    }
                }
                contract::Command::MoveStatus {
                    id,
                    operation,
                    source,
                    destination,
                    ..
                } => {
                    let result = {
                        let guard = app.apps.lock().await;
                        let current = identity(&app, &key).await;
                        match (guard.as_ref(), current.as_deref()) {
                            (Some(hub), Some(actor)) => {
                                hub.move_committed(actor, &source, &destination, &operation)
                            }
                            (Some(_), None) => Err(core::Error::Denied),
                            _ => Err(core::Error::Unavailable),
                        }
                    };
                    let (status, generation) = match result {
                        Ok(Some(g)) => (contract::ManagementStatus::Applied, Some(g)),
                        Ok(None) => (contract::ManagementStatus::Conflict, None),
                        Err(e) => (core::management_status(&e), None),
                    };
                    if !emit(
                        &output,
                        contract::Event::MoveStatus {
                            version: contract::VERSION,
                            id,
                            operation,
                            status,
                            generation,
                        },
                        principal.clone(),
                    )
                    .await
                    {
                        return;
                    }
                    force = true;
                }
                contract::Command::Watch {
                    observation: requested,
                    selected: wanted,
                    cursor,
                    ..
                } => {
                    selected = Some(wanted);
                    observation = requested;
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
            let mut guard = app.apps.lock().await;
            let presence_ready = guard
                .as_mut()
                .is_some_and(|hub| hub.refresh_presence().is_ok());
            match guard.as_ref() {
                _ if !presence_ready => Err(core::Error::Unavailable),
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
                contract::Event::Snapshot {
                    observation,
                    snapshot: Box::new(snapshot),
                    resync,
                }
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
