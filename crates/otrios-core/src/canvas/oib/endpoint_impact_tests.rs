// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::{
    layout::Value,
    oib::authoring::{AppConnection, ConnectionSource, InstanceRecord},
};

#[path = "../../../tests/support/oib_library_fixture.rs"]
#[allow(dead_code)]
mod library_fixture;

fn source(seed: &Seed) -> DocumentSource {
    let graph = seed.graph.as_ref().unwrap();
    DocumentSource {
        styles: None,
        definitions: graph.definitions.clone(),
        presets: graph.presets.clone(),
        baseline: vec![],
        next_identity: 300,
        placements: vec![],
        connections: vec![],
        instances: graph
            .instances
            .iter()
            .map(|i| {
                let preset = graph.presets.iter().find(|p| p.id == i.preset).unwrap();
                InstanceRecord {
                    id: i.id.0,
                    name: i.name.clone(),
                    component: preset.definition.0,
                    prototype: i.preset,
                }
            })
            .collect(),
    }
}
fn path(references: &[u64], node: u64, property: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
        property: PropertyId(property),
    }
}
fn sync(seed: &mut Seed, source: &DocumentSource) {
    let graph = seed.graph.as_mut().unwrap();
    graph.definitions = source.definitions.clone();
    graph.presets = source.presets.clone();
}
fn fixture() -> (Seed, DocumentSource) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    let mut seed = super::super::seed(&ui).unwrap().clone();
    let graph = seed.graph.as_mut().unwrap();
    let mut descriptor = graph
        .classes
        .iter()
        .flat_map(|c| &c.properties)
        .find(|p| p.value_type == otrios_interfaces::layout::ValueType::Text)
        .unwrap()
        .clone();
    descriptor.id = PropertyId(2);
    descriptor.name = "caption".into();
    descriptor.label = "Caption".into();
    let definition = graph
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    definition.properties.push(descriptor);
    definition.next_property = 3;
    let source = source(&seed);
    (seed, source)
}
#[test]
fn own_fallback_and_outgoing_routes_are_owned_removals_even_with_live_instances() {
    let (mut seed, mut source) = fixture();
    let def = source
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    def.nodes
        .iter_mut()
        .find(|n| n.id == def.root)
        .unwrap()
        .defaults
        .push(PropertyDefault {
            property: PropertyId(2),
            value: Value::Text("Default caption".into()),
        });
    def.exposed.push(ExposedProperty {
        property: PropertyId(2),
        targets: vec![path(&[7], 1, 3)],
    });
    sync(&mut seed, &source);
    let report = endpoint(&seed, &source, DefinitionId(10), PropertyId(2));
    assert!(report.can_remove(), "{:?}", report.blockers);
    assert!(source.instances.iter().any(|i| i.component == 10));
    assert!(
        report
            .items
            .iter()
            .any(|i| i.detail.contains("fallback") && !i.blocking)
    );
    assert!(
        report
            .items
            .iter()
            .any(|i| i.detail.contains("outgoing") && !i.blocking)
    );
    assert!(report.items.iter().all(|i| i.selection.is_some()));
    assert!(report.scope.contains("other documents are not checked"));
}
#[test]
fn another_endpoint_forwarding_into_the_removed_endpoint_blocks_with_named_use() {
    let (mut seed, mut source) = fixture();
    let def = source
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let mut descriptor = def.properties[0].clone();
    descriptor.id = PropertyId(3);
    descriptor.name = "heading".into();
    def.properties.push(descriptor);
    def.next_property = 4;
    def.exposed.push(ExposedProperty {
        property: PropertyId(3),
        targets: vec![path(&[], def.root.0, 2)],
    });
    sync(&mut seed, &source);
    let report = endpoint(&seed, &source, DefinitionId(10), PropertyId(2));
    assert!(report.complete, "{:?}", report.blockers);
    assert!(!report.can_remove());
    assert!(report.items.iter().any(|i| i.blocking
        && i.detail.contains("Another endpoint")
        && i.selection == Some(Selection::Definition(DefinitionId(10)))));
}
#[test]
fn reference_forward_source_and_visual_parameters_are_real_endpoint_uses() {
    let (mut seed, mut source) = fixture();
    let def = source
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let root = def.root;
    let reference = def.nodes.iter_mut().find(|n| n.id == NodeId(7)).unwrap();
    reference
        .reference
        .as_mut()
        .unwrap()
        .assignments
        .push(Assignment {
            target: path(&[], 1, 3),
            source: AssignmentSource::Forward(path(&[], root.0, 2)),
        });
    reference
        .visual
        .as_mut()
        .unwrap()
        .parameters
        .push(graph::Parameter {
            local: 15,
            source: ParameterSource::Property(path(&[], root.0, 2)),
            projection: otrios_interfaces::canvas::Projection::Identity,
        });
    sync(&mut seed, &source);
    let report = endpoint(&seed, &source, DefinitionId(10), PropertyId(2));
    assert!(report.complete, "{:?}", report.blockers);
    assert!(
        report
            .items
            .iter()
            .any(|i| i.blocking && i.detail.contains("forwards from"))
    );
    assert!(
        report
            .items
            .iter()
            .any(|i| i.blocking && i.detail.contains("Visual parameter"))
    );
    assert!(!report.can_remove());
}
#[test]
fn reference_target_preset_and_explicit_connection_resolve_the_terminal_definition() {
    let (mut seed, mut source) = fixture();
    let mut reference = source
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == NodeId(7))
        .unwrap()
        .clone();
    reference.id = NodeId(201);
    reference.name = "Linked settings".into();
    reference.owner = None;
    reference.reference = Some(ComponentReference {
        definition: DefinitionId(10),
        assignments: vec![Assignment {
            target: path(&[], 1, 2),
            source: AssignmentSource::Literal(Value::Text("Assigned".into())),
        }],
    });
    source.definitions.push(ComponentDefinition {
        id: DefinitionId(200),
        name: "Reference host".into(),
        root: reference.id,
        nodes: vec![reference],
        properties: vec![],
        next_property: 0,
        exposed: vec![],
        slots: vec![],
    });
    let preset = source
        .presets
        .iter_mut()
        .find(|p| p.definition == DefinitionId(10))
        .unwrap();
    preset.assignments.push(graph::Assignment {
        target: path(&[], 1, 2),
        source: graph::Source::Literal(Value::Text("Preset caption".into())),
    });
    let instance = source.instances.iter().find(|i| i.component == 10).unwrap();
    source.connections.push(AppConnection {
        object: ObjectAddress {
            instance: InstanceId(instance.id),
            references: vec![],
            node: NodeId(1),
        },
        property: PropertyId(2),
        source: ConnectionSource {
            field: 1,
            projection: otrios_interfaces::canvas::Projection::Identity,
        },
    });
    sync(&mut seed, &source);
    let report = endpoint(&seed, &source, DefinitionId(10), PropertyId(2));
    assert!(report.complete, "{:?}", report.blockers);
    for detail in [
        "Reference assignment targets",
        "Preset assigns",
        "App connection targets",
    ] {
        assert!(
            report
                .items
                .iter()
                .any(|i| i.blocking && i.detail.contains(detail)),
            "Missing use {detail}"
        );
    }
}
#[test]
fn missing_inherited_ambiguous_and_malformed_paths_do_not_prove_no_uses() {
    let (seed, source) = fixture();
    assert!(endpoint(&seed, &source, DefinitionId(10), PropertyId(1)).readonly);
    assert!(!endpoint(&seed, &source, DefinitionId(10), PropertyId(99)).complete);
    let mut malformed = source.clone();
    malformed.definitions[0].name.push_str(" changed");
    assert!(!endpoint(&seed, &malformed, DefinitionId(10), PropertyId(2)).can_remove());
    let mut malformed = source.clone();
    let def = malformed
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    def.exposed.push(ExposedProperty {
        property: PropertyId(2),
        targets: vec![path(&[999], 1, 3)],
    });
    let mut seed = seed.clone();
    sync(&mut seed, &malformed);
    let report = endpoint(&seed, &malformed, DefinitionId(10), PropertyId(2));
    assert!(!report.complete);
    assert!(
        report
            .items
            .iter()
            .any(|i| i.kind == Kind::Unknown && i.blocking)
    );
    malformed.definitions.push(malformed.definitions[0].clone());
    sync(&mut seed, &malformed);
    assert!(!endpoint(&seed, &malformed, DefinitionId(10), PropertyId(2)).complete);
}
#[test]
fn exact_imported_declaration_is_readonly_and_a_missing_body_is_incomplete() {
    let ui = library_fixture::update(&library_fixture::imported(), |library| {
        let mut descriptor = library
            .required_classes
            .iter()
            .flat_map(|c| &c.properties)
            .find(|p| p.value_type == otrios_interfaces::layout::ValueType::Text)
            .unwrap()
            .clone();
        descriptor.id = PropertyId(5);
        descriptor.name = "caption".into();
        let def = library
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(12))
            .unwrap();
        def.properties.push(descriptor);
        def.next_property = 6;
    });
    let mut seed = library_fixture::seed(&ui).clone();
    let source = source(&seed);
    let report = endpoint(&seed, &source, DefinitionId(12), PropertyId(5));
    assert!(report.complete, "{:?}", report.blockers);
    assert!(report.readonly);
    assert!(!report.can_remove());
    seed.resolved_libraries.clear();
    assert!(!endpoint(&seed, &source, DefinitionId(12), PropertyId(5)).complete);
}

#[test]
fn inherited_declaration_collision_is_incomplete_even_without_paths() {
    let (mut seed, mut source) = fixture();
    let def = source
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    def.properties[0].id = PropertyId(1);
    sync(&mut seed, &source);
    assert!(!endpoint(&seed, &source, DefinitionId(10), PropertyId(1)).complete);
}

#[test]
fn duplicate_placements_and_unknown_destination_slots_are_incomplete() {
    use otrios_interfaces::oib::authoring::{Destination, Placement};
    let (seed, mut source) = fixture();
    let instance = source.instances[0].clone();
    let placement = Placement {
        instance: InstanceId(instance.id),
        destination: Destination::Canvas,
        frame: None,
    };
    source.placements.extend([placement.clone(), placement]);
    assert!(!endpoint(&seed, &source, DefinitionId(10), PropertyId(2)).complete);
    source.placements.truncate(1);
    let def = source
        .definitions
        .iter()
        .find(|d| d.id.0 == instance.component)
        .unwrap();
    source.placements[0].destination = Destination::Slot {
        component: ObjectAddress {
            instance: InstanceId(instance.id),
            references: vec![],
            node: def.root,
        },
        slot: SlotId(u64::MAX),
    };
    assert!(!endpoint(&seed, &source, DefinitionId(10), PropertyId(2)).complete);
}

#[test]
fn cached_shallow_subtree_does_not_hide_overdepth_dependency() {
    let (seed, _) = fixture();
    let mut graph = seed.graph.unwrap();
    let template = graph
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.reference.is_some())
        .unwrap()
        .clone();
    let mut next = DefinitionId(10);
    for index in 0..MAX_REFERENCE_DEPTH {
        let id = DefinitionId(1000 + index as u64);
        let mut root = template.clone();
        root.reference.as_mut().unwrap().definition = next;
        root.reference.as_mut().unwrap().assignments.clear();
        graph.definitions.push(ComponentDefinition {
            id,
            name: format!("Depth {index}"),
            root: root.id,
            nodes: vec![root],
            properties: vec![],
            next_property: 0,
            exposed: vec![],
            slots: vec![],
        });
        next = id;
    }
    assert!(validate_graph(&graph).is_err());
}
