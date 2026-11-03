// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
//! Runtime-only D1 fixtures; these are not process/native/browser acceptance.
use otrios_core::canvas::{
    self, Document as StateDocument, Session, StateAdapter, StateSelection, oib,
};
use otrios_core::dashboard::layout::Layout;
use otrios_interfaces::{
    canvas::*,
    layout::{self, *},
    oib::{self as contract, designer::*},
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
        visual_children: children.iter().copied().map(NodeId).collect(),
        defaults: vec![],
        reference: reference.map(|d| ComponentReference {
            definition: DefinitionId(d),
            assignments: vec![],
        }),
    }
}
fn node(id: u16, frame: [f32; 4], shape: Shape, children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode {
        id,
        resize: None,
        frame: frame.map(|n| Expr::Literal(Value::Number(n))),
        visible: Expr::Literal(Value::Boolean(true)),
        shape,
        responder: None,
        semantics: None,
        children,
    }
}
fn container(id: u16, children: Vec<TemplateNode>) -> TemplateNode {
    node(
        id,
        [0., 0., 500., 100.],
        Shape::Canvas { clip: true },
        children,
    )
}
fn panel(id: u16, y: f32) -> TemplateNode {
    let leaf = |n| {
        node(
            n,
            [0., 0., 300., 20.],
            Shape::Text {
                value: Expr::Literal(text("Default")),
                scale: Expr::Literal(Value::Number(1.)),
                color: Expr::Literal(Value::Color([255; 4])),
            },
            vec![],
        )
    };
    let mut result = container(
        id,
        vec![
            container(id + 1, vec![container(id + 2, vec![leaf(id + 3)])]),
            container(id + 4, vec![container(id + 5, vec![leaf(id + 6)])]),
        ],
    );
    result.frame[1] = Expr::Literal(Value::Number(y));
    result
}
fn fixture() -> Ui {
    let mut classes = vec![
        class(1, ClassKind::OIBObject, None, vec![]),
        class(2, ClassKind::OIBDocument, Some(1), vec![]),
        class(3, ClassKind::O2DView, Some(1), vec![]),
        class(4, ClassKind::O2DPanel, Some(3), vec![descriptor()]),
        class(5, ClassKind::O2DText, Some(3), vec![]),
    ];
    classes[0].visual_child_classes.clear();
    classes[1].visual_child_classes.clear();
    let definitions = vec![
        ComponentDefinition {
            id: DefinitionId(10),
            name: "Panel".into(),
            root: NodeId(1),
            nodes: vec![
                definition_node(1, 4, None, &[7, 8], None),
                definition_node(7, 3, Some(1), &[], Some(12)),
                definition_node(8, 3, Some(1), &[], Some(12)),
            ],
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
            nodes: vec![
                definition_node(1, 4, None, &[2], None),
                definition_node(2, 5, Some(1), &[], None),
            ],
            exposed: vec![],
            slots: vec![],
        },
    ];
    let mut instances = Vec::new();
    let mut mappings = Mappings {
        objects: vec![],
        definitions: vec![
            DefinitionCellMapping {
                definition: DefinitionId(10),
                node: NodeId(1),
                property: PropertyId(1),
                cell: 100,
            },
            DefinitionCellMapping {
                definition: DefinitionId(12),
                node: NodeId(1),
                property: PropertyId(1),
                cell: 101,
            },
        ],
        roots: vec![],
    };
    let mut cells = vec![
        contract::Cell {
            id: 100,
            owner: 10,
            name: "label".into(),
            default: text("Default"),
            authored_override: None,
            choices: vec![text("Other")],
            inherit: None,
            targets: vec![],
        },
        contract::Cell {
            id: 101,
            owner: 12,
            name: "label".into(),
            default: text("Default"),
            authored_override: None,
            choices: vec![text("Other")],
            inherit: None,
            targets: vec![],
        },
    ];
    for (root, base, alias) in [(20, 10, 200), (30, 20, 300)] {
        instances.push(ComponentInstance {
            id: InstanceId(root),
            name: format!("panel-{root}"),
            definition: DefinitionId(10),
            owner: Owner::Document,
            visual_parent: Some(ObjectAddress {
                instance: InstanceId(40),
                references: vec![],
                node: NodeId(1),
            }),
            order: root as u16,
            assignments: vec![Assignment {
                target: path(&[]),
                source: AssignmentSource::Literal(text(if root == 20 {
                    "Audio"
                } else {
                    "Display"
                })),
            }],
        });
        let mut rows = Vec::new();
        for (rank, refs, key, target) in [
            (0, vec![], 2, None),
            (1, vec![7], 17, Some(base + 3)),
            (2, vec![8], 55, Some(base + 6)),
        ] {
            rows.push(RootCellKey {
                path: path(&refs),
                key,
                initial_cell: alias + rank,
            });
            cells.push(contract::Cell {
                id: alias + rank,
                owner: root,
                name: "label".into(),
                default: text("Default"),
                authored_override: if rank == 0 {
                    Some(text(if root == 20 { "Audio" } else { "Display" }))
                } else {
                    None
                },
                choices: vec![text("Other")],
                inherit: Some(if rank == 0 { 100 } else { 101 }),
                targets: target
                    .map(|node| contract::Target {
                        node,
                        property: contract::Property::Text,
                        scale: 1.,
                        offset: 0.,
                    })
                    .into_iter()
                    .collect(),
            });
        }
        mappings.roots.push(RootCellMapping {
            prototype: InstanceId(root),
            cells: rows,
        });
        for (refs, n, id) in [
            (vec![], 1, base),
            (vec![], 7, base + 1),
            (vec![], 8, base + 4),
            (vec![7], 1, base + 2),
            (vec![7], 2, base + 3),
            (vec![8], 1, base + 5),
            (vec![8], 2, base + 6),
        ] {
            mappings.objects.push(ObjectMapping {
                address: ObjectAddress {
                    instance: InstanceId(root),
                    references: refs.into_iter().map(NodeId).collect(),
                    node: NodeId(n),
                },
                layout_node: Some(id),
            });
        }
    }
    instances.push(ComponentInstance {
        id: InstanceId(40),
        name: "Shell".into(),
        definition: DefinitionId(11),
        owner: Owner::Document,
        visual_parent: None,
        order: 0,
        assignments: vec![],
    });
    mappings.objects.extend([
        ObjectMapping {
            address: ObjectAddress {
                instance: InstanceId(40),
                references: vec![],
                node: NodeId(1),
            },
            layout_node: Some(2),
        },
        ObjectMapping {
            address: ObjectAddress {
                instance: InstanceId(40),
                references: vec![],
                node: NodeId(2),
            },
            layout_node: None,
        },
    ]);
    mappings.objects.sort_by(|a, b| a.address.cmp(&b.address));
    let doc = otrios_interfaces::oib::designer::Document {
        id: DocumentId(1),
        name: "Runtime fixture".into(),
        class: ClassId(2),
        presented_root: InstanceId(40),
        hierarchy_viewport: None,
        classes,
        definitions,
        instances,
        mappings,
    };
    let blueprint = Blueprint {
        schema: layout::SPARSE_EVENTS_FORMAT.into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "root".into(),
            parameters: vec![],
            root: container(
                0,
                vec![
                    container(2, vec![panel(10, 0.), panel(20, 100.)]),
                    container(3, vec![container(30, vec![]), container(31, vec![])]),
                ],
            ),
        }],
        root: layout::Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    };
    Ui {
        format: "otrios-canvas-v8".into(),
        ui_ref: "app:designer-runtime".into(),
        title: "Runtime fixture".into(),
        state_schema: StateSchema {
            id: "designer-runtime-v1".into(),
            fields: vec![Field {
                id: 1,
                name: "app".into(),
                default: Value::Number(0.),
                writable: true,
            }],
        },
        layout: Layout::encode(&blueprint).unwrap(),
        selectors: vec![],
        handlers: vec![],
        navigation: vec![],
        behavior: None,
        presentation: Some(Presentation::Oib(contract::Seed {
            format: contract::FORMAT.into(),
            designer: Some(doc),
            main_oib: 1,
            main_class: "otrios.declarative-v1".into(),
            components: [10, 11, 12]
                .into_iter()
                .map(|id| contract::Object {
                    id,
                    name: format!("def-{id}"),
                    component: None,
                })
                .collect(),
            instances: [20, 30]
                .into_iter()
                .map(|id| contract::Object {
                    id,
                    name: format!("panel-{id}"),
                    component: Some(10),
                })
                .collect(),
            cells,
            inspector_viewport: 3,
            inspector_content: [500., 1000.],
            instance_surface: Some(contract::InstanceSurface {
                layout: None,
                previous_ui_sha256: vec![0; 32],
                preview_viewport: 2,
                preview_stride: 100.,
                inspector_start: 100.,
                inspector_stride: 100.,
                prototypes: vec![
                    contract::Prototype {
                        instance: 20,
                        preview_root: 10,
                        inspector_root: 30,
                        nodes: vec![10, 11, 12, 13, 14, 15, 16, 30],
                    },
                    contract::Prototype {
                        instance: 30,
                        preview_root: 20,
                        inspector_root: 31,
                        nodes: vec![20, 21, 22, 23, 24, 25, 26, 31],
                    },
                ],
                node_base: 40,
                node_stride: 8,
            }),
        })),
    }
}
fn seed(ui: &Ui) -> &contract::Seed {
    oib::seed(ui).unwrap()
}
fn seed_mut(ui: &mut Ui) -> &mut contract::Seed {
    match ui.presentation.as_mut().unwrap() {
        Presentation::Oib(s) => s,
        _ => unreachable!(),
    }
}

#[test]
fn full_paths_keys_and_nested_live_precedence() {
    let ui = fixture();
    oib::validate(&ui).unwrap();
    let seed = seed(&ui);
    let allocations = oib::instances::initial_allocations(seed);
    assert_eq!(
        oib::document::cell_for_address(seed, &allocations, &address(20, &[7])).unwrap(),
        201
    );
    assert_eq!(
        oib::document::cell_for_address(seed, &allocations, &address(20, &[8])).unwrap(),
        202
    );
    assert!(oib::instances::cell_id(seed, &allocations, 20, 101).is_err());
    let mut values = BTreeMap::new();
    values.insert(100, Some(text("Shared")));
    values.insert(101, Some(text("Child shared")));
    assert_eq!(oib::effective(seed, &values, 201).unwrap(), text("Audio"));
    values.insert(200, Some(text("Live outer")));
    assert_eq!(
        oib::effective(seed, &values, 201).unwrap(),
        text("Live outer")
    );
    values.insert(201, Some(text("Live outer")));
    values.insert(200, Some(text("New outer")));
    assert_eq!(
        oib::effective(seed, &values, 201).unwrap(),
        text("Live outer")
    );
    assert_eq!(
        oib::effective(seed, &values, 202).unwrap(),
        text("New outer")
    );
    values.insert(201, None);
    assert_eq!(
        oib::effective(seed, &values, 201).unwrap(),
        text("New outer")
    );
    values.insert(200, None);
    assert_eq!(oib::effective(seed, &values, 201).unwrap(), text("Audio"));
    let props = oib::document::properties(
        seed,
        &allocations,
        &Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }),
        &values,
    )
    .unwrap();
    assert_eq!(props[0].source, "forwarded");
    assert_eq!(props[0].inherited, Some(text("Audio")));
    let objects = oib::document::objects(seed, &allocations).unwrap();
    assert!(objects.iter().any(|o| o.layout_node.is_none()
        && matches!(&o.selection,Selection::Object(a) if a.instance.0==40&&a.node.0==2)));
}

#[test]
fn sparse_key_identity_survives_cell_order_and_names() {
    let mut ui = fixture();
    let mut allocations = oib::instances::initial_allocations(seed(&ui));
    allocations.push(contract::Allocation {
        id: 65536,
        name: "dynamic".into(),
        component: 10,
        prototype: 20,
        slot: 7,
    });
    let expected = 16777216 + 55;
    assert_eq!(
        oib::document::cell_for_address(seed(&ui), &allocations, &address(65536, &[8])).unwrap(),
        expected
    );
    seed_mut(&mut ui).cells.reverse();
    seed_mut(&mut ui).designer.as_mut().unwrap().definitions[0].nodes[1].name = "Renamed".into();
    oib::validate(&ui).unwrap();
    let expanded = oib::instances::expanded_seed(seed(&ui), &allocations).unwrap();
    assert_eq!(
        oib::document::cell_for_address(seed(&ui), &allocations, &address(65536, &[8])).unwrap(),
        expected
    );
    assert!(expanded.cells.iter().any(|c| c.id == expected));
    assert_eq!(
        oib::effective(&expanded, &BTreeMap::new(), expected).unwrap(),
        text("Audio")
    );
    assert_eq!(
        oib::document::cell_for_alias(seed(&ui), &allocations, 65536, 201).unwrap(),
        16777216 + 17
    );
    allocations.last_mut().unwrap().id = u64::MAX;
    assert!(oib::instances::expanded_seed(seed(&ui), &allocations).is_err());
}

#[test]
fn malformed_graph_mapping_and_types_reject() {
    let base = fixture();
    let mut bad = base.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().mappings.roots[0].cells[1].key = 2;
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().mappings.roots[0].cells[1].path = path(&[8]);
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().definitions[2].nodes[0].reference =
        Some(ComponentReference {
            definition: DefinitionId(10),
            assignments: vec![],
        });
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().definitions[0].exposed[0]
        .targets
        .push(path(&[]));
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).cells[2].default = Value::Boolean(false);
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).cells[3].targets[0].node = 23;
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().classes[3].properties[0].value_type =
        ValueType::Series;
    assert!(oib::validate(&bad).is_err());
    let mut bad = base.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().instances[0].assignments[0].source =
        AssignmentSource::Forward(path(&[]));
    assert!(oib::validate(&bad).is_err());
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
        oib::effective(seed(&ui), &a.oib_active().unwrap(), 201).unwrap(),
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
    assert_eq!(doc.oib_status().unwrap().conflicts, vec![201]);
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    assert_eq!(doc.oib_values(2).unwrap(), values);
    let saved = doc.save().unwrap();
    let mut loaded = StateDocument::load(ui.state_schema.clone(), "oib:runtime", &saved).unwrap();
    loaded.configure_oib(&ui).unwrap();
    assert_eq!(loaded.oib_status().unwrap().active_root, active);
}
#[test]
fn designer_none_canonical_bytes_and_legacy_inheritance_stay_unchanged() {
    let bytes =
        lz4_flex::decompress_size_prepended(include_bytes!("fixtures/oib-v25-ui.lz4")).unwrap();
    let ui = canvas::decode_ui(&bytes).unwrap();
    assert!(seed(&ui).designer.is_none());
    assert_eq!(canvas::encode_ui(&ui).unwrap(), bytes);
    oib::validate(&ui).unwrap();
    let mut doc = StateDocument::new(ui.state_schema.clone(), "oib:legacy-runtime").unwrap();
    doc.configure_oib(&ui).unwrap();
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
}

#[test]
fn canonical_seed_decoder_rejects_noncanonical_and_oversized_metadata() {
    let ui = fixture();
    let encoded = flexbuffers::to_vec(seed(&ui)).unwrap();
    assert_eq!(oib::document::decode_seed(&encoded).unwrap(), *seed(&ui));
    assert!(oib::document::decode_seed(&vec![0; 24 * 1024 + 1]).is_err());
    let mut bad = seed(&ui).clone();
    bad.designer.as_mut().unwrap().mappings.roots[0]
        .cells
        .reverse();
    assert!(oib::document::decode_seed(&flexbuffers::to_vec(&bad).unwrap()).is_err());
    let mut bad = seed(&ui).clone();
    bad.cells[3].targets[0].offset = -0.;
    assert!(oib::document::decode_seed(&flexbuffers::to_vec(&bad).unwrap()).is_err());
    let mut bad = seed(&ui).clone();
    bad.designer.as_mut().unwrap().classes[3].properties[0].label = "a\0b".into();
    assert!(oib::document::decode_seed(&flexbuffers::to_vec(&bad).unwrap()).is_err());
}

fn larger_fixture() -> Ui {
    let mut ui = fixture();
    let seed = seed_mut(&mut ui);
    for p in 2..=5 {
        let mut descriptor = descriptor();
        descriptor.id = PropertyId(p);
        descriptor.name = format!("extra{p}");
        seed.designer.as_mut().unwrap().classes[3]
            .properties
            .push(descriptor);
        for def in [10, 12] {
            let id = 1000 + def * 10 + p;
            seed.designer
                .as_mut()
                .unwrap()
                .mappings
                .definitions
                .push(DefinitionCellMapping {
                    definition: DefinitionId(def),
                    node: NodeId(1),
                    property: PropertyId(p),
                    cell: id,
                });
            seed.cells.push(contract::Cell {
                id,
                owner: def,
                name: format!("extra{p}"),
                default: text("Default"),
                authored_override: None,
                choices: vec![text("Other")],
                inherit: None,
                targets: vec![],
            });
        }
        for root in [20, 30] {
            // Each extra property exists at all three addressed panel roots.
            for (rank, refs) in [(0, vec![]), (1, vec![7]), (2, vec![8])] {
                let id = 2000 + root * 100 + p * 3 + rank;
                let mut path = path(&refs);
                path.property = PropertyId(p);
                seed.designer
                    .as_mut()
                    .unwrap()
                    .mappings
                    .roots
                    .iter_mut()
                    .find(|t| t.prototype.0 == root)
                    .unwrap()
                    .cells
                    .push(RootCellKey {
                        path,
                        key: (3 + (p - 2) * 3 + rank) as u8,
                        initial_cell: id,
                    });
                seed.cells.push(contract::Cell {
                    id,
                    owner: root,
                    name: format!("extra{p}"),
                    default: text("Default"),
                    authored_override: None,
                    choices: vec![text("Other")],
                    inherit: Some(1000 + if rank == 0 { 10 } else { 12 } * 10 + p),
                    targets: vec![],
                });
            }
        }
    }
    let doc = seed.designer.as_mut().unwrap();
    doc.mappings
        .definitions
        .sort_by_key(|m| (m.definition, m.node, m.property));
    for t in &mut doc.mappings.roots {
        t.cells.sort_by_key(|r| r.key);
    }
    ui
}
#[test]
fn resident_cell_union_rejects_before_identity_allocation() {
    // 10 shared +15 cells/root: three resident roots fit (55), four do not (70),
    // independently of the ten-descriptor and six-live-root bounds.
    let ui = larger_fixture();
    oib::validate(&ui).unwrap();
    let mut doc = StateDocument::new(ui.state_schema.clone(), "oib:runtime").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut client, mut session) = adapter(&ui, &mut doc, 1);
    client.instantiate_oib(10, None).unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(doc.oib_allocations().unwrap().len(), 3);
    let before = doc.root_hash();
    let ids = doc.oib_allocations().unwrap();
    assert!(!client.instantiate_oib(10, None).unwrap());
    assert_eq!(doc.root_hash(), before);
    assert_eq!(doc.oib_allocations().unwrap(), ids);
    let (_, mut bypass) = adapter(&ui, &mut doc, 9);
    let before = doc.root_hash();
    let request = canvas::encode_state_request(&StateRequest::OibInstance {
        sequence: 1,
        operation: contract::InstanceOperation::Instantiate {
            component: 10,
            name: None,
        },
    })
    .unwrap();
    assert!(
        doc.receive(&mut bypass, &request)
            .unwrap_err()
            .contains("property capacity")
    );
    assert_eq!(doc.root_hash(), before);
    let previous_id = ids.last().unwrap().id;
    client.remove_oib(previous_id).unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.instantiate_oib(10, None).unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(
        doc.oib_allocations().unwrap().last().unwrap().id,
        previous_id + 1
    );
}

fn toggle_fixture() -> Ui {
    let mut ui = fixture();
    ui.state_schema.fields.push(Field {
        id: 2,
        name: "second-toggle".into(),
        default: Value::Number(1000.),
        writable: true,
    });
    ui.selectors = [1, 2]
        .into_iter()
        .map(|id| Selector {
            local: id,
            field: id,
            projection: Projection::Nonzero,
        })
        .collect();
    ui.handlers = [1, 2]
        .into_iter()
        .map(|id| Handler {
            control: id as u8,
            event: id,
            kind: GestureKind::Activate,
            operation: Operation::Toggle,
            fields: vec![id],
        })
        .collect();
    let mut blueprint = Layout::from_bytes(&ui.layout).unwrap().blueprint().clone();
    for (base, control) in [(10, 1), (20, 2)] {
        let mut toggle = node(
            base + 7,
            [0., 50., 100., 40.],
            Shape::Canvas { clip: false },
            vec![],
        );
        toggle.responder = Some(Responder::AppEvent {
            control,
            drag: false,
        });
        toggle.semantics = Some(Semantics {
            label: Expr::Literal(text("Toggle")),
            checked: Expr::State(Binding::Local(u16::from(control))),
        });
        blueprint.templates[0].root.children[0]
            .children
            .iter_mut()
            .find(|n| n.id == base)
            .unwrap()
            .children
            .push(toggle);
    }
    ui.layout = Layout::encode(&blueprint).unwrap();
    let seed = seed_mut(&mut ui);
    let doc = seed.designer.as_mut().unwrap();
    let mut checked = descriptor();
    checked.id = PropertyId(2);
    checked.name = "checked".into();
    checked.value_type = ValueType::Boolean;
    checked.default = Value::Boolean(false);
    checked.editor = PropertyEditor::ReadOnly;
    checked.policy = PropertyPolicy {
        live_literal: false,
        app_binding: true,
    };
    doc.classes
        .push(class(6, ClassKind::O2DToggle, Some(3), vec![checked]));
    doc.definitions[0]
        .nodes
        .push(definition_node(9, 6, Some(1), &[], None));
    doc.definitions[0].nodes[0].visual_children.push(NodeId(9));
    for (root, base, local) in [(20, 10, 1), (30, 20, 2)] {
        let path = PropertyPath {
            references: vec![],
            node: NodeId(9),
            property: PropertyId(2),
        };
        doc.instances
            .iter_mut()
            .find(|i| i.id.0 == root)
            .unwrap()
            .assignments
            .push(Assignment {
                target: path,
                source: AssignmentSource::AppBinding(Binding::Local(local)),
            });
        doc.mappings.objects.push(ObjectMapping {
            address: ObjectAddress {
                instance: InstanceId(root),
                references: vec![],
                node: NodeId(9),
            },
            layout_node: Some(base + 7),
        });
        let prototype = seed
            .instance_surface
            .as_mut()
            .unwrap()
            .prototypes
            .iter_mut()
            .find(|p| p.instance == root)
            .unwrap();
        prototype.nodes.push(base + 7);
        prototype.nodes.sort();
    }
    seed.instance_surface.as_mut().unwrap().node_stride = 9;
    doc.mappings
        .objects
        .sort_by(|a, b| a.address.cmp(&b.address));
    ui
}
#[test]
fn toggle_binding_admission_projection_and_dynamic_remap() {
    let ui = toggle_fixture();
    oib::validate(&ui).unwrap();
    let allocations = oib::instances::initial_allocations(seed(&ui));
    let readonly = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![],
            node: NodeId(9),
            property: PropertyId(2),
        },
    };
    assert!(oib::document::cell_for_address(seed(&ui), &allocations, &readonly).is_err());
    let properties = oib::document::properties(
        seed(&ui),
        &allocations,
        &Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![],
            node: NodeId(9),
        }),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(properties[0].source, "bound");
    assert_eq!(properties[0].cell, None);
    assert_eq!(properties[0].value, None);
    let mut allocations = allocations;
    allocations.push(contract::Allocation {
        id: 65536,
        name: "Toggle_copy".into(),
        component: 10,
        prototype: 20,
        slot: 7,
    });
    let expanded = oib::instances::expand(
        &ui,
        &allocations,
        &[20, 30, 65536].into_iter().collect(),
        None,
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        expanded
            .handlers
            .iter()
            .any(|h| h.control > 2 && h.operation == Operation::Toggle && h.fields == vec![1])
    );
    let layout = oib::compile(&expanded, &BTreeMap::new()).unwrap();
    for checked in [false, true] {
        let slots = BTreeMap::from([(1, Value::Boolean(checked)), (2, Value::Boolean(true))]);
        let scene = layout.resolve_slots(&slots).unwrap();
        assert!(
            scene
                .controls()
                .iter()
                .filter(|p| p
                    .node
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.label == "Toggle"))
                .all(|p| p.node.semantics.as_ref().unwrap().checked
                    == (if p.node.id == 27 { true } else { checked }))
        );
    }
    let mut bad = ui.clone();
    bad.handlers[0].fields = vec![1, 2];
    assert!(oib::validate(&bad).is_err());
    let mut bad = ui.clone();
    bad.handlers[0].operation = Operation::Back;
    assert!(oib::validate(&bad).is_err());
    let mut bad = ui.clone();
    bad.selectors[0].projection = Projection::Identity;
    assert!(oib::validate(&bad).is_err());
    let mut bad = ui.clone();
    seed_mut(&mut bad).designer.as_mut().unwrap().instances[0].assignments[1].source =
        AssignmentSource::Literal(Value::Boolean(true));
    assert!(oib::validate(&bad).is_err());
    let mut doc = StateDocument::new(ui.state_schema.clone(), "oib:runtime").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut client, _) = adapter(&ui, &mut doc, 1);
    assert!(
        client
            .edit_oib_property(&readonly, Some(Value::Boolean(true)))
            .is_err()
    );
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
    assert_eq!(doc.oib_values(2).unwrap()[&201], Some(text("Custom")));
    client.remove_oib(20).unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.restore_oib(true).unwrap();
    sync(&mut doc, &mut session, &mut client);
    let allocations = doc.oib_allocations().unwrap();
    assert_eq!(allocations.len(), 2);
    assert!(!allocations.iter().any(|a| a.id == 20));
    let fresh = allocations.iter().find(|a| a.prototype == 20).unwrap().id;
    assert!(fresh >= 65536);
    let cell =
        oib::document::cell_for_address(seed(&ui), &allocations, &address(fresh, &[7])).unwrap();
    let expanded = oib::instances::expanded_seed(seed(&ui), &allocations).unwrap();
    assert_eq!(
        oib::effective(&expanded, &doc.oib_values(2).unwrap(), cell).unwrap(),
        text("Audio")
    );
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(0.));
}

#[test]
fn terminal_compilation_is_independent_of_cell_vector_order() {
    let mut ui = fixture();
    let seed = seed_mut(&mut ui);
    seed.cells
        .iter_mut()
        .find(|c| c.id == 101)
        .unwrap()
        .targets
        .push(contract::Target {
            node: 13,
            property: contract::Property::Text,
            scale: 1.,
            offset: 0.,
        });
    oib::validate(&ui).unwrap();
    let values = BTreeMap::from([
        (101, Some(text("Shared child"))),
        (200, Some(text("Resolved parent"))),
    ]);
    let expected = oib::compile(&ui, &values).unwrap().blueprint().clone();
    seed_mut(&mut ui).cells.reverse();
    let actual = oib::compile(&ui, &values).unwrap().blueprint().clone();
    assert_eq!(actual, expected);
    let leaf =
        &actual.templates[0].root.children[0].children[0].children[0].children[0].children[0];
    assert!(
        matches!(&leaf.shape,Shape::Text{value:Expr::Literal(Value::Text(v)),..} if v=="Resolved parent")
    );
}

#[test]
fn allowlisted_signed_successor_preserves_keys_registers_and_values() {
    let (ui, mut doc) = setup();
    let (mut client, mut session) = adapter(&ui, &mut doc, 1);
    client
        .edit_oib_property(&address(20, &[7]), Some(text("Retained")))
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    let values = doc.oib_values(2).unwrap();
    let allocations = doc.oib_allocations().unwrap();
    let mut next = ui.clone();
    let old_hash = canvas::hash(&canvas::encode_ui(&ui).unwrap());
    let seed = seed_mut(&mut next);
    seed.instance_surface.as_mut().unwrap().previous_ui_sha256 = old_hash;
    seed.designer.as_mut().unwrap().definitions[0].nodes[1].name = "Renamed wrapper".into();
    seed.cells.reverse();
    assert!(doc.configure_oib(&next).unwrap());
    assert_eq!(doc.oib_values(2).unwrap(), values);
    assert_eq!(doc.oib_allocations().unwrap(), allocations);
    let mut bad = next.clone();
    let hash = canvas::hash(&canvas::encode_ui(&next).unwrap());
    let seed = seed_mut(&mut bad);
    seed.instance_surface.as_mut().unwrap().previous_ui_sha256 = hash;
    seed.designer.as_mut().unwrap().mappings.roots[0].cells[1].key = 18;
    let before = doc.root_hash();
    assert!(doc.configure_oib(&bad).is_err());
    assert_eq!(doc.root_hash(), before);
}

#[test]
fn inspector_exposed_rows_precede_base_to_derived_group_order() {
    let mut ui = larger_fixture();
    let doc = seed_mut(&mut ui).designer.as_mut().unwrap();
    let mut readonly = descriptor();
    readonly.id = PropertyId(9);
    readonly.name = "identity".into();
    readonly.editor = PropertyEditor::ReadOnly;
    readonly.policy.live_literal = false;
    readonly.group = InspectorGroup::Identity;
    doc.classes[2].properties.push(readonly);
    for p in &mut doc.classes[3].properties {
        if p.id.0 == 2 {
            p.group = InspectorGroup::Appearance;
        }
        if p.id.0 == 3 {
            p.group = InspectorGroup::Placement;
        }
    }
    oib::validate(&ui).unwrap();
    let rows = oib::document::properties(
        seed(&ui),
        &oib::instances::initial_allocations(seed(&ui)),
        &Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![],
            node: NodeId(1),
        }),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        rows.iter().map(|p| p.descriptor.id.0).collect::<Vec<_>>(),
        vec![1, 9, 3, 2, 4, 5]
    );
    assert_eq!(rows[1].cell, None);
}
