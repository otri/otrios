// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded local tools over signed classes and the current authored source.
use crate::canvas::{Result, StateAdapter, oib};
use otrios_interfaces::{
    layout::{Expr, Shape, Value},
    oib::{Allocation, InstanceOperation, Seed, authoring::*, designer::*, graph},
};

const MAX_TOOLS: usize = 8;

fn literal(number: f32) -> Expr {
    Expr::Literal(Value::Number(number))
}
fn visual(shape: Shape, width: f32, height: f32) -> graph::Visual {
    graph::Visual {
        frame: [literal(0.), literal(0.), literal(width), literal(height)],
        resize: None,
        visible: Expr::Literal(Value::Boolean(true)),
        shape,
        semantics: None,
        action: None,
        parameters: vec![],
    }
}
fn node(
    id: u64,
    name: &str,
    class: ClassId,
    owner: Option<NodeId>,
    visual: graph::Visual,
) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: name.into(),
        class,
        owner,
        visual_children: vec![],
        defaults: vec![],
        reference: None,
        visual: Some(visual),
    }
}
fn accepts(classes: &[ClassDescriptor], allowed: &[ClassId], mut class: ClassId) -> bool {
    for _ in 0..=classes.len() {
        if allowed.contains(&class) {
            return true;
        }
        let Some(base) = classes
            .iter()
            .find(|candidate| candidate.id == class)
            .and_then(|candidate| candidate.base)
        else {
            return false;
        };
        class = base;
    }
    false
}
fn references(
    definitions: &[ComponentDefinition],
    start: DefinitionId,
    target: DefinitionId,
    seen: &mut Vec<DefinitionId>,
) -> bool {
    if start == target {
        return true;
    }
    if seen.contains(&start) {
        return false;
    }
    seen.push(start);
    definitions
        .iter()
        .find(|definition| definition.id == start)
        .is_some_and(|definition| {
            definition
                .nodes
                .iter()
                .filter_map(|node| node.reference.as_ref())
                .any(|reference| references(definitions, reference.definition, target, seen))
        })
}

/// Every returned intent captures the observed structural generation. Class
/// restrictions and a pure final-source admission pass filter unavailable tools;
/// the authority still performs its complete coherent-publication checks.
pub(super) fn tools(
    state: &StateAdapter,
    seed: &Seed,
    selection: &Selection,
    allocations: &[Allocation],
) -> Result<Vec<(String, InstanceOperation)>> {
    let graph = seed.graph.as_ref().ok_or("Missing canonical graph")?;
    let generation = state
        .oib_structural_generation()
        .ok_or("Missing structural generation")?;
    let next = state.oib_next_identity()?;
    let mut candidates: Vec<(String, Vec<DocumentEdit>)> = vec![];
    if let Some(class) = graph
        .classes
        .iter()
        .find(|class| class.implementation == ClassKind::O2DPanel)
        && let (Some(root), Some(preset)) = (next.checked_add(1), next.checked_add(2))
    {
        candidates.push((
            "New component".into(),
            vec![
                DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
                    definition: DefinitionId(next),
                    name: "New component".into(),
                    root: node(
                        root,
                        "Panel",
                        class.id,
                        None,
                        visual(Shape::Canvas { clip: true }, 320., 180.),
                    ),
                }),
                DocumentEdit::Preset(PresetEdit::Create {
                    preset: graph::Preset {
                        id: preset,
                        name: "New component".into(),
                        definition: DefinitionId(next),
                        assignments: vec![],
                        frame: None,
                        palette: true,
                    },
                }),
            ],
        ));
    }
    let selected = match selection {
        Selection::Definition(id) => graph
            .definitions
            .iter()
            .find(|definition| definition.id == *id)
            .map(|definition| (definition, definition.root)),
        Selection::DefinitionNode { definition, node } => graph
            .definitions
            .iter()
            .find(|current| current.id == *definition)
            .map(|definition| (definition, *node)),
        _ => None,
    };
    if let Some((definition, selected_node)) = selected {
        let fixed = graph
            .presets
            .iter()
            .any(|preset| preset.definition == definition.id && !preset.palette);
        if !fixed {
            candidates.push((
                "New preset".into(),
                vec![DocumentEdit::Preset(PresetEdit::Create {
                    preset: graph::Preset {
                        id: next,
                        name: "New preset".into(),
                        definition: definition.id,
                        assignments: vec![],
                        frame: None,
                        palette: true,
                    },
                })],
            ));
        }
        if let Some(parent) = definition
            .nodes
            .iter()
            .find(|node| node.id == selected_node && node.reference.is_none())
            && let Some(class) = graph.classes.iter().find(|class| class.id == parent.class)
        {
            for (kind, label, shape, width, height) in [
                (
                    ClassKind::O2DShape,
                    "Add shape",
                    Shape::Superellipse {
                        radius: literal(8.),
                        exponent: 4.,
                        color: Expr::Literal(Value::Color([85, 136, 255, 255])),
                    },
                    96.,
                    48.,
                ),
                (
                    ClassKind::O2DText,
                    "Add text",
                    Shape::Text {
                        value: Expr::Literal(Value::Text("Text".into())),
                        scale: literal(2.),
                        color: Expr::Literal(Value::Color([240, 244, 255, 255])),
                    },
                    128.,
                    28.,
                ),
            ] {
                if let Some(child) = graph.classes.iter().find(|child| {
                    child.implementation == kind
                        && accepts(&graph.classes, &class.owned_child_classes, child.id)
                        && accepts(&graph.classes, &class.visual_child_classes, child.id)
                }) {
                    candidates.push((
                        label.into(),
                        vec![DocumentEdit::Definition(DefinitionEdit::CreateNode {
                            definition: definition.id,
                            node: node(
                                next,
                                label,
                                child.id,
                                Some(parent.id),
                                visual(shape, width, height),
                            ),
                            visual_parent: Some(parent.id),
                            before: None,
                        })],
                    ));
                }
            }
            if let Some(wrapper) = graph.classes.iter().find(|candidate| {
                candidate.implementation == ClassKind::O2DView
                    && accepts(&graph.classes, &class.owned_child_classes, candidate.id)
                    && accepts(&graph.classes, &class.visual_child_classes, candidate.id)
            }) {
                let mut linked_count = 0;
                for linked in graph.definitions.iter().filter(|linked| {
                    !references(&graph.definitions, linked.id, definition.id, &mut vec![])
                }) {
                    let Some(root) = linked
                        .nodes
                        .iter()
                        .find(|node| node.id == linked.root && node.visual.is_some())
                    else {
                        continue;
                    };
                    let mut reference = node(
                        next,
                        "Linked component",
                        wrapper.id,
                        Some(parent.id),
                        root.visual.clone().unwrap(),
                    );
                    // Wrapper geometry is a safe viewport, independent of the
                    // referenced definition's parameter namespace.
                    reference.visual = Some(visual(Shape::Canvas { clip: true }, 160., 100.));
                    reference.reference = Some(ComponentReference {
                        definition: linked.id,
                        assignments: vec![],
                    });
                    candidates.push((
                        format!("Link {}", linked.name),
                        vec![DocumentEdit::Definition(DefinitionEdit::CreateNode {
                            definition: definition.id,
                            node: reference,
                            visual_parent: Some(parent.id),
                            before: None,
                        })],
                    ));
                    linked_count += 1;
                    if linked_count == 2 {
                        break;
                    }
                }
            }
        }
        if selected_node != definition.root
            && seed
                .designer
                .as_ref()
                .is_none_or(|metadata| metadata.connections.is_empty())
        {
            candidates.push((
                "Delete object".into(),
                vec![DocumentEdit::Definition(DefinitionEdit::DeleteNode {
                    definition: definition.id,
                    node: selected_node,
                })],
            ));
        }
        if !fixed
            && !allocations
                .iter()
                .any(|allocation| allocation.component == definition.id.0)
            && !graph.definitions.iter().any(|current| {
                current.nodes.iter().any(|node| {
                    node.reference
                        .as_ref()
                        .is_some_and(|reference| reference.definition == definition.id)
                })
            })
        {
            let mut edits = vec![DocumentEdit::Definition(DefinitionEdit::DeleteDefinition {
                definition: definition.id,
            })];
            edits.extend(
                graph
                    .presets
                    .iter()
                    .filter(|preset| preset.definition == definition.id)
                    .map(|preset| DocumentEdit::Preset(PresetEdit::Delete { preset: preset.id })),
            );
            if edits.len() <= MAX_OPERATIONS {
                candidates.push(("Delete component".into(), edits));
            }
        }
    }
    let mut result = vec![];
    for (label, operations) in candidates {
        let definitions: Vec<_> = operations
            .iter()
            .filter_map(|operation| match operation {
                DocumentEdit::Definition(edit) => Some(edit.clone()),
                _ => None,
            })
            .collect();
        let Ok(definitions) = oib::document::authoring::transform(seed, &definitions) else {
            continue;
        };
        let mut presets = graph.presets.clone();
        for edit in &operations {
            match edit {
                DocumentEdit::Preset(PresetEdit::Create { preset }) => presets.push(preset.clone()),
                DocumentEdit::Preset(PresetEdit::Delete { preset }) => {
                    presets.retain(|current| current.id != *preset)
                }
                _ => {}
            }
        }
        presets.sort_by_key(|preset| preset.id);
        if let Ok(candidate) = oib::graph::with_seed_source(seed, &definitions, &presets)
            && oib::instances::expanded_seed(&candidate, allocations).is_ok()
        {
            let batch = EditBatch {
                generation,
                operations,
            };
            if flexbuffers::to_vec(&batch)
                .map_err(|e| e.to_string())?
                .len()
                <= MAX_BATCH_BYTES
            {
                result.push((label, batch));
            }
        }
        if result.len() == MAX_TOOLS {
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
use super::tests::designer_fixture as fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{self, Document, Session, StateSelection};
    use otrios_interfaces::canvas::{StateRequest, Ui};

    fn setup(writer: u8) -> (StateAdapter, Document, Session) {
        assert_eq!(fixture::manifest().id, "oib-designer");
        let ui = fixture::ui(&[]);
        let mut document = Document::new(ui.state_schema.clone(), "d2:visible-tools").unwrap();
        document.configure_oib(&ui).unwrap();
        let (state, session) = connect(&ui, &mut document, writer);
        (state, document, session)
    }
    fn connect(ui: &Ui, document: &mut Document, writer: u8) -> (StateAdapter, Session) {
        let (mut session, _) = document.open_session(&[writer; 16], 0).unwrap();
        session.set_oib_editable(true);
        session
            .set_selection(StateSelection::from_ui(ui).unwrap())
            .unwrap();
        let mut state = StateAdapter::for_ui(ui, "d2:visible-tools", &[], [writer; 16]).unwrap();
        state
            .receive(&document.generate(&mut session).unwrap().unwrap())
            .unwrap();
        for _ in 0..20 {
            let mut progress = false;
            while let Some(request) = state.generate().unwrap() {
                document.receive(&mut session, &request).unwrap();
                progress = true;
            }
            if let Some(update) = document.generate(&mut session).unwrap() {
                state.receive(&update).unwrap();
                progress = true;
            }
            if !progress {
                return (state, session);
            }
        }
        panic!("fixture failed to settle");
    }
    fn accept(
        document: &mut Document,
        session: &mut Session,
        sequence: u64,
        operation: InstanceOperation,
    ) {
        let before = document.oib_status().unwrap().structural_generation;
        document
            .receive(
                session,
                &canvas::encode_state_request(&StateRequest::OibInstance {
                    epoch: document.oib_epoch().unwrap(),
                    sequence,
                    operation,
                })
                .unwrap(),
            )
            .unwrap();
        assert!(
            document.oib_status().unwrap().error.is_none(),
            "{:?}",
            document.oib_status().unwrap().error
        );
        assert_eq!(
            document.oib_status().unwrap().structural_generation,
            before + 1
        );
    }
    #[test]
    fn generated_tools_are_accepted_for_document_definition_and_leaf_selection() {
        let (state, document, _) = setup(61);
        let seed = state.oib_authoring_seed().unwrap();
        let allocations = document.oib_allocations().unwrap();
        let mut labels = vec![];
        for selection in [
            Selection::Document(seed.graph.as_ref().unwrap().id),
            Selection::Definition(DefinitionId(10)),
            Selection::DefinitionNode {
                definition: DefinitionId(10),
                node: NodeId(3),
            },
        ] {
            for (label, operation) in tools(&state, &seed, &selection, &allocations).unwrap() {
                assert_eq!(operation.generation, 1);
                let (mut client, mut authority, mut session) = setup(62);
                assert!(
                    client.edit_oib_structure(operation).unwrap(),
                    "{label} was hidden by stale capacity preflight"
                );
                authority
                    .receive(&mut session, &client.generate().unwrap().unwrap())
                    .unwrap();
                assert!(
                    authority.oib_status().unwrap().error.is_none(),
                    "{label}: {:?}",
                    authority.oib_status().unwrap().error
                );
                assert_eq!(
                    authority.oib_status().unwrap().structural_generation,
                    2,
                    "{label}"
                );
                labels.push(label);
            }
        }
        for label in [
            "New component",
            "New preset",
            "Add shape",
            "Add text",
            "Delete object",
        ] {
            assert!(labels.iter().any(|entry| entry == label), "missing {label}");
        }
        assert!(labels.iter().any(|label| label.starts_with("Link ")));
    }
    #[test]
    fn a_created_component_can_gain_a_preset_and_be_deleted_through_generated_tools() {
        let (state, mut document, mut session) = setup(63);
        let definition = DefinitionId(state.oib_next_identity().unwrap());
        let seed = state.oib_authoring_seed().unwrap();
        let operation = tools(
            &state,
            &seed,
            &Selection::Document(seed.graph.as_ref().unwrap().id),
            &document.oib_allocations().unwrap(),
        )
        .unwrap()
        .into_iter()
        .find(|(label, _)| label == "New component")
        .unwrap()
        .1;
        accept(&mut document, &mut session, 1, operation);
        let ui = fixture::ui(&[]);
        let (state, _) = connect(&ui, &mut document, 64);
        let operation = tools(
            &state,
            &state.oib_authoring_seed().unwrap(),
            &Selection::Definition(definition),
            &document.oib_allocations().unwrap(),
        )
        .unwrap()
        .into_iter()
        .find(|(label, _)| label == "New preset")
        .unwrap()
        .1;
        accept(&mut document, &mut session, 2, operation);
        let (state, _) = connect(&ui, &mut document, 65);
        let operation = tools(
            &state,
            &state.oib_authoring_seed().unwrap(),
            &Selection::Definition(definition),
            &document.oib_allocations().unwrap(),
        )
        .unwrap()
        .into_iter()
        .find(|(label, _)| label == "Delete component")
        .unwrap()
        .1;
        accept(&mut document, &mut session, 3, operation);
        assert!(
            !document
                .oib_compiled(2)
                .unwrap()
                .0
                .presentation
                .as_ref()
                .and_then(|presentation| match presentation {
                    otrios_interfaces::canvas::Presentation::Oib(seed) => seed.graph.as_ref(),
                    _ => None,
                })
                .unwrap()
                .definitions
                .iter()
                .any(|current| current.id == definition)
        );
    }
}
