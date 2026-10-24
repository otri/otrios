// SPDX-License-Identifier: LGPL-3.0-only
//! One admitted app activation. Durable state is authoritative; all client state is RAM only.
use super::*;
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
struct Mount {
    epoch: u32,
    viewport: [f32; 2],
    ui: Ui,
    layout: Layout,
    doc: StateAdapter,
    behavior: Option<Behavior>,
    input: InputRouter,
    state_ref: String,
    private: bool,
    session_controls: bool,
    scene: OnceLock<Arc<Scene>>,
    visible: BTreeMap<u16, Value>,
    keyboard: Keyboard,
    editors: BTreeMap<u16, TextBuffer>,
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
struct Request {
    epoch: u32,
    pair: (String, String),
    sent: bool,
    reconnecting: bool,
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
    pub sampled_monotonic_ms: u64,
    pub dispatch: DispatchUsage,
    pub state: StateUsage,
    pub behavior_cache_entries: usize,
    pub behavior_cache_bytes: usize,
    pub behavior_hash: Option<String>,
    pub reflow_order: Vec<u16>,
    pub reflow_editing: bool,
    pub wire: WireUsage,
}
#[derive(Default)]
pub struct Client {
    viewport: Option<[f32; 2]>,
    wire_clock: u64,
    decode_budget: DecodeBudget,
    generation: u32,
    requested: Option<Request>,
    decision: Option<MountDecision>,
    active: Option<Mount>,
    cache: VecDeque<(Vec<u8>, Vec<u8>)>,
    local: bool,
    navigation: Option<(String, String)>,
    catalog: Option<Catalog>,
    catalog_hash: Vec<u8>,
    previous_catalog: Option<(Catalog, Vec<u8>)>,
    font: Option<Arc<crate::dashboard::font::Font>>,
    auth_required: bool,
    session_controls: bool,
    sign_out_requested: bool,
    writer: Option<[u8; 16]>,
    dispatch: DispatchPolicy,
}
impl Client {
    /// Local presentation only. A rejected size retains the old scene and state.
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
        self.font = Some(font);
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
                || e.app.id.is_empty()
                || e.app.id.len() > 64
                || !e
                    .app
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-')
                || e.app.ui_ref != format!("app:{}/ui", e.app.id)
                || e.app.state_ref.len() > 128
                || !refs.insert(e.app.ui_ref.clone())
            {
                return Err("Invalid or duplicate catalog entry".into());
            }
        }
        self.catalog_hash = hash(bytes);
        self.catalog = Some(c);
        Ok(())
    }
    /// Admit compatible UI document revisions without replacing executable code.
    /// The staged mount qualifies its new coverage before replacing the old view.
    pub fn can_update_catalog(&mut self) -> bool {
        self.requested.is_none()
            && self.navigation.is_none()
            && !self.input_busy()
            && self.confirmed()
    }
    pub fn update_catalog(&mut self, bytes: &[u8]) -> Result<bool> {
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
    pub fn title(&self) -> String {
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
        self.active.as_ref().is_some_and(|m| m.private)
    }
    pub fn input_busy(&self) -> bool {
        self.active.as_ref().is_some_and(|m| {
            m.input.active() || m.keyboard_pointer.is_some() || m.order_drag.is_some()
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
        self.local || self.active.as_mut().is_none_or(|m| m.doc.confirmed())
    }
    /// A reconnect/mount handshake must not strand an idle old runtime. Only
    /// unconfirmed user intent and active input prevent code replacement.
    pub fn reload_ready(&self) -> bool {
        !self.input_busy() && (self.local || self.active.as_ref().is_none_or(|m| m.doc.confirmed()))
    }
    pub fn presentation(&self) -> Option<Presentation> {
        self.active.as_ref().and_then(|m| m.ui.presentation.clone())
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
            playing: self.value(playing)? > 0,
            speed: 0.25 + f32::from(self.value(speed)?) * 0.00375,
            count: (1. + f32::from(self.value(count)?) * 0.063).round() as u16,
        };
        i.validate()?;
        Ok(Some((three_d, i)))
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
            sampled_monotonic_ms: self.wire_clock,
            dispatch: self.dispatch_usage(),
            state: self
                .active
                .as_ref()
                .map_or_else(StateUsage::default, |m| m.doc.state_usage()),
            behavior_cache_entries: self.cache.len(),
            behavior_cache_bytes: self.cache.iter().map(|(_, bytes)| bytes.len()).sum(),
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
        self.decision = None;
        self.auth_required = false;
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
        self.auth_required = false;
        if let Some(r) = &mut self.requested {
            r.sent = false
        }
    }
    pub fn clear_private(&mut self) {
        if self.private() {
            self.active = None;
        }
        self.auth_required = false;
        let state = self
            .catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|e| e.app.id == "home"))
            .map(|e| e.app.state_ref.clone())
            .unwrap_or_else(|| "app:home/state-v3".into());
        self.open("app:home/ui", &state);
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        if self.catalog.is_none() {
            return Ok(None);
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
                MountDecision::Accept(epoch) => ClientMessage::AcceptMount { epoch },
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
                known_code: self.cache.iter().map(|v| v.0.clone()).collect(),
                writer: writer.to_vec(),
                resume: self
                    .active
                    .as_ref()
                    .filter(|m| m.state_ref == r.pair.1)
                    .map_or(0, |m| m.doc.resume_sequence()),
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
        })?)?;
        self.local = true;
        self.decision = None;
        Ok(changed)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool> {
        self.decode_budget.admit(bytes, self.wire_clock)?;
        match decode_server(bytes)? {
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
            } => {
                let Some(r) = &self.requested else {
                    return Ok(false);
                };
                if r.epoch != epoch {
                    return Ok(false);
                }
                let reconnecting = r.reconnecting;
                let candidate = (|| -> Result<Mount> {
                    let e = self
                        .catalog
                        .as_ref()
                        .and_then(|c| {
                            c.apps
                                .iter()
                                .find(|e| e.app.ui_ref == r.pair.0 && e.app.state_ref == r.pair.1)
                        })
                        .ok_or("App pair not admitted")?;
                    if hash(&ui) != e.ui_hash || ui_hash != e.ui_hash {
                        return Err("UI differs from admitted catalog".into());
                    }
                    let ui = decode_ui(&ui)?;
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
                    candidate.viewport = self.viewport.unwrap_or([1280., 800.]);
                    resolve_scene(&candidate)?;
                    candidate.invalidate();
                    candidate.doc.set_dispatch_policy(self.dispatch);
                    if let Some(old) = &self.active
                        && old.state_ref == candidate.state_ref
                        && old.ui.state_schema == candidate.ui.state_schema
                    {
                        candidate.doc.restore_pending(&old.doc)?;
                        candidate.now = old.now;
                        candidate.wall = old.wall;
                        candidate.refresh()?;
                    }
                    if self.previous_catalog.is_some()
                        && let Some(old) = &self.active
                    {
                        candidate.view = old.view.clone();
                        candidate.keyboard = old.keyboard.clone();
                        candidate.editors = old.editors.clone();
                        candidate.focus = old.focus;
                        candidate.ink = old.ink.clone();
                        candidate.pad_preview = old.pad_preview.clone();
                        candidate.order_editing = old.order_editing;
                    }
                    candidate.session_controls = self.session_controls;
                    candidate.invalidate();
                    cached_scene(&candidate)?;
                    Ok(candidate)
                })();
                match candidate {
                    Ok(mut candidate) => {
                        if self.previous_catalog.take().is_some() {
                            let mut current = self.active.take().ok_or("Missing app activation")?;
                            if current.ui.behavior != candidate.ui.behavior {
                                current.behavior = candidate.behavior;
                            }
                            current.ui = candidate.ui;
                            current.layout = candidate.layout;
                            current.doc = candidate.doc;
                            current.epoch = candidate.epoch;
                            current.visible = candidate.visible;
                            current.invalidate();
                            candidate = current;
                        }
                        if reconnecting
                            && self.active.as_ref().is_some_and(|old| {
                                old.ui == candidate.ui && old.state_ref == candidate.state_ref
                            })
                        {
                            let mut current =
                                self.active.take().ok_or("Missing reconnect activation")?;
                            current.doc = candidate.doc;
                            current.epoch = candidate.epoch;
                            current.session_controls = candidate.session_controls;
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
                        self.active = Some(candidate);
                        self.decision = Some(MountDecision::Accept(epoch));
                        self.local = false;
                        self.requested = None;
                        self.auth_required = false;
                        Ok(true)
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
                if self
                    .active
                    .as_ref()
                    .is_some_and(|m| m.epoch == epoch && m.private)
                {
                    let old = self.active.take().ok_or("Missing private activation")?;
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
                    return Ok(true);
                }
                if self.requested.as_ref().is_some_and(|r| r.epoch == epoch) {
                    if self.previous_catalog.is_some() {
                        self.reject_catalog_update();
                        self.decision = Some(MountDecision::Cancel(epoch));
                        self.requested = None;
                        return Err("UI update access rejected".into());
                    }
                    self.auth_required = true;
                }
                Ok(false)
            }
            ServerMessage::Error { epoch, message } => {
                if self.requested.as_ref().is_some_and(|r| r.epoch == epoch) {
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
    pub fn scene(&self) -> Result<Arc<Scene>> {
        cached_scene(self.active.as_ref().ok_or("No Canvas")?)
    }
    pub fn event(&mut self, epoch: u32, control: u8, a: u16, b: u16) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        let h = self
            .active
            .as_ref()
            .and_then(|m| m.ui.handlers.iter().find(|h| h.control == control))
            .cloned()
            .ok_or("Unknown app event")?;
        if self.requested.as_ref().is_some_and(|r| !r.reconnecting)
            && !matches!(h.operation, Operation::Back)
        {
            return Ok(false);
        }
        match h.operation {
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
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        let scene = self.scene()?;
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.order_editing
            && self.requested.as_ref().is_none_or(|r| r.reconnecting)
            && let Some(changed) = order_pointer(m, &scene, event)?
        {
            return Ok(changed);
        }
        let key = scene
            .hit_path(event.position)
            .iter()
            .rev()
            .find_map(|node| {
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
            m.invalidate();
            if let Some(edit) = edit {
                self.hardware_edit(&edit)?;
            }
            return Ok(true);
        }
        let delivery = m.input.dispatch(scene.clone(), event);
        // Activate responders retain release-inside/cancellation semantics. Resolve
        // the caret against the displayed field before focus adds its local marker.
        let caret = if self.requested.is_none()
            && let Some(Action::AppEvent(control)) = delivery.action
            && let Some(handler) =
                m.ui.handlers
                    .iter()
                    .find(|h| h.control == control && h.operation == Operation::Focus)
            && let Some(field) = handler.fields.first()
            && let Some(editor) = m.editors.get(field)
            && let Some(responder) = delivery.responder
            && let Some(path) = scene.path(responder)
        {
            fn bound_text(n: &SceneNode, value: &str) -> Option<u32> {
                if matches!(&n.primitive, otrios_interfaces::scene::Primitive::Text { value: v, .. } if v == value)
                {
                    return Some(n.id);
                }
                n.children.iter().find_map(|n| bound_text(n, value))
            }
            let focused = m.focus == Some(*field);
            let text = editor.display(focused);
            let located = path
                .last()
                .and_then(|owner| bound_text(owner.node, &text))
                .and_then(|id| scene.path(id));
            if let Some(text) = located.as_ref().and_then(|p| p.last())
                && let otrios_interfaces::scene::Primitive::Text { scale, .. } = text.node.primitive
            {
                let font = self
                    .font
                    .as_ref()
                    .ok_or("Font resource required for caret placement")?;
                Some((
                    *field,
                    editor.hit_test(
                        (event.position[0] - text.origin[0]) / text.scale,
                        font,
                        scale as f32,
                        focused,
                    ),
                ))
            } else {
                None
            }
        } else {
            None
        };
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
        if changed
            && let Some((field, cursor)) = caret
            && let Some(m) = &mut self.active
            && m.focus == Some(field)
            && let Some(editor) = m.editors.get_mut(&field)
        {
            editor.place_cursor(cursor);
            m.invalidate();
        }
        if matches!(event.phase, PointerPhase::Up | PointerPhase::Cancel)
            && let Some(m) = &mut self.active
        {
            m.doc.flush_pending();
        }
        Ok(changed)
    }
    pub fn activate(&mut self, epoch: u32, id: u32) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
            return Ok(false);
        }
        if self.order_draggable(id) {
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
                .and_then(|m| m.ui.handlers.iter().find(|h| h.control == control))
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
            return self.event(self.epoch(), control, a, b);
        }
        match d.action {
            Some(Action::AppNavigate(id)) => {
                if let Some(n) = self
                    .active
                    .as_ref()
                    .and_then(|m| m.ui.navigation.iter().find(|n| n.action == id))
                    .cloned()
                {
                    self.open(&n.ui_ref, &n.state_ref);
                    return Ok(true);
                }
            }
            Some(Action::AppEvent(id)) => return self.event(self.epoch(), id, 0, 0),
            _ => {}
        }
        Ok(false)
    }
    pub fn keyboard_visible(&self) -> bool {
        self.active.as_ref().is_some_and(|m| m.keyboard.visible)
    }
    pub fn editing(&self) -> bool {
        self.active.as_ref().is_some_and(|m| m.focus.is_some())
    }
    pub fn show_keyboard(&mut self) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if let Some(Presentation::Editor { fields }) = &m.ui.presentation {
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
        if self.requested.as_ref().is_some_and(|r| !r.reconnecting) || self.active.is_none() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
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
                    m.keyboard.focus();
                    m.keyboard_pointer = None;
                    m.input.cancel_all();
                    m.invalidate();
                    return Ok(true);
                }
            }
            Edit::Enter => {
                m.keyboard.hide();
                m.keyboard_pointer = None;
                m.input.cancel_all();
                m.invalidate();
                return Ok(true);
            }
            _ => {}
        }
        let editor = m.editors.get_mut(&field).ok_or("Undeclared editor")?;
        if !editor.edit(edit) {
            return Ok(false);
        }
        m.doc
            .set_typed(&[(field, Value::Text(editor.text().into()))])?;
        m.refresh()?;
        m.invalidate();
        Ok(true)
    }
    pub fn cancel(&mut self) -> Result<bool> {
        if let Some(m) = &mut self.active {
            m.input.cancel_all();
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
        if let Some(m) = &mut self.active
            && m.keyboard.visible
        {
            m.keyboard.hide();
            m.keyboard_pointer = None;
            m.input.cancel_all();
            m.invalidate();
            return Ok(true);
        }
        let state = self
            .catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|e| e.app.id == "home"))
            .map(|e| e.app.state_ref.clone())
            .unwrap_or_else(|| "app:home/state-v3".into());
        self.open("app:home/ui", &state);
        Ok(true)
    }
    pub fn tick(&mut self, now: u64, wall: i64) -> Result<bool> {
        self.wire_clock = self.wire_clock.max(now);
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        m.now = now;
        m.doc.set_wall_time(wall);
        m.doc.advance(now);
        let clock = m.wall != wall
            && m.ui
                .selectors
                .iter()
                .any(|s| matches!(s.projection, Projection::WindowSignal { .. }));
        m.wall = wall;
        let changed = m.ink.tick(now) || clock;
        let edit = m.keyboard.tick(now);
        if changed {
            m.invalidate();
        }
        if let Some(edit) = edit {
            return self.hardware_edit(&edit);
        }
        Ok(changed)
    }
    pub fn ink_outgoing(&mut self, now: u64) -> Vec<u8> {
        self.active
            .as_mut()
            .map_or_else(Vec::new, |m| m.ink.outgoing(now))
    }
    pub fn receive_ink(&mut self, bytes: &[u8], now: u64) -> Result<bool> {
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
        let h =
            m.ui.handlers
                .iter()
                .find(|h| h.control == control)
                .ok_or("Unknown control")?;
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
        let h =
            m.ui.handlers
                .iter()
                .find(|h| h.control == control)
                .ok_or("Unknown control")?;
        if h.kind == GestureKind::Activate {
            return Ok(None);
        }
        Ok(Some(f64::from(m.number(h.fields[0])?) / 10.))
    }
}
impl Mount {
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
        self.scene.take();
    }
    fn refresh(&mut self) -> Result<bool> {
        for (id, editor) in &mut self.editors {
            if let Value::Text(text) = self.doc.typed_value(*id)?
                && editor.text() != text
            {
                editor.replace(&text);
            }
        }
        let values = selected_values(self)?;
        if values == self.visible {
            return Ok(false);
        }
        self.visible = values;
        self.invalidate();
        Ok(true)
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
    if ui.format != "otrios-canvas-v2"
        || ui.title.len() > 96
        || ui.selectors.len() > 128
        || ui.handlers.len() > 128
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
                Projection::KeyboardLabel { key } => {
                    s.field != 0 || key >= crate::dashboard::keyboard::KEY_COUNT
                }
                Projection::KeyboardVisible
                | Projection::SessionControls
                | Projection::ReorderLabel => s.field != 0,
                _ => field(s.field).is_none(),
            }
        {
            return Err("Invalid selector".into());
        }
    }
    let mut controls = std::collections::BTreeSet::new();
    for h in &ui.handlers {
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
                playing,
                speed,
                count,
                load,
                ..
            } => {
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
    let mut m = Mount {
        epoch,
        viewport: [1280., 800.],
        ui,
        layout,
        doc,
        behavior,
        input: InputRouter::default(),
        state_ref: state_ref.into(),
        private,
        session_controls: false,
        scene: OnceLock::new(),
        visible: BTreeMap::new(),
        keyboard: Keyboard::default(),
        editors,
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
    m.visible = selected_values(&m)?;
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
fn resolve_scene(m: &Mount) -> Result<Scene> {
    let mut bindings = BTreeMap::new();
    for s in &m.ui.selectors {
        let value = if let Projection::KeyboardLabel { key } = s.projection {
            Value::Text(m.keyboard.label(key))
        } else if matches!(s.projection, Projection::ReorderLabel) {
            Value::Text(if m.order_editing { "Done" } else { "Edit" }.into())
        } else if matches!(s.projection, Projection::SessionControls) {
            Value::Boolean(m.private && m.session_controls)
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
                Projection::KeyboardLabel { .. }
                | Projection::KeyboardVisible
                | Projection::SessionControls
                | Projection::ReorderLabel => unreachable!(),
            }
        };
        bindings.insert(s.local, value);
    }
    let order: Vec<_> = ordered_items(m)?.iter().map(|i| i.node).collect();
    let scene = m.layout.resolve_ordered(&bindings, m.viewport, &order)?;
    let mut root = scene.root().clone();
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
        let mut viewport = crate::dashboard::components::node(
            2_900_000_000,
            otrios_interfaces::scene::Rect::new(frame[0], frame[1], frame[2], frame[3]),
            otrios_interfaces::scene::Primitive::Viewport {
                scale: frame[2] / 1280.,
            },
        );
        viewport.children.push(content);
        // An aggregate is content beneath the app header and navigation.
        // AppUi reserves the first root child for its backdrop.
        root.children.insert(root.children.len().min(1), viewport);
    }
    if !order.is_empty() {
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
    Scene::new(root)
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
    let scene = Arc::new(resolve_scene(m)?);
    let _ = m.scene.set(scene.clone());
    Ok(scene)
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
