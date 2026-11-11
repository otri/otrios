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
        properties: vec![],
        next_property: 0,
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

fn endpoint_descriptor(id: u64, name: &str) -> PropertyDescriptor {
    PropertyDescriptor {
        id: PropertyId(id),
        name: name.into(),
        label: name.into(),
        value_type: otrios_interfaces::layout::ValueType::Text,
        default: Value::Text("Default".into()),
        number_range: None,
        number_step: None,
        units: None,
        group: InspectorGroup::Appearance,
        editor: PropertyEditor::TextSuggestions,
        policy: PropertyPolicy {
            live_literal: true,
            app_binding: false,
        },
    }
}
fn endpoint_path(references: &[u64], node: u64, property: u64) -> PropertyPath {
    let mut path = path(references, node);
    path.property = PropertyId(property);
    path
}

#[test]
fn removed_endpoint_restores_fresh_property_and_exact_terminal_paths_and_values() {
    let mut owner = definition(10, 11);
    owner.properties = vec![
        endpoint_descriptor(2, "caption"),
        endpoint_descriptor(4, "heading"),
    ];
    owner.next_property = 5;
    owner.nodes[0].defaults.push(PropertyDefault {
        property: PropertyId(2),
        value: Value::Text("Fallback".into()),
    });
    owner.nodes.push(node(12, Some(11)));
    owner.exposed.push(ExposedProperty {
        property: PropertyId(2),
        targets: vec![endpoint_path(&[], 12, 2)],
    });
    let mut host = definition(30, 31);
    let mut reference = node(32, Some(31));
    reference.reference = Some(ComponentReference {
        definition: DefinitionId(10),
        assignments: vec![],
    });
    host.nodes.push(reference);
    let mut before = source(vec![owner, host], vec![]);
    before.instances = vec![
        InstanceRecord {
            id: 50,
            name: "Direct".into(),
            component: 10,
            prototype: 40,
        },
        InstanceRecord {
            id: 70,
            name: "Nested".into(),
            component: 30,
            prototype: 60,
        },
    ];
    let mut after = before.clone();
    after.definitions[0]
        .properties
        .retain(|p| p.id != PropertyId(2));
    after.definitions[0].nodes[0].defaults.clear();
    after.definitions[0].exposed.clear();
    let definition_value = PropertyAddress::Definition {
        definition: DefinitionId(10),
        node: NodeId(11),
        property: PropertyId(2),
    };
    let direct = PropertyAddress::Occurrence {
        instance: InstanceId(50),
        path: endpoint_path(&[], 11, 2),
    };
    let nested = PropertyAddress::Occurrence {
        instance: InstanceId(70),
        path: endpoint_path(&[32], 11, 2),
    };
    let unrelated = PropertyAddress::Definition {
        definition: DefinitionId(10),
        node: NodeId(12),
        property: PropertyId(2),
    };
    let properties = [definition_value, direct, nested, unrelated]
        .into_iter()
        .enumerate()
        .map(|(index, address)| (address, vec![Some(Value::Text(format!("Value {index}")))]))
        .collect::<Vec<_>>();
    let graph = Graph::capture(&before, &after, &properties).unwrap();
    let inverse = vec![
        DocumentEdit::Definition(DefinitionEdit::Expose {
            definition: DefinitionId(30),
            property: PropertyId(1),
            targets: vec![endpoint_path(&[32], 11, 2)],
        }),
        DocumentEdit::Definition(DefinitionEdit::SetDefault {
            definition: DefinitionId(10),
            node: NodeId(12),
            property: PropertyId(2),
            value: Value::Text("Child".into()),
        }),
        DocumentEdit::Definition(DefinitionEdit::ConnectReference {
            definition: DefinitionId(30),
            reference: NodeId(32),
            target: endpoint_path(&[], 11, 2),
            source: Some(LiteralSource::Forward(endpoint_path(&[], 31, 1))),
        }),
    ];
    let operations = graph.operations(&after, &inverse).unwrap();
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::SetProperty { definition: DefinitionId(10), descriptor }) if descriptor.id == PropertyId(5) && descriptor.name == "caption")));
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::SetDefault {node:NodeId(11),property:PropertyId(5),value,..}) if value == &Value::Text("Fallback".into()))));
    assert!(operations.iter().any(|op| matches!(
        op,
        DocumentEdit::Definition(DefinitionEdit::SetDefault {
            node: NodeId(12),
            property: PropertyId(2),
            ..
        })
    )));
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::Expose {definition:DefinitionId(10),property:PropertyId(5),targets}) if targets == &vec![endpoint_path(&[],12,2)])));
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::Expose {definition:DefinitionId(30),targets,..}) if targets == &vec![endpoint_path(&[32],11,5)])));
    assert!(operations.iter().any(|op| matches!(op, DocumentEdit::Definition(DefinitionEdit::ConnectReference {target,source,..}) if target == &endpoint_path(&[],11,5) && source == &Some(LiteralSource::Forward(endpoint_path(&[],31,1))))));
    let values = graph.values(&after).unwrap();
    assert_eq!(
        values.len(),
        3,
        "same numeric property ID on an unrelated child must not be captured"
    );
    for (address, _) in &values {
        assert!(matches!(
            address,
            PropertyAddress::Definition {
                property: PropertyId(5),
                ..
            } | PropertyAddress::Occurrence {
                path: PropertyPath {
                    property: PropertyId(5),
                    ..
                },
                ..
            }
        ));
    }
    assert!(operations.len() + values.len() <= MAX_OPERATIONS);
}

#[test]
fn whole_definition_restore_initializes_sparse_property_namespace_in_one_creation() {
    let mut old = definition(10, 11);
    old.properties = vec![
        endpoint_descriptor(2, "caption"),
        endpoint_descriptor(4, "heading"),
    ];
    old.next_property = 6;
    let before = source(vec![old.clone()], vec![]);
    let after = source(vec![], vec![]);
    let graph = Graph::capture(&before, &after, &[]).unwrap();
    let operations = graph.operations(&after, &[]).unwrap();
    assert_eq!(operations.len(), 1);
    assert!(
        matches!(&operations[0],DocumentEdit::Definition(DefinitionEdit::CreateDefinition {definition:DefinitionId(100),properties,next_property:6,..}) if properties == &old.properties)
    );
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
