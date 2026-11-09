// SPDX-License-Identifier: LGPL-3.0-only
//! D1 sample admission, authoring and source-resolution checks. These are
//! in-process checks, not authority-process, Linux IPC or live acceptance.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/live-ui/app.rs"]
mod live_ui;
use otrios_core::{
    canvas::{Document as StateDocument, Session, StateAdapter, StateSelection, oib},
    dashboard::layout::Layout,
};
use otrios_interfaces::{
    canvas::{Operation, Presentation, Projection, Ui},
    layout::{Binding, Expr, TemplateNode, Value},
    oib::{Allocation, MAX_CELLS, designer::*},
};
use std::collections::{BTreeMap, BTreeSet};

fn template(ui: &Ui) -> TemplateNode {
    Layout::from_bytes(&ui.layout)
        .unwrap()
        .blueprint()
        .templates[0]
        .root
        .clone()
}
fn find(root: &TemplateNode, id: u16) -> &TemplateNode {
    if root.id == id {
        return root;
    }
    root.children
        .iter()
        .find_map(|c| find_optional(c, id))
        .expect("mapped layout node")
}
fn find_optional(root: &TemplateNode, id: u16) -> Option<&TemplateNode> {
    if root.id == id {
        Some(root)
    } else {
        root.children.iter().find_map(|c| find_optional(c, id))
    }
}
fn count(root: &TemplateNode, depth: usize) -> (usize, usize) {
    root.children
        .iter()
        .map(|c| count(c, depth + 1))
        .fold((1, depth), |(n, d), (cn, cd)| (n + cn, d.max(cd)))
}
fn fixture_seed(ui: &Ui) -> Option<&otrios_interfaces::oib::Seed> {
    match &ui.presentation {
        Some(Presentation::Oib(seed)) => Some(seed),
        _ => None,
    }
}
fn document(ui: &Ui) -> &otrios_interfaces::oib::designer::Document {
    fixture_seed(ui).unwrap().designer.as_ref().unwrap()
}
fn path(references: &[u64], node: u64, property: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
        property: PropertyId(property),
    }
}
fn source(ui: &Ui, instance: u64, path: &PropertyPath) -> Binding {
    let row = document(ui)
        .instances
        .iter()
        .find(|i| i.id == InstanceId(instance))
        .unwrap()
        .assignments
        .iter()
        .find(|a| &a.target == path)
        .unwrap();
    let AssignmentSource::AppBinding(binding) = row.source else {
        panic!("signed app binding")
    };
    binding
}

#[test]
fn signed_sample_has_one_paint_tree_and_three_empty_tooling_surfaces() {
    let ui = app::ui(&[]);
    assert!(app::manifest().dependencies.iter().any(|d| d == FEATURE));
    let seed = fixture_seed(&ui).unwrap();
    let doc = document(&ui);
    assert_eq!(doc.presented_root, InstanceId(40));
    assert_eq!(doc.definitions.len(), 4);
    assert_eq!(
        seed.instances.iter().map(|i| i.id).collect::<Vec<_>>(),
        vec![20, 30, 50]
    );
    assert!(
        doc.mappings
            .roots
            .iter()
            .all(|r| r.prototype != InstanceId(40))
    );
    assert!(seed.cells.iter().all(|c| c.owner != 40));
    let root = template(&ui);
    let hierarchy = doc.hierarchy_viewport.unwrap();
    assert!(find(&root, hierarchy).children.is_empty());
    let surface = seed.instance_surface.as_ref().unwrap();
    for prototype in &surface.prototypes {
        assert!(prototype.inspector_root.is_none());
    }
    let preview = doc
        .mappings
        .objects
        .iter()
        .find(|m| {
            m.address
                == ObjectAddress {
                    instance: InstanceId(40),
                    references: vec![],
                    node: NodeId(1),
                }
        })
        .unwrap()
        .layout_node
        .unwrap();
    assert_eq!(preview, surface.preview_viewport);
    let config = doc
        .mappings
        .objects
        .iter()
        .find(|m| m.address.instance == InstanceId(40) && m.address.node == NodeId(2))
        .unwrap();
    assert!(config.layout_node.is_none());
    let ids: Vec<_> = doc
        .mappings
        .objects
        .iter()
        .filter_map(|m| m.layout_node)
        .collect();
    assert_eq!(
        ids.len(),
        ids.iter().copied().collect::<BTreeSet<_>>().len(),
        "references map wrappers and inherited roots to distinct real nodes"
    );
    for mapping in &doc.mappings.objects {
        if let Some(id) = mapping.layout_node {
            find(&root, id);
        }
    }
    for table in &doc.mappings.roots {
        for row in &table.cells {
            let owner = doc
                .mappings
                .objects
                .iter()
                .find(|m| {
                    m.address
                        == ObjectAddress {
                            instance: table.prototype,
                            references: row.path.references.clone(),
                            node: row.path.node,
                        }
                })
                .unwrap();
            let subtree = find(&root, owner.layout_node.unwrap());
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == row.initial_cell)
                .unwrap();
            for target in &cell.targets {
                assert!(find_optional(subtree, target.node).is_some());
            }
        }
    }
    assert_eq!(
        ui.handlers.len(),
        7,
        "three controls per panel and one signed palette toggle; no tooling handlers"
    );
}

#[test]
fn canonical_metadata_and_sparse_aliases_do_not_use_names_or_vector_rank() {
    let ui = app::ui(&[]);
    let doc = document(&ui);
    assert!(doc.classes.windows(2).all(|w| w[0].id < w[1].id));
    assert!(doc.definitions.windows(2).all(|w| w[0].id < w[1].id));
    assert!(doc.instances.windows(2).all(|w| w[0].id < w[1].id));
    for class in &doc.classes {
        assert!(class.properties.windows(2).all(|w| w[0].id < w[1].id));
    }
    for def in &doc.definitions {
        assert!(def.nodes.windows(2).all(|w| w[0].id < w[1].id));
        for node in &def.nodes {
            assert!(
                node.defaults
                    .windows(2)
                    .all(|w| w[0].property < w[1].property)
            );
        }
    }
    assert!(
        doc.mappings
            .objects
            .windows(2)
            .all(|w| w[0].address < w[1].address)
    );
    assert!(doc.mappings.definitions.windows(2).all(|w| (
        w[0].definition,
        w[0].node,
        w[0].property
    ) < (
        w[1].definition,
        w[1].node,
        w[1].property
    )));
    for (factory, start) in [(20, 200), (30, 300)] {
        let table = doc
            .mappings
            .roots
            .iter()
            .find(|r| r.prototype == InstanceId(factory))
            .unwrap();
        assert_eq!(
            table
                .cells
                .iter()
                .map(|r| r.initial_cell)
                .collect::<Vec<_>>(),
            (start..start + 6).collect::<Vec<_>>()
        );
        assert_eq!(
            table
                .cells
                .iter()
                .map(|r| r.path.clone())
                .collect::<Vec<_>>(),
            vec![
                path(&[], 1, 1),
                path(&[7], 1, 1),
                path(&[7], 1, 2),
                path(&[7], 1, 3),
                path(&[8], 1, 1),
                path(&[8], 1, 3)
            ]
        );
    }
    let mut successor = doc.clone();
    for def in &mut successor.definitions {
        def.name = format!("Renamed {}", def.id.0);
        for node in &mut def.nodes {
            node.name = format!("Node {}", node.id.0);
            node.visual_children.reverse();
        }
    }
    assert_eq!(
        doc.mappings, successor.mappings,
        "rename/paint ordering cannot allocate identities"
    );
}

#[test]
fn signed_bindings_and_checked_semantics_use_distinct_numeric_fields() {
    let ui = app::ui(&[]);
    let old = live_ui::ui(&[]);
    assert_eq!(live_ui::manifest().id, "live-ui");
    assert_eq!(old.state_schema.id, "live-ui-state-v2");
    assert_eq!(old.state_schema.fields.len(), 2);
    assert_eq!(ui.state_schema.id, "oib-designer-state-v2");
    assert_eq!(
        ui.state_schema
            .fields
            .iter()
            .map(|f| (f.id, f.default.clone()))
            .collect::<Vec<_>>(),
        vec![
            (1, Value::Number(400.)),
            (2, Value::Number(700.)),
            (3, Value::Number(1000.)),
            (4, Value::Number(0.))
        ]
    );
    let root = template(&ui);
    for (instance, value_field, toggle_field) in [(20, 1, 3), (30, 2, 4)] {
        let Binding::Local(value) = source(&ui, instance, &path(&[7], 1, 4));
        let selector = ui.selectors.iter().find(|s| s.local == value).unwrap();
        assert_eq!(
            (selector.field, &selector.projection),
            (value_field, &Projection::Identity)
        );
        let Binding::Local(checked) = source(&ui, instance, &path(&[8], 1, 5));
        let selector = ui.selectors.iter().find(|s| s.local == checked).unwrap();
        assert_eq!(
            (selector.field, &selector.projection),
            (toggle_field, &Projection::Nonzero)
        );
        let mapped = document(&ui)
            .mappings
            .objects
            .iter()
            .find(|m| {
                m.address
                    == ObjectAddress {
                        instance: InstanceId(instance),
                        references: vec![NodeId(8)],
                        node: NodeId(1),
                    }
            })
            .unwrap();
        let toggle = find(&root, mapped.layout_node.unwrap());
        assert_eq!(
            toggle.semantics.as_ref().unwrap().checked,
            Expr::State(Binding::Local(checked))
        );
    }
    assert_eq!(
        ui.handlers
            .iter()
            .filter(|h| h.operation == Operation::Toggle)
            .map(|h| h.fields.clone())
            .collect::<Vec<_>>(),
        vec![vec![3], vec![3], vec![4], vec![4], vec![3]]
    );
    assert!(
        document(&ui)
            .classes
            .iter()
            .flat_map(|c| &c.properties)
            .filter(|p| p.policy.app_binding)
            .all(|p| !p.policy.live_literal)
    );
}

#[test]
fn wide_and_portrait_tooling_keep_authored_inner_geometry_and_dynamic_checked_state() {
    let ui = app::ui(&[]);
    // Explicit initial-state oracle for this fixture, independent of the Canvas
    // projection evaluator. Resolve the actual covered layout/scene machinery.
    let slots: BTreeMap<_, _> = ui
        .selectors
        .iter()
        .map(|s| {
            let number = match s.field {
                1 => 400.,
                2 => 700.,
                3 => 1000.,
                4 => 0.,
                _ => panic!("fixture field"),
            };
            let value = match &s.projection {
                Projection::Identity => Value::Number(number),
                Projection::Nonzero => Value::Boolean(s.field == 3),
                Projection::Percent => Value::Text(if s.field == 1 { "40%" } else { "70%" }.into()),
                Projection::Number { scale, offset } => Value::Number(number * scale + offset),
                _ => panic!("unexpected fixture projection"),
            };
            (s.local, value)
        })
        .collect();
    let layout = Layout::from_bytes(&ui.layout).unwrap();
    for viewport in [[1280., 800.], [800., 1280.]] {
        let scene = layout.resolve_viewport(&slots, viewport).unwrap();
        for (instance, checked) in [(20, true), (30, false)] {
            let mapped = |refs: Vec<NodeId>| {
                document(&ui)
                    .mappings
                    .objects
                    .iter()
                    .find(|m| {
                        m.address
                            == ObjectAddress {
                                instance: InstanceId(instance),
                                references: refs.clone(),
                                node: NodeId(1),
                            }
                    })
                    .unwrap()
                    .layout_node
                    .unwrap()
            };
            let panel = scene.path(u32::from(mapped(vec![]))).unwrap();
            let panel = panel.last().unwrap().node;
            assert_eq!([panel.frame.width, panel.frame.height], [520., 280.]);
            let slider = scene.path(u32::from(mapped(vec![NodeId(7)]))).unwrap();
            let slider = slider.last().unwrap().node;
            assert_eq!([slider.frame.width, slider.frame.height], [440., 112.]);
            let toggle = scene.path(u32::from(mapped(vec![NodeId(8)]))).unwrap();
            assert_eq!(
                toggle
                    .last()
                    .unwrap()
                    .node
                    .semantics
                    .as_ref()
                    .unwrap()
                    .checked,
                checked
            );
        }
    }
}

#[test]
fn sample_admission_fits_signed_budgets_and_preflights_maximum_renderer_slot() {
    let ui = app::ui(&[]);
    oib::validate(&ui).unwrap();
    let seed = fixture_seed(&ui).unwrap();
    let doc = document(&ui);
    let initial = oib::instances::initial_allocations(seed);
    for (instance, references, property) in [(20, vec![NodeId(7)], 4), (30, vec![NodeId(8)], 5)] {
        let object = ObjectAddress {
            instance: InstanceId(instance),
            references,
            node: NodeId(1),
        };
        let properties = oib::document::properties(
            seed,
            &initial,
            &Selection::Object(object.clone()),
            &BTreeMap::new(),
        )
        .unwrap();
        let bound = properties
            .iter()
            .find(|p| p.descriptor.id == PropertyId(property))
            .unwrap();
        assert!(
            bound.cell.is_none(),
            "app connections are read-only to UI literals"
        );
        assert!(
            oib::document::cell_for_address(
                seed,
                &initial,
                &PropertyAddress::Occurrence {
                    instance: object.instance,
                    path: PropertyPath {
                        references: object.references,
                        node: object.node,
                        property: PropertyId(property)
                    }
                }
            )
            .is_err()
        );
    }
    let authored = 1
        + doc.definitions.len()
        + doc.definitions.iter().map(|d| d.nodes.len()).sum::<usize>()
        + doc.instances.len();
    assert_eq!(authored, 27);
    assert!(flexbuffers::to_vec(seed).unwrap().len() <= 24 * 1024);
    assert!(ui.layout.len() <= 128 * 1024);
    let allocations: Vec<_> = (0..10)
        .map(|slot| Allocation {
            id: 0x1_0000 + u64::from(slot),
            name: format!("Panel {slot}"),
            component: 10,
            prototype: if slot % 2 == 0 { 20 } else { 30 },
            slot,
        })
        .collect();
    // Nine resident closures fit 60 cells. Ten need 66, and must reject even
    // when only six roots are live; resident retention is not a larger cell cap.
    let resident = &allocations[1..];
    assert_eq!(
        oib::instances::expanded_seed(seed, resident)
            .unwrap()
            .cells
            .len(),
        60
    );
    assert!(oib::instances::expanded_seed(seed, &allocations).is_err());
    let live: BTreeSet<_> = resident.iter().rev().take(6).map(|a| a.id).collect();
    let expanded = oib::instances::expand(
        &ui,
        resident,
        &live,
        Some(allocations[9].id),
        &BTreeMap::new(),
    )
    .unwrap();
    let layout = oib::compile(&expanded, &BTreeMap::new()).unwrap();
    let root = &layout.blueprint().templates[0].root;
    let (nodes, depth) = count(root, 0);
    assert!(nodes <= 512 && depth <= 12);
    assert!(expanded.layout.len() <= 128 * 1024);
    assert!(expanded.handlers.len() <= 128 && expanded.selectors.len() <= 128);
    let table = &doc.mappings.roots[1];
    for (rank, row) in table.cells.iter().enumerate() {
        let lowered = oib::document::cell_for_address(
            seed,
            resident,
            &PropertyAddress::Occurrence {
                instance: InstanceId(allocations[9].id),
                path: row.path.clone(),
            },
        )
        .unwrap();
        assert_eq!(
            lowered,
            seed.cells.iter().map(|c| c.id).max().unwrap()
                + 1
                + u64::from(allocations[9].slot) * 64
                + rank as u64
        );
    }
    let six = oib::instances::expanded_seed(seed, &allocations[..6]).unwrap();
    assert_eq!(six.cells.len(), 42);
    assert!(six.cells.len() <= MAX_CELLS);
    eprintln!(
        "D1 budgets: seed={} layout={} authored={} initial nodes={:?}; six-live/high-slot layout={} nodes={} depth={} handlers={} selectors={}",
        flexbuffers::to_vec(seed).unwrap().len(),
        ui.layout.len(),
        authored,
        count(&template(&ui), 0),
        expanded.layout.len(),
        nodes,
        depth,
        expanded.handlers.len(),
        expanded.selectors.len()
    );
}

fn setup() -> (Ui, StateDocument, StateAdapter, Session) {
    let ui = app::ui(&[]);
    let mut doc = StateDocument::new(ui.state_schema.clone(), "designer:fixture").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut session, _) = doc.open_session(&[1; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(&ui, "designer:fixture", &[], [1; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    (ui, doc, client, session)
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
    panic!("fixture sync did not settle");
}
#[test]
fn forwarding_nested_override_reset_and_recovery_preserve_authored_labels_and_app_values() {
    let (ui, mut doc, mut client, mut session) = setup();
    client
        .set(&[(1, 481), (2, 729), (3, 0), (4, 1000)])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    let gold = Value::Color([218, 165, 32, 255]);
    let orange = Value::Color([255, 136, 85, 255]);
    client
        .edit_oib(&[(100, Some(gold.clone())), (102, Some(Value::Number(400.)))])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    let effective = |doc: &StateDocument, cell| {
        let allocations = doc.oib_allocations().unwrap();
        let expanded =
            oib::instances::expanded_seed(fixture_seed(&ui).unwrap(), &allocations).unwrap();
        oib::effective(&expanded, &doc.oib_values(2).unwrap(), cell).unwrap()
    };
    for cell in [201, 204, 301, 304] {
        assert_eq!(effective(&doc, cell), gold);
    }
    for cell in [202, 302] {
        assert_eq!(effective(&doc, cell), Value::Number(400.));
    }
    client.edit_oib(&[(201, Some(orange.clone()))]).unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(effective(&doc, 201), orange);
    assert_eq!(effective(&doc, 301), gold);
    client
        .edit_oib(&[(201, None), (103, Some(Value::Text("Shared".into())))])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(effective(&doc, 201), gold);
    client
        .edit_oib(&[(203, Some(Value::Text("Temporary".into())))])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client.edit_oib(&[(203, None)]).unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(effective(&doc, 203), Value::Text("Volume".into()));
    assert_eq!(effective(&doc, 303), Value::Text("Brightness".into()));
    for (field, value) in [(1, 481), (2, 729), (3, 0), (4, 1000)] {
        assert_eq!(client.value(field).unwrap(), value);
    }
    let saved = doc.save().unwrap();
    let mut recovered =
        StateDocument::load(ui.state_schema.clone(), "designer:fixture", &saved).unwrap();
    assert!(!recovered.configure_oib(&ui).unwrap());
    assert_eq!(doc.root_hash(), recovered.root_hash());
    assert_eq!(effective(&recovered, 201), gold);
    assert_eq!(effective(&recovered, 203), Value::Text("Volume".into()));
    assert!(doc.oib_status().unwrap().error.is_none());
}

#[test]
fn package_advertises_designer_requirement_and_rejects_its_removal() {
    use otrios_core::{bundle, canvas};
    use otrios_interfaces::bundle::{
        BundleFile, BundleFiles, FILES_FORMAT, FILES_PATH, INFO_PATH, SIGNATURE_PATH,
    };
    use sha2::{Digest, Sha256};

    let ui = app::ui(&[]);
    let bytes = canvas::encode_ui(&ui).unwrap();
    let entry = otrios_interfaces::canvas::CatalogEntry {
        app: app::manifest(),
        path: "test-playground/oib-designer".into(),
        private: true,
        ui_hash: Sha256::digest(&bytes).to_vec(),
    };
    let mut tree = bundle::pack(&entry, 1, &bytes, None).unwrap();
    let mut info = bundle::inspect(&tree).unwrap().info;
    assert!(info.required_features.iter().any(|f| f == FEATURE));
    info.required_features.retain(|f| f != FEATURE);
    tree.insert(INFO_PATH.into(), bundle::encode_info(&info).unwrap());

    // Keep the file closure valid so this exercises required-feature admission,
    // rather than failing earlier on the deliberately edited metadata's digest.
    let manifest = BundleFiles {
        format: FILES_FORMAT.into(),
        files: tree
            .iter()
            .filter(|(path, _)| !matches!(path.as_str(), FILES_PATH | SIGNATURE_PATH))
            .map(|(path, bytes)| BundleFile {
                path: path.clone(),
                bytes: bytes.len() as u64,
                sha256: Sha256::digest(bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            })
            .collect(),
    };
    tree.insert(FILES_PATH.into(), flexbuffers::to_vec(&manifest).unwrap());
    assert_eq!(
        bundle::inspect(&tree).unwrap_err(),
        "Missing required OIB designer feature"
    );
}

#[test]
fn embedded_designer_decoder_rejects_explicit_null_for_omitted_metadata() {
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("OIB fixture")
    };
    seed.designer.as_mut().unwrap().hierarchy_viewport = None;
    let canonical = otrios_core::canvas::encode_ui(&ui).unwrap();
    assert_eq!(otrios_core::canvas::decode_ui(&canonical).unwrap(), ui);
    let mut alternative = serde_json::to_value(&ui).unwrap();
    alternative["presentation"]["Oib"]["designer"]["hierarchy_viewport"] = serde_json::Value::Null;
    let bytes = flexbuffers::to_vec(&alternative).unwrap();
    assert_eq!(flexbuffers::from_slice::<Ui>(&bytes).unwrap(), ui);
    assert_eq!(
        otrios_core::canvas::decode_ui(&bytes).unwrap_err(),
        "Noncanonical OIB designer UI bytes"
    );
}

#[test]
fn slider_value_binding_must_match_the_control_mutation_field() {
    let mut ui = app::ui(&[]);
    let slider = ui
        .handlers
        .iter_mut()
        .find(|h| h.kind == otrios_interfaces::canvas::GestureKind::Slider && h.fields == [1])
        .unwrap();
    slider.fields = vec![2];
    assert_eq!(
        oib::validate(&ui).unwrap_err(),
        "OIB control and property binding use different app fields"
    );
}

fn property_registers(doc: &StateDocument) -> BTreeMap<(u64, u64), Vec<u8>> {
    [1, 2]
        .into_iter()
        .flat_map(|branch| {
            doc.oib_values(branch)
                .unwrap()
                .keys()
                .map(|id| {
                    (
                        (branch, *id),
                        doc.oib_property_leaf(branch, *id)
                            .unwrap()
                            .to_bytes()
                            .unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
fn structural_request(
    doc: &StateDocument,
    sequence: u64,
    operation: otrios_interfaces::oib::InstanceOperation,
) -> Vec<u8> {
    otrios_core::canvas::encode_state_request(
        &otrios_interfaces::canvas::StateRequest::OibInstance {
            epoch: doc.oib_epoch().unwrap(),
            sequence,
            operation,
        },
    )
    .unwrap()
}
fn fresh_admin(doc: &mut StateDocument, writer: u8) -> Session {
    let (mut session, _) = doc.open_session(&[writer; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
}

#[test]
fn d2_insert_exact_factory_and_reorder_preserve_property_registers_slots_and_values() {
    let (ui, mut doc, mut client, mut session) = setup();
    assert_eq!(
        client
            .oib_canvas_factories()
            .unwrap()
            .iter()
            .take(2)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [20, 30]
    );
    client
        .set(&[(1, 481), (2, 729), (3, 0), (4, 1000)])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client
        .edit_oib(&[
            (203, Some(Value::Text("Retained label".into()))),
            (301, Some(Value::Color([9, 8, 7, 255]))),
        ])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    let registers = property_registers(&doc);
    let initial = doc.oib_allocations().unwrap();
    assert!(client.insert_oib(30, Some(20)).unwrap());
    sync(&mut doc, &mut session, &mut client);
    let allocations = doc.oib_allocations().unwrap();
    let inserted = &allocations[0];
    assert!(inserted.id > 50);
    assert_eq!(
        inserted.prototype, 30,
        "explicit Display factory retains its binding"
    );
    assert_eq!(&allocations[1..], initial.as_slice());
    let expanded = oib::instances::expanded_seed(fixture_seed(&ui).unwrap(), &allocations).unwrap();
    let label = oib::document::cell_for_address(
        fixture_seed(&ui).unwrap(),
        &allocations,
        &PropertyAddress::Occurrence {
            instance: InstanceId(inserted.id),
            path: path(&[7], 1, 3),
        },
    )
    .unwrap();
    assert_eq!(
        oib::effective(&expanded, &doc.oib_values(2).unwrap(), label).unwrap(),
        Value::Text("Brightness".into())
    );
    let inserted_metadata = expanded
        .designer
        .as_ref()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id.0 == inserted.id)
        .unwrap();
    assert_eq!(
        inserted_metadata.assignments,
        document(&ui)
            .instances
            .iter()
            .find(|i| i.id == InstanceId(30))
            .unwrap()
            .assignments
    );
    let inserted_id = inserted.id;
    assert!(client.reorder_oib(20, None).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(
        doc.oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [inserted_id, 30, 20]
    );
    assert!(client.reorder_oib(inserted_id, None).unwrap());
    sync(&mut doc, &mut session, &mut client);
    let moved = doc.oib_allocations().unwrap();
    assert_eq!(
        moved.iter().map(|a| a.id).collect::<Vec<_>>(),
        [30, 20, inserted_id]
    );
    for allocation in &initial {
        assert_eq!(
            moved.iter().find(|a| a.id == allocation.id).unwrap(),
            allocation,
            "reorder changes no resident descriptor"
        );
    }
    for (key, bytes) in registers {
        assert_eq!(
            property_registers(&doc)[&key],
            bytes,
            "surviving causal register changed at {key:?}"
        );
    }
    let (compiled, _) = doc.oib_compiled(2).unwrap();
    let compiled_seed = fixture_seed(&compiled).unwrap();
    let preview = find(
        &template(&compiled),
        compiled_seed
            .instance_surface
            .as_ref()
            .unwrap()
            .preview_viewport,
    )
    .clone();
    let root_ids = moved
        .iter()
        .map(|a| {
            compiled_seed
                .designer
                .as_ref()
                .unwrap()
                .mappings
                .objects
                .iter()
                .find(|m| {
                    m.address.instance.0 == a.id
                        && m.address.references.is_empty()
                        && m.address.node == NodeId(1)
                })
                .unwrap()
                .layout_node
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        preview.children.iter().map(|n| n.id).collect::<Vec<_>>(),
        root_ids,
        "paint order follows the authoritative roster"
    );
    for (field, value) in [(1, 481), (2, 729), (3, 0), (4, 1000)] {
        assert_eq!(client.value(field).unwrap(), value);
    }
    let saved = doc.save().unwrap();
    let mut recovered =
        StateDocument::load(ui.state_schema.clone(), "designer:fixture", &saved).unwrap();
    assert!(!recovered.configure_oib(&ui).unwrap());
    assert_eq!(recovered.root_hash(), doc.root_hash());
    assert_eq!(recovered.oib_allocations().unwrap(), moved);
}

#[test]
fn d2_stale_and_invalid_structures_are_receipted_without_allocating_or_rewriting_properties() {
    use otrios_interfaces::oib::InstanceOperation::{Insert, Reorder};
    let (ui, mut doc, mut client, mut session) = setup();
    let mut other = fresh_admin(&mut doc, 2);
    let generation = doc.oib_status().unwrap().generation;
    let first = structural_request(
        &doc,
        1,
        Reorder {
            instance: 30,
            before: Some(20),
            generation,
        },
    );
    assert!(doc.receive(&mut other, &first).unwrap());
    assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
    let accepted = doc.oib_allocations().unwrap();
    let registers = property_registers(&doc);
    let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let next_id = tree.subtree(&[4, 4]).unwrap().to_bytes().unwrap();
    let rejected = [
        Insert {
            prototype: 20,
            before: None,
            generation,
        },
        Insert {
            prototype: 40,
            before: None,
            generation: generation + 1,
        },
        Insert {
            prototype: 20,
            before: Some(40),
            generation: generation + 1,
        },
        Reorder {
            instance: 40,
            before: None,
            generation: generation + 1,
        },
        Reorder {
            instance: 30,
            before: Some(30),
            generation: generation + 1,
        },
    ];
    let mut last = vec![];
    for (index, operation) in rejected.into_iter().enumerate() {
        last = structural_request(&doc, index as u64 + 2, operation);
        assert!(doc.receive(&mut other, &last).unwrap());
        assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
        assert_eq!(doc.oib_allocations().unwrap(), accepted);
        assert_eq!(property_registers(&doc), registers);
        let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
        assert_eq!(tree.subtree(&[4, 4]).unwrap().to_bytes().unwrap(), next_id);
        assert!(
            doc.oib_status()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .starts_with("Structure rejected:")
        );
        let root = doc.root_hash();
        assert!(
            !doc.receive(&mut other, &last).unwrap(),
            "same rejection replays its durable receipt"
        );
        assert_eq!(doc.root_hash(), root);
    }
    let saved = doc.save().unwrap();
    let mut recovered =
        StateDocument::load(ui.state_schema.clone(), "designer:fixture", &saved).unwrap();
    recovered.configure_oib(&ui).unwrap();
    let (mut resumed, _) = recovered.open_session(&[2; 16], 6).unwrap();
    resumed.set_oib_editable(true);
    assert!(!recovered.receive(&mut resumed, &last).unwrap());
    let retry = structural_request(
        &doc,
        7,
        Reorder {
            instance: 20,
            before: Some(30),
            generation: generation + 1,
        },
    );
    assert!(recovered.receive(&mut resumed, &retry).unwrap());
    assert!(recovered.oib_status().unwrap().error.is_none());
    assert_eq!(
        recovered
            .oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [20, 30]
    );
    assert_eq!(property_registers(&recovered), registers);
    // A client that refreshes intentionally can submit a new generation.
    sync(&mut doc, &mut session, &mut client);
    assert!(client.confirmed());
}

#[test]
fn d2_palette_only_power_uses_fresh_identity_and_restore_keeps_original_residents() {
    let (ui, mut doc, mut client, mut session) = setup();
    let seed = fixture_seed(&ui).unwrap();
    assert_eq!(
        client
            .oib_canvas_factories()
            .unwrap()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [20, 30, 50]
    );
    assert_eq!(
        doc.oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [20, 30]
    );
    assert!(!doc.oib_values(1).unwrap().contains_key(&400));
    let before = property_registers(&doc);
    assert!(client.insert_oib(50, Some(30)).unwrap());
    sync(&mut doc, &mut session, &mut client);
    let allocations = doc.oib_allocations().unwrap();
    let power = allocations.iter().find(|a| a.prototype == 50).unwrap();
    assert!(power.id > 50);
    assert_ne!(power.id, power.prototype);
    assert_eq!(power.component, 13);
    assert_eq!(
        allocations.iter().map(|a| a.prototype).collect::<Vec<_>>(),
        [20, 50, 30]
    );
    let expanded = oib::instances::expanded_seed(seed, &allocations).unwrap();
    let label = oib::document::cell_for_address(
        seed,
        &allocations,
        &PropertyAddress::Occurrence {
            instance: InstanceId(power.id),
            path: path(&[], 1, 3),
        },
    )
    .unwrap();
    assert_eq!(
        oib::document::address_for_cell(oib::seed(&doc.oib_compiled(2).unwrap().0).unwrap(), label)
            .unwrap(),
        PropertyAddress::Occurrence {
            instance: InstanceId(power.id),
            path: path(&[], 1, 3)
        }
    );
    assert_eq!(
        oib::effective(&expanded, &doc.oib_values(2).unwrap(), label).unwrap(),
        Value::Text("Power".into())
    );
    let root = expanded
        .designer
        .as_ref()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id.0 == power.id)
        .unwrap();
    let AssignmentSource::AppBinding(Binding::Local(checked)) = root
        .assignments
        .iter()
        .find(|a| a.target == path(&[], 1, 5))
        .unwrap()
        .source
    else {
        panic!("Power checked connection");
    };
    assert_eq!(
        ui.selectors
            .iter()
            .find(|s| s.local == checked)
            .unwrap()
            .field,
        3
    );
    for (key, bytes) in &before {
        assert_eq!(&property_registers(&doc)[key], bytes);
    }
    client
        .edit_oib(&[(label, Some(Value::Text("Custom power".into())))])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert!(client.reorder_oib(power.id, Some(20)).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(
        doc.oib_values(2).unwrap()[&label],
        Some(Value::Text("Custom power".into()))
    );
    assert!(client.restore_oib(false).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(
        doc.oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [20, 30]
    );
    assert!(!doc.oib_values(2).unwrap().contains_key(&label));
    for (key, bytes) in before {
        assert_eq!(property_registers(&doc)[&key], bytes);
    }
    assert!(client.insert_oib(50, None).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert!(
        doc.oib_allocations().unwrap().last().unwrap().id > power.id,
        "restoring never recycles a retired identity"
    );
}

#[test]
fn d2_full_canvas_rejection_preserves_identity_counter_and_surviving_causal_bytes() {
    use otrios_interfaces::oib::InstanceOperation;
    let (_, mut doc, mut client, mut session) = setup();
    for _ in 0..4 {
        assert!(client.insert_oib(20, None).unwrap());
        sync(&mut doc, &mut session, &mut client);
    }
    let residents = doc.oib_allocations().unwrap();
    assert_eq!(residents.len(), 6);
    let registers = property_registers(&doc);
    let before = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let generation = doc.oib_status().unwrap().generation;
    let mut other = fresh_admin(&mut doc, 3);
    let request = structural_request(
        &doc,
        1,
        InstanceOperation::Insert {
            prototype: 50,
            before: None,
            generation,
        },
    );
    assert!(doc.receive(&mut other, &request).unwrap());
    assert_eq!(doc.oib_status().unwrap().generation, generation);
    assert_eq!(doc.oib_allocations().unwrap(), residents);
    assert_eq!(property_registers(&doc), registers);
    let after = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    assert_eq!(
        after.subtree(&[4, 4]).unwrap().to_bytes().unwrap(),
        before.subtree(&[4, 4]).unwrap().to_bytes().unwrap()
    );
    assert!(
        doc.oib_status()
            .unwrap()
            .error
            .unwrap()
            .contains("capacity")
    );
}

#[test]
fn d2_captured_stale_client_actions_are_receipted_after_observing_remote_publication() {
    use otrios_interfaces::oib::InstanceOperation::{Insert, Reorder};
    let (_, mut doc, mut client, mut session) = setup();
    let generation = client.oib_generation().unwrap();
    let captured = [
        Insert {
            prototype: 50,
            before: Some(30),
            generation,
        },
        Reorder {
            instance: 20,
            before: Some(30),
            generation,
        },
    ];
    let mut other = fresh_admin(&mut doc, 2);
    assert!(
        doc.receive(
            &mut other,
            &structural_request(
                &doc,
                1,
                Reorder {
                    instance: 30,
                    before: Some(20),
                    generation,
                },
            ),
        )
        .unwrap()
    );
    // This is the release-after-remote-update case: the adapter has already
    // observed a newer generation before it queues the captured action.
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(client.oib_generation(), Some(generation + 1));
    let accepted = doc.oib_allocations().unwrap();
    let registers = property_registers(&doc);
    let before = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let next_id = before.subtree(&[4, 4]).unwrap().to_bytes().unwrap();
    for operation in captured {
        assert!(client.edit_oib_structure(operation).unwrap());
        assert!(!client.confirmed());
        sync(&mut doc, &mut session, &mut client);
        assert!(
            client.confirmed(),
            "stale intent receives a durable receipt"
        );
        assert_eq!(client.oib_generation(), Some(generation + 1));
        assert!(client.oib_status_text().contains("Canvas changed"));
        assert_eq!(doc.oib_allocations().unwrap(), accepted);
        assert_eq!(property_registers(&doc), registers);
        let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
        assert_eq!(tree.subtree(&[4, 4]).unwrap().to_bytes().unwrap(), next_id);
    }
    // A deliberate retry captures the confirmed generation and can succeed.
    assert!(client.reorder_oib(20, Some(30)).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(client.oib_generation(), Some(generation + 2));
    assert!(doc.oib_status().unwrap().error.is_none());
    assert_eq!(
        doc.oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [20, 30]
    );
    assert_eq!(property_registers(&doc), registers);
    assert!(client.insert_oib(50, None).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(doc.oib_allocations().unwrap().last().unwrap().prototype, 50);
}
