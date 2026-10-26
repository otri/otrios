// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded FlexBuffers reader and typed component/binding interpreter. No JSON path.
use super::{Result, scene::Scene};
mod composition;
pub mod resizing;
use flexbuffers::{FlexBufferType, Reader};
use otrios_interfaces::{layout::*, scene::*};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub const MAX_LAYOUT_BYTES: usize = 128 * 1024;
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

    pub fn blueprint(&self) -> &Blueprint {
        &self.blueprint
    }
    /// App layouts can read only their explicitly projected slots. No dashboard scope.
    pub fn resolve_slots(&self, slots: &BTreeMap<u16, Value>) -> Result<Scene> {
        self.resolve_viewport(slots, [1280., 800.])
    }
    pub fn resolve_viewport(
        &self,
        slots: &BTreeMap<u16, Value>,
        viewport: [f32; 2],
    ) -> Result<Scene> {
        self.resolve_ordered(slots, viewport, &[])
    }
    pub fn resolve_ordered(
        &self,
        slots: &BTreeMap<u16, Value>,
        viewport: [f32; 2],
        order: &[u16],
    ) -> Result<Scene> {
        let bindings = Bindings { local: slots };
        let mut root = self.blueprint.root.clone();
        let frames = if let Some(plan) = &self.blueprint.composition {
            root.frame[2..].copy_from_slice(&viewport);
            let mut plan = plan.clone();
            fn reorder(flow: &mut Flow, order: &[u16]) {
                match &mut flow.kind {
                    FlowKind::Wrap { nodes, .. } => nodes
                        .sort_by_key(|n| order.iter().position(|id| id == n).unwrap_or(usize::MAX)),
                    FlowKind::Group { children, .. } => {
                        for c in children {
                            reorder(c, order);
                        }
                    }
                    _ => {}
                }
            }
            reorder(&mut plan.wide.content, order);
            reorder(&mut plan.tall.content, order);
            Some(composition::resolve(&plan, viewport)?)
        } else {
            None
        };
        Scene::new(expand(
            &self.blueprint,
            &root,
            &bindings,
            0,
            &mut 0,
            frames.as_ref(),
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
    if !matches!(
        b.schema.as_str(),
        "otrios-layout-v1" | "otrios-layout-v2" | "otrios-layout-v3" | "otrios-layout-v4"
    ) || b.templates.is_empty()
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
    if let Some(plan) = &b.composition {
        if !matches!(b.schema.as_str(), "otrios-layout-v3" | "otrios-layout-v4")
            || b.root.id != 0
            || !b.root.children.is_empty()
        {
            return Err("Composition requires a v3 app root".into());
        }
        let root = &b
            .templates
            .iter()
            .find(|t| t.id == b.root.template)
            .ok_or("Missing root template")?
            .root;
        fn wraps(flow: &Flow) -> bool {
            match &flow.kind {
                FlowKind::Wrap { .. } => true,
                FlowKind::Group { children, .. } => children.iter().any(wraps),
                _ => false,
            }
        }
        if b.schema != "otrios-layout-v4"
            && (wraps(&plan.wide.content) || wraps(&plan.tall.content))
        {
            return Err("Wrapping flow requires layout v4".into());
        }
        composition::validate(plan, root)?;
    }
    instance(&b.root, b, 0, &mut BTreeSet::new())
}
struct Bindings<'a> {
    local: &'a BTreeMap<u16, Value>,
}
impl Bindings<'_> {
    fn value(&self, Binding::Local(id): Binding) -> Result<Value> {
        self.local
            .get(&id)
            .cloned()
            .ok_or("Missing local binding".into())
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
    #[cfg(feature = "fixed-ui-math")]
    if matches!(
        e,
        Expr::Add(..) | Expr::Subtract(..) | Expr::Multiply(..) | Expr::Select { .. }
    ) && let Ok(Some(value)) = eval_fixed_number(e, frame, params, b)
    {
        return Ok(Value::Number(value.to_f32()));
    }
    eval_float32(e, frame, params, b)
}
#[cfg(any(test, feature = "fixed-ui-math"))]
fn eval_fixed_number(
    e: &Expr,
    frame: [f32; 4],
    params: &BTreeMap<u16, Value>,
    b: &Bindings<'_>,
) -> Result<Option<super::fixed::Scalar>> {
    use super::fixed::Scalar;
    let leaf = |v: &Value| -> Result<Option<Scalar>> {
        if let Value::Number(n) = v {
            Ok(Some(Scalar::from_f32(*n)?))
        } else {
            Ok(None)
        }
    };
    let run = |e: &Expr| eval_fixed_number(e, frame, params, b);
    Ok(match e {
        Expr::Literal(v) => leaf(v)?,
        Expr::Parameter(key) => leaf(params.get(key).ok_or("Unbound component parameter")?)?,
        Expr::State(Binding::Local(id)) => leaf(b.local.get(id).ok_or("Missing local binding")?)?,
        Expr::Width => Some(Scalar::from_f32(frame[2])?),
        Expr::Height => Some(Scalar::from_f32(frame[3])?),
        Expr::Add(a, c) | Expr::Subtract(a, c) | Expr::Multiply(a, c) => {
            let a = run(a)?.ok_or("Expected numeric expression")?;
            let c = run(c)?.ok_or("Expected numeric expression")?;
            Some(match e {
                Expr::Add(..) => a.checked_add(c)?,
                Expr::Subtract(..) => a.checked_sub(c)?,
                _ => a.checked_mul(c)?,
            })
        }
        Expr::Select { condition, yes, no } => {
            let condition = boolean(eval_float32(condition, frame, params, b)?)?;
            let yes = run(yes)?;
            let no = run(no)?;
            if yes.is_some() != no.is_some() {
                return Err("Conditional branch types differ".into());
            }
            if condition { yes } else { no }
        }
    })
}
fn eval_float32(
    e: &Expr,
    frame: [f32; 4],
    params: &BTreeMap<u16, Value>,
    b: &Bindings<'_>,
) -> Result<Value> {
    let run = |e: &Expr| eval_float32(e, frame, params, b);
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
    frames: Option<&BTreeMap<u16, [f32; 4]>>,
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
        frames: Option<&'a BTreeMap<u16, [f32; 4]>>,
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
        if is_root && c.b.schema != "otrios-layout-v1" {
            f = c.i.frame;
        } else if let Some(frame) = c.frames.and_then(|frames| frames.get(&n.id)) {
            f = *frame;
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
                children.push(expand(c.b, child, c.bindings, c.depth + 1, count, None)?);
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
                Responder::Bubble => TouchResponder::Bubble,
                Responder::Consume => TouchResponder::Consume,
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
        frames,
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
mod tests;
