// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::oib::authoring::{
    AppConnection, ConnectionSource, InstanceRecord, Placement,
};

#[path = "../../../tests/support/oib_library_fixture.rs"]
#[allow(dead_code)]
mod library_fixture;

fn source(seed: &Seed) -> DocumentSource {
    let g = seed.graph.as_ref().unwrap();
    DocumentSource {
        definitions: g.definitions.clone(),
        presets: g.presets.clone(),
        instances: g
            .instances
            .iter()
            .map(|i| InstanceRecord {
                id: i.id.0,
                name: i.name.clone(),
                component: g
                    .presets
                    .iter()
                    .find(|p| p.id == i.preset)
                    .unwrap()
                    .definition
                    .0,
                prototype: i.preset,
            })
            .collect(),
        next_identity: 200,
        baseline: vec![],
        placements: vec![],
        connections: vec![],
    }
}
fn fixture() -> (Seed, DocumentSource, InstanceId) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    let seed = super::super::seed(&ui).unwrap().clone();
    let source = source(&seed);
    let root = source
        .instances
        .iter()
        .find(|i| {
            i.id != seed.graph.as_ref().unwrap().presented_root.0
                && source
                    .presets
                    .iter()
                    .any(|p| p.id == i.prototype && p.palette)
        })
        .unwrap();
    let target = InstanceId(root.id);
    (seed, source, target)
}
fn slot(seed: &mut Seed, source: &mut DocumentSource, id: InstanceId) -> ObjectAddress {
    let root = source.instances.iter().find(|i| i.id == id.0).unwrap();
    let d = source
        .definitions
        .iter_mut()
        .find(|d| d.id.0 == root.component)
        .unwrap();
    d.slots.push(SlotDescriptor {
        id: SlotId(900),
        name: "Owned content".into(),
        references: vec![],
        node: d.root,
        admitted_classes: vec![],
        cardinality: [0, 6],
        layout: SlotLayout::ExistingFlow,
    });
    let address = ObjectAddress {
        instance: id,
        references: vec![],
        node: d.root,
    };
    seed.graph.as_mut().unwrap().definitions = source.definitions.clone();
    address
}
fn attach(source: &mut DocumentSource, child: InstanceId, parent: ObjectAddress) {
    source.placements.push(Placement {
        instance: child,
        destination: Destination::Slot {
            component: parent,
            slot: SlotId(900),
        },
        frame: None,
    });
}
#[test]
fn root_removal_keeps_reusable_definition_and_has_navigable_evidence() {
    let (seed, source, target) = fixture();
    let report = instance(&seed, &source, target);
    assert!(report.can_remove(), "{:?}", report.blockers);
    assert_eq!(report.target, target);
    assert_eq!(report.removal_order, vec![target]);
    assert!(report.scope.contains("other documents are not checked"));
    assert!(report.items.iter().any(|i| i.kind == Kind::Definition
        && i.detail.contains("remains")
        && matches!(i.selection, Some(Selection::Definition(_)))));
    assert!(
        report
            .items
            .iter()
            .any(|i| i.kind == Kind::Instance && matches!(i.selection, Some(Selection::Object(_))))
    );
    assert!(report.items.iter().all(|i| !i.blocking));
}
#[test]
fn older_children_and_connections_are_removed_before_owner_in_source_sibling_order() {
    let (mut seed, mut source, target) = fixture();
    source
        .instances
        .iter_mut()
        .find(|i| i.id == target.0)
        .unwrap()
        .id = 192;
    let target = InstanceId(192);
    let owner = slot(&mut seed, &mut source, target);
    let mut child = source
        .instances
        .iter()
        .find(|i| i.id == target.0)
        .unwrap()
        .clone();
    child.id = 191;
    child.name = "First older child".into();
    let mut sibling = child.clone();
    sibling.id = 190;
    sibling.name = "Second older child".into();
    source.instances.insert(0, sibling);
    source.instances.insert(0, child);
    attach(&mut source, InstanceId(190), owner.clone());
    attach(&mut source, InstanceId(191), owner.clone());
    let child_address = ObjectAddress {
        instance: InstanceId(191),
        ..owner
    };
    source.connections.push(AppConnection {
        object: child_address.clone(),
        property: PropertyId(4),
        source: ConnectionSource {
            field: 1,
            projection: otrios_interfaces::canvas::Projection::Identity,
        },
    });
    let report = instance(&seed, &source, target);
    assert!(report.can_remove(), "{:?}", report.blockers);
    assert_eq!(
        report.removal_order,
        vec![InstanceId(191), InstanceId(190), target]
    );
    assert!(report.items.iter().any(|i| i.kind == Kind::Api
        && !i.blocking
        && i.selection == Some(Selection::Object(child_address.clone()))));
    assert!(
        report
            .items
            .iter()
            .filter(|i| i.kind == Kind::Instance)
            .all(|i| !i.blocking)
    );
}
#[test]
fn selected_child_reports_retained_external_parent() {
    let (mut seed, mut source, target) = fixture();
    let owner = slot(&mut seed, &mut source, target);
    let mut child = source
        .instances
        .iter()
        .find(|i| i.id == target.0)
        .unwrap()
        .clone();
    child.id = 191;
    source.instances.push(child);
    attach(&mut source, InstanceId(191), owner.clone());
    let report = instance(&seed, &source, InstanceId(191));
    assert!(report.can_remove(), "{:?}", report.blockers);
    assert_eq!(report.removal_order, vec![InstanceId(191)]);
    assert!(report.items.iter().any(|i| i.kind == Kind::Ownership
        && i.detail.contains("parent instance remains")
        && i.selection == Some(Selection::Object(owner.clone()))));
}
#[test]
fn missing_roots_prototypes_and_mismatched_candidates_block_removal() {
    let (seed, source, target) = fixture();
    assert!(!instance(&seed, &source, InstanceId(999)).can_remove());
    let mut invalid = source.clone();
    invalid
        .instances
        .iter_mut()
        .find(|i| i.id == target.0)
        .unwrap()
        .prototype = 999;
    assert!(!instance(&seed, &invalid, target).complete);
    let mut invalid = source.clone();
    invalid.definitions[0].name.push_str(" changed");
    assert!(!instance(&seed, &invalid, target).can_remove());
    let mut invalid = source.clone();
    invalid.instances.push(invalid.instances[0].clone());
    assert!(!instance(&seed, &invalid, target).can_remove());
}
#[test]
fn cycles_missing_owners_and_bad_slot_paths_are_incomplete() {
    let (mut seed, mut source, target) = fixture();
    let owner = slot(&mut seed, &mut source, target);
    attach(&mut source, target, owner.clone());
    assert!(!instance(&seed, &source, target).can_remove());
    source.placements.clear();
    attach(
        &mut source,
        target,
        ObjectAddress {
            instance: InstanceId(999),
            ..owner.clone()
        },
    );
    assert!(!instance(&seed, &source, target).complete);
    source.placements.clear();
    attach(
        &mut source,
        target,
        ObjectAddress {
            references: vec![NodeId(999)],
            ..owner
        },
    );
    assert!(!instance(&seed, &source, target).complete);
}
#[test]
fn fixed_occurrence_is_readonly() {
    let (seed, source, _) = fixture();
    let fixed = seed.graph.as_ref().unwrap().presented_root;
    let report = instance(&seed, &source, fixed);
    assert!(report.readonly);
    assert!(!report.can_remove());
}
#[test]
fn imported_definition_body_does_not_make_allocated_instance_readonly() {
    let ui = library_fixture::imported();
    let mut seed = library_fixture::seed(&ui).clone();
    let preset = graph::Preset {
        id: 190,
        name: "Library slider".into(),
        definition: DefinitionId(12),
        assignments: vec![],
        frame: None,
        palette: true,
    };
    seed.graph.as_mut().unwrap().presets.push(preset);
    let mut source = source(&seed);
    source.instances.push(InstanceRecord {
        id: 191,
        name: "Slider occurrence".into(),
        component: 12,
        prototype: 190,
    });
    let report = instance(&seed, &source, InstanceId(191));
    assert!(report.can_remove(), "{:?}", report.blockers);
    assert!(!report.readonly);
    assert!(report.items.iter().any(|i| i.kind == Kind::Definition
        && i.selection == Some(Selection::Definition(DefinitionId(12)))));
    seed.resolved_libraries.clear();
    assert!(!instance(&seed, &source, InstanceId(191)).complete);
}
