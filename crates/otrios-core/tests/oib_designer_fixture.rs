// SPDX-License-Identifier: LGPL-3.0-only
//! Canonical production graph admission, authoring and source-resolution checks. These are
//! in-process checks, not authority-process, Linux IPC or live acceptance.
#![cfg(feature = "canvas")]
// Both independently packaged app descriptions reuse the same composition source.
#![allow(clippy::duplicate_mod)]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/live-ui/app.rs"]
mod live_ui;
use otrios_core::{
    canvas::{self, Document as StateDocument, Session, StateAdapter, StateSelection, oib},
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

fn admitted(source: Ui) -> Ui {
    canvas::decode_ui(&canvas::encode_ui(&source).unwrap()).unwrap()
}
fn ui() -> Ui {
    admitted(app::ui(&[]))
}
fn occurrence(instance: u64, refs: &[u64], property: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: path(refs, 1, property),
    }
}
fn shared(definition: u64, property: u64) -> PropertyAddress {
    PropertyAddress::Definition {
        definition: DefinitionId(definition),
        node: NodeId(1),
        property: PropertyId(property),
    }
}
fn property_cell(ui: &Ui, allocations: &[Allocation], address: &PropertyAddress) -> u64 {
    oib::document::cell_for_address(fixture_seed(ui).unwrap(), allocations, address).unwrap()
}
fn value(ui: &Ui, doc: &StateDocument, address: &PropertyAddress) -> Value {
    let allocations = doc.oib_allocations().unwrap();
    let expanded = oib::instances::expanded_seed(fixture_seed(ui).unwrap(), &allocations).unwrap();
    oib::effective(
        &expanded,
        &doc.oib_values(2).unwrap(),
        property_cell(ui, &allocations, address),
    )
    .unwrap()
}

#[test]
fn canonical_package_contains_graph_and_empty_tooling_chrome_with_disposable_projections() {
    let ui = ui();
    let seed = fixture_seed(&ui).unwrap();
    let graph = seed.graph.as_ref().unwrap();
    assert!(app::manifest().dependencies.iter().any(|d| d == FEATURE));
    assert_eq!(
        graph.instances.iter().map(|i| i.id.0).collect::<Vec<_>>(),
        [20, 30, 40]
    );
    assert_eq!(
        graph
            .presets
            .iter()
            .filter(|p| p.palette)
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [20, 30, 50]
    );
    assert_eq!(document(&ui).presented_root, InstanceId(40));
    let stripped = oib::graph::strip(&ui).unwrap();
    let root = template(&stripped);
    for id in [
        Some(graph.preview_viewport),
        Some(graph.inspector_viewport),
        graph.hierarchy_viewport,
    ]
    .into_iter()
    .flatten()
    {
        assert!(find(&root, id).children.is_empty());
    }
    assert!(stripped.handlers.is_empty());
    assert!(stripped.selectors.is_empty());
    let bytes = canvas::encode_ui(&ui).unwrap();
    let raw: serde_json::Value = flexbuffers::from_slice(&bytes).unwrap();
    let source = &raw["presentation"]["Oib"];
    assert_eq!(
        source
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ["format", "graph"]
    );
    assert!(
        graph
            .definitions
            .iter()
            .flat_map(|d| &d.nodes)
            .any(|n| n.visual.is_some())
    );
}
#[test]
fn semantic_property_paths_are_ordered_and_do_not_depend_on_names_or_renderer_handles() {
    let ui = ui();
    let seed = fixture_seed(&ui).unwrap();
    let doc = document(&ui);
    assert!(doc.classes.windows(2).all(|w| w[0].id < w[1].id));
    assert!(doc.definitions.windows(2).all(|w| w[0].id < w[1].id));
    for table in &doc.mappings.roots {
        assert!(table.cells.windows(2).all(|w| w[0].path < w[1].path));
        for row in &table.cells {
            assert_eq!(
                oib::document::address_for_cell(seed, row.initial_cell).unwrap(),
                PropertyAddress::Occurrence {
                    instance: table.prototype,
                    path: row.path.clone()
                }
            );
        }
    }
    let mut definitions = seed.graph.as_ref().unwrap().definitions.clone();
    for d in &mut definitions {
        d.name = format!("Definition {}", d.id.0);
        for n in &mut d.nodes {
            n.name = format!("Object {}", n.id.0);
        }
    }
    let revised =
        oib::graph::with_source(&ui, &definitions, &seed.graph.as_ref().unwrap().presets).unwrap();
    assert_eq!(
        document(&revised).mappings.definitions,
        doc.mappings.definitions
    );
    assert_eq!(document(&revised).mappings.roots, doc.mappings.roots);
}

#[test]
fn signed_bindings_and_checked_semantics_use_distinct_numeric_fields() {
    let ui = ui();
    let old = admitted(live_ui::ui(&[]));
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
    let ui = ui();
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
            assert_eq!([panel.frame.width, panel.frame.height], [520., 360.]);
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
fn sample_admission_enforces_resident_property_live_scene_and_renderer_budgets_independently() {
    let ui = ui();
    oib::validate(&ui).unwrap();
    let seed = fixture_seed(&ui).unwrap();
    let initial = oib::instances::initial_allocations(seed);
    for (instance, refs, property) in [(20, &[7][..], 4), (30, &[8][..], 5)] {
        assert!(
            oib::document::cell_for_address(seed, &initial, &occurrence(instance, refs, property))
                .is_err(),
            "application-bound properties have no causal literal register"
        );
    }
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
    let resident = &allocations[1..];
    assert!(
        oib::instances::expanded_seed(seed, resident)
            .unwrap()
            .cells
            .len()
            <= MAX_CELLS
    );
    assert!(oib::instances::expanded_seed(seed, &allocations).is_err());
    let live: BTreeSet<_> = resident.iter().rev().take(6).map(|a| a.id).collect();
    let expanded = oib::instances::expand(&ui, resident, &live, None, &BTreeMap::new()).unwrap();
    let layout = oib::compile(&expanded, &BTreeMap::new()).unwrap();
    let (nodes, depth) = count(&layout.blueprint().templates[0].root, 0);
    assert!(nodes <= 512 && depth <= 12);
    assert!(expanded.handlers.len() <= 128 && expanded.selectors.len() <= 128);
    assert!(expanded.layout.len() <= 128 * 1024);
    let mut seven = live;
    seven.insert(resident[0].id);
    assert!(oib::instances::expand(&ui, resident, &seven, None, &BTreeMap::new()).is_err());
}

fn setup() -> (Ui, StateDocument, StateAdapter, Session) {
    let ui = ui();
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
        let mut sent = 0;
        while let Some(bytes) = client.generate().unwrap() {
            sent += 1;
            assert!(
                sent <= 24,
                "client repeatedly generated {:?}; status {:?}",
                canvas::decode_state_request(&bytes).unwrap(),
                doc.oib_status().unwrap().error
            );
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
fn forwarding_nested_override_reset_and_restart_preserve_authored_labels_and_app_values() {
    let (ui, mut doc, mut client, mut session) = setup();
    client
        .set(&[(1, 481), (2, 729), (3, 0), (4, 1000)])
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    let gold = Value::Color([218, 165, 32, 255]);
    let orange = Value::Color([255, 136, 85, 255]);
    client
        .edit_oib_property(&shared(10, 1), Some(gold.clone()))
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client
        .edit_oib_property(&shared(12, 2), Some(Value::Number(400.)))
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    for instance in [20, 30] {
        for refs in [&[7][..], &[8][..]] {
            assert_eq!(value(&ui, &doc, &occurrence(instance, refs, 1)), gold);
        }
        assert_eq!(
            value(&ui, &doc, &occurrence(instance, &[7], 2)),
            Value::Number(400.)
        );
    }
    client
        .edit_oib_property(&occurrence(20, &[7], 1), Some(orange.clone()))
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(value(&ui, &doc, &occurrence(20, &[7], 1)), orange);
    assert_eq!(value(&ui, &doc, &occurrence(30, &[7], 1)), gold);
    client
        .edit_oib_property(&occurrence(20, &[7], 1), None)
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client
        .edit_oib_property(&shared(12, 3), Some(Value::Text("Shared".into())))
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client
        .edit_oib_property(
            &occurrence(20, &[7], 3),
            Some(Value::Text("Temporary".into())),
        )
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client
        .edit_oib_property(&occurrence(20, &[7], 3), None)
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(
        value(&ui, &doc, &occurrence(20, &[7], 3)),
        Value::Text("Volume".into())
    );
    assert_eq!(
        value(&ui, &doc, &occurrence(30, &[7], 3)),
        Value::Text("Brightness".into())
    );
    let mut recovered = StateDocument::load(
        ui.state_schema.clone(),
        "designer:fixture",
        &doc.save().unwrap(),
    )
    .unwrap();
    assert!(!recovered.configure_oib(&ui).unwrap());
    assert_eq!(recovered.root_hash(), doc.root_hash());
    assert_eq!(value(&ui, &recovered, &occurrence(20, &[7], 1)), gold);
    for (field, expected) in [(1, 481), (2, 729), (3, 0), (4, 1000)] {
        assert_eq!(
            recovered.typed_value(field).unwrap(),
            Value::Number(expected as f32)
        );
    }
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
fn canonical_graph_decoder_rejects_explicit_null_for_omitted_metadata() {
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed.graph.as_mut().unwrap().hierarchy_viewport = None;
    let canonical = canvas::encode_ui(&ui).unwrap();
    let admitted = canvas::decode_ui(&canonical).unwrap();
    assert_eq!(canvas::encode_ui(&admitted).unwrap(), canonical);
    let mut alternate: serde_json::Value = flexbuffers::from_slice(&canonical).unwrap();
    alternate["presentation"]["Oib"]["graph"]["hierarchy_viewport"] = serde_json::Value::Null;
    assert!(canvas::decode_ui(&flexbuffers::to_vec(&alternate).unwrap()).is_err());
}

#[test]
fn slider_value_binding_must_match_the_control_mutation_field() {
    let mut ui = ui();
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

fn property_registers(doc: &StateDocument) -> BTreeMap<Vec<u64>, Vec<u8>> {
    fn walk(
        tree: &otrios_core::merkle::Snapshot,
        path: &mut Vec<u64>,
        rows: &mut BTreeMap<Vec<u64>, Vec<u8>>,
    ) {
        if tree.register().is_some() {
            rows.insert(path.clone(), tree.to_bytes().unwrap());
        }
        for key in tree.child_keys() {
            path.push(key);
            walk(&tree.subtree(&[key]).unwrap(), path, rows);
            path.pop();
        }
    }
    let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let mut rows = BTreeMap::new();
    for branch in [1, 2] {
        walk(
            &tree.subtree(&[4, branch]).unwrap(),
            &mut vec![4, branch],
            &mut rows,
        );
    }
    rows
}
fn structural_request(
    doc: &StateDocument,
    sequence: u64,
    operation: otrios_interfaces::oib::authoring::EditBatch,
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
        .edit_oib_property(
            &occurrence(20, &[7], 3),
            Some(Value::Text("Retained label".into())),
        )
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    client
        .edit_oib_property(&occurrence(30, &[7], 1), Some(Value::Color([9, 8, 7, 255])))
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
    use otrios_interfaces::oib::authoring::EditBatch as InstanceOperation;
    let (ui, mut doc, mut client, mut session) = setup();
    let mut other = fresh_admin(&mut doc, 2);
    let generation = doc.oib_status().unwrap().structural_generation;
    let first = structural_request(
        &doc,
        1,
        InstanceOperation::reorder(generation, 30, Some(20)),
    );
    assert!(doc.receive(&mut other, &first).unwrap());
    assert_eq!(
        doc.oib_status().unwrap().structural_generation,
        generation + 1
    );
    let accepted = doc.oib_allocations().unwrap();
    let registers = property_registers(&doc);
    let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let next_id = tree.subtree(&[4, 4]).unwrap().to_bytes().unwrap();
    let rejected = [
        InstanceOperation::insert(generation, 20, None, None),
        InstanceOperation::insert(generation + 1, 40, None, None),
        InstanceOperation::insert(generation + 1, 20, Some(40), None),
        InstanceOperation::reorder(generation + 1, 40, None),
        InstanceOperation::reorder(generation + 1, 30, Some(30)),
    ];
    let mut last = vec![];
    for (index, operation) in rejected.into_iter().enumerate() {
        last = structural_request(&doc, index as u64 + 2, operation);
        assert!(doc.receive(&mut other, &last).unwrap());
        assert_eq!(
            doc.oib_status().unwrap().structural_generation,
            generation + 1
        );
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
        InstanceOperation::reorder(generation + 1, 20, Some(30)),
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
    assert!(!doc.oib_allocations().unwrap().iter().any(|a| a.id == 50));
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
    use otrios_interfaces::oib::authoring::EditBatch as InstanceOperation;
    let (_, mut doc, mut client, mut session) = setup();
    for _ in 0..4 {
        assert!(client.insert_oib(20, None).unwrap());
        sync(&mut doc, &mut session, &mut client);
    }
    let residents = doc.oib_allocations().unwrap();
    assert_eq!(residents.len(), 6);
    let registers = property_registers(&doc);
    let before = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let generation = doc.oib_status().unwrap().structural_generation;
    let mut other = fresh_admin(&mut doc, 3);
    let request = structural_request(
        &doc,
        1,
        InstanceOperation::insert(generation, 50, None, None),
    );
    assert!(doc.receive(&mut other, &request).unwrap());
    assert_eq!(doc.oib_status().unwrap().structural_generation, generation);
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
    use otrios_interfaces::oib::authoring::EditBatch as InstanceOperation;
    let (_, mut doc, mut client, mut session) = setup();
    let generation = client.oib_structural_generation().unwrap();
    let captured = [
        InstanceOperation::insert(generation, 50, Some(30), None),
        InstanceOperation::reorder(generation, 20, Some(30)),
    ];
    let mut other = fresh_admin(&mut doc, 2);
    assert!(
        doc.receive(
            &mut other,
            &structural_request(
                &doc,
                1,
                InstanceOperation::reorder(generation, 30, Some(20)),
            ),
        )
        .unwrap()
    );
    // This is the release-after-remote-update case: the adapter has already
    // observed a newer generation before it queues the captured action.
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(client.oib_structural_generation(), Some(generation + 1));
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
        assert_eq!(client.oib_structural_generation(), Some(generation + 1));
        assert!(
            client.oib_status_text().contains("changed"),
            "{}",
            client.oib_status_text()
        );
        assert_eq!(doc.oib_allocations().unwrap(), accepted);
        assert_eq!(property_registers(&doc), registers);
        let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
        assert_eq!(tree.subtree(&[4, 4]).unwrap().to_bytes().unwrap(), next_id);
    }
    // A deliberate retry captures the confirmed generation and can succeed.
    assert!(client.reorder_oib(20, Some(30)).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(client.oib_structural_generation(), Some(generation + 2));
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
