// SPDX-License-Identifier: LGPL-3.0-only
//! Canonical graph and causal authority regressions; these are in-process checks.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::{
    self, Document as StateDocument, Session, StateAdapter, StateSelection, oib,
};
use otrios_interfaces::{
    canvas::*,
    layout::*,
    oib::{self as contract, designer::*, graph},
};
use std::collections::BTreeMap;

fn text(v: &str) -> Value {
    Value::Text(v.into())
}
fn path(references: &[u64]) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(1),
        property: PropertyId(1),
    }
}
fn address(root: u64, references: &[u64]) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(root),
        path: path(references),
    }
}
fn descriptor() -> PropertyDescriptor {
    PropertyDescriptor {
        id: PropertyId(1),
        name: "label".into(),
        label: "Label".into(),
        value_type: ValueType::Text,
        default: text("Default"),
        number_range: None,
        number_step: None,
        units: None,
        group: InspectorGroup::Component,
        editor: PropertyEditor::TextSuggestions,
        policy: PropertyPolicy {
            live_literal: true,
            app_binding: false,
        },
    }
}
fn class(
    id: u64,
    implementation: ClassKind,
    base: Option<u64>,
    properties: Vec<PropertyDescriptor>,
) -> ClassDescriptor {
    ClassDescriptor {
        id: ClassId(id),
        name: format!("class-{id}"),
        implementation,
        base: base.map(ClassId),
        properties,
        owned_child_classes: vec![ClassId(1)],
        visual_child_classes: vec![ClassId(3)],
    }
}
fn definition_node(
    id: u64,
    class: u64,
    owner: Option<u64>,
    children: &[u64],
    reference: Option<u64>,
) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: format!("node-{id}"),
        class: ClassId(class),
        owner: owner.map(NodeId),
        visual: None,
        visual_children: children.iter().copied().map(NodeId).collect(),
        defaults: vec![],
        reference: reference.map(|d| ComponentReference {
            definition: DefinitionId(d),
            assignments: vec![],
        }),
    }
}
fn visual(frame: [f32; 4]) -> graph::Visual {
    graph::Visual {
        layout: None,
        frame: frame.map(|v| Expr::Literal(Value::Number(v))),
        resize: None,
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: false },
        semantics: None,
        action: None,
        parameters: vec![],
    }
}
fn fixture_with(extra_properties: u64) -> Ui {
    assert_eq!(app::manifest().id, "oib-designer");
    let mut ui = app::ui(&[]);
    ui.state_schema = StateSchema {
        id: "designer-runtime-v2".into(),
        fields: vec![Field {
            id: 1,
            name: "app".into(),
            default: Value::Number(0.),
            writable: true,
        }],
    };
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    let previous = seed.graph.take().unwrap();
    let mut properties = vec![descriptor()];
    for id in 2..=extra_properties + 1 {
        let mut p = descriptor();
        p.id = PropertyId(id);
        p.name = format!("extra{id}");
        properties.push(p);
    }
    let mut classes = vec![
        class(1, ClassKind::OIBObject, None, vec![]),
        class(2, ClassKind::OIBDocument, Some(1), vec![]),
        class(3, ClassKind::O2DView, Some(1), vec![]),
        class(4, ClassKind::O2DPanel, Some(3), properties),
        class(5, ClassKind::O2DText, Some(3), vec![]),
    ];
    classes[0].visual_child_classes.clear();
    classes[1].visual_child_classes.clear();
    let mut panel = definition_node(1, 4, None, &[7, 8], None);
    panel.visual = Some(visual([0., 0., 400., 100.]));
    let mut a = definition_node(7, 3, Some(1), &[], Some(12));
    a.visual = Some(visual([0., 0., 180., 48.]));
    let mut b = definition_node(8, 3, Some(1), &[], Some(12));
    b.visual = Some(visual([200., 0., 180., 48.]));
    let mut child = definition_node(1, 4, None, &[2], None);
    child.visual = Some(visual([0., 0., 180., 48.]));
    let mut caption = definition_node(2, 5, Some(1), &[], None);
    let mut label = visual([0., 0., 176., 32.]);
    label.shape = Shape::Text {
        value: Expr::Parameter(1),
        scale: Expr::Literal(Value::Number(1.)),
        color: Expr::Literal(Value::Color([255; 4])),
    };
    label.parameters.push(graph::Parameter {
        local: 1,
        source: graph::ParameterSource::Property(path(&[])),
        projection: Projection::Identity,
    });
    caption.visual = Some(label);
    let definitions = vec![
        ComponentDefinition {
            id: DefinitionId(10),
            name: "Panel".into(),
            root: NodeId(1),
            nodes: vec![panel, a, b],
            exposed: vec![ExposedProperty {
                property: PropertyId(1),
                targets: vec![path(&[7]), path(&[8])],
            }],
            slots: vec![],
        },
        ComponentDefinition {
            id: DefinitionId(11),
            name: "Shell".into(),
            root: NodeId(1),
            nodes: vec![
                definition_node(1, 3, None, &[], None),
                definition_node(2, 1, Some(1), &[], None),
            ],
            exposed: vec![],
            slots: vec![],
        },
        ComponentDefinition {
            id: DefinitionId(12),
            name: "Child".into(),
            root: NodeId(1),
            nodes: vec![child, caption],
            exposed: vec![],
            slots: vec![],
        },
    ];
    let mut presets = vec![];
    let mut instances = vec![];
    for (order, id, name) in [(0, 20, "Audio"), (1, 30, "Display")] {
        presets.push(graph::Preset {
            id,
            name: name.into(),
            definition: DefinitionId(10),
            assignments: vec![graph::Assignment {
                target: path(&[]),
                source: graph::Source::Literal(text(name)),
            }],
            frame: None,
            palette: true,
        });
        instances.push(graph::Instance {
            id: InstanceId(id),
            name: name.into(),
            preset: id,
            owner: Owner::Document,
            visual_parent: Some(ObjectAddress {
                instance: InstanceId(40),
                references: vec![],
                node: NodeId(1),
            }),
            order,
        });
    }
    presets.push(graph::Preset {
        id: 40,
        name: "Shell".into(),
        definition: DefinitionId(11),
        assignments: vec![],
        frame: None,
        palette: false,
    });
    instances.push(graph::Instance {
        id: InstanceId(40),
        name: "Shell".into(),
        preset: 40,
        owner: Owner::Document,
        visual_parent: None,
        order: 0,
    });
    seed.graph = Some(graph::Document {
        id: DocumentId(1),
        name: "Runtime fixture".into(),
        class: ClassId(2),
        presented_root: InstanceId(40),
        classes,
        definitions,
        imports: None,
        presets,
        instances,
        grants: vec![],
        preview_viewport: previous.preview_viewport,
        inspector_viewport: previous.inspector_viewport,
        hierarchy_viewport: previous.hierarchy_viewport,
        preview_stride: 100.,
        inspector_content: [500., 1000.],
    });
    canvas::decode_ui(&canvas::encode_ui(&ui).unwrap()).unwrap()
}
fn fixture() -> Ui {
    fixture_with(0)
}
fn seed(ui: &Ui) -> &contract::Seed {
    oib::seed(ui).unwrap()
}
fn cell(ui: &Ui, allocations: &[contract::Allocation], address: &PropertyAddress) -> u64 {
    oib::document::cell_for_address(seed(ui), allocations, address).unwrap()
}
fn resolved(
    ui: &Ui,
    allocations: &[contract::Allocation],
    values: &BTreeMap<u64, Option<Value>>,
    address: &PropertyAddress,
) -> Value {
    let expanded = oib::instances::expanded_seed(seed(ui), allocations).unwrap();
    oib::effective(&expanded, values, cell(ui, allocations, address)).unwrap()
}

#[test]
fn full_reference_paths_keep_equal_nested_overrides_and_forwarded_sources_distinct() {
    let ui = fixture();
    let allocations = oib::instances::initial_allocations(seed(&ui));
    let outer = cell(&ui, &allocations, &address(20, &[]));
    let left = cell(&ui, &allocations, &address(20, &[7]));
    let right = cell(&ui, &allocations, &address(20, &[8]));
    assert_ne!(left, right);
    assert!(
        oib::instances::cell_id(
            seed(&ui),
            &allocations,
            20,
            seed(&ui).designer.as_ref().unwrap().mappings.definitions[1].cell
        )
        .is_err()
    );
    let mut values = BTreeMap::new();
    assert_eq!(
        resolved(&ui, &allocations, &values, &address(20, &[7])),
        text("Audio")
    );
    values.insert(outer, Some(text("Live outer")));
    values.insert(left, Some(text("Live outer")));
    values.insert(outer, Some(text("New outer")));
    assert_eq!(
        resolved(&ui, &allocations, &values, &address(20, &[7])),
        text("Live outer")
    );
    assert_eq!(
        resolved(&ui, &allocations, &values, &address(20, &[8])),
        text("New outer")
    );
    values.insert(left, None);
    assert_eq!(
        resolved(&ui, &allocations, &values, &address(20, &[7])),
        text("New outer")
    );
    values.insert(outer, None);
    assert_eq!(
        resolved(&ui, &allocations, &values, &address(20, &[7])),
        text("Audio")
    );
    let sources =
        oib::document::property_sources(seed(&ui), &allocations, &address(20, &[7]), &values)
            .unwrap();
    assert_eq!(sources.immediate_source, Some(address(20, &[])));
    assert!(oib::document::objects(seed(&ui),&allocations).unwrap().iter().any(|o|o.layout_node.is_none()&&matches!(&o.selection,Selection::Object(a) if a.instance==InstanceId(40)&&a.node==NodeId(2))));
}
#[test]
fn large_persistent_id_and_cache_vector_order_do_not_select_another_occurrence() {
    let mut ui = fixture();
    let mut allocations = oib::instances::initial_allocations(seed(&ui));
    allocations.push(contract::Allocation {
        id: u64::MAX,
        name: "dynamic".into(),
        component: 10,
        prototype: 20,
        slot: 7,
    });
    let expected = cell(&ui, &allocations, &address(u64::MAX, &[8]));
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed.cells.reverse();
    assert_eq!(cell(&ui, &allocations, &address(u64::MAX, &[8])), expected);
    assert_eq!(
        resolved(
            &ui,
            &allocations,
            &BTreeMap::new(),
            &address(u64::MAX, &[8])
        ),
        text("Audio")
    );
    assert_ne!(cell(&ui, &allocations, &address(u64::MAX, &[7])), expected);
}
#[test]
fn canonical_source_rejects_reference_cycles_wrong_types_and_invalid_forwarding() {
    for mutation in 0..3 {
        let mut ui = fixture();
        let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
            unreachable!()
        };
        let graph = seed.graph.as_mut().unwrap();
        match mutation {
            0 => {
                graph.definitions[2].nodes[0].reference = Some(ComponentReference {
                    definition: DefinitionId(10),
                    assignments: vec![],
                })
            }
            1 => {
                graph.presets[0].assignments[0].source =
                    graph::Source::Literal(Value::Boolean(false))
            }
            _ => graph.presets[0].assignments[0].source = graph::Source::Forward(path(&[])),
        }
        assert!(canvas::decode_ui(&canvas::encode_ui(&ui).unwrap()).is_err());
    }
}
#[test]
fn standalone_canonical_seed_discards_caches_and_checks_graph_bounds() {
    let ui = fixture();
    let encoded = flexbuffers::to_vec(seed(&ui)).unwrap();
    let decoded = oib::document::decode_seed(&encoded).unwrap();
    assert_eq!(decoded.graph, seed(&ui).graph);
    assert!(oib::document::decode_seed(&vec![0; 24 * 1024 + 1]).is_err());
    let mut bad = seed(&ui).clone();
    bad.graph.as_mut().unwrap().classes[3].properties[0].label = "a\0b".into();
    assert!(oib::document::decode_seed(&flexbuffers::to_vec(&bad).unwrap()).is_err());
}

fn setup() -> (Ui, StateDocument) {
    let ui = fixture();
    let mut doc = StateDocument::new(ui.state_schema.clone(), "oib:runtime").unwrap();
    doc.configure_oib(&ui).unwrap();
    (ui, doc)
}
fn sync(doc: &mut StateDocument, session: &mut Session, client: &mut StateAdapter) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = client.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            client.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("OIB runtime sync did not settle");
}
fn adapter(ui: &Ui, doc: &mut StateDocument, actor: u8) -> (StateAdapter, Session) {
    let (mut session, _) = doc.open_session(&[actor; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(ui, "oib:runtime", &[], [actor; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(doc, &mut session, &mut client);
    (client, session)
}
fn issue(client: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = client.generate().unwrap() {
        if matches!(
            canvas::decode_state_request(&bytes).unwrap(),
            StateRequest::OibWrite { .. }
        ) {
            return bytes;
        }
    }
    panic!("Missing OIB write");
}
#[test]
fn causal_equal_override_reset_duplicate_and_recovery() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    a.edit_oib_property(&address(20, &[7]), Some(text("Audio")))
        .unwrap();
    sync(&mut doc, &mut sa, &mut a);
    a.edit_oib_property(&address(20, &[]), Some(text("Changed")))
        .unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(
        resolved(
            &ui,
            &doc.oib_allocations().unwrap(),
            &a.oib_active().unwrap(),
            &address(20, &[7])
        ),
        text("Audio")
    );
    a.duplicate_oib(20, Some("copy".into())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let allocations = doc.oib_allocations().unwrap();
    let new = allocations.last().unwrap().id;
    let expanded = oib::instances::expanded_seed(seed(&ui), &allocations).unwrap();
    let copied =
        oib::document::cell_for_address(seed(&ui), &allocations, &address(new, &[7])).unwrap();
    let other =
        oib::document::cell_for_address(seed(&ui), &allocations, &address(new, &[8])).unwrap();
    let values = doc.oib_values(2).unwrap();
    assert_eq!(values[&copied], Some(text("Audio")));
    assert_eq!(values[&other], None);
    assert_eq!(
        oib::effective(&expanded, &values, other).unwrap(),
        text("Changed")
    );
    a.edit_oib_property(&address(new, &[7]), None).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(
        oib::effective(&expanded, &doc.oib_values(2).unwrap(), copied).unwrap(),
        text("Changed")
    );
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(0.));
    let mut loaded =
        StateDocument::load(ui.state_schema.clone(), "oib:runtime", &doc.save().unwrap()).unwrap();
    assert!(!loaded.configure_oib(&ui).unwrap());
    assert_eq!(loaded.root_hash(), doc.root_hash());
    let before = doc.root_hash();
    a.edit_oib_property(&address(new, &[7]), Some(Value::Number(9.)))
        .unwrap_err();
    assert_eq!(doc.root_hash(), before);
    a.remove_oib(new).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert!(!doc.oib_allocations().unwrap().iter().any(|r| r.id == new));
    assert!(!doc.oib_values(1).unwrap().contains_key(&copied));
    a.instantiate_oib(10, None).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    assert!(doc.oib_allocations().unwrap().last().unwrap().id > new);
}
#[test]
fn concurrent_nested_conflict_pins_exact_active_graph() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2);
    a.edit_oib_property(&address(20, &[7]), Some(text("A")))
        .unwrap();
    let a_write = issue(&mut a);
    b.edit_oib_property(&address(20, &[7]), Some(text("B")))
        .unwrap();
    let b_write = issue(&mut b);
    doc.receive(&mut sa, &a_write).unwrap();
    let active = doc.oib_status().unwrap().active_root;
    let values = doc.oib_values(2).unwrap();
    doc.receive(&mut sb, &b_write).unwrap();
    assert_eq!(doc.oib_status().unwrap().conflicts, vec![address(20, &[7])]);
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    assert_eq!(doc.oib_values(2).unwrap(), values);
    let saved = doc.save().unwrap();
    let mut loaded = StateDocument::load(ui.state_schema.clone(), "oib:runtime", &saved).unwrap();
    loaded.configure_oib(&ui).unwrap();
    assert_eq!(loaded.oib_status().unwrap().active_root, active);
}
#[test]
fn resident_property_union_rejects_before_identity_allocation_and_slots_are_reusable() {
    // Ten shared properties + fifteen per resident: three roots fit, four exceed64.
    let ui = fixture_with(4);
    let mut doc = StateDocument::new(ui.state_schema.clone(), "oib:runtime").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut client, mut session) = adapter(&ui, &mut doc, 1);
    assert!(client.insert_oib(20, None).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(doc.oib_allocations().unwrap().len(), 3);
    let ids = doc.oib_allocations().unwrap();
    let before = doc.root_hash();
    assert!(!client.insert_oib(20, None).unwrap());
    assert_eq!(doc.root_hash(), before);
    assert_eq!(doc.oib_allocations().unwrap(), ids);
    let last = ids.last().unwrap().id;
    let slot = ids.last().unwrap().slot;
    client.remove_oib(last).unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.insert_oib(20, None).unwrap();
    sync(&mut doc, &mut session, &mut client);
    let allocations = doc.oib_allocations().unwrap();
    let fresh = allocations.last().unwrap();
    assert!(fresh.id > last);
    assert_eq!(fresh.slot, slot);
}
#[test]
fn retired_full_path_intent_cannot_mutate_reused_slot() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1);
    a.duplicate_oib(20, None).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let retired = doc.oib_allocations().unwrap().last().unwrap().id;
    let (mut delayed, mut sd) = adapter(&ui, &mut doc, 3);
    delayed
        .edit_oib_property(&address(retired, &[8]), Some(text("Delayed")))
        .unwrap();
    let stale = issue(&mut delayed);
    a.remove_oib(retired).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    a.duplicate_oib(20, None).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let roots = doc.oib_allocations().unwrap();
    let fresh = roots.last().unwrap();
    assert!(fresh.id > retired);
    let before = doc.oib_status().unwrap().active_root;
    let values = doc.oib_values(2).unwrap();
    doc.receive(&mut sd, &stale).unwrap();
    assert_eq!(doc.oib_status().unwrap().active_root, before);
    assert_eq!(doc.oib_values(2).unwrap(), values);
    assert!(oib::document::cell_for_address(seed(&ui), &roots, &address(retired, &[8])).is_err());
}
#[test]
fn restore_uses_fresh_roots_and_preserves_signed_sources_and_app_values() {
    let (ui, mut doc) = setup();
    let (mut client, mut session) = adapter(&ui, &mut doc, 1);
    client
        .edit_oib_property(&address(20, &[7]), Some(text("Custom")))
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.duplicate_oib(20, None).unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.restore_oib(false).unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(doc.oib_allocations().unwrap().len(), 2);
    assert_eq!(
        doc.oib_values(2).unwrap()[&cell(&ui, &doc.oib_allocations().unwrap(), &address(20, &[7]))],
        Some(text("Custom"))
    );
    client.remove_oib(20).unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.restore_oib(true).unwrap();
    sync(&mut doc, &mut session, &mut client);
    let allocations = doc.oib_allocations().unwrap();
    assert_eq!(allocations.len(), 2);
    assert!(!allocations.iter().any(|a| a.id == 20));
    let fresh = allocations.iter().find(|a| a.prototype == 20).unwrap().id;
    assert!(fresh > 40);
    let cell =
        oib::document::cell_for_address(seed(&ui), &allocations, &address(fresh, &[7])).unwrap();
    let expanded = oib::instances::expanded_seed(seed(&ui), &allocations).unwrap();
    assert_eq!(
        oib::effective(&expanded, &doc.oib_values(2).unwrap(), cell).unwrap(),
        text("Audio")
    );
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(0.));
}
