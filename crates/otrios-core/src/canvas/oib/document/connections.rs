// SPDX-License-Identifier: LGPL-3.0-only
//! Explicit connections to existing signed selectors and fixed control effects.
use super::*;
use otrios_interfaces::{
    canvas::{GestureKind, Operation, Projection},
    layout::Responder,
    oib::{
        authoring::{AppConnection, ConnectionSource},
        graph::ActionOperation,
    },
};

fn endpoint(
    doc: &Document,
    object: &ObjectAddress,
    id: PropertyId,
) -> Result<(PropertyPath, ClassKind)> {
    let root = instance(doc, object.instance)?;
    let path = PropertyPath {
        references: object.references.clone(),
        node: object.node,
        property: id,
    };
    let descriptor = property(doc, root.definition, &path)?;
    let (_, node) = path_node(doc, root.definition, &object.references, object.node)?;
    let kind = class(doc, node.class)?.implementation;
    if !descriptor.policy.app_binding
        || !matches!(
            (kind, descriptor.value_type),
            (ClassKind::O2DSlider, ValueType::Number) | (ClassKind::O2DToggle, ValueType::Boolean)
        )
    {
        return Err("Only admitted slider-value and toggle-checked endpoints can reconnect".into());
    }
    Ok((path, kind))
}

fn selector(ui: &Ui, kind: ClassKind, source: &ConnectionSource) -> Result<Binding> {
    let expected = if kind == ClassKind::O2DSlider {
        Projection::Identity
    } else {
        Projection::Nonzero
    };
    let (gesture, effect) = if kind == ClassKind::O2DSlider {
        (GestureKind::Slider, ActionOperation::Set)
    } else {
        (GestureKind::Activate, ActionOperation::Toggle)
    };
    let graph = super::super::seed(ui)
        .and_then(|s| s.graph.as_ref())
        .ok_or("Missing signed graph grants")?;
    if source.projection != expected
        || !graph
            .grants
            .iter()
            .any(|g| g.field == source.field && g.kind == gesture && g.operation == effect)
        || !ui
            .state_schema
            .fields
            .iter()
            .any(|f| f.id == source.field && f.writable && matches!(f.default, Value::Number(_)))
    {
        return Err(
            "Connection source is not an admitted writable field of this control type".into(),
        );
    }
    ui.selectors
        .iter()
        .find(|s| s.field == source.field && s.projection == source.projection)
        .map(|s| Binding::Local(s.local))
        .ok_or_else(|| "Missing admitted graph connection selector".into())
}

pub fn admitted(
    ui: &Ui,
    doc: &Document,
    object: &ObjectAddress,
    property: PropertyId,
    source: ConnectionSource,
) -> Result<AppConnection> {
    let (_, kind) = endpoint(doc, object, property)?;
    selector(ui, kind, &source)?;
    Ok(AppConnection {
        object: object.clone(),
        property,
        source,
    })
}

pub fn choices(
    ui: &Ui,
    doc: &Document,
    object: &ObjectAddress,
    property: PropertyId,
) -> Result<Vec<(String, ConnectionSource)>> {
    let (_, kind) = endpoint(doc, object, property)?;
    let mut fields = BTreeMap::new();
    for candidate in &ui.selectors {
        let source = ConnectionSource {
            field: candidate.field,
            projection: candidate.projection.clone(),
        };
        if selector(ui, kind, &source).is_ok() {
            let name = &ui
                .state_schema
                .fields
                .iter()
                .find(|f| f.id == candidate.field)
                .ok_or("Missing signed field")?
                .name;
            fields
                .entry(candidate.field)
                .or_insert_with(|| (name.clone(), source));
        }
    }
    Ok(fields.into_values().collect())
}

pub(super) fn project_metadata(
    doc: &mut Document,
    selectors: &[otrios_interfaces::canvas::Selector],
) -> Result<()> {
    let connections = doc.connections.clone();
    if connections.len() > MAX_CELLS
        || connections
            .windows(2)
            .any(|w| (&w[0].object, w[0].property) >= (&w[1].object, w[1].property))
    {
        return Err("Noncanonical or oversized connection surface".into());
    }
    for connection in connections {
        let (path, _) = endpoint(doc, &connection.object, connection.property)?;
        let binding = selectors
            .iter()
            .find(|s| {
                s.field == connection.source.field && s.projection == connection.source.projection
            })
            .map(|s| Binding::Local(s.local))
            .ok_or("Unknown typed graph connection")?;
        let root = doc
            .instances
            .iter_mut()
            .find(|i| i.id == connection.object.instance)
            .ok_or("Missing connected root")?;
        if let Some(assignment) = root.assignments.iter_mut().find(|a| a.target == path) {
            assignment.source = AssignmentSource::AppBinding(binding);
        } else {
            root.assignments.push(Assignment {
                target: path,
                source: AssignmentSource::AppBinding(binding),
            });
            root.assignments.sort_by(|a, b| a.target.cmp(&b.target));
        }
    }
    Ok(())
}

fn expr(e: &mut Expr, remap: &BTreeMap<u16, u16>) {
    match e {
        Expr::State(Binding::Local(id)) => {
            if let Some(mapped) = remap.get(id) {
                *id = *mapped;
            }
        }
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            expr(a, remap);
            expr(b, remap);
        }
        Expr::Select { condition, yes, no } => {
            expr(condition, remap);
            expr(yes, remap);
            expr(no, remap);
        }
        _ => {}
    }
}
fn node_expressions(
    node: &mut TemplateNode,
    blocked: &BTreeSet<u16>,
    remap: &BTreeMap<u16, u16>,
) -> Result<()> {
    if blocked.contains(&node.id) {
        return Ok(());
    }
    for e in &mut node.frame {
        expr(e, remap);
    }
    expr(&mut node.visible, remap);
    if let Some(s) = &mut node.semantics {
        expr(&mut s.label, remap);
        expr(&mut s.checked, remap);
    }
    match &mut node.shape {
        Shape::Canvas { .. } => {}
        Shape::Text {
            value,
            scale,
            color,
        } => {
            expr(value, remap);
            expr(scale, remap);
            expr(color, remap);
        }
        Shape::Superellipse { radius, color, .. } => {
            expr(radius, remap);
            expr(color, remap);
        }
        Shape::Circle { color } => expr(color, remap),
        _ => return Err("Unsupported connected control rendering primitive".into()),
    }
    for child in &mut node.children {
        node_expressions(child, blocked, remap)?;
    }
    Ok(())
}
fn collect_expr(e: &Expr, out: &mut BTreeSet<u16>) {
    match e {
        Expr::State(Binding::Local(id)) => {
            out.insert(*id);
        }
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            collect_expr(a, out);
            collect_expr(b, out);
        }
        Expr::Select { condition, yes, no } => {
            collect_expr(condition, out);
            collect_expr(yes, out);
            collect_expr(no, out);
        }
        _ => {}
    }
}
fn collect(node: &TemplateNode, blocked: &BTreeSet<u16>, out: &mut BTreeSet<u16>) -> Result<()> {
    if blocked.contains(&node.id) {
        return Ok(());
    }
    for e in &node.frame {
        collect_expr(e, out);
    }
    collect_expr(&node.visible, out);
    if let Some(s) = &node.semantics {
        collect_expr(&s.label, out);
        collect_expr(&s.checked, out);
    }
    match &node.shape {
        Shape::Canvas { .. } => {}
        Shape::Text {
            value,
            scale,
            color,
        } => {
            collect_expr(value, out);
            collect_expr(scale, out);
            collect_expr(color, out);
        }
        Shape::Superellipse { radius, color, .. } => {
            collect_expr(radius, out);
            collect_expr(color, out);
        }
        Shape::Circle { color } => collect_expr(color, out),
        _ => return Err("Unsupported connected control rendering primitive".into()),
    }
    for c in &node.children {
        collect(c, blocked, out)?;
    }
    Ok(())
}

/// Update display, semantics and the existing effect's field together. Owned
/// child roots keep their independent connections even inside a control's slot.
pub fn project_ui(admission: &Ui, ui: &mut Ui) -> Result<()> {
    if super::super::seed(ui).is_some_and(|seed| seed.designer.is_none()) {
        return Ok(());
    }
    let doc = metadata(super::super::seed(ui).ok_or("Missing OIB seed")?)?.clone();
    if doc.connections.is_empty() {
        return Ok(());
    }
    let mut blueprint = Layout::from_bytes(&ui.layout)?.blueprint().clone();
    for connection in &doc.connections {
        admitted(
            admission,
            &doc,
            &connection.object,
            connection.property,
            connection.source.clone(),
        )?;
        let (_, kind) = endpoint(&doc, &connection.object, connection.property)?;
        let id = doc
            .mappings
            .objects
            .iter()
            .find(|m| m.address == connection.object)
            .and_then(|m| m.layout_node)
            .ok_or("Missing connected control mapping")?;
        let Some(node) = blueprint
            .templates
            .iter_mut()
            .find_map(|t| super::super::node(&mut t.root, id))
        else {
            // Retained draft residents may be absent from this active paint tree.
            continue;
        };
        let Some(Responder::AppEvent { control, drag }) = node.responder else {
            return Err("Connected endpoint has no signed app responder".into());
        };
        let handler = ui
            .handlers
            .iter_mut()
            .find(|h| h.control == control)
            .ok_or("Connected endpoint has no admitted effect")?;
        if handler.fields.len() != 1
            || !match kind {
                ClassKind::O2DSlider => {
                    drag && handler.kind == GestureKind::Slider
                        && handler.operation == Operation::Set
                }
                ClassKind::O2DToggle => {
                    !drag
                        && handler.kind == GestureKind::Activate
                        && handler.operation == Operation::Toggle
                }
                _ => false,
            }
        {
            return Err("Connection would change an unadmitted effect".into());
        }
        let old = handler.fields[0];
        let blocked: BTreeSet<_> = doc
            .mappings
            .objects
            .iter()
            .filter(|m| {
                m.address.instance != connection.object.instance && m.address.references.is_empty()
            })
            .filter_map(|m| m.layout_node)
            .collect();
        let mut refs = BTreeSet::new();
        collect(node, &blocked, &mut refs)?;
        let mut remap = BTreeMap::new();
        if old != connection.source.field {
            for local in refs {
                let source = ui
                    .selectors
                    .iter()
                    .find(|s| s.local == local)
                    .ok_or("Missing connected visual selector")?
                    .clone();
                if source.field != old {
                    continue;
                }
                let replacement = if let Some(s) = ui.selectors.iter().find(|s| {
                    s.field == connection.source.field && s.projection == source.projection
                }) {
                    s.local
                } else {
                    if ui.selectors.len() >= 128 {
                        return Err("Connected selector capacity exhausted".into());
                    }
                    let local = ui
                        .selectors
                        .iter()
                        .map(|s| s.local)
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or("Connection selector identity exhausted")?;
                    let mut copied = source;
                    copied.local = local;
                    copied.field = connection.source.field;
                    ui.selectors.push(copied);
                    local
                };
                remap.insert(local, replacement);
            }
        }
        node_expressions(node, &blocked, &remap)?;
        if kind == ClassKind::O2DToggle {
            node.semantics
                .as_mut()
                .ok_or("Toggle connection has no checked semantics")?
                .checked = Expr::State(selector(admission, kind, &connection.source)?);
        }
        handler.fields[0] = connection.source.field;
    }
    ui.layout = Layout::encode(&blueprint)?;
    Ok(())
}
