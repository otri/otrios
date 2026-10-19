// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded FlexBuffers reader and typed component/binding interpreter. No JSON path.
use super::{Result, scene::Scene};
pub mod resizing;
use flexbuffers::{FlexBufferType, Reader};
use otrios_interfaces::{DashboardView, ViewContent, layout::*, scene::*};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
};
pub const MAX_LAYOUT_BYTES: usize = 128 * 1024;
pub const DEFAULT_LAYOUT: &[u8] = include_bytes!("default.flex");
const MAX_ITEMS: usize = 32768;
#[derive(Clone)]
pub struct Layout {
    bytes: Arc<[u8]>,
    blueprint: Arc<Blueprint>,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
/// Preflight before owned deserialization: bound recursion, visits and every allocation length.
fn preflight(r: Reader<&[u8]>, depth: usize, budget: &mut usize) -> Result<()> {
    if depth > 48 || *budget == 0 {
        return Err("Layout nesting/visit limit".into());
    }
    *budget -= 1;
    let t = r.flexbuffer_type();
    if t == FlexBufferType::Map {
        let m = r.get_map().map_err(err)?;
        if m.len() > 32 {
            return Err("Layout map too large".into());
        }
        let mut previous = None;
        for i in 0..m.len() {
            let key = m
                .keys_vector()
                .index(i)
                .map_err(err)?
                .get_key()
                .map_err(err)?;
            if key.is_empty() || key.len() > 64 || previous.is_some_and(|p: &str| p >= key) {
                return Err("Invalid/duplicate layout map key".into());
            }
            previous = Some(key);
            preflight(m.index(i).map_err(err)?, depth + 1, budget)?;
        }
    } else if t.is_vector() {
        let v = r.get_vector().map_err(err)?;
        if v.len() > 512 {
            return Err("Layout vector too large".into());
        }
        for i in 0..v.len() {
            preflight(v.index(i).map_err(err)?, depth + 1, budget)?;
        }
    } else {
        match t {
            FlexBufferType::String => {
                if r.length() > 128 || r.get_str().map_err(err)?.len() > 128 {
                    return Err("Layout string too large".into());
                }
            }
            FlexBufferType::Float => {
                if !r.get_f64().map_err(err)?.is_finite() {
                    return Err("Nonfinite layout value".into());
                }
            }
            FlexBufferType::UInt => {
                r.get_u64().map_err(err)?;
            }
            FlexBufferType::Int => {
                r.get_i64().map_err(err)?;
            }
            FlexBufferType::Bool => {
                r.get_bool().map_err(err)?;
            }
            FlexBufferType::Null => {}
            _ => return Err("Unsupported layout binary value".into()),
        }
    }
    Ok(())
}
impl Layout {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_LAYOUT_BYTES {
            return Err("Invalid layout size".into());
        }
        crate::flex_guard::validate(bytes, 48, MAX_ITEMS, 512)?;
        let mut budget = MAX_ITEMS;
        preflight(Reader::get_root(bytes).map_err(err)?, 0, &mut budget)?;
        let blueprint: Blueprint = flexbuffers::from_slice(bytes).map_err(err)?;
        validate_blueprint(&blueprint)?;
        Ok(Self {
            bytes: bytes.into(),
            blueprint: Arc::new(blueprint),
        })
    }
    pub fn encode(blueprint: &Blueprint) -> Result<Vec<u8>> {
        validate_blueprint(blueprint)?;
        let bytes = flexbuffers::to_vec(blueprint).map_err(err)?;
        Self::from_bytes(&bytes)?;
        Ok(bytes)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn hex_id(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"OTriOS\0layout\0v1\0");
        hash.update(&self.bytes);
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn bootstrap() -> Result<Self> {
        static VALUE: OnceLock<Result<Layout>> = OnceLock::new();
        VALUE
            .get_or_init(|| Self::from_bytes(DEFAULT_LAYOUT))
            .clone()
    }
    pub fn blueprint(&self) -> &Blueprint {
        &self.blueprint
    }
    pub fn resolve(&self, view: &DashboardView) -> Result<Scene> {
        self.resolve_enabled(view, None)
    }
    /// Typed local UI state belongs to a host session, outside the collaborative document.
    pub fn resolve_local(
        &self,
        view: &DashboardView,
        local: &BTreeMap<u16, Value>,
    ) -> Result<Scene> {
        let bindings = Bindings {
            view: Some(view),
            enabled: None,
            local: Some(local),
        };
        Scene::new(expand(
            &self.blueprint,
            &self.blueprint.root,
            &bindings,
            0,
            &mut 0,
        )?)
    }
    /// App layouts can read only their explicitly projected slots. No dashboard scope.
    pub fn resolve_slots(&self, slots: &BTreeMap<u16, Value>) -> Result<Scene> {
        let bindings = Bindings {
            view: None,
            enabled: None,
            local: Some(slots),
        };
        Scene::new(expand(
            &self.blueprint,
            &self.blueprint.root,
            &bindings,
            0,
            &mut 0,
        )?)
    }
    /// Validate both switch states so a layout cannot disable the next normal toggle.
    pub fn qualify(&self, view: &DashboardView) -> Result<()> {
        for enabled in [false, true] {
            self.resolve_enabled(view, Some(enabled))?;
            // Normal actions append 24 or 78; validate those binding values too.
            for sample in [24, 78] {
                let mut next = view.clone();
                for node in &mut next.nodes {
                    if let ViewContent::Graph { samples, .. } = &mut node.content {
                        samples.push(sample);
                        if samples.len() > 32 {
                            samples.remove(0);
                        }
                    }
                }
                self.resolve_enabled(&next, Some(enabled))?;
            }
        }
        Ok(())
    }
    fn resolve_enabled(&self, view: &DashboardView, enabled: Option<bool>) -> Result<Scene> {
        let bindings = Bindings {
            view: Some(view),
            enabled,
            local: None,
        };
        let mut count = 0;
        Scene::new(expand(
            &self.blueprint,
            &self.blueprint.root,
            &bindings,
            0,
            &mut count,
        )?)
    }
}
fn bounded_expr(e: &Expr, depth: usize, parameters: bool) -> Result<()> {
    if depth > 12 {
        return Err("Expression nesting limit".into());
    }
    match e {
        Expr::Literal(v) => match v {
            Value::Number(n) if !n.is_finite() || n.abs() > 8192. => {
                return Err("Invalid numeric literal".into());
            }
            Value::Text(s) if s.len() > 96 => return Err("Text literal too long".into()),
            Value::Series(v)
                if v.len() > 256
                    || v.iter()
                        .any(|n| !n.is_finite() || !(0. ..=100.).contains(n)) =>
            {
                return Err("Invalid series literal".into());
            }
            Value::TimedSeries(v) if !valid_timed_series(v) => {
                return Err("Invalid timed series literal".into());
            }
            _ => {}
        },
        Expr::Parameter(_) if !parameters => {
            return Err("Parameter defaults/overrides cannot reference parameters".into());
        }
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            bounded_expr(a, depth + 1, parameters)?;
            bounded_expr(b, depth + 1, parameters)?;
        }
        Expr::Select { condition, yes, no } => {
            bounded_expr(condition, depth + 1, parameters)?;
            bounded_expr(yes, depth + 1, parameters)?;
            bounded_expr(no, depth + 1, parameters)?;
        }
        _ => {}
    }
    Ok(())
}
fn validate_blueprint(b: &Blueprint) -> Result<()> {
    if !matches!(b.schema.as_str(), "otrios-layout-v1" | "otrios-layout-v2")
        || b.templates.is_empty()
        || b.templates.len() > 32
    {
        return Err("Unknown/invalid layout schema/templates".into());
    }
    let mut templates = BTreeSet::new();
    let mut total = 0;
    for t in &b.templates {
        if !templates.insert(t.id) || t.name.len() > 96 || t.parameters.len() > 32 {
            return Err("Invalid template identity/parameters".into());
        }
        let mut parameters = BTreeSet::new();
        for p in &t.parameters {
            if !parameters.insert(p.key) || p.name.len() > 64 {
                return Err("Duplicate/invalid parameter".into());
            }
            bounded_expr(&p.default, 0, false)?;
        }
        fn node(
            n: &TemplateNode,
            depth: usize,
            ids: &mut BTreeSet<u16>,
            slots: &mut usize,
            total: &mut usize,
        ) -> Result<()> {
            *total += 1;
            if depth > 12 || n.id >= 1000 || !ids.insert(n.id) || *total > 512 {
                return Err("Template node limits/identity".into());
            }
            if let Some(rule) = n.resize {
                resizing::validate(rule)?;
            }
            for e in &n.frame {
                bounded_expr(e, 0, true)?;
            }
            bounded_expr(&n.visible, 0, true)?;
            match &n.shape {
                Shape::Text {
                    value,
                    color,
                    scale,
                } => {
                    bounded_expr(value, 0, true)?;
                    bounded_expr(color, 0, true)?;
                    bounded_expr(scale, 0, true)?;
                }
                Shape::Superellipse {
                    radius,
                    exponent,
                    color,
                } => {
                    bounded_expr(radius, 0, true)?;
                    bounded_expr(color, 0, true)?;
                    if !exponent.is_finite() || !(2. ..=8.).contains(exponent) {
                        return Err("Invalid superellipse exponent".into());
                    }
                }
                Shape::Circle { color } => bounded_expr(color, 0, true)?,
                Shape::SeriesPath {
                    series,
                    width,
                    color,
                }
                | Shape::TimedSeriesPath {
                    series,
                    width,
                    color,
                } => {
                    bounded_expr(series, 0, true)?;
                    bounded_expr(color, 0, true)?;
                    if !width.is_finite() || !(0.1..=16.).contains(width) {
                        return Err("Invalid path width".into());
                    }
                }
                Shape::Slot => {
                    *slots += 1;
                    if !n.children.is_empty() {
                        return Err("A child slot cannot define template children".into());
                    }
                }
                _ => {}
            }
            if !matches!(n.shape, Shape::Canvas { .. } | Shape::Slot) && !n.children.is_empty() {
                return Err("Only canvas templates contain children".into());
            }
            if let Some(s) = &n.semantics {
                bounded_expr(&s.label, 0, true)?;
                bounded_expr(&s.checked, 0, true)?;
            }
            for c in &n.children {
                node(c, depth + 1, ids, slots, total)?;
            }
            Ok(())
        }
        let mut slots = 0;
        node(&t.root, 0, &mut BTreeSet::new(), &mut slots, &mut total)?;
        fn has_resize(n: &TemplateNode) -> bool {
            n.resize.is_some() || n.children.iter().any(has_resize)
        }
        if t.root.resize.is_some() || (b.schema == "otrios-layout-v1" && has_resize(&t.root)) {
            return Err("Resize rules require layout v2 and an instance-owned root".into());
        }
        if t.root.id != 0 || slots > 1 {
            return Err("Template root must be local ID zero; at most one child slot".into());
        }
    }
    fn instance(i: &Instance, b: &Blueprint, depth: usize, ids: &mut BTreeSet<u32>) -> Result<()> {
        if depth > 8
            || ids.len() >= 128
            || !ids.insert(i.id)
            || i.id > 4_000_000
            || i.properties.len() > 32
            || i.frame.iter().any(|n| !n.is_finite() || n.abs() > 8192.)
            || i.frame[2] <= 0.
            || i.frame[3] <= 0.
        {
            return Err("Invalid instance identity/frame/limits".into());
        }
        let t = b
            .templates
            .iter()
            .find(|t| t.id == i.template)
            .ok_or("Unknown component template")?;
        let mut props = BTreeSet::new();
        for p in &i.properties {
            if !props.insert(p.key) || !t.parameters.iter().any(|v| v.key == p.key) {
                return Err("Unknown/duplicate instance property".into());
            }
            bounded_expr(&p.value, 0, false)?;
        }
        for c in &i.children {
            instance(c, b, depth + 1, ids)?;
        }
        Ok(())
    }
    if b.root.id != 0 || b.root.frame != [0., 0., 1280., 800.] {
        return Err("Root layout instance must be the screen".into());
    }
    instance(&b.root, b, 0, &mut BTreeSet::new())
}
struct Bindings<'a> {
    view: Option<&'a DashboardView>,
    enabled: Option<bool>,
    local: Option<&'a BTreeMap<u16, Value>>,
}
impl Bindings<'_> {
    fn dashboard(&self) -> Result<&DashboardView> {
        self.view
            .ok_or("Dashboard bindings are unavailable in app scope".into())
    }
    fn value(&self, b: Binding) -> Result<Value> {
        match b {
            Binding::Local(id) => self
                .local
                .and_then(|v| v.get(&id))
                .cloned()
                .ok_or("Missing local binding".into()),
            Binding::Enabled => {
                let view = self.dashboard()?;
                Ok(Value::Boolean(self.enabled.unwrap_or_else(|| {
                    view.nodes
                        .iter()
                        .find_map(|n| {
                            if let ViewContent::Switch { enabled, .. } = n.content {
                                Some(enabled)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(false)
                })))
            }
            Binding::Title | Binding::TouchLabel => {
                let id = if b == Binding::Title { 1 } else { 4 };
                self.dashboard()?
                    .nodes
                    .iter()
                    .find_map(|n| {
                        if n.id == id {
                            if let ViewContent::Label(s) = &n.content {
                                Some(Value::Text(s.clone()))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .ok_or("Missing text binding".into())
            }
            Binding::Samples | Binding::LastSample | Binding::HasSamples => {
                let values = self
                    .dashboard()?
                    .nodes
                    .iter()
                    .find_map(|n| {
                        if let ViewContent::Graph { samples, .. } = &n.content {
                            Some(samples)
                        } else {
                            None
                        }
                    })
                    .ok_or("Missing series binding")?;
                Ok(match b {
                    Binding::Samples => Value::Series(values.iter().map(|n| *n as f32).collect()),
                    Binding::LastSample => Value::Number(*values.last().unwrap_or(&0) as f32),
                    _ => Value::Boolean(!values.is_empty()),
                })
            }
        }
    }
}

fn number(v: Value) -> Result<f32> {
    if let Value::Number(n) = v
        && n.is_finite()
    {
        Ok(n)
    } else {
        Err("Expected finite numeric expression".into())
    }
}
fn boolean(v: Value) -> Result<bool> {
    if let Value::Boolean(v) = v {
        Ok(v)
    } else {
        Err("Expected boolean expression".into())
    }
}
fn text(v: Value) -> Result<String> {
    if let Value::Text(v) = v {
        Ok(v)
    } else {
        Err("Expected text expression".into())
    }
}
fn color(v: Value) -> Result<Color> {
    if let Value::Color(v) = v {
        Ok(v)
    } else {
        Err("Expected colour expression".into())
    }
}
fn valid_timed_series(points: &[[f32; 2]]) -> bool {
    points.len() <= 256
        && points.iter().all(|p| {
            p[0].is_finite()
                && (0. ..=1.).contains(&p[0])
                && p[1].is_finite()
                && (0. ..=100.).contains(&p[1])
        })
        && points.windows(2).all(|p| p[0][0] <= p[1][0])
}
fn value_type(v: &Value) -> ValueType {
    match v {
        Value::Number(_) => ValueType::Number,
        Value::Boolean(_) => ValueType::Boolean,
        Value::Text(_) => ValueType::Text,
        Value::Color(_) => ValueType::Color,
        Value::Series(_) => ValueType::Series,
        Value::TimedSeries(_) => ValueType::TimedSeries,
    }
}
fn eval(
    e: &Expr,
    frame: [f32; 4],
    params: &BTreeMap<u16, Value>,
    b: &Bindings<'_>,
) -> Result<Value> {
    let run = |e: &Expr| eval(e, frame, params, b);
    Ok(match e {
        Expr::Literal(v) => v.clone(),
        Expr::Parameter(key) => params
            .get(key)
            .cloned()
            .ok_or("Unbound component parameter")?,
        Expr::State(id) => b.value(*id)?,
        Expr::Width => Value::Number(frame[2]),
        Expr::Height => Value::Number(frame[3]),
        Expr::Add(a, c) => Value::Number(number(run(a)?)? + number(run(c)?)?),
        Expr::Subtract(a, c) => Value::Number(number(run(a)?)? - number(run(c)?)?),
        Expr::Multiply(a, c) => Value::Number(number(run(a)?)? * number(run(c)?)?),
        Expr::Select { condition, yes, no } => {
            let condition = boolean(run(condition)?)?;
            let yes = run(yes)?;
            let no = run(no)?;
            if value_type(&yes) != value_type(&no) {
                return Err("Conditional branch types differ".into());
            }
            if condition { yes } else { no }
        }
    })
}
fn expand(
    b: &Blueprint,
    i: &Instance,
    bindings: &Bindings<'_>,
    depth: usize,
    count: &mut usize,
) -> Result<SceneNode> {
    if depth > 8 {
        return Err("Instance expansion limit".into());
    }
    let t = b
        .templates
        .iter()
        .find(|t| t.id == i.template)
        .ok_or("Unknown component")?;
    let mut params = BTreeMap::new();
    for p in &t.parameters {
        let e = i
            .properties
            .iter()
            .find(|v| v.key == p.key)
            .map(|v| &v.value)
            .unwrap_or(&p.default);
        let value = eval(e, i.frame, &BTreeMap::new(), bindings)?;
        if value_type(&value) != p.value_type {
            return Err(format!("Wrong type for {}", p.name));
        }
        params.insert(p.key, value);
    }
    struct Context<'a, 'v> {
        b: &'a Blueprint,
        i: &'a Instance,
        bindings: &'a Bindings<'v>,
        params: &'a BTreeMap<u16, Value>,
        depth: usize,
    }
    fn walk(
        n: &TemplateNode,
        c: &Context<'_, '_>,
        parent_size: [f32; 2],
        is_root: bool,
        count: &mut usize,
        slots: &mut usize,
    ) -> Result<Option<SceneNode>> {
        let run = |e: &Expr| eval(e, c.i.frame, c.params, c.bindings);
        if !boolean(run(&n.visible)?)? {
            return Ok(None);
        }
        *count += 1;
        if *count > 512 {
            return Err("Expanded scene too large".into());
        }
        let mut f = [0.; 4];
        for (v, e) in f.iter_mut().zip(&n.frame) {
            *v = number(run(e)?)?;
        }
        if is_root && c.b.schema == "otrios-layout-v2" {
            f = c.i.frame;
        } else if let Some(rule) = n.resize {
            f = resizing::resolve(f, parent_size, rule)?;
        }
        let r = Rect::new(f[0], f[1], f[2], f[3]);
        let primitive = match &n.shape {
            Shape::Canvas { clip } => Primitive::Canvas { clip: *clip },
            Shape::Slot => Primitive::Canvas { clip: true },
            Shape::Text {
                value,
                scale,
                color: paint,
            } => Primitive::Text {
                value: text(run(value)?)?,
                scale: {
                    let s = number(run(scale)?)?;
                    if !(1. ..=8.).contains(&s) || s.fract() != 0. {
                        return Err("Invalid text scale".into());
                    }
                    s as u32
                },
                color: color(run(paint)?)?,
            },
            Shape::Superellipse {
                radius,
                exponent,
                color: paint,
            } => Primitive::Superellipse {
                radius: number(run(radius)?)?,
                exponent: *exponent,
                color: color(run(paint)?)?,
            },
            Shape::Circle { color: paint } => Primitive::Circle {
                color: color(run(paint)?)?,
            },
            Shape::TimedSeriesPath {
                series,
                width,
                color: paint,
            } => {
                let Value::TimedSeries(values) = run(series)? else {
                    return Err("Expected timed series binding".into());
                };
                if !valid_timed_series(&values) {
                    return Err("Invalid timed series binding".into());
                }
                let mut points: Vec<_> = values
                    .iter()
                    .map(|p| [p[0] * r.width, r.height - p[1] * r.height / 100.])
                    .collect();
                // A single transition is still visible without inventing older history.
                if points.len() == 1 {
                    let p = points[0];
                    points.insert(0, [(p[0] - *width).max(0.), p[1]]);
                }
                Primitive::Path {
                    points,
                    width: *width,
                    color: color(run(paint)?)?,
                }
            }
            Shape::SeriesPath {
                series,
                width,
                color: paint,
            } => {
                let Value::Series(values) = run(series)? else {
                    return Err("Expected series binding".into());
                };
                let points = values
                    .iter()
                    .enumerate()
                    .map(|(index, v)| {
                        [
                            index as f32 * r.width / (values.len().max(2) - 1) as f32,
                            r.height - *v * r.height / 100.,
                        ]
                    })
                    .collect();
                Primitive::Path {
                    points,
                    width: *width,
                    color: color(run(paint)?)?,
                }
            }
        };
        let mut children = Vec::new();
        if matches!(n.shape, Shape::Slot) {
            *slots += 1;
            for child in &c.i.children {
                children.push(expand(c.b, child, c.bindings, c.depth + 1, count)?);
            }
        } else {
            for child in &n.children {
                if let Some(node) = walk(child, c, [r.width, r.height], false, count, slots)? {
                    children.push(node);
                }
            }
        }
        let responder = if let Some(r) = n.responder {
            Some(match r {
                Responder::AppEvent { control, drag } => {
                    if control >= 128 {
                        return Err("Canvas event index exceeds limit".into());
                    }
                    if drag {
                        TouchResponder::Drag(Action::AppEvent(control))
                    } else {
                        TouchResponder::Activate(Action::AppEvent(control))
                    }
                }
                Responder::AppNavigate(id) => TouchResponder::Activate(Action::AppNavigate(id)),
                Responder::DragControl(id) => {
                    if id > 2 {
                        return Err("Unknown drag control".into());
                    }
                    TouchResponder::Drag(Action::Control(id))
                }
                Responder::Bubble => TouchResponder::Bubble,
                Responder::Consume => TouchResponder::Consume,
                Responder::Motion(id) => TouchResponder::Activate(Action::Motion(id)),
                Responder::ResetControls => TouchResponder::Activate(Action::ResetControls),
                Responder::Stress => TouchResponder::Activate(Action::Stress),
                Responder::ToggleFocus => TouchResponder::Activate(Action::ToggleFocus),
                Responder::Open(screen) => TouchResponder::Activate(Action::Open(screen)),
                Responder::Back => TouchResponder::Activate(Action::Back),
                Responder::Home => TouchResponder::Activate(Action::Home),
                Responder::FocusField(id) => TouchResponder::Activate(Action::FocusField(id)),
                Responder::KeyboardKey(id) => TouchResponder::Activate(Action::KeyboardKey(id)),
                Responder::KeyParameter(key) => {
                    let id = number(c.params.get(&key).cloned().ok_or("Missing key parameter")?)?;
                    if id.fract() != 0. || !(0. ..=65535.).contains(&id) {
                        return Err("Invalid key parameter".into());
                    }
                    TouchResponder::Activate(Action::KeyboardKey(id as u16))
                }
            })
        } else {
            None
        };
        let semantics = if let Some(s) = &n.semantics {
            Some(SwitchSemantics {
                label: text(run(&s.label)?)?,
                checked: boolean(run(&s.checked)?)?,
            })
        } else {
            None
        };
        Ok(Some(SceneNode {
            id: c.i.id * 1000 + u32::from(n.id),
            frame: r,
            primitive,
            children,
            responder,
            semantics,
        }))
    }
    let c = Context {
        b,
        i,
        bindings,
        params: &params,
        depth,
    };
    let mut slots = 0;
    let mut root = walk(
        &t.root,
        &c,
        [i.frame[2], i.frame[3]],
        true,
        count,
        &mut slots,
    )?
    .ok_or("Component root cannot be hidden")?;
    if slots == 0 && !i.children.is_empty() {
        return Err("Component has children but no visible slot".into());
    }
    root.frame = Rect::new(i.frame[0], i.frame[1], i.frame[2], i.frame[3]);
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{Replica, SyncSession, ViewSnapshot};
    fn blueprint() -> Blueprint {
        Layout::bootstrap().unwrap().blueprint().clone()
    }
    fn changed() -> Vec<u8> {
        let mut b = blueprint();
        let switch = b.root.children.iter_mut().find(|i| i.id == 2).unwrap();
        switch.frame = [64., 64., 440., 240.];
        switch.properties.push(Property {
            key: 1,
            value: Expr::Literal(Value::Text("Quiet mode".into())),
        });
        Layout::encode(&b).unwrap()
    }
    #[test]
    fn replaceable_feed_views_pin_old_roots_without_growing_crdt_history() {
        let mut replica = Replica::new().unwrap();
        let before = replica.save();
        let old = replica.snapshot().clone();
        let mut b = blueprint();
        for index in 0..100 {
            b.root.children[0].frame[0] = 64. + index as f32;
            let bytes = Layout::encode(&b).unwrap();
            replica.set_presentation(Some(&bytes)).unwrap();
        }
        assert_eq!(replica.save(), before);
        assert_eq!(old.layout().bytes(), DEFAULT_LAYOUT);
        let latest = replica.snapshot().id();
        assert!(replica.set_presentation(Some(b"broken")).is_err());
        assert_eq!(replica.snapshot().id(), latest);
        // A feed cannot hide an invalid base layout received through ordinary sync.
        use automerge::transaction::Transactable;
        let mut invalid = automerge::AutoCommit::load(&replica.save()).unwrap();
        invalid
            .put(
                automerge::ROOT,
                "layout",
                automerge::ScalarValue::Bytes(b"bad".to_vec()),
            )
            .unwrap();
        assert!(replica.merge(&invalid.save()).is_err());
        assert_eq!(replica.snapshot().id(), latest);
        let mut peer = Replica::new().unwrap();
        peer.set_enabled(false).unwrap();
        replica.merge(&peer.save()).unwrap();
        assert!(replica.presentation().is_some());
        replica.set_presentation(None).unwrap();
        assert_eq!(replica.snapshot().layout().bytes(), DEFAULT_LAYOUT);
        assert!(
            replica
                .snapshot()
                .view()
                .nodes
                .iter()
                .any(|n| matches!(n.content, ViewContent::Switch { enabled: false, .. }))
        );
    }
    #[test]
    fn actual_binary_uses_typed_vectors_and_round_trips_without_json() {
        let r = Reader::get_root(DEFAULT_LAYOUT).unwrap().get_map().unwrap();
        let frame = r
            .index("root")
            .unwrap()
            .get_map()
            .unwrap()
            .index("frame")
            .unwrap();
        assert_eq!(frame.flexbuffer_type(), FlexBufferType::VectorFloat4);
        let b = blueprint();
        assert_eq!(Layout::encode(&b).unwrap(), DEFAULT_LAYOUT);
    }
    #[test]
    fn binary_layout_moves_and_renames_control_while_state_and_pinned_scene_survive() {
        let mut r = Replica::new().unwrap();
        let old = r.snapshot().clone();
        let bytes = changed();
        assert!(r.set_layout(&bytes).unwrap());
        assert_ne!(old.id(), r.snapshot().id());
        assert_eq!(old.scene().controls()[0].origin, [64., 208.]);
        let control = r.snapshot().scene().controls()[0];
        assert_eq!(control.origin, [64., 64.]);
        assert_eq!(control.node.semantics.as_ref().unwrap().label, "Quiet mode");
        assert!(!r.touch(200, 400).unwrap());
        assert!(r.touch(200, 150).unwrap());
        assert!(
            !r.snapshot().scene().controls()[0]
                .node
                .semantics
                .as_ref()
                .unwrap()
                .checked
        );
        assert_eq!(r.snapshot().layout().bytes(), bytes);
        let copy = ViewSnapshot::from_bytes(r.snapshot().bytes()).unwrap();
        assert_eq!(copy.scene().root(), r.snapshot().scene().root());
        let persisted = Replica::load(&r.save()).unwrap();
        assert_eq!(persisted.snapshot().id(), r.snapshot().id());
    }
    #[test]
    fn changing_template_definition_changes_primitive_geometry() {
        let mut b = blueprint();
        let panel = b.templates.iter_mut().find(|t| t.id == 3).unwrap();
        if let Shape::Superellipse { radius, color, .. } = &mut panel.root.children[0].shape {
            *radius = Expr::Literal(Value::Number(24.));
            *color = Expr::Literal(Value::Color([60, 80, 120, 255]));
        }
        let mut r = Replica::new().unwrap();
        r.set_layout(&Layout::encode(&b).unwrap()).unwrap();
        assert!(matches!(
            r.snapshot()
                .scene()
                .path(4001)
                .unwrap()
                .last()
                .unwrap()
                .node
                .primitive,
            Primitive::Superellipse {
                radius: 24.,
                color: [60, 80, 120, 255],
                ..
            }
        ));
    }
    #[test]
    fn slots_accept_nested_instances_and_id_conflicts_are_rejected() {
        let mut b = blueprint();
        b.root
            .children
            .iter_mut()
            .find(|i| i.id == 4)
            .unwrap()
            .children
            .push(Instance {
                id: 42,
                template: 2,
                frame: [0., 0., 100., 20.],
                properties: vec![Property {
                    key: 1,
                    value: Expr::Literal(Value::Text("Nested".into())),
                }],
                children: vec![],
            });
        let mut r = Replica::new().unwrap();
        r.set_layout(&Layout::encode(&b).unwrap()).unwrap();
        assert!(r.snapshot().scene().path(42000).is_some());
        b.root.children[0].id = 42;
        assert!(Layout::encode(&b).is_err());
    }
    #[test]
    fn malformed_layout_and_wrong_property_type_retain_old_root() {
        let mut r = Replica::new().unwrap();
        let old = r.snapshot().id();
        assert!(r.set_layout(b"bad").is_err());
        let mut b = blueprint();
        b.root
            .children
            .iter_mut()
            .find(|i| i.id == 2)
            .unwrap()
            .properties
            .push(Property {
                key: 2,
                value: Expr::Literal(Value::Text("wrong colour type".into())),
            });
        assert!(r.set_layout(&Layout::encode(&b).unwrap()).is_err());
        assert_eq!(r.snapshot().id(), old);
        assert_eq!(r.snapshot().layout().bytes(), DEFAULT_LAYOUT);
    }
    #[test]
    fn conditional_geometry_is_qualified_in_both_checked_states() {
        let mut b = blueprint();
        if let Shape::Superellipse { radius, .. } = &mut b
            .templates
            .iter_mut()
            .find(|t| t.id == 4)
            .unwrap()
            .root
            .children[0]
            .shape
        {
            *radius = Expr::Select {
                condition: Box::new(Expr::State(Binding::Enabled)),
                yes: Box::new(Expr::Literal(Value::Number(16.))),
                no: Box::new(Expr::Literal(Value::Number(1000.))),
            };
        }
        let mut r = Replica::new().unwrap();
        assert!(r.set_layout(&Layout::encode(&b).unwrap()).is_err());
    }
    #[test]
    fn concurrent_binary_layout_revisions_and_app_edits_converge() {
        let mut a = Replica::new().unwrap();
        let mut b = Replica::new().unwrap();
        a.set_layout(&changed()).unwrap();
        let mut other = blueprint();
        other
            .root
            .children
            .iter_mut()
            .find(|i| i.id == 2)
            .unwrap()
            .frame[1] = 128.;
        b.set_layout(&Layout::encode(&other).unwrap()).unwrap();
        b.set_enabled(false).unwrap();
        let mut sa = SyncSession::default();
        let mut sb = SyncSession::default();
        for _ in 0..50 {
            let x = a.generate_sync(&mut sa).unwrap();
            let y = b.generate_sync(&mut sb).unwrap();
            if x.is_none() && y.is_none() {
                break;
            }
            if let Some(x) = x {
                b.receive_sync(&mut sb, &x).unwrap();
            }
            if let Some(y) = y {
                a.receive_sync(&mut sa, &y).unwrap();
            }
        }
        assert_eq!(a.snapshot().id(), b.snapshot().id());
        assert_eq!(a.snapshot().layout().bytes(), b.snapshot().layout().bytes());
        assert!(
            !a.snapshot().scene().controls()[0]
                .node
                .semantics
                .as_ref()
                .unwrap()
                .checked
        );
    }
    #[test]
    fn truncated_mutated_and_excessively_nested_buffers_fail_safely() {
        for n in 0..DEFAULT_LAYOUT.len() {
            assert!(Layout::from_bytes(&DEFAULT_LAYOUT[..n]).is_err());
        }
        for i in (0..DEFAULT_LAYOUT.len()).step_by(11) {
            let mut bytes = DEFAULT_LAYOUT.to_vec();
            bytes[i] ^= 0xff;
            let _ = Layout::from_bytes(&bytes);
        }
        let mut b = blueprint();
        b.schema = "unknown".into();
        assert!(Layout::encode(&b).is_err());
        let mut e = Expr::Literal(Value::Number(1.));
        for _ in 0..14 {
            e = Expr::Add(Box::new(e), Box::new(Expr::Literal(Value::Number(1.))));
        }
        let mut b = blueprint();
        b.root.properties.push(Property { key: 2, value: e });
        assert!(Layout::encode(&b).is_err());
        assert!(Layout::from_bytes(&vec![0; MAX_LAYOUT_BYTES + 1]).is_err());
    }
}
