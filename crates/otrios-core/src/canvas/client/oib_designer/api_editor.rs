// SPDX-License-Identifier: LGPL-3.0-only
//! Existing typed endpoint forwarding. This never invents descriptors or grants.
use super::*;
use otrios_interfaces::oib::{InstanceOperation, authoring::*, designer::*};

#[derive(Clone)]
pub(super) struct Choice {
    pub label: String,
    pub connected: bool,
    pub operation: Option<InstanceOperation>,
    pub reason: Option<String>,
}
#[derive(Clone)]
pub(super) struct Field {
    pub descriptor: PropertyDescriptor,
    pub path: PropertyPath,
}
#[derive(Clone)]
pub(super) struct Model {
    pub fields: Vec<Field>,
    pub selected: Option<PropertyPath>,
    pub choices: Vec<Choice>,
    pub root: bool,
}
pub(super) fn definition(selection: &Selection) -> Option<DefinitionId> {
    match selection {
        Selection::Definition(id) | Selection::DefinitionNode { definition: id, .. } => Some(*id),
        _ => None,
    }
}
fn operation(
    state: &StateAdapter,
    definition: DefinitionId,
    property: PropertyId,
    targets: Vec<PropertyPath>,
) -> Result<InstanceOperation> {
    let mut apply = state.oib_apply_base()?;
    apply
        .operations
        .push(DocumentEdit::Definition(DefinitionEdit::Expose {
            definition,
            property,
            targets,
        }));
    state.preview_oib_apply(&apply)?;
    Ok(InstanceOperation {
        generation: apply.generation,
        operations: apply.operations,
    })
}
pub(super) fn model(
    state: &StateAdapter,
    seed: &otrios_interfaces::oib::Seed,
    selection: &Selection,
    selected: Option<&PropertyPath>,
    writable: bool,
) -> Result<Option<Model>> {
    let Some(id) = definition(selection) else {
        return Ok(None);
    };
    let graph = crate::canvas::oib::imports::resolve(seed)?;
    let metadata = seed.designer.as_ref().ok_or("Missing API metadata")?;
    let definition = graph
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or("Missing API component")?;
    let node_id = match selection {
        Selection::DefinitionNode { node, .. } => *node,
        _ => definition.root,
    };
    let node = definition
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or("Missing API object")?;
    let root = node_id == definition.root;
    let (target, references) = if let Some(reference) = &node.reference
        && !root
    {
        let linked = graph
            .definitions
            .iter()
            .find(|d| d.id == reference.definition)
            .ok_or("Missing linked API")?;
        (
            linked
                .nodes
                .iter()
                .find(|n| n.id == linked.root)
                .ok_or("Missing linked root")?,
            vec![node.id],
        )
    } else {
        (node, vec![])
    };
    let fields: Vec<_> = crate::canvas::oib::document::descriptors(metadata, target.class)?
        .into_iter()
        .map(|descriptor| {
            let path = PropertyPath {
                references: references.clone(),
                node: target.id,
                property: descriptor.id,
            };
            Field { descriptor, path }
        })
        .collect();
    let selected = selected
        .filter(|p| fields.iter().any(|f| &f.path == *p))
        .cloned();
    let mut choices = vec![];
    let imported = crate::canvas::oib::imports::imported_definitions(seed).contains(&id);
    if let Some(path) = &selected {
        let field = fields.iter().find(|f| &f.path == path).unwrap();
        if root {
            for target in definition
                .exposed
                .iter()
                .filter(|e| e.property == path.property)
                .flat_map(|e| &e.targets)
            {
                let mut targets = definition
                    .exposed
                    .iter()
                    .find(|e| e.property == path.property)
                    .unwrap()
                    .targets
                    .clone();
                targets.retain(|p| p != target);
                let result = if writable && !imported {
                    operation(state, id, path.property, targets)
                } else {
                    Err("Read-only component".into())
                };
                let mut current = definition;
                let mut names = vec![];
                for reference in &target.references {
                    let node = current
                        .nodes
                        .iter()
                        .find(|n| n.id == *reference)
                        .ok_or("Missing forwarding reference")?;
                    names.push(node.name.clone());
                    let linked = node
                        .reference
                        .as_ref()
                        .ok_or("Missing forwarding definition")?;
                    current = graph
                        .definitions
                        .iter()
                        .find(|d| d.id == linked.definition)
                        .ok_or("Missing forwarding definition")?;
                }
                let node = current
                    .nodes
                    .iter()
                    .find(|n| n.id == target.node)
                    .ok_or("Missing forwarding object")?;
                names.push(node.name.clone());
                let descriptor = crate::canvas::oib::document::descriptors(metadata, node.class)?
                    .into_iter()
                    .find(|p| p.id == target.property)
                    .ok_or("Missing forwarding property")?;
                names.push(descriptor.name);
                let label = format!("Disconnect {}", names.join(" / "));
                choices.push(Choice {
                    label,
                    connected: true,
                    operation: result.as_ref().ok().cloned(),
                    reason: result.err(),
                });
            }
        } else {
            let root_node = definition
                .nodes
                .iter()
                .find(|n| n.id == definition.root)
                .ok_or("Missing API root")?;
            for source in crate::canvas::oib::document::descriptors(metadata, root_node.class)?
                .into_iter()
                .filter(|p| {
                    p.value_type == field.descriptor.value_type
                        && p.policy.live_literal
                        && p.editor != PropertyEditor::ReadOnly
                })
            {
                let mut targets = definition
                    .exposed
                    .iter()
                    .find(|e| e.property == source.id)
                    .map_or_else(Vec::new, |e| e.targets.clone());
                let connected = targets.contains(path);
                if connected {
                    targets.retain(|p| p != path);
                } else {
                    targets.push(path.clone());
                    targets.sort();
                }
                let result = if writable && !imported {
                    operation(state, id, source.id, targets)
                } else {
                    Err("Read-only component".into())
                };
                choices.push(Choice {
                    label: format!(
                        "{} {}",
                        if connected {
                            "Disconnect"
                        } else {
                            "Forward from"
                        },
                        source.name
                    ),
                    connected,
                    operation: result.as_ref().ok().cloned(),
                    reason: result.err(),
                });
            }
        }
    }
    Ok(Some(Model {
        fields,
        selected,
        choices,
        root,
    }))
}

fn button(b: &mut Builder, y: &mut f32, label: &str, action: LocalAction) -> Result<SceneNode> {
    let lines = oib_workspace::wrapped_lines(&[label.into()], 19);
    let mut semantic: String = label
        .chars()
        .scan(0, |bytes, c| {
            *bytes += c.len_utf8();
            (*bytes <= 93).then_some(c)
        })
        .collect();
    if semantic.len() != label.len() {
        semantic.push_str("...");
    }
    let mut row = b.focus_button(0., *y, 320., &semantic, action)?;
    let height = (lines.len() as f32 * 28. + 24.).max(52.);
    row.frame.height = height;
    style::resize_surface(&mut row.children[0], 320., height)?;
    row.children.truncate(1);
    for (index, line) in lines.iter().enumerate() {
        let mut text = b.label(8., 12. + index as f32 * 28., line.trim_end(), WHITE)?;
        text.frame.width = 304.;
        if let Primitive::Text { scale, .. } = &mut text.primitive {
            *scale = 2;
        }
        row.children.push(text);
    }
    *y += height + 4.;
    Ok(row)
}
fn description(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    y: &mut f32,
    label: &str,
) -> Result<()> {
    for line in oib_workspace::wrapped_lines(&[label.into()], 38) {
        nodes.push(b.label(0., *y, line.trim_end(), MUTED)?);
        *y += 28.;
    }
    Ok(())
}
pub(super) fn build(
    b: &mut Builder,
    model: Option<&Model>,
    open: bool,
    y: &mut f32,
) -> Result<Vec<SceneNode>> {
    let mut nodes = vec![button(
        b,
        y,
        if open {
            "v Component API"
        } else {
            "> Component API"
        },
        LocalAction::ApiOpen(!open),
    )?];
    let Some(model) = model.filter(|_| open) else {
        return Ok(nodes);
    };
    if model.fields.is_empty() {
        description(b, &mut nodes, y, "No declared properties")?;
    }
    for field in &model.fields {
        let expanded = model.selected.as_ref() == Some(&field.path);
        nodes.push(button(
            b,
            y,
            &format!(
                "{} {} · {:?}",
                if expanded { "v" } else { ">" },
                field.descriptor.name,
                field.descriptor.value_type
            ),
            LocalAction::ApiProperty(if expanded {
                None
            } else {
                Some(field.path.clone())
            }),
        )?);
        if !expanded {
            continue;
        }
        if model.choices.is_empty() {
            description(
                b,
                &mut nodes,
                y,
                if model.root {
                    "No forwarding targets"
                } else {
                    "No compatible root property"
                },
            )?;
        }
        for choice in &model.choices {
            if let Some(operation) = &choice.operation {
                let mut row = button(
                    b,
                    y,
                    &choice.label,
                    LocalAction::Structure(operation.clone()),
                )?;
                if let Some(semantics) = &mut row.semantics {
                    semantics.checked = choice.connected;
                }
                nodes.push(row);
            } else {
                description(b, &mut nodes, y, &choice.label)?;
                if let Some(reason) = &choice.reason {
                    description(b, &mut nodes, y, reason)?;
                }
            }
        }
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_target_path_retains_visible_text_and_bounds_semantic_label() {
        let label = format!(
            "Disconnect {} / {} / background",
            "Container".repeat(8),
            "スライダー".repeat(4)
        );
        let mut b = Builder::at(FIRST);
        let mut y = 0.;
        let row = button(&mut b, &mut y, &label, LocalAction::ApiOpen(true)).unwrap();
        assert!(row.semantics.as_ref().unwrap().label.len() <= 96);
        assert!(row.semantics.as_ref().unwrap().label.ends_with("..."));
        let visible: String = row
            .children
            .iter()
            .filter_map(|n| match &n.primitive {
                Primitive::Text { value, .. } => Some(value.trim()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(visible.replace(' ', ""), label.replace(' ', ""));
        assert!(row.frame.height > 52.);
        let scene = Scene::new(canvas(0, Rect::new(0., 0., 320., y), vec![row])).unwrap();
        assert_eq!(scene.controls().len(), 1);
    }
}
