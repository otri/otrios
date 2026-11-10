// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
pub(super) use crate::test_support::oib_designer as designer_fixture;
use otrios_interfaces::{
    layout::ValueType,
    oib::{Cell, Seed, designer::*},
};
fn fixture_ui() -> Ui {
    crate::canvas::decode_ui(&crate::canvas::encode_ui(&designer_fixture::ui(&[])).unwrap())
        .unwrap()
}

/// A manual diagnostic, deliberately excluded from correctness test runs.
/// Measures the real admitted production graph and designer refresh path; the
/// fixture uses local definitions rather than a signed installed-library import.
#[test]
#[ignore = "manual optimized UI rebuild timing diagnostic"]
fn profile_oib_designer_refresh_stages() {
    use otrios_interfaces::oib::authoring::{HierarchyBatch, HierarchyEdit};
    use std::time::Instant;

    fn measure(label: &str, roots: usize, repeats: usize, mut action: impl FnMut()) {
        let samples: Vec<_> = (0..repeats)
            .map(|_| {
                let start = Instant::now();
                action();
                start.elapsed().as_micros()
            })
            .collect();
        println!("{{\"stage\":\"{label}\",\"roots\":{roots},\"us\":{samples:?}}}");
    }
    let installed = std::env::var_os("OTRIOS_OIB_PROFILE_UI");
    println!(
        "{{\"baseline\":\"{}\"}}",
        if installed.is_some() {
            "installed_library"
        } else {
            "production_local_fixture"
        }
    );
    for extra in if installed.is_some() {
        vec![0]
    } else {
        vec![0, 4]
    } {
        let (mut client, mut authority, mut session) = mounted_fixture_session_with_ui(
            true,
            extra,
            installed.as_ref().map_or_else(fixture_ui, |path| {
                crate::canvas::decode_ui(&std::fs::read(path).unwrap()).unwrap()
            }),
        );
        let mount = client.active.as_mut().unwrap();
        let roots = mount.doc.oib_allocations().unwrap().len();
        let original = mount.doc.oib_root();
        measure("unchanged_refresh_designer", roots, 20, || {
            assert!(!mount.refresh_designer().unwrap());
        });
        measure("unchanged_oib_status_text", roots, 20, || {
            std::hint::black_box(mount.doc.oib_status_text());
        });
        measure("unchanged_refresh_oib_view", roots, 20, || {
            assert!(!mount.refresh_oib_view().unwrap());
        });
        measure("unchanged_mount_refresh", roots, 20, || {
            assert!(!mount.refresh().unwrap());
        });
        measure("nested_selection_warm_tools_invalidate", roots, 3, || {
            mount.designer.as_mut().unwrap().revision = None;
            assert!(mount.refresh_designer().unwrap());
        });
        let root = mount.designer.as_ref().unwrap().root_selection(20).unwrap();
        mount.designer.as_mut().unwrap().select(root);
        measure("root_selection_cold_tools", roots, 1, || {
            assert!(mount.refresh_designer().unwrap());
        });
        measure("root_selection_warm_tools_invalidate", roots, 3, || {
            mount.designer.as_mut().unwrap().revision = None;
            assert!(mount.refresh_designer().unwrap());
        });
        assert_eq!(mount.doc.oib_root(), original);
        let generation = mount.doc.oib_structural_generation().unwrap();
        for (label, edit) in [
            (
                "hierarchy_allowed_duplicate",
                HierarchyEdit::Duplicate {
                    instance: InstanceId(20),
                    name: None,
                    destination: otrios_interfaces::oib::authoring::Destination::Canvas,
                    before: None,
                    frame: Some([0., 0., 400., 200.]),
                },
            ),
            (
                "hierarchy_allowed_move",
                HierarchyEdit::Move {
                    instance: InstanceId(20),
                    destination: otrios_interfaces::oib::authoring::Destination::Canvas,
                    before: None,
                    frame: Some([0., 40., 400., 200.]),
                },
            ),
        ] {
            let batch = HierarchyBatch {
                generation,
                operations: vec![edit],
            };
            measure(label, roots, 3, || {
                std::hint::black_box(mount.doc.oib_hierarchy_allowed(&batch));
            });
        }
        measure("document_oib_compiled", roots, 3, || {
            std::hint::black_box(mount.doc.oib_compiled(Some(20)).unwrap());
        });
        measure("state_oib_info", roots, 3, || {
            std::hint::black_box(mount.doc.oib_info().unwrap());
        });
        mount.designer.as_mut().unwrap().mode = DesignerMode::Test;
        measure("root_selection_test_mode_invalidate", roots, 3, || {
            mount.designer.as_mut().unwrap().revision = None;
            assert!(mount.refresh_designer().unwrap());
        });
        mount.designer.as_mut().unwrap().mode = DesignerMode::Edit;
        mount.designer.as_mut().unwrap().revision = None;
        mount.refresh_designer().unwrap();
        for width in [362., 364., 366.] {
            let start = Instant::now();
            assert!(
                mount
                    .doc
                    .edit_oib_property(&editor_address(2), Some(Value::Number(width)))
                    .unwrap()
            );
            println!(
                "{{\"stage\":\"property_queue\",\"roots\":{roots},\"us\":[{}]}}",
                start.elapsed().as_micros()
            );
            let start = Instant::now();
            fixture_sync(&mut authority, &mut session, &mut mount.doc);
            println!(
                "{{\"stage\":\"property_authority_and_client_sync\",\"roots\":{roots},\"us\":[{}]}}",
                start.elapsed().as_micros()
            );
            measure("property_confirmed_mount_refresh", roots, 1, || {
                assert!(mount.refresh().unwrap());
            });
        }
        let mut tick = 1;
        measure("unchanged_client_tick", roots, 20, || {
            std::hint::black_box(client.tick(tick, 0).unwrap());
            tick += 1;
        });
        let mount = client.active.as_mut().unwrap();
        let active_generation = mount.doc.oib_generation().unwrap();
        assert!(
            mount
                .doc
                .edit_oib_structure(otrios_interfaces::oib::InstanceOperation::reorder(
                    u64::MAX,
                    20,
                    None
                ))
                .unwrap()
        );
        fixture_sync(&mut authority, &mut session, &mut mount.doc);
        assert_eq!(mount.doc.oib_generation().unwrap(), active_generation);
        assert!(mount.doc.oib_status_text().contains("Structure rejected:"));
        measure(
            "rejected_structure_status_only_mount_refresh",
            roots,
            1,
            || {
                assert!(mount.refresh().unwrap());
            },
        );
        measure("unchanged_rejected_status_client_tick", roots, 20, || {
            std::hint::black_box(client.tick(tick, 0).unwrap());
            tick += 1;
        });
        measure("unchanged_client_reload_ready", roots, 20, || {
            std::hint::black_box(client.reload_ready());
        });
        measure("unchanged_client_resource_usage", roots, 20, || {
            std::hint::black_box(client.resource_usage());
        });
    }
}
fn seed() -> Seed {
    Seed {
        graph: None,
        resolved_libraries: vec![],
        expanded: false,
        format: otrios_interfaces::oib::FORMAT.into(),
        main_oib: 1,
        main_class: "otrios.declarative-v1".into(),
        designer: Some(otrios_interfaces::oib::designer::Document {
            placements: vec![],
            connections: vec![],
            id: DocumentId(1),
            name: "Document".into(),
            class: ClassId(1),
            presented_root: InstanceId(40),
            hierarchy_viewport: Some(10),
            classes: vec![],
            definitions: vec![],
            instances: vec![],
            mappings: Mappings {
                objects: vec![],
                definitions: vec![],
                roots: vec![],
            },
        }),
        components: vec![],
        instances: vec![],
        cells: vec![],
        inspector_viewport: 11,
        inspector_content: [240., 1000.],
        instance_surface: None,
    }
}
fn object(node: u64, depth: u16, visual: Option<u16>) -> ObjectInfo {
    ObjectInfo {
        selection: Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(node),
        }),
        name: format!("Object {node}"),
        class: ClassId(1),
        depth,
        layout_node: visual,
    }
}
fn property() -> PropertyInfo {
    PropertyInfo {
        address: Some(PropertyAddress::Occurrence {
            instance: InstanceId(20),
            path: PropertyPath {
                references: vec![NodeId(7)],
                node: NodeId(1),
                property: PropertyId(1),
            },
        }),
        cell: Some(200),
        descriptor: PropertyDescriptor {
            id: PropertyId(1),
            name: "accent".into(),
            label: "Accent".into(),
            value_type: ValueType::Color,
            default: Value::Color([1, 2, 3, 255]),
            number_range: None,
            number_step: None,
            units: None,
            group: InspectorGroup::Appearance,
            editor: PropertyEditor::Palette,
            policy: PropertyPolicy {
                live_literal: true,
                app_binding: true,
            },
        },
        value: Some(Value::Color([3, 2, 1, 255])),
        inherited: Some(Value::Color([1, 2, 3, 255])),
        origin: "component".into(),
        source: "forwarded".into(),
        overridden: true,
    }
}
fn layout() -> Scene {
    Scene::new(canvas(
        0,
        Rect::new(0., 0., 800., 600.),
        vec![
            canvas(10, Rect::new(0., 0., 240., 100.), vec![]),
            canvas(11, Rect::new(260., 0., 240., 100.), vec![]),
        ],
    ))
    .unwrap()
}
#[test]
fn nonvisual_selection_and_full_ancestry_stay_local() {
    let seed = seed();
    let mut d = Designer::new(&seed).unwrap();
    let nonvisual = object(2, 1, None);
    let selection = nonvisual.selection.clone();
    d.rebuild(
        &seed,
        vec![object(1, 0, Some(12)), nonvisual],
        &[],
        &[],
        "Active",
        true,
    )
    .unwrap();
    let before = flexbuffers::to_vec(&seed).unwrap();
    assert!(d.select(selection.clone()));
    assert_eq!(d.selection, selection);
    let Selection::Object(mut other) = selection else {
        panic!()
    };
    other.references = vec![NodeId(8)];
    assert_ne!(d.selection, Selection::Object(other));
    assert_eq!(before, flexbuffers::to_vec(&seed).unwrap());
    assert!(
        d.actions
            .values()
            .any(|a| matches!(a,LocalAction::Select(Selection::Object(o)) if o.node==NodeId(2)))
    );
}
#[test]
fn edit_intercepts_test_dispatches_and_mode_waits_for_capture() {
    let mut d = Designer::new(&seed()).unwrap();
    assert!(d.intercepts_preview());
    d.capture = Some(Capture {
        pointer: 7,
        start: [0., 0.],
        moving: false,
        action: None,
        panel: None,
    });
    assert!(d.set_mode(DesignerMode::Test, d.busy()));
    d.defer_viewport([800., 1280.]);
    assert_eq!(d.pending_viewport, Some([800., 1280.]));
    assert_eq!(d.mode, DesignerMode::Edit);
    assert_eq!(d.pending_mode, Some(DesignerMode::Test));
    d.capture = None;
    d.set_mode(d.pending_mode.unwrap(), d.busy());
    assert!(!d.intercepts_preview());
    assert!(d.pending_mode.is_none());
}
#[test]
fn reset_is_causal_clear_and_conflict_resolve_is_distinct() {
    let mut b = Builder::new();
    let p = property();
    inspector(
        &mut b,
        &seed(),
        &object(1, 0, None).selection,
        &[p],
        &[200],
        "Conflict / active retained",
        true,
    )
    .unwrap();
    assert!(
        b.actions
            .values()
            .any(|a| matches!(a, LocalAction::Write(_, None)))
    );
    assert!(
        b.actions
            .values()
            .any(|a| matches!(a, LocalAction::Resolve(_)))
    );
    assert!(
        !b.actions
            .values()
            .any(|a| matches!(a, LocalAction::Write(_, Some(_))))
    );
}
#[test]
fn bound_unsupported_and_readonly_properties_never_gain_editors() {
    let mut p = property();
    p.cell = None;
    assert!(choices(&seed(), &p).is_empty());
    let mut b = Builder::new();
    inspector(
        &mut b,
        &seed(),
        &Selection::Document(DocumentId(1)),
        &[p],
        &[200],
        "Active",
        true,
    )
    .unwrap();
    assert!(b.actions.is_empty());
    let mut p = property();
    p.descriptor.editor = PropertyEditor::ReadOnly;
    assert!(choices(&seed(), &p).is_empty());
    p.descriptor.editor = PropertyEditor::Toggle; // A color cannot become boolean.
    assert!(choices(&seed(), &p).is_empty());
}
#[test]
fn numeric_steps_and_boolean_choices_are_explicit_and_bounded() {
    let mut p = property();
    p.descriptor.editor = PropertyEditor::NumberSuggestions;
    p.descriptor.value_type = ValueType::Number;
    p.descriptor.number_range = Some([10., 20.]);
    p.descriptor.number_step = Some(4.);
    p.value = Some(Value::Number(19.));
    let mut s = seed();
    s.cells.push(Cell {
        id: 200,
        owner: 20,
        name: "size".into(),
        default: Value::Number(10.),
        authored_override: None,
        inherit: Some(100),
        targets: vec![],
        choices: vec![Value::Number(9.), Value::Number(10.), Value::Number(21.)],
    });
    assert_eq!(
        choices(&s, &p),
        vec![Value::Number(10.), Value::Number(15.), Value::Number(20.)]
    );
    p.descriptor.editor = PropertyEditor::Toggle;
    p.descriptor.value_type = ValueType::Boolean;
    assert_eq!(
        choices(&s, &p),
        vec![Value::Boolean(false), Value::Boolean(true)]
    );
}
#[test]
fn hierarchy_expansion_and_scroll_do_not_rebuild_or_write_document() {
    let s = seed();
    let mut d = Designer::new(&s).unwrap();
    d.rebuild(
        &s,
        (0..12)
            .map(|n| object(n, if n == 0 { 0 } else { 1 }, None))
            .collect(),
        &[],
        &[],
        "Active",
        true,
    )
    .unwrap();
    d.sync(&layout()).unwrap();
    let before = flexbuffers::to_vec(&s).unwrap();
    let nodes = d.panels[0].nodes.clone();
    d.panels[0].scroll.set_offset([0., 80.]);
    assert_eq!(d.panels[0].scroll.info().offset[1], 80.);
    assert_eq!(d.panels[1].scroll.info().offset[1], 0.);
    assert_eq!(d.panels[0].nodes, nodes);
    assert_eq!(before, flexbuffers::to_vec(&s).unwrap());
    let mut b = Builder::new();
    let rows = vec![
        object(0, 0, None),
        object(1, 1, None),
        object(2, 2, None),
        object(3, 0, None),
    ];
    hierarchy(
        &mut b,
        &rows,
        &rows[0].selection,
        &[rows[0].selection.clone()],
        DesignerMode::Edit,
    )
    .unwrap();
    assert!(
        b.actions
            .values()
            .any(|a| matches!(a,LocalAction::Select(s) if s==&rows[3].selection))
    );
    assert!(
        !b.actions
            .values()
            .any(|a| matches!(a,LocalAction::Select(s) if s==&rows[2].selection))
    );
}
#[test]
fn local_rows_cover_corners_clip_and_obey_modal_barriers() {
    let mut b = Builder::new();
    let row = b
        .button(
            0.,
            0.,
            240.,
            "Choose",
            LocalAction::Mode(DesignerMode::Test),
        )
        .unwrap();
    let id = row.id;
    let mut root = layout().root().clone();
    find_node(&mut root, 10).unwrap().children.push(row);
    let scene = Scene::new(root.clone()).unwrap();
    assert!(scene.hit_path([0.1, 0.1]).iter().any(|p| p.node.id == id));
    let panel = find_node(&mut root, 10).unwrap();
    panel.children[0].frame.y = -16.;
    host_panel::present_panel(&mut root, 10);
    let scene = Scene::new(root.clone()).unwrap();
    assert_eq!(scene.controls()[0].node.frame.height, 16.);
    assert!(scene.hit_path([120., 1.]).iter().any(|p| p.node.id == id));
    assert!(!scene.hit_path([120., 20.]).iter().any(|p| p.node.id == id));
    let mut modal = canvas(
        98,
        Rect::new(0., 0., 240., 100.),
        vec![background(99, 240., 100., CARD, 0.)],
    );
    modal.responder = Some(TouchResponder::Consume);
    root.children.push(modal);
    let scene = Scene::new(root).unwrap();
    let hit = scene.hit_path([50., 50.]);
    assert!(!admits_viewport(&scene, &hit, 10, [50., 50.]));
}
#[test]
fn outline_passes_through_to_deepest_application_responder() {
    let mut app = canvas(
        12,
        Rect::new(0., 0., 100., 60.),
        vec![background(13, 100., 60., CARD, 8.)],
    );
    app.responder = Some(TouchResponder::Activate(Action::AppEvent(1)));
    let mut overlay = canvas(
        14,
        Rect::new(0., 0., 100., 60.),
        vec![background(15, 100., 60., ACCENT, 0.)],
    );
    overlay.responder = Some(TouchResponder::PassThrough);
    let scene = Scene::new(canvas(0, Rect::new(0., 0., 800., 600.), vec![app, overlay])).unwrap();
    assert!(scene.hit_path([0.1, 0.1]).iter().any(|p| p.node.id == 12));
    assert!(!scene.hit_path([50., 30.]).iter().any(|p| p.node.id == 14));
}
#[test]
fn oversize_chrome_rejects_without_paging_or_raising_limits() {
    let rows: Vec<_> = (0..214).map(|n| object(n, 0, None)).collect();
    let (nodes, content) = hierarchy(
        &mut Builder::new(),
        &rows,
        &Selection::Document(DocumentId(1)),
        &[],
        DesignerMode::Edit,
    )
    .unwrap();
    assert_eq!(content, 8170.);
    assert_eq!(
        nodes.len(),
        215,
        "every coordinate-admitted row is retained"
    );
    let mut b = Builder::new();
    let rows: Vec<_> = (0..215).map(|n| object(n, 0, None)).collect();
    assert!(
        hierarchy(
            &mut b,
            &rows,
            &Selection::Document(DocumentId(1)),
            &[],
            DesignerMode::Edit
        )
        .is_err()
    );
    let mut legacy = seed();
    legacy.designer = None;
    assert!(Designer::new(&legacy).is_none());
}

// Contract-query model of the full fixture: fixed shell/configuration, six
// panels with all 16 expanded objects each, and all four definitions/18 nodes.
// This deliberately does not call or replace runtime graph/source resolution.
fn six_panel_objects() -> Vec<ObjectInfo> {
    let mut rows = vec![ObjectInfo {
        selection: Selection::Document(DocumentId(1)),
        name: "Document".into(),
        class: ClassId(1),
        depth: 0,
        layout_node: None,
    }];
    for (id, depth) in [(1, 1), (2, 2)] {
        let mut o = object(id, depth, None);
        let Selection::Object(a) = &mut o.selection else {
            panic!()
        };
        a.instance = InstanceId(40);
        a.references.clear();
        rows.push(o);
    }
    for instance in 0..6 {
        for (reference, node, depth) in [
            (0, 1, 2),
            (0, 2, 3),
            (0, 3, 3),
            (7, 7, 3),
            (7, 1, 4),
            (7, 2, 5),
            (7, 3, 5),
            (7, 4, 5),
            (7, 5, 5),
            (8, 8, 3),
            (8, 1, 4),
            (8, 2, 5),
            (8, 3, 5),
            (0, 9, 3),
            (0, 10, 4),
            (0, 11, 4),
        ] {
            let mut o = object(node, depth, None);
            let Selection::Object(a) = &mut o.selection else {
                panic!()
            };
            a.instance = InstanceId(0x1_0000 + instance);
            a.references = if reference != 0 && node != reference {
                vec![NodeId(reference)]
            } else {
                vec![]
            };
            rows.push(o);
        }
    }
    for (definition, count) in [(10, 8), (11, 2), (12, 5), (13, 3)] {
        let mut o = object(1, 1, None);
        o.selection = Selection::Definition(DefinitionId(definition));
        o.name = format!("Definition {definition}");
        rows.push(o);
        for node in 1..=count {
            let mut o = object(node, 2, None);
            o.selection = Selection::DefinitionNode {
                definition: DefinitionId(definition),
                node: NodeId(node),
            };
            rows.push(o);
        }
    }
    assert_eq!(rows.len(), 121);
    rows
}
fn count_nodes(n: &SceneNode) -> usize {
    1 + n.children.iter().map(count_nodes).sum::<usize>()
}
fn budget_layout(height: f32) -> Scene {
    // Conservative app reserve, including base app controls: 200 nodes/24
    // controls exceeds the fixture's 70 initial + four 21-node panel slots and
    // six panels' 18 controls. Do not measure chrome against an empty app.
    let mut root = canvas(
        0,
        Rect::new(0., 0., 1280., 800.),
        vec![
            canvas(10, Rect::new(0., 0., 208., height), vec![]),
            canvas(11, Rect::new(900., 0., 344., height), vec![]),
        ],
    );
    for id in 100..297 {
        let mut node = canvas(id, Rect::new(300., 0., 100., 32.), vec![]);
        if id < 124 {
            node.responder = Some(TouchResponder::Activate(Action::AppEvent(1)));
            node.semantics = Some(SwitchSemantics {
                label: "App control".into(),
                checked: false,
            });
        }
        root.children.push(node);
    }
    let scene = Scene::new(root).unwrap();
    assert_eq!(count_nodes(scene.root()), 200);
    assert_eq!(scene.controls().len(), 24);
    scene
}
fn populated_designer(base: &Scene) -> (Seed, Designer) {
    let mut s = seed();
    let mut d = Designer::new(&s).unwrap();
    let rows = six_panel_objects();
    d.selection = rows[7].selection.clone(); // Nested slider, four properties.
    let mut properties = vec![];
    for index in 0..4 {
        let mut p = property();
        p.cell = Some(200 + index);
        p.descriptor.id = PropertyId(index + 1);
        if let Some(PropertyAddress::Occurrence { path, instance }) = &mut p.address {
            path.property = p.descriptor.id;
            *instance = InstanceId(0x1_0000);
        }
        s.cells.push(Cell {
            id: 200 + index,
            owner: 0x1_0000,
            name: format!("property {index}"),
            default: p.descriptor.default.clone(),
            authored_override: None,
            inherit: Some(100),
            targets: vec![],
            choices: (0..8).map(|n| Value::Color([n, 2, 3, 255])).collect(),
        });
        properties.push(p);
    }
    d.rebuild(&s, rows, &properties, &[], "Active", true)
        .unwrap();
    d.sync(base).unwrap();
    assert!(
        d.actions.len() > 128,
        "full retained action model exceeds scene budget"
    );
    assert!(
        d.panels
            .iter()
            .flat_map(|p| &p.nodes)
            .map(count_nodes)
            .sum::<usize>()
            > 512
    );
    (s, d)
}
fn presented(d: &Designer, base: &Scene) -> Scene {
    let mut root = base.root().clone();
    d.present(&mut root, base, None, "Active").unwrap();
    let scene = Scene::new(root).expect("combined app/chrome scene fits 512 nodes");
    assert!(scene.controls().len() <= 128);
    scene
}
fn client_for_scene(d: Designer, scene: Scene) -> Client {
    let ui = crate::authoring::AppUi::new(
        "designer-input-test",
        "Designer input test",
        otrios_interfaces::canvas::StateSchema {
            id: "designer-input-v1".into(),
            fields: vec![],
        },
    )
    .build("Designer input test", vec![]);
    let mut mount = qualify(
        ui,
        "designer-input",
        &[],
        &VecDeque::new(),
        &[],
        Qualification {
            component_runtime: Default::default(),
            epoch: 1,
            private: false,
            writer: [1; 16],
        },
    )
    .unwrap();
    mount.designer = Some(d);
    mount.scene = OnceLock::from(Arc::new(scene));
    Client {
        active: Some(mount),
        ..Client::default()
    }
}
#[test]
fn full_six_panel_hierarchy_and_inspector_fit_combined_scene_throughout_scroll_range() {
    for height in [172., 588.] {
        // Tall and wide fixture panel heights.
        let base = budget_layout(height);
        let (_, mut d) = populated_designer(&base);
        let mut maximum_nodes = 0;
        let mut maximum_controls = 0;
        let retained = d.panels.iter().map(|p| p.nodes.clone()).collect::<Vec<_>>();
        for panel in 0..2 {
            let limit = d.panels[panel].scroll.info().limit[1];
            for offset in (0..=(limit / 13.).ceil() as u32).map(|n| n as f32 * 13.) {
                d.panels[panel].scroll.set_offset([0., offset]);
                let scene = presented(&d, &base);
                maximum_nodes = maximum_nodes.max(count_nodes(scene.root()));
                maximum_controls = maximum_controls.max(scene.controls().len());
            }
            d.panels[panel].scroll.set_offset([0., 0.]);
        }
        // Top, middle and bottom include occurrence and definition selections.
        for index in [0, 60, 120] {
            let selection = d.objects[index].selection.clone();
            let id = *d
                .actions
                .iter()
                .find(|(_, a)| **a == LocalAction::Select(selection.clone()))
                .unwrap()
                .0;
            let y = d.panels[0]
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .frame
                .y;
            d.panels[0].scroll.set_offset([0., y]);
            let scene = presented(&d, &base);
            let control = scene
                .controls()
                .into_iter()
                .find(|p| p.node.id == id)
                .unwrap();
            let point = [
                control.origin[0] + 200.,
                control.origin[1] + control.node.frame.height / 2.,
            ];
            assert!(scene.hit_path(point).iter().any(|p| p.node.id == id));
            assert_eq!(
                d.semantic_action(&scene, id),
                Some(LocalAction::Select(selection))
            );
        }
        for (panel, before) in d.panels.iter().zip(retained) {
            assert_eq!(
                panel.nodes, before,
                "panning preserves full retained rows/IDs"
            );
        }
        eprintln!(
            "contract-model panel height {height}: maximum combined {maximum_nodes} nodes/{maximum_controls} controls; retained {} nodes/{} actions",
            d.panels
                .iter()
                .flat_map(|p| &p.nodes)
                .map(count_nodes)
                .sum::<usize>(),
            d.actions.len()
        );
    }
}
#[test]
fn virtual_rows_clip_edges_and_reject_offscreen_or_retired_semantic_ids() {
    let base = budget_layout(588.);
    let (s, mut d) = populated_designer(&base);
    let row = d.panels[0].nodes[30].clone();
    for offset in [row.frame.y + 16., row.frame.y - 588. + 16.] {
        d.panels[0].scroll.set_offset([0., offset]);
        let scene = presented(&d, &base);
        let control = scene
            .controls()
            .into_iter()
            .find(|p| p.node.id == row.id)
            .unwrap();
        assert_eq!(control.node.frame.height, 16.);
        let point = [200., control.origin[1] + 0.1];
        assert!(scene.hit_path(point).iter().any(|p| p.node.id == row.id));
        assert!(d.semantic_action(&scene, row.id).is_some());
    }
    let initial_id = d.panels[0].nodes[0].id;
    d.panels[0].scroll.set_offset([0., 100.]);
    let scene = presented(&d, &base);
    assert!(
        scene.path(initial_id).is_some(),
        "offscreen overscan is retained in this slab"
    );
    assert_eq!(d.semantic_action(&scene, initial_id), None);
    let mut client = client_for_scene(d.clone(), scene);
    assert_eq!(client.designer_activate(initial_id).unwrap(), Some(false));
    d.panels[0].scroll.set_offset([0., 3000.]);
    assert!(presented(&d, &base).path(initial_id).is_none());
    d.rebuild(&s, six_panel_objects(), &[], &[], "Changed", true)
        .unwrap();
    d.sync(&base).unwrap();
    d.panels[0].scroll.set_offset([0., 0.]);
    let scene = presented(&d, &base);
    assert!(d.panels[0].nodes[0].id > initial_id);
    assert_eq!(d.semantic_action(&scene, initial_id), None);
    let mut client = client_for_scene(d, scene);
    assert_eq!(client.designer_activate(initial_id).unwrap(), Some(false));
}
#[test]
fn slab_pans_reuse_paint_buffers_and_capture_keeps_typed_action() {
    use crate::dashboard::retained_text::TextPresentation;
    let base = budget_layout(588.);
    let (s, mut d) = populated_designer(&base);
    let font = crate::dashboard::font::test_font();
    let mut paint = TextPresentation::default();
    // For a 588-unit viewport the former independent bottom edge crossed at
    // offset 20, even though the top remained in slab zero.
    d.panels[0].scroll.set_offset([0., 0.]);
    let edge = paint.compose(&presented(&d, &base), &font);
    for offset in [16., 24., 40.] {
        d.panels[0].scroll.set_offset([0., offset]);
        let frame = paint.compose(&presented(&d, &base), &font);
        assert_eq!(frame.generation, edge.generation);
        assert!(Arc::ptr_eq(&frame.mesh, &edge.mesh));
    }
    d.panels[0].scroll.set_offset([0., 40.]);
    let scene = presented(&d, &base);
    let id = d.panels[0].nodes[2].id;
    let action = d.actions[&id].clone();
    let capture = Capture {
        pointer: 1,
        start: [200., 50.],
        moving: false,
        action: Some(action.clone()),
        panel: Some(0),
    };
    let mut client = client_for_scene(d.clone(), scene.clone());
    client.active.as_mut().unwrap().layout_scene = OnceLock::from(Arc::new(base.clone()));
    for (phase, y, at) in [
        (PointerPhase::Down, 50., 1),
        (PointerPhase::Move, -250., 17),
    ] {
        assert_eq!(
            client
                .designer_pointer(
                    PointerEvent {
                        id: 1,
                        phase,
                        position: [200., y]
                    },
                    at
                )
                .unwrap(),
            Some(true)
        );
        let held = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        assert_eq!(held.capture.as_ref().unwrap().action, Some(action.clone()));
        assert_eq!(held.panels[0].nodes, d.panels[0].nodes);
    }
    assert_eq!(
        client
            .designer_pointer(
                PointerEvent {
                    id: 1,
                    phase: PointerPhase::Up,
                    position: [200., -250.]
                },
                33
            )
            .unwrap(),
        Some(true)
    );
    assert!(
        !client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .busy()
    );
    let first = paint.compose(&scene, &font);
    for offset in [41., 42., 43., 44.] {
        d.panels[0].scroll.set_offset([0., offset]);
        let scene = presented(&d, &base);
        let frame = paint.compose(&scene, &font);
        assert_eq!(frame.generation, first.generation);
        assert!(Arc::ptr_eq(&frame.mesh, &first.mesh));
        assert_eq!(
            d.tap_action(&capture, &scene, [200., 50.]),
            Some(action.clone())
        );
    }
    // Larger pans may replace the bounded geometry working set independently
    // of the larger semantic slab. Capture keeps its typed action and node IDs;
    // visibility still rejects an action that has scrolled away.
    let slab_nodes = d.panels[0].nodes.clone();
    for offset in [160., 200., 300.] {
        d.panels[0].scroll.set_offset([0., offset]);
        let scene = presented(&d, &base);
        paint.compose(&scene, &font);
        assert_eq!(d.panels[0].nodes, slab_nodes);
        assert_eq!(d.tap_action(&capture, &scene, [200., 50.]), None);
    }
    d.panels[0].scroll.set_offset([0., WINDOW_STEP + 48.]);
    let scene = presented(&d, &base);
    assert_ne!(
        paint.compose(&scene, &font).generation,
        first.generation,
        "crossing a slab boundary rebuilds the bounded presented paint topology"
    );
    assert_eq!(d.tap_action(&capture, &scene, [200., 50.]), None);
    // A rebuild while a capture is held cannot recycle its node ID or action.
    d.rebuild(
        &s,
        six_panel_objects().into_iter().rev().collect(),
        &[],
        &[],
        "Changed",
        true,
    )
    .unwrap();
    let scene = presented(&d, &base);
    assert!(!d.actions.contains_key(&id));
    assert_eq!(capture.action, Some(action));
    assert_eq!(d.tap_action(&capture, &scene, [200., 50.]), None);
}
#[test]
fn retained_cache_and_local_identity_exhaustion_are_explicit() {
    let mut b = Builder::new();
    for _ in 0..MAX_CACHED_NODES {
        b.id().unwrap();
    }
    assert_eq!(
        b.id().unwrap_err(),
        "Designer retained node budget exceeded"
    );
    let mut b = Builder::new();
    for id in 0..MAX_CACHED_ACTIONS {
        b.actions
            .insert(id as u32, LocalAction::Mode(DesignerMode::Test));
    }
    assert_eq!(
        b.button(0., 0., 240., "Mode", LocalAction::Mode(DesignerMode::Test))
            .unwrap_err(),
        "Designer retained action budget exceeded"
    );
    assert_eq!(
        Builder::at(END).id().unwrap_err(),
        "Designer local identity budget exhausted"
    );
    let mut d = Designer::new(&seed()).unwrap();
    assert_eq!(
        d.rebuild(
            &seed(),
            (0..=MAX_CACHED_NODES)
                .map(|n| object(n as u64, 0, None))
                .collect(),
            &[],
            &[],
            "Active",
            true
        )
        .unwrap_err(),
        "Designer object model budget exceeded"
    );
}

fn fixture_sync(
    doc: &mut crate::canvas::Document,
    session: &mut Session,
    adapter: &mut StateAdapter,
) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = adapter.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            adapter.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(adapter.confirmed());
            return;
        }
    }
    panic!("designer fixture sync did not settle");
}
fn mounted_six_panel_fixture(focus: bool) -> Client {
    mounted_fixture_session(focus).0
}
fn mounted_fixture_session(focus: bool) -> (Client, crate::canvas::Document, Session) {
    mounted_fixture_session_with_count(focus, 4)
}
fn mounted_fixture_session_with_count(
    focus: bool,
    extra: usize,
) -> (Client, crate::canvas::Document, Session) {
    mounted_fixture_session_with_ui(focus, extra, fixture_ui())
}
fn mounted_fixture_session_with_ui(
    focus: bool,
    extra: usize,
    ui: Ui,
) -> (Client, crate::canvas::Document, Session) {
    assert!(
        designer_fixture::manifest()
            .dependencies
            .contains(&FEATURE.into())
    );
    let mut authority =
        crate::canvas::Document::new(ui.state_schema.clone(), "designer-mount").unwrap();
    authority.configure_oib(&ui).unwrap();
    let (mut session, _) = authority.open_session(&[1; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let mut mount = qualify(
        ui,
        "designer-mount",
        &[],
        &VecDeque::new(),
        &[],
        Qualification {
            component_runtime: Default::default(),
            epoch: 1,
            private: false,
            writer: [1; 16],
        },
    )
    .unwrap();
    mount
        .doc
        .receive(&authority.generate(&mut session).unwrap().unwrap())
        .unwrap();
    fixture_sync(&mut authority, &mut session, &mut mount.doc);
    mount.refresh().unwrap();
    for _ in 0..extra {
        assert!(mount.doc.instantiate_oib(10, None).unwrap());
        fixture_sync(&mut authority, &mut session, &mut mount.doc);
        mount
            .refresh()
            .expect("real fixture plus local chrome admits each new root");
    }
    assert_eq!(mount.doc.oib_allocations().unwrap().len(), 2 + extra);
    let instance = mount.doc.oib_allocations().unwrap().last().unwrap().id;
    mount.designer.as_mut().unwrap().focus_layout = focus;
    mount.designer.as_mut().unwrap().revision = None;
    mount.invalidate();
    let mut client = Client {
        active: Some(mount),
        ..Client::default()
    };
    assert!(
        client
            .inspect_oib_selection(Selection::Object(ObjectAddress {
                instance: InstanceId(instance),
                references: vec![NodeId(7)],
                node: NodeId(1),
            }))
            .unwrap()
    );
    let m = client.active.as_ref().unwrap();
    let properties = crate::canvas::oib::document::properties(
        crate::canvas::oib::seed(&m.ui).unwrap(),
        &m.doc.oib_allocations().unwrap(),
        &m.designer.as_ref().unwrap().selection,
        &m.doc.oib_active().unwrap(),
    )
    .unwrap();
    assert_eq!(
        properties.len(),
        4,
        "selected slider has its real multi-property inspector"
    );
    if extra == 4 {
        assert_eq!(m.designer.as_ref().unwrap().objects.len(), 128);
    }
    if !focus {
        assert!(m.designer.as_ref().unwrap().actions.len() > 128);
    }
    (client, authority, session)
}
fn fixture_scroll_to(client: &mut Client, panel: usize, offset: f32) {
    let scene = client.scene().unwrap();
    let viewport = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .panels[panel]
        .viewport;
    let path = scene.path(u32::from(viewport)).unwrap();
    let p = path.last().unwrap();
    let point = [p.origin[0] + 10., p.origin[1] + 10.];
    let current = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .panels[panel]
        .scroll
        .info()
        .offset[1];
    client
        .scroll_at(client.epoch(), offset - current, point)
        .unwrap();
}
#[test]
fn actual_six_panel_mount_scrolls_all_rows_and_selected_inspector_within_combined_budgets() {
    let mut client = mounted_six_panel_fixture(false);
    let selected = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .selection
        .clone();
    for viewport in [[1280., 800.], [800., 1280.]] {
        let mut maximum_nodes = 0;
        let mut maximum_controls = 0;
        client.set_viewport(viewport).unwrap();
        client.tick(1, 0).unwrap();
        client.inspect_oib_selection(selected.clone()).unwrap();
        let m = client.active.as_ref().unwrap();
        let base = resolve_layout(m).unwrap();
        let base_nodes = count_nodes(base.root());
        assert!(base_nodes > 100, "actual six-panel app budget is included");
        let d = m.designer.as_ref().unwrap();
        let cached_nodes = d
            .panels
            .iter()
            .flat_map(|p| &p.nodes)
            .map(count_nodes)
            .sum::<usize>();
        assert!(
            base_nodes + cached_nodes > 512,
            "the uncullled combined scene would overflow"
        );
        for panel in 0..2 {
            let limit = client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .panels[panel]
                .scroll
                .info()
                .limit[1];
            for offset in (0..=(limit / 19.).ceil() as u32).map(|n| n as f32 * 19.) {
                fixture_scroll_to(&mut client, panel, offset);
                let scene = client.scene().expect("real app/chrome Scene::new succeeds");
                maximum_nodes = maximum_nodes.max(count_nodes(scene.root()));
                maximum_controls = maximum_controls.max(scene.controls().len());
                assert!(count_nodes(scene.root()) <= 512 && scene.controls().len() <= 128);
            }
            fixture_scroll_to(&mut client, panel, 0.);
        }
        // Real activation refreshes the selected inspector and invalidates old
        // local IDs, so resolve each next row from the current retained model.
        for index in [0, 60, 120] {
            let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
            let selection = d.objects[index].selection.clone();
            let id = *d
                .actions
                .iter()
                .find(|(_, a)| **a == LocalAction::Select(selection.clone()))
                .unwrap()
                .0;
            let y = d.panels[0]
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .frame
                .y;
            fixture_scroll_to(&mut client, 0, y);
            let scene = client.scene().unwrap();
            let control = scene
                .controls()
                .into_iter()
                .find(|p| p.node.id == id)
                .unwrap();
            let point = [
                control.origin[0] + control.node.frame.width - 0.1,
                control.origin[1] + control.node.frame.height / 2.,
            ];
            assert!(scene.hit_path(point).iter().any(|p| p.node.id == id));
            assert!(client.activate(client.epoch(), id).unwrap());
            assert_eq!(
                client
                    .active
                    .as_ref()
                    .unwrap()
                    .designer
                    .as_ref()
                    .unwrap()
                    .selection,
                selection
            );
            assert!(
                !client.activate(client.epoch(), id).unwrap(),
                "retired semantic IDs cannot retarget"
            );
        }
        eprintln!(
            "real six-panel viewport {viewport:?}: base {base_nodes} + retained {cached_nodes} nodes; maximum combined {maximum_nodes} nodes/{maximum_controls} controls"
        );
    }
}
#[test]
fn actual_fixture_clipped_taps_offscreen_activation_and_pan_reuse() {
    use crate::dashboard::retained_text::TextPresentation;
    let mut client = mounted_six_panel_fixture(false);
    fixture_scroll_to(&mut client, 0, 100.);
    let scene = client.scene().unwrap();
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let mode_id = d.panels[0].nodes[0].id;
    assert!(scene.path(mode_id).is_some()); // Retained overscan, clipped semantics.
    assert!(!client.activate(client.epoch(), mode_id).unwrap());
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .mode,
        DesignerMode::Edit
    );
    let font = crate::dashboard::font::test_font();
    let mut paint = TextPresentation::default();
    fixture_scroll_to(&mut client, 0, 40.);
    let first = paint.compose(client.scene().unwrap().as_ref(), &font);
    for offset in [41., 42., 43., 44.] {
        fixture_scroll_to(&mut client, 0, offset);
        let next = paint.compose(client.scene().unwrap().as_ref(), &font);
        assert_eq!(first.generation, next.generation);
        assert!(Arc::ptr_eq(&first.mesh, &next.mesh));
    }
    for edge in [0, 1] {
        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        let row = d.panels[0].nodes[30].clone();
        let selection = match &d.actions[&row.id] {
            LocalAction::Select(s) => s.clone(),
            _ => panic!(),
        };
        let scene = client.scene().unwrap();
        let path = scene.path(u32::from(d.panels[0].viewport)).unwrap();
        let height = path.last().unwrap().node.frame.height;
        fixture_scroll_to(
            &mut client,
            0,
            if edge == 0 {
                row.frame.y + 16.
            } else {
                row.frame.y - height + 16.
            },
        );
        let scene = client.scene().unwrap();
        let control = scene
            .controls()
            .into_iter()
            .find(|p| p.node.id == row.id)
            .unwrap();
        assert_eq!(control.node.frame.height, 16.);
        let point = [
            control.origin[0] + control.node.frame.width - 0.1,
            control.origin[1] + 0.1,
        ];
        for (phase, at) in [(PointerPhase::Down, 1), (PointerPhase::Up, 2)] {
            assert_eq!(
                client
                    .designer_pointer(
                        PointerEvent {
                            id: 1,
                            phase,
                            position: point
                        },
                        at
                    )
                    .unwrap(),
                Some(true)
            );
        }
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .selection,
            selection
        );
    }
}

#[test]
fn nonzero_selector_evaluates_numeric_state_and_rejects_other_types() {
    use otrios_interfaces::{
        canvas::{Field, Projection, Selector, StateSchema, Ui},
        layout::{Binding, Blueprint, Expr, Instance, Semantics, Shape, Template, TemplateNode},
    };
    let template = TemplateNode {
        id: 0,
        resize: None,
        frame: [
            Expr::Literal(Value::Number(0.)),
            Expr::Literal(Value::Number(0.)),
            Expr::Width,
            Expr::Height,
        ],
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: true },
        responder: Some(otrios_interfaces::layout::Responder::AppEvent {
            control: 1,
            drag: false,
        }),
        semantics: Some(Semantics {
            label: Expr::Literal(Value::Text("Normalized toggle".into())),
            checked: Expr::State(Binding::Local(1)),
        }),
        children: vec![],
    };
    let layout = crate::dashboard::layout::Layout::encode(&Blueprint {
        schema: "otrios-layout-v1".into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "Root".into(),
            parameters: vec![],
            root: template,
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    })
    .unwrap();
    for (value, expected) in [
        (Value::Number(0.), Some(false)),
        (Value::Number(1.), Some(true)),
        (Value::Number(1000.), Some(true)),
        (Value::Boolean(true), None),
        (Value::Text("1".into()), None),
        (Value::Number(f32::INFINITY), None),
        (Value::Number(-1.), None),
    ] {
        let ui = Ui {
            format: "otrios-canvas-v8".into(),
            ui_ref: "app:test/ui".into(),
            title: "Nonzero".into(),
            state_schema: StateSchema {
                id: "test-v1".into(),
                fields: vec![Field {
                    id: 1,
                    name: "checked".into(),
                    default: value,
                    writable: true,
                }],
            },
            layout: layout.clone(),
            selectors: vec![Selector {
                local: 1,
                field: 1,
                projection: Projection::Nonzero,
            }],
            handlers: vec![otrios_interfaces::canvas::Handler {
                control: 1,
                event: 1,
                kind: otrios_interfaces::canvas::GestureKind::Activate,
                operation: otrios_interfaces::canvas::Operation::Toggle,
                fields: vec![1],
            }],
            navigation: vec![],
            behavior: None,
            presentation: None,
        };
        let mount = qualify(
            ui,
            "nonzero",
            &[],
            &VecDeque::new(),
            &[],
            Qualification {
                component_runtime: Default::default(),
                epoch: 1,
                private: false,
                writer: [1; 16],
            },
        );
        if let Some(expected) = expected {
            let mut m = mount.unwrap();
            assert_eq!(
                resolve_layout(&m)
                    .unwrap()
                    .root()
                    .semantics
                    .as_ref()
                    .unwrap()
                    .checked,
                expected
            );
            m.doc.set(&[(1, if expected { 0 } else { 1000 })]).unwrap();
            assert_eq!(
                resolve_layout(&m)
                    .unwrap()
                    .root()
                    .semantics
                    .as_ref()
                    .unwrap()
                    .checked,
                !expected
            );
        } else {
            assert!(mount.is_err());
        }
    }
}

#[test]
fn frame_focus_drawer_geometry_hits_and_release_outside_are_local() {
    let mut client = mounted_six_panel_fixture(true);
    let m = client.active.as_ref().unwrap();
    let root_before = m.doc.oib_root();
    let fields_before = [1, 2, 3, 4].map(|f| m.read(f).unwrap());
    let seed = crate::canvas::oib::seed(&m.ui).unwrap();
    let hierarchy = u32::from(seed.designer.as_ref().unwrap().hierarchy_viewport.unwrap());
    let inspector = u32::from(seed.inspector_viewport);
    let preview = u32::from(seed.instance_surface.as_ref().unwrap().preview_viewport);
    for viewport in [[1280., 800.], [800., 1280.]] {
        client.set_viewport(viewport).unwrap();
        let closed = client.scene().unwrap();
        let panel = closed.path(inspector).unwrap();
        let panel = panel.last().unwrap();
        assert_eq!(panel.node.frame.width, 320.);
        assert!(panel.origin[0] > viewport[0] / 2.);
        let canvas = closed.path(preview).unwrap();
        assert_eq!(canvas.last().unwrap().origin, [16., 80.]);
        assert!(!closed.controls().iter().any(|c| {
            closed
                .path(c.node.id)
                .unwrap()
                .iter()
                .any(|p| p.node.id == hierarchy)
        }));
        assert!(
            client
                .designer_activate(super::super::oib_focus_layout::LAYERS)
                .unwrap()
                .unwrap()
        );
        let opened = client.scene().unwrap();
        let layer = opened.path(hierarchy).unwrap();
        assert_eq!(layer.last().unwrap().origin, [16., 128.]);
        let hit = opened.hit_path([200., 400.]);
        assert!(
            !hit.iter().any(|n| n.node.id == preview),
            "drawer barrier covers preview"
        );
        // Empty gutters need a real modal hit target too; decoration passes
        // through, and a Consume canvas alone does not establish a leaf hit.
        for point in [
            [310., 200.],
            [310., 66.],
            [1., 300.],
            [310., viewport[1] - 1.],
        ] {
            let hit = opened.hit_path(point);
            assert!(
                !hit.iter().any(|n| n.node.id == preview),
                "blank drawer gutter leaks at {point:?}"
            );
            assert!(
                hit.iter()
                    .any(|n| n.node.responder == Some(TouchResponder::Consume))
            );
            let mut router = crate::dashboard::input::InputRouter::default();
            for phase in [PointerPhase::Down, PointerPhase::Move, PointerPhase::Up] {
                let delivery = router.dispatch(
                    opened.clone(),
                    PointerEvent {
                        id: 991,
                        phase,
                        position: point,
                    },
                );
                assert!(
                    delivery.action.is_none() && delivery.gesture.is_none(),
                    "drawer gutter must never dispatch an app action or drag"
                );
            }
        }
        assert!(
            client
                .designer_activate(super::super::oib_focus_layout::LAYERS)
                .unwrap()
                .unwrap()
        );
    }
    for (phase, position) in [
        (PointerPhase::Down, [120., 24.]),
        (PointerPhase::Up, [600., 24.]),
    ] {
        assert_eq!(
            client
                .designer_pointer(
                    PointerEvent {
                        id: 777,
                        phase,
                        position
                    },
                    100
                )
                .unwrap(),
            Some(true)
        );
    }
    assert!(
        !client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .layers_open
    );
    let m = client.active.as_ref().unwrap();
    assert_eq!(m.doc.oib_root(), root_before);
    assert_eq!([1, 2, 3, 4].map(|f| m.read(f).unwrap()), fields_before);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}

#[test]
fn frame_focus_tree_disclosure_and_full_expansion_stay_bounded() {
    let mut client = mounted_six_panel_fixture(true);
    client
        .inspect_oib_selection(Selection::Document(DocumentId(1)))
        .unwrap();
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(d.tree.iter().any(|r| r.name == "Audio"));
    assert!(d.tree.iter().any(|r| r.name == "Display"));
    assert!(d.tree_closed.iter().any(|key| matches!(key, HierarchyKey::Selection(Selection::Object(a)) if !a.references.is_empty())));
    assert!(
        d.tree
            .iter()
            .all(|r| !matches!(r.selection, Some(Selection::Definition(_))))
    );
    for row in &d.tree {
        if let Some(parent) = &row.parent {
            assert!(d.tree.iter().any(|r| &r.key == parent));
        }
    }
    client
        .designer_activate(super::super::oib_focus_layout::LAYERS)
        .unwrap();
    {
        let m = client.active.as_mut().unwrap();
        let d = m.designer.as_mut().unwrap();
        d.tree_closed.clear();
        d.revision = None;
        m.refresh_designer().unwrap();
    }
    for viewport in [[1280., 800.], [800., 1280.]] {
        client.set_viewport(viewport).unwrap();
        client.tick(1, 0).unwrap();
        for panel in 0..2 {
            let limit = client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .panels[panel]
                .scroll
                .info()
                .limit[1];
            for offset in (0..=(limit / 51.).ceil() as u32).map(|i| i as f32 * 51.) {
                fixture_scroll_to(&mut client, panel, offset);
                let scene = client.scene().unwrap();
                assert!(count_nodes(scene.root()) <= 512);
                assert!(scene.controls().len() <= 128);
            }
            fixture_scroll_to(&mut client, panel, 0.);
        }
    }
    client
        .designer_action(LocalAction::TreeScope(HierarchyScope::Components))
        .unwrap();
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(
        d.tree
            .iter()
            .any(|r| matches!(r.selection, Some(Selection::Definition(_))))
    );
    assert!(
        d.tree
            .iter()
            .all(|r| !matches!(r.selection, Some(Selection::Object(_))))
    );
}

fn editor_address(property: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(property),
        },
    }
}
fn editor_override(client: &Client, property: u64) -> Option<Value> {
    let register = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&editor_address(property))
        .unwrap();
    assert_eq!(register.alternatives.len(), 1);
    register.alternatives[0].clone()
}
fn editor_value(client: &Client, property: u64) -> Value {
    let state = &client.active.as_ref().unwrap().doc;
    let address = editor_address(property);
    crate::canvas::oib::document::properties(
        &state.oib_authoring_seed().unwrap(),
        &state.oib_allocations().unwrap(),
        &editing::selection(&address),
        &state.oib_active().unwrap(),
    )
    .unwrap()
    .into_iter()
    .find(|field| field.address.as_ref() == Some(&address))
    .unwrap()
    .value
    .unwrap()
}
fn focus_property(client: &mut Client, property: u64) -> u32 {
    focus_address(client, editor_address(property))
}
fn focus_address(client: &mut Client, address: PropertyAddress) -> u32 {
    let m = client.active.as_mut().unwrap();
    m.font = Some(Arc::new(crate::dashboard::font::test_font()));
    let d = m.designer.as_ref().unwrap();
    let id = d.drafts.draft(&address).unwrap().fields[0].id;
    let y = d
        .panels
        .last()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap()
        .frame
        .y;
    fixture_scroll_to(client, 1, y - 8.);
    assert_eq!(client.designer_activate(id).unwrap(), Some(true));
    // Software keyboard reduces the dock height; keep the field visible.
    fixture_scroll_to(client, 1, y - 8.);
    id
}
fn settle_editor(
    client: &mut Client,
    authority: &mut crate::canvas::Document,
    session: &mut Session,
) {
    let m = client.active.as_mut().unwrap();
    fixture_sync(authority, session, &mut m.doc);
    m.refresh().unwrap();
}
#[test]
fn frame_focus_direct_inputs_commit_once_and_reset_without_changing_bindings() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let fields = [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap());
    let id = focus_property(&mut client, 3);
    let root = client.active.as_ref().unwrap().doc.oib_root();
    let epoch = client.epoch();
    assert!(
        client
            .edit_text_input(
                epoch,
                id,
                &Edit::Replace {
                    text: "Sound".into(),
                    cursor: 5,
                    anchor: 5,
                    selection_only: false
                }
            )
            .unwrap()
    );
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
    assert!(
        client
            .text_inputs()
            .unwrap()
            .iter()
            .any(|i| i.id == id && i.focused && i.value == "Sound")
    );
    assert!(client.edit_text_input(epoch, id, &Edit::Enter).unwrap());
    assert!(client.active.as_ref().unwrap().doc.oib_saving());
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(editor_value(&client, 3), Value::Text("Sound".into()));
    assert!(
        client
            .designer_action(LocalAction::Write(editor_address(3), None))
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(editor_value(&client, 3), Value::Text("Volume".into()));
    let id = focus_property(&mut client, 2);
    assert!(
        client
            .edit_text_input(
                epoch,
                id,
                &Edit::Replace {
                    text: "377.5".into(),
                    cursor: 5,
                    anchor: 5,
                    selection_only: false
                }
            )
            .unwrap()
    );
    assert!(client.blur_text_input(epoch, id).unwrap());
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(editor_override(&client, 2), Some(Value::Number(377.5)));
    assert_eq!(
        [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap()),
        fields
    );
}
#[test]
fn paint_dependencies_classify_accent_and_reject_width_text_and_composed_colors() {
    let (client, _, _) = mounted_fixture_session(true);
    let mount = client.active.as_ref().unwrap();
    let seed = super::super::super::oib::seed(&mount.ui).unwrap();
    let previous = mount.doc.oib_active().unwrap();
    for (property, value, paint_only) in [
        (1, Value::Color([19, 143, 207, 255]), true),
        (2, Value::Number(377.), false),
        (3, Value::Text("New label".into()), false),
    ] {
        let address = editor_address(property);
        let cell = super::super::super::oib::document::properties(
            &mount.doc.oib_authoring_seed().unwrap(),
            &mount.doc.oib_allocations().unwrap(),
            &editing::selection(&address),
            &previous,
        )
        .unwrap()
        .into_iter()
        .find(|p| p.address.as_ref() == Some(&address))
        .unwrap()
        .cell
        .unwrap();
        let mut next = previous.clone();
        next.insert(cell, Some(value));
        let result = super::super::super::oib::paint::changes(seed, &previous, &next).unwrap();
        assert_eq!(result.is_some(), paint_only);
        if paint_only {
            assert!(!result.unwrap().is_empty());
            let mut composed = seed.clone();
            let visual = composed
                .graph
                .as_mut()
                .unwrap()
                .definitions
                .iter_mut()
                .flat_map(|definition| &mut definition.nodes)
                .filter_map(|node| node.visual.as_mut())
                .find(|visual| {
                    matches!(
                        visual.shape,
                        otrios_interfaces::layout::Shape::Superellipse { .. }
                    )
                })
                .unwrap();
            if let otrios_interfaces::layout::Shape::Superellipse { color, .. } = &mut visual.shape
            {
                use otrios_interfaces::layout::Expr;
                *color = Expr::Select {
                    condition: Box::new(Expr::Literal(Value::Boolean(true))),
                    yes: Box::new(color.clone()),
                    no: Box::new(Expr::Literal(Value::Color([0, 0, 0, 255]))),
                };
            }
            assert!(
                super::super::super::oib::paint::changes(&composed, &previous, &next)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn inspector_rows_retain_only_matching_typed_control_identities() {
    let address = editor_address(1);
    let other = editor_address(2);
    let mut old_builder = Builder::new();
    let old = vec![
        old_builder
            .focus_button(0., 0., 148., "Apply", LocalAction::Apply(address.clone()))
            .unwrap(),
        old_builder
            .focus_button(
                164.,
                0.,
                148.,
                "Cancel",
                LocalAction::Discard(address.clone()),
            )
            .unwrap(),
        old_builder
            .label(0., 56., "Unchanged heading", WHITE)
            .unwrap(),
    ];
    let mut next_builder = Builder::at(old_builder.next);
    let mut next = vec![
        next_builder
            .label(0., 0., "New conflict notice", ACCENT)
            .unwrap(),
        next_builder
            .focus_button(0., 40., 148., "Apply", LocalAction::Apply(address.clone()))
            .unwrap(),
        next_builder
            .focus_button(
                164.,
                40.,
                148.,
                "Cancel",
                LocalAction::Discard(address.clone()),
            )
            .unwrap(),
        next_builder
            .label(0., 96., "Unchanged heading", WHITE)
            .unwrap(),
        next_builder
            .focus_button(0., 136., 148., "Apply", LocalAction::Apply(other.clone()))
            .unwrap(),
    ];
    let new_control = next[4].id;
    retain_inspector_ids(
        &old,
        &mut next,
        &old_builder.actions,
        &mut next_builder.actions,
        0..0,
    );
    assert_eq!(next[1].id, old[0].id);
    assert_eq!(next[1].children, old[0].children);
    assert_eq!(next[2].id, old[1].id);
    assert_ne!(
        next[3].id, old[2].id,
        "matching text alone cannot move a diagnostic identity to another row"
    );
    assert_eq!(next[4].id, new_control);
    assert_eq!(
        next_builder.actions.get(&old[0].id),
        Some(&LocalAction::Apply(address.clone()))
    );
    assert_eq!(
        next_builder.actions.get(&new_control),
        Some(&LocalAction::Apply(other))
    );

    let mut changed_builder = Builder::at(next_builder.next);
    let mut changed = vec![
        changed_builder
            .focus_button(0., 40., 148., "Apply", LocalAction::Discard(address))
            .unwrap(),
    ];
    let fresh = changed[0].id;
    retain_inspector_ids(
        &old[..1],
        &mut changed,
        &old_builder.actions,
        &mut changed_builder.actions,
        0..0,
    );
    assert_eq!(
        changed[0].id, fresh,
        "an old Apply ID cannot retarget a different action"
    );
}

fn inspector_controls(designer: &Designer) -> Vec<(u32, LocalAction)> {
    designer
        .panels
        .last()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|node| {
            designer
                .actions
                .get(&node.id)
                .map(|action| (node.id, action.clone()))
        })
        .collect()
}
fn assert_compatible_inspector_controls(before: &[(u32, LocalAction)], designer: &Designer) {
    let current = inspector_controls(designer);
    for (id, action) in before {
        if current.iter().any(|(_, next)| next == action) {
            assert_eq!(
                designer.actions.get(id),
                Some(action),
                "unchanged typed control keeps its identity"
            );
        }
    }
}

#[test]
fn confirmed_accent_waits_for_held_designer_input_before_paint_activation() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    let old_generation = client.active.as_ref().unwrap().oib_generation;
    let old_scene = cached_layout(client.active.as_ref().unwrap()).unwrap();
    {
        let mount = client.active.as_mut().unwrap();
        assert!(
            mount
                .doc
                .edit_oib_property(&editor_address(1), Some(Value::Color([19, 143, 207, 255])))
                .unwrap()
        );
        mount.designer.as_mut().unwrap().capture = Some(Capture {
            pointer: 77,
            start: [0., 0.],
            moving: false,
            action: None,
            panel: None,
        });
    }
    settle_editor(&mut client, &mut authority, &mut session);
    let held = client.active.as_ref().unwrap();
    assert!(held.doc.confirmed());
    assert_ne!(held.doc.oib_generation(), old_generation);
    assert_eq!(held.oib_generation, old_generation);
    assert_eq!(cached_layout(held).unwrap().root(), old_scene.root());
    let mount = client.active.as_mut().unwrap();
    mount.designer.as_mut().unwrap().capture = None;
    assert!(mount.refresh().unwrap());
    assert_eq!(mount.oib_generation, mount.doc.oib_generation());
    assert_eq!(
        cached_layout(mount).unwrap().root(),
        resolve_layout(mount).unwrap().root()
    );
    assert_ne!(cached_layout(mount).unwrap().root(), old_scene.root());
}

#[test]
fn confirmed_accent_keeps_focused_inspector_field_and_keyboard_geometry() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client
        .inspect_oib_selection(editing::selection(&editor_address(2)))
        .unwrap();
    let field = focus_property(&mut client, 2);
    let focused = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .drafts
        .focused()
        .cloned();
    let old_generation = client.active.as_ref().unwrap().oib_generation;
    let old_scene = cached_layout(client.active.as_ref().unwrap()).unwrap();
    {
        let mount = client.active.as_mut().unwrap();
        assert!(mount.keyboard.visible);
        assert!(
            mount
                .doc
                .edit_oib_property(&editor_address(1), Some(Value::Color([19, 143, 207, 255])))
                .unwrap()
        );
        assert!(!mount.doc.confirmed());
        mount.refresh().unwrap();
        assert_eq!(
            mount.oib_generation, old_generation,
            "unconfirmed local values cannot enter the displayed graph"
        );
        assert_eq!(cached_layout(mount).unwrap().root(), old_scene.root());
    }
    settle_editor(&mut client, &mut authority, &mut session);
    let mount = client.active.as_ref().unwrap();
    assert!(mount.keyboard.visible);
    assert_eq!(
        mount.designer.as_ref().unwrap().drafts.focused(),
        focused.as_ref()
    );
    assert!(
        mount
            .designer
            .as_ref()
            .unwrap()
            .drafts
            .drafts()
            .iter()
            .flat_map(|draft| &draft.fields)
            .any(|input| input.id == field)
    );
    assert_eq!(
        cached_layout(mount).unwrap().root(),
        resolve_layout(mount).unwrap().root()
    );
}

#[test]
fn display_root_accent_apply_confirms_once_without_changing_structure_or_app_values() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    let selection = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .root_selection(30)
        .unwrap();
    assert!(client.inspect_oib_selection(selection.clone()).unwrap());
    let address = {
        let mount = client.active.as_ref().unwrap();
        crate::canvas::oib::document::properties(
            &mount.doc.oib_authoring_seed().unwrap(),
            &mount.doc.oib_allocations().unwrap(),
            &selection,
            &mount.doc.oib_active().unwrap(),
        )
        .unwrap()
        .into_iter()
        .find(|property| property.descriptor.label == "Accent")
        .unwrap()
        .address
        .unwrap()
    };
    let values = [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap());
    let structure = authority.oib_status().unwrap().structural_generation;
    let audio_accent = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&PropertyAddress::Occurrence {
            instance: InstanceId(20),
            path: PropertyPath {
                references: vec![],
                node: NodeId(1),
                property: PropertyId(1),
            },
        })
        .unwrap();
    for (index, color) in [[153, 102, 204, 255], [52, 211, 153, 255]]
        .into_iter()
        .enumerate()
    {
        let generation = authority.oib_status().unwrap().generation;
        let (before_hierarchy, before_management, before_controls) = {
            let designer = client.active.as_ref().unwrap().designer.as_ref().unwrap();
            (
                designer.panels[0].nodes.clone(),
                designer.management_nodes.clone(),
                inspector_controls(designer),
            )
        };
        assert!(
            client
                .designer_action(LocalAction::PickColor(address.clone(), color))
                .unwrap()
        );
        assert_eq!(authority.oib_status().unwrap().generation, generation);
        {
            let designer = client.active.as_ref().unwrap().designer.as_ref().unwrap();
            assert_eq!(
                designer.panels[0].nodes, before_hierarchy,
                "local palette preview retains hierarchy identities"
            );
            assert_eq!(designer.management_nodes, before_management);
            assert_compatible_inspector_controls(&before_controls, designer);
        }
        assert!(
            client
                .active
                .as_mut()
                .unwrap()
                .doc
                .generate()
                .unwrap()
                .is_none(),
            "choosing an accent stays local until Apply"
        );
        assert!(activate_inspector_action(
            &mut client,
            &LocalAction::Apply(address.clone())
        ));
        assert!(client.active.as_ref().unwrap().doc.oib_saving());
        {
            let mount = client.active.as_ref().unwrap();
            let designer = mount.designer.as_ref().unwrap();
            assert!(!mount.doc.confirmed());
            assert_eq!(
                designer.panels[0].nodes, before_hierarchy,
                "Saving status retains hierarchy identities before confirmation"
            );
            assert_eq!(designer.management_nodes, before_management);
            assert_compatible_inspector_controls(&before_controls, designer);
        }
        let (held_layout, held_hierarchy, held_handlers, held_inspector) = {
            let mount = client.active.as_ref().unwrap();
            (
                cached_layout(mount).unwrap(),
                mount.designer.as_ref().unwrap().panels[0].nodes.clone(),
                mount.ui.handlers.clone(),
                mount
                    .designer
                    .as_ref()
                    .unwrap()
                    .panels
                    .last()
                    .unwrap()
                    .nodes
                    .clone(),
            )
        };
        settle_editor(&mut client, &mut authority, &mut session);
        let mount = client.active.as_ref().unwrap();
        assert_eq!(mount.ui.handlers, held_handlers);
        assert_compatible_inspector_controls(&before_controls, mount.designer.as_ref().unwrap());
        if index > 0 {
            let before: Vec<_> = held_inspector.iter().map(|node| node.id).collect();
            let after: Vec<_> = mount
                .designer
                .as_ref()
                .unwrap()
                .panels
                .last()
                .unwrap()
                .nodes
                .iter()
                .map(|node| node.id)
                .collect();
            assert_eq!(
                before, after,
                "value-only inspector refresh preserves row IDs"
            );
        }
        assert_eq!(
            mount.designer.as_ref().unwrap().panels[0].nodes,
            held_hierarchy,
            "paint activation keeps hierarchy rows and their semantic IDs"
        );
        let painted = cached_layout(mount).unwrap();
        assert_ne!(painted.root(), held_layout.root());
        assert_eq!(
            painted.root(),
            resolve_layout(mount).unwrap().root(),
            "incremental paint agrees with a full resolution of current state"
        );
        fn scene_structure(node: &SceneNode, out: &mut Vec<(u32, Rect, Option<TouchResponder>)>) {
            out.push((node.id, node.frame, node.responder));
            for child in &node.children {
                scene_structure(child, out);
            }
        }
        let mut before = Vec::new();
        let mut after = Vec::new();
        scene_structure(held_layout.root(), &mut before);
        scene_structure(painted.root(), &mut after);
        assert_eq!(
            before, after,
            "Accent preserves resolved layout and hit-test identities"
        );
        assert!(mount.doc.confirmed());
        assert_eq!(authority.oib_status().unwrap().generation, generation + 1);
        assert_eq!(
            authority.oib_status().unwrap().structural_generation,
            structure
        );
        assert_eq!(
            mount
                .doc
                .oib_property_register(&address)
                .unwrap()
                .alternatives,
            vec![Some(Value::Color(color))]
        );
        assert_eq!(
            mount
                .doc
                .oib_property_register(&audio_accent.address)
                .unwrap(),
            audio_accent
        );
        assert_eq!([1, 2, 3, 4].map(|id| mount.read(id).unwrap()), values);
        assert!(mount.designer.as_ref().unwrap().structure_tools.is_empty());
        assert_eq!(mount.designer.as_ref().unwrap().selection, selection);
        assert!(
            mount
                .designer
                .as_ref()
                .unwrap()
                .drafts
                .draft(&address)
                .is_some_and(|draft| !draft.dirty),
            "the next Apply starts from the newly confirmed accent"
        );
        client.designer_action(LocalAction::Layers).unwrap();
        let tools = &client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .structure_tools;
        assert!(
            !tools.is_empty(),
            "opening Layers prepares current authoring actions"
        );
        assert!(
            tools
                .iter()
                .all(|(_, operation)| operation.generation == structure)
        );
        client.designer_action(LocalAction::Layers).unwrap();
    }
}
#[test]
fn frame_focus_invalid_drafts_cancel_and_color_picker_stay_local_until_apply() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let id = focus_property(&mut client, 2);
    let epoch = client.epoch();
    let root = client.active.as_ref().unwrap().doc.oib_root();
    assert!(
        client
            .edit_text_input(
                epoch,
                id,
                &Edit::Replace {
                    text: "NaN".into(),
                    cursor: 3,
                    anchor: 3,
                    selection_only: false
                }
            )
            .unwrap()
    );
    client.blur_text_input(epoch, id).unwrap();
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(d.drafts.draft(&editor_address(2)).unwrap().dirty);
    assert!(d.drafts.draft(&editor_address(2)).unwrap().error.is_some());
    let new = focus_property(&mut client, 2);
    assert!(!client.cancel_text_input(epoch + 1, new).unwrap());
    assert!(client.cancel_text_input(epoch, new).unwrap());
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert!(
        !client
            .edit_text_input(epoch, new, &Edit::Commit("400".into()))
            .unwrap()
    );
    let address = editor_address(1);
    assert!(
        client
            .designer_action(LocalAction::PickColor(address.clone(), [12, 34, 56, 128]))
            .unwrap()
    );
    assert!(
        client
            .designer_action(LocalAction::Opacity(address.clone(), 50))
            .unwrap()
    );
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
    assert!(client.designer_action(LocalAction::Apply(address)).unwrap());
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(
        editor_override(&client, 1),
        Some(Value::Color([12, 34, 56, 128]))
    );
}

#[test]
fn frame_focus_software_keyboard_routes_shared_input_and_preserves_held_geometry() {
    let mut client = mounted_six_panel_fixture(true);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let field = focus_property(&mut client, 3);
    let scene = client.scene().unwrap();
    let key = scene
        .controls()
        .into_iter()
        .find(|n| n.node.responder == Some(TouchResponder::Activate(Action::KeyboardKey(34))))
        .unwrap();
    let point = [key.origin[0] + 8., key.origin[1] + 8.];
    let epoch = client.epoch();
    assert!(
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 88,
                    phase: PointerPhase::Down,
                    position: point
                },
                10
            )
            .unwrap()
    );
    assert!(client.input_busy());
    let old = client.active.as_ref().unwrap().viewport;
    client.set_viewport([800., 1280.]).unwrap();
    assert_eq!(client.active.as_ref().unwrap().viewport, old);
    assert!(
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 88,
                    phase: PointerPhase::Up,
                    position: point
                },
                11
            )
            .unwrap()
    );
    assert!(!client.input_busy());
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert_eq!(
        d.drafts
            .draft(&editor_address(3))
            .unwrap()
            .field(super::super::oib_property_editor::Part::Value)
            .unwrap()
            .buffer()
            .text(),
        "Volume "
    );
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
    let composed = client.scene().unwrap();
    assert!(count_nodes(composed.root()) <= 512 && composed.controls().len() <= 128);
    assert!(client.cancel_text_input(epoch, field).unwrap());
    assert!(!client.keyboard_visible());
    assert!(!client.editing());
}

fn activate_inspector_action(client: &mut Client, action: &LocalAction) -> bool {
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let id = *d.actions.iter().find(|(_, a)| *a == action).unwrap().0;
    let y = d
        .panels
        .last()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap()
        .frame
        .y;
    fixture_scroll_to(client, 1, y - 8.);
    client.designer_activate(id).unwrap().unwrap()
}
#[test]
fn frame_focus_source_navigation_targets_controlling_shared_property() {
    let mut client = mounted_six_panel_fixture(true);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let root = client.active.as_ref().unwrap().doc.oib_root();
    let at = editor_address(1);
    assert!(activate_inspector_action(
        &mut client,
        &LocalAction::Details(at)
    ));
    let shared = PropertyAddress::Definition {
        definition: DefinitionId(10),
        node: NodeId(1),
        property: PropertyId(1),
    };
    assert!(activate_inspector_action(
        &mut client,
        &LocalAction::Source(shared.clone())
    ));
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert_eq!(
        d.selection,
        Selection::DefinitionNode {
            definition: DefinitionId(10),
            node: NodeId(1)
        }
    );
    assert_eq!(d.details.as_ref().unwrap().terminal_shared, Some(shared));
    assert_eq!(d.tree_scope, HierarchyScope::Components);
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}
fn issue_editor_write(adapter: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = adapter.generate().unwrap() {
        if matches!(
            crate::canvas::decode_state_request(&bytes).unwrap(),
            otrios_interfaces::canvas::StateRequest::OibWrite { .. }
        ) {
            return bytes;
        }
    }
    panic!("missing OIB write");
}
#[test]
fn frame_focus_conflict_choice_is_explicit_and_stale_choice_cannot_resolve_new_state() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let ui = fixture_ui();
    let mut other = StateAdapter::for_ui(&ui, "designer-mount", &[], [2; 16]).unwrap();
    let (mut other_session, _) = authority.open_session(&[2; 16], 0).unwrap();
    other_session.set_oib_editable(true);
    other_session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    other
        .receive(&authority.generate(&mut other_session).unwrap().unwrap())
        .unwrap();
    fixture_sync(&mut authority, &mut other_session, &mut other);
    let at = editor_address(3);
    assert!(
        client
            .designer_action(LocalAction::Write(
                at.clone(),
                Some(Value::Text("Sound".into()))
            ))
            .unwrap()
    );
    let a = issue_editor_write(&mut client.active.as_mut().unwrap().doc);
    assert!(other.edit_oib_property(&at, None).unwrap());
    let b = issue_editor_write(&mut other);
    authority.receive(&mut session, &a).unwrap();
    authority.receive(&mut other_session, &b).unwrap();
    settle_editor(&mut client, &mut authority, &mut session);
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let view = d
        .registers
        .iter()
        .find(|v| v.address == at)
        .unwrap()
        .clone();
    assert!(view.alternatives.contains(&None));
    assert!(
        view.alternatives
            .contains(&Some(Value::Text("Sound".into())))
    );
    let choice = LocalAction::Choose(Box::new(view.clone()), Some(Value::Text("Sound".into())));
    assert!(activate_inspector_action(&mut client, &choice));
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_conflicts()
            .is_empty()
    );
    assert_eq!(
        editor_override(&client, 3),
        Some(Value::Text("Sound".into()))
    );
    let root = client.active.as_ref().unwrap().doc.oib_root();
    assert!(
        !client
            .designer_action(LocalAction::Choose(Box::new(view), None))
            .unwrap()
    );
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}

#[test]
fn frame_focus_managed_back_does_not_cover_layers_button() {
    let mut client = mounted_six_panel_fixture(true);
    let m = client.active.as_mut().unwrap();
    m.quit_control = Some(255);
    m.invalidate();
    let scene = client.scene().unwrap();
    let hit = scene.hit_path([120., 24.]);
    assert!(
        hit.iter()
            .any(|p| p.node.id == super::super::oib_focus_layout::LAYERS)
    );
    let back = scene
        .controls()
        .into_iter()
        .find(|c| c.node.semantics.as_ref().unwrap().label == "Back to OTriOS")
        .unwrap();
    assert_eq!([back.node.frame.width, back.node.frame.height], [48., 48.]);
    let epoch = client.epoch();
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        assert!(
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 9,
                        phase,
                        position: [120., 24.]
                    },
                    1
                )
                .unwrap()
        );
    }
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .layers_open
    );
}

#[test]
fn frame_focus_tab_reveals_below_fold_fields_without_admitting_stale_ids() {
    let mut client = mounted_six_panel_fixture(true);
    client.set_viewport([1280., 800.]).unwrap();
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let hex = focus_property(&mut client, 1);
    let tab = client
        .scene()
        .unwrap()
        .controls()
        .into_iter()
        .find(|c| c.node.responder == Some(TouchResponder::Activate(Action::KeyboardKey(11))))
        .unwrap()
        .node
        .id;
    assert_eq!(client.designer_activate(tab).unwrap(), Some(true));
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .drafts
            .focused()
            .unwrap()
            .part,
        super::super::oib_property_editor::Part::Opacity
    );
    assert_eq!(client.designer_activate(tab).unwrap(), Some(true));
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    // Traversal follows signed order and wraps back to the Layout field.
    assert_eq!(d.drafts.focused().unwrap().address, editor_address(3));
    assert_eq!(client.designer_activate(tab).unwrap(), Some(true));
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert_eq!(d.drafts.focused().unwrap().address, editor_address(2));
    let width = d.drafts.draft(&editor_address(2)).unwrap().fields[0].id;
    assert!(
        client
            .text_inputs()
            .unwrap()
            .iter()
            .any(|i| i.id == width && i.focused)
    );
    fixture_scroll_to(&mut client, 1, f32::MAX);
    assert_eq!(client.designer_activate(hex).unwrap(), Some(false));
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .drafts
            .focused()
            .unwrap()
            .address,
        editor_address(2)
    );
}
#[test]
fn frame_focus_source_navigation_waits_for_focused_edit_confirmation() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.set_viewport([1280., 800.]).unwrap();
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let id = focus_property(&mut client, 1);
    let epoch = client.epoch();
    client
        .edit_text_input(
            epoch,
            id,
            &Edit::Replace {
                text: "#123456".into(),
                cursor: 7,
                anchor: 7,
                selection_only: false,
            },
        )
        .unwrap();
    activate_inspector_action(&mut client, &LocalAction::Details(editor_address(1)));
    let shared = PropertyAddress::Definition {
        definition: DefinitionId(10),
        node: NodeId(1),
        property: PropertyId(1),
    };
    assert!(
        !activate_inspector_action(&mut client, &LocalAction::Source(shared.clone())),
        "navigation is pending, not reported complete"
    );
    assert!(client.active.as_ref().unwrap().doc.oib_saving());
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .pending_source,
        Some(shared)
    );
    settle_editor(&mut client, &mut authority, &mut session);
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert_eq!(
        d.selection,
        Selection::DefinitionNode {
            definition: DefinitionId(10),
            node: NodeId(1)
        }
    );
    assert!(d.pending_source.is_none());
    assert_eq!(
        editor_override(&client, 1),
        Some(Value::Color([0x12, 0x34, 0x56, 255]))
    );
}

#[test]
fn frame_focus_hierarchy_selection_after_valid_blur_waits_for_confirmation() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.set_viewport([1280., 800.]).unwrap();
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let id = focus_property(&mut client, 3);
    let epoch = client.epoch();
    client
        .edit_text_input(
            epoch,
            id,
            &Edit::Replace {
                text: "Sound".into(),
                cursor: 5,
                anchor: 5,
                selection_only: false,
            },
        )
        .unwrap();
    let target = Selection::Object(ObjectAddress {
        instance: InstanceId(30),
        references: vec![NodeId(7)],
        node: NodeId(1),
    });
    assert!(
        !client
            .designer_action(LocalAction::Select(target.clone()))
            .unwrap()
    );
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .pending_selection,
        Some(target.clone())
    );
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .selection,
        target
    );
    assert_eq!(
        editor_override(&client, 3),
        Some(Value::Text("Sound".into()))
    );
}

#[test]
fn frame_focus_split_buttons_and_color_swatches_keep_their_declared_widths() {
    let mut client = mounted_six_panel_fixture(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.designer_action(LocalAction::Layers).unwrap();
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let scope = *d
        .actions
        .iter()
        .find(|(_, a)| **a == LocalAction::TreeScope(HierarchyScope::Composition))
        .unwrap()
        .0;
    assert_eq!(
        client
            .scene()
            .unwrap()
            .path(scope)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        140.
    );
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    client.designer_action(LocalAction::Layers).unwrap();
    let action = LocalAction::Apply(editor_address(2));
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let apply = *d.actions.iter().find(|(_, a)| **a == action).unwrap().0;
    let y = d
        .panels
        .last()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == apply)
        .unwrap()
        .frame
        .y;
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert_eq!(
        client
            .scene()
            .unwrap()
            .path(apply)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        148.
    );
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let swatch = d
        .panels
        .last()
        .unwrap()
        .nodes
        .iter()
        .find(|n| {
            n.frame.x == 0. && matches!(d.actions.get(&n.id), Some(LocalAction::PickColor(_, _)))
        })
        .unwrap();
    let (id, y) = (swatch.id, swatch.frame.y);
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert_eq!(
        client
            .scene()
            .unwrap()
            .path(id)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        72.
    );
}

#[test]
fn frame_focus_outlined_keyboard_and_both_panes_keep_combined_limits() {
    for viewport in [[1280., 800.], [800., 1280.]] {
        let mut client = mounted_six_panel_fixture(true);
        client.set_viewport(viewport).unwrap();
        client
            .inspect_oib_selection(Selection::Object(ObjectAddress {
                instance: InstanceId(20),
                references: vec![NodeId(7)],
                node: NodeId(1),
            }))
            .unwrap();
        client.designer_action(LocalAction::Layers).unwrap();
        {
            let m = client.active.as_mut().unwrap();
            let d = m.designer.as_mut().unwrap();
            d.tree_closed.clear();
            d.revision = None;
            m.refresh_designer().unwrap();
        }
        let before = client.active.as_ref().unwrap().doc.oib_root();
        let mut maximum_nodes = 0;
        let mut maximum_controls = 0;
        for property in [2, 1, 3] {
            let field = focus_property(&mut client, property);
            assert!(client.keyboard_visible());
            let limits: Vec<_> = client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .panels
                .iter()
                .map(|p| p.scroll.info().limit[1])
                .collect();
            for first in [0., 151., 303., 305., 607., limits[0]] {
                for second in [0., 151., 303., 305., 607., limits[1]] {
                    fixture_scroll_to(&mut client, 0, first);
                    fixture_scroll_to(&mut client, 1, second);
                    let scene = client
                        .scene()
                        .expect("outlined panes plus keyboard admit together");
                    maximum_nodes = maximum_nodes.max(count_nodes(scene.root()));
                    maximum_controls = maximum_controls.max(scene.controls().len());
                    assert!(count_nodes(scene.root()) <= 512);
                    assert!(scene.controls().len() <= 128);
                    assert_eq!(
                        scene
                            .controls()
                            .iter()
                            .filter(|c| matches!(c.node.responder,
                        Some(TouchResponder::Activate(Action::KeyboardKey(key))) if key < 38))
                            .count(),
                        38
                    );
                }
            }
            client.cancel_text_input(client.epoch(), field).unwrap();
        }
        assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), before);
        eprintln!(
            "outlined six-root keyboard {viewport:?}: {maximum_nodes} nodes/{maximum_controls} controls"
        );
    }
}

#[test]
fn desktop_studio_keeps_three_panels_and_resizes_locally_with_cancel() {
    use super::super::oib_focus_layout::{PROFILE, RESIZE_LEFT, RESIZE_RIGHT};
    let mut client = mounted_six_panel_fixture(true);
    client.set_viewport([1280., 800.]).unwrap();
    let generation = client.active.as_ref().unwrap().doc.oib_generation();
    let state = client.active.as_ref().unwrap().doc.oib_root();
    assert!(client.set_oib_designer_studio(true).unwrap());
    let seed = crate::canvas::oib::seed(&client.active.as_ref().unwrap().ui).unwrap();
    let hierarchy = u32::from(seed.designer.as_ref().unwrap().hierarchy_viewport.unwrap());
    let inspector = u32::from(seed.inspector_viewport);
    let preview = u32::from(seed.instance_surface.as_ref().unwrap().preview_viewport);
    let scene = client.scene().unwrap();
    assert_eq!(
        scene.path(hierarchy).unwrap().last().unwrap().origin,
        [32., 144.]
    );
    assert_eq!(
        scene.path(preview).unwrap().last().unwrap().origin,
        [352., 80.]
    );
    assert_eq!(
        scene
            .path(inspector)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        320.
    );
    assert!(scene.controls().iter().any(|c| {
        scene
            .path(c.node.id)
            .unwrap()
            .iter()
            .any(|p| p.node.id == hierarchy)
    }));
    assert!(scene.controls().len() <= 128);
    for (handle, dx, cancel) in [(RESIZE_LEFT, 80., false), (RESIZE_RIGHT, -64., true)] {
        let scene = client.scene().unwrap();
        let handle_path = scene.path(handle).unwrap();
        let pos = handle_path.last().unwrap().origin;
        let point = [pos[0] + 8., pos[1] + 200.];
        let before = client.designer_studio.widths;
        for (phase, offset) in [
            (PointerPhase::Down, 0.),
            (PointerPhase::Move, dx),
            (
                if cancel {
                    PointerPhase::Cancel
                } else {
                    PointerPhase::Up
                },
                dx,
            ),
        ] {
            assert!(
                client
                    .pointer_at(
                        client.epoch(),
                        PointerEvent {
                            id: 4,
                            phase,
                            position: [point[0] + offset, point[1]],
                        },
                        10
                    )
                    .unwrap()
            );
        }
        if cancel {
            assert_eq!(client.designer_studio.widths, before);
        } else {
            assert_eq!(client.designer_studio.widths[0], 400.);
        }
        assert!(!client.input_busy());
        assert!(client.scene().unwrap().controls().len() <= 128);
    }
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), state);
    assert_eq!(
        client.active.as_ref().unwrap().doc.oib_generation(),
        generation
    );
    assert!(client.designer_activate(PROFILE).unwrap().unwrap());
    assert!(!client.designer_studio.enabled);
    assert!(client.designer_activate(PROFILE).unwrap().unwrap());
    assert_eq!(client.designer_studio.widths[0], 400.);
}

#[test]
fn desktop_studio_expanded_panes_and_editor_keep_clips_and_combined_budgets() {
    let mut maximum_nodes = 0;
    let mut maximum_controls = 0;
    for viewport in [[800., 1280.], [1280., 800.], [1920., 1080.]] {
        for widths in [[320., 352.], [480., 560.]] {
            let mut client = mounted_six_panel_fixture(true);
            client.set_viewport(viewport).unwrap();
            client.set_oib_designer_studio(true).unwrap();
            client
                .inspect_oib_selection(Selection::Object(ObjectAddress {
                    instance: InstanceId(20),
                    references: vec![NodeId(7)],
                    node: NodeId(1),
                }))
                .unwrap();
            {
                let m = client.active.as_mut().unwrap();
                let d = m.designer.as_mut().unwrap();
                d.studio.widths = widths;
                d.tree_closed.clear();
                d.revision = None;
                m.refresh_designer().unwrap();
                m.invalidate();
            }
            let before = client.active.as_ref().unwrap().doc.oib_root();
            for keyboard in [false, true] {
                eprintln!("Studio case {viewport:?} widths={widths:?} keyboard={keyboard}");
                let field = keyboard.then(|| focus_property(&mut client, 2));
                let limits: Vec<_> = client
                    .active
                    .as_ref()
                    .unwrap()
                    .designer
                    .as_ref()
                    .unwrap()
                    .panels
                    .iter()
                    .map(|p| p.scroll.info().limit[1])
                    .collect();
                for first in [0., 303., 305., 607., limits[0]] {
                    for second in [0., 303., 305., 607., limits[1]] {
                        fixture_scroll_to(&mut client, 0, first);
                        fixture_scroll_to(&mut client, 1, second);
                        let scene = client
                            .scene()
                            .expect("both Studio panes and six roots admit");
                        maximum_nodes = maximum_nodes.max(count_nodes(scene.root()));
                        maximum_controls = maximum_controls.max(scene.controls().len());
                        assert!(count_nodes(scene.root()) <= 512);
                        assert!(scene.controls().len() <= 128);
                        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
                        for panel in &d.panels {
                            let path = scene.path(u32::from(panel.viewport)).unwrap();
                            let p = path.last().unwrap();
                            let parent = path[path.len() - 2];
                            assert!(
                                p.origin[1] + p.node.frame.height
                                    <= parent.origin[1] + parent.node.frame.height
                            );
                            assert!(
                                p.origin[0] + p.node.frame.width
                                    <= parent.origin[0] + parent.node.frame.width
                            );
                        }
                    }
                }
                if let Some(field) = field {
                    client.cancel_text_input(client.epoch(), field).unwrap();
                }
            }
            assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), before);
        }
    }
    eprintln!(
        "Studio six-root joint panel cases: maximum {maximum_nodes} nodes/{maximum_controls} controls"
    );
}

#[test]
fn desktop_studio_resize_limits_pointer_identity_and_hidden_handles_are_safe() {
    use super::super::oib_focus_layout::{RESIZE_LEFT, RESIZE_RIGHT};
    let mut client = mounted_six_panel_fixture(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let before = client.active.as_ref().unwrap().doc.oib_root();
    for (id, delta, expected) in [(RESIZE_LEFT, 2000., 480.), (RESIZE_RIGHT, -2000., 560.)] {
        let scene = client.scene().unwrap();
        let path = scene.path(id).unwrap();
        let p = path.last().unwrap();
        let pos = [p.origin[0] + 8., p.origin[1] + 20.];
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 9,
                    phase: PointerPhase::Down,
                    position: pos,
                },
                1,
            )
            .unwrap();
        // A second pointer cannot release or retarget the captured divider.
        assert!(
            !client
                .pointer_at(
                    client.epoch(),
                    PointerEvent {
                        id: 10,
                        phase: PointerPhase::Up,
                        position: pos
                    },
                    2
                )
                .unwrap()
        );
        for phase in [PointerPhase::Move, PointerPhase::Up] {
            client
                .pointer_at(
                    client.epoch(),
                    PointerEvent {
                        id: 9,
                        phase,
                        position: [pos[0] + delta, pos[1]],
                    },
                    3,
                )
                .unwrap();
        }
        let side = usize::from(id == RESIZE_RIGHT);
        assert_eq!(client.designer_studio.widths[side], expected);
        assert!(!client.input_busy());
    }
    // Host blur/cancel restores the same preference as a pointer Cancel. A
    // later profile switch must not resurrect a canceled preview width.
    let widths = client.designer_studio.widths;
    let scene = client.scene().unwrap();
    let path = scene.path(RESIZE_LEFT).unwrap();
    let p = path.last().unwrap();
    let pos = [p.origin[0] + 8., p.origin[1] + 20.];
    for (phase, delta) in [(PointerPhase::Down, 0.), (PointerPhase::Move, -80.)] {
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 9,
                    phase,
                    position: [pos[0] + delta, pos[1]],
                },
                4,
            )
            .unwrap();
    }
    assert_ne!(client.designer_studio.widths, widths);
    client.cancel().unwrap();
    assert_eq!(client.designer_studio.widths, widths);
    assert!(!client.input_busy());
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), before);
    client.set_oib_designer_studio(false).unwrap();
    let scene = client.scene().unwrap();
    assert!(
        !scene
            .controls()
            .iter()
            .any(|p| [RESIZE_LEFT, RESIZE_RIGHT].contains(&p.node.id))
    );
    assert!(!client.activate(client.epoch(), RESIZE_LEFT).unwrap());
    client.set_oib_designer_studio(true).unwrap();
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .studio
            .widths,
        widths
    );
}

fn activate_studio_tool(client: &mut Client, label: &str) -> u32 {
    let (id, y) = {
        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        let row = d.panels[0]
            .nodes
            .iter()
            .find(|n| n.semantics.as_ref().is_some_and(|s| s.label == label))
            .unwrap_or_else(|| {
                panic!(
                    "Missing Studio tool: {label}; confirmed={} status={} factories={:?}",
                    client.active.as_ref().unwrap().doc.confirmed(),
                    client.active.as_ref().unwrap().doc.oib_status_text(),
                    client.active.as_ref().unwrap().doc.oib_canvas_factories()
                )
            });
        (row.id, row.frame.y)
    };
    fixture_scroll_to(client, 0, y - 8.);
    assert!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .any(|p| p.node.id == id),
        "{label} must be presented"
    );
    assert!(
        client.activate(client.epoch(), id).unwrap(),
        "activate {label}"
    );
    id
}
fn activate_studio_root(client: &mut Client, instance: u64) {
    let selection = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .root_selection(instance)
        .unwrap();
    let (id, y) = {
        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        let row = d.panels[0]
            .nodes
            .iter()
            .find(|n| d.actions.get(&n.id) == Some(&LocalAction::Select(selection.clone())))
            .expect("Root has a real hierarchy row");
        (row.id, row.frame.y)
    };
    fixture_scroll_to(client, 0, y - 8.);
    let selected = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .selection
        .clone();
    let changed = client.activate(client.epoch(), id).unwrap();
    assert!(
        changed || selected == selection,
        "Hierarchy root {instance} failed selection"
    );
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .selection,
        selection
    );
}

#[test]
fn desktop_studio_visible_palette_authors_power_and_reorders_without_retargeting() {
    let (mut client, mut authority, mut session) = mounted_fixture_session_with_count(true, 0);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let temporary: Vec<_> = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_instances()
        .unwrap()
        .into_iter()
        .filter(|i| i.id != i.prototype)
        .map(|i| i.id)
        .collect();
    for id in temporary {
        assert!(client.remove_oib(id).unwrap());
        settle_editor(&mut client, &mut authority, &mut session);
    }
    let fields = [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap());
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .edit_oib_property(&editor_address(3), Some(Value::Text("Sound".into())))
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    let audio_before = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&editor_address(3))
        .unwrap();
    let display_address = PropertyAddress::Occurrence {
        instance: InstanceId(30),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    let display_before = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&display_address)
        .unwrap();
    // The local palette and its operation come from the canonical signed factory,
    // even though its template is absent from the compiled resident document.
    activate_studio_tool(&mut client, "Insert component...");
    let stale_insert = activate_studio_tool(&mut client, "Insert Power");
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(!client.activate(client.epoch(), stale_insert).unwrap());
    let power = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|i| i.component == 13 && i.active)
        .expect("palette insertion publishes Power");
    assert_eq!(power.prototype, 50);
    assert_ne!(power.id, power.prototype);
    activate_studio_root(&mut client, power.id);
    let label = PropertyAddress::Occurrence {
        instance: InstanceId(power.id),
        path: PropertyPath {
            references: vec![],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    let (field, y) = {
        let m = client.active.as_mut().unwrap();
        m.font = Some(Arc::new(crate::dashboard::font::test_font()));
        let d = m.designer.as_ref().unwrap();
        let field = d
            .drafts
            .draft(&label)
            .expect("new root has shared typed property editor")
            .fields[0]
            .id;
        let y = d.panels[1]
            .nodes
            .iter()
            .find(|n| n.id == field)
            .unwrap()
            .frame
            .y;
        let checked = d
            .properties
            .iter()
            .find(|p| p.descriptor.id == PropertyId(5))
            .unwrap();
        assert_eq!(checked.source, "bound");
        assert!(checked.cell.is_none());
        (field, y)
    };
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert!(client.activate(client.epoch(), field).unwrap());
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert!(
        client
            .edit_text_input(
                client.epoch(),
                field,
                &Edit::Replace {
                    text: "Main power".into(),
                    cursor: 10,
                    anchor: 10,
                    selection_only: false,
                }
            )
            .unwrap()
    );
    assert!(
        client
            .edit_text_input(client.epoch(), field, &Edit::Enter)
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    let power_before = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&label)
        .unwrap();
    assert_eq!(
        power_before.alternatives,
        vec![Some(Value::Text("Main power".into()))]
    );
    let order = |c: &Client| {
        c.active
            .as_ref()
            .unwrap()
            .doc
            .oib_instances()
            .unwrap()
            .into_iter()
            .filter(|i| i.active)
            .map(|i| i.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&client), [20, 30, power.id]);
    for (instance, tool, expected) in [
        (20, "Move later", vec![30, 20, power.id]),
        (20, "Move earlier", vec![20, 30, power.id]),
        (30, "Move earlier", vec![30, 20, power.id]),
        (30, "Move later", vec![20, 30, power.id]),
    ] {
        activate_studio_root(&mut client, instance);
        let stale_move = activate_studio_tool(&mut client, tool);
        settle_editor(&mut client, &mut authority, &mut session);
        assert_eq!(order(&client), expected);
        assert!(!client.activate(client.epoch(), stale_move).unwrap());
        let m = client.active.as_ref().unwrap();
        assert_eq!(
            m.doc.oib_property_register(&editor_address(3)).unwrap(),
            audio_before
        );
        assert_eq!(
            m.doc.oib_property_register(&display_address).unwrap(),
            display_before
        );
        assert_eq!(m.doc.oib_property_register(&label).unwrap(), power_before);
        assert_eq!([1, 2, 3, 4].map(|f| m.read(f).unwrap()), fields);
    }
    let m = client.active.as_ref().unwrap();
    let seed = m.doc.oib_authoring_seed().unwrap();
    let expanded =
        crate::canvas::oib::instances::expanded_seed(&seed, &m.doc.oib_allocations().unwrap())
            .unwrap();
    let canonical = seed
        .designer
        .as_ref()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id == InstanceId(50))
        .unwrap();
    let current = expanded
        .designer
        .as_ref()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id == InstanceId(power.id))
        .unwrap();
    let app_binding = |i: &otrios_interfaces::oib::designer::ComponentInstance| {
        i.assignments
            .iter()
            .find(|a| a.target.property == PropertyId(5))
            .unwrap()
            .source
            .clone()
    };
    assert!(matches!(
        app_binding(current),
        AssignmentSource::AppBinding(_)
    ));
    assert_eq!(app_binding(current), app_binding(canonical));
    assert!(client.confirmed());
    assert!(!client.input_busy());
    assert!(client.scene().unwrap().controls().len() <= 128);
}

#[test]
fn desktop_studio_structural_rejection_stays_visible_above_scrolled_properties() {
    use super::super::oib_focus_layout::PROPERTIES_TITLE;
    use otrios_interfaces::oib::authoring::EditBatch as InstanceOperation;
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    fixture_scroll_to(&mut client, 1, 600.);
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .panels[1]
            .scroll
            .info()
            .offset[1]
            > 150.
    );
    let captured_generation = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_structural_generation()
        .unwrap();
    let stale = InstanceOperation::reorder(captured_generation, 30, Some(20));

    let ui = fixture_ui();
    let mut rival = StateAdapter::for_ui(&ui, "designer-mount", &[], [2; 16]).unwrap();
    let (mut rival_session, _) = authority.open_session(&[2; 16], 0).unwrap();
    rival_session.set_oib_editable(true);
    rival_session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    rival
        .receive(&authority.generate(&mut rival_session).unwrap().unwrap())
        .unwrap();
    fixture_sync(&mut authority, &mut rival_session, &mut rival);
    assert!(rival.reorder_oib(20, None).unwrap());
    fixture_sync(&mut authority, &mut rival_session, &mut rival);
    let accepted_status = authority.oib_status().unwrap();
    let accepted_order = authority.oib_allocations().unwrap();
    let accepted_ui = authority.oib_compiled(2).unwrap().0;
    assert_eq!(accepted_status.generation, captured_generation + 1);
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(
        client.active.as_ref().unwrap().doc.oib_generation(),
        Some(accepted_status.generation)
    );

    // The captured action is sent unchanged after the rival publication reaches
    // this replica. Its durable rejection must remain visible outside the
    // inspector's scrolled content, where the full notice cannot be seen.
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .edit_oib_structure(stale)
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(client.confirmed());
    assert!(!client.active.as_ref().unwrap().doc.oib_saving());
    let scene = client.scene().unwrap();
    let title_path = scene.path(PROPERTIES_TITLE).unwrap();
    let title = title_path.last().unwrap();
    assert!(
        matches!(&title.node.primitive, Primitive::Text { value, color, .. } if value == "Change rejected" && *color == ACCENT)
    );
    let m = client.active.as_ref().unwrap();
    let d = m.designer.as_ref().unwrap();
    assert!(d.panels[1].scroll.info().offset[1] > 150.);
    assert!(
        d.editor_notice
            .as_ref()
            .is_some_and(|s| s.contains("Structure rejected:"))
    );
    let notice = d.panels[1].nodes.iter().find(|n| matches!(&n.primitive, Primitive::Text { value, .. } if value.contains("Structure rejected:")))
        .expect("full rejection detail remains in the inspector");
    assert!(
        scene.path(notice.id).is_none(),
        "detail is outside the retained scroll window"
    );
    assert!(
        title.origin[1]
            < scene
                .path(u32::from(d.panels[1].viewport))
                .unwrap()
                .last()
                .unwrap()
                .origin[1]
    );
    let rejected = authority.oib_status().unwrap();
    assert_eq!(rejected.generation, accepted_status.generation);
    assert_eq!(rejected.active_root, accepted_status.active_root);
    assert_eq!(authority.oib_allocations().unwrap(), accepted_order);
    assert_eq!(authority.oib_compiled(2).unwrap().0, accepted_ui);

    assert!(client.reorder_oib(30, Some(20)).unwrap());
    settle_editor(&mut client, &mut authority, &mut session);
    let scene = client.scene().unwrap();
    let title = scene.path(PROPERTIES_TITLE).unwrap();
    assert!(
        matches!(&title.last().unwrap().node.primitive, Primitive::Text { value, color, .. } if value == "Properties" && *color == WHITE)
    );
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .editor_notice
            .is_none()
    );
    assert_eq!(
        authority.oib_status().unwrap().generation,
        accepted_status.generation + 1
    );
    assert_ne!(authority.oib_allocations().unwrap(), accepted_order);
    assert!(client.confirmed());
}

fn studio_scene_control(client: &Client, label: &str) -> (u32, [f32; 2]) {
    let scene = client.scene().unwrap();
    let control = scene
        .controls()
        .into_iter()
        .find(|p| p.node.semantics.as_ref().is_some_and(|s| s.label == label))
        .unwrap_or_else(|| panic!("Missing visible Studio control {label}"));
    (
        control.node.id,
        [
            control.origin[0] + control.node.frame.width / 2.,
            control.origin[1] + control.node.frame.height / 2.,
        ],
    )
}

#[test]
fn desktop_studio_visible_component_management_edits_and_deletes_inserted_power_safely() {
    let (mut client, mut authority, mut session) = mounted_fixture_session_with_count(true, 0);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let temporary: Vec<_> = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_instances()
        .unwrap()
        .into_iter()
        .filter(|i| i.id != i.prototype)
        .map(|i| i.id)
        .collect();
    for id in temporary {
        assert!(client.remove_oib(id).unwrap());
        settle_editor(&mut client, &mut authority, &mut session);
    }
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .edit_oib_property(&editor_address(3), Some(Value::Text("Sound".into())))
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    let fields = [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap());
    let original = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&editor_address(3))
        .unwrap();
    activate_studio_tool(&mut client, "Insert component...");
    activate_studio_tool(&mut client, "Insert Power");
    settle_editor(&mut client, &mut authority, &mut session);
    let power = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|i| i.component == 13)
        .unwrap();
    let descendant = Selection::Object(ObjectAddress {
        instance: InstanceId(power.id),
        references: vec![],
        node: NodeId(3),
    });
    let scene = client.scene().unwrap();
    let viewport = client
        .active
        .as_ref()
        .unwrap()
        .preview_scroll
        .as_ref()
        .unwrap()
        .node();
    let path = scene.path(viewport).unwrap();
    let preview = path.last().unwrap();
    client
        .scroll_at(
            client.epoch(),
            10_000.,
            [
                preview.origin[0] + preview.node.frame.width * preview.scale / 2.,
                preview.origin[1] + preview.node.frame.height * preview.scale / 2.,
            ],
        )
        .unwrap();
    let layout_node = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.selection == descendant)
        .expect("Inserted Power label is an inspectable object")
        .layout_node
        .expect("Inserted Power label has its compiled renderer mapping");
    let scene = client.scene().unwrap();
    let path = scene
        .path(u32::from(layout_node))
        .expect("Inserted Power label is retained in the presented scene");
    let object = path.last().unwrap();
    let point = [
        object.origin[0] + object.node.frame.width * object.scale / 2.,
        object.origin[1] + object.node.frame.height * object.scale / 2.,
    ];
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        assert!(
            client
                .pointer_at(
                    client.epoch(),
                    PointerEvent {
                        id: 71,
                        phase,
                        position: point
                    },
                    1
                )
                .unwrap()
        );
    }
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .selection,
        descendant
    );
    let (edit, _) = studio_scene_control(&client, "Edit component");
    assert!(client.activate(client.epoch(), edit).unwrap());
    let root = Selection::Object(ObjectAddress {
        instance: InstanceId(power.id),
        references: vec![],
        node: NodeId(1),
    });
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .selection,
        root
    );
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .panels[1]
            .scroll
            .info()
            .offset[1],
        0.
    );
    let label = PropertyAddress::Occurrence {
        instance: InstanceId(power.id),
        path: PropertyPath {
            references: vec![],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    let (field, y) = {
        let m = client.active.as_mut().unwrap();
        m.font = Some(Arc::new(crate::dashboard::font::test_font()));
        let d = m.designer.as_ref().unwrap();
        let field = d.drafts.draft(&label).unwrap().fields[0].id;
        (
            field,
            d.panels[1]
                .nodes
                .iter()
                .find(|n| n.id == field)
                .unwrap()
                .frame
                .y,
        )
    };
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert!(client.activate(client.epoch(), field).unwrap());
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert!(
        client
            .edit_text_input(
                client.epoch(),
                field,
                &Edit::Replace {
                    text: "Main power".into(),
                    cursor: 10,
                    anchor: 10,
                    selection_only: false
                }
            )
            .unwrap()
    );
    assert!(
        client
            .edit_text_input(client.epoch(), field, &Edit::Enter)
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_property_register(&label)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Main power".into()))]
    );
    let (delete, point) = studio_scene_control(&client, "Delete component");
    let generation = client.active.as_ref().unwrap().doc.oib_generation();
    assert!(
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 72,
                    phase: PointerPhase::Down,
                    position: point
                },
                2
            )
            .unwrap()
    );
    assert!(
        !client.activate(client.epoch(), delete).unwrap(),
        "capture prevents competing semantic delete"
    );
    assert!(
        !client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 73,
                    phase: PointerPhase::Up,
                    position: point
                },
                3
            )
            .unwrap()
    );
    assert!(
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 72,
                    phase: PointerPhase::Cancel,
                    position: point
                },
                4
            )
            .unwrap()
    );
    assert!(!client.input_busy());
    assert_eq!(
        client.active.as_ref().unwrap().doc.oib_generation(),
        generation
    );
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_instances()
            .unwrap()
            .iter()
            .any(|i| i.id == power.id && i.active)
    );
    assert!(client.activate(client.epoch(), delete).unwrap());
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(
        !client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_instances()
            .unwrap()
            .iter()
            .any(|i| i.id == power.id)
    );
    assert!(client.active.as_ref().unwrap().designer.as_ref().unwrap().drafts.drafts().iter().all(|d| !matches!(&d.address, PropertyAddress::Occurrence { instance, .. } if instance.0 == power.id)));
    // A new resident may reuse the renderer slot, but neither a former delete
    // action nor its typed occurrence target may acquire the new identity.
    let palette_open = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .palette_open;
    if !palette_open {
        activate_studio_tool(&mut client, "Insert component...");
    }
    activate_studio_tool(&mut client, "Insert Power");
    settle_editor(&mut client, &mut authority, &mut session);
    let replacement = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|i| i.component == 13)
        .unwrap();
    assert!(replacement.id > power.id);
    activate_studio_root(&mut client, replacement.id);
    assert!(!client.activate(client.epoch(), delete).unwrap());
    activate_studio_root(&mut client, 20);
    assert!(!client.activate(client.epoch(), delete).unwrap());
    let m = client.active.as_ref().unwrap();
    assert_eq!([1, 2, 3, 4].map(|f| m.read(f).unwrap()), fields);
    assert_eq!(
        m.doc.oib_property_register(&editor_address(3)).unwrap(),
        original
    );
    assert!(
        m.doc
            .oib_instances()
            .unwrap()
            .iter()
            .any(|i| i.id == replacement.id && i.active)
    );
    assert!(client.confirmed());
}

#[test]
fn desktop_studio_component_management_respects_test_readonly_and_fixed_ownership() {
    let mut client = mounted_six_panel_fixture(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let has_delete = |c: &Client| {
        c.scene().unwrap().controls().iter().any(|p| {
            p.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Delete component")
        })
    };
    for selection in [
        Selection::Document(DocumentId(1)),
        Selection::Definition(DefinitionId(13)),
        Selection::Object(ObjectAddress {
            instance: InstanceId(40),
            references: vec![],
            node: NodeId(1),
        }),
    ] {
        client.inspect_oib_selection(selection).unwrap();
        assert!(!has_delete(&client));
    }
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(2),
        }))
        .unwrap();
    assert!(
        has_delete(&client),
        "resident descendants have management actions"
    );
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    assert!(!has_delete(&client));
    client.set_oib_designer_mode(DesignerMode::Edit).unwrap();
    assert!(has_delete(&client));
    {
        let m = client.active.as_mut().unwrap();
        m.binding = Some(otrios_interfaces::canvas::MountBinding {
            version: 1,
            actor: [1; 32],
            collection: "designer-mount".into(),
            editable: true,
            ui_editable: false,
            comments: false,
            moderate: false,
        });
        m.refresh_designer().unwrap();
        m.invalidate();
    }
    assert!(!has_delete(&client));
}

#[test]
fn desktop_studio_info_disclosure_has_visible_open_closed_state() {
    let mut client = mounted_six_panel_fixture(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let before = client.active.as_ref().unwrap().doc.oib_root();
    let address = editor_address(3);
    for expanded in [false, true] {
        let (id, y) = {
            let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
            let row = d.panels[1]
                .nodes
                .iter()
                .find(|n| d.actions.get(&n.id) == Some(&LocalAction::Details(address.clone())))
                .unwrap();
            (row.id, row.frame.y)
        };
        fixture_scroll_to(&mut client, 1, y - 8.);
        let scene = client.scene().unwrap();
        let path = scene.path(id).unwrap();
        let button = path.last().unwrap().node;
        let semantics = button.semantics.as_ref().unwrap();
        assert_eq!(
            semantics.label,
            if expanded {
                "Hide property info"
            } else {
                "Show property info"
            }
        );
        assert_eq!(semantics.checked, expanded);
        assert!(
            matches!(&button.children[1].primitive, Primitive::Text { value, color, .. } if value == if expanded { "v Info" } else { "> Info" } && *color == if expanded { ACCENT } else { WHITE })
        );
        assert!(client.activate(client.epoch(), id).unwrap());
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .designer
                .as_ref()
                .unwrap()
                .details
                .as_ref()
                .is_some_and(|d| d.address == address),
            !expanded
        );
    }
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), before);
}

#[test]
fn desktop_studio_delete_release_after_remote_removal_keeps_replacement_safe() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let target = match &client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .selection
    {
        Selection::Object(a) => a.instance.0,
        _ => panic!("fixture selects a resident descendant"),
    };
    let (delete, point) = studio_scene_control(&client, "Delete component");
    let fields = [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap());
    assert!(
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 81,
                    phase: PointerPhase::Down,
                    position: point
                },
                1
            )
            .unwrap()
    );
    assert!(client.input_busy());

    let ui = fixture_ui();
    let mut rival = StateAdapter::for_ui(&ui, "designer-mount", &[], [2; 16]).unwrap();
    let (mut rival_session, _) = authority.open_session(&[2; 16], 0).unwrap();
    rival_session.set_oib_editable(true);
    rival_session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    rival
        .receive(&authority.generate(&mut rival_session).unwrap().unwrap())
        .unwrap();
    fixture_sync(&mut authority, &mut rival_session, &mut rival);
    assert!(rival.remove_oib(target).unwrap());
    fixture_sync(&mut authority, &mut rival_session, &mut rival);
    assert!(rival.insert_oib(50, None).unwrap());
    fixture_sync(&mut authority, &mut rival_session, &mut rival);
    let replacement = rival
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|i| i.component == 13)
        .unwrap();
    assert_ne!(replacement.id, target);
    let accepted_order = authority.oib_allocations().unwrap();
    let accepted_generation = authority.oib_status().unwrap().generation;
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(client.input_busy());
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_instances()
            .unwrap()
            .iter()
            .any(|i| i.id == replacement.id && i.active)
    );
    // The old scene/control stays captured while its target disappears and its
    // reusable renderer slot can already belong to a different stable identity.
    assert!(
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 81,
                    phase: PointerPhase::Up,
                    position: point
                },
                2
            )
            .is_ok()
    );
    assert!(!client.input_busy());
    assert!(!client.keyboard_visible());
    assert!(client.confirmed());
    assert_eq!(authority.oib_allocations().unwrap(), accepted_order);
    assert_eq!(
        authority.oib_status().unwrap().generation,
        accepted_generation
    );
    let m = client.active.as_ref().unwrap();
    let d = m.designer.as_ref().unwrap();
    assert!(!matches!(&d.selection, Selection::Object(a) if a.instance.0 == target));
    assert!(d.drafts.drafts().iter().all(|draft| !matches!(&draft.address, PropertyAddress::Occurrence { instance, .. } if instance.0 == target)));
    assert_eq!([1, 2, 3, 4].map(|f| m.read(f).unwrap()), fields);
    assert!(!client.activate(client.epoch(), delete).unwrap());
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_instances()
            .unwrap()
            .iter()
            .any(|i| i.id == replacement.id && i.active)
    );
}

#[test]
fn desktop_studio_delete_discards_own_dirty_draft_and_retains_unrelated_draft() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let fields = [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap());
    let audio_register = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&editor_address(2))
        .unwrap();
    let audio_field = focus_property(&mut client, 2);
    assert!(
        client
            .edit_text_input(
                client.epoch(),
                audio_field,
                &Edit::Replace {
                    text: "oops".into(),
                    cursor: 4,
                    anchor: 4,
                    selection_only: false
                }
            )
            .unwrap()
    );
    assert!(client.blur_text_input(client.epoch(), audio_field).unwrap());
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .drafts
            .draft(&editor_address(2))
            .unwrap()
            .dirty
    );
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(30),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let display = PropertyAddress::Occurrence {
        instance: InstanceId(30),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    let (field, y) = {
        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        let field = d.drafts.draft(&display).unwrap().fields[0].id;
        (
            field,
            d.panels[1]
                .nodes
                .iter()
                .find(|n| n.id == field)
                .unwrap()
                .frame
                .y,
        )
    };
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert!(client.activate(client.epoch(), field).unwrap());
    fixture_scroll_to(&mut client, 1, y - 8.);
    assert!(
        client
            .edit_text_input(
                client.epoch(),
                field,
                &Edit::Replace {
                    text: "Discard display".into(),
                    cursor: 15,
                    anchor: 15,
                    selection_only: false
                }
            )
            .unwrap()
    );
    let generation = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_generation()
        .unwrap();
    let (delete, _) = studio_scene_control(&client, "Delete component");
    assert!(client.activate(client.epoch(), delete).unwrap());
    assert!(!client.keyboard_visible());
    settle_editor(&mut client, &mut authority, &mut session);
    let m = client.active.as_ref().unwrap();
    assert_eq!(m.doc.oib_generation(), Some(generation + 1));
    assert!(!m.doc.oib_instances().unwrap().iter().any(|i| i.id == 30));
    assert!(m.designer.as_ref().unwrap().drafts.drafts().iter().all(|draft| !matches!(&draft.address, PropertyAddress::Occurrence { instance, .. } if instance.0 == 30)));
    let audio = m
        .designer
        .as_ref()
        .unwrap()
        .drafts
        .draft(&editor_address(2))
        .unwrap();
    assert!(audio.dirty);
    assert_eq!(audio.fields[0].buffer().text(), "oops");
    assert!(audio.error.is_some());
    assert_eq!(
        m.doc.oib_property_register(&editor_address(2)).unwrap(),
        audio_register
    );
    assert_eq!([1, 2, 3, 4].map(|f| m.read(f).unwrap()), fields);
    assert!(client.confirmed());
    assert!(!client.input_busy());
}

#[test]
fn desktop_studio_shared_definition_tools_preserve_instances_values_and_stale_targets() {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let fields = [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap());
    let allocations = authority.oib_allocations().unwrap();
    let values = authority.oib_values(2).unwrap();
    client
        .inspect_oib_selection(Selection::DefinitionNode {
            definition: DefinitionId(10),
            node: NodeId(8),
        })
        .unwrap();
    let generation = authority.oib_status().unwrap().generation;
    let stale = activate_studio_tool(&mut client, "Move shared earlier");
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(authority.oib_status().unwrap().generation, generation + 1);
    assert!(!client.activate(client.epoch(), stale).unwrap());
    assert_eq!(authority.oib_allocations().unwrap(), allocations);
    assert_eq!(authority.oib_values(2).unwrap(), values);
    assert_eq!(
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap()),
        fields
    );
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .selection,
        Selection::DefinitionNode {
            definition: DefinitionId(10),
            node: NodeId(8)
        }
    );
    let authored = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_authoring_seed()
        .unwrap();
    let definition = authored
        .designer
        .as_ref()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    assert_eq!(
        definition.nodes[0].visual_children,
        [2, 3, 8, 7, 9, 12].map(NodeId)
    );
    let tree = &client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .tree;
    let rank = |node| {
        tree.iter()
            .position(|row| {
                row.selection
                    == Some(Selection::DefinitionNode {
                        definition: DefinitionId(10),
                        node: NodeId(node),
                    })
            })
            .unwrap()
    };
    assert!(
        rank(8) < rank(7),
        "Components follows the authored visual order"
    );
    activate_studio_tool(&mut client, "Move shared later");
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(authority.oib_status().unwrap().generation, generation + 2);
    assert_eq!(authority.oib_values(2).unwrap(), values);
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .structure_tools
            .is_empty()
    );
}

#[test]
fn desktop_studio_design_fields_retain_stale_drafts_and_create_editable_components() {
    use otrios_interfaces::oib::authoring::{DefinitionBatch, DefinitionEdit};
    let (mut client, mut authority, mut session) = mounted_fixture_session_with_count(true, 0);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    let values = [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap());
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(10)))
        .unwrap();
    let name = PropertyAddress::Structural {
        target: StructuralTarget::Definition(DefinitionId(10)),
        property: StructuralProperty::Name,
    };
    let id = focus_address(&mut client, name.clone());
    let replace = |text: &str| Edit::Replace {
        text: text.into(),
        cursor: u32::try_from(text.len()).unwrap(),
        anchor: u32::try_from(text.len()).unwrap(),
        selection_only: false,
    };
    let epoch = client.epoch();
    client
        .edit_text_input(epoch, id, &replace("Local design"))
        .unwrap();
    let (mut remote, _) = authority.open_session(&[91; 16], 0).unwrap();
    remote.set_oib_editable(true);
    authority
        .receive(
            &mut remote,
            &crate::canvas::encode_state_request(&StateRequest::OibInstance {
                epoch: authority.oib_epoch().unwrap(),
                sequence: 1,
                operation: otrios_interfaces::oib::InstanceOperation::definitions(
                    DefinitionBatch {
                        generation: authority.oib_status().unwrap().structural_generation,
                        operations: vec![DefinitionEdit::RenameDefinition {
                            definition: DefinitionId(10),
                            name: "Remote design".into(),
                        }],
                    },
                ),
            })
            .unwrap(),
        )
        .unwrap();
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(!client.designer_commit_property(&name, false).unwrap());
    let designer = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(designer.drafts.draft(&name).unwrap().dirty);
    assert!(
        designer
            .editor_notice
            .as_ref()
            .unwrap()
            .contains("Document changed")
    );
    assert!(!client.active.as_ref().unwrap().doc.oib_saving());
    client.designer_discard_property(&name).unwrap();
    client.active.as_mut().unwrap().refresh().unwrap();

    let next = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_next_identity()
        .unwrap();
    activate_studio_tool(&mut client, "New component");
    settle_editor(&mut client, &mut authority, &mut session);
    let definition = DefinitionId(next);
    let node = NodeId(next + 1);
    client
        .inspect_oib_selection(Selection::Definition(definition))
        .unwrap();
    let width = PropertyAddress::Structural {
        target: StructuralTarget::Node { definition, node },
        property: StructuralProperty::Width,
    };
    let id = focus_address(&mut client, width.clone());
    client.edit_text_input(epoch, id, &replace("400")).unwrap();
    assert!(client.edit_text_input(epoch, id, &Edit::Enter).unwrap());
    settle_editor(&mut client, &mut authority, &mut session);
    let authored = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_authoring_seed()
        .unwrap();
    let graph = authored.graph.as_ref().unwrap();
    assert_eq!(
        graph
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .unwrap()
            .nodes[0]
            .visual
            .as_ref()
            .unwrap()
            .frame[2],
        otrios_interfaces::layout::Expr::Literal(Value::Number(400.))
    );
    assert!(
        graph
            .presets
            .iter()
            .filter(|p| p.definition == definition)
            .all(|p| p.frame.is_none())
    );
    assert_eq!(
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap()),
        values
    );
    assert!(client.scene().unwrap().controls().len() <= 128);
}

#[test]
fn desktop_studio_connects_an_unbound_preset_through_visible_source_choices() {
    let (mut client, mut authority, mut session) = mounted_fixture_session_with_count(true, 0);
    client.set_viewport([1280., 800.]).unwrap();
    client.set_oib_designer_studio(true).unwrap();
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(12)))
        .unwrap();
    let preset = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_next_identity()
        .unwrap();
    let values = [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap());
    activate_studio_tool(&mut client, "New preset");
    settle_editor(&mut client, &mut authority, &mut session);
    activate_studio_tool(&mut client, "Insert component...");
    activate_studio_tool(&mut client, "Insert New preset");
    settle_editor(&mut client, &mut authority, &mut session);
    let instance = authority
        .oib_allocations()
        .unwrap()
        .into_iter()
        .find(|a| a.prototype == preset)
        .unwrap()
        .id;
    client.inspect_oib(instance).unwrap();
    let address = PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: vec![],
            node: NodeId(1),
            property: PropertyId(4),
        },
    };
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .details
            .is_none()
    );
    assert!(activate_inspector_action(
        &mut client,
        &LocalAction::Details(address.clone())
    ));
    let operation = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .connection_tools
        .iter()
        .find(|(target, _)| target == &address)
        .unwrap()
        .1
        .iter()
        .find(|(label, _)| label == "Bind to volume")
        .unwrap()
        .1
        .clone();
    let properties = authority.oib_values(1).unwrap();
    assert!(activate_inspector_action(
        &mut client,
        &LocalAction::Structure(operation)
    ));
    settle_editor(&mut client, &mut authority, &mut session);
    assert!(authority.oib_status().unwrap().error.is_none());
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_property_register(&address)
            .is_err()
    );
    assert_eq!(authority.oib_values(1).unwrap(), properties);
    assert_eq!(
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().read(id).unwrap()),
        values
    );
    let info = editing::sources(
        &client.active.as_ref().unwrap().doc,
        &client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_authoring_seed()
            .unwrap(),
        &address,
    )
    .unwrap();
    assert!(info.binding.is_some());
}

#[path = "../../../tests/support/oib_library_fixture.rs"]
mod library_fixture;

fn captured_slider_fixture() -> (
    Client,
    crate::canvas::Document,
    Session,
    u16,
    u32,
    [f32; 2],
    f32,
) {
    let ui = library_fixture::admit(&library_fixture::imported());
    let (mut client, authority, session) = mounted_fixture_session_with_ui(true, 0, ui);
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    client.set_viewport([1280., 800.]).unwrap();
    client.prepare_presentation_scene().unwrap();
    let scene = client.scene().unwrap();
    let mount = client.active.as_ref().unwrap();
    let slider = scene
        .controls()
        .into_iter()
        .find(|node| {
            let Some(TouchResponder::Drag(Action::AppEvent(control))) = node.node.responder else {
                return false;
            };
            mount
                .ui
                .handlers
                .iter()
                .any(|h| h.control == control && h.kind == GestureKind::Slider)
        })
        .unwrap();
    let Some(TouchResponder::Drag(Action::AppEvent(control))) = slider.node.responder else {
        unreachable!()
    };
    let field = mount.handler(control).unwrap().fields[0];
    (
        client,
        authority,
        session,
        field,
        slider.node.id,
        [
            slider.origin[0],
            slider.origin[1] + slider.node.frame.height * 0.5 * slider.scale,
        ],
        slider.node.frame.width * slider.scale,
    )
}
fn capture_event(client: &mut Client, phase: PointerPhase, position: [f32; 2], at: u64) {
    client
        .pointer_at(
            client.epoch(),
            PointerEvent {
                id: 812,
                phase,
                position,
            },
            at,
        )
        .unwrap();
}
fn capture_template(
    root: &mut otrios_interfaces::layout::TemplateNode,
    id: u16,
) -> Option<&mut otrios_interfaces::layout::TemplateNode> {
    if root.id == id {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| capture_template(child, id))
}
#[test]
fn imported_slider_coalesces_one_hundred_moves_until_presentation_credit() {
    let (mut client, mut authority, mut session, field, _node, origin, width) =
        captured_slider_fixture();
    capture_event(
        &mut client,
        PointerPhase::Down,
        [origin[0] + width * 0.1, origin[1]],
        1000,
    );
    let raw = {
        let mount = client.active.as_ref().unwrap();
        assert!(mount.input.dragging(812));
        assert!(
            mount.capture_validation.is_some(),
            "real imported slider must qualify for captured input reuse"
        );
        mount
            .raw_layout
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .scene
            .clone()
    };
    assert!(client.set_viewport([1000., 700.]).unwrap());
    assert_eq!(
        client.active.as_ref().unwrap().viewport,
        [1280., 800.],
        "viewport activation waits for held input"
    );
    for index in 1..=100 {
        let fraction = 0.1 + 0.8 * index as f32 / 100.;
        capture_event(
            &mut client,
            PointerPhase::Move,
            [origin[0] + width * fraction, origin[1]],
            1000 + index * 8,
        );
        client.tick(1000 + index * 8, 0).unwrap();
        let mount = client.active.as_ref().unwrap();
        assert!(mount.capture_validation.is_some());
        assert!(
            Arc::ptr_eq(
                &raw,
                &mount.raw_layout.lock().unwrap().as_ref().unwrap().scene
            ),
            "Move and Tick cannot resolve a visual layout while renderer credit is held"
        );
        assert!(mount.layout_scene.get().is_none());
    }
    assert_eq!(client.active.as_ref().unwrap().number(field).unwrap(), 900);
    client.prepare_presentation_scene().unwrap();
    let rendered = client.scene().unwrap();
    let latest = client
        .active
        .as_ref()
        .unwrap()
        .raw_layout
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .scene
        .clone();
    assert!(!Arc::ptr_eq(&raw, &latest));
    assert!(Arc::ptr_eq(&rendered, &client.scene().unwrap()));
    assert!(Arc::ptr_eq(
        &latest,
        &client
            .active
            .as_ref()
            .unwrap()
            .raw_layout
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .scene
    ));
    let full = resolve_layout(client.active.as_ref().unwrap()).unwrap();
    assert_eq!(
        cached_layout(client.active.as_ref().unwrap())
            .unwrap()
            .root(),
        full.root(),
        "content update matches a full current-state resolution"
    );
    capture_event(
        &mut client,
        PointerPhase::Up,
        [origin[0] + width * 0.9, origin[1]],
        1810,
    );
    assert!(!client.active.as_ref().unwrap().input.active());
    assert!(client.active.as_ref().unwrap().capture_validation.is_none());
    settle_editor(&mut client, &mut authority, &mut session);
    assert_eq!(client.active.as_ref().unwrap().viewport, [1000., 700.]);
}
#[test]
fn captured_slider_visibility_changes_force_current_owner_validation() {
    use otrios_interfaces::layout::{Binding, Expr};
    let (mut client, _authority, _session, field, node, origin, width) = captured_slider_fixture();
    {
        let mount = client.active.as_mut().unwrap();
        mount.ui.selectors.push(Selector {
            local: 60000,
            field,
            projection: Projection::Nonzero,
        });
        let mut blueprint = mount.layout.blueprint().clone();
        let target = blueprint
            .templates
            .iter_mut()
            .find_map(|t| capture_template(&mut t.root, node as u16))
            .unwrap();
        target.visible = Expr::State(Binding::Local(60000));
        mount.layout = Layout::from_blueprint(blueprint).unwrap();
        mount.invalidate();
    }
    capture_event(
        &mut client,
        PointerPhase::Down,
        [origin[0] + width * 0.2, origin[1]],
        1000,
    );
    capture_event(
        &mut client,
        PointerPhase::Move,
        [origin[0], origin[1]],
        1010,
    );
    assert_eq!(client.active.as_ref().unwrap().number(field).unwrap(), 0);
    assert!(
        client.active.as_ref().unwrap().capture_validation.is_none(),
        "changed visibility cannot retain an old validation tree"
    );
    capture_event(
        &mut client,
        PointerPhase::Move,
        [origin[0] + width * 0.8, origin[1]],
        1020,
    );
    assert!(!client.active.as_ref().unwrap().input.active());
    assert_eq!(
        client.active.as_ref().unwrap().number(field).unwrap(),
        0,
        "removed owner cannot receive another captured value"
    );
}
#[test]
fn captured_slider_handler_rebind_cannot_retarget_another_field_after_invalidation() {
    let (mut client, _authority, _session, field, _node, origin, width) = captured_slider_fixture();
    capture_event(
        &mut client,
        PointerPhase::Down,
        [origin[0] + width * 0.2, origin[1]],
        1000,
    );
    let other = if field == 1 { 3 } else { 1 };
    let before = [field, other].map(|field| client.active.as_ref().unwrap().number(field).unwrap());
    {
        let mount = client.active.as_mut().unwrap();
        let control = mount.captured_handlers[&812].control;
        mount
            .ui
            .handlers
            .iter_mut()
            .find(|handler| handler.control == control)
            .unwrap()
            .fields = vec![other];
        mount.invalidate();
        assert!(mount.capture_validation.is_none());
    }
    capture_event(
        &mut client,
        PointerPhase::Move,
        [origin[0] + width * 0.8, origin[1]],
        1010,
    );
    assert!(!client.active.as_ref().unwrap().input.active());
    assert!(
        !client
            .active
            .as_ref()
            .unwrap()
            .captured_handlers
            .contains_key(&812)
    );
    assert_eq!(
        [field, other].map(|field| client.active.as_ref().unwrap().number(field).unwrap()),
        before
    );
}

#[test]
fn captured_slider_rebound_responder_revokes_the_validation_tree() {
    let (mut client, _authority, _session, field, node, origin, width) = captured_slider_fixture();
    capture_event(
        &mut client,
        PointerPhase::Down,
        [origin[0] + width * 0.2, origin[1]],
        1000,
    );
    let before = client.active.as_ref().unwrap().number(field).unwrap();
    {
        let mount = client.active.as_mut().unwrap();
        let mut blueprint = mount.layout.blueprint().clone();
        let target = blueprint
            .templates
            .iter_mut()
            .find_map(|t| capture_template(&mut t.root, node as u16))
            .unwrap();
        target.responder = Some(otrios_interfaces::layout::Responder::Consume);
        target.semantics = None;
        mount.layout = Layout::from_blueprint(blueprint).unwrap();
        mount.invalidate();
        assert!(mount.capture_validation.is_none());
    }
    capture_event(
        &mut client,
        PointerPhase::Move,
        [origin[0] + width * 0.8, origin[1]],
        1020,
    );
    assert!(!client.active.as_ref().unwrap().input.active());
    assert_eq!(
        client.active.as_ref().unwrap().number(field).unwrap(),
        before
    );
}

#[test]
fn compatible_library_activation_waits_for_held_slider_and_final_receipt_without_ui_history() {
    fn pump(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
        for _ in 0..30 {
            let now = client
                .active
                .as_ref()
                .map_or(1000, |mount| mount.now.saturating_add(33));
            client.tick(now, 0).unwrap();
            let mut progress = false;
            while let Some(bytes) = client.outgoing().unwrap() {
                if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes).unwrap() {
                    authority.receive(session, &bytes).unwrap();
                    progress = true;
                }
            }
            if let Some(bytes) = authority.generate(session).unwrap() {
                client
                    .receive(
                        &encode_server(&ServerMessage::Sync {
                            epoch: client.epoch(),
                            bytes,
                        })
                        .unwrap(),
                    )
                    .unwrap();
                progress = true;
            }
            if !progress {
                assert!(client.confirmed());
                return;
            }
        }
        panic!("library client did not settle");
    }
    let ui = library_fixture::admit(&library_fixture::imported());
    let state = "library:held-slider";
    let mut authority = crate::canvas::Document::new(ui.state_schema.clone(), state).unwrap();
    authority.configure_oib(&ui).unwrap();
    let encoded = crate::canvas::encode_ui(&ui).unwrap();
    let mut manifest = designer_fixture::manifest();
    manifest.state_ref = state.into();
    manifest
        .dependencies
        .push(otrios_interfaces::oib::library::FEATURE.into());
    let mut client = Client::default();
    client
        .admit_catalog(
            &crate::canvas::encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app: manifest,
                    path: "oib-designer".into(),
                    private: false,
                    ui_hash: hash(&encoded),
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client.open(&ui.ui_ref, state);
    let ClientMessage::Mount { epoch, writer, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount request")
    };
    let (mut session, _) = authority.open_session(&writer, 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&encoded),
                ui: encoded,
                code: vec![],
                state: authority.generate(&mut session).unwrap().unwrap(),
                binding: None,
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::AcceptMount{epoch:accepted} if accepted==epoch)
    );
    pump(&mut client, &mut authority, &mut session);
    assert!(
        client.ui_cache.is_empty(),
        "imported bodies never enter optional UI history"
    );
    let label = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    client
        .active
        .as_mut()
        .unwrap()
        .doc
        .edit_oib_property(&label, Some(Value::Text("Pinned volume".into())))
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    client.set_viewport([1280., 800.]).unwrap();
    let override_version = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&label)
        .unwrap()
        .version;
    let address = ObjectAddress {
        instance: InstanceId(20),
        references: vec![NodeId(7)],
        node: NodeId(1),
    };
    let slider = super::super::oib::seed(&client.active.as_ref().unwrap().ui)
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .mappings
        .objects
        .iter()
        .find(|mapping| mapping.address == address)
        .unwrap()
        .layout_node
        .unwrap();
    let scene = client.scene().unwrap();
    let located = scene.path(u32::from(slider)).unwrap();
    let located = located.last().unwrap();
    assert_eq!(located.node.frame.width, 440.);
    assert!(matches!(
        located.node.responder,
        Some(TouchResponder::Drag(Action::AppEvent(_)))
    ));
    let point = |fraction: f32| {
        [
            located.origin[0] + located.node.frame.width * fraction * located.scale,
            located.origin[1] + located.node.frame.height * 0.5 * located.scale,
        ]
    };
    let down = point(0.25);
    let release = point(0.75);
    assert!(
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 247,
                    phase: PointerPhase::Down,
                    position: down
                },
                10000
            )
            .unwrap()
    );
    assert!(client.input_busy());
    pump(&mut client, &mut authority, &mut session);
    assert!(client.confirmed());
    let old = client.active.as_ref().unwrap().ui.clone();
    let old_layout = crate::dashboard::layout::Layout::encode(
        client.active.as_ref().unwrap().layout.blueprint(),
    )
    .unwrap();
    let old_handlers = old.handlers.clone();
    let old_generation = client.active.as_ref().unwrap().oib_generation;
    let collection_epoch = authority.oib_epoch().unwrap();
    let untouched = [2, 3, 4].map(|field| authority.typed_value(field).unwrap());
    let changed = library_fixture::update(&ui, |library| {
        library
            .definitions
            .iter_mut()
            .find(|definition| definition.id == DefinitionId(12))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|value| value.property == PropertyId(2))
            .unwrap()
            .value = Value::Number(400.);
    });
    let (generation, baseline) = authority.oib_baseline().unwrap();
    assert!(
        authority
            .adopt_oib_baseline(&changed, generation, baseline)
            .unwrap()
    );
    pump(&mut client, &mut authority, &mut session);
    let held = client.active.as_ref().unwrap();
    assert!(held.input.active());
    assert_eq!(
        held.doc.oib_generation(),
        Some(authority.oib_status().unwrap().generation)
    );
    assert_eq!(held.oib_generation, old_generation);
    assert_eq!(
        held.doc.admitted_oib_hash(),
        Some(hash(&crate::canvas::encode_ui(&changed).unwrap())),
        "the new admitted baseline arrives while presentation remains pinned"
    );
    assert_eq!(held.ui, old);
    assert_eq!(held.ui.handlers, old_handlers);
    assert_eq!(
        crate::dashboard::layout::Layout::encode(held.layout.blueprint()).unwrap(),
        old_layout
    );
    assert_eq!(
        library_fixture::seed(&held.ui).resolved_libraries[0].revision,
        [1; 32]
    );
    assert!(
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 247,
                    phase: PointerPhase::Up,
                    position: release
                },
                10100
            )
            .unwrap()
    );
    assert!(!client.input_busy());
    assert!(!client.confirmed());
    assert_eq!(
        client.value(1).unwrap(),
        750,
        "release uses the captured 440px extent rather than the new 400px library extent"
    );
    assert_eq!(
        client.active.as_ref().unwrap().ui,
        old,
        "unconfirmed final value also pins the old complete closure"
    );
    pump(&mut client, &mut authority, &mut session);
    assert!(client.confirmed());
    let current = client.active.as_ref().unwrap();
    assert_eq!(
        current.oib_generation,
        Some(authority.oib_status().unwrap().generation)
    );
    assert_eq!(
        library_fixture::seed(&current.ui).resolved_libraries[0].revision,
        [2; 32]
    );
    assert_ne!(current.ui.layout, old.layout);
    assert_eq!(current.ui.handlers, old_handlers);
    assert_eq!(authority.oib_epoch().unwrap(), collection_epoch);
    assert_eq!(authority.typed_value(1).unwrap(), Value::Number(750.));
    assert_eq!(
        [2, 3, 4].map(|field| authority.typed_value(field).unwrap()),
        untouched
    );
    assert_eq!(
        current
            .doc
            .oib_property_register(&label)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Pinned volume".into()))]
    );
    assert_eq!(
        current.doc.oib_property_register(&label).unwrap().version,
        override_version,
        "baseline activation preserves the exact causal override head"
    );
    assert!(client.ui_cache.is_empty());
    let scene = client.scene().unwrap();
    assert_eq!(
        scene
            .path(u32::from(slider))
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        400.
    );
}
