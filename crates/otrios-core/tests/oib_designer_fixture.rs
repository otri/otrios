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
        vec![20, 30]
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
        assert!(find(&root, prototype.inspector_root).children.is_empty());
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
        6,
        "only three app controls per panel; no per-row inspector handlers"
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
            table.cells.iter().map(|r| r.key).collect::<Vec<_>>(),
            vec![2, 9, 17, 28, 41, 55]
        );
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
    assert_eq!(old.state_schema.id, "live-ui-state-v1");
    assert_eq!(old.state_schema.fields.len(), 2);
    assert_eq!(ui.state_schema.id, "oib-designer-state-v1");
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
        vec![vec![3], vec![3], vec![4], vec![4]]
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
    assert_eq!(authored, 26);
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
    for row in &table.cells {
        let lowered = oib::document::cell_for_address(
            seed,
            resident,
            &PropertyAddress::Occurrence {
                instance: InstanceId(allocations[9].id),
                path: row.path.clone(),
            },
        )
        .unwrap();
        assert_eq!(lowered, 0x100_0000 + 9 * 64 + u64::from(row.key));
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
        ui_hash: Sha256::digest(&bytes).into(),
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
