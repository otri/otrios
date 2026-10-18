// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::dashboard::{
    input::{Delivery, InputRouter},
    layout::Layout,
    scene::Scene,
};
use otrios_interfaces::{
    layout::Value,
    scene::{Action, PointerEvent, PointerPhase},
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, OnceLock},
};
struct Mount {
    ui: Ui,
    layout: Layout,
    doc: Document,
    session: Session,
    behavior: Option<Behavior>,
    input: InputRouter,
    state_ref: String,
    scene: OnceLock<Arc<Scene>>,
    visible: BTreeMap<u16, u16>,
}
#[derive(Default)]
pub struct Client {
    epoch: u32,
    requested: Option<(String, String)>,
    active: Option<Mount>,
    cache: VecDeque<(Vec<u8>, Vec<u8>)>,
    navigation: Option<(String, String)>,
}
impl Client {
    pub fn epoch(&self) -> u32 {
        self.epoch
    }
    pub fn title(&self) -> String {
        self.active
            .as_ref()
            .map(|a| a.ui.title.clone())
            .unwrap_or_else(|| "Opening app".into())
    }
    pub fn ui_ref(&self) -> String {
        self.active
            .as_ref()
            .map(|a| a.ui.ui_ref.clone())
            .unwrap_or_default()
    }
    pub fn state_ref(&self) -> String {
        self.active
            .as_ref()
            .map(|a| a.state_ref.clone())
            .unwrap_or_default()
    }
    pub fn value(&self, id: u16) -> Result<u16> {
        self.active.as_ref().ok_or("No Canvas")?.doc.value(id)
    }
    pub fn confirmed(&mut self) -> bool {
        self.active
            .as_mut()
            .is_none_or(|a| a.doc.confirmed(&a.session))
    }
    pub fn open(&mut self, ui_ref: &str, state_ref: &str) {
        self.navigation = Some((ui_ref.into(), state_ref.into()));
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        let idle = self.active.as_ref().is_none_or(|a| !a.input.active());
        if self.requested.is_none()
            && idle
            && self.confirmed()
            && let Some(pair) = self.navigation.take()
        {
            self.epoch = self.epoch.checked_add(1).ok_or("Canvas epoch exhausted")?;
            self.requested = Some(pair.clone());
            return encode_client(&ClientMessage::Mount {
                epoch: self.epoch,
                ui_ref: pair.0,
                state_ref: pair.1,
                known_code: self.cache.iter().map(|v| v.0.clone()).collect(),
            })
            .map(Some);
        }
        if self.requested.is_none()
            && let Some(a) = &mut self.active
            && let Some(bytes) = a.doc.generate(&mut a.session)?
        {
            return encode_client(&ClientMessage::Sync {
                epoch: self.epoch,
                bytes,
            })
            .map(Some);
        }
        Ok(None)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool> {
        match decode_server(bytes)? {
            ServerMessage::Mounted {
                epoch,
                ui,
                ui_hash,
                code,
            } => {
                if epoch != self.epoch {
                    return Ok(false);
                }
                let pair = self
                    .requested
                    .as_ref()
                    .ok_or("Unrequested Canvas activation")?;
                if hash(&ui) != ui_hash {
                    return Err("UI digest mismatch".into());
                }
                let ui = decode_ui(&ui)?;
                if ui.ui_ref != pair.0 {
                    return Err("Wrong UI reference".into());
                }
                let candidate = qualify(ui, &pair.1, &code, &self.cache)?;
                // Qualify a primitive scene before discarding the old input/subscription scope.
                cached_scene(&candidate)?;
                if let Some(reference) = &candidate.ui.behavior
                    && !code.is_empty()
                    && !self.cache.iter().any(|(id, _)| *id == reference.sha256)
                {
                    self.cache.push_back((reference.sha256.clone(), code));
                    while self.cache.len() > 8 {
                        self.cache.pop_front();
                    }
                }
                self.active = Some(candidate);
                self.requested = None;
                Ok(true)
            }
            ServerMessage::Sync { epoch, bytes } => {
                if epoch != self.epoch || self.requested.is_some() {
                    return Ok(false);
                }
                let a = self.active.as_mut().ok_or("No Canvas")?;
                if a.doc.receive(&mut a.session, &bytes)? {
                    a.refresh_inputs()
                } else {
                    Ok(false)
                }
            }
            ServerMessage::Error { epoch, message } => {
                if epoch != self.epoch {
                    return Ok(false);
                }
                self.requested = None;
                Err(message)
            }
        }
    }
    pub fn scene(&self) -> Result<Arc<Scene>> {
        cached_scene(self.active.as_ref().ok_or("No Canvas")?)
    }
    /// Returns whether projected presentation inputs changed. Nonvisual edits still sync.
    pub fn event(&mut self, epoch: u32, control: u8, a: u16, b: u16) -> Result<bool> {
        if epoch != self.epoch || self.requested.is_some() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let h =
            m.ui.handlers
                .iter()
                .find(|h| h.control == control)
                .ok_or("Unknown app event")?
                .clone();
        let current = h
            .fields
            .first()
            .map(|f| m.doc.value(*f))
            .transpose()?
            .unwrap_or(0);
        let commands = if let Some(guest) = &mut m.behavior {
            guest.call(h.event, a, b, current)?
        } else {
            h.fields
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, if i == 0 { a } else { b }))
                .collect()
        };
        if commands.iter().any(|(id, _)| !h.fields.contains(id)) {
            return Err("Behavior exceeded event field scope".into());
        }
        if m.doc.set(&commands)? {
            m.refresh_inputs()
        } else {
            Ok(false)
        }
    }
    /// Resolve a node's generated event index within its Canvas generation.
    pub fn event_for_node(&mut self, epoch: u32, node: u32, a: u16, b: u16) -> Result<bool> {
        if epoch != self.epoch || self.requested.is_some() {
            return Ok(false);
        }
        let scene = self.scene()?;
        let action = InputRouter::activate(scene.as_ref(), node).action;
        if let Some(Action::AppEvent(control)) = action {
            self.event(epoch, control, a, b)
        } else {
            Err("Node is not an app event responder".into())
        }
    }
    pub fn pointer(&mut self, epoch: u32, event: PointerEvent) -> Result<bool> {
        if epoch != self.epoch || self.requested.is_some() {
            return Ok(false);
        }
        let s = self.scene()?;
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let delivery = m.input.dispatch(s, event);
        self.deliver(delivery)
    }
    pub fn activate(&mut self, epoch: u32, id: u32) -> Result<bool> {
        if epoch != self.epoch || self.requested.is_some() {
            return Ok(false);
        }
        let delivery = InputRouter::activate(self.scene()?.as_ref(), id);
        self.deliver(delivery)
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
            return self.event(self.epoch, control, a, b);
        }
        if let Some(action) = d.action {
            match action {
                Action::AppNavigate(id) => {
                    if let Some(nav) = self
                        .active
                        .as_ref()
                        .and_then(|m| m.ui.navigation.iter().find(|n| n.action == id))
                        .cloned()
                    {
                        self.open(&nav.ui_ref, &nav.state_ref);
                        return Ok(true);
                    }
                }
                Action::AppEvent(id) => return self.event(self.epoch, id, 0, 0),
                _ => {}
            }
        }
        Ok(false)
    }
}
fn qualify(
    ui: Ui,
    state_ref: &str,
    code: &[u8],
    cache: &VecDeque<(Vec<u8>, Vec<u8>)>,
) -> Result<Mount> {
    if ui.format != "otrios-canvas-v1"
        || ui.title.len() > 96
        || ui.selectors.len() > 32
        || ui.handlers.len() > 32
        || ui.navigation.len() > 16
    {
        return Err("Canvas format/limits invalid".into());
    }
    validate_schema(&ui.state_schema)?;
    let fields = |id: &u16| ui.state_schema.fields.iter().any(|f| f.id == *id);
    let mut locals = std::collections::BTreeSet::new();
    let mut controls = std::collections::BTreeSet::new();
    for s in &ui.selectors {
        if !fields(&s.field) || !locals.insert(s.local) {
            return Err("Selector outside schema/duplicate local".into());
        }
    }
    for h in &ui.handlers {
        if h.fields.is_empty()
            || h.fields.len() > 5
            || !h.fields.iter().all(fields)
            || !controls.insert(h.control)
        {
            return Err("Invalid event fields".into());
        }
    }
    let behavior = if let Some(r) = &ui.behavior {
        if !r.writes.iter().all(fields) {
            return Err("Behavior writes outside schema".into());
        }
        let bytes = if code.is_empty() {
            cache
                .iter()
                .find(|(id, _)| *id == r.sha256)
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
    let doc = Document::new(ui.state_schema.clone(), state_ref)?;
    let mut mount = Mount {
        ui,
        layout,
        doc,
        behavior,
        session: Session::default(),
        input: InputRouter::default(),
        state_ref: state_ref.into(),
        scene: OnceLock::new(),
        visible: BTreeMap::new(),
    };
    mount.visible = selected_values(&mount.ui, &mount.doc)?;
    validate_responders(&mount.ui, mount.layout.blueprint())?;
    let original = mount.doc.clone();
    for bits in 0..(1 << mount.ui.state_schema.fields.len()) {
        let commands: Vec<_> = mount
            .ui
            .state_schema
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id, if bits & (1 << i) == 0 { 0 } else { 1000 }))
            .collect();
        mount.doc.set(&commands)?;
        resolve_scene(&mount)?;
    }
    mount.doc = original;
    Ok(mount)
}
fn resolve_scene(m: &Mount) -> Result<Scene> {
    let mut bindings = BTreeMap::new();
    for s in &m.ui.selectors {
        let n = m.doc.value(s.field)?;
        let v = match s.projection {
            Projection::Number { scale, offset } => Value::Number(f32::from(n) * scale + offset),
            Projection::Percent => Value::Text(format!("{}%", n / 10)),
            Projection::EnabledText => Value::Text(if n == 0 { "Off" } else { "On" }.into()),
        };
        bindings.insert(s.local, v);
    }
    m.layout.resolve_slots(&bindings)
}

fn selected_values(ui: &Ui, doc: &Document) -> Result<BTreeMap<u16, u16>> {
    let mut values = BTreeMap::new();
    for s in &ui.selectors {
        if let std::collections::btree_map::Entry::Vacant(entry) = values.entry(s.field) {
            entry.insert(doc.value(s.field)?);
        }
    }
    Ok(values)
}
impl Mount {
    fn refresh_inputs(&mut self) -> Result<bool> {
        let visible = selected_values(&self.ui, &self.doc)?;
        if visible == self.visible {
            return Ok(false);
        }
        self.visible = visible;
        self.scene.take();
        Ok(true)
    }
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
        for child in &n.children {
            node(child, ui)?;
        }
        Ok(())
    }
    for template in &blueprint.templates {
        node(&template.root, ui)?;
    }
    Ok(())
}
