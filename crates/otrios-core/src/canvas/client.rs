// SPDX-License-Identifier: LGPL-3.0-only
//! One admitted app activation. Durable state is authoritative; all client state is RAM only.
use super::text_viewport::TextViewport;
use super::*;
mod host_access;
mod host_map;
mod host_moves;
mod host_panel;
mod host_transfers;
mod oib_designer;
mod oib_focus_layout;
mod oib_property_editor;
use crate::dashboard::{
    input::{Delivery, InputRouter},
    keyboard::{Keyboard, TextBuffer},
    layout::Layout,
    scene::Scene,
};
use otrios_interfaces::{
    keyboard::Edit,
    layout::Value,
    scene::{Action, PointerEvent, PointerPhase, SceneNode, TouchResponder},
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, OnceLock},
};
#[derive(Clone)]
struct OrderDrag {
    pointer: u32,
    node: u16,
    start: [f32; 2],
    offset: [f32; 2],
    position: [f32; 2],
    moving: bool,
}
struct OibView {
    root: crate::merkle::Hash,
    saving: bool,
    deferred: bool,
    info: super::oib::Info,
}
struct Mount {
    epoch: u32,
    viewport: [f32; 2],
    ui: Ui,
    layout: Layout,
    oib_generation: Option<u64>,
    oib_view: Option<OibView>,
    oib_selected: Option<u64>,
    designer: Option<oib_designer::Designer>,
    doc: StateAdapter,
    behavior: Option<Behavior>,
    input: InputRouter,
    state_ref: String,
    binding: Option<otrios_interfaces::canvas::MountBinding>,
    quit_control: Option<u8>,
    private: bool,
    session_controls: bool,
    private_access: bool,
    scene: OnceLock<Arc<Scene>>,
    layout_scene: OnceLock<Arc<Scene>>,
    visible: BTreeMap<u16, Value>,
    keyboard: Keyboard,
    editors: BTreeMap<u16, TextBuffer>,
    text_views: BTreeMap<u16, TextViewport>,
    text_bounds_dirty: bool,
    text_scroll: Option<super::text_scroll::Scroll>,
    map_scroll: Option<super::text_scroll::Scroll>,
    panel_scroll: Option<super::text_scroll::Scroll>,
    preview_scroll: Option<super::text_scroll::Scroll>,
    host_backdrop: Option<SceneNode>,
    map_drag: Option<host_map::Preview>,
    text_document: Option<super::document::Viewport>,
    chat: Option<super::chat::Client>,
    font: Option<Arc<crate::dashboard::font::Font>>,
    focus: Option<u16>,
    ink: crate::dashboard::ink::Ink,
    now: u64,
    wall: i64,
    stress: u8,
    stress_last: Option<f64>,
    stress_updates: u64,
    stress_workload: Option<StressWorkload>,
    keyboard_pointer: Option<u32>,
    view: Option<Layout>,
    pad_preview: BTreeMap<u16, Value>,
    order_editing: bool,
    order_drag: Option<OrderDrag>,
}
#[derive(Clone, Copy)]
struct AuxScope {
    epoch: u32,
    managed: bool,
}
impl AuxScope {
    fn wrap(self, bytes: Vec<u8>) -> Vec<u8> {
        if self.managed {
            otrios_interfaces::channels::scoped(self.epoch, bytes)
        } else {
            bytes
        }
    }
}
struct Request {
    epoch: u32,
    pair: (String, String),
    sent: bool,
    reconnecting: bool,
}
struct Qualified {
    mount: Mount,
    reconnecting: bool,
    code: Vec<u8>,
    accepted: bool,
}
enum MountDecision {
    Accept(u32),
    Cancel(u32),
}
/// Disposable two-peer CRDT workload. It never shares a document, session or
/// persistence path with the admitted app's authoritative state.
struct StressWorkload {
    source: StateAdapter,
    sink: Document,
    session: Session,
    scope: String,
    field: u16,
    operations: u16,
}
impl StressWorkload {
    fn new(schema: StateSchema, scope: String, field: u16) -> Result<Self> {
        Ok(Self {
            source: StateAdapter::new(schema.clone(), &scope, &[], state::writer_nonce()?)?,
            sink: Document::new(schema, &scope)?,
            session: Session::default(),
            scope,
            field,
            operations: 0,
        })
    }
    fn step(&mut self) -> Result<bool> {
        if self.operations == 256 {
            *self = Self::new(self.sink.schema().clone(), self.scope.clone(), self.field)?;
        }
        let next = (self.source.value(self.field)? + 1) % 1001;
        let changed = self.source.set(&[(self.field, next)])?;
        for _ in 0..8 {
            let a = self.source.generate()?;
            if let Some(bytes) = a.as_ref() {
                self.sink.receive(&mut self.session, bytes)?;
            }
            let b = self.sink.generate(&mut self.session)?;
            if let Some(bytes) = b.as_ref() {
                self.source.receive(bytes)?;
            }
            if a.is_none() && b.is_none() {
                break;
            }
        }
        if self.sink.value(self.field)? != next {
            return Err("Diagnostic Merkle workload did not converge".into());
        }
        self.operations += u16::from(changed);
        Ok(changed)
    }
}
/// Read-only diagnostics sampled by the app owner. These logical byte/capacity
/// counters exclude container overhead, GPU buffers, guest runtime and process RSS.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ResourceUsage {
    pub oib: Option<super::oib::Info>,
    pub chat: Option<super::chat::Info>,
    pub sampled_monotonic_ms: u64,
    pub dispatch: DispatchUsage,
    pub state: StateUsage,
    pub behavior_cache_entries: usize,
    pub behavior_cache_bytes: usize,
    pub ui_cache_entries: usize,
    pub ui_cache_bytes: usize,
    pub ui_cache_hits: u64,
    pub behavior_hash: Option<String>,
    pub reflow_order: Vec<u16>,
    pub reflow_editing: bool,
    pub wire: WireUsage,
    pub text_scroll: Option<super::text_scroll::Info>,
    pub map_scroll: Option<super::text_scroll::Info>,
    pub panel_scroll: Option<super::text_scroll::Info>,
    pub oib_preview_scroll: Option<super::text_scroll::Info>,
    pub text_document: Option<super::document::Stats>,
}
#[derive(Default)]
pub struct Client {
    designer_studio: oib_focus_layout::Studio,
    transfer_caps: super::host_ui::transfers::Capabilities,
    transfer_task: Option<otrios_interfaces::transfers::TransferTask>,
    next_transfer: u32,
    transfer_copy: Option<u32>,
    local_apps: super::device_apps::LocalApps,
    local_uninstall: bool,
    local_apps_offline: bool,
    local_uninstall_disconnected: bool,
    chat_credential: Option<String>,
    chat_new_credential: Option<String>,
    chat_enabled: bool,
    chat_scope: Option<AuxScope>,
    chat_native: bool,
    chat_reduced_motion: bool,
    collection_generation: u64,
    collection_cancel: Option<(AuxScope, crate::collection::Command)>,
    collection_revoked: bool,
    external_keyboard: bool,
    viewport: Option<[f32; 2]>,
    layout_viewport: Option<[f32; 2]>,
    wire_clock: u64,
    decode_budget: DecodeBudget,
    generation: u32,
    requested: Option<Request>,
    qualified: Option<Qualified>,
    decision: Option<MountDecision>,
    active: Option<Mount>,
    cache: VecDeque<(Vec<u8>, Vec<u8>)>,
    // Public, fully qualified documents only. State and private UI are never cached.
    ui_cache: VecDeque<(Vec<u8>, Vec<u8>)>,
    ui_cache_hits: u64,
    local: bool,
    navigation: Option<(String, String)>,
    catalog: Option<Catalog>,
    catalog_hash: Vec<u8>,
    previous_catalog: Option<(Catalog, Vec<u8>)>,
    font: Option<Arc<crate::dashboard::font::Font>>,
    auth_required: bool,
    session_controls: bool,
    private_access: bool,
    sign_out_requested: bool,
    writer: Option<[u8; 16]>,
    dispatch: DispatchPolicy,
    spaces: crate::spaces::Observer,
    host: super::host_ui::Model,
    host_active: bool,
    host_actions: BTreeMap<u8, super::host_ui::HostAction>,
    host_requested: bool,
    host_wire: VecDeque<ClientMessage>,
    space_switch: Option<String>,
    sign_in_requested: bool,
    selected_space: Option<String>,
    managed_transport: bool,
}
impl Client {
    /// Losing the old transport ends the opportunity to confirm its RAM-only
    /// edits. An explicit local uninstall may then discard those intents.
    pub fn set_local_apps_online(&mut self, online: bool) {
        self.local_apps_offline = !online;
        self.local_uninstall_disconnected |= !online && self.local_uninstall;
    }
    /// Apply device-operator metadata, including fail-closed invalidation. Hosts
    /// must keep ticking after an error so an existing execution is stopped.
    pub fn update_local_apps(
        &mut self,
        snapshot: Option<otrios_interfaces::device_apps::Namespace>,
    ) -> Result<bool> {
        let result = self.local_apps.update(snapshot);
        self.local_uninstall = !self.host_active
            && self
                .active
                .as_ref()
                .is_some_and(|m| !self.local_apps.reference(&m.ui.ui_ref));
        self.local_uninstall_disconnected =
            self.local_uninstall && (self.local_uninstall_disconnected || self.local_apps_offline);
        if self.local_uninstall
            || self
                .navigation
                .as_ref()
                .is_some_and(|p| !self.local_apps.reference(&p.0))
        {
            self.navigation = None;
        }
        if self.local_uninstall
            || self
                .requested
                .as_ref()
                .is_some_and(|r| !self.local_apps.reference(&r.pair.0))
        {
            if let Some(request) = self.requested.take().filter(|r| r.sent) {
                self.host_wire.push_back(ClientMessage::CancelMount {
                    epoch: request.epoch,
                });
                // Accept may already be in the transport queue, or Activated
                // in flight. Leave closes that epoch in either authority slot.
                self.host_wire.push_back(ClientMessage::Leave {
                    epoch: request.epoch,
                });
            }
            self.qualified = None;
            self.decision = None;
            self.auth_required = false;
        }
        let changed = if self.host_active {
            self.build_host(true)?;
            true
        } else {
            false
        };
        result.map(|()| changed)
    }
    fn finish_local_uninstall(&mut self) -> Result<bool> {
        if !self.local_uninstall
            || self.input_busy()
            || !self.local_uninstall_disconnected && !self.reload_ready()
        {
            return Ok(false);
        }
        // Preserve the final confirmed authority state; only this participant
        // leaves. A pending launch cannot keep removed code alive indefinitely.
        if let Some(request) = self.requested.take().filter(|r| r.sent) {
            self.host_wire.push_back(ClientMessage::CancelMount {
                epoch: request.epoch,
            });
            self.host_wire.push_back(ClientMessage::Leave {
                epoch: request.epoch,
            });
        }
        self.qualified = None;
        self.decision = None;
        self.navigation = None;
        self.auth_required = false;
        if let Some(mount) = self.active.take().filter(|_| !self.local) {
            self.host_wire
                .push_back(ClientMessage::Leave { epoch: mount.epoch });
        }
        self.local_uninstall = false;
        self.local_uninstall_disconnected = false;
        self.collection_revoked = true;
        if self.spaces.snapshot().is_some() {
            self.build_host(false)?;
        }
        Ok(true)
    }
    pub fn space_managed(&self) -> bool {
        self.managed_transport || self.active.as_ref().is_some_and(|m| m.binding.is_some())
    }
    pub fn configure_spaces(&mut self, selected: Option<&str>) -> Result<()> {
        self.spaces.configure(selected).map_err(err)?;
        self.selected_space = selected.map(str::to_owned);
        Ok(())
    }
    pub fn spaces_snapshot(&self) -> Option<&otrios_interfaces::spaces::Discovery> {
        self.spaces.snapshot()
    }
    pub fn spaces_outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        self.spaces.outgoing().map_err(err)
    }
    fn pending_available(&self) -> bool {
        self.requested.as_ref().is_some_and(|request| {
            if !self.local_apps.reference(&request.pair.0) {
                return false;
            }
            let owner = request
                .pair
                .1
                .strip_prefix("app:")
                .and_then(|reference| reference.split_once('/'))
                .map(|(owner, _)| owner);
            self.spaces
                .snapshot()
                .and_then(|snapshot| snapshot.selected.as_ref())
                .is_some_and(|detail| {
                    detail
                        .apps
                        .iter()
                        .any(|app| Some(app.app.as_str()) == owner && app.launch.is_some())
                })
        })
    }
    pub fn receive_spaces(&mut self, bytes: &[u8]) -> Result<bool> {
        let previous_map = if self.host_active
            && matches!(
                self.host.pane,
                super::host_ui::Pane::Home | super::host_ui::Pane::Spaces
            ) {
            self.spaces.snapshot().map(|s| {
                (
                    s.generation,
                    s.identity.clone(),
                    s.catalog.clone(),
                    s.profile.clone(),
                    s.spaces.clone(),
                )
            })
        } else {
            None
        };
        let changed = self.spaces.receive(bytes).map_err(err)?;
        if self.host.transfer.as_ref().is_some_and(|d| {
            self.spaces.snapshot().is_none_or(|s| {
                s.identity.as_deref() != Some(d.actor.as_str())
                    || !s.spaces.iter().any(|space| {
                        space.id == d.space
                            && space.role == Some(otrios_interfaces::spaces::Role::Admin)
                    })
            })
        }) {
            self.host.transfer = None;
            self.transfer_task = None;
            self.transfer_copy = None;
            if self.host.pane == super::host_ui::Pane::Transfer {
                self.host.pane = super::host_ui::Pane::Home;
            }
            self.host.field_owner = None;
        }
        let map_unchanged = !self.spaces.has_outcome()
            && previous_map.is_some_and(|(generation, identity, catalog, profile, spaces)| {
                self.spaces.snapshot().is_some_and(|s| {
                    s.generation == generation
                        && s.identity == identity
                        && s.catalog == catalog
                        && s.profile == profile
                        && s.spaces == spaces
                })
            });
        self.managed_transport |= self.spaces.snapshot().is_some();
        let mut rendered = false;
        if changed
            && let Some(mount) = &mut self.active
            && let Some(chat) = &mut mount.chat
        {
            if let Some(snapshot) = self.spaces.snapshot() {
                chat.publication_targets(snapshot);
            } else {
                chat.publication_unavailable();
            }
            mount.invalidate_local_scene();
        }
        if self.host_active && self.spaces.snapshot().is_none() {
            self.active = None;
            self.host_active = false;
            self.host_actions.clear();
            self.collection_revoked = true;
            return Ok(true);
        }
        if self.host_requested && self.spaces.unavailable() {
            self.host_requested = false;
            self.open_entry();
            return Ok(true);
        }
        if self.auth_required && self.pending_available() {
            self.retry_mount();
        }
        if self.spaces.snapshot().is_some()
            && (self.host_active
                || self.host_requested
                || self
                    .requested
                    .as_ref()
                    .is_some_and(|r| r.pair.0 == "app:home/ui")
                || self
                    .active
                    .as_ref()
                    .is_some_and(|m| m.ui.ui_ref == "app:home/ui")
                || self.active.is_none() && self.requested.is_none()
                || self.selected_space.is_none() && !self.auth_required
                || self.auth_required
                    && self.spaces.snapshot().is_some_and(|s| s.identity.is_some()))
        {
            if self.host_active {
                if changed && !map_unchanged {
                    self.collection_revoked = true;
                    self.build_host(true)?;
                    rendered = true;
                }
            } else {
                rendered = self.show_spaces()?;
            }
        }
        Ok(changed || rendered)
    }
    pub fn manage_space(&mut self, change: otrios_interfaces::spaces::Change) -> Result<u32> {
        self.spaces.request(change).map_err(err)
    }
    pub fn take_space_outcome(&mut self) -> Option<crate::spaces::Outcome> {
        self.spaces.take_outcome()
    }
    pub fn take_space_switch(&mut self) -> Option<String> {
        self.space_switch.take()
    }
    pub fn select_space(&mut self, space: &str) -> Result<()> {
        self.configure_spaces(Some(space))?;
        self.spaces.forget();
        self.active = None;
        self.qualified = None;
        self.requested = None;
        self.navigation = None;
        self.decision = None;
        self.host_wire.clear();
        self.host_active = false;
        self.host_actions.clear();
        self.host_requested = false;
        self.host.pane = super::host_ui::Pane::Home;
        self.host.message.clear();
        self.host.drag = None;
        self.host.field_owner = None;
        self.auth_required = false;
        self.collection_revoked = true;
        Ok(())
    }
    pub fn take_sign_in(&mut self) -> bool {
        std::mem::take(&mut self.sign_in_requested)
    }
    pub fn spaces_ui(&self) -> bool {
        self.host_active
    }
    pub fn spaces_map(&self) -> bool {
        self.host_active
    }
    /// Describe the active surface without announcing an inert modal backdrop.
    pub fn accessibility_text(&self) -> Result<String> {
        fn visit(node: &SceneNode, backdrop: Option<u32>, text: &mut Vec<String>) {
            if Some(node.id) == backdrop {
                return;
            }
            if let otrios_interfaces::scene::Primitive::Text { value, .. } = &node.primitive {
                text.push(value.clone());
            }
            for child in &node.children {
                visit(child, backdrop, text);
            }
        }
        let backdrop = self
            .active
            .as_ref()
            .and_then(|m| m.host_backdrop.as_ref())
            .map(|n| n.id);
        let mut text = vec![];
        visit(self.scene()?.root(), backdrop, &mut text);
        Ok(text.join(". "))
    }
    pub fn show_spaces(&mut self) -> Result<bool> {
        if self.spaces.snapshot().is_none() {
            return Ok(false);
        }
        if !self.host_active
            && (!self.reload_ready() || self.requested.is_some() || self.decision.is_some())
        {
            // Home is local, but connected edits and finished gestures still
            // precede disposal of the participant's current execution.
            if self.active.is_some() {
                self.host_requested = true;
                return Ok(false);
            }
        }
        if !self.host_active {
            if let Some(request) = self.requested.take()
                && request.sent
            {
                self.host_wire.push_back(ClientMessage::CancelMount {
                    epoch: request.epoch,
                });
            }
            if let Some(mount) = &self.active
                && !self.local
            {
                self.host_wire
                    .push_back(ClientMessage::Leave { epoch: mount.epoch });
            }
            self.qualified = None;
            self.decision = None;
            self.navigation = None;
            self.host.pane = super::host_ui::Pane::Home;
            self.host.page = 0;
        }
        self.host_requested = false;
        self.build_host(false)?;
        Ok(true)
    }
    fn build_host(&mut self, preserve: bool) -> Result<()> {
        self.host.transfer_caps = self.transfer_caps.clone();
        // Only metadata/navigation rebuild the host. Local drag feedback must
        // retain the mount; an external rebuild invalidates the captured drag.
        self.host.drag = None;
        let awaiting_sign_in = self.host_active && self.auth_required;
        let mut draft = if preserve && self.host_active {
            self.typed_value(1)
                .ok()
                .and_then(|v| {
                    if let Value::Text(v) = v {
                        Some(v)
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        if let Some(outcome) = self.spaces.take_outcome() {
            if let crate::spaces::Outcome::AppShared { status, share, .. } = &outcome {
                self.transfer_share_outcome(*status, share.as_ref());
            }
            self.host.background = None;
            match &outcome {
                crate::spaces::Outcome::Unknown { .. } => self.host.form_blocked = true,
                crate::spaces::Outcome::Decided {
                    status: otrios_interfaces::spaces::ManagementStatus::Applied,
                    ..
                } => {
                    if let Some((owner, value)) = self.host.form_pending.take()
                        && self.host.field_owner.as_ref() == Some(&owner)
                    {
                        self.host.form_value = value;
                    }
                }
                _ => {
                    self.host.form_pending = None;
                }
            }
            self.host.message = match outcome {
                crate::spaces::Outcome::AppImported { status, state, .. } => {
                    use otrios_interfaces::{spaces::ManagementStatus as Status, transfers::AppImportPhase as Phase};
                    match (status, state.map(|s| s.phase)) {
                        (Status::Applied, Some(Phase::Applied)) => "",
                        (Status::Applied, Some(Phase::Queued | Phase::Armed)) => "Installing app…",
                        (Status::Applied, Some(Phase::Rejected)) => "The install was cancelled or its review changed. Review the current Space before trying again.",
                        (Status::Denied, _) => "Current Space Admin and host installation authorization are required.",
                        _ => "Installation outcome is unknown. Reconnect to check before trying again.",
                    }.into()
                }
                crate::spaces::Outcome::AppShared { status, share, .. } => {
                    match (status, share) {
                        (otrios_interfaces::spaces::ManagementStatus::Applied, Some(share)) => if self.host.transfer.is_some() { String::new() } else { format!("App shared without data: {}/apps/{}/{}", self.transfer_caps.origin, share.app, share.digest) },
                        (otrios_interfaces::spaces::ManagementStatus::Denied, _) => "Host publication authorization and Space Admin access are required.".into(),
                        _ => "The app changed or sharing is unavailable. Refresh before reviewing it again.".into(),
                    }
                }
                crate::spaces::Outcome::Replacement { status, outcome, .. } => {
                    match (status, outcome) {
                        (otrios_interfaces::spaces::ManagementStatus::Applied, Some(otrios_interfaces::transfers::ReplacementOutcome::Committed)) => "",
                        (otrios_interfaces::spaces::ManagementStatus::Denied, _) => "Admin access is required to check this replacement.",
                        _ => "Replacement outcome is unknown. Refresh the dataset before reviewing another change.",
                    }.into()
                }
                crate::spaces::Outcome::MoveStatus { status, .. } => match status {
                    otrios_interfaces::spaces::ManagementStatus::Applied => "",
                    otrios_interfaces::spaces::ManagementStatus::Conflict => {
                        "The move was not recorded. Review both Spaces before trying again."
                    }
                    otrios_interfaces::spaces::ManagementStatus::Denied => {
                        "Admin access to both Spaces is required."
                    }
                    _ => "Couldn't confirm the move. Reconnect to check its outcome.",
                }
                .into(),
                crate::spaces::Outcome::Unknown { .. } => {
                    "Reply lost. Check the refreshed Space before trying again.".into()
                }
                crate::spaces::Outcome::Decided { status, .. } => match status {
                    otrios_interfaces::spaces::ManagementStatus::Applied => "",
                    otrios_interfaces::spaces::ManagementStatus::Denied => {
                        "Admin access is required."
                    }
                    otrios_interfaces::spaces::ManagementStatus::LastAdmin => {
                        "A Space needs at least one Admin."
                    }
                    otrios_interfaces::spaces::ManagementStatus::Conflict => {
                        "The Space changed. Review it and try again."
                    }
                    otrios_interfaces::spaces::ManagementStatus::Incompatible => {
                        "The retained data needs a compatible app version."
                    }
                    otrios_interfaces::spaces::ManagementStatus::Invalid => {
                        "Check the details and try again."
                    }
                    _ => "The request is unavailable. Try again after reconnecting.",
                }
                .into(),
            };
        }
        let snapshot = self
            .local_apps
            .project(self.spaces.snapshot().ok_or("Spaces are unavailable")?);
        let private = snapshot.identity.is_some();
        if self.host.field_owner.is_some()
            && draft == self.host.form_value
            && !self.spaces.pending()
            && !self.host.form_blocked
        {
            use super::host_ui::Pane;
            let profile_changed = matches!(self.host.pane, Pane::ProfileEdit)
                && snapshot.profile.as_ref().map_or(0, |p| p.revision)
                    != self.host.profile_revision;
            if snapshot.generation > self.host.form_generation || profile_changed {
                let current = match &self.host.pane {
                    Pane::SpaceSettings => snapshot
                        .selected
                        .as_ref()
                        .and_then(|d| snapshot.spaces.iter().find(|s| s.id == d.space))
                        .map(|s| s.name.clone()),
                    Pane::AppSettings(id) => snapshot
                        .selected
                        .as_ref()
                        .and_then(|s| s.apps.iter().find(|a| a.app == *id))
                        .map(|a| a.title.clone()),
                    Pane::Dataset(cid) => snapshot
                        .selected
                        .as_ref()
                        .and_then(|s| s.retained.iter().find(|a| a.collection == *cid))
                        .map(|a| a.title.clone()),
                    Pane::ProfileEdit => snapshot
                        .profile
                        .as_ref()
                        .map(|p| p.name.clone())
                        .or_else(|| snapshot.identity.clone()),
                    _ => None,
                };
                if let Some(current) = current {
                    draft = current.clone();
                    self.host.form_value = current;
                }
                self.host.form_generation = snapshot.generation;
                if matches!(self.host.pane, Pane::ProfileEdit) {
                    self.host.profile_revision =
                        snapshot.profile.as_ref().map_or(0, |p| p.revision);
                }
            }
        }
        if let Some((space, pane, app)) = self.host.destination.clone()
            && snapshot.selected.as_ref().is_some_and(|s| s.space == space)
        {
            self.host.destination = None;
            self.host.pane = pane;
            self.host.field_owner = None;
            self.host.background = None;
            let app = app.or_else(|| {
                if self.host.pane == super::host_ui::Pane::Apps && !self.host.editing {
                    let apps = snapshot
                        .selected
                        .as_ref()?
                        .apps
                        .iter()
                        .filter(|a| a.attached)
                        .collect::<Vec<_>>();
                    if apps.len() == 1 {
                        return Some(apps[0].app.clone());
                    }
                }
                None
            });
            if let Some(app) = app {
                if let Some(launch) = snapshot
                    .selected
                    .as_ref()
                    .and_then(|s| s.apps.iter().find(|a| a.app == app))
                    .and_then(|a| a.launch.clone())
                {
                    self.host_active = false;
                    self.host_actions.clear();
                    self.open(&launch.ui_ref, &launch.state_ref);
                    return Ok(());
                }
                self.host.message = "This app is unavailable on this device.".into();
            }
        }
        self.prepare_host_move(&snapshot)?;
        let host_viewport = self
            .viewport
            .or(self.layout_viewport)
            .unwrap_or([1280., 800.]);
        let rendered = super::host_ui::render(
            &snapshot,
            &mut self.host,
            &draft,
            !self.chat_native,
            !self.chat_native,
            host_viewport,
            self.font.as_deref(),
        )?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Canvas epoch exhausted")?;
        let writer = match self.writer {
            Some(writer) => writer,
            None => {
                let writer = state::writer_nonce()?;
                self.writer = Some(writer);
                writer
            }
        };
        let mut mount = qualify(
            rendered.ui,
            "host:spaces/state",
            &[],
            &self.cache,
            &[],
            Qualification {
                epoch: self.generation,
                private,
                writer,
            },
        )?;
        mount.host_backdrop = rendered.backdrop;
        mount.font = self.font.clone();
        mount.session_controls = self.session_controls;
        mount.private_access = self.private_access;
        mount.viewport = host_viewport;
        mount.keyboard.set_enabled(!self.external_keyboard);
        mount.invalidate();
        self.prepare_map_scroll(&mut mount, rendered.map_viewport)?;
        self.prepare_panel_scroll(&mut mount, rendered.panel_viewport)?;
        cached_scene(&mount)?;
        self.active = Some(mount);
        self.host_active = true;
        self.local_uninstall = false;
        self.host_actions = rendered.actions;
        self.local = true;
        self.auth_required = awaiting_sign_in;
        self.requested = None;
        Ok(())
    }
    fn host_action(&mut self, action: super::host_ui::HostAction) -> Result<bool> {
        use super::host_ui::{HostAction, Pane};
        self.host.message.clear();
        match action {
            action @ (HostAction::TransferUpload
            | HostAction::TransferShare(_)
            | HostAction::TransferExport(..)
            | HostAction::TransferReplace(..)
            | HostAction::TransferPick
            | HostAction::TransferUrl
            | HostAction::TransferFile(_)
            | HostAction::TransferCommit
            | HostAction::TransferCancel
            | HostAction::TransferCopy) => return self.host_transfer_action(action),
            HostAction::Pane(pane) => {
                if !matches!(
                    pane,
                    Pane::Members | Pane::AccessAudience | Pane::AudienceConfirm { .. }
                ) {
                    self.host.access = None;
                }
                self.host.panel_scroll = 0.;
                if !matches!(pane, Pane::AppSettings(_)) {
                    self.commit_host_label()?;
                }
                self.host.review = None;
                self.host.moving = None;
                self.host.remove_data = false;
                self.host.page = if matches!(pane, Pane::Home | Pane::Spaces) {
                    self.host.map_page
                } else {
                    0
                };
                self.host.pane = pane;
                self.host.field_owner = None;
                self.host.background = None;
                self.build_host(false)?;
            }
            HostAction::Edit => {
                self.host.editing = !self.host.editing;
                self.host.drag = None;
                self.build_host(false)?;
            }
            HostAction::MapSpace(space) => {
                let app = self
                    .spaces
                    .snapshot()
                    .and_then(|s| s.spaces.iter().find(|s| s.id == space))
                    .filter(|s| s.attached_count == 1)
                    .and_then(|s| s.apps.first())
                    .map(|a| a.app.clone());
                let pane = if self.host.editing {
                    Pane::SpaceSettings
                } else {
                    Pane::Apps
                };
                self.host_destination(space, pane, if self.host.editing { None } else { app })?;
            }
            HostAction::SpaceBody(space) => {
                if !self.host.editing {
                    return self.host_action(HostAction::MapSpace(space));
                }
            }
            HostAction::MapApp(space, app) => {
                if self.host.editing {
                    self.host_destination(space, Pane::AppSettings(app), None)?;
                } else {
                    self.host_destination(space, Pane::Home, Some(app))?;
                }
            }
            HostAction::MapDataset(space, _, collection) => {
                self.host_destination(space, Pane::Dataset(collection), None)?;
            }
            HostAction::MoveDestination(space) => {
                if let Some(moving) = &mut self.host.moving {
                    moving.destination = Some(space.clone());
                    self.host_destination(space, Pane::MoveReview, None)?;
                }
            }
            HostAction::MoveMode(mode) => {
                if let Some(moving) = &mut self.host.moving
                    && moving.attached
                {
                    moving.mode = mode;
                }
                self.build_host(true)?;
            }
            HostAction::MoveConfirm => return self.confirm_host_move(),
            HostAction::ProfileSave => {
                let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
                let revision = snapshot.profile.as_ref().map_or(0, |p| p.revision);
                let name = match self.typed_value(1)? {
                    Value::Text(name) => name,
                    _ => String::new(),
                };
                if snapshot.identity.is_none() || revision != self.host.profile_revision {
                    self.host.message = "The profile changed. Close and review it again.".into();
                } else if name.trim() == self.host.form_value
                    || self
                        .spaces
                        .request(otrios_interfaces::spaces::Change::Profile {
                            name,
                            expected_revision: revision,
                        })
                        .is_ok()
                {
                    self.host.pane = Pane::Profile;
                    self.host.field_owner = None;
                } else {
                    self.host.message = "Check the name and wait for the current request.".into();
                }
                self.build_host(true)?;
            }
            HostAction::BackgroundApply => {
                let text = match self.typed_value(1)? {
                    Value::Text(text) => text,
                    _ => String::new(),
                };
                let hex = text.trim().trim_start_matches('#');
                let color = (hex.len() == 6 && hex.is_ascii())
                    .then(|| {
                        Some([
                            u8::from_str_radix(&hex[..2], 16).ok()?,
                            u8::from_str_radix(&hex[2..4], 16).ok()?,
                            u8::from_str_radix(&hex[4..6], 16).ok()?,
                        ])
                    })
                    .flatten();
                let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
                if let Some(rgb) = color {
                    let space = snapshot
                        .selected
                        .as_ref()
                        .and_then(|d| snapshot.spaces.iter().find(|s| s.id == d.space))
                        .ok_or("Space unavailable")?;
                    let mut layout = space.layout;
                    layout.background = otrios_interfaces::spaces::Background::Rgb(rgb);
                    if snapshot.generation != self.host.form_generation || self.host.form_blocked {
                        self.host.message = "The Space changed. Close and review it again.".into();
                    } else if self
                        .spaces
                        .request(otrios_interfaces::spaces::Change::ConfigureSpace {
                            space: space.id.clone(),
                            name: space.name.clone(),
                            layout,
                        })
                        .is_ok()
                    {
                        self.host.pane = Pane::SpaceSettings;
                        self.host.field_owner = None;
                        self.host.background = Some(layout.background);
                        self.host.message.clear();
                    } else {
                        self.host.message = "Wait for the current request.".into();
                    }
                } else {
                    self.host.message = "Use a six-digit hex color, such as #E7EBE1.".into();
                }
                self.build_host(true)?;
            }

            action @ (HostAction::AccessAudience(_)
            | HostAction::AccessRole(_)
            | HostAction::AccessRemove(_)
            | HostAction::AccessAdd
            | HostAction::AccessSave
            | HostAction::AccessApply) => return self.host_access_action(action),
            HostAction::Settings(space, pane) => {
                self.host.review = None;
                self.host.remove_data = false;
                self.host_destination(space, pane, None)?;
            }
            HostAction::ToggleRemoveData => {
                self.host.remove_data = !self.host.remove_data;
                self.build_host(true)?;
            }
            HostAction::Reviewed(change, generation) => {
                if self
                    .spaces
                    .snapshot()
                    .is_none_or(|s| s.generation != generation)
                {
                    self.host.message = "The Space changed. Close and review it again.".into();
                    self.build_host(true)?;
                } else {
                    return self.host_action(HostAction::Manage(change));
                }
            }
            HostAction::Resize(_) => {}
            HostAction::Background(color) => {
                self.host.background = Some(color);
                self.save_host_space()?;
                self.build_host(true)?;
            }
            HostAction::Unavailable(message) => {
                self.commit_host_label()?;
                self.host.message = message.into();
                self.build_host(true)?;
            }
            HostAction::Page(direction) => {
                self.host.page = self.host.page.saturating_add_signed(isize::from(direction));
                if matches!(self.host.pane, Pane::Home | Pane::Spaces) {
                    self.host.scroll = 0.;
                } else {
                    self.host.panel_scroll = 0.;
                }
                self.build_host(true)?;
            }
            HostAction::Launch(ui, state) => {
                self.host_active = false;
                self.host_actions.clear();
                self.open(&ui, &state);
            }
            HostAction::SignIn => {
                self.sign_in_requested = true;
                self.auth_required = true;
            }
            HostAction::SignOut => {
                self.sign_out_requested = true;
            }
            action => {
                let text = if matches!(action, HostAction::Create) {
                    match self.typed_value(1)? {
                        Value::Text(text) => text,
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                let change = match action {
                    HostAction::Manage(change) => change,
                    HostAction::Create => otrios_interfaces::spaces::Change::Create { name: text },
                    _ => unreachable!(),
                };
                let deleting =
                    matches!(change, otrios_interfaces::spaces::Change::DeleteData { .. });
                let sharing = matches!(change, otrios_interfaces::spaces::Change::Audience { .. });
                let retained = matches!(
                    self.host.pane,
                    Pane::RemoveApp(_)
                        | Pane::DeleteDataset(_)
                        | Pane::RestoreDataset(_)
                        | Pane::DeleteSpace
                        | Pane::Dataset(_)
                );
                match self.spaces.request(change) {
                    Ok(_) => {
                        self.host.message.clear();
                        if retained {
                            self.host.pane = Pane::Home;
                            self.host.review = None;
                        } else if self.host.pane == Pane::Create {
                            self.host.pane = Pane::Spaces;
                        } else if deleting {
                            self.host.pane = Pane::Cleanup;
                        } else if sharing {
                            self.host.pane = Pane::Members;
                        }
                    }
                    Err(_) => {
                        self.host.message =
                            "Check the details and wait for the current request.".into()
                    }
                }
                self.build_host(true)?;
            }
        }
        Ok(true)
    }
    fn host_destination(
        &mut self,
        space: String,
        pane: super::host_ui::Pane,
        app: Option<String>,
    ) -> Result<()> {
        self.host.destination = Some((space.clone(), pane, app));
        if self.selected_space.as_deref() == Some(&space) {
            self.build_host(false)?;
        } else {
            self.space_switch = Some(space);
        }
        Ok(())
    }
    fn host_request_form(&mut self, change: otrios_interfaces::spaces::Change) -> Result<()> {
        if self.host.form_blocked
            || self
                .spaces
                .snapshot()
                .is_none_or(|s| s.generation != self.host.form_generation)
        {
            self.host.message = "The Space changed. Close and review it again.".into();
        } else {
            let name = match &change {
                otrios_interfaces::spaces::Change::LabelApp { name, .. }
                | otrios_interfaces::spaces::Change::LabelDataset { name, .. }
                | otrios_interfaces::spaces::Change::ConfigureSpace { name, .. } => {
                    Some(name.trim().to_owned())
                }
                _ => None,
            };
            if self.spaces.request(change).is_ok() {
                self.host.form_pending =
                    name.and_then(|name| self.host.field_owner.clone().map(|owner| (owner, name)));
                self.host.message.clear();
            } else {
                self.host.message = "Check the name and wait for the current request.".into();
            }
        }
        Ok(())
    }
    fn save_host_space(&mut self) -> Result<()> {
        let Some(space) = self
            .spaces
            .snapshot()
            .and_then(|s| {
                s.selected
                    .as_ref()
                    .and_then(|d| s.spaces.iter().find(|s| s.id == d.space))
            })
            .cloned()
        else {
            return Ok(());
        };
        let name = match self.typed_value(1)? {
            Value::Text(name) => name,
            _ => return Ok(()),
        };
        let mut layout = space.layout;
        layout.background = self.host.background.unwrap_or(layout.background);
        if name != self.host.form_value || layout.background != space.layout.background {
            self.host_request_form(otrios_interfaces::spaces::Change::ConfigureSpace {
                space: space.id,
                name,
                layout,
            })?;
        }
        Ok(())
    }
    fn commit_host_label(&mut self) -> Result<()> {
        if !self.host_active {
            return Ok(());
        }
        if self.spaces.pending()
            && let Some((owner, pending)) = &self.host.form_pending
            && self.host.field_owner.as_ref() == Some(owner)
            && matches!(self.typed_value(1)?, Value::Text(name) if name.trim() == pending)
        {
            return Ok(());
        }
        match self.host.pane.clone() {
            super::host_ui::Pane::SpaceSettings => self.save_host_space()?,
            super::host_ui::Pane::AppSettings(app) => {
                if let Value::Text(name) = self.typed_value(1)?
                    && name != self.host.form_value
                    && let Some(space) = self.selected_space.clone()
                {
                    self.host_request_form(otrios_interfaces::spaces::Change::LabelApp {
                        space,
                        app,
                        name,
                    })?;
                }
            }
            super::host_ui::Pane::Dataset(collection) => {
                if let Value::Text(name) = self.typed_value(1)?
                    && name != self.host.form_value
                    && let Some((space, app)) = self
                        .spaces
                        .snapshot()
                        .and_then(|s| s.selected.as_ref())
                        .and_then(|s| {
                            s.retained
                                .iter()
                                .find(|d| d.collection == collection)
                                .map(|d| (s.space.clone(), d.app.clone()))
                        })
                {
                    self.host_request_form(otrios_interfaces::spaces::Change::LabelDataset {
                        space,
                        app,
                        collection,
                        name,
                    })?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    /// The host supplies text input; retain editor focus without drawing keys.
    pub fn set_software_keyboard(&mut self, enabled: bool) {
        self.external_keyboard = !enabled;
        if let Some(m) = &mut self.active {
            m.keyboard.set_enabled(enabled);
            m.invalidate();
        }
    }
    pub fn chat_info(&self) -> Option<super::chat::Info> {
        self.active
            .as_ref()?
            .chat
            .as_ref()
            .map(super::chat::Client::info)
    }
    pub fn chat_messages(&self) -> Vec<otrios_interfaces::chat::Message> {
        self.active
            .as_ref()
            .and_then(|m| m.chat.as_ref())
            .map_or_else(Vec::new, super::chat::Client::recent_messages)
    }
    pub fn set_chat_native(&mut self, native: bool) {
        self.chat_native = native;
    }
    pub fn set_chat_credential(&mut self, credential: &str) -> Result<()> {
        if !credential.is_empty() && !super::chat::valid_hex(credential, 32) {
            return Err("Invalid guest credential".into());
        }
        self.chat_credential = (!credential.is_empty()).then(|| credential.into());
        Ok(())
    }
    pub fn take_chat_credential(&mut self) -> Option<String> {
        self.chat_new_credential.take()
    }
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.chat_reduced_motion = reduced;
        if let Some(chat) = self.active.as_mut().and_then(|m| m.chat.as_mut()) {
            chat.quiet(reduced);
        }
    }
    pub fn chat_disconnected(&mut self) {
        if let Some(m) = &mut self.active
            && let Some(chat) = &mut m.chat
        {
            chat.disconnected();
            m.invalidate();
        }
    }
    fn aux_payload<'a>(&self, bytes: &'a [u8]) -> Result<Option<&'a [u8]>> {
        if !self.managed_transport && self.active.as_ref().is_none_or(|m| m.binding.is_none()) {
            return Ok(Some(bytes));
        }
        let (epoch, payload) = otrios_interfaces::channels::scope(bytes).map_err(err)?;
        Ok(self
            .active
            .as_ref()
            .filter(|m| m.binding.is_some() && m.epoch == epoch)
            .map(|_| payload))
    }
    pub fn take_chat_upload_request(&mut self) -> Option<u32> {
        let mount = self.active.as_mut()?;
        mount
            .chat
            .as_mut()?
            .take_upload_request()
            .then_some(mount.epoch)
    }
    pub fn send_chat_attachment(&mut self, epoch: u32, name: &str, bytes: &[u8]) -> Result<bool> {
        let Some(mount) = self
            .active
            .as_mut()
            .filter(|mount| mount.epoch == epoch && mount.binding.is_some())
        else {
            return Ok(false);
        };
        let Some(chat) = &mut mount.chat else {
            return Ok(false);
        };
        let changed = chat.upload(name, bytes)?;
        if changed {
            mount.keyboard.hide();
            mount.invalidate();
        }
        Ok(changed)
    }
    pub fn take_chat_download(&mut self) -> Option<(super::chat::Asset, Vec<u8>)> {
        self.active.as_mut()?.chat.as_mut()?.take_download()
    }
    pub fn chat_outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        if self.managed_transport && self.requested.as_ref().is_some_and(|r| r.reconnecting) {
            return Ok(None);
        }
        if let Some(m) = &mut self.active
            && let Some(chat) = &mut m.chat
        {
            if self.managed_transport && m.binding.is_none() {
                return Ok(None);
            }
            let scope = AuxScope {
                epoch: m.epoch,
                managed: m.binding.is_some(),
            };
            self.chat_scope = Some(scope);
            self.chat_enabled = true;
            chat.native(self.chat_native);
            chat.quiet(self.chat_reduced_motion);
            return chat
                .outgoing(if scope.managed {
                    None
                } else {
                    self.chat_credential.clone()
                })
                .map(|bytes| bytes.map(|bytes| scope.wrap(bytes)));
        }
        if self.chat_enabled {
            self.chat_enabled = false;
            if let Some(scope) = self.chat_scope.take() {
                if self.managed_transport && !scope.managed {
                    return Ok(None);
                }
                return super::chat::encode_command(&otrios_interfaces::chat::Command::Leave)
                    .map(|bytes| Some(scope.wrap(bytes)));
            }
        }
        Ok(None)
    }
    pub fn receive_chat(&mut self, bytes: &[u8]) -> Result<bool> {
        let Some(bytes) = self.aux_payload(bytes)? else {
            return Ok(false);
        };
        let event = super::chat::decode_event(bytes)?;
        let cosmetic = matches!(event, otrios_interfaces::chat::Event::Effect { .. });
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        let follow = m.text_document.as_ref().is_none_or(|v| v.at_tail());
        let Some(chat) = &mut m.chat else {
            return Ok(false);
        };
        let old_bounds = (chat.input_height(), chat.draft_height());
        if let Some(token) = chat.receive(event, follow)? {
            self.chat_credential = Some(token.clone());
            self.chat_new_credential = Some(token);
        }
        self.collection_revoked |= std::mem::take(&mut chat.collection_revoked);
        if let Some(viewport) = &mut m.text_document {
            chat.synchronize_document(viewport)?;
        }
        if old_bounds != (chat.input_height(), chat.draft_height()) {
            m.invalidate();
        } else if !cosmetic {
            m.invalidate_local_scene();
        }
        Ok(true)
    }
    pub fn collection_outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        // Reconnect first qualifies a fresh mount epoch. Sending a new lane
        // selection under the retained mount's old epoch would strand its reply.
        if self.managed_transport && self.requested.as_ref().is_some_and(|r| r.reconnecting) {
            return Ok(None);
        }
        if let Some((scope, command)) = self.collection_cancel.take() {
            if self.managed_transport && !scope.managed {
                return Ok(None);
            }
            return crate::collection::encode_command(&command)
                .map(|bytes| Some(scope.wrap(bytes)))
                .map_err(err);
        }
        let Some(mount) = &mut self.active else {
            return Ok(None);
        };
        if self.managed_transport && mount.binding.is_none() {
            return Ok(None);
        }
        let scope = AuxScope {
            epoch: mount.epoch,
            managed: mount.binding.is_some(),
        };
        mount.chat.as_mut().map_or(Ok(None), |chat| {
            chat.collection_outgoing(&mut self.collection_generation)
                .map(|bytes| bytes.map(|bytes| scope.wrap(bytes)))
        })
    }
    pub fn receive_collection(&mut self, bytes: &[u8]) -> Result<bool> {
        let Some(bytes) = self.aux_payload(bytes)? else {
            return Ok(false);
        };
        let Some(mount) = &mut self.active else {
            return Ok(false);
        };
        let Some(chat) = &mut mount.chat else {
            return Ok(false);
        };
        let follow = mount
            .text_document
            .as_ref()
            .is_none_or(|viewport| viewport.at_tail())
            && !chat.more_after;
        let changed = chat.receive_collection(bytes, follow)?;
        self.collection_revoked |= std::mem::take(&mut chat.collection_revoked);
        if let Some(viewport) = &mut mount.text_document {
            chat.synchronize_document(viewport)?;
        }
        if changed {
            mount.invalidate_local_scene();
        }
        Ok(changed)
    }
    pub fn take_collection_revoked(&mut self) -> bool {
        std::mem::take(&mut self.collection_revoked)
    }
    pub fn effect_vertices(&self) -> Result<Vec<f32>> {
        let mut vertices = self
            .active
            .as_ref()
            .and_then(|m| m.chat.as_ref())
            .map_or_else(|| Ok(vec![]), |chat| chat.effect_vertices())?;
        if let Some(m) = &self.active
            && m.keyboard.visible
            && let Some(key) = m.keyboard.feedback_key()
        {
            let scene = self.scene()?;
            if let Some(owner) = scene.controls().into_iter().find(|owner| {
                if m.designer
                    .as_ref()
                    .and_then(|d| d.keyboard_key(owner.node.id))
                    == Some(key)
                {
                    return true;
                }
                let Some(TouchResponder::Activate(Action::AppEvent(control))) =
                    owner.node.responder
                else {
                    return false;
                };
                m.ui.handlers
                    .iter()
                    .any(|h| h.control == control && h.operation == Operation::Key(key))
            }) {
                let [x, y] = owner.origin;
                let w = owner.node.frame.width * owner.scale;
                let h = owner.node.frame.height * owner.scale;
                let inset = 2. * owner.scale;
                let color = [1., 0.86, 0.55, 0.28];
                for [x, y] in [
                    [x + inset, y + inset],
                    [x + w - inset, y + inset],
                    [x + w - inset, y + h - inset],
                    [x + inset, y + inset],
                    [x + w - inset, y + h - inset],
                    [x + inset, y + h - inset],
                ] {
                    vertices.extend([x, y]);
                    vertices.extend(color);
                }
            }
        }
        Ok(vertices)
    }
    pub(super) fn feedback_prepared(&mut self) {
        if let Some(m) = &mut self.active {
            m.keyboard.feedback_prepared(m.now);
        }
    }
    /// Local presentation only. A rejected size retains the old scene and state.
    /// The host's standard layout extent, independent of a chat visual viewport.
    /// Use it to qualify the next non-chat mount while retaining the current one.
    pub fn set_layout_viewport(&mut self, viewport: [f32; 2]) -> Result<()> {
        if viewport
            .iter()
            .any(|v| !v.is_finite() || !(1. ..=8192.).contains(v))
        {
            return Err("Invalid layout viewport".into());
        }
        if self.layout_viewport != Some(viewport) {
            self.layout_viewport = Some(viewport);
            if let Some(m) = &mut self.active
                && let Some(chat) = &mut m.chat
            {
                chat.set_header_width(viewport[0]);
                m.invalidate();
            }
        }
        Ok(())
    }
    pub fn set_viewport(&mut self, viewport: [f32; 2]) -> Result<bool> {
        if viewport
            .iter()
            .any(|v| !v.is_finite() || !(1. ..=8192.).contains(v))
        {
            return Err("Invalid viewport size".into());
        }
        if self.viewport == Some(viewport) {
            return Ok(false);
        }
        if self.host_active {
            self.viewport = Some(viewport);
            self.host.drag = None;
            self.build_host(true)?;
            return Ok(true);
        }
        if let Some(m) = &mut self.active
            && m.designer.is_some()
            && m.oib_activation_blocked()
        {
            if let Some(designer) = &mut m.designer {
                designer.defer_viewport(viewport);
            }
            self.viewport = Some(viewport);
            return Ok(true);
        }
        if let Some(m) = &mut self.active
            && m.layout.blueprint().composition.is_some()
        {
            let old = m.viewport;
            m.viewport = viewport;
            let candidate = resolve_scene(m);
            m.viewport = old;
            candidate?;
            self.cancel()?;
            let m = self.active.as_mut().ok_or("No Canvas")?;
            m.viewport = viewport;
            if let Some(s) = &mut m.text_scroll {
                s.bounds_dirty = true;
            }
            m.invalidate();
            self.viewport = Some(viewport);
            return Ok(true);
        }
        self.viewport = Some(viewport);
        Ok(false)
    }
    pub fn set_dispatch_policy(&mut self, policy: DispatchPolicy) {
        self.dispatch = policy;
        if let Some(m) = &mut self.active {
            m.doc.set_dispatch_policy(policy);
        }
    }
    pub fn set_font(&mut self, font: Arc<crate::dashboard::font::Font>) {
        self.font = Some(font.clone());
        if let Some(candidate) = &mut self.qualified {
            candidate.mount.font = Some(font.clone());
            candidate.mount.invalidate();
        }
        if let Some(m) = &mut self.active {
            m.font = Some(font);
            m.invalidate();
        }
    }
    /// Controls presentation only; the host still verifies and revokes its cookie session.
    pub fn set_session_controls(&mut self, enabled: bool) -> bool {
        self.session_controls = enabled;
        if !enabled {
            self.sign_out_requested = false;
        }
        if let Some(m) = &mut self.active
            && m.session_controls != enabled
        {
            m.session_controls = enabled;
            m.invalidate();
            return true;
        }
        false
    }
    /// Presentation only. Authority admission independently checks every private request.
    pub fn set_private_access(&mut self, granted: bool) -> bool {
        self.private_access = granted;
        if let Some(m) = &mut self.active
            && m.private_access != granted
        {
            m.private_access = granted;
            m.invalidate();
            return true;
        }
        false
    }
    pub fn take_sign_out(&mut self) -> bool {
        std::mem::take(&mut self.sign_out_requested)
    }
    pub fn admit_catalog(&mut self, bytes: &[u8]) -> Result<()> {
        let c = decode_catalog(bytes)?;
        if c.format != "otrios-app-catalog-v1" || c.apps.is_empty() || c.apps.len() > 32 {
            return Err("Invalid app catalog".into());
        }
        let mut refs = std::collections::BTreeSet::new();
        for e in &c.apps {
            if e.ui_hash.len() != 32
                || !valid_destination(&e.app.id)
                || e.app.ui_ref != format!("app:{}/ui", e.app.id)
                || e.app.state_ref.len() > 128
                || !refs.insert(e.app.ui_ref.clone())
            {
                return Err("Invalid or duplicate catalog entry".into());
            }
        }
        let mut paths = std::collections::BTreeSet::new();
        for entry in &c.apps {
            if (!entry.path.is_empty() && !valid_destination(&entry.path))
                || !paths.insert(&entry.path)
            {
                return Err("Invalid or duplicate destination path".into());
            }
            if let Some((parent, _)) = entry.path.rsplit_once('/')
                && !c.apps.iter().any(|e| e.path == parent)
            {
                return Err("Destination parent is not admitted".into());
            }
        }
        self.catalog_hash = hash(bytes);
        self.catalog = Some(c);
        Ok(())
    }
    /// Admit compatible UI document revisions without replacing executable code.
    /// The staged mount qualifies its new coverage before replacing the old view.
    pub fn can_update_catalog(&mut self) -> bool {
        if self.local_uninstall {
            return false;
        }
        if self.host_active {
            return !self.input_busy();
        }
        self.requested.is_none()
            && self.navigation.is_none()
            && !self.input_busy()
            && self.confirmed()
    }
    pub fn update_catalog(&mut self, bytes: &[u8]) -> Result<bool> {
        if self.host_active {
            let mount = self.active.take();
            let result = self.admit_catalog(bytes);
            self.active = mount;
            self.navigation = None;
            result?;
            return Ok(false);
        }
        if hash(bytes) == self.catalog_hash {
            return Ok(false);
        }
        if !self.can_update_catalog() {
            return Err("UI update requires idle confirmed input".into());
        }
        let next = decode_catalog(bytes)?;
        let old = self.catalog.as_ref().ok_or("Missing catalog")?;
        if next.format != old.format || next.apps.len() != old.apps.len() {
            return Err("UI update changed the catalog contract".into());
        }
        for entry in &old.apps {
            let replacement = next
                .apps
                .iter()
                .find(|n| n.app.id == entry.app.id)
                .ok_or("UI update removed an app")?;
            if entry.app != replacement.app || entry.private != replacement.private {
                return Err("Only app UI data may change".into());
            }
        }
        let previous = (old.clone(), self.catalog_hash.clone());
        let remount = self.active.as_ref().is_some_and(|m| {
            old.apps
                .iter()
                .find(|e| e.app.ui_ref == m.ui.ui_ref)
                .map(|e| &e.ui_hash)
                != next
                    .apps
                    .iter()
                    .find(|e| e.app.ui_ref == m.ui.ui_ref)
                    .map(|e| &e.ui_hash)
        });
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("Canvas epoch exhausted")?;
        self.admit_catalog(bytes)?;
        if remount {
            let m = self.active.as_ref().ok_or("Missing app")?;
            self.previous_catalog = Some(previous);
            self.generation = generation;
            self.requested = Some(Request {
                epoch: generation,
                pair: (m.ui.ui_ref.clone(), m.state_ref.clone()),
                sent: false,
                reconnecting: false,
            });
        }
        Ok(remount)
    }
    pub fn catalog_update_pending(&self) -> bool {
        self.previous_catalog.is_some()
    }
    pub fn catalog_hash(&self) -> &[u8] {
        &self.catalog_hash
    }
    fn reject_catalog_update(&mut self) {
        if let Some((catalog, hash)) = self.previous_catalog.take() {
            self.catalog = Some(catalog);
            self.catalog_hash = hash;
        }
    }
    pub fn epoch(&self) -> u32 {
        self.active.as_ref().map_or(0, |m| m.epoch)
    }
    /// Unsent mount generation, for host timing without interpreting wire bytes.
    pub fn pending_mount_epoch(&self) -> u32 {
        if self.navigation.is_some() {
            self.generation.saturating_add(1)
        } else {
            self.requested
                .as_ref()
                .filter(|r| !r.sent)
                .map_or(0, |r| r.epoch)
        }
    }
    pub fn title(&self) -> String {
        if self.auth_required
            && let Some(request) = &self.requested
            && let Some(entry) = self
                .catalog
                .as_ref()
                .and_then(|c| c.apps.iter().find(|e| e.app.ui_ref == request.pair.0))
        {
            return entry.app.title.clone();
        }
        self.active
            .as_ref()
            .map_or_else(|| "Opening app".into(), |m| m.ui.title.clone())
    }
    pub fn ui_ref(&self) -> String {
        self.active
            .as_ref()
            .map_or_else(String::new, |m| m.ui.ui_ref.clone())
    }
    pub fn state_ref(&self) -> String {
        self.active
            .as_ref()
            .map_or_else(String::new, |m| m.state_ref.clone())
    }
    pub fn pending_ui_ref(&self) -> String {
        self.requested
            .as_ref()
            .map_or_else(String::new, |r| r.pair.0.clone())
    }
    pub fn pending_path(&self) -> String {
        self.catalog
            .as_ref()
            .and_then(|c| {
                c.apps
                    .iter()
                    .find(|e| e.app.ui_ref == self.pending_ui_ref())
            })
            .map(|e| e.path.clone())
            .unwrap_or_default()
    }
    pub fn pending_state_ref(&self) -> String {
        self.requested
            .as_ref()
            .map_or_else(String::new, |r| r.pair.1.clone())
    }
    /// A recoverable candidate rejection leaves the current app/session intact.
    pub fn rejected_mount_pending(&self) -> bool {
        matches!(self.decision, Some(MountDecision::Cancel(_)))
    }
    pub fn auth_required(&self) -> bool {
        self.auth_required
    }
    pub fn private(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|m| m.private || m.binding.is_some())
            || self
                .qualified
                .as_ref()
                .is_some_and(|q| q.mount.private || q.mount.binding.is_some())
    }
    pub fn input_busy(&self) -> bool {
        self.host.drag.is_some()
            || self.active.as_ref().is_some_and(|m| {
                m.designer
                    .as_ref()
                    .is_some_and(oib_designer::Designer::busy)
                    || m.input.active()
                    || m.keyboard_pointer.is_some()
                    || m.order_drag.is_some()
                    || m.text_views.values().any(TextViewport::active)
                    || m.preview_scroll.as_ref().is_some_and(|s| s.touching())
                    || m.text_scroll.as_ref().is_some_and(|s| s.touching())
                    || m.text_document.as_ref().is_some_and(|s| s.touching())
            })
    }
    pub fn value(&self, id: u16) -> Result<u16> {
        {
            let m = self.active.as_ref().ok_or("No Canvas")?;
            if let Value::Number(n) = m.read(id)?
                && n.fract() == 0.
                && (0. ..=1000.).contains(&n)
            {
                return Ok(n as u16);
            }
            Err("Not a normalized integer".into())
        }
    }
    pub fn typed_value(&self, id: u16) -> Result<Value> {
        self.active.as_ref().ok_or("No Canvas")?.read(id)
    }
    pub fn confirmed(&mut self) -> bool {
        if self.requested.is_some() || self.decision.is_some() {
            return false;
        }
        (self.local || self.active.as_mut().is_none_or(|m| m.doc.confirmed()))
            && self
                .active
                .as_ref()
                .is_none_or(|m| m.chat.as_ref().is_none_or(super::chat::Client::confirmed))
    }
    /// A reconnect/mount handshake must not strand an idle old runtime. Only
    /// unconfirmed user intent and active input prevent code replacement.
    pub fn reload_ready(&self) -> bool {
        !self.input_busy()
            && !self
                .host
                .transfer
                .as_ref()
                .is_some_and(|d| matches!(d.step, super::host_ui::transfers::Step::Waiting { .. }))
            && !self.spaces.pending()
            && (self.local || self.active.as_ref().is_none_or(|m| m.doc.confirmed()))
            && self
                .active
                .as_ref()
                .is_none_or(|m| m.chat.as_ref().is_none_or(super::chat::Client::confirmed))
    }
    pub fn presentation(&self) -> Option<&Presentation> {
        self.active
            .as_ref()
            .and_then(|m| m.ui.presentation.as_ref())
    }
    pub fn motion_intent(&self) -> Result<Option<(bool, crate::dashboard::motion::Intent)>> {
        let Some(Presentation::Motion {
            three_d,
            playing,
            speed,
            count,
            ..
        }) = self.presentation()
        else {
            return Ok(None);
        };
        let i = crate::dashboard::motion::Intent {
            playing: self.value(*playing)? > 0,
            speed: 0.25 + f32::from(self.value(*speed)?) * 0.00375,
            count: (1. + f32::from(self.value(*count)?) * 0.063).round() as u16,
        };
        i.validate()?;
        Ok(Some((*three_d, i)))
    }
    pub fn motion_viewport(&self) -> Result<Option<[f32; 4]>> {
        let Some(Presentation::Motion { viewport, .. }) = self.presentation() else {
            return Ok(None);
        };
        let Some(id) = viewport else {
            return Ok(Some([80., 280., 1120., 400.]));
        };
        let scene = self.scene()?;
        let path = scene
            .path(u32::from(*id))
            .ok_or("Missing motion viewport")?;
        let node = path.last().ok_or("Missing motion path")?;
        Ok(Some([
            node.origin[0],
            node.origin[1],
            node.node.frame.width * node.scale,
            node.node.frame.height * node.scale,
        ]))
    }
    pub fn stress_hz(&self) -> u8 {
        self.active.as_ref().map_or(0, |m| m.stress)
    }
    pub fn set_stress(&mut self, hz: u8) -> Result<bool> {
        if hz > 60 {
            return Err("Unsupported stress rate".into());
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        m.stress = hz;
        m.stress_last = None;
        Ok(true)
    }
    pub fn stress_updates(&self) -> u64 {
        self.active.as_ref().map_or(0, |m| m.stress_updates)
    }
    pub fn stress_tick(&mut self, now: f64) -> Result<bool> {
        if self.local_uninstall {
            return Ok(false);
        }
        if !now.is_finite() {
            return Ok(false);
        }
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        let Some(Presentation::Motion { load, .. }) = m.ui.presentation else {
            return Ok(false);
        };
        let hz = if m.stress == 31 { 30 } else { m.stress };
        if hz == 0 {
            m.stress_last = None;
            return Ok(false);
        }
        if m.stress_last
            .is_some_and(|last| now - last < 1000. / f64::from(hz))
        {
            return Ok(false);
        }
        m.stress_last = Some(now);
        if m.stress_workload.is_none() {
            let scope = format!("diagnostic:{}", hex(&hash(m.state_ref.as_bytes())));
            m.stress_workload = Some(StressWorkload::new(m.ui.state_schema.clone(), scope, load)?);
        }
        let changed = m
            .stress_workload
            .as_mut()
            .ok_or("Missing diagnostic workload")?
            .step()?;
        m.stress_updates += u64::from(changed);
        Ok(changed)
    }
    pub fn stress_stall_ms(&self) -> u64 {
        if self.stress_hz() == 31 { 250 } else { 0 }
    }
    pub fn resource_usage(&self) -> ResourceUsage {
        ResourceUsage {
            oib: self
                .active
                .as_ref()
                .and_then(|m| m.oib_view.as_ref().map(|view| view.info.clone())),
            chat: self.chat_info(),
            sampled_monotonic_ms: self.wire_clock,
            dispatch: self.dispatch_usage(),
            state: self
                .active
                .as_ref()
                .map_or_else(StateUsage::default, |m| m.doc.state_usage()),
            behavior_cache_entries: self.cache.len(),
            behavior_cache_bytes: self.cache.iter().map(|(_, bytes)| bytes.len()).sum(),
            ui_cache_entries: self.ui_cache.len(),
            ui_cache_bytes: self.ui_cache.iter().map(|(_, bytes)| bytes.len()).sum(),
            ui_cache_hits: self.ui_cache_hits,
            behavior_hash: self
                .active
                .as_ref()
                .and_then(|m| m.behavior.as_ref())
                .map(|b| hex(b.code_hash())),
            reflow_order: self
                .active
                .as_ref()
                .and_then(|m| ordered_items(m).ok())
                .unwrap_or_default()
                .iter()
                .map(|i| i.field)
                .collect(),
            reflow_editing: self.active.as_ref().is_some_and(|m| m.order_editing),
            wire: wire_usage(),
            text_scroll: self.text_scroll_info(),
            map_scroll: self
                .active
                .as_ref()
                .and_then(|m| m.map_scroll.as_ref())
                .map(|s| s.info()),
            oib_preview_scroll: self
                .active
                .as_ref()
                .and_then(|m| m.preview_scroll.as_ref())
                .map(|s| s.info()),
            panel_scroll: self
                .active
                .as_ref()
                .and_then(|m| m.panel_scroll.as_ref())
                .map(|s| s.info()),
            text_document: self
                .active
                .as_ref()
                .and_then(|m| m.text_document.as_ref())
                .and_then(|s| s.stats()),
        }
    }
    pub fn dispatch_usage(&self) -> DispatchUsage {
        self.active
            .as_ref()
            .map_or_else(DispatchUsage::default, |m| m.doc.dispatch_usage())
    }
    pub fn pending_count(&mut self) -> usize {
        if self.confirmed() {
            0
        } else {
            self.active
                .as_ref()
                .map_or(0, |m| m.doc.pending_count().max(1))
        }
    }
    pub fn open(&mut self, ui_ref: &str, state_ref: &str) {
        if !self.local_apps.reference(ui_ref) || self.local_uninstall {
            return;
        }
        self.reject_catalog_update();
        self.sign_out_requested = false;
        if let Some(request) = self.requested.take()
            && request.sent
        {
            self.decision = Some(MountDecision::Cancel(request.epoch));
        }
        self.auth_required = false;
        self.navigation = Some((ui_ref.into(), state_ref.into()));
    }
    pub fn reset_connection(&mut self) {
        self.spaces.reset();
        self.qualified = None;
        self.chat_disconnected();
        self.decision = None;
        self.auth_required = false;
        self.host_wire.clear();
        if self.local_uninstall {
            self.local_uninstall_disconnected = true;
            self.requested = None;
            self.navigation = None;
            return;
        }
        if self.host_active {
            return;
        }
        if let Some(r) = &mut self.requested {
            r.sent = false;
            return;
        }
        if let Some(m) = &mut self.active {
            m.doc.reconnect();
            self.generation = self.generation.saturating_add(1);
            self.requested = Some(Request {
                epoch: self.generation,
                pair: (m.ui.ui_ref.clone(), m.state_ref.clone()),
                sent: false,
                reconnecting: true,
            });
        }
    }
    pub fn retry_mount(&mut self) {
        if let Some(r) = &mut self.requested {
            if r.sent {
                let Some(epoch) = self.generation.checked_add(1) else {
                    return;
                };
                self.generation = epoch;
                r.epoch = epoch;
            }
            r.sent = false
        }
        self.auth_required = false;
    }
    /// The same catalog destination is used by browser URLs and native entry points.
    pub fn screen(&self) -> String {
        if self.host_active {
            return "spaces".into();
        }
        self.catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|e| e.app.ui_ref == self.ui_ref()))
            .map(|e| e.app.id.clone())
            .unwrap_or_default()
    }
    pub fn path(&self) -> String {
        if self.host_active {
            return String::new();
        }
        self.catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|e| e.app.ui_ref == self.ui_ref()))
            .map(|e| e.path.clone())
            .unwrap_or_default()
    }
    pub fn navigate(&mut self, path: &str) -> Result<()> {
        let path = path.strip_prefix('/').unwrap_or(path);
        if path.is_empty() && self.spaces.snapshot().is_some() {
            self.show_spaces()?;
            return Ok(());
        }
        if path.is_empty() && self.spaces.awaiting_snapshot() {
            self.host_requested = true;
            return Ok(());
        }
        let entry = self
            .catalog
            .as_ref()
            .and_then(|c| {
                if path.is_empty() {
                    c.apps.first()
                } else {
                    c.apps.iter().find(|e| e.path == path)
                }
            })
            .ok_or("Unknown admitted destination")?
            .app
            .clone();
        if !self.local_apps.reference(&entry.ui_ref) || self.local_uninstall {
            return Err("App is not installed on this device".into());
        }
        if self.ui_ref() == entry.ui_ref
            && self.state_ref() == entry.state_ref
            && self.navigation.is_none()
            && self.requested.is_none()
            && !self.auth_required
        {
            return Ok(());
        }
        self.open(&entry.ui_ref, &entry.state_ref);
        Ok(())
    }
    fn open_entry(&mut self) -> bool {
        self.navigate("").is_ok()
    }
    pub fn clear_private(&mut self) {
        self.transfer_task = None;
        self.transfer_copy = None;
        let host = self.host_active;
        self.host_active = false;
        self.host_actions.clear();
        self.host_requested = false;
        self.host = super::host_ui::Model::default();
        self.spaces.forget();
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.private || m.binding.is_some())
        {
            self.active = None;
        }
        self.qualified = None;
        self.reject_catalog_update();
        if let Some(request) = self.requested.take()
            && request.sent
        {
            self.decision = Some(MountDecision::Cancel(request.epoch));
        }
        self.auth_required = false;
        if !host {
            self.open_entry();
        }
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        if self.catalog.is_none() {
            return Ok(None);
        }
        if let Some(message) = self.host_wire.pop_front() {
            return encode_client(&message).map(Some);
        }
        let writer = match self.writer {
            Some(w) => w,
            None => {
                let w = state::writer_nonce()?;
                self.writer = Some(w);
                w
            }
        };
        if let Some(decision) = self.decision.take() {
            let message = match decision {
                MountDecision::Accept(epoch) => {
                    if let Some(candidate) = &mut self.qualified
                        && candidate.mount.epoch == epoch
                    {
                        candidate.accepted = true;
                    }
                    ClientMessage::AcceptMount { epoch }
                }
                MountDecision::Cancel(epoch) => ClientMessage::CancelMount { epoch },
            };
            return encode_client(&message).map(Some);
        }
        if self.requested.is_none()
            && !self.input_busy()
            && self.confirmed()
            && let Some(pair) = self.navigation.take()
        {
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or("Canvas epoch exhausted")?;
            self.requested = Some(Request {
                epoch: self.generation,
                pair,
                sent: false,
                reconnecting: false,
            });
        }
        if let Some(r) = &mut self.requested {
            if r.sent || self.auth_required {
                return Ok(None);
            }
            r.sent = true;
            return encode_client(&ClientMessage::Mount {
                epoch: r.epoch,
                ui_ref: r.pair.0.clone(),
                state_ref: r.pair.1.clone(),
                catalog_hash: self.catalog_hash.clone(),
                known_ui: self.catalog.as_ref().and_then(|c| {
                    c.apps
                        .iter()
                        .find(|e| {
                            e.app.ui_ref == r.pair.0 && e.app.state_ref == r.pair.1 && !e.private
                        })
                        .filter(|e| self.ui_cache.iter().any(|v| v.0 == e.ui_hash))
                        .map(|e| e.ui_hash.clone())
                }),
                known_code: self.cache.iter().map(|v| v.0.clone()).collect(),
                writer: writer.to_vec(),
                resume: self
                    .active
                    .as_ref()
                    .filter(|m| m.state_ref == r.pair.1)
                    .map_or(0, |m| m.doc.resume_sequence()),

                collection: self
                    .active
                    .as_ref()
                    .filter(|m| m.state_ref == r.pair.1)
                    .and_then(|m| m.binding.as_ref().map(|b| b.collection.clone())),
            })
            .map(Some);
        }
        if self.local {
            return Ok(None);
        }
        if let Some(m) = &mut self.active
            && let Some(bytes) = m.doc.generate()?
        {
            return encode_client(&ClientMessage::Sync {
                epoch: m.epoch,
                bytes,
            })
            .map(Some);
        }
        Ok(None)
    }
    pub fn mount_local(
        &mut self,
        ui: &[u8],
        state_ref: &str,
        code: &[u8],
        state: &[u8],
    ) -> Result<bool> {
        let description = decode_ui(ui)?;
        if self.local_uninstall || !self.local_apps.reference(&description.ui_ref) {
            return Err("App is not installed on this device".into());
        }
        self.qualified = None;
        self.host_active = false;
        self.host_actions.clear();
        self.generation = self.generation.checked_add(1).ok_or("Epoch exhausted")?;
        if self.writer.is_none() {
            self.writer = Some(state::writer_nonce()?);
        }
        self.requested = Some(Request {
            epoch: self.generation,
            pair: (description.ui_ref, state_ref.into()),
            sent: true,
            reconnecting: false,
        });
        let changed = self.receive(&encode_server(&ServerMessage::Mounted {
            epoch: self.generation,
            ui: ui.into(),
            ui_hash: hash(ui),
            code: code.into(),
            state: state.into(),

            binding: None,
            initial_view: None,
        })?)?;
        self.local = true;
        self.decision = None;
        Ok(changed)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool> {
        self.decode_budget.admit(bytes, self.wire_clock)?;
        match decode_server(bytes)? {
            ServerMessage::Activated { epoch } => {
                if self.requested.as_ref().is_none_or(|r| r.epoch != epoch)
                    || self
                        .qualified
                        .as_ref()
                        .is_none_or(|q| q.mount.epoch != epoch)
                {
                    return Ok(false);
                }
                if !self.qualified.as_ref().is_some_and(|q| q.accepted) {
                    return Err("Activation before candidate acceptance".into());
                }
                let candidate = self.qualified.take().ok_or("Missing qualified candidate")?;
                self.activate_candidate(candidate.mount, candidate.reconnecting, candidate.code)
            }
            ServerMessage::View { epoch, bytes } => {
                let Some(m) = &mut self.active else {
                    return Ok(false);
                };
                if epoch != m.epoch {
                    return Ok(false);
                }
                if !matches!(m.ui.presentation, Some(Presentation::View { .. })) {
                    return Err("Undeclared aggregate view".into());
                }
                let view = Layout::from_bytes(&bytes)?;
                view.resolve_slots(&BTreeMap::new())?;
                m.view = Some(view);
                m.invalidate();
                Ok(true)
            }
            ServerMessage::Mounted {
                epoch,
                ui,
                ui_hash,
                code,
                state,
                binding,
                initial_view,
            } => {
                self.managed_transport |= binding.is_some();
                let Some(r) = &self.requested else {
                    return Ok(false);
                };
                if r.epoch != epoch {
                    return Ok(false);
                }
                if self.qualified.is_some() {
                    return Ok(false);
                }
                let reconnecting = r.reconnecting;
                let cached = ui.is_empty();
                let candidate = (|| -> Result<Mount> {
                    if !self.local_apps.reference(&r.pair.0) {
                        return Err("App is not installed on this device".into());
                    }
                    let e = self
                        .catalog
                        .as_ref()
                        .and_then(|c| {
                            c.apps
                                .iter()
                                .find(|e| e.app.ui_ref == r.pair.0 && e.app.state_ref == r.pair.1)
                        })
                        .ok_or("App pair not admitted")?;
                    let bytes = if cached && !e.private {
                        &self
                            .ui_cache
                            .iter()
                            .find(|v| v.0 == ui_hash)
                            .ok_or("Missing cached UI")?
                            .1
                    } else {
                        &ui
                    };
                    if hash(bytes) != e.ui_hash || ui_hash != e.ui_hash {
                        return Err("UI differs from admitted catalog".into());
                    }
                    let ui = decode_ui(bytes)?;
                    if self.previous_catalog.is_some() {
                        compatible_app_update(&self.active.as_ref().ok_or("Missing app")?.ui, &ui)?;
                    }
                    if ui.ui_ref != r.pair.0 {
                        return Err("Wrong UI reference".into());
                    }
                    let mut candidate = qualify(
                        ui,
                        &r.pair.1,
                        &code,
                        &self.cache,
                        &state,
                        Qualification {
                            epoch,
                            private: e.private || e.app.id == "monitoring",
                            writer: self.writer.ok_or("Missing writer nonce")?,
                        },
                    )?;
                    candidate.binding = binding;
                    if candidate.binding.is_some() {
                        candidate.quit_control = (0..=u8::MAX).rev().find(|control| {
                            !candidate.ui.handlers.iter().any(|h| h.control == *control)
                        });
                    }
                    candidate
                        .doc
                        .set_read_only(candidate.binding.as_ref().is_some_and(|b| !b.editable));
                    if let Some(chat) = &mut candidate.chat {
                        chat.managed(candidate.quit_control);
                        if let Some(snapshot) = self.spaces.snapshot() {
                            chat.publication_targets(snapshot);
                        }
                        chat.access(
                            candidate
                                .binding
                                .as_ref()
                                .is_none_or(|b| b.editable || b.comments),
                        );
                    }
                    if let Some(bytes) = initial_view {
                        match candidate.ui.presentation {
                            Some(Presentation::View { .. }) => {
                                let view = Layout::from_bytes(&bytes)?;
                                view.resolve_slots(&BTreeMap::new())?;
                                candidate.view = Some(view);
                            }
                            Some(Presentation::Chat) if candidate.binding.is_some() => {
                                candidate
                                    .chat
                                    .as_mut()
                                    .ok_or("Missing chat candidate")?
                                    .receive_initial(&bytes)?;
                            }
                            _ => return Err("Undeclared initial app view".into()),
                        }
                    } else if candidate.binding.is_some()
                        && matches!(
                            candidate.ui.presentation,
                            Some(Presentation::View { .. } | Presentation::Chat)
                        )
                    {
                        return Err("Missing initial app view".into());
                    }
                    candidate.font = self.font.clone();
                    if let Some(chat) = &mut candidate.chat
                        && let Some(viewport) = self.layout_viewport
                    {
                        chat.set_header_width(viewport[0]);
                    }
                    candidate.viewport =
                        if matches!(candidate.ui.presentation, Some(Presentation::Chat)) {
                            self.viewport
                        } else {
                            self.layout_viewport.or(self.viewport)
                        }
                        .unwrap_or([1280., 800.]);
                    if let Some(chat) = &mut candidate.chat {
                        chat.prepare(
                            candidate.viewport,
                            candidate.keyboard.visible,
                            candidate.font.clone(),
                        );
                        if let Some(reader) = &mut candidate.text_document {
                            chat.synchronize_document(reader)?;
                        }
                    }
                    let scene = resolve_scene(&candidate)?;
                    if let Some(reader) = &mut candidate.text_document {
                        reader.sync(&scene, candidate.font.as_ref())?;
                    }
                    candidate.invalidate();
                    candidate.doc.set_dispatch_policy(self.dispatch);
                    if let Some(old) = &self.active
                        && old.state_ref == candidate.state_ref
                        && old.ui.state_schema == candidate.ui.state_schema
                        && old.binding == candidate.binding
                    {
                        candidate.doc.restore_pending(&old.doc)?;
                        candidate.now = old.now;
                        candidate.wall = old.wall;
                        candidate.refresh()?;
                    }
                    if self.previous_catalog.is_some()
                        && let Some(old) = &self.active
                        && old.binding == candidate.binding
                    {
                        if candidate.view.is_none() {
                            candidate.view = old.view.clone();
                        }
                        candidate.keyboard = old.keyboard.clone();
                        candidate.editors = old.editors.clone();
                        candidate.text_views = old.text_views.clone();
                        for view in candidate.text_views.values_mut() {
                            view.cancel();
                        }
                        candidate.focus = old.focus;
                        candidate.ink = old.ink.clone();
                        candidate.pad_preview = old.pad_preview.clone();
                        candidate.order_editing = old.order_editing;
                    }
                    candidate.session_controls = self.session_controls;
                    candidate.private_access = self.private_access;
                    candidate.keyboard.set_enabled(!self.external_keyboard);
                    candidate.invalidate();
                    cached_scene(&candidate)?;
                    Ok(candidate)
                })();
                match candidate {
                    Ok(candidate) => {
                        if !candidate.private {
                            // Move the qualified bytes, including on a cache hit.
                            // Repeated mounts must not clone an entire UI document.
                            let bytes = if cached {
                                let index = self
                                    .ui_cache
                                    .iter()
                                    .position(|v| v.0 == ui_hash)
                                    .ok_or("Missing cached UI")?;
                                self.ui_cache.remove(index).ok_or("Missing cached UI")?.1
                            } else {
                                ui
                            };
                            self.ui_cache.retain(|v| v.0 != ui_hash);
                            self.ui_cache.push_back((ui_hash, bytes));
                            while self.ui_cache.len() > 8
                                || self.ui_cache.iter().map(|v| v.1.len()).sum::<usize>()
                                    > 2 * 1024 * 1024
                            {
                                self.ui_cache.pop_front();
                            }
                            self.ui_cache_hits += u64::from(cached);
                        }
                        self.decision = if candidate.binding.is_some() && candidate.chat.is_some() {
                            // Warm the first transcript incrementally on owner
                            // ticks while the previous activation stays visible.
                            None
                        } else {
                            Some(MountDecision::Accept(epoch))
                        };
                        if candidate.binding.is_some() {
                            self.qualified = Some(Qualified {
                                mount: candidate,
                                reconnecting,
                                code,
                                accepted: false,
                            });
                            Ok(false)
                        } else {
                            self.activate_candidate(candidate, reconnecting, code)
                        }
                    }
                    Err(e) => {
                        self.reject_catalog_update();
                        self.decision = Some(MountDecision::Cancel(epoch));
                        self.requested = None;
                        self.auth_required = false;
                        Err(e)
                    }
                }
            }
            ServerMessage::Sync { epoch, bytes } => {
                let Some(m) = &mut self.active else {
                    return Ok(false);
                };
                if epoch != m.epoch {
                    return Ok(false);
                }
                if m.doc.receive(&bytes)? {
                    m.refresh()
                } else {
                    Ok(false)
                }
            }
            ServerMessage::AccessRequired { epoch } => {
                let result = self.discard_revoked(epoch);
                if (self.managed_transport || self.selected_space.is_some())
                    && !self
                        .host_wire
                        .iter()
                        .any(|m| matches!(m, ClientMessage::Leave { epoch: old } if *old == epoch))
                {
                    self.host_wire.push_back(ClientMessage::Leave { epoch });
                }
                result
            }
            ServerMessage::Error { epoch, message } => {
                if self.requested.as_ref().is_some_and(|r| r.epoch == epoch) {
                    self.qualified = None;
                    self.reject_catalog_update();
                    self.decision = Some(MountDecision::Cancel(epoch));
                    self.requested = None;
                    self.auth_required = false;
                    return Err(message);
                }
                Ok(false)
            }
        }
    }
    fn discard_revoked(&mut self, epoch: u32) -> Result<bool> {
        if self.active.as_ref().is_some_and(|m| m.epoch == epoch) {
            // Revoke held presentation frames for every collection,
            // including a public app used inside a Closed Space.
            self.collection_revoked = true;
            let old = self.active.take().ok_or("Missing revoked activation")?;
            // Revocation belongs to the old lease. A newer staged public
            // navigation must still be able to qualify its Mounted reply.
            if self.requested.is_none() {
                self.generation = self
                    .generation
                    .checked_add(1)
                    .ok_or("Canvas epoch exhausted")?;
                self.requested = Some(Request {
                    epoch: self.generation,
                    pair: (old.ui.ui_ref, old.state_ref),
                    sent: false,
                    reconnecting: false,
                });
                self.auth_required = true;
            }
            self.spaces.forget();
            return Ok(true);
        }
        if self.requested.as_ref().is_some_and(|r| r.epoch == epoch) {
            if self.requested.as_ref().is_some_and(|r| r.reconnecting)
                && self.active.as_ref().is_some_and(|m| m.binding.is_some())
            {
                self.active = None;
                self.collection_revoked = true;
            }
            self.qualified = None;
            self.decision = Some(MountDecision::Cancel(epoch));
            if self.previous_catalog.is_some() {
                self.reject_catalog_update();
                self.decision = Some(MountDecision::Cancel(epoch));
                self.requested = None;
                return Err("UI update access rejected".into());
            }
            self.auth_required = true;
            self.spaces.forget();
        }
        Ok(false)
    }
    fn activate_candidate(
        &mut self,
        mut candidate: Mount,
        reconnecting: bool,
        code: Vec<u8>,
    ) -> Result<bool> {
        // Reconnecting views stay editable during the activation round trip.
        // Capture any later RAM intents only within the same admitted collection.
        if let Some(old) = &self.active
            && old.state_ref == candidate.state_ref
            && old.ui.state_schema == candidate.ui.state_schema
            && old.binding == candidate.binding
        {
            candidate.doc.restore_pending(&old.doc)?;
            candidate.refresh()?;
        }
        if self.previous_catalog.take().is_some()
            && self
                .active
                .as_ref()
                .is_some_and(|old| old.binding == candidate.binding)
        {
            let mut current = self.active.take().ok_or("Missing app activation")?;
            if current.ui.behavior != candidate.ui.behavior {
                current.behavior = candidate.behavior;
            }
            current.ui = candidate.ui;
            current.layout = candidate.layout;
            current.doc = candidate.doc;
            current.epoch = candidate.epoch;
            current.visible = candidate.visible;
            current.view = candidate.view;
            current.quit_control = candidate.quit_control;
            current.invalidate();
            candidate = current;
        }
        if reconnecting
            && self.active.as_ref().is_some_and(|old| {
                (old.ui == candidate.ui
                    || old
                        .doc
                        .admitted_oib_hash()
                        .is_some_and(|h| Some(h) == candidate.doc.admitted_oib_hash()))
                    && old.state_ref == candidate.state_ref
                    && old.binding == candidate.binding
            })
        {
            let mut current = self.active.take().ok_or("Missing reconnect activation")?;
            current.doc = candidate.doc;
            current.epoch = candidate.epoch;
            current.session_controls = candidate.session_controls;
            current.private_access = candidate.private_access;
            if candidate.view.is_some() {
                current.view = candidate.view;
            }
            current.refresh()?;
            current.invalidate();
            candidate = current;
        }
        if let Some(reference) = &candidate.ui.behavior
            && !code.is_empty()
            && !self.cache.iter().any(|v| v.0 == reference.sha256)
        {
            self.cache.push_back((reference.sha256.clone(), code));
            while self.cache.len() > 8 {
                self.cache.pop_front();
            }
        }
        if let Some(old) = &mut self.active
            && let Some(chat) = &mut old.chat
        {
            self.collection_cancel = chat.cancel_collection().map(|command| {
                (
                    AuxScope {
                        epoch: old.epoch,
                        managed: old.binding.is_some(),
                    },
                    command,
                )
            });
            self.collection_revoked = true;
        }
        if let Some(chat) = &mut candidate.chat {
            chat.managed(candidate.quit_control);
        }
        if let Some(d) = &mut candidate.designer {
            d.studio = self.designer_studio;
            d.invalidate_bounds();
            candidate.invalidate();
        }
        self.active = Some(candidate);
        self.host_active = false;
        self.host_actions.clear();
        self.local = false;
        self.requested = None;
        self.auth_required = false;
        Ok(true)
    }
    pub fn scene(&self) -> Result<Arc<Scene>> {
        cached_scene(self.active.as_ref().ok_or("No Canvas")?)
    }
    pub fn edit_oib(&mut self, changes: &[(u64, Option<Value>)]) -> Result<bool> {
        let mount = self.active.as_mut().ok_or("No Canvas")?;
        if mount.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let changed = mount.doc.edit_oib(changes)?;
        if changed {
            mount.refresh()?;
            mount.invalidate();
        }
        Ok(changed)
    }
    /// Select an inspector locally; this never emits an authoritative operation.
    pub fn inspect_oib(&mut self, instance: u64) -> Result<bool> {
        if let Some(selection) = self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .and_then(|d| d.root_selection(instance))
        {
            return self.inspect_oib_selection(selection);
        }
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if !m
            .doc
            .oib_info()?
            .instances
            .iter()
            .any(|a| a.id == instance && a.active)
        {
            return Ok(false);
        }
        let (ui, layout) = m.doc.oib_compiled(Some(instance))?;
        m.oib_selected = Some(instance);
        if let Some(view) = &mut m.oib_view {
            view.info.selected_instance = Some(instance);
        }
        m.ui = ui;
        m.layout = layout;
        m.invalidate();
        m.sync_panel_scroll()?;
        if let Some(seed) = super::oib::seed(&m.ui)
            && let Some(surface) = &seed.instance_surface
            && let Some(scroll) = &mut m.panel_scroll
        {
            scroll.set_offset([0., surface.inspector_start]);
        }
        Ok(true)
    }
    pub fn instantiate_oib(&mut self, component: u64, name: Option<String>) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let changed = m.doc.instantiate_oib(component, name)?;
        if changed {
            m.refresh()?;
            m.invalidate();
        }
        Ok(changed)
    }
    pub fn restore_oib(&mut self, defaults: bool) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let changed = m.doc.restore_oib(defaults)?;
        if changed {
            m.refresh()?;
            m.invalidate();
        }
        Ok(changed)
    }
    pub fn edit_oib_instance_property(
        &mut self,
        instance: u64,
        property: &str,
        value: Option<Value>,
    ) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let changed = m
            .doc
            .edit_oib_instance_property(instance, property, value)?;
        if changed {
            m.refresh()?;
            m.invalidate();
        }
        Ok(changed)
    }
    pub fn duplicate_oib(&mut self, instance: u64, name: Option<String>) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let changed = m.doc.duplicate_oib(instance, name)?;
        if changed {
            m.refresh()?;
            m.invalidate();
        }
        Ok(changed)
    }
    pub fn remove_oib(&mut self, instance: u64) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let changed = m.doc.remove_oib(instance)?;
        if changed {
            m.refresh()?;
            m.invalidate();
        }
        Ok(changed)
    }
    pub fn event(&mut self, epoch: u32, control: u8, a: u16, b: u16) -> Result<bool> {
        if self.local_uninstall {
            return Ok(false);
        }
        self.dispatch_event(epoch, control, a, b)
    }
    fn dispatch_event(&mut self, epoch: u32, control: u8, a: u16, b: u16) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.quit_control == Some(control))
        {
            return self.show_spaces();
        }
        if self.host_active
            && let Some(action) = self.host_actions.get(&control).cloned()
        {
            return self.host_action(action);
        }
        let h = self
            .active
            .as_ref()
            .and_then(|m| m.handler(control))
            .ok_or("Unknown app event")?;
        if self.requested.as_ref().is_some_and(|r| !r.reconnecting)
            && !matches!(h.operation, Operation::Back)
        {
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.binding.as_ref().is_some_and(|b| !b.editable))
            && !matches!(
                h.operation,
                Operation::Chat(_)
                    | Operation::Back
                    | Operation::SignOut
                    | Operation::OibInstance {
                        action: otrios_interfaces::oib::InstanceAction::Inspect,
                        ..
                    }
            )
        {
            return Ok(false);
        }
        if self.active.as_ref().is_some_and(|m| m.designer_editing())
            && !matches!(
                h.operation,
                Operation::Oib { .. }
                    | Operation::OibInstance { .. }
                    | Operation::Back
                    | Operation::SignOut
            )
        {
            return Ok(false);
        }
        match h.operation {
            Operation::OibInstance { target, action } => {
                if action == otrios_interfaces::oib::InstanceAction::Inspect {
                    return self.inspect_oib(target);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
                    return Ok(false);
                }
                let changed = m.doc.oib_instance_action(target, action)?;
                if changed {
                    m.refresh()?;
                    m.invalidate();
                }
                return Ok(changed);
            }
            Operation::Oib { cell, action } => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
                    return Ok(false);
                }
                let changed = m.doc.oib_inspector_action(cell, action)?;
                if changed {
                    m.refresh()?;
                    m.invalidate();
                }
                return Ok(changed);
            }
            Operation::Chat(command) => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let chat = m.chat.as_mut().ok_or("No chat presentation")?;
                let action = chat.action(command)?;
                if chat.editing() {
                    m.keyboard.focus();
                } else {
                    m.keyboard.hide();
                }
                m.invalidate();
                match action {
                    super::chat::UiAction::Home => self.navigate("")?,
                    super::chat::UiAction::BrowserLogin => self.navigate("monitoring")?,
                    super::chat::UiAction::Changed => {}
                };
                return Ok(true);
            }
            Operation::EditOrder => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if !matches!(m.ui.presentation, Some(Presentation::Reorder { .. })) {
                    return Err("No reorder presentation".into());
                }
                m.input.cancel_all();
                m.order_drag = None;
                m.order_editing = !m.order_editing;
                m.invalidate();
                return Ok(true);
            }
            Operation::SignOut => {
                if !self.session_controls || !self.private() {
                    return Ok(false);
                }
                self.sign_out_requested = true;
                return Ok(true);
            }
            Operation::Back => return self.back(),
            Operation::Focus => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                for view in m.text_views.values_mut() {
                    view.cancel();
                }
                m.focus = h.fields.first().copied();
                m.keyboard.focus();
                m.keyboard_pointer = None;
                m.input.cancel_all();
                m.invalidate();
                return Ok(true);
            }
            Operation::Key(key) => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let edit = m.keyboard.press(key);
                if let Some(edit) = edit {
                    return self.hardware_edit(&edit);
                }
                m.invalidate();
                return Ok(true);
            }
            Operation::Stress => {
                let m = self.active.as_mut().ok_or("No Canvas")?;
                m.stress = match m.stress {
                    0 => 10,
                    10 => 30,
                    30 => 31,
                    _ => 0,
                };
                return Ok(true);
            }
            _ => {}
        }
        if self.requested.as_ref().is_some_and(|r| !r.reconnecting) {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.editable) {
            return Ok(false);
        }
        let current = h
            .fields
            .first()
            .map(|id| m.number(*id))
            .transpose()?
            .unwrap_or(0);
        let commands = if let Some(guest) = &mut m.behavior {
            guest.call(h.event, a, b, current)?
        } else {
            match h.operation {
                Operation::Toggle => vec![(h.fields[0], if current == 0 { 1000 } else { 0 })],
                Operation::Cycle(values) => {
                    let index = values
                        .iter()
                        .position(|n| *n == current)
                        .map_or(0, |i| (i + 1) % values.len());
                    vec![(h.fields[0], values[index])]
                }
                _ => h
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (*id, if i == 0 { a } else { b }))
                    .collect(),
            }
        };
        if commands.iter().any(|(id, _)| !h.fields.contains(id)) {
            return Err("Behavior exceeded event field scope".into());
        }
        if h.kind != GestureKind::Activate {
            m.doc.defer(&commands)?;
            m.refresh()
        } else if m.doc.set(&commands)? {
            m.refresh()
        } else {
            Ok(false)
        }
    }
    pub fn event_for_node(&mut self, epoch: u32, node: u32, a: u16, b: u16) -> Result<bool> {
        let action = InputRouter::activate(self.scene()?.as_ref(), node).action;
        if let Some(Action::AppEvent(control)) = action {
            self.event(epoch, control, a, b)
        } else {
            Err("Node is not an app event responder".into())
        }
    }
    pub fn pointer(&mut self, epoch: u32, event: PointerEvent) -> Result<bool> {
        let at = self.active.as_ref().map_or(0, |m| m.now);
        self.pointer_at(epoch, event, at)
    }
    /// Preserve source sample time independently of worker delivery/animation time.
    pub fn pointer_at(&mut self, epoch: u32, mut event: PointerEvent, at: u64) -> Result<bool> {
        if self.local_uninstall && event.phase == PointerPhase::Down {
            return Ok(false);
        }
        if event.position.iter().any(|v| !v.is_finite()) {
            event.phase = PointerPhase::Cancel;
            event.position = [0., 0.];
        }
        if self.host_active
            && let Some(changed) = self.host_map_pointer(epoch, event, at)?
        {
            return Ok(changed);
        }
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        if !self.host_active
            && let Some(changed) = self.designer_pointer(event, at)?
        {
            return Ok(changed);
        }
        if !self.host_active
            && let Some(changed) = self.oib_preview_pointer(epoch, event, at)?
        {
            return Ok(changed);
        }
        if !self.host_active
            && self
                .active
                .as_ref()
                .is_some_and(|m| super::oib::seed(&m.ui).is_some())
            && let Some(changed) = self.host_panel_pointer(epoch, event, at)?
        {
            return Ok(changed);
        }
        // Captured text pans use layout coordinates, which do not move with the
        // text. Avoid constructing a second presented tree between physics and
        // the next render; uncaptured hit tests still use the visible scene.
        let captured_text = self.active.as_ref().is_some_and(|m| {
            m.text_views.values().any(|v| v.owns(event.id))
                || m.text_scroll.as_ref().is_some_and(|s| s.touching())
                || m.text_document.as_ref().is_some_and(|s| s.touching())
        });
        let scene = if captured_text && event.phase != PointerPhase::Down {
            cached_layout(self.active.as_ref().ok_or("No Canvas")?)?
        } else {
            self.scene()?
        };
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if let Some(reader) = &mut m.text_document {
            if self.requested.as_ref().is_some_and(|r| !r.reconnecting) {
                reader.cancel();
            } else if let Some(changed) = reader.pointer(&scene, event, at) {
                return Ok(changed);
            }
        }
        if let Some(chat) = &mut m.chat
            && let Some(changed) = chat.pointer(event, at)
        {
            if chat.editing() {
                m.keyboard.focus();
            }
            m.invalidate();
            return Ok(changed);
        }
        if let Some(scroll) = &mut m.text_scroll {
            if self.requested.as_ref().is_some_and(|r| !r.reconnecting) {
                scroll.cancel();
            } else if let Some(changed) = scroll.pointer(&scene, event, at)? {
                m.invalidate_presentation();
                return Ok(changed);
            }
        }
        if m.order_editing
            && self.requested.as_ref().is_none_or(|r| r.reconnecting)
            && let Some(changed) = order_pointer(m, &scene, event)?
        {
            return Ok(changed);
        }
        if self.requested.as_ref().is_some_and(|r| !r.reconnecting) {
            for view in m.text_views.values_mut() {
                if view.owns(event.id) {
                    view.cancel();
                    return Ok(true);
                }
            }
        }
        if self.requested.as_ref().is_none_or(|r| r.reconnecting)
            && let Some(changed) = text_pointer(m, &scene, event, at)?
        {
            return Ok(changed);
        }
        let key = scene
            .hit_path(event.position)
            .iter()
            .rev()
            .find_map(|node| {
                if let Some(key) = m
                    .designer
                    .as_ref()
                    .and_then(|d| d.keyboard_key(node.node.id))
                {
                    return Some(key);
                }
                let Some(TouchResponder::Activate(Action::AppEvent(control))) = node.node.responder
                else {
                    return None;
                };
                m.ui.handlers
                    .iter()
                    .find(|h| h.control == control)
                    .and_then(|h| {
                        if let Operation::Key(key) = h.operation {
                            Some(key)
                        } else {
                            None
                        }
                    })
            });
        let owned = m.keyboard_pointer == Some(event.id);
        if m.keyboard.visible && (key.is_some() || owned) {
            if self.requested.as_ref().is_some_and(|r| !r.reconnecting) {
                return Ok(false);
            }
            let labels = (m.keyboard.page, m.keyboard.shift, m.keyboard.visible);
            let edit = match event.phase {
                PointerPhase::Down => key.and_then(|key| m.keyboard.down(event.id, key, m.now)),
                PointerPhase::Move => {
                    m.keyboard.motion(event.id, key);
                    None
                }
                PointerPhase::Up => {
                    if owned {
                        m.keyboard.motion(event.id, key);
                    }
                    m.keyboard.up(event.id)
                }
                PointerPhase::Cancel => {
                    if owned {
                        m.keyboard.cancel();
                    }
                    None
                }
            };
            if m.keyboard.touching(event.id) {
                m.keyboard_pointer = Some(event.id);
            } else if owned {
                m.keyboard_pointer = None;
            }
            // Contact feedback uses the cheap effects layer. Only a keyboard
            // page/shift/visibility change alters its authored scene/labels.
            if labels != (m.keyboard.page, m.keyboard.shift, m.keyboard.visible) {
                m.invalidate();
            }
            if let Some(edit) = edit {
                self.hardware_edit(&edit)?;
            }
            return Ok(true);
        }
        let delivery = m.input.dispatch_at(scene.clone(), event, at);
        let pad = delivery.gesture.as_ref().is_some_and(|g| matches!(m.ui.presentation,Some(Presentation::TouchPad{control}) if g.action==Action::AppEvent(control)));
        if pad && self.requested.as_ref().is_some_and(|r| !r.reconnecting) {
            return Ok(false);
        }
        if pad && let Some(g) = &delivery.gesture {
            m.ink.gesture(event.id, g, m.now);
            m.invalidate();
        }
        // Pad moves are transient presentation data. Only the released point is
        // a durable app edit; its local marker still follows every sample.
        if pad
            && let Some(g) = &delivery.gesture
            && g.phase != PointerPhase::Up
        {
            if g.phase != PointerPhase::Cancel
                && let Action::AppEvent(control) = g.action
            {
                let h =
                    m.ui.handlers
                        .iter()
                        .find(|h| h.control == control)
                        .ok_or("Unknown pad")?;
                for (field, value) in h.fields.iter().zip(
                    g.local
                        .into_iter()
                        .zip(g.extent)
                        .map(|(p, e)| (p / e).clamp(0., 1.) * 1000.),
                ) {
                    m.pad_preview.insert(*field, Value::Number(value.round()));
                }
                m.refresh()?;
            }
            if g.phase == PointerPhase::Cancel {
                m.pad_preview.clear();
                m.refresh()?;
            }
            return Ok(true);
        }
        if pad {
            m.pad_preview.clear();
        }
        let changed = self.deliver(delivery)? || pad;
        if matches!(event.phase, PointerPhase::Up | PointerPhase::Cancel)
            && let Some(m) = &mut self.active
        {
            m.doc.flush_pending();
        }
        Ok(changed)
    }
    pub fn activate(&mut self, epoch: u32, id: u32) -> Result<bool> {
        if self.local_uninstall {
            return Ok(false);
        }
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        if !self.host_active
            && let Some(changed) = self.designer_activate(id)?
        {
            return Ok(changed);
        }
        if !self.host_active && self.order_draggable(id) {
            return Ok(false);
        }
        self.deliver(InputRouter::activate(self.scene()?.as_ref(), id))
    }
    fn deliver(&mut self, d: Delivery) -> Result<bool> {
        if let Some(g) = d.gesture
            && g.phase != PointerPhase::Cancel
            && let Action::AppEvent(control) = g.action
        {
            let kind = self
                .active
                .as_ref()
                .and_then(|m| m.handler(control))
                .map(|h| h.kind)
                .ok_or("Unknown gesture")?;
            let norm = |v: f32| (v.clamp(0., 1.) * 1000.).round() as u16;
            let (a, b) = match kind {
                GestureKind::Slider => (norm(g.local[0] / g.extent[0]), 0),
                GestureKind::Pad => (
                    norm(g.local[0] / g.extent[0]),
                    norm(g.local[1] / g.extent[1]),
                ),
                GestureKind::Dial => {
                    let x = g.local[0] - g.extent[0] / 2.;
                    let y = g.local[1] - g.extent[1] / 2.;
                    (
                        norm((x.atan2(-y) / std::f32::consts::TAU).rem_euclid(1.)),
                        0,
                    )
                }
                GestureKind::Activate => (0, 0),
            };
            return self.dispatch_event(self.epoch(), control, a, b);
        }
        match d.action {
            Some(Action::AppNavigate(id)) => {
                if let Some(n) = self
                    .active
                    .as_ref()
                    .and_then(|m| m.ui.navigation.iter().find(|n| n.action == id))
                    .cloned()
                {
                    if n.ui_ref == "app:home/ui" && self.spaces.snapshot().is_some() {
                        return self.show_spaces();
                    }
                    if n.state_ref.is_empty() {
                        let destination = self
                            .catalog
                            .as_ref()
                            .and_then(|c| c.apps.iter().find(|e| e.app.ui_ref == n.ui_ref))
                            .ok_or("App destination is not admitted")?
                            .clone();
                        self.navigate(&destination.path)?;
                    } else {
                        self.open(&n.ui_ref, &n.state_ref);
                    }
                    return Ok(true);
                }
            }
            Some(Action::AppEvent(id)) => return self.dispatch_event(self.epoch(), id, 0, 0),
            _ => {}
        }
        Ok(false)
    }
    pub fn text_scroll_info(&self) -> Option<super::text_scroll::Info> {
        self.active
            .as_ref()
            .and_then(|m| m.text_scroll.as_ref())
            .map(|s| s.info())
    }
    pub fn document_frame(&mut self) -> Option<super::document::Frame> {
        self.active.as_mut()?.text_document.as_mut()?.frame()
    }
    pub fn document_scene_parts(&self, scene: &Scene) -> Result<Option<(Scene, Scene)>> {
        match self.presentation() {
            Some(Presentation::TextDocument { viewport, .. }) => {
                super::document::scene::split(scene, u32::from(*viewport)).map(Some)
            }
            Some(Presentation::Chat) => {
                super::document::scene::split(scene, u32::from(super::chat::VIEWPORT)).map(Some)
            }
            Some(Presentation::Oib(seed)) => {
                // Numeric preview feedback and inspector content have different
                // lifetimes. Use the existing ordered base/overlay resources so
                // one changed percentage cannot upload the whole inspector.
                super::document::scene::split_before(scene, u32::from(seed.inspector_viewport))
                    .map(Some)
            }
            _ => Ok(None),
        }
    }
    pub fn keyboard_visible(&self) -> bool {
        self.active.as_ref().is_some_and(|m| m.keyboard.visible)
    }
    pub fn editing(&self) -> bool {
        self.active.as_ref().is_some_and(|m| {
            m.focus.is_some()
                || m.designer.as_ref().is_some_and(|d| d.editor_focused())
                || m.chat.as_ref().is_some_and(super::chat::Client::editing)
        })
    }
    /// Native browser fields mirror only visible, editable controls in this
    /// mount. A modal never exposes a background composer to the host.
    pub fn text_inputs(&self) -> Result<Vec<otrios_interfaces::keyboard::TextInput>> {
        let Some(m) = &self.active else {
            return Ok(vec![]);
        };
        let Some(font) = &m.font else {
            return Ok(vec![]);
        };
        let scene = self.scene()?;
        let frames = editor_frames(m, &scene);
        let mut inputs = m
            .designer
            .as_ref()
            .map_or_else(Vec::new, |d| d.editor_inputs(&scene, font));
        for owner in scene.controls() {
            let Some(TouchResponder::Activate(Action::AppEvent(control))) = owner.node.responder
            else {
                continue;
            };
            let Some(handler) = m.handler(control) else {
                continue;
            };
            let mut input = match (&handler.operation, &m.chat) {
                (Operation::Chat(command), Some(chat)) => {
                    let Some(input) = chat.host_input(*command, owner.node.id) else {
                        continue;
                    };
                    input
                }
                (Operation::Focus, _) => {
                    let Some(frame) = frames.iter().find(|f| f.responder == owner.node.id) else {
                        continue;
                    };
                    let mut input = m.editors[&frame.field]
                        .host_input(owner.node.id, m.focus == Some(frame.field));
                    input.inset = [
                        frame.origin[0] - owner.origin[0],
                        frame.origin[1] - owner.origin[1],
                        owner.node.frame.width * owner.scale
                            - (frame.origin[0] - owner.origin[0])
                            - frame.width * frame.factor,
                        0.,
                    ];
                    input
                }
                _ => continue,
            };
            input.bounds = [
                owner.origin[0],
                owner.origin[1],
                owner.node.frame.width * owner.scale,
                owner.node.frame.height * owner.scale,
            ];
            input.font_size = font.metrics().cache_em(2.) * owner.scale;
            input.line_height = font.metrics().line_height(2.) * owner.scale;
            if let Some(frame) = frames.iter().find(|f| f.responder == owner.node.id) {
                input.font_size = font.metrics().cache_em(frame.scale) * frame.factor;
                input.line_height = font.metrics().line_height(frame.scale) * frame.factor;
            }
            inputs.push(input);
        }
        Ok(inputs)
    }
    /// Reject stale host events after navigation, a modal change or focus loss.
    pub fn edit_text_input(&mut self, epoch: u32, id: u32, edit: &Edit) -> Result<bool> {
        if epoch != self.epoch() || !self.text_inputs()?.iter().any(|i| i.id == id && i.focused) {
            return Ok(false);
        }
        self.hardware_edit(edit)
    }
    pub fn blur_text_input(&mut self, epoch: u32, id: u32) -> Result<bool> {
        if epoch != self.epoch() || !self.text_inputs()?.iter().any(|i| i.id == id && i.focused) {
            return Ok(false);
        }
        if let Some(changed) = self.designer_blur_input(id)? {
            return Ok(changed);
        }
        self.commit_host_label()?;
        let m = self.active.as_mut().ok_or("No Canvas")?;
        m.focus = None;
        if let Some(chat) = &mut m.chat {
            chat.blur_input();
        }
        m.keyboard.hide();
        m.invalidate();
        Ok(true)
    }
    /// HTML owns its scroll offset. Keep canvas edit sparks in that same local
    /// viewport, rather than running a second caret-follow animation underneath.
    pub fn scroll_text_input(&mut self, epoch: u32, id: u32, offset: f32) -> Result<bool> {
        if epoch != self.epoch()
            || !self
                .text_inputs()?
                .iter()
                .any(|i| i.id == id && i.focused && i.multiline)
        {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let changed = m.chat.as_mut().is_some_and(|chat| chat.host_scroll(offset));
        if changed {
            m.invalidate();
        }
        Ok(changed)
    }
    pub fn show_keyboard(&mut self) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if let Some(chat) = &mut m.chat {
            chat.action(1)?;
            if chat.editing() {
                m.keyboard.focus();
            } else {
                m.keyboard.hide();
            }
            m.invalidate();
            return Ok(true);
        }
        if let Some(Presentation::Editor { fields }) = &m.ui.presentation {
            for view in m.text_views.values_mut() {
                view.cancel();
            }
            m.focus = m.focus.or_else(|| fields.first().copied());
            m.keyboard.focus();
            m.keyboard_pointer = None;
            m.input.cancel_all();
            m.invalidate();
            return Ok(true);
        }
        Ok(false)
    }
    pub fn hardware_edit(&mut self, edit: &Edit) -> Result<bool> {
        if self.local_uninstall {
            return Ok(false);
        }
        if self.requested.as_ref().is_some_and(|r| !r.reconnecting) || self.active.is_none() {
            return Ok(false);
        }
        if let Some(changed) = self.designer_edit(edit)? {
            return Ok(changed);
        }
        if matches!(edit, Edit::Enter) {
            self.commit_host_label()?;
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if let Some(chat) = &mut m.chat {
            let changed = chat.edit(edit)?;
            if !chat.editing() {
                m.keyboard.hide();
            }
            m.invalidate();
            return Ok(changed);
        }
        if m.binding.as_ref().is_some_and(|b| !b.editable) {
            return Ok(false);
        }
        let Some(field) = m.focus else {
            return Ok(false);
        };
        match edit {
            Edit::Tab { backward } => {
                if let Some(Presentation::Editor { fields }) = &m.ui.presentation {
                    let i = fields.iter().position(|f| *f == field).unwrap_or(0);
                    m.focus = Some(
                        fields[(i + if *backward { fields.len() - 1 } else { 1 }) % fields.len()],
                    );
                    for view in m.text_views.values_mut() {
                        view.cancel();
                    }
                    m.keyboard.focus();
                    m.keyboard_pointer = None;
                    m.input.cancel_all();
                    m.invalidate();
                    return Ok(true);
                }
            }
            Edit::Enter => {
                for view in m.text_views.values_mut() {
                    view.cancel();
                }
                m.keyboard.hide();
                m.focus = None;
                m.keyboard_pointer = None;
                m.input.cancel_all();
                m.invalidate();
                return Ok(true);
            }
            _ => {}
        }
        let editor = m.editors.get_mut(&field).ok_or("Undeclared editor")?;
        let changed = editor.edit(edit);
        if changed {
            m.doc
                .set_typed(&[(field, Value::Text(editor.text().into()))])?;
            m.refresh()?;
            m.invalidate();
        }
        // Even an arrow at a text boundary must recover a manually panned caret.
        if matches!(
            edit,
            Edit::Commit(_)
                | Edit::Replace { .. }
                | Edit::Backspace
                | Edit::Left { .. }
                | Edit::Right { .. }
                | Edit::SelectAll
        ) {
            update_text_views(m, Some(field))?;
            m.invalidate();
            return Ok(true);
        }
        Ok(changed)
    }
    pub fn cancel(&mut self) -> Result<bool> {
        if self.host_active {
            self.host.drag = None;
            self.preview_map_drag();
            if let Some(m) = &mut self.active {
                if let Some(s) = &mut m.map_scroll {
                    s.cancel();
                }
                if let Some(s) = &mut m.panel_scroll {
                    s.cancel();
                }
                m.input.cancel_all();
                m.invalidate_presentation();
            }
            return Ok(true);
        }
        self.host.drag = None;
        if let Some(m) = &mut self.active {
            if let Some(designer) = &mut m.designer {
                designer.cancel();
                self.designer_studio = designer.studio;
            }
            if let Some(scroll) = &mut m.preview_scroll {
                scroll.cancel();
            }
            if let Some(scroll) = &mut m.panel_scroll {
                scroll.cancel();
            }
            m.input.cancel_all();
            if let Some(s) = &mut m.text_scroll {
                s.cancel();
            }
            if let Some(s) = &mut m.text_document {
                s.cancel();
            }
            for view in m.text_views.values_mut() {
                view.cancel();
            }
            m.order_drag = None;
            m.doc.flush_pending();
            m.keyboard.cancel();
            m.keyboard_pointer = None;
            m.ink.cancel();
            m.pad_preview.clear();
            m.refresh()?;
            m.invalidate();
            return Ok(true);
        }
        Ok(false)
    }
    pub fn back(&mut self) -> Result<bool> {
        if !self.host_active
            && let Some(address) = self
                .active
                .as_ref()
                .and_then(|m| m.designer.as_ref())
                .and_then(|d| d.focused_property())
        {
            return self.designer_discard_property(&address);
        }
        if self.host_active {
            if self.host.pane == super::host_ui::Pane::Transfer {
                return self.host_transfer_action(super::host_ui::HostAction::TransferCancel);
            }
            if self.host.drag.is_some() {
                return self.cancel();
            }
            return self.host_action(super::host_ui::HostAction::Pane(super::host_ui::Pane::Home));
        }
        if let Some(m) = &mut self.active
            && let Some(chat) = &mut m.chat
            && chat.close_modal()
        {
            m.keyboard.hide();
            m.invalidate();
            return Ok(true);
        }
        if let Some(m) = &mut self.active
            && m.keyboard.visible
        {
            for view in m.text_views.values_mut() {
                view.cancel();
            }
            m.keyboard.hide();
            m.keyboard_pointer = None;
            m.input.cancel_all();
            m.invalidate();
            return Ok(true);
        }
        if let Some((parent, _)) = self.path().rsplit_once('/') {
            self.navigate(parent)?;
            return Ok(true);
        }
        Ok(self.open_entry())
    }
    pub fn tick(&mut self, now: u64, wall: i64) -> Result<bool> {
        if self.finish_local_uninstall()? {
            return Ok(true);
        }
        if self.decision.is_none()
            && let Some(candidate) = self.qualified.as_mut().filter(|q| !q.accepted)
        {
            let mount = &mut candidate.mount;
            mount.viewport = self.viewport.unwrap_or(mount.viewport);
            mount.font = self.font.clone();
            if let Some(chat) = &mut mount.chat {
                if let Some(viewport) = self.layout_viewport {
                    chat.set_header_width(viewport[0]);
                }
                chat.prepare(mount.viewport, mount.keyboard.visible, mount.font.clone());
            }
            mount.invalidate();
            let outcome = (|| {
                let scene = resolve_scene(mount)?;
                let reader = mount
                    .text_document
                    .as_mut()
                    .ok_or("Missing candidate transcript")?;
                reader.sync(&scene, mount.font.as_ref())?;
                reader.tick(now)?;
                Ok::<_, String>(reader.frame().is_some_and(|frame| !frame.loading))
            })();
            match outcome {
                Ok(true) => self.decision = Some(MountDecision::Accept(mount.epoch)),
                Ok(false) => (),
                Err(_) => {
                    // A candidate failure is recoverable. Owners report the
                    // pending rejection and send Cancel before any activation.
                    self.decision = Some(MountDecision::Cancel(mount.epoch));
                    self.qualified = None;
                    self.requested = None;
                    self.auth_required = false;
                }
            }
        }
        if self.host_requested && self.confirmed() && !self.input_busy() {
            return self.show_spaces();
        }
        self.wire_clock = self.wire_clock.max(now);
        let map_changed = self.tick_map_drag(now)?;
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        m.now = now;
        let oib_changed = if super::oib::seed(&m.ui).is_some() {
            m.refresh()?
        } else {
            false
        };
        let (chat_changed, chat_bounds_changed, chat_effects) = if let Some(chat) = &mut m.chat {
            chat.prepare(m.viewport, m.keyboard.visible, m.font.clone());
            let old_bounds = (chat.input_height(), chat.draft_height());
            let (changed, effects) = chat.tick(now, (wall.max(0) as u64).saturating_mul(1000));
            if let Some(doc) = &mut m.text_document {
                chat.update_window(doc)?;
                chat.synchronize_document(doc)?;
            }
            (
                changed,
                old_bounds != (chat.input_height(), chat.draft_height()),
                effects,
            )
        } else {
            (false, false, false)
        };
        if chat_bounds_changed {
            m.invalidate();
        }
        m.doc.set_wall_time(wall);
        m.doc.advance(now);
        let clock = m.wall != wall
            && m.ui
                .selectors
                .iter()
                .any(|s| matches!(s.projection, Projection::WindowSignal { .. }));
        m.wall = wall;
        update_text_views(m, None)?;
        let mut scrolled = false;
        for view in m.text_views.values_mut() {
            scrolled |= view.tick(now);
        }
        if m.text_scroll.as_ref().is_some_and(|s| s.bounds_dirty) {
            let scene = cached_scene(m)?;
            if let Some(s) = &mut m.text_scroll {
                s.sync(&scene)?;
                scrolled = true;
            }
        }
        if let Some(s) = &mut m.text_scroll {
            scrolled |= s.tick(now);
        }
        if let Some(s) = &mut m.map_scroll {
            scrolled |= s.tick(now);
            self.host.scroll = s.info().offset[1];
        }
        m.sync_panel_scroll()?;
        if let Some(s) = &mut m.preview_scroll {
            scrolled |= s.tick(now);
        }
        if let Some(designer) = &mut m.designer {
            scrolled |= designer.tick(now);
        }
        if let Some(s) = &mut m.panel_scroll {
            scrolled |= s.tick(now);
            self.host.panel_scroll = s.info().offset[1];
        }
        if m.text_document.as_ref().is_some_and(|s| s.bounds_dirty) {
            let scene = cached_layout(m)?;
            if let Some(s) = &mut m.text_document {
                s.sync(&scene, m.font.as_ref())?;
            }
        }
        let document_changed = if let Some(s) = &mut m.text_document {
            s.tick(now)?
        } else {
            false
        };
        // The host's renderer credit coalesces paint requests. Input/physics
        // clocks do not impose a second, app-specific presentation cadence.
        let paint = u8::from(chat_changed)
            | (u8::from(document_changed) << 1)
            | (u8::from(chat_effects) << 2);
        if paint & 1 != 0 {
            m.invalidate_local_scene();
        }
        let changed = m.ink.tick(now) || clock || scrolled || map_changed;
        let keyboard_feedback = m.keyboard.feedback_key();
        let edit = m.keyboard.tick(now);
        let feedback_changed = keyboard_feedback != m.keyboard.feedback_key();
        if changed {
            if clock {
                m.invalidate();
            } else {
                m.invalidate_presentation();
            }
        }
        if let Some(edit) = edit {
            return self.hardware_edit(&edit);
        }
        Ok(oib_changed || changed || feedback_changed || paint != 0)
    }
    pub fn ink_outgoing(&mut self, now: u64) -> Vec<u8> {
        self.active.as_mut().map_or_else(Vec::new, |m| {
            if self.managed_transport && m.binding.is_none()
                || m.binding.as_ref().is_some_and(|b| !b.editable)
            {
                return vec![];
            }
            let bytes = m.ink.outgoing(now);
            if bytes.is_empty() {
                bytes
            } else {
                AuxScope {
                    epoch: m.epoch,
                    managed: m.binding.is_some(),
                }
                .wrap(bytes)
            }
        })
    }
    pub fn receive_ink(&mut self, bytes: &[u8], now: u64) -> Result<bool> {
        let Some(bytes) = self.aux_payload(bytes)? else {
            return Ok(false);
        };
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        if !matches!(m.ui.presentation, Some(Presentation::TouchPad { .. })) {
            return Ok(false);
        }
        if !bytes
            .len()
            .is_multiple_of(otrios_interfaces::ink::SERVER_BYTES)
        {
            return Err("Malformed ink batch".into());
        }
        let mut changed = false;
        for point in bytes.chunks_exact(otrios_interfaces::ink::SERVER_BYTES) {
            changed |= m.ink.receive(point, now)?;
        }
        if changed {
            m.invalidate()
        }
        Ok(changed)
    }
    pub fn order_draggable(&self, id: u32) -> bool {
        if self.host_active {
            return self.host_map_draggable(id);
        }
        self.active.as_ref().is_some_and(|m| {
            m.order_editing && order_items(m).iter().any(|i| u32::from(i.node) == id)
        })
    }
    pub fn reflow_frame(&self) -> Result<Option<reflow::Frame>> {
        let Some(m) = &self.active else {
            return Ok(None);
        };
        let items = order_items(m);
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(reflow::Frame {
            identity: format!("{}:{}", m.ui.ui_ref, m.epoch),
            root: self.scene()?.root().clone(),
            nodes: items.iter().map(|i| u32::from(i.node)).collect(),
            editing: m.order_editing,
            drag: m
                .order_drag
                .as_ref()
                .filter(|d| d.moving)
                .map(|d| reflow::DragView {
                    node: u32::from(d.node),
                    position: [
                        (d.position[0] - d.offset[0]).clamp(-8192., 8192.),
                        (d.position[1] - d.offset[1]).clamp(-8192., 8192.),
                    ],
                }),
        }))
    }
    pub fn adjust_control(&mut self, id: u32, dx: i16, dy: i16) -> Result<bool> {
        if self.local_uninstall {
            return Ok(false);
        }
        if self.host_active {
            return self.host_map_adjust(id, dx, dy);
        }
        if self.order_draggable(id) {
            let m = self.active.as_mut().ok_or("No Canvas")?;
            let order = ordered_items(m)?;
            let index = order
                .iter()
                .position(|i| u32::from(i.node) == id)
                .ok_or("Unknown order item")?;
            let delta = if dx != 0 { dx.signum() } else { -dy.signum() };
            let target =
                (index as isize + delta as isize).clamp(0, order.len() as isize - 1) as usize;
            return move_item(m, id as u16, target);
        }
        let scene = self.scene()?;
        let action = InputRouter::activate(scene.as_ref(), id).action;
        let Some(Action::AppEvent(control)) = action else {
            return Ok(false);
        };
        let m = self.active.as_ref().ok_or("No Canvas")?;
        if m.quit_control == Some(control) {
            return Ok(false);
        }
        let h = m.handler(control).ok_or("Unknown control")?;
        if h.kind == GestureKind::Activate {
            return Ok(false);
        }
        let shift = |value: u16, delta: i16| {
            (i32::from(value) + i32::from(delta) * 10).clamp(0, 1000) as u16
        };
        let a = shift(m.number(h.fields[0])?, dx);
        let b = if h.fields.len() > 1 {
            shift(m.number(h.fields[1])?, dy)
        } else {
            0
        };
        self.event(self.epoch(), control, a, b)
    }
    pub fn control_value(&self, id: u32) -> Result<Option<f64>> {
        let scene = self.scene()?;
        let action = InputRouter::activate(scene.as_ref(), id).action;
        let Some(Action::AppEvent(control)) = action else {
            return Ok(None);
        };
        let m = self.active.as_ref().ok_or("No Canvas")?;
        if m.quit_control == Some(control) {
            return Ok(None);
        }
        let h = m.handler(control).ok_or("Unknown control")?;
        if h.kind == GestureKind::Activate {
            return Ok(None);
        }
        Ok(Some(f64::from(m.number(h.fields[0])?) / 10.))
    }
}
impl Mount {
    // Fixed Chat presentation actions are host-owned, including when an older
    // compatible app description does not list newer publication controls.
    fn handler(&self, control: u8) -> Option<Handler> {
        if self.chat.is_some() && (40..=254).contains(&control) {
            Some(Handler {
                control,
                event: 0,
                kind: GestureKind::Activate,
                operation: Operation::Chat(u16::from(control - 40)),
                fields: vec![],
            })
        } else {
            self.ui
                .handlers
                .iter()
                .find(|h| h.control == control)
                .cloned()
        }
    }
    /// Local paint changed within the same layout and document clipping bounds.
    fn invalidate_local_scene(&mut self) {
        self.layout_scene.take();
        self.invalidate_presentation();
    }
    fn number(&self, id: u16) -> Result<u16> {
        match self.read(id)? {
            Value::Number(n) => Ok(n as u16),
            _ => Err("Not a normalized field".into()),
        }
    }
    fn read(&self, id: u16) -> Result<Value> {
        self.pad_preview
            .get(&id)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| self.doc.typed_value(id))
    }
    fn invalidate(&mut self) {
        if let Some(chat) = &mut self.chat {
            chat.prepare(self.viewport, self.keyboard.visible, self.font.clone());
        }
        self.layout_scene.take();
        if let Some(designer) = &mut self.designer {
            designer.invalidate_bounds();
        }
        if let Some(scroll) = &mut self.panel_scroll {
            scroll.bounds_dirty = true;
        }
        if let Some(scroll) = &mut self.preview_scroll {
            scroll.bounds_dirty = true;
        }
        if let Some(s) = &mut self.text_document {
            s.bounds_dirty = true;
        }
        self.text_bounds_dirty = true;
        self.invalidate_presentation();
    }
    fn invalidate_presentation(&mut self) {
        self.scene.take();
    }
    fn sync_panel_scroll(&mut self) -> Result<()> {
        if self.designer.is_some() {
            let scene = cached_layout(self)?;
            if let Some(designer) = &mut self.designer {
                designer.sync(&scene)?;
            }
        }
        if self.preview_scroll.as_ref().is_some_and(|s| s.bounds_dirty) {
            let scene = cached_layout(self)?;
            self.preview_scroll
                .as_mut()
                .ok_or("Missing preview scroll")?
                .sync(&scene)?;
        }
        if self.panel_scroll.as_ref().is_some_and(|s| s.bounds_dirty) {
            let scene = cached_layout(self)?;
            self.panel_scroll
                .as_mut()
                .ok_or("Missing panel scroll")?
                .sync(&scene)?;
        }
        Ok(())
    }
    fn refresh(&mut self) -> Result<bool> {
        let mut oib_changed = self.refresh_oib_view()?;
        if let Some(generation) = self
            .oib_view
            .as_ref()
            .map(|view| view.info.active_generation)
            && self.oib_generation != Some(generation)
            && !self.oib_activation_blocked()
        {
            let (ui, layout) = self.doc.oib_compiled(self.oib_selected)?;
            let previous_ui = std::mem::replace(&mut self.ui, ui);
            let previous = std::mem::replace(&mut self.layout, layout);
            if let Err(error) = resolve_scene(self) {
                self.layout = previous;
                self.ui = previous_ui;
                return Err(error);
            }
            if let Some(seed) = super::oib::seed(&self.ui)
                && let Some(scroll) = &mut self.panel_scroll
            {
                scroll.set_content(seed.inspector_content);
            }
            if let Some(seed) = super::oib::seed(&self.ui)
                && let Some(surface) = &seed.instance_surface
                && let Some(scroll) = &mut self.preview_scroll
            {
                scroll.set_content([0., seed.instances.len() as f32 * surface.preview_stride]);
            }
            self.oib_generation = Some(generation);
            if let Some(view) = &mut self.oib_view {
                view.info.presented_generation = generation;
            }
            self.invalidate();
            oib_changed = true;
        }
        oib_changed |= self.refresh_designer()?;
        for (id, editor) in &mut self.editors {
            if let Value::Text(text) = self.doc.typed_value(*id)?
                && editor.text() != text
            {
                editor.replace(&text);
            }
        }
        let values = selected_values(self)?;
        if values == self.visible {
            return Ok(oib_changed);
        }
        self.visible = values;
        self.invalidate();
        Ok(true)
    }
    fn oib_activation_blocked(&self) -> bool {
        self.designer
            .as_ref()
            .is_some_and(oib_designer::Designer::busy)
            || self.input.active()
            || (self.designer.is_some()
                && (self.keyboard_pointer.is_some()
                    || self.order_drag.is_some()
                    || self.text_views.values().any(TextViewport::active)
                    || (self.keyboard.visible && self.focus.is_some())))
            || self.panel_scroll.as_ref().is_some_and(|s| s.touching())
            || self.preview_scroll.as_ref().is_some_and(|s| s.touching())
            || !self.doc.confirmed()
    }
    fn refresh_oib_view(&mut self) -> Result<bool> {
        if super::oib::seed(&self.ui).is_none() {
            return Ok(false);
        }
        let root = self.doc.oib_root().ok_or("Missing OIB state")?;
        let saving = self.doc.oib_saving();
        let generation = self
            .oib_view
            .as_ref()
            .filter(|v| v.root == root)
            .map(|v| v.info.active_generation)
            .or_else(|| self.doc.oib_generation());
        let deferred = generation != self.oib_generation && self.oib_activation_blocked();
        if self
            .oib_view
            .as_ref()
            .is_some_and(|v| v.root == root && v.saving == saving && v.deferred == deferred)
        {
            return Ok(false);
        }
        let mut info = self.doc.oib_info()?;
        if deferred {
            // These properties still belong to the displayed inspector. Pin only
            // its presentation until the held gesture and pending edits finish;
            // the authority has already reclaimed the deleted object itself.
            if let Some(previous) = &self.oib_view {
                info.properties.clone_from(&previous.info.properties);
            }
        } else if self
            .oib_selected
            .is_none_or(|id| !info.instances.iter().any(|a| a.id == id && a.active))
        {
            self.oib_selected = info.instances.iter().find(|a| a.active).map(|a| a.id);
        }
        info.selected_instance = self.oib_selected;
        info.presented_generation = self.oib_generation.unwrap_or(0);
        let changed = self
            .oib_view
            .as_ref()
            .is_none_or(|v| v.info.status != info.status || v.info.properties != info.properties);
        self.oib_view = Some(OibView {
            root,
            saving,
            deferred,
            info,
        });
        if changed {
            self.invalidate();
        }
        Ok(changed)
    }
}
struct Qualification {
    epoch: u32,
    private: bool,
    writer: [u8; 16],
}
fn qualify(
    ui: Ui,
    state_ref: &str,
    code: &[u8],
    cache: &VecDeque<(Vec<u8>, Vec<u8>)>,
    state: &[u8],
    identity: Qualification,
) -> Result<Mount> {
    let Qualification {
        epoch,
        private,
        writer,
    } = identity;
    validate_ui_format(&ui)?;
    if ui.title.len() > 96
        || ui.selectors.len() > 128
        || ui.handlers.len()
            > if matches!(ui.presentation, Some(Presentation::Chat)) {
                255
            } else {
                128
            }
        || ui.navigation.len() > 32
    {
        return Err("Canvas format/limits invalid".into());
    }
    validate_schema(&ui.state_schema)?;
    let field = |id: u16| ui.state_schema.fields.iter().find(|f| f.id == id);
    let mut locals = std::collections::BTreeSet::new();
    for s in &ui.selectors {
        if !locals.insert(s.local)
            || match s.projection {
                Projection::Nonzero => {
                    !field(s.field).is_some_and(|f| matches!(f.default, Value::Number(_)))
                }
                Projection::OibStatus => s.field != 0 || super::oib::seed(&ui).is_none(),
                Projection::OibCell { cell } => {
                    s.field != 0
                        || !super::oib::seed(&ui)
                            .is_some_and(|seed| seed.cells.iter().any(|c| c.id == cell))
                }
                Projection::KeyboardLabel { key } => {
                    s.field != 0 || key >= crate::dashboard::keyboard::KEY_COUNT
                }
                Projection::KeyboardVisible
                | Projection::SessionControls
                | Projection::SessionAccess { .. }
                | Projection::ReorderLabel => s.field != 0,
                _ => field(s.field).is_none(),
            }
        {
            return Err("Invalid selector".into());
        }
    }
    let mut controls = std::collections::BTreeSet::new();
    for h in &ui.handlers {
        if matches!(ui.presentation, Some(Presentation::Chat))
            && (h.control == 255
                || (40..=254).contains(&h.control)
                    && h.operation != Operation::Chat(u16::from(h.control - 40)))
        {
            return Err("Chat host controls cannot be remapped".into());
        }
        if !controls.insert(h.control)
            || h.fields.len() > 5
            || !h
                .fields
                .iter()
                .all(|id| field(*id).is_some_and(|f| f.writable))
        {
            return Err("Invalid event fields".into());
        }
        let numeric = h
            .fields
            .iter()
            .all(|id| field(*id).is_some_and(|f| matches!(f.default, Value::Number(_))));
        let shape = match h.kind {
            GestureKind::Slider | GestureKind::Dial => {
                h.fields.len() == 1 && numeric && h.operation == Operation::Set
            }
            GestureKind::Pad => h.fields.len() == 2 && numeric && h.operation == Operation::Set,
            GestureKind::Activate => true,
        };
        if !shape {
            return Err("Gesture does not match its typed fields".into());
        }
        let valid = match &h.operation {
            Operation::OibInstance { target, action } => {
                h.fields.is_empty()
                    && super::oib::seed(&ui).is_some_and(|s| {
                        s.instance_surface.is_some()
                            && match action {
                                otrios_interfaces::oib::InstanceAction::RemovePending
                                | otrios_interfaces::oib::InstanceAction::Restore
                                | otrios_interfaces::oib::InstanceAction::ResetDefaults => {
                                    *target == s.main_oib
                                }
                                otrios_interfaces::oib::InstanceAction::Add => {
                                    s.components.iter().any(|c| c.id == *target)
                                }
                                _ => s.instances.iter().any(|i| i.id == *target),
                            }
                    })
            }
            Operation::Oib { cell, .. } => {
                h.fields.is_empty()
                    && super::oib::seed(&ui).is_some_and(|s| s.cells.iter().any(|c| c.id == *cell))
            }
            Operation::Chat(command) => {
                h.fields.is_empty()
                    && *command <= 214
                    && h.control == (*command + 40) as u8
                    && matches!(ui.presentation, Some(Presentation::Chat))
            }
            Operation::Back | Operation::Stress | Operation::SignOut => h.fields.is_empty(),
            Operation::EditOrder => {
                h.fields.is_empty() && matches!(ui.presentation, Some(Presentation::Reorder { .. }))
            }
            Operation::Key(k) => h.fields.is_empty() && *k < crate::dashboard::keyboard::KEY_COUNT,
            Operation::Focus => {
                h.fields.len() == 1
                    && field(h.fields[0]).is_some_and(|f| matches!(f.default, Value::Text(_)))
            }
            Operation::Cycle(v) => {
                h.fields.len() == 1
                    && numeric
                    && !v.is_empty()
                    && v.len() <= 32
                    && v.iter().all(|n| *n <= 1000)
            }
            Operation::Toggle => h.fields.len() == 1 && numeric,
            Operation::Set => !h.fields.is_empty() && numeric,
        };
        if !valid {
            return Err("Invalid declarative operation".into());
        }
    }
    if let Some(p) = &ui.presentation {
        match p {
            Presentation::Oib(_) => super::oib::validate(&ui)?,
            Presentation::Chat => {
                if !ui.state_schema.fields.is_empty() || ui.behavior.is_some() {
                    return Err(
                        "Chat presentation has no replicated composer fields or guest code".into(),
                    );
                }
            }
            Presentation::TextDocument {
                viewport,
                source,
                scale,
                ..
            } => {
                if !(1..=8).contains(scale)
                    || source.decoded_bytes as usize > super::document::MAX_SOURCE_BYTES
                    || source.sha256.len() != 32
                    || source.lz4.is_empty()
                    || source.lz4.len() > MAX_BYTES
                {
                    return Err("Invalid document resource contract".into());
                }
                fn find(
                    n: &otrios_interfaces::layout::TemplateNode,
                    id: u16,
                ) -> Option<&otrios_interfaces::layout::TemplateNode> {
                    if n.id == id {
                        Some(n)
                    } else {
                        n.children.iter().find_map(|c| find(c, id))
                    }
                }
                let layout = Layout::from_bytes(&ui.layout)?;
                for template in &layout.blueprint().templates {
                    let node =
                        find(&template.root, *viewport).ok_or("Missing document viewport")?;
                    if !matches!(
                        node.shape,
                        otrios_interfaces::layout::Shape::Canvas { clip: true }
                    ) || !node.children.is_empty()
                        || node.responder.is_some()
                    {
                        return Err("Document requires an empty clipped canvas".into());
                    }
                }
            }
            Presentation::TextScroll { viewport, content } => {
                if content
                    .iter()
                    .any(|v| !v.is_finite() || !(1. ..=8192.).contains(v))
                {
                    return Err("Invalid scroll content bounds".into());
                }
                fn find(
                    n: &otrios_interfaces::layout::TemplateNode,
                    id: u16,
                ) -> Option<&otrios_interfaces::layout::TemplateNode> {
                    if n.id == id {
                        Some(n)
                    } else {
                        n.children.iter().find_map(|c| find(c, id))
                    }
                }
                let layout = Layout::from_bytes(&ui.layout)?;
                let node = layout
                    .blueprint()
                    .templates
                    .iter()
                    .find_map(|t| find(&t.root, *viewport))
                    .ok_or("Missing text scroll viewport")?;
                if !matches!(
                    node.shape,
                    otrios_interfaces::layout::Shape::Canvas { clip: true }
                ) || node.children.is_empty()
                    || node.children.len() > 256
                    || node.children.iter().any(|n| {
                        !matches!(n.shape, otrios_interfaces::layout::Shape::Text { .. })
                            || n.responder.is_some()
                    })
                {
                    return Err(
                        "Text scroll requires direct text children in a clipped canvas".into(),
                    );
                }
            }
            Presentation::Reorder { items } => {
                let nodes: std::collections::BTreeSet<_> = items.iter().map(|i| i.node).collect();
                let fields: std::collections::BTreeSet<_> = items.iter().map(|i| i.field).collect();
                if items.is_empty()
                    || items.len() > 32
                    || nodes.len() != items.len()
                    || fields.len() != items.len()
                    || !items.iter().all(|i| {
                        field(i.field).is_some_and(|f| {
                            f.writable
                                && matches!(&f.default, Value::Text(s) if reflow::valid_position(s))
                        })
                    })
                {
                    return Err("Invalid reorder mapping".into());
                }
                // Reorder declarations must cover one identical wrap in both profiles.
                fn wraps(
                    flow: &otrios_interfaces::layout::Flow,
                    nodes: &std::collections::BTreeSet<u16>,
                ) -> usize {
                    use otrios_interfaces::layout::FlowKind;
                    match &flow.kind {
                        FlowKind::Wrap { nodes: ids, .. } => usize::from(
                            ids.iter()
                                .copied()
                                .collect::<std::collections::BTreeSet<_>>()
                                == *nodes,
                        ),
                        FlowKind::Group { children, .. } => {
                            children.iter().map(|c| wraps(c, nodes)).sum()
                        }
                        _ => 0,
                    }
                }
                if Layout::from_bytes(&ui.layout)?
                    .blueprint()
                    .composition
                    .as_ref()
                    .is_none_or(|c| {
                        wraps(&c.wide.content, &nodes) != 1 || wraps(&c.tall.content, &nodes) != 1
                    })
                {
                    return Err("Reorder mapping requires a matching wrap in both profiles".into());
                }
            }
            Presentation::View { frame } => {
                if !frame.iter().all(|n| n.is_finite()) || frame[2] <= 0. || frame[3] <= 0. {
                    return Err("Invalid aggregate frame".into());
                }
            }
            Presentation::Editor { fields } => {
                if fields.is_empty()
                    || fields.len() > 16
                    || !fields.iter().all(|id| {
                        field(*id)
                            .is_some_and(|f| f.writable && matches!(f.default, Value::Text(_)))
                    })
                {
                    return Err("Invalid editor declaration".into());
                }
            }
            Presentation::Motion {
                viewport,
                playing,
                speed,
                count,
                load,
                ..
            } => {
                if let Some(id) = viewport
                    && (ui.format != "otrios-canvas-v6"
                        || !Layout::from_bytes(&ui.layout)?
                            .blueprint()
                            .templates
                            .iter()
                            .any(|t| {
                                fn contains(
                                    n: &otrios_interfaces::layout::TemplateNode,
                                    id: u16,
                                ) -> bool {
                                    n.id == id || n.children.iter().any(|c| contains(c, id))
                                }
                                contains(&t.root, *id)
                            }))
                {
                    return Err("Invalid motion viewport".into());
                }
                if ![playing, speed, count, load]
                    .iter()
                    .all(|id| field(**id).is_some_and(|f| matches!(f.default, Value::Number(_))))
                {
                    return Err("Invalid motion fields".into());
                }
            }
            Presentation::TouchPad { control } => {
                if !ui
                    .handlers
                    .iter()
                    .any(|h| h.control == *control && h.kind == GestureKind::Pad)
                {
                    return Err("Invalid trail responder".into());
                }
            }
        }
    }
    let behavior = if let Some(r) = &ui.behavior {
        if !r.writes.iter().all(|id| {
            field(*id).is_some_and(|f| f.writable && matches!(f.default, Value::Number(_)))
        }) {
            return Err("Invalid behavior permissions".into());
        }
        let bytes = if code.is_empty() {
            cache
                .iter()
                .find(|v| v.0 == r.sha256)
                .map(|v| v.1.as_slice())
                .ok_or("Missing behavior cache entry")?
        } else {
            code
        };
        Some(Behavior::load(bytes, r)?)
    } else {
        if !code.is_empty() {
            return Err("Unreferenced code".into());
        }
        None
    };
    let layout = Layout::from_bytes(&ui.layout)?;
    validate_responders(&ui, layout.blueprint())?;
    let doc = StateAdapter::for_ui(&ui, state_ref, state, writer)?;
    let mut editors = BTreeMap::new();
    if let Some(Presentation::Editor { fields }) = &ui.presentation {
        for id in fields {
            let mut b = TextBuffer::default();
            if let Value::Text(text) = doc.typed_value(*id)? {
                b.replace(&text);
            }
            editors.insert(*id, b);
        }
    }
    let text_scroll = if let Some(Presentation::TextScroll { viewport, content }) = &ui.presentation
    {
        Some(super::text_scroll::Scroll::new(*viewport, *content))
    } else {
        None
    };
    let mut m = Mount {
        chat: matches!(ui.presentation, Some(Presentation::Chat))
            .then(super::chat::Client::default),
        text_document: if let Some(Presentation::TextDocument {
            viewport,
            source,
            scale,
            color,
        }) = &ui.presentation
        {
            Some(super::document::Viewport::new(
                *viewport, source, *scale, *color,
            )?)
        } else if matches!(ui.presentation, Some(Presentation::Chat)) {
            Some(super::document::Viewport::new(
                super::chat::VIEWPORT,
                &super::document::encode_resource("")?,
                2,
                super::chat::icons::TEXT,
            )?)
        } else {
            None
        },
        text_scroll,
        map_scroll: None,
        preview_scroll: super::oib::seed(&ui).and_then(|s| {
            s.instance_surface.as_ref().map(|surface| {
                super::text_scroll::Scroll::new(
                    surface.preview_viewport,
                    [0., s.instances.len() as f32 * surface.preview_stride],
                )
            })
        }),
        panel_scroll: super::oib::seed(&ui).map(|seed| {
            super::text_scroll::Scroll::new(seed.inspector_viewport, seed.inspector_content)
        }),
        host_backdrop: None,
        map_drag: None,
        epoch,
        viewport: [1280., 800.],
        designer: super::oib::seed(&ui).and_then(oib_designer::Designer::new),
        ui,
        layout,
        oib_generation: None,
        oib_view: None,
        oib_selected: None,
        doc,
        behavior,
        input: InputRouter::default(),
        state_ref: state_ref.into(),
        binding: None,
        quit_control: None,
        private,
        session_controls: false,
        private_access: false,
        scene: OnceLock::new(),
        layout_scene: OnceLock::new(),
        visible: BTreeMap::new(),
        keyboard: Keyboard::default(),
        text_views: editors
            .keys()
            .map(|id| (*id, TextViewport::default()))
            .collect(),
        text_bounds_dirty: true,
        editors,
        font: None,
        focus: None,
        ink: Default::default(),
        now: 0,
        wall: 0,
        stress: 0,
        stress_last: None,
        stress_updates: 0,
        stress_workload: None,
        keyboard_pointer: None,
        view: None,
        pad_preview: BTreeMap::new(),
        order_editing: false,
        order_drag: None,
    };
    if super::oib::seed(&m.ui).is_some() {
        let (ui, layout) = m.doc.oib_compiled(m.oib_selected)?;
        m.ui = ui;
        m.layout = layout;
        if let Some(seed) = super::oib::seed(&m.ui)
            && let Some(scroll) = &mut m.panel_scroll
        {
            scroll.set_content(seed.inspector_content);
        }
        if let Some(seed) = super::oib::seed(&m.ui)
            && let Some(surface) = &seed.instance_surface
            && let Some(scroll) = &mut m.preview_scroll
        {
            scroll.set_content([0., seed.instances.len() as f32 * surface.preview_stride]);
        }
        m.oib_generation = m.doc.oib_generation();
        m.refresh_oib_view()?;
    }
    m.refresh_designer()?;
    m.visible = selected_values(&m)?;
    m.sync_panel_scroll()?;
    resolve_scene(&m)?;
    Ok(m)
}
fn selected_values(m: &Mount) -> Result<BTreeMap<u16, Value>> {
    let mut values = BTreeMap::new();
    for s in &m.ui.selectors {
        if s.field != 0 && !values.contains_key(&s.field) {
            values.insert(s.field, m.read(s.field)?);
        }
    }
    for item in order_items(m) {
        values.insert(item.field, m.read(item.field)?);
    }
    Ok(values)
}
fn resolve_layout(m: &Mount) -> Result<Scene> {
    let mut bindings = BTreeMap::new();
    for s in &m.ui.selectors {
        let value = if matches!(s.projection, Projection::OibStatus) {
            Value::Text(
                m.oib_view
                    .as_ref()
                    .ok_or("Missing OIB view")?
                    .info
                    .status
                    .clone(),
            )
        } else if let Projection::OibCell { cell } = s.projection {
            Value::Text(
                m.oib_view
                    .as_ref()
                    .and_then(|v| v.info.properties.get(&cell))
                    .ok_or("Missing OIB property view")?
                    .clone(),
            )
        } else if let Projection::KeyboardLabel { key } = s.projection {
            Value::Text(m.keyboard.label(key))
        } else if matches!(s.projection, Projection::ReorderLabel) {
            Value::Text(if m.order_editing { "Done" } else { "Edit" }.into())
        } else if matches!(s.projection, Projection::SessionControls) {
            Value::Boolean(m.private && m.session_controls)
        } else if let Projection::SessionAccess { granted } = s.projection {
            Value::Boolean(m.private_access == granted)
        } else if matches!(s.projection, Projection::KeyboardVisible) {
            Value::Boolean(m.keyboard.visible)
        } else {
            let v = m.read(s.field)?;
            let number = || {
                if let Value::Number(n) = v {
                    Ok(n)
                } else {
                    Err("Numeric projection requires number".to_string())
                }
            };
            match &s.projection {
                Projection::Identity => {
                    if let Some(editor) = m.editors.get(&s.field) {
                        Value::Text(editor.display(m.focus == Some(s.field)))
                    } else {
                        v
                    }
                }
                Projection::Nonzero => Value::Boolean(number()? != 0.),
                Projection::Number { scale, offset } => Value::Number(number()? * scale + offset),
                Projection::Percent => Value::Text(format!("{}%", (number()? / 10.).round())),
                Projection::EnabledText => {
                    Value::Text(if number()? == 0. { "Off" } else { "On" }.into())
                }
                Projection::ChooseText { zero, nonzero } => {
                    Value::Text(if number()? == 0. { zero } else { nonzero }.clone())
                }
                Projection::ChooseColor { zero, nonzero } => {
                    Value::Color(if number()? == 0. { *zero } else { *nonzero })
                }
                Projection::TurnX { center, radius } => Value::Number(
                    center + radius * (number()? * std::f32::consts::TAU / 1000.).sin(),
                ),
                Projection::TurnY { center, radius } => Value::Number(
                    center - radius * (number()? * std::f32::consts::TAU / 1000.).cos(),
                ),
                Projection::NumberText {
                    prefix,
                    suffix,
                    scale,
                    offset,
                    precision,
                } => {
                    if *precision > 4 {
                        return Err("Excessive numeric precision".into());
                    }
                    Value::Text(format!(
                        "{}{:.*}{}",
                        prefix,
                        usize::from(*precision),
                        number()? * scale + offset,
                        suffix
                    ))
                }
                Projection::WindowSignal { seconds } => {
                    Value::TimedSeries(m.doc.signal(s.field, m.wall, *seconds))
                }
                Projection::OibStatus
                | Projection::OibCell { .. }
                | Projection::KeyboardLabel { .. }
                | Projection::KeyboardVisible
                | Projection::SessionControls
                | Projection::SessionAccess { .. }
                | Projection::ReorderLabel => unreachable!(),
            }
        };
        bindings.insert(s.local, value);
    }
    let order: Vec<_> = ordered_items(m)?.iter().map(|i| i.node).collect();
    let scene = m.layout.resolve_ordered(&bindings, m.viewport, &order)?;
    let scene = if let Some(designer) = &m.designer
        && designer.focus_layout()
    {
        let seed = super::oib::seed(&m.ui).ok_or("Missing designer seed")?;
        let back = m.quit_control.or_else(|| {
            m.ui.handlers
                .iter()
                .find(|h| matches!(h.operation, Operation::Back))
                .map(|h| h.control)
        });
        oib_focus_layout::project(
            &scene,
            seed,
            back,
            designer.layers_open(),
            m.keyboard.visible,
            designer.studio,
            designer.management_visible(),
        )?
    } else {
        scene
    };
    if let Some(chat) = &m.chat {
        chat.scene(&scene, m.viewport, m.keyboard.visible)
    } else {
        Ok(scene)
    }
}
fn resolve_scene(m: &Mount) -> Result<Scene> {
    // Candidate admission and tentative resize validation must always resolve
    // current inputs, even while an older visible scene is still retained.
    present_scene(m, &resolve_layout(m)?)
}
fn present_scene(m: &Mount, scene: &Scene) -> Result<Scene> {
    let mut root = scene.root().clone();
    if let Some(backdrop) = &m.host_backdrop {
        root.children
            .insert(root.children.len().min(1), backdrop.clone());
    }
    present_text_views(m, scene, &mut root);
    if let Some(designer) = &m.designer {
        designer.present(
            &mut root,
            scene,
            m.preview_scroll.as_ref().map(|s| s.node()),
            &format!(
                "{} / active {} / shown {}{}",
                m.oib_view
                    .as_ref()
                    .map_or("Live UI unavailable", |v| v.info.status.as_str()),
                m.oib_view.as_ref().map_or(0, |v| v.info.active_generation),
                m.oib_generation.unwrap_or(0),
                if m.oib_view.as_ref().map(|v| v.info.active_generation) != m.oib_generation {
                    " / activation pending"
                } else {
                    ""
                }
            ),
        )?;
        if let Some(font) = &m.font {
            designer.present_editor_fields(&mut root, font);
        }
        designer.present_editor_keyboard(&mut root, &m.keyboard)?;
    }
    if let Some(scroll) = &m.text_scroll {
        scroll.present(&mut root);
    }
    if let Some(scroll) = &m.map_scroll {
        scroll.present(&mut root);
        host_map::present_map(&mut root, scroll.node(), scroll.info());
    }
    if let Some(scroll) = &m.preview_scroll {
        scroll.present(&mut root);
        host_panel::present_panel(&mut root, scroll.node());
    }
    if m.designer.is_none()
        && let Some(scroll) = &m.panel_scroll
    {
        scroll.present(&mut root);
        host_panel::present_panel(&mut root, scroll.node());
    }
    if let Some(preview) = &m.map_drag {
        host_map::present_drag(&mut root, preview, m.viewport);
    }
    if let Some(Presentation::TouchPad { control }) = m.ui.presentation {
        fn append(n: &mut SceneNode, control: u8, ink: &crate::dashboard::ink::Ink) {
            if n.responder == Some(TouchResponder::Drag(Action::AppEvent(control))) {
                n.children
                    .extend(ink.nodes([n.frame.width, n.frame.height], 4_000_000_000));
            } else {
                for c in &mut n.children {
                    append(c, control, ink)
                }
            }
        }
        append(&mut root, control, &m.ink);
    }
    if let Some(Presentation::View { frame }) = m.ui.presentation
        && let Some(view) = &m.view
    {
        let mut content = view.resolve_slots(&BTreeMap::new())?.root().clone();
        fn remap(n: &mut SceneNode) {
            n.id = n.id.saturating_add(3_000_000_000);
            n.responder = None;
            for c in &mut n.children {
                remap(c)
            }
        }
        remap(&mut content);
        let scale = (frame[2] / content.frame.width).min(frame[3] / content.frame.height);
        let width = content.frame.width * scale;
        let height = content.frame.height * scale;
        let mut viewport = crate::dashboard::components::node(
            2_900_000_000,
            otrios_interfaces::scene::Rect::new(
                frame[0] + (frame[2] - width) / 2.,
                frame[1],
                width,
                height,
            ),
            otrios_interfaces::scene::Primitive::Viewport { scale },
        );
        viewport.children.push(content);
        // An aggregate is content beneath the app header and navigation.
        // AppUi reserves the first root child for its backdrop.
        root.children.insert(root.children.len().min(1), viewport);
    }
    if !order_items(m).is_empty() {
        fn local_labels(n: &mut SceneNode, editing: bool, ui: &Ui) {
            if let Some(TouchResponder::Activate(Action::AppEvent(c))) = n.responder
                && ui
                    .handlers
                    .iter()
                    .any(|h| h.control == c && h.operation == Operation::EditOrder)
                && let Some(s) = &mut n.semantics
            {
                s.label = if editing {
                    "Done editing cards"
                } else {
                    "Edit cards"
                }
                .into();
            }
            for c in &mut n.children {
                local_labels(c, editing, ui);
            }
        }
        local_labels(&mut root, m.order_editing, &m.ui);
    }
    if m.chat.is_none() && m.binding.as_ref().is_some_and(|b| !b.editable) {
        fn disable(node: &mut SceneNode, ui: &Ui) {
            let action = match node.responder {
                Some(TouchResponder::Activate(action) | TouchResponder::Drag(action)) => {
                    Some(action)
                }
                _ => None,
            };
            if let Some(Action::AppEvent(control)) = action
                && ui.handlers.iter().any(|h| {
                    h.control == control
                        && !matches!(
                            h.operation,
                            Operation::Back
                                | Operation::SignOut
                                | Operation::OibInstance {
                                    action: otrios_interfaces::oib::InstanceAction::Inspect,
                                    ..
                                }
                        )
                })
            {
                node.responder = None;
                node.semantics = None;
            }
            for child in &mut node.children {
                disable(child, ui);
            }
        }
        disable(&mut root, &m.ui);
    }
    if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
        fn disable_oib(node: &mut SceneNode, ui: &Ui) {
            if let Some(TouchResponder::Activate(Action::AppEvent(control))) = node.responder
                && ui.handlers.iter().any(|h| {
                    h.control == control
                        && matches!(
                            h.operation,
                            Operation::Oib { .. }
                                | Operation::OibInstance {
                                    action: otrios_interfaces::oib::InstanceAction::Add
                                        | otrios_interfaces::oib::InstanceAction::Duplicate
                                        | otrios_interfaces::oib::InstanceAction::Remove
                                        | otrios_interfaces::oib::InstanceAction::RemovePending,
                                    ..
                                }
                                | Operation::OibInstance {
                                    action: otrios_interfaces::oib::InstanceAction::Restore
                                        | otrios_interfaces::oib::InstanceAction::ResetDefaults,
                                    ..
                                }
                        )
                })
            {
                node.responder = None;
                node.semantics = None;
            }
            for child in &mut node.children {
                disable_oib(child, ui);
            }
        }
        disable_oib(&mut root, &m.ui);
    }
    if let Some(control) = m.quit_control
        && m.chat.is_none()
        && !m.designer.as_ref().is_some_and(|d| d.focus_layout())
    {
        host_map::present_breadcrumb(&mut root, m, control);
    }
    let scene = Scene::new(root)?;
    if m.designer.is_some() && scene.controls().len() > 128 {
        return Err("Designer combined control budget exceeded".into());
    }
    Ok(scene)
}
fn order_items(m: &Mount) -> &[OrderItem] {
    if let Some(Presentation::Reorder { items }) = &m.ui.presentation {
        items
    } else {
        &[]
    }
}
fn position(m: &Mount, item: &OrderItem) -> Result<String> {
    let value = m.read(item.field)?;
    if let Value::Text(s) = value
        && reflow::valid_position(&s)
    {
        return Ok(s);
    }
    if let Some(f) = m.ui.state_schema.fields.iter().find(|f| f.id == item.field)
        && let Value::Text(s) = &f.default
    {
        return Ok(s.clone());
    }
    Err("Invalid position field".into())
}
fn ordered_items(m: &Mount) -> Result<Vec<OrderItem>> {
    let mut items: Vec<_> = order_items(m)
        .iter()
        .map(|i| Ok((position(m, i)?, i.clone())))
        .collect::<Result<_>>()?;
    items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.field.cmp(&b.1.field)));
    Ok(items.into_iter().map(|(_, i)| i).collect())
}
fn move_item(m: &mut Mount, node: u16, target: usize) -> Result<bool> {
    let mut order = ordered_items(m)?;
    let index = order
        .iter()
        .position(|i| i.node == node)
        .ok_or("Unknown order item")?;
    if target == index {
        return Ok(false);
    }
    let item = order.remove(index);
    let target = target.min(order.len());
    let left = target
        .checked_sub(1)
        .map(|i| position(m, &order[i]))
        .transpose()?;
    let right = order.get(target).map(|i| position(m, i)).transpose()?;
    // Legacy/malformed equal ranks have deterministic order but no open interval.
    // Do not rewrite neighbors to repair them implicitly.
    let Ok(key) = reflow::between(left.as_deref(), right.as_deref(), item.field) else {
        return Ok(false);
    };
    if m.doc.set_typed(&[(item.field, Value::Text(key))])? {
        m.refresh()
    } else {
        Ok(false)
    }
}
fn order_pointer(m: &mut Mount, scene: &Scene, event: PointerEvent) -> Result<Option<bool>> {
    if event.position.iter().any(|v| !v.is_finite()) {
        m.order_drag = None;
        m.invalidate();
        return Ok(Some(true));
    }
    if event.phase == PointerPhase::Down && m.order_drag.is_none() {
        let hit = scene.hit_path(event.position).iter().find_map(|n| {
            order_items(m)
                .iter()
                .find(|i| u32::from(i.node) == n.node.id)
                .map(|i| (i.node, n.origin))
        });
        if let Some((node, origin)) = hit {
            m.order_drag = Some(OrderDrag {
                pointer: event.id,
                node,
                start: event.position,
                offset: [event.position[0] - origin[0], event.position[1] - origin[1]],
                position: event.position,
                moving: false,
            });
            m.input.cancel_all();
            m.invalidate();
            return Ok(Some(true));
        }
    }
    let Some(mut drag) = m.order_drag.take() else {
        return Ok(None);
    };
    if drag.pointer != event.id {
        m.order_drag = Some(drag);
        return Ok(Some(false));
    }
    if matches!(event.phase, PointerPhase::Up | PointerPhase::Cancel) {
        if event.phase == PointerPhase::Up && drag.position != event.position {
            m.order_drag = Some(drag);
            order_pointer(
                m,
                scene,
                PointerEvent {
                    phase: PointerPhase::Move,
                    ..event
                },
            )?;
            m.order_drag = None;
        }
        m.invalidate();
        return Ok(Some(true));
    }
    if event.phase != PointerPhase::Move {
        m.order_drag = Some(drag);
        return Ok(Some(false));
    }
    drag.position = event.position;
    drag.moving |=
        (drag.start[0] - event.position[0]).hypot(drag.start[1] - event.position[1]) >= 8.;
    if drag.moving {
        let order = ordered_items(m)?;
        let target = order
            .iter()
            .enumerate()
            .filter_map(|(index, i)| {
                scene
                    .root()
                    .children
                    .iter()
                    .find(|n| n.id == u32::from(i.node))
                    .map(|n| {
                        let x = n.frame.x + n.frame.width / 2. - event.position[0];
                        let y = n.frame.y + n.frame.height / 2. - event.position[1];
                        (index, x * x + y * y)
                    })
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|v| v.0)
            .unwrap_or(0);
        move_item(m, drag.node, target)?;
    }
    m.order_drag = Some(drag);
    m.invalidate();
    Ok(Some(true))
}
fn cached_scene(m: &Mount) -> Result<Arc<Scene>> {
    if let Some(scene) = m.scene.get() {
        return Ok(scene.clone());
    }
    let layout = cached_layout(m)?;
    let scene = Arc::new(present_scene(m, &layout)?);
    let _ = m.scene.set(scene.clone());
    Ok(scene)
}
fn cached_layout(m: &Mount) -> Result<Arc<Scene>> {
    Ok(if let Some(scene) = m.layout_scene.get() {
        scene.clone()
    } else {
        let scene = Arc::new(resolve_layout(m)?);
        let _ = m.layout_scene.set(scene.clone());
        scene
    })
}
fn validate_responders(ui: &Ui, blueprint: &otrios_interfaces::layout::Blueprint) -> Result<()> {
    use otrios_interfaces::layout::{Responder, TemplateNode};
    fn node(n: &TemplateNode, ui: &Ui) -> Result<()> {
        match n.responder {
            None | Some(Responder::Bubble | Responder::Consume) => {}
            Some(Responder::AppNavigate(id)) if ui.navigation.iter().any(|n| n.action == id) => {}
            Some(Responder::AppEvent { control, drag })
                if ui
                    .handlers
                    .iter()
                    .any(|h| h.control == control && drag == (h.kind != GestureKind::Activate)) => {
            }
            _ => return Err("App UI contains a legacy or unbound responder".into()),
        }
        for c in &n.children {
            node(c, ui)?
        }
        Ok(())
    }
    for t in &blueprint.templates {
        node(&t.root, ui)?
    }
    Ok(())
}

// Keep the authored text node's identity as the viewport. Its rendered child
// uses a reserved local presentation ID, and never becomes a new responder.
const TEXT_RUN_ID: u32 = 4_100_000_000;
struct EditorFrame {
    field: u16,
    node: u32,
    origin: [f32; 2],
    factor: f32,
    width: f32,
    scale: f32,
    responder: u32,
}
fn editor_frames(m: &Mount, scene: &Scene) -> Vec<EditorFrame> {
    fn text_node(n: &SceneNode, value: &str) -> Option<u32> {
        if matches!(&n.primitive, otrios_interfaces::scene::Primitive::Text { value: v, .. } if v == value || n.id >= TEXT_RUN_ID)
        {
            return Some(n.id);
        }
        n.children.iter().find_map(|n| text_node(n, value))
    }
    scene
        .controls()
        .into_iter()
        .filter_map(|owner| {
            let Some(TouchResponder::Activate(Action::AppEvent(control))) = owner.node.responder
            else {
                return None;
            };
            let field = *m
                .ui
                .handlers
                .iter()
                .find(|h| h.control == control && h.operation == Operation::Focus)?
                .fields
                .first()?;
            let editor = m.editors.get(&field)?;
            let id = text_node(owner.node, &editor.display(m.focus == Some(field)))?;
            let path = scene.path(id)?;
            let text = path.last()?;
            let otrios_interfaces::scene::Primitive::Text { scale, .. } = text.node.primitive
            else {
                return None;
            };
            let viewport = if id >= TEXT_RUN_ID {
                path.get(path.len() - 2)?
            } else {
                text
            };
            Some(EditorFrame {
                field,
                node: viewport.node.id,
                origin: viewport.origin,
                factor: viewport.scale,
                width: viewport.node.frame.width,
                scale: scale as f32,
                responder: owner.node.id,
            })
        })
        .collect()
}
fn update_text_views(m: &mut Mount, follow: Option<u16>) -> Result<()> {
    if m.editors.is_empty() {
        return Ok(());
    }
    // Panning changes presentation only. Font advances and resolved editor
    // frames remain valid until content, focus, font or layout invalidation.
    if !m.text_bounds_dirty && follow.is_none() {
        return Ok(());
    }
    let Some(font) = m.font.clone() else {
        return Ok(());
    };
    let scene = cached_scene(m)?;
    let mut clamped = false;
    for frame in editor_frames(m, &scene) {
        let editor = &m.editors[&frame.field];
        let view = m
            .text_views
            .get_mut(&frame.field)
            .ok_or("Missing text viewport")?;
        let marker = if m.focus == Some(frame.field) {
            font.text_width("|", frame.scale)
        } else {
            0.
        };
        let old = view.offset;
        view.bounds(
            font.text_width(editor.text(), frame.scale) + marker,
            frame.width,
        );
        clamped |= old != view.offset;
        if follow == Some(frame.field) {
            view.follow(
                editor.caret_x(&font, frame.scale),
                marker,
                frame.width,
                m.now,
            );
        }
    }
    m.text_bounds_dirty = false;
    if clamped {
        m.invalidate_presentation();
    }
    Ok(())
}
fn present_text_views(m: &Mount, scene: &Scene, root: &mut SceneNode) {
    fn wrap(n: &mut SceneNode, frame: &EditorFrame, m: &Mount) {
        if n.id == frame.node {
            let editor = &m.editors[&frame.field];
            let offset = m.text_views.get(&frame.field).map_or(0., |v| v.offset);
            let value = editor.display(m.focus == Some(frame.field));
            let x = -offset;
            let mut text = n.clone();
            text.id = TEXT_RUN_ID + u32::from(frame.field);
            text.frame.x = x;
            text.frame.y = 0.;
            text.frame.width = (frame.width - x + 1.).clamp(1., 8192.);
            if let otrios_interfaces::scene::Primitive::Text { value: v, .. } = &mut text.primitive
            {
                *v = value;
            }
            n.primitive = otrios_interfaces::scene::Primitive::Canvas { clip: true };
            n.children = vec![text];
        } else {
            for c in &mut n.children {
                wrap(c, frame, m);
            }
        }
    }
    for frame in editor_frames(m, scene) {
        wrap(root, &frame, m);
    }
}
fn text_pointer(
    m: &mut Mount,
    scene: &Scene,
    event: PointerEvent,
    at: u64,
) -> Result<Option<bool>> {
    let frames = editor_frames(m, scene);
    let owner = m
        .text_views
        .iter()
        .find(|(_, v)| v.owns(event.id))
        .map(|(id, _)| *id);
    let frame = if let Some(field) = owner {
        frames.into_iter().find(|f| f.field == field)
    } else if event.phase == PointerPhase::Down {
        let path = scene.hit_path(event.position);
        frames
            .into_iter()
            .find(|f| path.iter().any(|n| n.node.id == f.responder))
    } else {
        None
    };
    let Some(frame) = frame else { return Ok(None) };
    // A second contact must never steal an existing text or keyboard gesture.
    if owner.is_none()
        && (m.text_views.values().any(TextViewport::active) || m.keyboard_pointer.is_some())
    {
        return Ok(Some(false));
    }
    update_text_views(m, None)?;
    let view = m
        .text_views
        .get_mut(&frame.field)
        .ok_or("Missing text viewport")?;
    let x = (event.position[0] - frame.origin[0]) / frame.factor;
    let mut changed_layout = false;
    match event.phase {
        PointerPhase::Down => view.down(event.id, x, at),
        PointerPhase::Move => view.motion(x, at),
        PointerPhase::Cancel => view.cancel(),
        PointerPhase::Up => {
            let dragged = view.up(x, at);
            if !dragged
                && scene
                    .hit_path(event.position)
                    .iter()
                    .any(|n| n.node.id == frame.responder)
            {
                let font = m
                    .font
                    .as_ref()
                    .ok_or("Font resource required for caret placement")?;
                let editor = m.editors.get_mut(&frame.field).ok_or("Missing editor")?;
                let cursor = editor.hit_test(
                    x + view.offset,
                    font,
                    frame.scale,
                    m.focus == Some(frame.field),
                );
                editor.place_cursor(cursor);
                // Focus changes stop other pans but do not request caret following.
                for v in m.text_views.values_mut() {
                    v.cancel();
                }
                m.focus = Some(frame.field);
                m.keyboard.focus();
                m.keyboard_pointer = None;
                m.input.cancel_all();
                changed_layout = true;
            }
        }
    }
    if changed_layout {
        m.invalidate();
    } else {
        m.invalidate_presentation();
    }
    Ok(Some(true))
}
