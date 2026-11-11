// SPDX-License-Identifier: Apache-2.0
//! One authority, independently persisted app roots and private admission before payloads.
use super::*;
use otrios_core::canvas::{self, Behavior, Document, Session};
use otrios_interfaces::canvas::{App as AppContract, ClientMessage, ServerMessage};
use otrios_interfaces::session::STATE_PROTOCOL;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
};
#[path = "server_app_transfers.rs"]
mod transfers;

#[derive(Clone)]
struct Package {
    bundle_sha256: Option<String>,
    library_locks: Vec<otrios_interfaces::oib::graph::LibraryLock>,
    app: AppContract,
    ui: Vec<u8>,
    ui_hash: Vec<u8>,
    code: Vec<u8>,
    private: bool,
    signal_fields: Vec<u16>,
    selection: canvas::StateSelection,
    schema: otrios_interfaces::canvas::StateSchema,
    policy: otrios_interfaces::spaces::AppPolicy,
    chat: bool,
    aggregate_view: bool,
    ink: bool,
    primary_ref: String,
    code_bytes: Option<u64>,
    revision: Option<u64>,
}
struct Bundle {
    packages: BTreeMap<String, Package>,
}
struct Room {
    doc: Document,
    file: Option<PathBuf>,
    failed: bool,
    lease: Arc<()>,
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
    publisher: Option<[u8; 32]>,
    spaces: Option<otrios_core::spaces::Store>,
    collector: Option<otrios_interfaces::spaces::ServiceBinding>,
    collection_views: BTreeMap<String, (Vec<u8>, Vec<u8>)>,
    provisioning: bool,
    discovery: otrios_interfaces::spaces::DiscoveryCursor,
    discovery_catalog: Vec<u8>,
    ink_rooms: BTreeMap<String, Arc<ink::Room>>,
    chat_rooms: BTreeMap<String, Arc<space_chat::Hub>>,
    cleanup: BTreeMap<String, otrios_interfaces::spaces::CleanupSummary>,
    next_maintenance: Option<std::time::Instant>,
    presence: Vec<std::sync::Weak<Presence>>,
    next_presence: u64,
    staged_data: Option<transfers::PendingData>,
    staged_app: Option<transfers::PendingApp>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Grant {
    principal: Option<String>,
    actions: u8,
}
struct Presence {
    scope: spaces::Scope,
    binding: otrios_interfaces::spaces::Binding,
    grant: Grant,
    anonymous: u64,
}
struct Lease {
    epoch: u32,
    state_ref: String,
    private: bool,
    sync: Session,
    view_id: Vec<u8>,
    binding: Option<otrios_interfaces::spaces::Binding>,
    grant: Grant,
    _room: Arc<()>,
    _chat: Option<Arc<space_chat::Hub>>,
    _presence: Option<Arc<Presence>>,
    ink: bool,
    chat: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Activation {
    pub epoch: u32,
    pub binding: otrios_interfaces::spaces::Binding,
    pub grant: Grant,
    pub room: String,
    pub ink: bool,
    pub chat: bool,
}
// Minimal pins survive revocation until the trusted host confirms disposal by
// sending Leave(epoch), or its connection closes. No old sync snapshot is kept.
struct Retired {
    epoch: u32,
    _room: Arc<()>,
    _chat: Option<Arc<space_chat::Hub>>,
}
#[derive(Default)]
struct Leases {
    active: Option<Lease>,
    staged: Option<Lease>,
    last_epoch: u32,
    retired: Vec<Retired>,
}
impl Leases {
    fn activation(&self) -> Option<Activation> {
        self.active.as_ref().and_then(|lease| {
            lease.binding.as_ref().map(|binding| Activation {
                epoch: lease.epoch,
                binding: binding.clone(),
                grant: lease.grant.clone(),
                room: lease.state_ref.clone(),
                ink: lease.ink,
                chat: lease.chat,
            })
        })
    }
    fn publish(&self, sender: &watch::Sender<Option<Activation>>) {
        let next = self.activation();
        sender.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
    }
    fn retire(&mut self, lease: Lease) {
        if lease.binding.is_some() {
            self.retired.push(Retired {
                epoch: lease.epoch,
                _room: lease._room,
                _chat: lease._chat,
            });
        }
    }
    fn revoke_private(&mut self, allowed: bool) -> Vec<ServerMessage> {
        if allowed {
            return vec![];
        }
        let mut messages = vec![];
        let mut revoked = vec![];
        for slot in [&mut self.active, &mut self.staged] {
            if slot.as_ref().is_some_and(|l| l.private) {
                let l = slot.take().expect("present lease");
                messages.push(ServerMessage::AccessRequired { epoch: l.epoch });
                revoked.push(l);
            }
        }
        for lease in revoked {
            self.retire(lease);
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
/// Package bytes have already passed signed closure admission. A changed library
/// body uses its own expected-version operation; arbitrary app source changes
/// still require a fresh collection. Callers journal the candidate before swap.
fn configure_package_oib(doc: &mut Document, ui: &otrios_interfaces::canvas::Ui) -> Result<bool> {
    match doc.configure_oib(ui) {
        Ok(changed) => Ok(changed),
        Err(original) => {
            if canvas::oib::seed(ui)
                .and_then(|seed| seed.graph.as_ref())
                .and_then(|graph| graph.imports.as_ref())
                .is_none()
            {
                return Err(original);
            }
            let (generation, lock) = doc.oib_baseline()?;
            doc.adopt_oib_baseline(ui, generation, lock)
        }
    }
}

fn collection_room(binding: &otrios_interfaces::spaces::Binding, package: &Package) -> String {
    if package.app.state_ref == package.primary_ref {
        binding.collection.clone()
    } else {
        format!(
            "{}:{}",
            binding.collection,
            canvas::hex(&canvas::hash(package.app.state_ref.as_bytes()))
        )
    }
}
fn collection_data_usage(root: &Path, collection: &str) -> Option<u64> {
    let path = root.join(format!("collection-{collection}.data"));
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(0),
        _ => return None,
    };
    let mut total = 0_u64;
    for (count, entry) in entries.enumerate() {
        if count == 128 {
            return None;
        }
        let entry = entry.ok()?;
        let meta = fs::symlink_metadata(entry.path()).ok()?;
        if !meta.is_file() {
            return None;
        }
        total = total.checked_add(meta.len())?;
    }
    Some(total)
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
            publisher: None,
            spaces: None,
            collector: None,
            collection_views: BTreeMap::new(),
            provisioning: false,
            discovery: otrios_core::spaces::discovery_cursor().map_err(|e| e.to_string())?,
            discovery_catalog: vec![],
            ink_rooms: BTreeMap::new(),
            chat_rooms: BTreeMap::new(),
            cleanup: BTreeMap::new(),
            next_maintenance: None,
            presence: vec![],
            next_presence: 0,
            staged_data: None,
            staged_app: None,
        })
    }
    pub(super) fn trust_publisher(&mut self, key: [u8; 32]) {
        self.publisher = Some(key);
    }
    pub(super) fn enable_spaces(&mut self, path: PathBuf, provisioning: bool) -> Result<()> {
        if path.with_extension("provisioning").exists() && !provisioning {
            return Err(
                "Space provisioning is incomplete; resume the explicit provisioning command".into(),
            );
        }
        self.spaces = Some(otrios_core::spaces::Store::open(path).map_err(|e| e.to_string())?);
        self.spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .bind_storage(
                self.state
                    .as_deref()
                    .ok_or("Durable app storage required")?,
            )
            .map_err(|e| e.to_string())?;
        self.spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .recover_storage()
            .map_err(|e| e.to_string())?;
        let registry = self.registry_path()?.to_path_buf();
        let marker = self
            .state
            .as_ref()
            .ok_or("Durable app storage required")?
            .join("spaces.registry");
        if !marker.exists() {
            persist(
                &marker,
                registry
                    .to_str()
                    .ok_or("Space registry path must be UTF-8")?
                    .as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        }
        self.provisioning = provisioning;
        self.maintain_storage(true)?;
        Ok(())
    }
    pub(super) fn cleanup(
        &self,
        space: &str,
    ) -> Option<&otrios_interfaces::spaces::CleanupSummary> {
        self.cleanup.get(space)
    }
    pub(super) fn maintain_storage(&mut self, force: bool) -> Result<bool> {
        self.expire_data_stage(chat::now());
        self.expire_app_stage(chat::now());
        if self.spaces.is_none() {
            return Ok(false);
        }
        let now = std::time::Instant::now();
        if !force && self.next_maintenance.is_some_and(|next| now < next) {
            return Ok(false);
        }
        self.next_maintenance = Some(now + Duration::from_secs(60));
        let deleted = self
            .registry()
            .map_err(|e| e.to_string())?
            .deletions
            .iter()
            .map(|d| d.collection.clone())
            .collect::<BTreeSet<_>>();
        let mut active = BTreeSet::new();
        self.rooms.retain(|key, room| {
            let collection = key.split(':').next().unwrap_or_default();
            if !deleted.contains(collection) {
                return true;
            }
            if Arc::strong_count(&room.lease) > 1 {
                active.insert(collection.to_owned());
                return true;
            }
            false
        });
        self.chat_rooms.retain(|key, room| {
            let collection = key.split(':').next().unwrap_or_default();
            if !deleted.contains(collection) {
                return true;
            }
            if Arc::strong_count(room) > 1 {
                active.insert(collection.to_owned());
                return true;
            }
            false
        });
        self.ink_rooms.retain(|key, room| {
            let collection = key.split(':').next().unwrap_or_default();
            if !deleted.contains(collection) {
                return true;
            }
            if Arc::strong_count(room) > 1 {
                active.insert(collection.to_owned());
                return true;
            }
            false
        });
        self.collection_views
            .retain(|collection, _| !deleted.contains(collection));
        let unix_ms = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis(),
        )
        .map_err(|e| e.to_string())?;
        let summary = self
            .spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .maintain_deleted(unix_ms, &active)
            .map_err(|e| e.to_string())?;
        if summary.values().any(|s| s.local_pending > 0) {
            self.next_maintenance = Some(now + Duration::from_secs(1));
        }
        if summary == self.cleanup {
            return Ok(false);
        }
        let cursor =
            otrios_core::spaces::advance_discovery(&self.discovery).map_err(|e| e.to_string())?;
        self.cleanup = summary;
        self.discovery = cursor;
        Ok(true)
    }
    pub(super) fn registry_path(&self) -> Result<&Path> {
        self.spaces
            .as_ref()
            .map(|store| store.path())
            .ok_or("Spaces unavailable".into())
    }
    pub(super) fn service_binding(
        &self,
        admin: &str,
        space: &str,
        app: &str,
        identity: &str,
    ) -> Result<otrios_interfaces::spaces::ServiceBinding> {
        if !otrios_core::spaces::valid_identity(identity) {
            return Err("Invalid service identity".into());
        }
        otrios_core::spaces::admit_service_identity(
            self.registry().map_err(|e| e.to_string())?,
            identity,
        )
        .map_err(|e| e.to_string())?;
        let p = self
            .bundles
            .get(&self.active)
            .and_then(|b| b.packages.values().find(|p| p.app.id == app))
            .ok_or("Installed app unavailable")?;
        let binding = self.binding(
            &spaces::Scope {
                space: Some(space.into()),
                identity: Some(admin.into()),
            },
            p,
        )?;
        Ok(otrios_interfaces::spaces::ServiceBinding {
            identity: identity.into(),
            binding,
        })
    }
    pub(super) fn configure_collector(&mut self, path: &Path) -> Result<()> {
        let grant: otrios_interfaces::spaces::ServiceBinding =
            serde_json::from_slice(&spaces::private_bytes(path, 8192)?)
                .map_err(|_| "Invalid collector binding")?;
        if grant.binding.app != "monitoring"
            || !otrios_core::spaces::valid_identity(&grant.identity)
        {
            return Err("Collector must be explicitly bound to Monitoring".into());
        }
        otrios_core::spaces::admit_service_identity(
            self.registry().map_err(|e| e.to_string())?,
            &grant.identity,
        )
        .map_err(|e| e.to_string())?;
        self.collector = Some(grant);
        Ok(())
    }
    pub(super) fn set_collector_view(&mut self, identity: &str, bytes: Vec<u8>) -> Result<()> {
        if bytes.len() > canvas::MAX_BYTES {
            return Err("Collection presentation exceeds transport budget".into());
        }
        let service = self.collector.as_ref().ok_or("Collector unavailable")?;
        otrios_core::spaces::authorize_service_installed(
            self.registry().map_err(|e| e.to_string())?,
            identity,
            service,
            &self.app_policy(&service.binding.app)?,
        )
        .map_err(|e| e.to_string())?;
        let checked = otrios_core::dashboard::layout::Layout::from_bytes(&bytes)?;
        checked.resolve_slots(&BTreeMap::new())?;
        let collection = service.binding.collection.clone();
        let path = self
            .state
            .as_ref()
            .ok_or("Durable collection storage required")?
            .join(format!("collection-{collection}.presentation.flex"));
        persist(&path, &bytes).map_err(|e| e.to_string())?;
        if self.collection_views.len() >= 128
            && !self.collection_views.contains_key(&collection)
            && let Some(key) = self.collection_views.keys().next().cloned()
        {
            self.collection_views.remove(&key);
        }
        self.collection_views
            .insert(collection, (canvas::hash(&bytes), bytes));
        Ok(())
    }
    fn load_collection_view(&mut self, collection: &str) -> Result<()> {
        if self.collection_views.contains_key(collection) {
            return Ok(());
        }
        let path = self
            .state
            .as_ref()
            .ok_or("Durable collection storage required")?
            .join(format!("collection-{collection}.presentation.flex"));
        let bytes = bounded(&path, canvas::MAX_BYTES)?;
        let checked = otrios_core::dashboard::layout::Layout::from_bytes(&bytes)?;
        checked.resolve_slots(&BTreeMap::new())?;
        if self.collection_views.len() >= 128
            && let Some(key) = self.collection_views.keys().next().cloned()
        {
            self.collection_views.remove(&key);
        }
        self.collection_views
            .insert(collection.into(), (canvas::hash(&bytes), bytes));
        Ok(())
    }
    pub(super) fn import_legacy(
        &mut self,
        identity: &str,
        space: &str,
        app: &str,
        source: &str,
    ) -> Result<Value> {
        let package = self
            .bundles
            .get(&self.active)
            .and_then(|b| {
                b.packages
                    .values()
                    .find(|p| p.app.id.split('/').next() == Some(app) && p.app.state_ref == source)
            })
            .cloned()
            .ok_or("Installed app unavailable")?;
        if package.app.state_ref != source {
            return Err("Legacy source differs from admitted app State".into());
        }
        let binding = self.binding(
            &spaces::Scope {
                space: Some(space.into()),
                identity: Some(identity.into()),
            },
            &package,
        )?;
        let source = self
            .rooms
            .get(source)
            .filter(|r| !r.failed)
            .ok_or("Legacy root unavailable")?;
        let bytes = source.doc.save()?;
        let root = source.doc.root_hash();
        let original = source
            .file
            .clone()
            .ok_or("Legacy source has no durable checkpoint")?;
        self.spaces
            .as_ref()
            .ok_or("Spaces unavailable")?
            .check_legacy_import(&binding.collection, &original)
            .map_err(|e| e.to_string())?;
        let mut originals = vec![original.clone()];
        if original.with_extension("merkle.log").exists() {
            originals.push(original.with_extension("merkle.log"));
        }
        let path = self.collection_file(&binding, &package)?;
        if !path.exists() && path.with_extension("merkle.log").exists() {
            return Err("Missing migration checkpoint with retained journal".into());
        }
        self.collection_parent(&path)?;
        if path.exists() {
            let existing = store::load(
                &path,
                Document::load(
                    package.schema,
                    &package.app.state_ref,
                    &bounded(&path, canvas::MAX_BYTES)?,
                )?,
            )?;
            if existing.root_hash() != root {
                return Err("Migration target already contains different confirmed data".into());
            }
        } else {
            persist(&path, &bytes).map_err(|e| e.to_string())?;
        }
        if app == "monitoring" && !self.view.is_empty() {
            let presentation = path.with_file_name(format!(
                "collection-{}.presentation.flex",
                binding.collection
            ));
            if !presentation.exists() {
                persist(&presentation, &self.view).map_err(|e| e.to_string())?;
            }
        }
        self.spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .register_legacy_sources(&binding.collection, &originals)
            .map_err(|e| e.to_string())?;
        Ok(
            json!({"collection":binding.collection,"source":package.app.state_ref,"source_root":canvas::hex(&root)}),
        )
    }
    /// Recover cleanup attribution from a completed private import receipt.
    /// Never replay payloads or change current admission from historical records.
    pub(super) fn recover_legacy_original(&mut self, record: &Value) -> Result<()> {
        let collection = record["collection"]
            .as_str()
            .ok_or("Missing import collection")?;
        let source = record["source"].as_str().ok_or("Missing import source")?;
        let chat = record["kind"] == "legacy-chat";
        let original = if chat {
            if source != "brass-relay.journal" {
                return Err("Unknown legacy journal source".into());
            }
            self.state
                .as_ref()
                .ok_or("Durable storage required")?
                .join(source)
        } else {
            if self
                .spaces
                .as_ref()
                .ok_or("Spaces unavailable")?
                .legacy_document_registered(collection, source)
                .map_err(|e| e.to_string())?
            {
                return Ok(());
            }
            self.rooms
                .get(source)
                .and_then(|room| room.file.clone())
                .ok_or("Legacy owning app unavailable")?
        };
        if self
            .spaces
            .as_ref()
            .ok_or("Spaces unavailable")?
            .legacy_source_registered(collection, &original)
            .map_err(|e| e.to_string())?
        {
            return Ok(());
        }
        match fs::symlink_metadata(&original) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        }
        let mut originals = vec![original.clone()];
        let _owner;
        if chat {
            _owner = Some(otrios_core::canvas::chat::Room::open(
                Some(original.clone()),
                chat::now(),
            )?);
            let bytes = spaces::private_bytes(&original, 64 * 1024 * 1024)?;
            if record["bytes"].as_u64() != Some(bytes.len() as u64)
                || record["source_sha256"] != canvas::hex(&canvas::hash(&bytes))
            {
                return Err("Legacy journal differs from its completed import receipt".into());
            }
        } else {
            _owner = None;
            let room = self
                .rooms
                .get(source)
                .filter(|r| !r.failed)
                .ok_or("Legacy root unavailable")?;
            if record["source_root"] != canvas::hex(&room.doc.root_hash()) {
                return Err("Legacy root differs from its completed import receipt".into());
            }
            let log = original.with_extension("merkle.log");
            if log.exists() {
                originals.push(log);
            }
        }
        self.spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .register_legacy_sources(collection, &originals)
            .map_err(|e| e.to_string())
    }
    pub(super) fn registry(
        &self,
    ) -> std::result::Result<&otrios_interfaces::spaces::Registry, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .registry()
    }
    pub(super) fn import_legacy_chat(
        &mut self,
        identity: &str,
        space: &str,
        app: &str,
        destination: &str,
    ) -> Result<Value> {
        use otrios_interfaces::spaces::Role;
        const LIMIT: usize = 64 * 1024 * 1024;
        if !self.provisioning
            || !self
                .registry()
                .map_err(|e| e.to_string())?
                .spaces
                .iter()
                .any(|s| {
                    s.id == space
                        && otrios_core::spaces::role(s, Some(identity)) == Some(Role::Admin)
                })
        {
            return Err("Chat import requires offline provisioning by a Space Admin".into());
        }
        let package = self
            .bundles
            .get(&self.active)
            .and_then(|bundle| {
                bundle.packages.values().find(|p| {
                    p.chat
                        && p.app.id.split('/').next() == Some(app)
                        && p.app.state_ref == destination
                })
            })
            .cloned()
            .ok_or("Chat import target is not an installed owning root")?;
        let binding = self.binding(
            &spaces::Scope {
                space: Some(space.into()),
                identity: Some(identity.into()),
            },
            &package,
        )?;
        let path = self.chat_file(&binding, &package)?;
        let receipt = path.with_file_name("legacy-chat-import.json");
        if receipt.exists() {
            let record: Value = serde_json::from_slice(&spaces::private_bytes(&receipt, 4096)?)
                .map_err(|_| "Invalid chat import receipt")?;
            if record.as_object().is_none_or(|r| r.len() != 9)
                || record["version"] != 1
                || record["kind"] != "legacy-chat"
                || record["app"] != app
                || record["collection"] != binding.collection
                || record["root"] != package.app.state_ref
                || record["schema"] != binding.schema
                || record["source"] != "brass-relay.journal"
                || !record["source_sha256"]
                    .as_str()
                    .is_some_and(otrios_interfaces::release::valid_hash)
                || !record["bytes"]
                    .as_u64()
                    .is_some_and(|n| (8..=LIMIT as u64).contains(&n))
            {
                return Err("Collection has a different or invalid legacy chat import".into());
            }
            // A completed import is never replayed over subsequent confirmed
            // edits. Validate its current journal even if the old source is gone.
            spaces::private_metadata(&path, LIMIT)?;
            let _target = otrios_core::canvas::chat::Room::open(Some(path), chat::now())?;
            self.recover_legacy_original(&record)?;
            return Ok(record);
        }
        let source = self
            .state
            .as_ref()
            .ok_or("Durable chat storage required")?
            .join("brass-relay.journal");
        self.spaces
            .as_ref()
            .ok_or("Spaces unavailable")?
            .check_legacy_import(&binding.collection, &source)
            .map_err(|e| e.to_string())?;
        spaces::private_metadata(&source, LIMIT)?;
        // This owns the old journal's stable sidecar lock through the copy.
        // Recovery validates every complete record and only trims a torn tail.
        let source_room = otrios_core::canvas::chat::Room::open(Some(source.clone()), chat::now())?;
        source_room.qualify_legacy_import()?;
        let bytes = spaces::private_bytes(&source, LIMIT)?;
        if !path.exists()
            && (receipt.exists() || path.with_extension("journal.owner.lock").exists())
        {
            return Err("Missing imported chat journal with retained ownership metadata".into());
        }
        let record = json!({"version":1,"kind":"legacy-chat","app":app,
            "collection":binding.collection,"root":package.app.state_ref,"schema":binding.schema,
            "source":"brass-relay.journal","source_sha256":canvas::hex(&canvas::hash(&bytes)),"bytes":bytes.len()});
        let record_bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        self.collection_parent(&path)?;
        if path.exists() {
            if spaces::private_bytes(&path, LIMIT)? != bytes {
                return Err("Chat migration target contains different confirmed data".into());
            }
        } else {
            persist(&path, &bytes).map_err(|e| e.to_string())?;
        }
        persist(&receipt, &record_bytes).map_err(|e| e.to_string())?;
        self.spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .register_legacy_sources(&binding.collection, &[source])
            .map_err(|e| e.to_string())?;
        Ok(record)
    }
    pub(super) fn manage_space(
        &mut self,
        actor: &str,
        request: &otrios_interfaces::spaces::Request,
        policy: Option<&otrios_interfaces::spaces::AppPolicy>,
    ) -> std::result::Result<u64, otrios_core::spaces::Error> {
        if let Some(service) = &self.collector {
            otrios_core::spaces::admit_human_management(
                actor,
                request,
                &[service.identity.as_str()],
            )?;
        }
        if let otrios_interfaces::spaces::Change::Move {
            operation,
            source,
            destination,
            app,
            collection,
            destination_collection,
            ..
        } = &request.change
            && self
                .move_committed(actor, source, destination, operation)?
                .is_none()
        {
            let service = self.bundles.get(&self.active).is_some_and(|b| {
                b.packages
                    .values()
                    .any(|p| p.policy.app == *app && p.aggregate_view)
            }) || self
                .collector
                .as_ref()
                .is_some_and(|g| g.binding.app == *app);
            if service {
                return Err(otrios_core::spaces::Error::Unavailable);
            }
            for cid in std::iter::once(collection).chain(destination_collection.iter()) {
                if self
                    .rooms
                    .iter()
                    .any(|(key, r)| key.split(':').next() == Some(cid.as_str()) && r.failed)
                    || self
                        .chat_rooms
                        .iter()
                        .any(|(key, r)| key.split(':').next() == Some(cid.as_str()) && !r.healthy())
                {
                    return Err(otrios_core::spaces::Error::Unavailable);
                }
            }
            // Verified discovery checks code/schema and known owned-chat failures
            // without reading private app documents into the management channel.
            for (id, cid) in [
                (source, Some(collection)),
                (destination, destination_collection.as_ref()),
            ] {
                let snapshot = self.discovery(Some(actor), Some(id))?;
                let detail = snapshot
                    .selected
                    .ok_or(otrios_core::spaces::Error::Denied)?;
                if let Some(cid) = cid {
                    let fact = detail
                        .apps
                        .iter()
                        .find(|a| a.collection == *cid)
                        .map(|a| (a.availability, a.stored_bytes))
                        .or_else(|| {
                            detail
                                .retained
                                .iter()
                                .find(|d| d.collection == *cid)
                                .map(|d| (d.availability, d.stored_bytes))
                        });
                    if !fact.is_some_and(|(availability, bytes)| {
                        availability == otrios_interfaces::spaces::AppAvailability::Available
                            && bytes.is_some()
                    }) {
                        return Err(otrios_core::spaces::Error::Unavailable);
                    }
                }
            }
        }
        let removed = match &request.change {
            otrios_interfaces::spaces::Change::Remove { space, app } => self
                .registry()?
                .spaces
                .iter()
                .find(|s| &s.id == space)
                .and_then(|s| s.apps.iter().find(|a| &a.policy.app == app))
                .map(|a| a.collection.clone()),
            _ => None,
        };
        let cursor = otrios_core::spaces::advance_discovery(&self.discovery)?;
        let policies = self
            .bundles
            .get(&self.active)
            .into_iter()
            .flat_map(|bundle| bundle.packages.values())
            .filter(|p| !p.app.id.contains('/'))
            .map(|p| p.policy.clone())
            .collect::<Vec<_>>();
        let store = self
            .spaces
            .as_mut()
            .ok_or(otrios_core::spaces::Error::Unavailable)?;
        let generation = if matches!(
            request.change,
            otrios_interfaces::spaces::Change::Audience { .. }
                | otrios_interfaces::spaces::Change::Access { .. }
        ) {
            store.commit_catalog(actor, request, &policies)?.generation
        } else {
            store.commit(actor, request, policy)?.generation
        };
        self.discovery = cursor;
        if let Some(collection) = removed {
            self.rooms.retain(|key, room| {
                room.failed
                    || Arc::strong_count(&room.lease) > 1
                    || key.split(':').next() != Some(collection.as_str())
            });
            self.collection_views.remove(&collection);
            self.ink_rooms
                .retain(|key, _| key.split(':').next() != Some(collection.as_str()));
        }
        // A newly created Space must have storage status immediately; discovery
        // must not wait for the next periodic maintenance pass to offer controls.
        if let Err(error) = self.maintain_storage(true) {
            eprintln!("Space cleanup pending: {error}");
        }
        Ok(generation)
    }
    pub(super) fn move_committed(
        &self,
        actor: &str,
        source: &str,
        destination: &str,
        operation: &str,
    ) -> std::result::Result<Option<u64>, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .move_committed(actor, source, destination, operation)
    }
    pub(super) fn refresh_presence(&mut self) -> Result<bool> {
        let before = self.presence.len();
        self.presence.retain(|p| p.strong_count() > 0);
        let changed = before != self.presence.len();
        if changed {
            self.discovery = otrios_core::spaces::advance_discovery(&self.discovery)
                .map_err(|e| e.to_string())?;
        }
        Ok(changed)
    }
    pub(super) fn discovery_cursor(&self) -> &otrios_interfaces::spaces::DiscoveryCursor {
        &self.discovery
    }
    fn discovery_catalog_changed(&mut self) -> Result<()> {
        if self.discovery_catalog != self.active {
            self.discovery = otrios_core::spaces::advance_discovery(&self.discovery)
                .map_err(|e| e.to_string())?;
            self.discovery_catalog = self.active.clone();
        }
        Ok(())
    }
    pub(super) fn discovery(
        &self,
        identity: Option<&str>,
        selected: Option<&str>,
    ) -> std::result::Result<otrios_interfaces::spaces::Discovery, otrios_core::spaces::Error> {
        use otrios_interfaces::spaces::InstalledApp;
        let registry = self.registry()?;
        let installed = self
            .bundles
            .get(&self.active)
            .into_iter()
            .flat_map(|b| b.packages.values())
            .filter(|p| !p.app.id.contains('/'))
            .map(|p| InstalledApp {
                bundle_sha256: p.bundle_sha256.clone(),
                code_bytes: p.code_bytes,
                revision: p.revision,
                policy: p.policy.clone(),
                title: p.app.title.clone(),
                ui_ref: p.app.ui_ref.clone(),
                state_ref: p.app.state_ref.clone(),
            })
            .collect::<Vec<_>>();
        // Only the selected admitted Space needs storage facts. Do not read any
        // collection payload or start app execution during discovery.
        let mut facts = BTreeMap::new();
        for s in registry.spaces.iter().filter(|s| {
            Some(s.id.as_str()) == selected && otrios_core::spaces::visible(s, identity)
        }) {
            for (policy, collection) in s.apps.iter().map(|a| (&a.policy, &a.collection)).chain(
                registry
                    .retained
                    .iter()
                    .filter(|d| d.space == s.id)
                    .map(|d| (&d.policy, &d.collection)),
            ) {
                let package = self
                    .bundles
                    .get(&self.active)
                    .and_then(|b| b.packages.values().find(|p| p.app.id == policy.app));
                let mut available = package.is_some()
                    && !self.rooms.iter().any(|(key, r)| {
                        r.failed && key.split(':').next() == Some(collection.as_str())
                    })
                    && !self.chat_rooms.iter().any(|(key, room)| {
                        key.split(':').next() == Some(collection.as_str()) && !room.healthy()
                    });
                if available {
                    available = self.bundles.get(&self.active).is_some_and(|bundle| {
                        bundle
                            .packages
                            .values()
                            .filter(|p| p.policy.app == policy.app && p.chat)
                            .all(|p| {
                                self.chat_path(collection, p)
                                    .and_then(|path| self.missing_owned_chat(&path, p))
                                    .is_ok_and(|missing| !missing)
                            })
                    });
                }
                let stored_bytes = self.state.as_ref().and_then(|root| {
                    let checkpoint = root.join(format!("collection-{}.merkle", collection));
                    let presentation =
                        root.join(format!("collection-{}.presentation.flex", collection));
                    if package.is_some_and(|p| p.aggregate_view) && !presentation.is_file() {
                        available = false;
                    }
                    [
                        checkpoint.with_extension("merkle.log"),
                        checkpoint,
                        presentation,
                    ]
                    .iter()
                    .try_fold(
                        collection_data_usage(root, collection)?,
                        |sum, p| match fs::metadata(p) {
                            Ok(m) if m.is_file() => sum.checked_add(m.len()),
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(sum),
                            _ => None,
                        },
                    )
                });
                facts.insert(
                    collection.clone(),
                    otrios_core::spaces::CollectionFacts {
                        stored_bytes,
                        available,
                    },
                );
            }
        }
        let mut discovery = otrios_core::spaces::discovery_with_cleanup(
            registry,
            identity,
            selected,
            &installed,
            &facts,
            &self.cleanup,
            self.discovery.clone(),
        )?;
        discovery.profile = identity
            .map(|actor| {
                self.spaces
                    .as_ref()
                    .ok_or(otrios_core::spaces::Error::Unavailable)?
                    .profile(actor)
            })
            .transpose()?
            .flatten();
        discovery.catalog = self.active.clone();
        if let Some(detail) = discovery.selected.as_mut()
            && discovery.spaces.iter().any(|s| {
                s.id == detail.space && s.role == Some(otrios_interfaces::spaces::Role::Admin)
            })
        {
            let participants = self
                .presence
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .filter(|p| {
                    p.binding.space == detail.space
                        && self.grant_current(&p.scope, &p.binding, &p.grant)
                })
                .collect::<Vec<_>>();
            let users = || {
                participants
                    .iter()
                    .map(|p| otrios_core::spaces::AcceptedUser {
                        collection: &p.binding.collection,
                        principal: p.grant.principal.as_deref(),
                        anonymous: p.anonymous,
                    })
            };
            detail.active_users = Some(otrios_core::spaces::active_users(users(), None));
            for app in &mut detail.apps {
                app.active_users = Some(otrios_core::spaces::active_users(
                    users(),
                    Some(&app.collection),
                ));
            }
        }
        Ok(discovery)
    }
    pub(super) fn app_policy(&self, app: &str) -> Result<otrios_interfaces::spaces::AppPolicy> {
        let p = self
            .bundles
            .get(&self.active)
            .and_then(|b| b.packages.values().find(|p| p.app.id == app))
            .ok_or("Installed app unavailable")?;
        Ok(p.policy.clone())
    }
    fn binding(
        &self,
        scope: &spaces::Scope,
        package: &Package,
    ) -> Result<otrios_interfaces::spaces::Binding> {
        let owner = package
            .app
            .id
            .split('/')
            .next()
            .ok_or("Missing installed app")?;
        let schema = &package.policy.schema;
        let registry = self.registry().map_err(|e| e.to_string())?;
        let binding = otrios_core::spaces::bind(
            self.registry().map_err(|e| e.to_string())?,
            scope.identity.as_deref(),
            scope.space.as_deref().ok_or("Select a Space")?,
            owner,
            schema,
        )
        .map_err(|e| e.to_string())?;
        otrios_core::spaces::authorize_installed(
            registry,
            scope.identity.as_deref(),
            &binding,
            otrios_interfaces::spaces::Operation::Read,
            &self.app_policy(owner)?,
        )
        .map_err(|e| e.to_string())?;
        Ok(binding)
    }
    pub(super) fn allowed(
        &self,
        scope: &spaces::Scope,
        binding: &otrios_interfaces::spaces::Binding,
        operation: otrios_interfaces::spaces::Operation,
    ) -> bool {
        scope.space.as_deref() == Some(binding.space.as_str())
            && (operation == otrios_interfaces::spaces::Operation::Read
                || otrios_core::spaces::admit_human_action(
                    scope.identity.as_deref(),
                    &[self.collector.as_ref().map_or("", |c| c.identity.as_str())],
                )
                .is_ok())
            && self.registry().is_ok_and(|registry| {
                self.app_policy(&binding.app).is_ok_and(|policy| {
                    otrios_core::spaces::authorize_installed(
                        registry,
                        scope.identity.as_deref(),
                        binding,
                        operation,
                        &policy,
                    )
                    .is_ok()
                })
            })
    }
    fn ui_allowed(
        &self,
        scope: &spaces::Scope,
        binding: &otrios_interfaces::spaces::Binding,
    ) -> bool {
        self.allowed(scope, binding, otrios_interfaces::spaces::Operation::Edit)
            && self.registry().is_ok_and(|r| {
                r.spaces.iter().any(|space| {
                    space.id == binding.space
                        && otrios_core::spaces::role(space, scope.identity.as_deref())
                            == Some(otrios_interfaces::spaces::Role::Admin)
                })
            })
    }
    fn actions(&self, scope: &spaces::Scope, binding: &otrios_interfaces::spaces::Binding) -> u8 {
        use otrios_interfaces::spaces::Operation;
        u8::from(self.allowed(scope, binding, Operation::Edit))
            | (u8::from(self.allowed(scope, binding, Operation::Comment)) << 1)
            | (u8::from(self.allowed(scope, binding, Operation::Moderate)) << 2)
            | (u8::from(self.ui_allowed(scope, binding)) << 3)
    }
    pub(super) fn grant_current(
        &self,
        scope: &spaces::Scope,
        binding: &otrios_interfaces::spaces::Binding,
        grant: &Grant,
    ) -> bool {
        grant.principal == scope.identity
            && grant.actions == self.actions(scope, binding)
            && self.allowed(scope, binding, otrios_interfaces::spaces::Operation::Read)
    }
    pub(super) fn chat_allowed(
        &self,
        scope: &spaces::Scope,
        binding: &otrios_interfaces::spaces::Binding,
        command: &otrios_interfaces::chat::Command,
    ) -> bool {
        self.allowed(scope, binding, otrios_interfaces::spaces::Operation::Read)
            && (matches!(
                command,
                otrios_interfaces::chat::Command::Leave
                    | otrios_interfaces::chat::Command::ReadAsset { .. }
            ) || otrios_core::spaces::admit_human_action(
                scope.identity.as_deref(),
                &[self.collector.as_ref().map_or("", |c| c.identity.as_str())],
            )
            .is_ok())
            && self.registry().is_ok_and(|registry| {
                self.app_policy(&binding.app).is_ok_and(|policy| {
                    otrios_core::spaces::authorize_chat(
                        registry,
                        scope.identity.as_deref(),
                        binding,
                        &policy,
                        command,
                    )
                    .is_ok()
                })
            })
    }
    fn revoke_scoped(&self, leases: &mut Leases, scope: &spaces::Scope) -> Vec<ServerMessage> {
        let mut messages = vec![];
        let mut revoked = vec![];
        for slot in [&mut leases.active, &mut leases.staged] {
            if slot.as_ref().is_some_and(|lease| {
                lease
                    .binding
                    .as_ref()
                    .is_some_and(|binding| !self.grant_current(scope, binding, &lease.grant))
            }) {
                let lease = slot.take().expect("present lease");
                messages.push(ServerMessage::AccessRequired { epoch: lease.epoch });
                revoked.push(lease);
            }
        }
        for lease in revoked {
            leases.retire(lease);
        }
        messages
    }
    fn scoped_room(
        &mut self,
        binding: &otrios_interfaces::spaces::Binding,
        package: &Package,
    ) -> Result<()> {
        let key = collection_room(binding, package);
        if self.rooms.contains_key(&key) {
            return Ok(());
        }
        if self.rooms.len() >= 256 {
            // Only durable, healthy, idle rooms can be reconstructed. Keep an
            // uncertain persistence outcome poisoned until operator recovery;
            // active and staged participants keep their room pinned.
            let idle = self
                .rooms
                .iter()
                .find_map(|(key, room)| {
                    (room.file.is_some() && !room.failed && Arc::strong_count(&room.lease) == 1)
                        .then(|| key.clone())
                })
                .ok_or("App root budget exhausted")?;
            self.rooms.remove(&idle);
        }
        let file = Some(self.collection_file(binding, package)?);
        if let Some(path) = &file {
            if !path.exists() && path.with_extension("merkle.log").exists() {
                return Err("Missing collection checkpoint with retained journal".into());
            }
            self.collection_parent(path)?;
        }
        let description = canvas::decode_ui(&package.ui)?;
        let mut doc = match file.as_ref().filter(|p| p.exists()) {
            Some(path) => {
                let mut checkpoint = Document::load(
                    package.schema.clone(),
                    &package.app.state_ref,
                    &bounded(path, canvas::MAX_BYTES)?,
                )?;
                checkpoint.prepare_oib_replay(&description)?;
                store::load(path, checkpoint)?
            }
            None => Document::new(package.schema.clone(), &package.app.state_ref)?,
        };
        let persisted = doc.clone();
        let seeded = configure_package_oib(&mut doc, &description)?;
        if seeded && let Some(file) = file.as_ref().filter(|p| p.exists()) {
            store::append(file, &persisted, &doc)?;
        }
        doc.set_signal_fields(&package.signal_fields)?;
        if let Some(file) = &file
            && !file.exists()
        {
            persist(file, &doc.save()?).map_err(|e| e.to_string())?;
        }
        self.rooms.insert(
            key,
            Room {
                doc,
                file,
                failed: false,
                lease: Arc::new(()),
            },
        );
        Ok(())
    }
    pub(super) fn ink_room(
        &mut self,
        activation: &Activation,
        peers: usize,
    ) -> Result<Arc<ink::Room>> {
        if !activation.ink {
            return Err("App does not declare ink".into());
        }
        if let Some(room) = self.ink_rooms.get(&activation.room) {
            return Ok(room.clone());
        }
        if self.ink_rooms.len() >= 128 {
            let idle = self
                .ink_rooms
                .iter()
                .find_map(|(key, room)| {
                    (Arc::strong_count(room) == 1 && room.subscribers() == 0).then(|| key.clone())
                })
                .ok_or("Ink room budget exhausted")?;
            self.ink_rooms.remove(&idle);
        }
        let room = Arc::new(ink::Room::new(peers)?);
        self.ink_rooms.insert(activation.room.clone(), room.clone());
        Ok(room)
    }
    fn prepare_chat(
        &mut self,
        binding: &otrios_interfaces::spaces::Binding,
        package: &Package,
    ) -> Result<Arc<space_chat::Hub>> {
        let key = collection_room(binding, package);
        if let Some(room) = self.chat_rooms.get(&key) {
            return Ok(room.clone());
        }
        if self.chat_rooms.len() >= 8 {
            let idle = self
                .chat_rooms
                .iter()
                .find_map(|(key, room)| {
                    (Arc::strong_count(room) == 1 && room.healthy()).then(|| key.clone())
                })
                .ok_or("Chat room budget exhausted")?;
            self.chat_rooms.remove(&idle);
        }
        let path = self.chat_file(binding, package)?;
        if self.missing_owned_chat(&path, package)? {
            return Err("Missing owned chat journal; verified storage repair required".into());
        }
        self.collection_parent(&path)?;
        let room = space_chat::Hub::new(path)?;
        self.chat_rooms.insert(key, room.clone());
        Ok(room)
    }
    fn chat_file(
        &self,
        binding: &otrios_interfaces::spaces::Binding,
        package: &Package,
    ) -> Result<PathBuf> {
        self.chat_path(&binding.collection, package)
    }
    fn chat_path(&self, collection: &str, package: &Package) -> Result<PathBuf> {
        let suffix = if package.app.state_ref == package.primary_ref {
            "primary".into()
        } else {
            canvas::hex(&canvas::hash(package.app.state_ref.as_bytes()))
        };
        Ok(self
            .state
            .as_ref()
            .ok_or("Durable chat storage required")?
            .join(format!("collection-{collection}.data"))
            .join(format!("chat-{suffix}.journal")))
    }
    fn missing_owned_chat(&self, path: &Path, package: &Package) -> Result<bool> {
        if path.exists() {
            return Ok(false);
        }
        if path.with_extension("journal.owner.lock").exists() {
            return Ok(true);
        }
        let receipt = path.with_file_name("legacy-chat-import.json");
        if !receipt.exists() {
            return Ok(false);
        }
        let record: Value = serde_json::from_slice(&spaces::private_bytes(&receipt, 4096)?)
            .map_err(|_| "Invalid chat import receipt")?;
        if record.as_object().is_none_or(|object| object.len() != 9)
            || record["version"] != 1
            || record["kind"] != "legacy-chat"
            || !record["root"]
                .as_str()
                .is_some_and(|root| root.starts_with("app:") && root.len() <= 128)
        {
            return Err("Invalid chat import receipt".into());
        }
        // One import receipt can name a subsidiary root. It does not establish
        // prior data for an uninitialized sibling in the same collection.
        Ok(record["root"] == package.app.state_ref)
    }
    pub(super) fn chat_room(&self, active: &Activation) -> Result<Arc<space_chat::Hub>> {
        if !active.chat {
            return Err("App does not declare chat".into());
        }
        self.chat_rooms
            .get(&active.room)
            .cloned()
            .ok_or("Chat room must be qualified before activation".into())
    }
    pub(super) fn owned_assets(
        &self,
        binding: &otrios_interfaces::spaces::Binding,
    ) -> Result<otrios_core::canvas::chat::assets::Assets> {
        otrios_core::canvas::chat::assets::Assets::new(
            self.state
                .as_deref()
                .ok_or("Durable asset storage required")?,
            &binding.collection,
        )
    }
    pub(super) fn publish_chat(
        &mut self,
        scope: &spaces::Scope,
        active: &Activation,
        source: &space_chat::Hub,
        command: &otrios_interfaces::chat::Command,
        now: u64,
    ) -> Result<u64> {
        use otrios_core::canvas::chat::{Publication, publish};
        let otrios_interfaces::chat::Command::Publish {
            epoch,
            nonce,
            sequence,
            revision,
            destination_space,
            expected_generation,
        } = command
        else {
            return Err("Expected a publication command".into());
        };
        if !self.chat_allowed(scope, &active.binding, command) {
            return Err("Publication admission required".into());
        }
        let actor = scope.identity.as_deref().ok_or("Sign in first")?;
        let policy = self.app_policy(&active.binding.app)?;
        let destination = otrios_core::spaces::publication_destination(
            self.registry().map_err(|e| e.to_string())?,
            actor,
            &active.binding,
            destination_space,
            *expected_generation,
            &policy,
        )
        .map_err(|_| "Publication access or reviewed destination changed")?;
        let package = self
            .bundles
            .get(&self.active)
            .and_then(|bundle| {
                bundle.packages.values().find(|p| {
                    p.app.id == active.binding.app && p.chat && p.app.state_ref == p.primary_ref
                })
            })
            .cloned()
            .ok_or("App has no primary chat publication adapter")?;
        let destination_room = self.prepare_chat(&destination, &package)?;
        let source_assets = self.owned_assets(&active.binding)?;
        let destination_assets = self.owned_assets(&destination)?;
        source.chat.run_inline(|source| {
            destination_room.chat.run_inline(|target| {
                publish(
                    source,
                    target,
                    &source_assets,
                    &destination_assets,
                    Publication {
                        source: &active.binding,
                        destination: &destination,
                        actor,
                        sequence: *sequence,
                        revision: *revision,
                        epoch: *epoch,
                        nonce,
                    },
                    now,
                )
            })
        })
    }
    fn collection_file(
        &self,
        binding: &otrios_interfaces::spaces::Binding,
        package: &Package,
    ) -> Result<PathBuf> {
        self.collection_path(&binding.collection, package)
    }
    fn collection_path(&self, collection: &str, package: &Package) -> Result<PathBuf> {
        let root = self
            .state
            .as_ref()
            .ok_or("Durable collection storage required")?;
        if package.app.state_ref == package.primary_ref {
            Ok(root.join(format!("collection-{collection}.merkle")))
        } else {
            Ok(root
                .join(format!("collection-{collection}.data"))
                .join(format!(
                    "doc-{}.merkle",
                    canvas::hex(&canvas::hash(package.app.state_ref.as_bytes()))
                )))
        }
    }
    fn collection_parent(&self, path: &Path) -> Result<()> {
        let root = self
            .state
            .as_ref()
            .ok_or("Durable collection storage required")?;
        let parent = path.parent().ok_or("Missing collection directory")?;
        if parent == root {
            return Ok(());
        }
        match fs::create_dir(parent) {
            Ok(()) => fs::File::open(root)
                .and_then(|d| d.sync_all())
                .map_err(|e| e.to_string()),
            Err(e)
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    && fs::symlink_metadata(parent).is_ok_and(|m| m.is_dir()) =>
            {
                fs::File::open(root)
                    .and_then(|d| d.sync_all())
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }
    pub(super) fn refresh(&mut self, releases: &Option<PathBuf>, release: &Value) -> Result<()> {
        let id = release["id"].as_str().unwrap_or("development");
        if let Some(store) = &self.spaces
            && let Some(job) = store.pending_app_install().map_err(|e| e.to_string())?
            && job.state.phase == otrios_core::transfers::AppImportPhase::Armed
            && id != job.review.release
            && job.state.release.as_deref() != Some(id)
        {
            return Err("An armed app install requires its exact planned release".into());
        }
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
                    || manifest["sync_protocol"] != STATE_PROTOCOL
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
        let bundle_format = manifest.as_ref().and_then(|m| m.get("bundle_abi"));
        if bundle_format.is_some_and(|format| format != "otrios-app-bundle-v1") {
            return Err("Unsupported installed bundle ABI".into());
        }
        if bundle_format.is_none() && self.bundles.contains_key(&catalog_hash) {
            self.active = catalog_hash;
            self.active_release = id.into();
            self.discovery_catalog_changed()?;
            return Ok(());
        }
        let catalog = canvas::decode_catalog(&catalog_bytes)?;
        if catalog.format != "otrios-app-catalog-v1"
            || catalog.apps.is_empty()
            || catalog.apps.len() > 16
        {
            return Err("App catalog format/count invalid".into());
        }
        let mut paths = std::collections::BTreeSet::new();
        for entry in &catalog.apps {
            if (!entry.path.is_empty()
                && !otrios_interfaces::canvas::valid_destination(&entry.path))
                || !paths.insert(&entry.path)
            {
                return Err("Invalid or duplicate destination path".into());
            }
            if let Some((parent, _)) = entry.path.rsplit_once('/')
                && !catalog.apps.iter().any(|e| e.path == parent)
            {
                return Err("Destination parent is not admitted".into());
            }
        }
        let mut packages = BTreeMap::new();
        let mut state_refs = BTreeMap::new();
        let mut app_ids = std::collections::BTreeSet::new();
        let mut new_rooms: BTreeMap<String, Room> = BTreeMap::new();
        for entry in catalog.apps {
            let a = entry.app;
            let owner = a.id.split('/').next().unwrap_or_default();
            let private = entry.private || owner == "monitoring";
            if !otrios_interfaces::canvas::valid_destination(&a.id)
                || a.ui_ref != format!("app:{}/ui", a.id)
                || !app_ids.insert(a.id.clone())
            {
                return Err("Invalid/duplicate destination identity".into());
            }
            if let Some((old_owner, old_private)) = state_refs.get(&a.state_ref) {
                if old_owner != owner || *old_private != private {
                    return Err(
                        "Shared State requires the same app owner and admission policy".into(),
                    );
                }
            } else {
                state_refs.insert(a.state_ref.clone(), (owner.to_owned(), private));
            }
            let verified = if bundle_format.is_some() {
                let m = manifest.as_ref().ok_or("Missing bundle release manifest")?;
                let root = releases
                    .as_ref()
                    .ok_or("Bundle installation requires a signed release")?
                    .join(id);
                let prefix = format!("authority/bundles/{}.otapp/", a.id);
                let tree =
                    super::bundle_files::read_tree(&root.join(prefix.trim_end_matches('/')))?;
                if m["authority"]
                    .as_object()
                    .ok_or("Missing private release closure")?
                    .keys()
                    .filter(|name| name.starts_with(&prefix))
                    .count()
                    != tree.len()
                {
                    return Err("Bundle differs from signed release closure".into());
                }
                for (path, bytes) in &tree {
                    verify_asset(m, &format!("{prefix}{path}"), bytes)?;
                }
                let verified = otrios_core::bundle::verify(
                    tree,
                    self.publisher
                        .as_ref()
                        .ok_or("Missing trusted bundle publisher key")?,
                )?;
                otrios_core::bundle::qualify_install(&verified)?;
                if verified.summary().info.app != a
                    || verified.summary().info.private != entry.private
                {
                    return Err("Bundle identity/policy differs from signed catalog".into());
                }
                Some(verified)
            } else {
                None
            };
            let base = if manifest.is_some() {
                folder.join(&a.id)
            } else {
                folder.join(&a.id).join("package")
            };
            let ui = if let Some(verified) = &verified {
                verified.file(otrios_interfaces::bundle::UI_PATH)?.to_vec()
            } else {
                bounded(&base.join("ui.flex"), canvas::MAX_BYTES)?
            };
            if entry.ui_hash != canvas::hash(&ui) {
                return Err("Catalog UI digest mismatch".into());
            }
            if let Some(m) = &manifest
                && verified.is_none()
            {
                verify_asset(m, &format!("authority/apps/{}/ui.flex", a.id), &ui)?;
            }
            let description = canvas::decode_ui(&ui)?;
            if description.ui_ref != a.ui_ref || canvas::validate_ui_format(&description).is_err() {
                return Err("UI reference/format differs from admitted catalog".into());
            }
            canvas::validate_schema(&description.state_schema)?;
            let selection = canvas::StateSelection::from_ui(&description)?;
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
                let bytes = if let Some(verified) = &verified {
                    verified
                        .file(otrios_interfaces::bundle::CODE_PATH)?
                        .to_vec()
                } else {
                    bounded(&base.join("behavior.wasm"), 65536)?
                };
                if let Some(m) = &manifest
                    && verified.is_none()
                {
                    verify_asset(m, &format!("authority/apps/{}/behavior.wasm", a.id), &bytes)?;
                }
                Behavior::load(&bytes, reference)?;
                bytes
            } else {
                vec![]
            };
            if self.spaces.is_none()
                || self.provisioning
                    && self
                        .spaces
                        .as_ref()
                        .is_some_and(|store| store.legacy_imports_open().unwrap_or(false))
            {
                if let Some(room) = self
                    .rooms
                    .get(&a.state_ref)
                    .or_else(|| new_rooms.get(&a.state_ref))
                {
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
                            owner,
                            canvas::hex(&canvas::hash(a.state_ref.as_bytes()))
                        ))
                    });
                    let mut doc = if let Some(path) = file.as_ref().filter(|p| p.exists()) {
                        let mut checkpoint = Document::load(
                            description.state_schema.clone(),
                            &a.state_ref,
                            &bounded(path, canvas::MAX_BYTES)?,
                        )?;
                        checkpoint.prepare_oib_replay(&description)?;
                        store::load(path, checkpoint)?
                    } else {
                        if file
                            .as_ref()
                            .is_some_and(|p| p.with_extension("merkle.log").exists())
                        {
                            return Err("Missing legacy checkpoint with retained journal".into());
                        }
                        Document::new(description.state_schema.clone(), &a.state_ref)?
                    };
                    configure_package_oib(&mut doc, &description)?;
                    doc.set_signal_fields(&signal_fields)?;
                    if let Some(path) = &file
                        && !path.exists()
                        && self.spaces.is_none()
                    {
                        persist(path, &doc.save()?).map_err(|e| e.to_string())?;
                    }
                    new_rooms.insert(
                        a.state_ref.clone(),
                        Room {
                            doc,
                            file,
                            failed: false,
                            lease: Arc::new(()),
                        },
                    );
                }
            }
            let policy = otrios_interfaces::spaces::AppPolicy {
                app: a.id.clone(),
                schema: canvas::hex(&canvas::hash(&canvas::encode_schema(
                    &description.state_schema,
                )?)),
                closed_only: private,
                editable: description.state_schema.fields.iter().any(|f| f.writable),
                comments: matches!(
                    description.presentation,
                    Some(otrios_interfaces::canvas::Presentation::Chat)
                ),
            };
            let chat = matches!(
                description.presentation,
                Some(otrios_interfaces::canvas::Presentation::Chat)
            );
            let aggregate_view = matches!(
                description.presentation,
                Some(otrios_interfaces::canvas::Presentation::View { .. })
            );
            if packages
                .insert(
                    a.ui_ref.clone(),
                    Package {
                        bundle_sha256: verified.as_ref().map(|v| v.summary().digest.clone()),
                        library_locks: verified
                            .as_ref()
                            .map_or_else(Vec::new, |v| v.summary().info.libraries.clone()),
                        code_bytes: verified.as_ref().map(|v| v.summary().bytes as u64),
                        revision: verified.as_ref().map(|v| v.summary().info.revision),
                        app: a,
                        ui,
                        ui_hash: entry.ui_hash,
                        code,
                        private,
                        signal_fields,
                        selection,
                        schema: description.state_schema,
                        policy,
                        chat,
                        aggregate_view,
                        ink: matches!(
                            description.presentation,
                            Some(otrios_interfaces::canvas::Presentation::TouchPad { .. })
                        ),
                        primary_ref: String::new(),
                    },
                )
                .is_some()
            {
                return Err("Duplicate UI reference".into());
            }
        }
        let mut policies = BTreeMap::new();
        for p in packages.values().filter(|p| !p.app.id.contains('/')) {
            let owned = packages
                .values()
                .filter(|other| other.app.id.split('/').next() == Some(p.app.id.as_str()))
                .collect::<Vec<_>>();
            let roots = owned
                .iter()
                .map(|p| otrios_interfaces::spaces::RootSchema {
                    state_ref: p.app.state_ref.clone(),
                    schema: p.schema.clone(),
                })
                .collect::<Vec<_>>();
            let policy = otrios_interfaces::spaces::AppPolicy {
                app: p.app.id.clone(),
                schema: otrios_core::spaces::collection_schema(&roots)
                    .map_err(|e| e.to_string())?,
                closed_only: owned.iter().any(|p| p.private),
                editable: owned.iter().any(|p| p.policy.editable || p.chat),
                comments: owned.iter().any(|p| p.chat),
            };
            policies.insert(p.app.id.clone(), (policy, p.app.state_ref.clone()));
        }
        for p in packages.values_mut() {
            let owner = p.app.id.split('/').next().ok_or("Missing app owner")?;
            let (policy, primary_ref) = policies.get(owner).ok_or("Missing admitted app owner")?;
            p.policy = policy.clone();
            p.primary_ref = primary_ref.clone();
        }
        // Optional catalog history must not keep an unbounded sequence of
        // library bodies. Retain each app's candidate and one prior exact lock;
        // an older catalog request must discover/remount the current release.
        // Active session snapshots have their own bounded Merkle resync policy.
        let previous_pins: BTreeMap<_, _> = self
            .bundles
            .get(&self.active)
            .into_iter()
            .flat_map(|bundle| &bundle.packages)
            .map(|(ui, package)| (ui.clone(), package.library_locks.clone()))
            .collect();
        for old in self.bundles.values_mut() {
            old.packages.retain(|ui, package| {
                let Some(candidate) = packages.get(ui) else {
                    return true;
                };
                if candidate.library_locks.is_empty() && package.library_locks.is_empty() {
                    return true;
                }
                package.library_locks == candidate.library_locks
                    || previous_pins
                        .get(ui)
                        .is_some_and(|pins| pins == &package.library_locks)
            });
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
        let managed_refs = self
            .spaces
            .as_ref()
            .map(|store| {
                store.registry().map(|r| {
                    r.spaces
                        .iter()
                        .flat_map(|s| s.apps.iter())
                        .map(|a| (a.collection.clone(), a.policy.app.clone()))
                        .collect::<BTreeMap<_, _>>()
                })
            })
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        for (state_ref, room) in &mut self.rooms {
            if let Some(owner) = managed_refs.get(state_ref.split(':').next().unwrap_or(state_ref))
            {
                let fields = self
                    .bundles
                    .values()
                    .flat_map(|b| b.packages.values())
                    .filter(|p| {
                        p.app.id.split('/').next() == Some(owner.as_str())
                            && &p.schema == room.doc.schema()
                            && p.app.state_ref == room.doc.scope()
                    })
                    .flat_map(|p| p.signal_fields.iter().copied())
                    .collect::<std::collections::BTreeSet<_>>();
                room.doc
                    .set_signal_fields(&fields.into_iter().collect::<Vec<_>>())?;
                continue;
            }
            let fields = retained
                .get(state_ref)
                .map(|ids| ids.iter().copied().collect::<Vec<_>>())
                .unwrap_or_default();
            room.doc.set_signal_fields(&fields)?;
        }
        self.active = catalog_hash;
        self.active_release = id.into();
        if self.spaces.is_some() {
            self.finish_install()?;
        }
        self.discovery_catalog_changed()?;
        Ok(())
    }
    pub(super) fn catalog_bytes(&self) -> Result<Vec<u8>> {
        bounded(&self.source.join("catalog.flex"), canvas::MAX_BYTES)
    }
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.bundles.len(), self.rooms.len())
    }
    pub(super) fn ink_subscribers(&self) -> usize {
        self.ink_rooms.values().map(|room| room.subscribers()).sum()
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
        scope: &spaces::Scope,
    ) -> Result<(Vec<ServerMessage>, bool)> {
        let _profile = canvas::profiling::span("authority.process");
        let mut messages = leases.revoke_private(allow_private);
        messages.extend(self.revoke_scoped(leases, scope));
        if leases.retired.len() > 2 {
            return Err("Revoked mounts were not discarded".into());
        }
        let mut changed = false;
        match message {
            ClientMessage::Mount {
                epoch,
                ui_ref,
                state_ref,
                catalog_hash,
                known_ui,
                known_code,
                writer,
                resume,
                collection,
            } => {
                if epoch == 0
                    || epoch <= leases.last_epoch
                    || known_code.len() > 8
                    || known_ui.as_ref().is_some_and(|h| h.len() != 32)
                    || known_code.iter().any(|h| h.len() != 32)
                {
                    return Err("Invalid Canvas mount generation/cache".into());
                }
                leases.last_epoch = epoch;
                leases.staged = None;
                let p = self.package(&catalog_hash, &ui_ref)?;
                let binding = if self.spaces.is_some() {
                    match self.binding(scope, &p) {
                        Ok(binding) => Some(binding),
                        Err(_) => {
                            messages.push(ServerMessage::AccessRequired { epoch });
                            return Ok((messages, false));
                        }
                    }
                } else {
                    None
                };
                if p.private && !allow_private {
                    messages.push(ServerMessage::AccessRequired { epoch });
                    return Ok((messages, false));
                }
                if state_ref != p.app.state_ref {
                    return Err("State root is not granted by app catalog".into());
                }
                let room_key = if let Some(binding) = &binding {
                    self.scoped_room(binding, &p)?;
                    if p.aggregate_view {
                        self.load_collection_view(&binding.collection)?;
                    }
                    collection_room(binding, &p)
                } else {
                    state_ref.clone()
                };
                let mount_binding = binding
                    .as_ref()
                    .map(|b| -> Result<_> {
                        use otrios_interfaces::spaces::Operation;
                        Ok(otrios_interfaces::canvas::MountBinding {
                            version: 2,
                            actor: otrios_core::spaces::actor_context(
                                self.registry().map_err(|e| e.to_string())?,
                                scope.identity.as_deref(),
                                b,
                            )
                            .map_err(|e| e.to_string())?,
                            collection: b.collection.clone(),
                            editable: self.allowed(scope, b, Operation::Edit),
                            ui_editable: canvas::oib::seed(&canvas::decode_ui(&p.ui)?).is_some()
                                && self.ui_allowed(scope, b),
                            comments: self.allowed(scope, b, Operation::Comment),
                            moderate: self.allowed(scope, b, Operation::Moderate),
                        })
                    })
                    .transpose()?;
                let grant = Grant {
                    principal: scope.identity.clone(),
                    actions: binding
                        .as_ref()
                        .map_or(0, |binding| self.actions(scope, binding)),
                };
                let prepared_chat = if p.chat {
                    binding
                        .as_ref()
                        .map(|binding| self.prepare_chat(binding, &p))
                        .transpose()?
                } else {
                    None
                };
                let initial_view = if p.aggregate_view {
                    Some(if let Some(b) = &binding {
                        self.collection_views
                            .get(&b.collection)
                            .ok_or("Missing collection presentation")?
                            .1
                            .clone()
                    } else {
                        self.view.clone()
                    })
                } else if let Some(chat) = &prepared_chat {
                    Some(chat.chat.prepare()?)
                } else {
                    None
                };
                // Resume receipts belong to the admitted collection. A replacement
                // collection starts at zero even when the app/state references match.
                let resume = if binding
                    .as_ref()
                    .is_some_and(|b| collection.as_deref() != Some(&b.collection))
                {
                    0
                } else {
                    resume
                };
                let room = self
                    .rooms
                    .get_mut(&room_key)
                    .ok_or("Missing app State root")?;
                if room.failed {
                    return Err("App persistence outcome uncertain; restart required".into());
                }
                let mut candidate = room.doc.clone();
                let oib_seed_changed =
                    configure_package_oib(&mut candidate, &canvas::decode_ui(&p.ui)?)?;
                // Baseline adoption preserves every receipt/app register. Persist
                // that operation separately before registering a new writer.
                if oib_seed_changed {
                    if let Some(file) = &room.file
                        && let Err(error) = store::append(file, &room.doc, &candidate)
                    {
                        room.failed = true;
                        return Err(error.to_string());
                    }
                    room.doc = candidate.clone();
                }
                candidate.set_wall_time(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                );
                let (mut sync, changed) = if mount_binding.as_ref().is_some_and(|b| !b.editable)
                    || !p.schema.fields.iter().any(|field| field.writable)
                {
                    (candidate.open_read_session(&writer)?, false)
                } else {
                    candidate.open_session(&writer, resume)?
                };
                sync.set_oib_editable(mount_binding.as_ref().is_some_and(|b| b.ui_editable));
                sync.set_selection(p.selection.clone())?;
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
                    ui: if !p.private && known_ui.as_ref() == Some(&p.ui_hash) {
                        vec![]
                    } else {
                        p.ui
                    },
                    ui_hash: p.ui_hash,
                    code: if known_code.contains(&canvas::hash(&p.code)) {
                        vec![]
                    } else {
                        p.code
                    },
                    state,

                    binding: mount_binding,
                    initial_view,
                });
                leases.staged = Some(Lease {
                    epoch,
                    state_ref: room_key,
                    private: p.private,
                    sync,
                    view_id: vec![],
                    binding,
                    grant,
                    _room: room.lease.clone(),
                    _chat: prepared_chat,
                    _presence: None,
                    ink: p.ink,
                    chat: p.chat,
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
                self.refresh_presence()?;
                if self.presence.len() >= 512 {
                    return Err("Presence capacity reached".into());
                }
                leases.active = leases.staged.take();
                if let Some(lease) = &mut leases.active
                    && let Some(binding) = &lease.binding
                {
                    self.next_presence = self
                        .next_presence
                        .checked_add(1)
                        .ok_or("Presence counter exhausted")?;
                    let presence = Arc::new(Presence {
                        scope: scope.clone(),
                        binding: binding.clone(),
                        grant: lease.grant.clone(),
                        anonymous: self.next_presence,
                    });
                    self.presence.push(Arc::downgrade(&presence));
                    lease._presence = Some(presence);
                    self.discovery = otrios_core::spaces::advance_discovery(&self.discovery)
                        .map_err(|e| e.to_string())?;
                    changed = true;
                }
                if leases.active.as_ref().is_some_and(|l| l.binding.is_some()) {
                    messages.push(ServerMessage::Activated { epoch });
                }
            }
            ClientMessage::CancelMount { epoch } => {
                if leases.staged.as_ref().is_some_and(|l| l.epoch == epoch) {
                    leases.staged = None;
                }
                return Ok((messages, self.refresh_presence()?));
            }
            ClientMessage::Sync { epoch, bytes } => {
                // Retired epochs cannot decode or mutate the replacement. Keep
                // the shared socket usable while disposal acknowledgement drains.
                if leases.retired.iter().any(|old| old.epoch == epoch)
                    || leases
                        .active
                        .as_ref()
                        .is_some_and(|live| live.epoch != epoch)
                {
                    return Ok((messages, false));
                }
                if let Some(binding) = leases.active.as_ref().and_then(|l| l.binding.as_ref()) {
                    if matches!(
                        canvas::decode_state_request(&bytes)?,
                        otrios_interfaces::canvas::StateRequest::OibWrite { .. }
                            | otrios_interfaces::canvas::StateRequest::OibInstance { .. }
                            | otrios_interfaces::canvas::StateRequest::OibApply { .. }
                    ) && !self.ui_allowed(scope, binding)
                    {
                        return Err("OIB editing requires Space Admin".into());
                    }
                    // ACK/resync are reads; only actual write requests require Edit.
                    if matches!(
                        canvas::decode_state_request(&bytes)?,
                        otrios_interfaces::canvas::StateRequest::Write { .. }
                    ) && !self.allowed(
                        scope,
                        binding,
                        otrios_interfaces::spaces::Operation::Edit,
                    ) {
                        return Err("Space edit denied".into());
                    }
                }
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
                leases.retired.retain(|lease| lease.epoch != epoch);
                if leases.active.as_ref().is_some_and(|l| l.epoch == epoch) {
                    leases.active = None;
                }
                if leases.staged.as_ref().is_some_and(|l| l.epoch == epoch) {
                    leases.staged = None;
                }
                return Ok((messages, false));
            }
        }
        messages.extend(self.updates(leases, allow_private, scope)?);
        Ok((messages, changed))
    }
    fn updates(
        &mut self,
        leases: &mut Leases,
        allow_private: bool,
        scope: &spaces::Scope,
    ) -> Result<Vec<ServerMessage>> {
        let _profile = canvas::profiling::span("authority.updates");
        let mut messages = leases.revoke_private(allow_private);
        messages.extend(self.revoke_scoped(leases, scope));
        if leases.retired.len() > 2 {
            return Err("Revoked mounts were not discarded".into());
        }
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
        if let Some(binding) = &l.binding
            && binding.app == "monitoring"
            && let Some((hash, bytes)) = self.collection_views.get(&binding.collection)
            && &l.view_id != hash
        {
            messages.push(ServerMessage::View {
                epoch: l.epoch,
                bytes: bytes.clone(),
            });
            l.view_id = hash.clone();
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

/// Bounded state worker. The session/control loop never acquires this worker's
/// authority lock or decodes a state packet on its behalf.
pub(super) struct Packet {
    pub bytes: Vec<u8>,
    pub private: bool,
    pub binding: Option<otrios_interfaces::spaces::Binding>,
    pub grant: Option<Grant>,
    pub activated: Option<Box<Activation>>,
}
pub(super) async fn private_allowed(app: &App, public: bool, key: &Option<String>) -> bool {
    if !public {
        return true;
    }
    match (&app.auth, key) {
        (Some(a), Some(k)) => a.valid(k).await,
        _ => false,
    }
}
async fn emit(
    output: &tokio::sync::mpsc::Sender<Packet>,
    message: ServerMessage,
    private: bool,
    binding: Option<otrios_interfaces::spaces::Binding>,
    grant: Option<Grant>,
    activated: Option<Activation>,
) -> bool {
    let Ok(mut bytes) = canvas::encode_server(&message) else {
        return false;
    };
    // UI/code mounts are bulk transfers. Keep interactive state deltas raw;
    // sync-v5 already supports independent bounded LZ4 blocks on both clients.
    if matches!(message, ServerMessage::Mounted { .. }) && bytes.len() >= 8 * 1024 {
        let mut compressed = Vec::new();
        if canvas::compress_packet_into(&bytes, &mut compressed).is_err() {
            return false;
        }
        bytes = compressed;
    }
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            output.send(Packet {
                bytes,
                private,
                binding,
                grant,
                activated: activated.map(Box::new),
            })
        )
        .await,
        Ok(Ok(()))
    )
}
pub(super) async fn worker(
    app: Arc<App>,
    public: bool,
    key: Option<String>,
    space: Option<String>,
    mut input: tokio::sync::mpsc::Receiver<Vec<u8>>,
    output: tokio::sync::mpsc::Sender<Packet>,
    activation: watch::Sender<Option<Activation>>,
) {
    let mut leases = Leases::default();
    let mut notice = app.changed.subscribe();
    let mut expiry = tokio::time::interval(Duration::from_secs(1));
    let mut window = std::time::Instant::now();
    let mut count = 0;
    loop {
        let incoming = tokio::select! {
            value=input.recv()=>match value{Some(v)=>Some(v),None=>return},
            c=notice.changed()=>{if c.is_err(){return;}None},
            _=expiry.tick()=>None,
        };
        let allowed = private_allowed(&app, public, &key).await;
        let mut scope = spaces::Scope {
            space: space.clone(),
            identity: spaces::identity(&app, &key).await,
        };
        for revoked in leases.revoke_private(allowed) {
            leases.publish(&activation);
            if !emit(&output, revoked, false, None, None, None).await {
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
                return;
            }
            let release = app.release.read().await.clone();
            let mut guard = app.apps.lock().await;
            scope.identity = spaces::identity(&app, &key).await;
            let Some(hub) = guard.as_mut() else {
                continue;
            };
            let result = canvas::decode_client(&bytes).and_then(|message| {
                recoverable_epoch = match &message {
                    ClientMessage::Mount { epoch, .. }
                    | ClientMessage::AcceptMount { epoch }
                    | ClientMessage::CancelMount { epoch } => Some(*epoch),
                    _ => None,
                };
                if matches!(message, ClientMessage::Mount { .. }) {
                    hub.refresh(&app.releases, &release)?;
                }
                hub.process(&mut leases, message, allowed, &scope)
            });
            leases.publish(&activation);
            result
        } else {
            let mut guard = app.apps.lock().await;
            scope.identity = spaces::identity(&app, &key).await;
            let result = match guard.as_mut() {
                Some(hub) => hub
                    .updates(&mut leases, allowed, &scope)
                    .map(|m| (m, false)),
                None => continue,
            };
            leases.publish(&activation);
            result
        };
        super::profile_trace::flush();
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
                        | ServerMessage::Activated { epoch }
                        | ServerMessage::Sync { epoch, .. }
                        | ServerMessage::View { epoch, .. }
                        | ServerMessage::AccessRequired { epoch }
                        | ServerMessage::Error { epoch, .. } => *epoch,
                    };
                    let private = private_epochs.contains(&epoch);
                    if private && !private_allowed(&app, public, &key).await {
                        for revoked in leases.revoke_private(false) {
                            leases.publish(&activation);
                            if !emit(&output, revoked, false, None, None, None).await {
                                return;
                            }
                        }
                        continue;
                    }
                    let binding = [leases.active.as_ref(), leases.staged.as_ref()]
                        .into_iter()
                        .flatten()
                        .find(|l| l.epoch == epoch)
                        .and_then(|l| l.binding.clone());
                    let grant = [leases.active.as_ref(), leases.staged.as_ref()]
                        .into_iter()
                        .flatten()
                        .find(|lease| lease.epoch == epoch && lease.binding.is_some())
                        .map(|lease| lease.grant.clone());
                    let activated = if matches!(message, ServerMessage::Activated { .. }) {
                        leases.activation()
                    } else {
                        None
                    };
                    if !emit(&output, message, private, binding, grant, activated).await {
                        return;
                    }
                }
            }
            Err(message) => {
                let epoch = recoverable_epoch
                    .unwrap_or_else(|| leases.active.as_ref().map_or(1, |l| l.epoch));
                if !emit(
                    &output,
                    ServerMessage::Error { epoch, message },
                    false,
                    None,
                    None,
                    None,
                )
                .await
                {
                    return;
                }
                if recoverable_epoch.is_none() {
                    return;
                }
            }
        }
    }
}
