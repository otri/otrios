// SPDX-License-Identifier: LGPL-3.0-only
//! Mapping/budget proofs; authority acceptance is qualified by client fixtures.
use super::*;

fn definition() -> ComponentDefinition {
    ComponentDefinition {
        id: DefinitionId(10),
        name: "Panel".into(),
        root: NodeId(11),
        nodes: vec![DefinitionNode {
            id: NodeId(11),
            name: "Root".into(),
            class: ClassId(1),
            owner: None,
            visual_children: vec![],
            defaults: vec![],
            reference: None,
            visual: None,
        }],
        properties: vec![],
        next_property: 0,
        exposed: vec![],
        slots: vec![],
    }
}
fn source() -> DocumentSource {
    DocumentSource {
        styles: None,
        definitions: vec![definition()],
        presets: vec![Preset {
            id: 40,
            name: "Panel preset".into(),
            definition: DefinitionId(10),
            assignments: vec![],
            frame: None,
            palette: true,
        }],
        instances: vec![],
        next_identity: 100,
        baseline: vec![],
        placements: vec![],
        connections: vec![],
    }
}
fn instance(id: u64, name: &str) -> InstanceRecord {
    InstanceRecord {
        id,
        name: name.into(),
        component: 10,
        prototype: 40,
    }
}
fn object(id: u64) -> ObjectAddress {
    ObjectAddress {
        instance: InstanceId(id),
        references: vec![],
        node: NodeId(11),
    }
}
fn slot(owner: u64) -> Destination {
    Destination::Slot {
        component: object(owner),
        slot: SlotId(5),
    }
}
fn address(id: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(id),
        path: PropertyPath {
            references: vec![],
            node: NodeId(11),
            property: PropertyId(1),
        },
    }
}

#[test]
fn removed_occurrence_closure_maps_parent_before_older_child_and_keeps_owned_state() {
    let mut before = source();
    // Durable source order and numeric identity order both put the child before
    // its later-created owner. Fresh allocation must follow insertion topology.
    before.instances = vec![
        instance(20, "Child"),
        instance(70, "Unchanged"),
        instance(50, "Parent"),
        instance(30, "Grandchild"),
    ];
    before.placements = vec![
        Placement {
            instance: InstanceId(20),
            destination: slot(50),
            frame: Some([4., 8., 100., 50.]),
        },
        Placement {
            instance: InstanceId(30),
            destination: slot(20),
            frame: None,
        },
        Placement {
            instance: InstanceId(50),
            destination: Destination::Canvas,
            frame: Some([10., 20., 320., 180.]),
        },
    ];
    before.connections.push(AppConnection {
        object: object(20),
        property: PropertyId(2),
        source: ConnectionSource {
            field: 2,
            projection: otrios_interfaces::canvas::Projection::Identity,
        },
    });
    let mut after = source();
    after.instances = vec![instance(70, "Unchanged")];
    let value = Some(Value::Text("Retained label".into()));
    let graph = Graph::capture(&before, &after, &[(address(20), vec![value.clone()])]).unwrap();
    assert!(encoded(&graph).unwrap().len() <= MAX_HISTORY_BYTES);
    let operations = graph.operations(&after, &[]).unwrap();
    let inserts: Vec<_> = operations
        .iter()
        .filter_map(|operation| match operation {
            DocumentEdit::Hierarchy(HierarchyEdit::Insert {
                name,
                destination,
                frame,
                ..
            }) => Some((name.clone().unwrap(), destination.clone(), *frame)),
            _ => None,
        })
        .collect();
    assert_eq!(
        inserts
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["Parent", "Child", "Grandchild"]
    );
    assert_eq!(inserts[1].1, slot(100));
    assert_eq!(inserts[2].1, slot(101));
    assert_eq!(inserts[1].2, Some([4., 8., 100., 50.]));
    assert_eq!(graph.values(&after).unwrap(), vec![(address(101), value)]);
    assert_eq!(
        graph
            .selection(&after, &Selection::Object(object(20)))
            .unwrap(),
        Selection::Object(object(101))
    );
    assert!(operations.iter().any(|operation|matches!(operation,DocumentEdit::Hierarchy(HierarchyEdit::Connect {object,property:PropertyId(2),source:Some(source)}) if object.instance==InstanceId(101)&&source.field==2)));
    assert!(operations.len() <= MAX_OPERATIONS);
    assert!(encoded(&operations).unwrap().len() <= MAX_BATCH_BYTES);
}

#[test]
fn removed_occurrence_combines_graph_and_implicit_ids_in_one_fresh_interval() {
    let mut before = source();
    before.instances = vec![instance(50, "Parent")];
    let mut after = source();
    after.definitions.clear();
    after.presets.clear();
    let value = Some(Value::Text("Retained".into()));
    let graph = Graph::capture(&before, &after, &[(address(50), vec![value.clone()])]).unwrap();
    let operations = graph.operations(&after, &[]).unwrap();
    assert!(operations.iter().any(|operation|matches!(operation,DocumentEdit::Definition(DefinitionEdit::CreateDefinition {definition:DefinitionId(100),root,..}) if root.id==NodeId(101))));
    assert!(operations.iter().any(|operation|matches!(operation,DocumentEdit::Preset(PresetEdit::Create {preset}) if preset.id==102&&preset.definition==DefinitionId(100))));
    assert!(operations.iter().any(|operation| matches!(
        operation,
        DocumentEdit::Hierarchy(HierarchyEdit::Insert {
            prototype: InstanceId(102),
            ..
        })
    )));
    assert_eq!(
        graph.values(&after).unwrap(),
        vec![(
            PropertyAddress::Occurrence {
                instance: InstanceId(103),
                path: PropertyPath {
                    references: vec![],
                    node: NodeId(101),
                    property: PropertyId(1)
                }
            },
            value
        )]
    );
}

#[test]
fn removed_occurrence_history_rejects_changed_siblings_and_unavailable_presets() {
    let mut before = source();
    before.instances = vec![instance(20, "Removed"), instance(70, "Unchanged")];
    let mut after = source();
    after.instances = vec![instance(70, "Unchanged")];
    let apply = Apply {
        epoch: vec![1; 32],
        baseline: vec![2; 32],
        generation: 1,
        operations: vec![DocumentEdit::Hierarchy(HierarchyEdit::Remove {
            instance: InstanceId(20),
        })],
        changes: vec![],
    };
    let receipt = ApplyReceipt {
        sequence: 1,
        digest: hash(&encoded(&("oib-apply-v1", &apply)).unwrap()),
        result: ApplyResult::Accepted {
            generation: 2,
            structural_generation: 2,
        },
        property_versions: vec![],
    };
    let entry = Entry::capture_with_properties(&before, &after, &apply, &receipt, &[]).unwrap();
    let base = Apply {
        generation: 2,
        operations: vec![],
        ..apply.clone()
    };
    assert!(entry.plan(&after, 2, base.clone(), &[]).is_ok());
    after.instances.push(instance(90, "New sibling"));
    assert!(!entry.review(&after, 3, &[]).unwrap().is_empty());
    assert!(entry.plan(&after, 3, base, &[]).is_err());
    before.presets.clear();
    assert!(
        Graph::capture(&before, &after, &[])
            .unwrap_err()
            .contains("preset")
    );
}

#[test]
fn removed_occurrence_capture_preserves_conflict_and_ownership_cycle_gates() {
    let mut before = source();
    before.instances = vec![instance(20, "One"), instance(50, "Two")];
    let after = source();
    assert!(
        Graph::capture(
            &before,
            &after,
            &[(
                address(20),
                vec![None, Some(Value::Text("Conflict".into()))]
            )]
        )
        .unwrap_err()
        .contains("conflict")
    );
    before.placements = vec![
        Placement {
            instance: InstanceId(20),
            destination: slot(50),
            frame: None,
        },
        Placement {
            instance: InstanceId(50),
            destination: slot(20),
            frame: None,
        },
    ];
    let graph = Graph::capture(&before, &after, &[]).unwrap();
    assert!(
        graph
            .operations(&after, &[])
            .unwrap_err()
            .contains("cyclic")
    );
}

#[test]
fn removed_occurrence_unrelated_retired_selection_does_not_veto_restoration() {
    let mut before = source();
    before.instances.push(instance(20, "Restored"));
    let current = source();
    let graph = Graph::capture(&before, &current, &[]).unwrap();
    let selection = Selection::Object(object(999));
    assert_eq!(graph.selection(&current, &selection).unwrap(), selection);
    assert!(
        graph
            .operations(&current, &[])
            .unwrap()
            .iter()
            .any(|operation| matches!(
                operation,
                DocumentEdit::Hierarchy(HierarchyEdit::Insert { .. })
            ))
    );
}
