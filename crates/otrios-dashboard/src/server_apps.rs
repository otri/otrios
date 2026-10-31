// SPDX-License-Identifier: Apache-2.0
//! One authority, independently persisted app roots and private admission before payloads.
use super::*;
use otrios_core::canvas::{self, Behavior, Document, Session};
use otrios_interfaces::canvas::{App as AppContract, ClientMessage, ServerMessage};
use otrios_interfaces::session::STATE_PROTOCOL;
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
    selection: canvas::StateSelection,
    schema: otrios_interfaces::canvas::StateSchema,
    policy: otrios_interfaces::spaces::AppPolicy,
    chat: bool,
    aggregate_view: bool,
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
    publisher: Option<[u8; 32]>,
    spaces: Option<otrios_core::spaces::Store>,
    collector: Option<otrios_interfaces::spaces::ServiceBinding>,
    collection_views: BTreeMap<String, (Vec<u8>, Vec<u8>)>,
    provisioning: bool,
}
struct Lease {
    epoch: u32,
    state_ref: String,
    private: bool,
    sync: Session,
    view_id: Vec<u8>,
    binding: Option<otrios_interfaces::spaces::Binding>,
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
            publisher: None,
            spaces: None,
            collector: None,
            collection_views: BTreeMap::new(),
            provisioning: false,
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
        Ok(())
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
            .and_then(|b| b.packages.values().find(|p| p.app.id == app))
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
        let path = self
            .state
            .as_ref()
            .ok_or("Durable app storage required")?
            .join(format!("collection-{}.merkle", binding.collection));
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
        Ok(
            json!({"collection":binding.collection,"source":package.app.state_ref,"source_root":canvas::hex(&root)}),
        )
    }
    pub(super) fn registry(
        &self,
    ) -> std::result::Result<&otrios_interfaces::spaces::Registry, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .registry()
    }
    pub(super) fn manage_space(
        &mut self,
        actor: &str,
        request: &otrios_interfaces::spaces::Request,
        policy: Option<&otrios_interfaces::spaces::AppPolicy>,
    ) -> std::result::Result<u64, otrios_core::spaces::Error> {
        Ok(self
            .spaces
            .as_mut()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .commit(actor, request, policy)?
            .generation)
    }
    pub(super) fn app_policy(&self, app: &str) -> Result<otrios_interfaces::spaces::AppPolicy> {
        let p = self
            .bundles
            .get(&self.active)
            .and_then(|b| b.packages.values().find(|p| p.app.id == app))
            .ok_or("Installed app unavailable")?;
        // A global chat/collection lane is never admitted into a scoped mount.
        // Its collection-owned adapter is introduced separately in this milestone.
        if p.chat {
            return Err("Scoped chat adapter unavailable".into());
        }
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
        let schema = canvas::hex(&canvas::hash(&canvas::encode_schema(&package.schema)?));
        let registry = self.registry().map_err(|e| e.to_string())?;
        let binding = otrios_core::spaces::bind(
            self.registry().map_err(|e| e.to_string())?,
            scope.identity.as_deref(),
            scope.space.as_deref().ok_or("Select a Space")?,
            owner,
            &schema,
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
    fn revoke_scoped(&self, leases: &mut Leases, scope: &spaces::Scope) -> Vec<ServerMessage> {
        let mut messages = vec![];
        for slot in [&mut leases.active, &mut leases.staged] {
            if slot
                .as_ref()
                .and_then(|l| l.binding.as_ref())
                .is_some_and(|b| {
                    !self.allowed(scope, b, otrios_interfaces::spaces::Operation::Read)
                })
            {
                let lease = slot.take().expect("present lease");
                messages.push(ServerMessage::AccessRequired { epoch: lease.epoch });
            }
        }
        messages
    }
    fn scoped_room(
        &mut self,
        binding: &otrios_interfaces::spaces::Binding,
        package: &Package,
    ) -> Result<()> {
        if self.rooms.contains_key(&binding.collection) {
            return Ok(());
        }
        if self.rooms.len() >= 256 {
            return Err("App root budget exhausted".into());
        }
        let file = self
            .state
            .as_ref()
            .map(|root| root.join(format!("collection-{}.merkle", binding.collection)));
        let mut doc = match file.as_ref().filter(|p| p.exists()) {
            Some(path) => store::load(
                path,
                Document::load(
                    package.schema.clone(),
                    &package.app.state_ref,
                    &bounded(path, canvas::MAX_BYTES)?,
                )?,
            )?,
            None => Document::new(package.schema.clone(), &package.app.state_ref)?,
        };
        doc.set_signal_fields(&package.signal_fields)?;
        if let Some(file) = &file
            && !file.exists()
        {
            persist(file, &doc.save()?).map_err(|e| e.to_string())?;
        }
        self.rooms.insert(
            binding.collection.clone(),
            Room {
                doc,
                file,
                failed: false,
            },
        );
        Ok(())
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
            if self.spaces.is_none() || self.provisioning {
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
                        store::load(
                            path,
                            Document::load(
                                description.state_schema.clone(),
                                &a.state_ref,
                                &bounded(path, canvas::MAX_BYTES)?,
                            )?,
                        )?
                    } else {
                        Document::new(description.state_schema.clone(), &a.state_ref)?
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
            }
            let policy = otrios_interfaces::spaces::AppPolicy {
                app: a.id.clone(),
                schema: canvas::hex(&canvas::hash(&canvas::encode_schema(
                    &description.state_schema,
                )?)),
                closed_only: private,
                editable: description.state_schema.fields.iter().any(|f| f.writable),
                comments: false,
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
            if let Some(owner) = managed_refs.get(state_ref) {
                let fields = self
                    .bundles
                    .values()
                    .flat_map(|b| b.packages.values())
                    .filter(|p| {
                        p.app.id.split('/').next() == Some(owner.as_str())
                            && &p.schema == room.doc.schema()
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
        scope: &spaces::Scope,
    ) -> Result<(Vec<ServerMessage>, bool)> {
        let mut messages = leases.revoke_private(allow_private);
        messages.extend(self.revoke_scoped(leases, scope));
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
                    binding.collection.clone()
                } else {
                    state_ref.clone()
                };
                let mount_binding = binding.as_ref().map(|b| {
                    use otrios_interfaces::spaces::Operation;
                    otrios_interfaces::canvas::MountBinding {
                        version: 1,
                        collection: b.collection.clone(),
                        editable: self.allowed(scope, b, Operation::Edit),
                        comments: self.allowed(scope, b, Operation::Comment),
                        moderate: self.allowed(scope, b, Operation::Moderate),
                    }
                });
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
                candidate.set_wall_time(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                );
                let (mut sync, changed) = if mount_binding.as_ref().is_some_and(|b| !b.editable) {
                    (candidate.open_read_session(&writer)?, false)
                } else {
                    candidate.open_session(&writer, resume)?
                };
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
                if let Some(binding) = leases.active.as_ref().and_then(|l| l.binding.as_ref()) {
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
        let mut messages = leases.revoke_private(allow_private);
        messages.extend(self.revoke_scoped(leases, scope));
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
                binding
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
        let scope = spaces::Scope {
            space: space.clone(),
            identity: spaces::identity(&app, &key).await,
        };
        for revoked in leases.revoke_private(allowed) {
            if !emit(&output, revoked, false, None).await {
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
            let Some(hub) = guard.as_mut() else {
                continue;
            };
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
                hub.process(&mut leases, message, allowed, &scope)
            })
        } else {
            let mut guard = app.apps.lock().await;
            match guard.as_mut() {
                Some(hub) => hub
                    .updates(&mut leases, allowed, &scope)
                    .map(|m| (m, false)),
                None => continue,
            }
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
                        | ServerMessage::AccessRequired { epoch }
                        | ServerMessage::Error { epoch, .. } => *epoch,
                    };
                    let private = private_epochs.contains(&epoch);
                    if private && !private_allowed(&app, public, &key).await {
                        for revoked in leases.revoke_private(false) {
                            if !emit(&output, revoked, false, None).await {
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
                    if !emit(&output, message, private, binding).await {
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
