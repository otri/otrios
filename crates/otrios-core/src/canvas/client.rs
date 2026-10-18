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
struct Mount {
    epoch: u32,
    ui: Ui,
    layout: Layout,
    doc: Document,
    session: Session,
    behavior: Option<Behavior>,
    input: InputRouter,
    state_ref: String,
    private: bool,
    session_controls: bool,
    scene: OnceLock<Arc<Scene>>,
    visible: BTreeMap<u16, Value>,
    snapshot: BTreeMap<u16, Value>,
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
    pending: BTreeMap<u16, Value>,
    pad_preview: BTreeMap<u16, Value>,
    sent_at: u64,
}
struct Request {
    epoch: u32,
    pair: (String, String),
    sent: bool,
}
enum MountDecision {
    Accept(u32),
    Cancel(u32),
}
/// Disposable two-peer CRDT workload. It never shares a document, session or
/// persistence path with the admitted app's authoritative state.
struct StressWorkload {
    source: Document,
    sink: Document,
    source_session: Session,
    sink_session: Session,
    scope: String,
    field: u16,
    operations: u16,
}
impl StressWorkload {
    fn new(schema: StateSchema, scope: String, field: u16) -> Result<Self> {
        Ok(Self {
            source: Document::new(schema.clone(), &scope)?,
            sink: Document::new(schema, &scope)?,
            source_session: Session::default(),
            sink_session: Session::default(),
            scope,
            field,
            operations: 0,
        })
    }
    fn step(&mut self) -> Result<bool> {
        if self.operations == 256 {
            *self = Self::new(self.source.schema().clone(), self.scope.clone(), self.field)?;
        }
        let next = (self.source.value(self.field)? + 1) % 1001;
        let changed = self.source.set(&[(self.field, next)])?;
        for _ in 0..8 {
            let a = self.source.generate(&mut self.source_session)?;
            let b = self.sink.generate(&mut self.sink_session)?;
            if a.is_none() && b.is_none() {
                break;
            }
            if let Some(bytes) = a {
                self.sink.receive(&mut self.sink_session, &bytes)?;
            }
            if let Some(bytes) = b {
                self.source.receive(&mut self.source_session, &bytes)?;
            }
        }
        if self.sink.value(self.field)? != next {
            return Err("Diagnostic CRDT workload did not converge".into());
        }
        self.operations += u16::from(changed);
        Ok(changed)
    }
}
#[derive(Default)]
pub struct Client {
    generation: u32,
    requested: Option<Request>,
    decision: Option<MountDecision>,
    active: Option<Mount>,
    cache: VecDeque<(Vec<u8>, Vec<u8>)>,
    local: bool,
    navigation: Option<(String, String)>,
    catalog: Option<Catalog>,
    catalog_hash: Vec<u8>,
    font: Option<Arc<crate::dashboard::font::Font>>,
    auth_required: bool,
    session_controls: bool,
    sign_out_requested: bool,
}
impl Client {
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
        self.active
            .as_ref()
            .is_some_and(|m| m.input.active() || m.keyboard_pointer.is_some())
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
        self.local
            || self
                .active
                .as_mut()
                .is_none_or(|m| m.pending.is_empty() && m.doc.confirmed(&m.session))
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
    pub fn pending_count(&mut self) -> usize {
        if self.confirmed() {
            0
        } else {
            self.active.as_ref().map_or(0, |m| m.pending.len().max(1))
        }
    }
    pub fn open(&mut self, ui_ref: &str, state_ref: &str) {
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
            m.session = Session::default();
            self.generation = self.generation.saturating_add(1);
            self.requested = Some(Request {
                epoch: self.generation,
                pair: (m.ui.ui_ref.clone(), m.state_ref.clone()),
                sent: false,
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
        self.open("app:home/ui", "app:home/state");
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>> {
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
            })
            .map(Some);
        }
        if self.local {
            return Ok(None);
        }
        if let Some(m) = &mut self.active
            && let Some(bytes) = m.doc.generate(&mut m.session)?
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
        self.requested = Some(Request {
            epoch: self.generation,
            pair: (description.ui_ref, state_ref.into()),
            sent: true,
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
                    if ui.ui_ref != r.pair.0 {
                        return Err("Wrong UI reference".into());
                    }
                    let mut candidate = qualify(
                        ui,
                        &r.pair.1,
                        &code,
                        &self.cache,
                        &state,
                        epoch,
                        e.private || e.app.id == "monitoring",
                    )?;
                    if let Some(old) = &self.active
                        && old.state_ref == candidate.state_ref
                        && old.ui.state_schema == candidate.ui.state_schema
                    {
                        candidate.doc = old.doc.clone();
                        candidate.pending = old.pending.clone();
                        candidate.now = old.now;
                        candidate.wall = old.wall;
                        candidate.sent_at = old.sent_at;
                        if !state.is_empty() {
                            candidate.doc.merge_authority(&state)?;
                        }
                        candidate.refresh()?;
                    }
                    candidate.session_controls = self.session_controls;
                    candidate.invalidate();
                    cached_scene(&candidate)?;
                    Ok(candidate)
                })();
                match candidate {
                    Ok(candidate) => {
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
                if m.doc.receive_authority(&mut m.session, &bytes)? {
                    m.refresh()
                } else {
                    Ok(false)
                }
            }
            ServerMessage::Snapshot { epoch, values } => {
                let Some(m) = &mut self.active else {
                    return Ok(false);
                };
                if epoch != m.epoch {
                    return Ok(false);
                }
                m.doc.validate_snapshot(&values)?;
                let snapshot = values.into_iter().collect();
                if m.snapshot == snapshot {
                    return Ok(false);
                }
                m.snapshot = snapshot;
                m.refresh()
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
                        });
                        self.auth_required = true;
                    }
                    return Ok(true);
                }
                if self.requested.as_ref().is_some_and(|r| r.epoch == epoch) {
                    self.auth_required = true;
                }
                Ok(false)
            }
            ServerMessage::Error { epoch, message } => {
                if self.requested.as_ref().is_some_and(|r| r.epoch == epoch) {
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
        if self.requested.is_some() && !matches!(h.operation, Operation::Back) {
            return Ok(false);
        }
        match h.operation {
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
        if self.requested.is_some() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let current = h
            .fields
            .first()
            .map(|id| m.doc.value(*id))
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
            for (id, value) in commands {
                m.pending.insert(id, Value::Number(f32::from(value)));
            }
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
            if self.requested.is_some() {
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
        if pad && self.requested.is_some() {
            return Ok(false);
        }
        if pad && let Some(g) = &delivery.gesture {
            m.ink.gesture_normalized(event.id, g, m.now);
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
            m.flush()?;
        }
        Ok(changed)
    }
    pub fn activate(&mut self, epoch: u32, id: u32) -> Result<bool> {
        if epoch != self.epoch() || self.active.is_none() {
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
        if self.requested.is_some() || self.active.is_none() {
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
        self.open("app:home/ui", "app:home/state");
        Ok(true)
    }
    pub fn tick(&mut self, now: u64, wall: i64) -> Result<bool> {
        let Some(m) = &mut self.active else {
            return Ok(false);
        };
        m.now = now;
        m.doc.set_wall_time(wall);
        if now.saturating_sub(m.sent_at) >= 33 {
            m.flush()?;
        }
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
    pub fn adjust_control(&mut self, id: u32, dx: i16, dy: i16) -> Result<bool> {
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
        let a = shift(m.doc.value(h.fields[0])?, dx);
        let b = if h.fields.len() > 1 {
            shift(m.doc.value(h.fields[1])?, dy)
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
        Ok(Some(f64::from(m.doc.value(h.fields[0])?) / 10.))
    }
}
impl Mount {
    fn read(&self, id: u16) -> Result<Value> {
        self.pad_preview
            .get(&id)
            .or_else(|| self.pending.get(&id))
            .or_else(|| self.snapshot.get(&id))
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| self.doc.typed_value(id))
    }
    fn flush(&mut self) -> Result<bool> {
        if self.pending.is_empty() {
            return Ok(false);
        }
        let commands = self
            .pending
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect::<Vec<_>>();
        let changed = self.doc.set_typed(&commands)?;
        self.pending.clear();
        self.sent_at = self.now;
        Ok(changed)
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
fn qualify(
    ui: Ui,
    state_ref: &str,
    code: &[u8],
    cache: &VecDeque<(Vec<u8>, Vec<u8>)>,
    state: &[u8],
    epoch: u32,
    private: bool,
) -> Result<Mount> {
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
                Projection::KeyboardVisible | Projection::SessionControls => s.field != 0,
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
    let doc = if state.is_empty() {
        Document::new(ui.state_schema.clone(), state_ref)?
    } else {
        Document::load(ui.state_schema.clone(), state_ref, state)?
    };
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
        ui,
        layout,
        doc,
        session: Session::default(),
        behavior,
        input: InputRouter::default(),
        state_ref: state_ref.into(),
        private,
        session_controls: false,
        scene: OnceLock::new(),
        visible: BTreeMap::new(),
        snapshot: BTreeMap::new(),
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
        pending: BTreeMap::new(),
        pad_preview: BTreeMap::new(),
        sent_at: 0,
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
    Ok(values)
}
fn resolve_scene(m: &Mount) -> Result<Scene> {
    let mut bindings = BTreeMap::new();
    for s in &m.ui.selectors {
        let value = if let Projection::KeyboardLabel { key } = s.projection {
            Value::Text(m.keyboard.label(key))
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
                | Projection::SessionControls => unreachable!(),
            }
        };
        bindings.insert(s.local, value);
    }
    let scene = m.layout.resolve_slots(&bindings)?;
    let mut root = scene.root().clone();
    if let Some(Presentation::TouchPad { control }) = m.ui.presentation {
        fn append(n: &mut SceneNode, control: u8, ink: &crate::dashboard::ink::Ink) {
            if n.responder == Some(TouchResponder::Drag(Action::AppEvent(control))) {
                n.children
                    .extend(ink.nodes_normalized([n.frame.width, n.frame.height], 4_000_000_000));
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
    Scene::new(root)
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
