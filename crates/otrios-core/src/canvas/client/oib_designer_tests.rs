// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::{
    layout::ValueType,
    oib::{Cell, Seed, designer::*},
};
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
    let mut b = Builder::new();
    let rows: Vec<_> = (0..200).map(|n| object(n, 0, None)).collect();
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
