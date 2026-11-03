// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::{
    layout::ValueType,
    oib::{Cell, Seed, designer::*},
};
#[path = "../../../../../apps/oib-designer/app.rs"]
mod designer_fixture;
fn seed() -> Seed {
    Seed {
        format: otrios_interfaces::oib::FORMAT.into(),
        main_oib: 1,
        main_class: "otrios.declarative-v1".into(),
        designer: Some(otrios_interfaces::oib::designer::Document {
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
    d.panels[0].scroll.set_offset([0., 200.]);
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
    assert!(
        designer_fixture::manifest()
            .dependencies
            .contains(&FEATURE.into())
    );
    let ui = designer_fixture::ui(&[]);
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
    for _ in 0..4 {
        assert!(mount.doc.instantiate_oib(10, None).unwrap());
        fixture_sync(&mut authority, &mut session, &mut mount.doc);
        mount
            .refresh()
            .expect("real fixture plus local chrome admits each new root");
    }
    assert_eq!(mount.doc.oib_allocations().unwrap().len(), 6);
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
    assert_eq!(m.designer.as_ref().unwrap().objects.len(), 121);
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
fn focus_property(client: &mut Client, property: u64) -> u32 {
    let address = editor_address(property);
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
    let m = client.active.as_ref().unwrap();
    let seed = crate::canvas::oib::seed(&m.ui).unwrap();
    assert_eq!(
        crate::canvas::oib::effective(seed, &m.doc.oib_active().unwrap(), 203).unwrap(),
        Value::Text("Sound".into())
    );
    assert!(
        client
            .designer_action(LocalAction::Write(editor_address(3), None))
            .unwrap()
    );
    settle_editor(&mut client, &mut authority, &mut session);
    let m = client.active.as_ref().unwrap();
    let seed = crate::canvas::oib::seed(&m.ui).unwrap();
    assert_eq!(
        crate::canvas::oib::effective(seed, &m.doc.oib_active().unwrap(), 203).unwrap(),
        Value::Text("Volume".into())
    );
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
    assert_eq!(
        client.active.as_ref().unwrap().doc.oib_active().unwrap()[&202],
        Some(Value::Number(377.5))
    );
    assert_eq!(
        [1, 2, 3, 4].map(|f| client.active.as_ref().unwrap().read(f).unwrap()),
        fields
    );
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
        client.active.as_ref().unwrap().doc.oib_active().unwrap()[&201],
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
    let ui = designer_fixture::ui(&[]);
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
        client.active.as_ref().unwrap().doc.oib_active().unwrap()[&203],
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
        client.active.as_ref().unwrap().doc.oib_active().unwrap()[&201],
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
        client.active.as_ref().unwrap().doc.oib_active().unwrap()[&203],
        Some(Value::Text("Sound".into()))
    );
}
