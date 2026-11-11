// SPDX-License-Identifier: LGPL-3.0-only
//! Human review labels and typed field summaries.
use super::*;
pub(super) fn property_selection(
    s: &DocumentSource,
    address: &PropertyAddress,
) -> Option<Selection> {
    match address {
        PropertyAddress::Definition {
            definition: d,
            node: n,
            ..
        } if node(s, *d, *n).is_ok() => Some(Selection::DefinitionNode {
            definition: *d,
            node: *n,
        }),
        PropertyAddress::Occurrence { instance, path }
            if s.instances.iter().any(|i| i.id == instance.0) =>
        {
            Some(Selection::Object(ObjectAddress {
                instance: *instance,
                references: path.references.clone(),
                node: path.node,
            }))
        }
        _ => None,
    }
}
pub(super) fn selection(s: &DocumentSource, key: &Key) -> Option<Selection> {
    match key {
        Key::Definition(d)
        | Key::DefinitionName(d)
        | Key::PropertyDescriptor(d, _)
        | Key::Slots(d)
        | Key::Exposed(d, _)
            if definition(s, *d).is_ok() =>
        {
            Some(Selection::Definition(*d))
        }
        Key::Node(d, n)
        | Key::NodeName(d, n)
        | Key::Visual(d, n)
        | Key::Default(d, n, _)
        | Key::Reference(d, n, _)
        | Key::NodeOwner(d, n)
        | Key::Children(d, n)
            if node(s, *d, *n).is_ok() =>
        {
            Some(Selection::DefinitionNode {
                definition: *d,
                node: *n,
            })
        }
        Key::Preset(p) => s
            .presets
            .iter()
            .find(|p2| p2.id == *p)
            .map(|p| Selection::Definition(p.definition)),
        Key::Instance(id) | Key::InstanceName(id) | Key::Placement(id) => {
            s.instances.iter().find(|i| i.id == id.0).and_then(|i| {
                definition(s, DefinitionId(i.component)).ok().map(|d| {
                    Selection::Object(ObjectAddress {
                        instance: *id,
                        references: vec![],
                        node: d.root,
                    })
                })
            })
        }
        Key::Connection(object, _)
        | Key::Siblings(Destination::Slot {
            component: object, ..
        }) if s.instances.iter().any(|i| i.id == object.instance.0) => {
            Some(Selection::Object(object.clone()))
        }
        _ => None,
    }
}
pub(super) fn value_text(value: &Option<Value>) -> String {
    match value {
        None => "Inherited".into(),
        Some(Value::Text(v)) => v.clone(),
        Some(Value::Number(v)) => v.to_string(),
        Some(Value::Boolean(v)) => if *v { "On" } else { "Off" }.into(),
        Some(Value::Color(v)) => format!("#{:02X}{:02X}{:02X}{:02X}", v[0], v[1], v[2], v[3]),
        Some(Value::Series(v)) => format!("{} values", v.len()),
        Some(Value::TimedSeries(v)) => format!("{} timed values", v.len()),
    }
}
fn expression_text(value: &Expr) -> String {
    match value {
        Expr::Literal(v) => value_text(&Some(v.clone())),
        Expr::Width => "Available width".into(),
        Expr::Height => "Available height".into(),
        Expr::Parameter(id) => format!("Parameter {id}"),
        Expr::State(otrios_interfaces::layout::Binding::Local(id)) => format!("App binding {id}"),
        Expr::Add(a, b) => format!("({} + {})", expression_text(a), expression_text(b)),
        Expr::Subtract(a, b) => format!("({} − {})", expression_text(a), expression_text(b)),
        Expr::Multiply(a, b) => format!("({} × {})", expression_text(a), expression_text(b)),
        Expr::Select { condition, yes, no } => format!(
            "If {}: {}; otherwise {}",
            expression_text(condition),
            expression_text(yes),
            expression_text(no)
        ),
    }
}
fn path_text(s: &DocumentSource, d: DefinitionId, p: &PropertyPath) -> String {
    format!("{} / Property {}", node_name(s, d, p.node), p.property.0)
}
fn assignment_text(source: &AssignmentSource) -> String {
    match source {
        AssignmentSource::Literal(v) => value_text(&Some(v.clone())),
        AssignmentSource::Forward(p) => format!("Forwarded property {}", p.property.0),
        AssignmentSource::AppBinding(otrios_interfaces::layout::Binding::Local(id)) => {
            format!("App binding {id}")
        }
    }
}
fn visual_text(v: &Option<otrios_interfaces::oib::graph::Visual>) -> String {
    use otrios_interfaces::layout::{Direction, Sizing};
    let Some(v) = v else { return "Absent".into() };
    let axis = |mode: Sizing| match mode {
        Sizing::Fixed => "Fixed",
        Sizing::Hug => "Hug",
        Sizing::Fill => "Fill",
    };
    let l = v.layout.unwrap_or_default();
    let mut text = format!(
        "X {}, Y {}, W {} {}, H {} {}; visible {}",
        expression_text(&v.frame[0]),
        expression_text(&v.frame[1]),
        axis(l.width.mode),
        expression_text(&v.frame[2]),
        axis(l.height.mode),
        expression_text(&v.frame[3]),
        expression_text(&v.visible)
    );
    if let Some(c) = l.container {
        text.push_str(&format!(
            "; {}, gap {}, padding {} / {} / {} / {}",
            match c.direction {
                Direction::Horizontal => "Horizontal",
                Direction::Vertical => "Vertical",
            },
            c.gap,
            c.padding[0],
            c.padding[1],
            c.padding[2],
            c.padding[3]
        ));
    }
    text.push_str(&match &v.shape {
        Shape::Canvas { clip } => format!("; clip {}", if *clip { "on" } else { "off" }),
        Shape::Text {
            value,
            scale,
            color,
        } => format!(
            "; text {}, scale {}, color {}",
            expression_text(value),
            expression_text(scale),
            expression_text(color)
        ),
        Shape::Superellipse { radius, color, .. } => format!(
            "; radius {}, fill {}",
            expression_text(radius),
            expression_text(color)
        ),
        Shape::Circle { color } => format!("; circle fill {}", expression_text(color)),
        Shape::SeriesPath { width, color, .. } | Shape::TimedSeriesPath { width, color, .. } => {
            format!("; stroke {} {}", width, expression_text(color))
        }
        Shape::Slot => "; content slot".into(),
    });
    text
}
fn definition_name(s: &DocumentSource, d: DefinitionId) -> String {
    definition(s, d)
        .map(|v| v.name.clone())
        .unwrap_or_else(|_| format!("Component {}", d.0))
}
fn node_name(s: &DocumentSource, d: DefinitionId, n: NodeId) -> String {
    node(s, d, n)
        .map(|v| v.name.clone())
        .unwrap_or_else(|_| format!("Layer {}", n.0))
}
fn instance_name(s: &DocumentSource, id: InstanceId) -> String {
    s.instances
        .iter()
        .find(|v| v.id == id.0)
        .map(|v| v.name.clone())
        .unwrap_or_else(|| format!("Instance {}", id.0))
}
pub(super) fn property_label(s: &DocumentSource, address: &PropertyAddress) -> String {
    match address {
        PropertyAddress::Definition {
            definition: d,
            node: n,
            property: p,
        } => format!(
            "{} / {} / Property {}",
            definition_name(s, *d),
            node_name(s, *d, *n),
            p.0
        ),
        PropertyAddress::Occurrence { instance, path } => format!(
            "{} / Property {}",
            instance_name(s, *instance),
            path.property.0
        ),
        _ => "Control property".into(),
    }
}
pub(super) fn label(s: &DocumentSource, key: &Key) -> String {
    match key {
        Key::Styles => "Styles and variables".into(),
        Key::Definition(d) => definition_name(s, *d),
        Key::DefinitionName(d) => format!("{} / Name", definition_name(s, *d)),
        Key::Node(d, n) => format!("{} / {}", definition_name(s, *d), node_name(s, *d, *n)),
        Key::NodeName(d, n)
        | Key::Visual(d, n)
        | Key::Default(d, n, _)
        | Key::NodeOwner(d, n)
        | Key::Children(d, n)
        | Key::Reference(d, n, _) => format!(
            "{} / {} / {}",
            definition_name(s, *d),
            node_name(s, *d, *n),
            match key {
                Key::NodeName(..) => "Name",
                Key::Visual(..) => "Appearance and layout",
                Key::Default(..) => "Default",
                Key::NodeOwner(..) => "Owner",
                Key::Children(..) => "Layer order",
                _ => "Reference binding",
            }
        ),
        Key::PropertyDescriptor(d, p) => {
            format!("{} / Property {} declaration", definition_name(s, *d), p.0)
        }
        Key::Slots(d) => format!("{} / Content slots", definition_name(s, *d)),
        Key::Exposed(d, p) => format!("{} / Outlet {}", definition_name(s, *d), p.0),
        Key::Preset(p) => s
            .presets
            .iter()
            .find(|v| v.id == *p)
            .map(|v| format!("{} / Style reference", v.name))
            .unwrap_or_else(|| format!("Style reference {p}")),
        Key::Instance(i) => instance_name(s, *i),
        Key::InstanceName(i) => format!("{} / Name", instance_name(s, *i)),
        Key::Placement(i) => format!("{} / Position and size", instance_name(s, *i)),
        Key::Siblings(Destination::Canvas) => "Canvas / Component order".into(),
        Key::Siblings(Destination::Slot { component, slot }) => format!(
            "{} / Slot {} order",
            instance_name(s, component.instance),
            slot.0
        ),
        Key::Connection(object, p) => format!(
            "{} / Property {} binding",
            instance_name(s, object.instance),
            p.0
        ),
    }
}
pub(super) fn source_text(s: &DocumentSource, key: &Key) -> String {
    let absent = || "Absent".to_string();
    match key {
        Key::Styles => s
            .styles
            .as_ref()
            .map(|styles| {
                format!(
                    "{} styles, {} variables",
                    styles
                        .catalog
                        .systems
                        .iter()
                        .map(|system| system.styles.len())
                        .sum::<usize>(),
                    styles
                        .catalog
                        .systems
                        .iter()
                        .map(|system| system.variables.len())
                        .sum::<usize>()
                )
            })
            .unwrap_or_else(absent),
        Key::Definition(d) => definition(s, *d)
            .map(|v| {
                format!(
                    "{} · {} layers, {} outlets, {} slots",
                    v.name,
                    v.nodes.len(),
                    v.exposed.len(),
                    v.slots.len()
                )
            })
            .unwrap_or_else(|_| absent()),
        Key::DefinitionName(d) => definition(s, *d)
            .map(|v| v.name.clone())
            .unwrap_or_else(|_| absent()),
        Key::Node(d, n) => node(s, *d, *n)
            .map(|v| format!("{}; {}", v.name, visual_text(&v.visual)))
            .unwrap_or_else(|_| absent()),
        Key::NodeName(d, n) => node(s, *d, *n)
            .map(|v| v.name.clone())
            .unwrap_or_else(|_| absent()),
        Key::Visual(d, n) => node(s, *d, *n)
            .map(|v| visual_text(&v.visual))
            .unwrap_or_else(|_| absent()),
        Key::Default(d, n, p) => node(s, *d, *n)
            .ok()
            .and_then(|v| v.defaults.iter().find(|v| v.property == *p))
            .map(|v| value_text(&Some(v.value.clone())))
            .unwrap_or_else(|| "Inherited".into()),
        Key::PropertyDescriptor(d, p) => definition(s, *d)
            .ok()
            .and_then(|d| d.properties.iter().find(|v| v.id == *p))
            .map(|p| {
                format!(
                    "{} · {:?}; default {}",
                    p.name,
                    p.value_type,
                    value_text(&Some(p.default.clone()))
                )
            })
            .unwrap_or_else(absent),
        Key::Slots(d) => definition(s, *d)
            .map(|v| {
                v.slots
                    .iter()
                    .map(|v| {
                        format!(
                            "{} ({}–{} children)",
                            v.name, v.cardinality[0], v.cardinality[1]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|_| absent()),
        Key::Exposed(d, p) => definition(s, *d)
            .ok()
            .and_then(|v| v.exposed.iter().find(|v| v.property == *p))
            .map(|v| {
                if v.targets.is_empty() {
                    "Disconnected".into()
                } else {
                    v.targets
                        .iter()
                        .map(|p| path_text(s, *d, p))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            })
            .unwrap_or_else(absent),
        Key::Reference(d, n, p) => node(s, *d, *n)
            .ok()
            .and_then(|v| v.reference.as_ref())
            .and_then(|r| r.assignments.iter().find(|a| a.target == *p))
            .map(|a| assignment_text(&a.source))
            .unwrap_or_else(absent),
        Key::NodeOwner(d, n) => node(s, *d, *n)
            .ok()
            .and_then(|v| v.owner)
            .map(|p| node_name(s, *d, p))
            .unwrap_or_else(|| "Component root".into()),
        Key::Children(d, n) => node(s, *d, *n)
            .map(|v| {
                v.visual_children
                    .iter()
                    .map(|n| node_name(s, *d, *n))
                    .collect::<Vec<_>>()
                    .join(" → ")
            })
            .unwrap_or_else(|_| absent()),
        Key::Preset(p) => s
            .presets
            .iter()
            .find(|v| v.id == *p)
            .map(|v| {
                format!(
                    "{} → {}; {}",
                    v.name,
                    definition_name(s, v.definition),
                    v.assignments
                        .iter()
                        .map(|a| format!(
                            "{}: {}",
                            path_text(s, v.definition, &a.target),
                            match &a.source {
                                otrios_interfaces::oib::graph::Source::Literal(v) =>
                                    value_text(&Some(v.clone())),
                                otrios_interfaces::oib::graph::Source::Forward(p) =>
                                    format!("Forwarded property {}", p.property.0),
                                otrios_interfaces::oib::graph::Source::AppBinding {
                                    field, ..
                                } => format!("App field {field}"),
                            }
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            })
            .unwrap_or_else(absent),
        Key::Instance(i) => s
            .instances
            .iter()
            .find(|v| v.id == i.0)
            .map(|v| {
                format!(
                    "{} → {}",
                    v.name,
                    definition_name(s, DefinitionId(v.component))
                )
            })
            .unwrap_or_else(absent),
        Key::InstanceName(i) => s
            .instances
            .iter()
            .find(|v| v.id == i.0)
            .map(|v| v.name.clone())
            .unwrap_or_else(absent),
        Key::Placement(i) => {
            let destination = match destination(s, *i) {
                Destination::Canvas => "Canvas".into(),
                Destination::Slot { component, slot } => {
                    format!("{} / Slot {}", instance_name(s, component.instance), slot.0)
                }
            };
            match frame(s, *i) {
                Some(f) => format!(
                    "{destination}; X {}, Y {}, W {}, H {}",
                    f[0], f[1], f[2], f[3]
                ),
                None => format!("{destination}; automatic position and size"),
            }
        }
        Key::Siblings(d) => sibling_ids(s, d)
            .iter()
            .map(|i| instance_name(s, *i))
            .collect::<Vec<_>>()
            .join(" → "),
        Key::Connection(o, p) => s
            .connections
            .iter()
            .find(|v| v.object == *o && v.property == *p)
            .map(|v| format!("App field {}", v.source.field))
            .unwrap_or_else(|| "Signed binding".into()),
    }
}
