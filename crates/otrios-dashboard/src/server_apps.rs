// SPDX-License-Identifier: Apache-2.0
//! One authority, independently persisted app roots and private admission before payloads.
use super::*;
use otrios_core::canvas::{self, Behavior, Document, Session};
use otrios_interfaces::canvas::{App as AppContract, ClientMessage, ServerMessage};
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
};

#[derive(Clone)]
struct Package {
    app: AppContract,
    ui: Vec<u8>,
    ui_hash: Vec<u8>,
    code: Vec<u8>,
    private: bool,
    signal_fields: Vec<u16>,
}
struct Bundle {
    packages: BTreeMap<String, Package>,
}
struct Room {
    doc: Document,
    file: Option<PathBuf>,
    failed: bool,
}
pub(super) struct Hub {
    bundles: BTreeMap<Vec<u8>, Bundle>,
    order: VecDeque<Vec<u8>>,
    rooms: BTreeMap<String, Room>,
    state: Option<PathBuf>,
    source: PathBuf,
    view: Vec<u8>,
    view_id: Vec<u8>,
    active: Vec<u8>,
    active_release: String,
}
struct Lease {
    epoch: u32,
    state_ref: String,
    private: bool,
    sync: Session,
    view_id: Vec<u8>,
    binary: bool,
}
#[derive(Default)]
struct Leases {
    active: Option<Lease>,
    staged: Option<Lease>,
    last_epoch: u32,
}
impl Leases {
    fn revoke_private(&mut self, allowed: bool) -> Vec<ServerMessage> {
        if allowed {
            return vec![];
        }
        let mut messages = vec![];
        for slot in [&mut self.active, &mut self.staged] {
            if slot.as_ref().is_some_and(|l| l.private) {
                let l = slot.take().expect("present lease");
                messages.push(ServerMessage::AccessRequired { epoch: l.epoch });
            }
        }
        messages
    }
}
type Result<T> = std::result::Result<T, String>;
fn bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    std::io::Read::take(file, max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("App artifact exceeds budget".into());
    }
    Ok(bytes)
}
impl Hub {
    pub(super) fn new(source: PathBuf, state: Option<PathBuf>) -> Result<Self> {
        if let Some(path) = &state {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
            fs::File::open(path)
                .and_then(|d| d.sync_all())
                .map_err(|e| e.to_string())?;
            if let Some(parent) = path.parent() {
                fs::File::open(parent)
                    .and_then(|d| d.sync_all())
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(Self {
            bundles: BTreeMap::new(),
            order: VecDeque::new(),
            rooms: BTreeMap::new(),
            state,
            source,
            view: vec![],
            view_id: vec![],
            active: vec![],
            active_release: String::new(),
        })
    }
    pub(super) fn refresh(&mut self, releases: &Option<PathBuf>, release: &Value) -> Result<()> {
        let id = release["id"].as_str().unwrap_or("development");
        if valid_id(id) && self.active_release == id {
            return Ok(());
        }
        let (catalog_bytes, folder, manifest) =
            if let Some(root) = releases.as_ref().filter(|_| valid_id(id)) {
                let folder = root.join(id);
                let raw = bounded(&folder.join("manifest.json"), 65536)?;
                if canvas::hex(&canvas::hash(&raw)) != id {
                    return Err("Installed release manifest digest mismatch".into());
                }
                let manifest: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
                if manifest["format"] != "otrios-release-v1"
                    || manifest["state_schema"] != "otrios-canvas-merkle-state-v1"
                    || manifest["sync_protocol"] != "otrios-canvas-sync-v2"
                {
                    return Err("Installed release app protocol mismatch".into());
                }
                let catalog = bounded(&folder.join("apps/catalog.flex"), canvas::MAX_BYTES)?;
                verify_asset(&manifest, "apps/catalog.flex", &catalog)?;
                (catalog, folder.join("authority/apps"), Some(manifest))
            } else {
                (
                    bounded(&self.source.join("catalog.flex"), canvas::MAX_BYTES)?,
                    self.source.clone(),
                    None,
                )
            };
        let catalog_hash = canvas::hash(&catalog_bytes);
        if self.bundles.contains_key(&catalog_hash) {
            self.active = catalog_hash;
            self.active_release = id.into();
            return Ok(());
        }
        let catalog = canvas::decode_catalog(&catalog_bytes)?;
        if catalog.format != "otrios-app-catalog-v1"
            || catalog.apps.is_empty()
            || catalog.apps.len() > 16
        {
            return Err("App catalog format/count invalid".into());
        }
        let mut packages = BTreeMap::new();
        let mut state_refs = std::collections::BTreeSet::new();
        let mut app_ids = std::collections::BTreeSet::new();
        let mut new_rooms = BTreeMap::new();
        for entry in catalog.apps {
            let a = entry.app;
            if a.id.is_empty()
                || a.id.len() > 64
                || !a
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !app_ids.insert(a.id.clone())
                || !state_refs.insert(a.state_ref.clone())
            {
                return Err("Invalid/duplicate app identity or State root".into());
            }
            let base = if manifest.is_some() {
                folder.join(&a.id)
            } else {
                folder.join(&a.id).join("package")
            };
            let ui = bounded(&base.join("ui.flex"), canvas::MAX_BYTES)?;
            if entry.ui_hash != canvas::hash(&ui) {
                return Err("Catalog UI digest mismatch".into());
            }
            if let Some(m) = &manifest {
                verify_asset(m, &format!("authority/apps/{}/ui.flex", a.id), &ui)?;
            }
            let description = canvas::decode_ui(&ui)?;
            if description.ui_ref != a.ui_ref || description.format != "otrios-canvas-v2" {
                return Err("UI reference/format differs from admitted catalog".into());
            }
            canvas::validate_schema(&description.state_schema)?;
            let signal_fields = description
                .selectors
                .iter()
                .filter_map(|selector| {
                    matches!(
                        selector.projection,
                        otrios_interfaces::canvas::Projection::WindowSignal { .. }
                    )
                    .then_some(selector.field)
                })
                .collect::<Vec<_>>();
            let code = if let Some(reference) = &description.behavior {
                let bytes = bounded(&base.join("behavior.wasm"), 65536)?;
                if let Some(m) = &manifest {
                    verify_asset(m, &format!("authority/apps/{}/behavior.wasm", a.id), &bytes)?;
                }
                Behavior::load(&bytes, reference)?;
                bytes
            } else {
                vec![]
            };
            if let Some(room) = self.rooms.get(&a.state_ref) {
                room.doc.clone().set_signal_fields(&signal_fields)?;
                if room.doc.schema() != &description.state_schema {
                    return Err("App schema requires a new State reference".into());
                }
            } else {
                if self.rooms.len() + new_rooms.len() >= 128 {
                    return Err("App root budget exhausted".into());
                }
                let file = self.state.as_ref().map(|p| {
                    p.join(format!(
                        "{}-{}.merkle",
                        a.id,
                        canvas::hex(&canvas::hash(a.state_ref.as_bytes()))
                    ))
                });
                let mut doc = if let Some(path) = file.as_ref().filter(|p| p.exists()) {
                    store::load(
                        path,
                        Document::load(
                            description.state_schema.clone(),
                            &a.state_ref,
                            &bounded(path, canvas::MAX_BYTES)?,
                        )?,
                    )?
                } else if let Some(old) = file
                    .as_ref()
                    .map(|p| p.with_extension("automerge"))
                    .filter(|p| p.exists())
                {
                    Document::import_legacy(
                        description.state_schema,
                        &a.state_ref,
                        &bounded(&old, canvas::MAX_BYTES)?,
                    )?
                } else {
                    Document::new(description.state_schema, &a.state_ref)?
                };
                doc.set_signal_fields(&signal_fields)?;
                if let Some(path) = &file
                    && !path.exists()
                {
                    persist(path, &doc.save()?).map_err(|e| e.to_string())?;
                }
                new_rooms.insert(
                    a.state_ref.clone(),
                    Room {
                        doc,
                        file,
                        failed: false,
                    },
                );
            }
            let private = entry.private || a.id == "monitoring";
            if packages
                .insert(
                    a.ui_ref.clone(),
                    Package {
                        app: a,
                        ui,
                        ui_hash: entry.ui_hash,
                        code,
                        private,
                        signal_fields,
                    },
                )
                .is_some()
            {
                return Err("Duplicate UI reference".into());
            }
        }
        self.rooms.extend(new_rooms);
        self.bundles
            .insert(catalog_hash.clone(), Bundle { packages });
        self.order.push_back(catalog_hash.clone());
        while self.order.len() > 8 {
            if let Some(old) = self.order.pop_front() {
                self.bundles.remove(&old);
            }
        }
        // Older admitted bundles can still have live leases. Keep their signal
        // dependencies until that bundle is retired; unrelated apps retain none.
        let mut retained = BTreeMap::<String, std::collections::BTreeSet<u16>>::new();
        for package in self.bundles.values().flat_map(|b| b.packages.values()) {
            retained
                .entry(package.app.state_ref.clone())
                .or_default()
                .extend(&package.signal_fields);
        }
        for (state_ref, room) in &mut self.rooms {
            let fields = retained
                .get(state_ref)
                .map(|ids| ids.iter().copied().collect::<Vec<_>>())
                .unwrap_or_default();
            room.doc.set_signal_fields(&fields)?;
        }
        self.active = catalog_hash;
        self.active_release = id.into();
        Ok(())
    }
    pub(super) fn catalog_bytes(&self) -> Result<Vec<u8>> {
        bounded(&self.source.join("catalog.flex"), canvas::MAX_BYTES)
    }
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.bundles.len(), self.rooms.len())
    }
    pub(super) fn view(&self) -> &[u8] {
        &self.view
    }
    pub(super) fn set_view(&mut self, bytes: Vec<u8>) -> Result<bool> {
        if !bytes.is_empty() {
            let layout = otrios_core::dashboard::layout::Layout::from_bytes(&bytes)?;
            layout.resolve_slots(&BTreeMap::new())?;
        }
        if self.view == bytes {
            return Ok(false);
        }
        self.view_id = canvas::hash(&bytes);
        self.view = bytes;
        Ok(true)
    }
    fn package(&self, hash: &[u8], ui_ref: &str) -> Result<Package> {
        self.bundles
            .get(hash)
            .and_then(|b| b.packages.get(ui_ref))
            .cloned()
            .ok_or_else(|| "App is not in an admitted catalog".into())
    }
    fn process(
        &mut self,
        leases: &mut Leases,
        message: ClientMessage,
        allow_private: bool,
    ) -> Result<(Vec<ServerMessage>, bool)> {
        let mut messages = leases.revoke_private(allow_private);
        let mut changed = false;
        match message {
            ClientMessage::Mount {
                epoch,
                ui_ref,
                state_ref,
                catalog_hash,
                known_code,
                writer,
                resume,
                binary,
            } => {
                if epoch == 0
                    || epoch <= leases.last_epoch
                    || known_code.len() > 8
                    || known_code.iter().any(|h| h.len() != 32)
                {
                    return Err("Invalid Canvas mount generation/cache".into());
                }
                leases.last_epoch = epoch;
                leases.staged = None;
                let p = self.package(&catalog_hash, &ui_ref)?;
                if p.private && !allow_private {
                    messages.push(ServerMessage::AccessRequired { epoch });
                    return Ok((messages, false));
                }
                if state_ref != p.app.state_ref {
                    return Err("State root is not granted by app catalog".into());
                }
                let room = self
                    .rooms
                    .get_mut(&state_ref)
                    .ok_or("Missing app State root")?;
                if room.failed {
                    return Err("App persistence outcome uncertain; restart required".into());
                }
                let mut candidate = room.doc.clone();
                candidate.set_wall_time(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                );
                let (mut sync, changed) = candidate.open_session(&writer, resume)?;
                sync.set_binary_transport(binary);
                if changed
                    && let Some(file) = &room.file
                    && let Err(e) = store::append(file, &room.doc, &candidate)
                {
                    room.failed = true;
                    return Err(e.to_string());
                }
                room.doc = candidate;
                let state = room
                    .doc
                    .generate(&mut sync)?
                    .ok_or("Missing initial Merkle state")?;
                messages.push(ServerMessage::Mounted {
                    epoch,
                    ui: p.ui,
                    ui_hash: p.ui_hash,
                    code: if known_code.contains(&canvas::hash(&p.code)) {
                        vec![]
                    } else {
                        p.code
                    },
                    state,
                });
                leases.staged = Some(Lease {
                    epoch,
                    state_ref,
                    private: p.private,
                    sync,
                    view_id: vec![],
                    binary,
                });
                // The previous activation/session remains authoritative until the client
                // has qualified every candidate dependency and explicitly accepts it.
                return Ok((messages, false));
            }
            ClientMessage::AcceptMount { epoch } => {
                if leases.staged.as_ref().is_none_or(|l| l.epoch != epoch) {
                    if messages
                        .iter()
                        .any(|m| matches!(m,ServerMessage::AccessRequired{epoch:e} if *e==epoch))
                    {
                        return Ok((messages, false));
                    }
                    if epoch <= leases.last_epoch {
                        return Ok((messages, false));
                    }
                    return Err("No matching staged Canvas mount".into());
                }
                leases.active = leases.staged.take();
            }
            ClientMessage::CancelMount { epoch } => {
                if leases.staged.as_ref().is_some_and(|l| l.epoch == epoch) {
                    leases.staged = None;
                }
                return Ok((messages, false));
            }
            ClientMessage::Sync { epoch, bytes } => {
                let Some(l) = leases.active.as_mut() else {
                    if !messages.is_empty() {
                        return Ok((messages, false));
                    }
                    return Err("Sync without accepted Canvas mount".into());
                };
                if l.epoch != epoch {
                    return Ok((messages, false));
                }
                let room = self
                    .rooms
                    .get_mut(&l.state_ref)
                    .ok_or("Missing app State root")?;
                if room.failed {
                    return Err("App persistence outcome uncertain; restart required".into());
                }
                let mut candidate = room.doc.clone();
                candidate.set_wall_time(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                );
                let mut session = l.sync.clone();
                changed = candidate.receive(&mut session, &bytes)?;
                if changed
                    && let Some(file) = &room.file
                    && let Err(e) = store::append(file, &room.doc, &candidate)
                {
                    room.failed = true;
                    return Err(e.to_string());
                }
                room.doc = candidate;
                l.sync = session;
            }
            ClientMessage::Leave { epoch } => {
                if leases.active.as_ref().is_some_and(|l| l.epoch == epoch) {
                    leases.active = None;
                }
                if leases.staged.as_ref().is_some_and(|l| l.epoch == epoch) {
                    leases.staged = None;
                }
                return Ok((messages, false));
            }
        }
        messages.extend(self.updates(leases, allow_private)?);
        Ok((messages, changed))
    }
    fn updates(&mut self, leases: &mut Leases, allow_private: bool) -> Result<Vec<ServerMessage>> {
        let mut messages = leases.revoke_private(allow_private);
        let Some(l) = &mut leases.active else {
            return Ok(messages);
        };
        let room = self
            .rooms
            .get_mut(&l.state_ref)
            .ok_or("Missing app State root")?;
        if room.failed {
            return Err("App persistence outcome uncertain; restart required".into());
        }
        if let Some(bytes) = room.doc.generate(&mut l.sync)? {
            messages.push(ServerMessage::Sync {
                epoch: l.epoch,
                bytes,
            });
        }
        if l.private && l.state_ref == "app:monitoring/state" && l.view_id != self.view_id {
            messages.push(ServerMessage::View {
                epoch: l.epoch,
                bytes: self.view.clone(),
            });
            l.view_id = self.view_id.clone();
        }
        Ok(messages)
    }
}
fn verify_asset(manifest: &Value, path: &str, bytes: &[u8]) -> Result<()> {
    let item = if path.starts_with("authority/") {
        &manifest["authority"][path]
    } else {
        &manifest["assets"][path]
    };
    if item["bytes"].as_u64() != Some(bytes.len() as u64)
        || item["sha256"].as_str() != Some(canvas::hex(&canvas::hash(bytes)).as_str())
    {
        return Err(format!("Release app asset digest mismatch: {path}"));
    }
    Ok(())
}

pub(super) async fn trusted(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    open(app, headers, upgrade, false).await
}
pub(super) async fn public(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    open(app, headers, upgrade, true).await
}
async fn open(
    app: Arc<App>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
    public: bool,
) -> Response {
    let allowed = if public {
        headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|o| app.origins.iter().any(|a| a == o))
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
        return failure(StatusCode::SERVICE_UNAVAILABLE, "Too many screens");
    };
    let key = if public {
        match &app.auth {
            Some(a) => a.key(&headers).await,
            None => None,
        }
    } else {
        None
    };
    upgrade
        .max_message_size(canvas::MAX_BYTES)
        .max_frame_size(canvas::MAX_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            peer(app, socket, public, key).await
        })
        .into_response()
}
async fn private_allowed(app: &App, public: bool, key: &Option<String>) -> bool {
    if !public {
        return true;
    }
    match (&app.auth, key) {
        (Some(a), Some(k)) => a.valid(k).await,
        _ => false,
    }
}
async fn send(
    output: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: ServerMessage,
    binary: bool,
) -> bool {
    let encode = if binary {
        canvas::encode_server_binary
    } else {
        canvas::encode_server
    };
    let Ok(bytes) = encode(&message) else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            output.send(Message::Binary(bytes.into()))
        )
        .await,
        Ok(Ok(()))
    )
}
async fn peer(app: Arc<App>, socket: WebSocket, public: bool, key: Option<String>) {
    let (mut output, mut input) = socket.split();
    let mut leases = Leases::default();
    let mut notice = app.changed.subscribe();
    let mut expiry = tokio::time::interval(Duration::from_secs(1));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    let mut window = std::time::Instant::now();
    let mut count = 0;
    let mut release_id = String::new();
    loop {
        let release = app.release.read().await.clone();
        let id = release["id"].as_str().unwrap_or("development");
        if release_id != id {
            let message = Message::Text(
                json!({"type":"release","release":release})
                    .to_string()
                    .into(),
            );
            if !matches!(
                tokio::time::timeout(Duration::from_secs(2), output.send(message)).await,
                Ok(Ok(()))
            ) {
                break;
            }
            release_id = id.to_string();
        }
        let incoming = tokio::select! {
            m=input.next()=>match m {Some(Ok(Message::Binary(bytes)))=>Some(bytes),Some(Ok(Message::Ping(bytes)))=>{if output.send(Message::Pong(bytes)).await.is_err(){break;}continue;},Some(Ok(Message::Pong(_)))=>continue,_=>break},
            c=notice.changed()=>{if c.is_err(){break;}None},
            _=expiry.tick()=>None,
            _=heartbeat.tick()=>{if !matches!(tokio::time::timeout(Duration::from_secs(2),output.send(Message::Ping(Bytes::new()))).await,Ok(Ok(()))){break;}continue;},
        };
        let allowed = private_allowed(&app, public, &key).await;
        for revoked in leases.revoke_private(allowed) {
            if !send(&mut output, revoked, false).await {
                return;
            }
        }
        let mut recoverable_epoch = None;
        let result = if let Some(bytes) = incoming {
            if window.elapsed() >= Duration::from_secs(1) {
                window = std::time::Instant::now();
                count = 0;
            }
            count += 1;
            if count > 192 {
                break;
            }
            let release = app.release.read().await.clone();
            let mut hub = app.apps.lock().await;
            canvas::decode_client(&bytes).and_then(|message| {
                recoverable_epoch = match &message {
                    ClientMessage::Mount { epoch, .. }
                    | ClientMessage::AcceptMount { epoch }
                    | ClientMessage::CancelMount { epoch } => Some(*epoch),
                    _ => None,
                };
                if matches!(message, ClientMessage::Mount { .. }) {
                    hub.refresh(&app.releases, &release)?;
                }
                hub.process(&mut leases, message, allowed)
            })
        } else {
            app.apps
                .lock()
                .await
                .updates(&mut leases, allowed)
                .map(|m| (m, false))
        };
        match result {
            Ok((messages, changed)) => {
                if changed {
                    announce(&app).await;
                }
                let private_epochs = [leases.active.as_ref(), leases.staged.as_ref()]
                    .into_iter()
                    .flatten()
                    .filter(|l| l.private)
                    .map(|l| l.epoch)
                    .collect::<Vec<_>>();
                for message in messages {
                    let epoch = match &message {
                        ServerMessage::Mounted { epoch, .. }
                        | ServerMessage::Sync { epoch, .. }
                        | ServerMessage::View { epoch, .. }
                        | ServerMessage::Snapshot { epoch, .. }
                        | ServerMessage::AccessRequired { epoch }
                        | ServerMessage::Error { epoch, .. } => *epoch,
                    };
                    if private_epochs.contains(&epoch) && !private_allowed(&app, public, &key).await
                    {
                        for revoked in leases.revoke_private(false) {
                            if !send(&mut output, revoked, false).await {
                                return;
                            }
                        }
                        continue;
                    }
                    if !send(
                        &mut output,
                        message,
                        [leases.active.as_ref(), leases.staged.as_ref()]
                            .into_iter()
                            .flatten()
                            .any(|l| l.epoch == epoch && l.binary),
                    )
                    .await
                    {
                        return;
                    }
                }
            }
            Err(message) => {
                let epoch = recoverable_epoch
                    .unwrap_or_else(|| leases.active.as_ref().map_or(1, |l| l.epoch));
                if !send(&mut output, ServerMessage::Error { epoch, message }, false).await {
                    return;
                }
                if recoverable_epoch.is_none() {
                    break;
                }
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), output.send(Message::Close(None))).await;
}
