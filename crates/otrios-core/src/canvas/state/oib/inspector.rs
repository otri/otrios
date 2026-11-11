// SPDX-License-Identifier: LGPL-3.0-only
//! Typed inspector fields backed by generation-guarded graph edits.
use super::*;
use crate::canvas::oib::document::PropertyInfo;
use otrios_interfaces::{
    layout::{Expr, Shape, ValueType},
    oib::{authoring::*, designer::*, graph},
};

fn value(visual: &graph::Visual, property: StructuralProperty) -> Option<Value> {
    let layout = visual.layout.unwrap_or_default();
    let number = match property {
        StructuralProperty::LayoutGap => Some(layout.container?.gap),
        StructuralProperty::Padding | StructuralProperty::PaddingLeft => {
            Some(layout.container?.padding[0])
        }
        StructuralProperty::PaddingTop => Some(layout.container?.padding[1]),
        StructuralProperty::PaddingRight => Some(layout.container?.padding[2]),
        StructuralProperty::PaddingBottom => Some(layout.container?.padding[3]),
        StructuralProperty::MinWidth => Some(layout.width.min),
        StructuralProperty::MinHeight => Some(layout.height.min),
        StructuralProperty::MaxWidth => Some(layout.width.max.unwrap_or(8192.)),
        StructuralProperty::MaxHeight => Some(layout.height.max.unwrap_or(8192.)),
        _ => None,
    };
    if let Some(number) = number {
        return Some(Value::Number(number));
    }
    let expr = match property {
        StructuralProperty::X => &visual.frame[0],
        StructuralProperty::Y => &visual.frame[1],
        StructuralProperty::Width => &visual.frame[2],
        StructuralProperty::Height => &visual.frame[3],
        StructuralProperty::Visible => &visual.visible,
        StructuralProperty::Text => match &visual.shape {
            Shape::Text { value, .. } => value,
            _ => return None,
        },
        StructuralProperty::Color => match &visual.shape {
            Shape::Text { color, .. }
            | Shape::Circle { color }
            | Shape::Superellipse { color, .. } => color,
            _ => return None,
        },
        StructuralProperty::Radius => match &visual.shape {
            Shape::Superellipse { radius, .. } => radius,
            _ => return None,
        },
        _ => return None,
    };
    if let Expr::Literal(value) = expr {
        Some(value.clone())
    } else {
        None
    }
}

fn field(
    target: StructuralTarget,
    property: StructuralProperty,
    label: &str,
    current: Value,
    inherited: Option<Value>,
) -> PropertyInfo {
    let (value_type, editor) = match current {
        Value::Text(_) => (ValueType::Text, PropertyEditor::TextSuggestions),
        Value::Number(_) => (ValueType::Number, PropertyEditor::NumberSuggestions),
        Value::Color(_) => (ValueType::Color, PropertyEditor::Palette),
        Value::Boolean(_) => (ValueType::Boolean, PropertyEditor::Toggle),
        _ => unreachable!("inspector scalar"),
    };
    let range = if value_type == ValueType::Number {
        Some(match property {
            StructuralProperty::X | StructuralProperty::Y => [-8192., 8192.],
            StructuralProperty::Width | StructuralProperty::Height => [1., 8192.],
            StructuralProperty::LayoutGap
            | StructuralProperty::Padding
            | StructuralProperty::PaddingLeft
            | StructuralProperty::PaddingTop
            | StructuralProperty::PaddingRight
            | StructuralProperty::PaddingBottom
            | StructuralProperty::MinWidth
            | StructuralProperty::MinHeight
            | StructuralProperty::MaxWidth
            | StructuralProperty::MaxHeight => [0., 8192.],
            _ => [0., 1000.],
        })
    } else {
        None
    };
    PropertyInfo {
        address: Some(PropertyAddress::Structural { target, property }),
        cell: None,
        descriptor: PropertyDescriptor {
            id: PropertyId(0x8000 + property as u64),
            name: label.into(),
            label: label.into(),
            value_type,
            default: inherited.clone().unwrap_or_else(|| current.clone()),
            number_range: range,
            number_step: range.map(|_| 1.),
            units: range.map(|_| "px".into()),
            group: if property == StructuralProperty::Name {
                InspectorGroup::Identity
            } else if matches!(
                property,
                StructuralProperty::X
                    | StructuralProperty::Y
                    | StructuralProperty::Width
                    | StructuralProperty::Height
            ) {
                InspectorGroup::Layout
            } else {
                InspectorGroup::Appearance
            },
            editor,
            policy: PropertyPolicy {
                live_literal: true,
                app_binding: false,
            },
        },
        overridden: inherited
            .as_ref()
            .is_some_and(|baseline| baseline != &current),
        value: Some(current),
        inherited,
        origin: "Authored graph".into(),
        source: "Component design".into(),
    }
}

fn original_definition(source: &DocumentSource, id: DefinitionId) -> DefinitionId {
    source
        .baseline
        .iter()
        .find_map(|entry| match entry {
            BaselineIdentity::Definition { source, current } if *current == id => Some(*source),
            _ => None,
        })
        .unwrap_or(id)
}
fn original_node(source: &DocumentSource, definition: DefinitionId, id: NodeId) -> NodeId {
    source
        .baseline
        .iter()
        .find_map(|entry| match entry {
            BaselineIdentity::Node {
                definition: owner,
                source,
                current,
            } if *owner == definition && *current == id => Some(*source),
            _ => None,
        })
        .unwrap_or(id)
}
fn baseline<'a>(
    signed: &'a graph::Document,
    source: &DocumentSource,
    definition: DefinitionId,
    node: NodeId,
) -> Option<&'a DefinitionNode> {
    let definition = original_definition(source, definition);
    let node = original_node(source, definition, node);
    signed
        .definitions
        .iter()
        .find(|d| d.id == definition)?
        .nodes
        .iter()
        .find(|n| n.id == node)
}

impl StateAdapter {
    pub(in crate::canvas) fn oib_next_identity(&self) -> Result<u64> {
        Ok(self.doc.oib_source_shared()?.next_identity)
    }

    pub(in crate::canvas) fn oib_structural_properties(
        &self,
        selected: &Selection,
    ) -> Result<Vec<PropertyInfo>> {
        let source = self.doc.oib_source_shared()?;
        let binding = self.doc.oib_binding()?;
        let imported = crate::canvas::oib::imports::imported_definitions(&binding.seed);
        if match selected {
            Selection::Definition(id) => imported.contains(id),
            Selection::DefinitionNode { definition, .. } => imported.contains(definition),
            _ => false,
        } {
            // Immutable library structure is edited in its publishing workspace.
            // Ordinary shared/occurrence property overrides remain available.
            return Ok(vec![]);
        }
        let signed = binding.seed.graph.as_ref().ok_or("Missing signed graph")?;
        let resolved = self.doc.oib_baseline_graph()?;
        let mut result = vec![];
        let selected_definition = match selected {
            Selection::Definition(id) | Selection::DefinitionNode { definition: id, .. } => {
                Some(*id)
            }
            _ => None,
        };
        if let Some(id) = selected_definition
            && let Some(def) = source.definitions.iter().find(|d| d.id == id)
            && match selected {
                Selection::Definition(_) => true,
                Selection::DefinitionNode { node, .. } => *node == def.root,
                _ => false,
            }
        {
            for descriptor in &def.properties {
                result.push(field(
                    StructuralTarget::Property {
                        definition: id,
                        property: descriptor.id,
                    },
                    StructuralProperty::Name,
                    &format!("Property name: {}", descriptor.name),
                    Value::Text(descriptor.name.clone()),
                    None,
                ));
            }
        }
        let (definition, selected_node) = match selected {
            Selection::Definition(id) => {
                let definition = source
                    .definitions
                    .iter()
                    .find(|d| d.id == *id)
                    .ok_or("Missing component")?;
                let inherited = signed
                    .definitions
                    .iter()
                    .find(|d| d.id == original_definition(&source, *id))
                    .map(|d| Value::Text(d.name.clone()));
                result.push(field(
                    StructuralTarget::Definition(*id),
                    StructuralProperty::Name,
                    "Component name",
                    Value::Text(definition.name.clone()),
                    inherited,
                ));
                for preset in source
                    .presets
                    .iter()
                    .filter(|p| p.palette && p.definition == *id)
                {
                    let original = source
                        .baseline
                        .iter()
                        .find_map(|entry| match entry {
                            BaselineIdentity::Preset { source, current }
                                if *current == preset.id =>
                            {
                                Some(*source)
                            }
                            _ => None,
                        })
                        .unwrap_or(preset.id);
                    let inherited = signed
                        .presets
                        .iter()
                        .find(|p| p.id == original)
                        .map(|p| Value::Text(p.name.clone()));
                    result.push(field(
                        StructuralTarget::Preset {
                            preset: preset.id,
                            definition: *id,
                        },
                        StructuralProperty::Name,
                        &format!("Preset: {}", preset.name),
                        Value::Text(preset.name.clone()),
                        inherited,
                    ));
                }
                (*id, definition.root)
            }
            Selection::DefinitionNode { definition, node } => {
                let current = source
                    .definitions
                    .iter()
                    .find(|d| d.id == *definition)
                    .and_then(|d| d.nodes.iter().find(|n| n.id == *node))
                    .ok_or("Missing component node")?;
                result.push(field(
                    StructuralTarget::Node {
                        definition: *definition,
                        node: *node,
                    },
                    StructuralProperty::Name,
                    "Object name",
                    Value::Text(current.name.clone()),
                    baseline(signed, &source, *definition, *node)
                        .map(|n| Value::Text(n.name.clone())),
                ));
                (*definition, *node)
            }
            Selection::Object(object) if object.references.is_empty() => {
                if let Some(instance) = source.instances.iter().find(|i| i.id == object.instance.0)
                    && source
                        .definitions
                        .iter()
                        .chain(resolved.definitions.iter())
                        .any(|d| d.id.0 == instance.component && d.root == object.node)
                {
                    let original = source
                        .baseline
                        .iter()
                        .find_map(|entry| match entry {
                            BaselineIdentity::Instance { source, current }
                                if *current == object.instance =>
                            {
                                Some(*source)
                            }
                            _ => None,
                        })
                        .unwrap_or(object.instance);
                    let inherited = signed
                        .instances
                        .iter()
                        .find(|i| i.id == original)
                        .map(|i| Value::Text(i.name.clone()));
                    result.push(field(
                        StructuralTarget::Instance(object.clone()),
                        StructuralProperty::Name,
                        "Instance name",
                        Value::Text(instance.name.clone()),
                        inherited,
                    ));
                }
                return Ok(result);
            }
            _ => return Ok(result),
        };
        let node = source
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .and_then(|d| d.nodes.iter().find(|n| n.id == selected_node))
            .ok_or("Missing visual node")?;
        if let Some(visual) = &node.visual {
            for (property, label) in [
                (StructuralProperty::X, "X"),
                (StructuralProperty::Y, "Y"),
                (StructuralProperty::Width, "Design width"),
                (StructuralProperty::Height, "Design height"),
                (StructuralProperty::Text, "Text"),
                (StructuralProperty::Color, "Fill color"),
                (StructuralProperty::Radius, "Corner radius"),
                (StructuralProperty::Visible, "Visible"),
                (StructuralProperty::LayoutGap, "Gap"),
                (StructuralProperty::Padding, "Padding"),
                (StructuralProperty::PaddingLeft, "Left padding"),
                (StructuralProperty::PaddingTop, "Top padding"),
                (StructuralProperty::PaddingRight, "Right padding"),
                (StructuralProperty::PaddingBottom, "Bottom padding"),
                (StructuralProperty::MinWidth, "Minimum width"),
                (StructuralProperty::MinHeight, "Minimum height"),
                (StructuralProperty::MaxWidth, "Maximum width"),
                (StructuralProperty::MaxHeight, "Maximum height"),
            ] {
                if let Some(current) = value(visual, property) {
                    let inherited = baseline(signed, &source, definition, selected_node)
                        .and_then(|n| n.visual.as_ref())
                        .and_then(|v| value(v, property));
                    result.push(field(
                        StructuralTarget::Node {
                            definition,
                            node: selected_node,
                        },
                        property,
                        label,
                        current,
                        inherited,
                    ));
                }
            }
        }
        Ok(result)
    }

    pub(in crate::canvas) fn oib_structural_operation(
        &self,
        address: &PropertyAddress,
        supplied: Option<Value>,
    ) -> Result<DocumentEdit> {
        let PropertyAddress::Structural { target, property } = address else {
            return Err("Not a graph source field".into());
        };
        let selected = match target {
            StructuralTarget::Definition(id)
            | StructuralTarget::Property { definition: id, .. } => Selection::Definition(*id),
            StructuralTarget::Node { definition, node } => Selection::DefinitionNode {
                definition: *definition,
                node: *node,
            },
            StructuralTarget::Instance(object) => Selection::Object(object.clone()),
            StructuralTarget::Preset { definition, .. } => Selection::Definition(*definition),
        };
        let info = self
            .oib_structural_properties(&selected)?
            .into_iter()
            .find(|p| p.address.as_ref() == Some(address))
            .ok_or("Graph source field unavailable")?;
        let candidate = supplied
            .or(info.inherited)
            .ok_or("No authored default for this field")?;
        let source = self.doc.oib_source_shared()?;
        let name = || -> Result<String> {
            match &candidate {
                Value::Text(value)
                    if !value.is_empty() && value.len() <= 64 && !value.contains('\0') =>
                {
                    Ok(value.clone())
                }
                _ => Err("Names use 1–64 text bytes".into()),
            }
        };
        let operation = match target {
            StructuralTarget::Property {
                definition,
                property: endpoint,
            } if *property == StructuralProperty::Name => {
                let mut descriptor = source
                    .definitions
                    .iter()
                    .find(|d| d.id == *definition)
                    .and_then(|d| d.properties.iter().find(|p| p.id == *endpoint))
                    .ok_or("Endpoint changed")?
                    .clone();
                descriptor.name = name()?;
                descriptor.label = descriptor.name.clone();
                DocumentEdit::Definition(DefinitionEdit::SetProperty {
                    definition: *definition,
                    descriptor,
                })
            }

            StructuralTarget::Definition(definition) if *property == StructuralProperty::Name => {
                DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                    definition: *definition,
                    name: name()?,
                })
            }
            StructuralTarget::Node { definition, node }
                if *property == StructuralProperty::Name =>
            {
                DocumentEdit::Definition(DefinitionEdit::RenameNode {
                    definition: *definition,
                    node: *node,
                    name: name()?,
                })
            }
            StructuralTarget::Instance(object) if *property == StructuralProperty::Name => {
                DocumentEdit::Hierarchy(HierarchyEdit::Rename {
                    instance: object.instance,
                    name: name()?,
                })
            }
            StructuralTarget::Preset { preset, definition }
                if *property == StructuralProperty::Name =>
            {
                let mut preset = source
                    .presets
                    .iter()
                    .find(|p| p.id == *preset && p.definition == *definition)
                    .ok_or("Preset changed")?
                    .clone();
                preset.name = name()?;
                DocumentEdit::Preset(PresetEdit::Update { preset })
            }
            StructuralTarget::Node { definition, node } => {
                let mut visual = source
                    .definitions
                    .iter()
                    .find(|d| d.id == *definition)
                    .and_then(|d| d.nodes.iter().find(|n| n.id == *node))
                    .and_then(|n| n.visual.clone())
                    .ok_or("Missing visual declaration")?;
                match (property, &candidate) {
                    (
                        StructuralProperty::X
                        | StructuralProperty::Y
                        | StructuralProperty::Width
                        | StructuralProperty::Height,
                        Value::Number(number),
                    ) => {
                        let (index, minimum, maximum) = match property {
                            StructuralProperty::X => (0, -8192., 8192.),
                            StructuralProperty::Y => (1, -8192., 8192.),
                            StructuralProperty::Width => (2, 1., 8192.),
                            _ => (3, 1., 8192.),
                        };
                        if !number.is_finite() || *number < minimum || *number > maximum {
                            return Err("Invalid design geometry".into());
                        }
                        visual.frame[index] = Expr::Literal(candidate.clone());
                    }
                    (
                        property @ (StructuralProperty::LayoutGap
                        | StructuralProperty::Padding
                        | StructuralProperty::PaddingLeft
                        | StructuralProperty::PaddingTop
                        | StructuralProperty::PaddingRight
                        | StructuralProperty::PaddingBottom
                        | StructuralProperty::MinWidth
                        | StructuralProperty::MinHeight
                        | StructuralProperty::MaxWidth
                        | StructuralProperty::MaxHeight),
                        Value::Number(number),
                    ) => {
                        if !number.is_finite() || !(0. ..=8192.).contains(number) {
                            return Err("Invalid layout length".into());
                        }
                        visual.resize = None;
                        let layout = visual.layout.get_or_insert_with(Default::default);
                        match property {
                            StructuralProperty::LayoutGap => {
                                layout
                                    .container
                                    .as_mut()
                                    .ok_or("Node has no Auto Layout")?
                                    .gap = *number
                            }
                            StructuralProperty::Padding => {
                                layout
                                    .container
                                    .as_mut()
                                    .ok_or("Node has no Auto Layout")?
                                    .padding = [*number; 4]
                            }
                            StructuralProperty::PaddingLeft => {
                                layout
                                    .container
                                    .as_mut()
                                    .ok_or("Node has no Auto Layout")?
                                    .padding[0] = *number
                            }
                            StructuralProperty::PaddingTop => {
                                layout
                                    .container
                                    .as_mut()
                                    .ok_or("Node has no Auto Layout")?
                                    .padding[1] = *number
                            }
                            StructuralProperty::PaddingRight => {
                                layout
                                    .container
                                    .as_mut()
                                    .ok_or("Node has no Auto Layout")?
                                    .padding[2] = *number
                            }
                            StructuralProperty::PaddingBottom => {
                                layout
                                    .container
                                    .as_mut()
                                    .ok_or("Node has no Auto Layout")?
                                    .padding[3] = *number
                            }
                            StructuralProperty::MinWidth => layout.width.min = *number,
                            StructuralProperty::MinHeight => layout.height.min = *number,
                            StructuralProperty::MaxWidth => {
                                layout.width.max = (*number < 8192.).then_some(*number)
                            }
                            StructuralProperty::MaxHeight => {
                                layout.height.max = (*number < 8192.).then_some(*number)
                            }
                            _ => unreachable!(),
                        }
                        if layout.width.max.is_some_and(|max| max < layout.width.min)
                            || layout.height.max.is_some_and(|max| max < layout.height.min)
                        {
                            return Err("Minimum exceeds maximum".into());
                        }
                    }
                    (StructuralProperty::Visible, Value::Boolean(_)) => {
                        visual.visible = Expr::Literal(candidate.clone())
                    }
                    (StructuralProperty::Color, Value::Color(_)) => match &mut visual.shape {
                        Shape::Text { color, .. }
                        | Shape::Circle { color }
                        | Shape::Superellipse { color, .. } => {
                            *color = Expr::Literal(candidate.clone())
                        }
                        _ => return Err("Object has no fill color".into()),
                    },
                    (StructuralProperty::Text, Value::Text(text))
                        if !text.is_empty() && text.len() <= 96 && !text.contains('\0') =>
                    {
                        match &mut visual.shape {
                            Shape::Text { value, .. } => {
                                if let Some(semantics) = &mut visual.semantics
                                    && semantics.label == *value
                                {
                                    semantics.label = Expr::Literal(candidate.clone());
                                }
                                *value = Expr::Literal(candidate.clone());
                            }
                            _ => return Err("Object has no text".into()),
                        }
                    }
                    (StructuralProperty::Radius, Value::Number(radius))
                        if radius.is_finite() && (0. ..=1000.).contains(radius) =>
                    {
                        match &mut visual.shape {
                            Shape::Superellipse { radius, .. } => {
                                *radius = Expr::Literal(candidate.clone())
                            }
                            _ => return Err("Object has no corner radius".into()),
                        }
                    }
                    _ => return Err("Invalid graph source field value".into()),
                }
                DocumentEdit::Definition(DefinitionEdit::SetVisual {
                    definition: *definition,
                    node: *node,
                    visual: Some(visual),
                })
            }
            _ => return Err("Unsupported graph source field".into()),
        };
        Ok(operation)
    }
}
