// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::oib::authoring::{AppConnection, ConnectionSource, InstanceRecord};

#[path = "../../../tests/support/oib_library_fixture.rs"]
#[allow(dead_code)] // Shared signed fixtures also expose update helpers for other suites.
mod library_fixture;

fn fixture() -> (Seed, DocumentSource) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    let seed = super::super::seed(&ui).unwrap().clone();
    let source = source(&seed);
    (seed, source)
}
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
fn add_unused(seed: &mut Seed, source: &mut DocumentSource) -> DefinitionId {
    let mut d = source.definitions[0].clone();
    d.id = DefinitionId(100);
    d.name = "Unused custom component".into();
    let mut root = d.nodes.iter().find(|n| n.id == d.root).unwrap().clone();
    root.id = NodeId(101);
    root.owner = None;
    root.visual_children.clear();
    root.reference = None;
    d.root = root.id;
    d.nodes = vec![root];
    d.exposed.clear();
    d.slots.clear();
    let preset = graph::Preset {
        id: 102,
        name: "Custom insertion".into(),
        definition: d.id,
        assignments: vec![],
        frame: None,
        palette: true,
    };
    source.definitions.push(d.clone());
    source.presets.push(preset.clone());
    let g = seed.graph.as_mut().unwrap();
    g.definitions.push(d);
    g.presets.push(preset);
    DefinitionId(100)
}
#[test]
fn unused_component_lists_palette_removal_without_claiming_global_absence() {
    let (mut seed, mut source) = fixture();
    let target = add_unused(&mut seed, &mut source);
    let report = component(&seed, &source, target);
    assert!(report.complete, "{:?}", report.blockers);
    assert!(report.can_delete());
    assert!(report.scope.contains("external consumers are not scanned"));
    assert_eq!(report.removable_presets, vec![102]);
    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].kind, Kind::Preset);
    assert!(!report.items[0].blocking);
    assert_eq!(report.items[0].label, "Custom insertion");
}
#[test]
fn component_reports_named_direct_transitive_uses_and_each_document_instance() {
    let (seed, source) = fixture();
    // The shipped canonical fixture composes Slider12 through SettingsPanel10.
    let target = DefinitionId(12);
    let report = component(&seed, &source, target);
    assert!(report.complete, "{:?}", report.blockers);
    assert!(!report.can_delete());
    assert!(report.items.iter().any(|item| item.kind == Kind::Definition
        && item.direct
        && item.label.contains("SettingsPanel")
        && matches!(item.selection, Some(Selection::DefinitionNode { .. }))));
    for instance in source.instances.iter().filter(|i| i.component == 10) {
        assert!(report.items.iter().any(|item| item.kind == Kind::Instance
            && !item.direct
            && item.label == instance.name
            && matches!(item.selection, Some(Selection::Object(_)))));
    }
    assert!(report.items.iter().any(|item| item.kind == Kind::Api));
    let again = component(&seed, &source, target);
    assert_eq!(
        report
            .items
            .iter()
            .map(|i| (&i.label, &i.detail))
            .collect::<Vec<_>>(),
        again
            .items
            .iter()
            .map(|i| (&i.label, &i.detail))
            .collect::<Vec<_>>()
    );
}
#[test]
fn required_preset_and_shell_ownership_block_even_without_mutable_instances() {
    let (seed, mut source) = fixture();
    source.instances.clear();
    let g = seed.graph.as_ref().unwrap();
    let shell = g
        .instances
        .iter()
        .find(|i| i.id == g.presented_root)
        .unwrap();
    let target = g
        .presets
        .iter()
        .find(|p| p.id == shell.preset)
        .unwrap()
        .definition;
    let report = component(&seed, &source, target);
    assert!(!report.can_delete());
    assert!(
        report
            .items
            .iter()
            .any(|i| i.kind == Kind::Ownership && i.blocking)
    );
    assert!(
        report
            .items
            .iter()
            .any(|i| i.kind == Kind::Preset && i.blocking)
    );
}
#[test]
fn stale_or_incomplete_sources_never_establish_zero_uses() {
    let (mut seed, mut source) = fixture();
    let target = add_unused(&mut seed, &mut source);
    source
        .definitions
        .iter_mut()
        .find(|d| d.id == target)
        .unwrap()
        .name = "Other candidate".into();
    let report = component(&seed, &source, target);
    assert!(!report.complete);
    assert!(!report.can_delete());
    assert!(report.items.iter().any(|i| i.kind == Kind::Unknown));
    let report = component(&seed, &source, DefinitionId(999));
    assert!(!report.complete);
    assert!(!report.can_delete());
}
#[test]
fn imported_body_reports_exact_export_owner_and_missing_closure_is_unknown() {
    let ui = library_fixture::imported();
    let seed = library_fixture::seed(&ui);
    let source = source(seed);
    let report = component(seed, &source, DefinitionId(12));
    assert!(report.complete, "{:?}", report.blockers);
    assert!(report.readonly);
    assert!(!report.can_delete());
    assert!(report.items.iter().any(|i| i.kind == Kind::Export
        && i.label == "Slider"
        && i.detail.contains(":controls @ 01010101")));
    let mut missing = seed.clone();
    missing.resolved_libraries.clear();
    let incomplete = component(&missing, &source, DefinitionId(12));
    assert!(!incomplete.complete);
    assert!(!incomplete.can_delete());
    assert!(incomplete.items.iter().any(|i| i.kind == Kind::Unknown));
}
#[test]
fn app_connection_reports_navigable_semantic_use() {
    let (seed, mut source) = fixture();
    let instance = source.instances.iter().find(|i| i.component == 10).unwrap();
    let d = source
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let reference = d
        .nodes
        .iter()
        .find(|n| {
            n.reference
                .as_ref()
                .is_some_and(|r| r.definition == DefinitionId(12))
        })
        .unwrap();
    let slider = source
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(12))
        .unwrap();
    let object = ObjectAddress {
        instance: InstanceId(instance.id),
        references: vec![reference.id],
        node: slider.root,
    };
    source.connections.push(AppConnection {
        object: object.clone(),
        property: PropertyId(3),
        source: ConnectionSource {
            field: 1,
            projection: otrios_interfaces::canvas::Projection::Identity,
        },
    });
    let report = component(&seed, &source, DefinitionId(12));
    assert!(report.complete, "{:?}", report.blockers);
    assert!(report.items.iter().any(|i| i.kind == Kind::Api
        && i.detail == "App property connection"
        && matches!(&i.selection,Some(Selection::Object(o)) if *o==object)));
}
