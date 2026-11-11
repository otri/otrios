// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

fn node(id: u64, owner: Option<u64>) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: format!("Node {id}"),
        class: ClassId(1),
        owner: owner.map(NodeId),
        visual_children: vec![],
        defaults: vec![],
        reference: None,
        visual: None,
    }
}
fn definition(id: u64, root: u64) -> ComponentDefinition {
    ComponentDefinition {
        id: DefinitionId(id),
        name: format!("Component {id}"),
        root: NodeId(root),
        nodes: vec![node(root, None)],
        exposed: vec![],
        slots: vec![],
    }
}
fn source(definitions: Vec<ComponentDefinition>, presets: Vec<Preset>) -> DocumentSource {
    DocumentSource {
        definitions,
        presets,
        instances: vec![],
        next_identity: 100,
        baseline: vec![],
        placements: vec![],
        connections: vec![],
    }
}
fn path(references: &[u64], node: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
        property: PropertyId(1),
    }
}

#[test]
fn fresh_graph_remaps_reference_forward_exposed_slot_and_preset_paths() {
    let mut first = definition(10, 11);
    first.nodes[0].visual_children = vec![NodeId(12)];
    let mut reference = node(12, Some(11));
    reference.reference = Some(ComponentReference {
        definition: DefinitionId(20),
        assignments: vec![Assignment {
            target: path(&[], 21),
            source: AssignmentSource::Forward(path(&[], 11)),
        }],
    });
    first.nodes.push(reference);
    first.exposed.push(ExposedProperty {
        property: PropertyId(1),
        targets: vec![path(&[12], 21)],
    });
    first.slots.push(SlotDescriptor {
        id: SlotId(30),
        name: "Content".into(),
        references: vec![],
        node: NodeId(11),
        admitted_classes: vec![ClassId(1)],
        cardinality: [0, 4],
        layout: SlotLayout::Absolute,
    });
    let preset = Preset {
        id: 40,
        name: "Insert".into(),
        definition: DefinitionId(10),
        assignments: vec![otrios_interfaces::oib::graph::Assignment {
            target: path(&[12], 21),
            source: otrios_interfaces::oib::graph::Source::Forward(path(&[], 11)),
        }],
        frame: None,
        palette: true,
    };
    let before = source(vec![first, definition(20, 21)], vec![preset]);
    let after = source(vec![], vec![]);
    let graph = Graph::capture(&before, &after, &[]).unwrap();
    let operations = graph.operations(&after, &[]).unwrap();
    assert!(operations.len() <= MAX_OPERATIONS);
    assert!(encoded(&operations).unwrap().len() <= MAX_BATCH_BYTES);
    let roots: Vec<_> = operations
        .iter()
        .filter_map(|op| match op {
            DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
                definition, root, ..
            }) => Some((*definition, root.id)),
            _ => None,
        })
        .collect();
    assert_eq!(
        roots,
        vec![
            (DefinitionId(100), NodeId(102)),
            (DefinitionId(101), NodeId(104))
        ]
    );
    let reference = operations
        .iter()
        .find_map(|op| match op {
            DocumentEdit::Definition(DefinitionEdit::CreateNode { node, .. }) => {
                node.reference.as_ref()
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(reference.definition, DefinitionId(101));
    assert_eq!(reference.assignments[0].target, path(&[], 104));
    assert_eq!(
        reference.assignments[0].source,
        AssignmentSource::Forward(path(&[], 102))
    );
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::Expose { definition: DefinitionId(100), targets, .. }) if targets == &vec![path(&[103],104)])));
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::SetSlots { slots, .. }) if slots[0].id == SlotId(105) && slots[0].node == NodeId(102))));
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Preset(PresetEdit::Create { preset }) if preset.id == 106 && preset.definition == DefinitionId(100) && preset.assignments[0].target == path(&[103],104))));
    assert_eq!(
        graph
            .selection(
                &after,
                &Selection::DefinitionNode {
                    definition: DefinitionId(10),
                    node: NodeId(12)
                }
            )
            .unwrap(),
        Selection::DefinitionNode {
            definition: DefinitionId(100),
            node: NodeId(103)
        }
    );
}

#[test]
fn fresh_graph_uses_before_occurrence_origin_after_replacement_and_preserves_conflict_gate() {
    let removed = definition(10, 11);
    let replacement = definition(20, 21);
    let preset = Preset {
        id: 40,
        name: "Insert".into(),
        definition: DefinitionId(10),
        assignments: vec![],
        frame: None,
        palette: true,
    };
    let mut before = source(vec![removed, replacement.clone()], vec![preset.clone()]);
    before.instances.push(InstanceRecord {
        id: 50,
        name: "Occurrence".into(),
        component: 10,
        prototype: 40,
    });
    let mut after = source(
        vec![replacement],
        vec![Preset {
            definition: DefinitionId(20),
            ..preset.clone()
        }],
    );
    after.instances.push(InstanceRecord {
        id: 50,
        name: "Occurrence".into(),
        component: 20,
        prototype: 40,
    });
    let address = PropertyAddress::Occurrence {
        instance: InstanceId(50),
        path: path(&[], 11),
    };
    let value = Some(Value::Color([1, 2, 3, 255]));
    let graph = Graph::capture(&before, &after, &[(address.clone(), vec![value.clone()])]).unwrap();
    assert!(encoded(&graph).unwrap().len() <= MAX_HISTORY_BYTES);
    assert!(graph.guard_keys().contains(&Key::Instance(InstanceId(50))));
    assert_eq!(
        graph.values(&after).unwrap(),
        vec![(
            PropertyAddress::Occurrence {
                instance: InstanceId(50),
                path: path(&[], 101)
            },
            value
        )]
    );
    assert_eq!(
        graph
            .selection(
                &after,
                &Selection::Object(ObjectAddress {
                    instance: InstanceId(50),
                    references: vec![],
                    node: NodeId(11)
                })
            )
            .unwrap(),
        Selection::Object(ObjectAddress {
            instance: InstanceId(50),
            references: vec![],
            node: NodeId(101)
        })
    );
    let operations = graph
        .operations(
            &after,
            &[DocumentEdit::Preset(PresetEdit::Update { preset })],
        )
        .unwrap();
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Preset(PresetEdit::Update { preset }) if preset.id == 40 && preset.definition == DefinitionId(100))));
    assert!(
        Graph::capture(
            &before,
            &after,
            &[(address, vec![None, Some(Value::Color([0, 0, 0, 255]))])]
        )
        .unwrap_err()
        .contains("causal conflict")
    );
}

#[test]
fn fresh_node_restoration_orders_ownership_separately_from_visual_siblings() {
    let mut d = definition(10, 11);
    d.nodes[0].visual_children = vec![NodeId(12), NodeId(13), NodeId(14)];
    d.nodes
        .extend([node(12, Some(11)), node(13, Some(12)), node(14, Some(11))]);
    let before = source(vec![d.clone()], vec![]);
    d.nodes.retain(|n| n.id != NodeId(12) && n.id != NodeId(13));
    d.nodes[0].visual_children = vec![NodeId(14)];
    let after = source(vec![d], vec![]);
    let graph = Graph::capture(&before, &after, &[]).unwrap();
    let operations = graph.operations(&after, &[]).unwrap();
    let created: Vec<_> = operations
        .iter()
        .filter_map(|op| match op {
            DocumentEdit::Definition(DefinitionEdit::CreateNode { node, .. }) => {
                Some((node.id, node.owner))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        created,
        vec![
            (NodeId(100), Some(NodeId(11))),
            (NodeId(101), Some(NodeId(100)))
        ]
    );
    let moves: Vec<_> = operations
        .iter()
        .filter_map(|op| match op {
            DocumentEdit::Definition(DefinitionEdit::MoveNode { node, before, .. }) => {
                Some((*node, *before))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        moves,
        vec![
            (NodeId(101), Some(NodeId(14))),
            (NodeId(100), Some(NodeId(101)))
        ]
    );
}
