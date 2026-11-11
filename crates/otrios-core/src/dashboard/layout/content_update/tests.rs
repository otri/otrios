// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
fn n(value: f32) -> Expr {
    Expr::Literal(Value::Number(value))
}
fn state(id: u16) -> Expr {
    Expr::State(Binding::Local(id))
}
fn node(id: u16, frame: [Expr; 4], shape: Shape, children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode {
        reference_root: false,
        layout: None,
        id,
        frame,
        shape,
        children,
        resize: None,
        visible: Expr::Literal(Value::Boolean(true)),
        responder: None,
        semantics: None,
    }
}
fn blueprint() -> Blueprint {
    let thumb = node(
        2,
        [
            Expr::Add(
                Box::new(n(10.)),
                Box::new(Expr::Multiply(Box::new(state(1)), Box::new(n(2.)))),
            ),
            n(10.),
            n(16.),
            n(16.),
        ],
        Shape::Superellipse {
            radius: n(4.),
            exponent: 4.,
            color: state(3),
        },
        vec![],
    );
    let label = node(
        3,
        [n(10.), n(34.), n(200.), n(20.)],
        Shape::Text {
            value: state(2),
            scale: state(4),
            color: state(3),
        },
        vec![],
    );
    let fill = node(
        4,
        [n(10.), n(22.), state(5), n(6.)],
        Shape::Superellipse {
            radius: n(1.),
            exponent: 4.,
            color: state(3),
        },
        vec![],
    );
    let mut slider = node(
        1,
        [n(20.), n(30.), n(300.), n(60.)],
        Shape::Canvas { clip: true },
        vec![thumb, label, fill],
    );
    slider.responder = Some(Responder::AppEvent {
        control: 1,
        drag: true,
    });
    slider.semantics = Some(Semantics {
        label: state(2),
        checked: Expr::Literal(Value::Boolean(false)),
    });
    Blueprint {
        schema: "otrios-layout-v2".into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "Fixed slider".into(),
            parameters: vec![],
            root: node(
                0,
                [n(0.), n(0.), Expr::Width, Expr::Height],
                Shape::Canvas { clip: true },
                vec![slider],
            ),
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    }
}
fn values() -> BTreeMap<u16, Value> {
    BTreeMap::from([
        (1, Value::Number(10.)),
        (2, Value::Text("10%".into())),
        (3, Value::Color([10, 40, 220, 255])),
        (4, Value::Number(1.)),
        (5, Value::Number(50.)),
        (6, Value::Boolean(true)),
    ])
}
fn parity(layout: &Layout, old: &BTreeMap<u16, Value>, new: &BTreeMap<u16, Value>) -> Scene {
    let previous = layout.resolve_slots(old).unwrap();
    let patched = layout
        .update_content(&previous, old, new, [1280., 800.])
        .unwrap()
        .expect("qualified content change");
    assert_eq!(patched.root(), layout.resolve_slots(new).unwrap().root());
    patched
}
#[test]
fn slider_thumb_text_paint_and_semantics_update_inside_fixed_bounds() {
    let layout = Layout::from_blueprint(blueprint()).unwrap();
    let old = values();
    let mut next = old.clone();
    next.insert(1, Value::Number(80.));
    next.insert(2, Value::Text("80%".into()));
    next.insert(3, Value::Color([240, 90, 10, 255]));
    next.insert(4, Value::Number(2.));
    assert!(layout.same_source(&layout.clone()));
    assert!(!layout.same_source(&Layout::from_bytes(layout.bytes()).unwrap()));
    assert!(layout.content_input_bounds_stable(&old, &next));
    let before = layout.resolve_slots(&old).unwrap();
    let after = parity(&layout, &old, &next);
    let owner = &after.root().children[0];
    let old_owner = &before.root().children[0];
    assert_eq!(owner.frame, old_owner.frame);
    assert_eq!(owner.id, old_owner.id);
    assert_eq!(owner.responder, old_owner.responder);
    assert_ne!(owner.children[0].frame.x, old_owner.children[0].frame.x);
    assert_eq!(
        owner.children[0].frame.width,
        old_owner.children[0].frame.width
    );
    assert_eq!(owner.semantics.as_ref().unwrap().label, "80%");
    assert_eq!(
        before.root().children[0].semantics.as_ref().unwrap().label,
        "10%",
        "held scene is immutable"
    );
}
#[test]
fn decorative_fill_size_requires_layout_at_presentation_but_not_capture_revalidation() {
    let layout = Layout::from_blueprint(blueprint()).unwrap();
    let old = values();
    let mut next = old.clone();
    next.insert(5, Value::Number(90.));
    let before = layout.resolve_slots(&old).unwrap();
    assert!(layout.content_input_bounds_stable(&old, &next));
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .unwrap()
            .is_none()
    );
    let mut source = blueprint();
    source.templates[0].root.children[0].frame[2] = state(5);
    let layout = Layout::from_blueprint(source).unwrap();
    assert!(!layout.content_input_bounds_stable(&old, &next));
}
#[test]
fn visibility_and_interactive_placement_use_conservative_capture_rules() {
    let old = values();
    let mut next = old.clone();
    next.insert(6, Value::Boolean(false));
    let mut source = blueprint();
    source.templates[0].root.children[0].children[0].visible = state(6);
    let layout = Layout::from_blueprint(source).unwrap();
    let before = layout.resolve_slots(&old).unwrap();
    assert!(!layout.content_input_bounds_stable(&old, &next));
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .unwrap()
            .is_none()
    );
    let mut source = blueprint();
    source.templates[0].root.children[0].frame[0] = state(1);
    let layout = Layout::from_blueprint(source).unwrap();
    let mut next = old.clone();
    next.insert(1, Value::Number(50.));
    assert!(!layout.content_input_bounds_stable(&old, &next));
    parity(&layout, &old, &next);
}
#[test]
fn unchanged_invisible_branches_are_skipped_without_evaluating_their_sizes() {
    let mut source = blueprint();
    let hidden = &mut source.templates[0].root.children[0].children[2];
    hidden.visible = Expr::Literal(Value::Boolean(false));
    hidden.frame[2] = state(8);
    let layout = Layout::from_blueprint(source).unwrap();
    let old = values();
    let mut next = old.clone();
    next.insert(8, Value::Number(70.));
    next.insert(2, Value::Text("updated".into()));
    let scene = parity(&layout, &old, &next);
    assert_eq!(scene.root().children[0].children.len(), 2);
}
#[test]
fn resize_uses_existing_rules_and_falls_back_when_position_affects_size() {
    let old = values();
    let mut next = old.clone();
    next.insert(1, Value::Number(70.));
    let mut source = blueprint();
    source.templates[0].root.children[0].children[0].resize = Some(ResizeRule {
        reference_size: [200., 60.],
        horizontal: AxisResize::Springs {
            leading: true,
            length: true,
            trailing: false,
        },
        vertical: AxisResize::Fixed,
    });
    let layout = Layout::from_blueprint(source).unwrap();
    let before = layout.resolve_slots(&old).unwrap();
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .unwrap()
            .is_none()
    );
    let mut source = blueprint();
    source.templates[0].root.children[0].children[0].resize = Some(ResizeRule {
        reference_size: [300., 60.],
        horizontal: AxisResize::Pins {
            leading: Some(12.),
            trailing: None,
        },
        vertical: AxisResize::Fixed,
    });
    let layout = Layout::from_blueprint(source).unwrap();
    parity(&layout, &old, &next);
}
#[test]
fn parameter_and_slot_candidates_fall_back_while_static_composition_reuses_frames() {
    let old = values();
    let mut next = old.clone();
    next.insert(2, Value::Text("updated".into()));
    let mut source = blueprint();
    source.templates[0].parameters.push(Parameter {
        key: 1,
        name: "unused".into(),
        value_type: ValueType::Number,
        default: n(1.),
    });
    let layout = Layout::from_blueprint(source).unwrap();
    let before = layout.resolve_slots(&old).unwrap();
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .unwrap()
            .is_none()
    );
    assert!(!layout.content_input_bounds_stable(&old, &next));
    let mut source = blueprint();
    source.templates[0].root.children.push(node(
        900,
        [n(0.), n(0.), n(10.), n(10.)],
        Shape::Slot,
        vec![],
    ));
    let layout = Layout::from_blueprint(source).unwrap();
    let before = layout.resolve_slots(&old).unwrap();
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .unwrap()
            .is_none()
    );
    let mut source = blueprint();
    source.schema = "otrios-layout-v3".into();
    let plan = FlowPlan {
        insets: [8.; 4],
        content: Flow {
            flexible: [true, true],
            kind: FlowKind::Item {
                node: 1,
                preferred: [300., 60.],
                minimum: [100., 60.],
            },
        },
    };
    source.composition = Some(Composition {
        wide_min_width: 900.,
        wide: plan.clone(),
        tall: plan,
    });
    let layout = Layout::from_blueprint(source).unwrap();
    let before = layout.resolve_slots(&old).unwrap();
    parity(&layout, &old, &next);
    assert!(
        layout
            .update_content(&before, &old, &next, [800., 1280.])
            .unwrap()
            .is_none(),
        "changed viewport requires fresh composition"
    );
}
#[test]
fn invalid_content_reuses_full_evaluator_and_scene_validation_errors() {
    let layout = Layout::from_blueprint(blueprint()).unwrap();
    let old = values();
    let before = layout.resolve_slots(&old).unwrap();
    for invalid in [
        Value::Number(f32::NAN),
        Value::Number(1.5),
        Value::Text("wrong scale".into()),
    ] {
        let mut next = old.clone();
        next.insert(4, invalid);
        assert!(
            layout
                .update_content(&before, &old, &next, [1280., 800.])
                .is_err()
        );
        assert!(layout.resolve_slots(&next).is_err());
    }
    let mut next = old.clone();
    next.insert(2, Value::Text("x".repeat(258)));
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .is_err()
    );
    let mut next = old.clone();
    next.remove(&2);
    assert!(
        layout
            .update_content(&before, &old, &next, [1280., 800.])
            .is_err()
    );
}

#[cfg(feature = "canvas")]
#[path = "../../../../tests/support/oib_library_fixture.rs"]
#[allow(dead_code)]
mod imported_fixture;
#[test]
#[cfg(feature = "canvas")]
fn imported_oib_slider_content_matches_full_resolve_without_new_composition() {
    use otrios_interfaces::canvas::Projection;
    let ui = imported_fixture::admit(&imported_fixture::imported());
    let mut document =
        crate::canvas::Document::new(ui.state_schema.clone(), "oib:content-update").unwrap();
    document.configure_oib(&ui).unwrap();
    let (ui, layout) = document.oib_compiled(2).unwrap();
    assert!(template(layout.blueprint()).is_some());
    assert!(layout.blueprint().composition.is_some());
    let slots = |value: f32| {
        ui.selectors
            .iter()
            .map(|selector| {
                let default = if selector.field == 1 {
                    Value::Number(value)
                } else {
                    ui.state_schema
                        .fields
                        .iter()
                        .find(|field| field.id == selector.field)
                        .unwrap()
                        .default
                        .clone()
                };
                let value = match (&selector.projection, default) {
                    (Projection::Identity, value) => value,
                    (Projection::Percent, Value::Number(value)) => {
                        Value::Text(format!("{}%", (value / 10.).round()))
                    }
                    (Projection::Number { scale, offset }, Value::Number(value)) => {
                        Value::Number(value * scale + offset)
                    }
                    (Projection::Nonzero, Value::Number(value)) => Value::Boolean(value != 0.),
                    _ => panic!("fixture selector changed"),
                };
                (selector.local, value)
            })
            .collect::<BTreeMap<_, _>>()
    };
    let old = slots(400.);
    let next = slots(870.);
    assert!(layout.content_input_bounds_stable(&old, &next));
    let previous = layout.resolve_slots(&old).unwrap();
    let updated = layout
        .update_content(&previous, &old, &next, [1280., 800.])
        .unwrap()
        .expect("actual imported Slider qualifies");
    assert_eq!(updated.root(), layout.resolve_slots(&next).unwrap().root());
    assert_ne!(updated.root(), previous.root());
}

#[test]
fn series_paths_and_radius_reuse_full_geometry_validation() {
    let mut old = values();
    old.insert(7, Value::Series(vec![10., 90.]));
    let mut source = blueprint();
    source.templates[0].root.children[0].children[0].shape = Shape::SeriesPath {
        series: state(7),
        width: 1.,
        color: state(3),
    };
    let layout = Layout::from_blueprint(source).unwrap();
    let mut next = old.clone();
    next.insert(7, Value::Series(vec![0., 50., 100.]));
    parity(&layout, &old, &next);
    let mut invalid = next.clone();
    invalid.insert(7, Value::Series(vec![f32::NAN]));
    let before = layout.resolve_slots(&old).unwrap();
    assert!(
        layout
            .update_content(&before, &old, &invalid, [1280., 800.])
            .is_err()
    );
    let mut source = blueprint();
    source.templates[0].root.children[0].children[0].shape = Shape::TimedSeriesPath {
        series: state(7),
        width: 1.,
        color: state(3),
    };
    old.insert(7, Value::TimedSeries(vec![[0.5, 70.]]));
    let mut next = old.clone();
    next.insert(7, Value::TimedSeries(vec![[0.1, 20.], [0.9, 80.]]));
    let layout = Layout::from_blueprint(source).unwrap();
    parity(&layout, &old, &next);
    let mut invalid = next.clone();
    invalid.insert(7, Value::TimedSeries(vec![[0.9, 10.], [0.1, 20.]]));
    let before = layout.resolve_slots(&old).unwrap();
    assert!(
        layout
            .update_content(&before, &old, &invalid, [1280., 800.])
            .is_err()
    );
    let mut source = blueprint();
    let Shape::Superellipse { radius, .. } =
        &mut source.templates[0].root.children[0].children[0].shape
    else {
        unreachable!()
    };
    *radius = state(7);
    old.insert(7, Value::Number(4.));
    let mut next = old.clone();
    next.insert(7, Value::Number(8.));
    let layout = Layout::from_blueprint(source).unwrap();
    parity(&layout, &old, &next);
    let mut invalid = next.clone();
    invalid.insert(7, Value::Number(9.));
    let before = layout.resolve_slots(&old).unwrap();
    assert!(
        layout
            .update_content(&before, &old, &invalid, [1280., 800.])
            .is_err()
    );
}

#[cfg(feature = "canvas")]
#[test]
fn raw_binding_cache_distinguishes_signed_zero_in_all_numeric_values() {
    for value in [
        Value::Number(-0.),
        Value::Series(vec![-0.]),
        Value::TimedSeries(vec![[1., -0.]]),
    ] {
        let positive = match &value {
            Value::Number(_) => Value::Number(0.),
            Value::Series(_) => Value::Series(vec![0.]),
            Value::TimedSeries(_) => Value::TimedSeries(vec![[1., 0.]]),
            _ => unreachable!(),
        };
        let a = BTreeMap::from([(1, positive)]);
        let b = BTreeMap::from([(1, value)]);
        assert!(!super::super::same_binding_values(&a, &b));
        assert!(super::super::same_binding_values(&b, &b));
    }
}

#[test]
fn auto_layout_keeps_color_and_legacy_slider_placement_on_content_path() {
    let mut source = blueprint();
    source.schema = AUTO_LAYOUT_FORMAT.into();
    source.templates[0].root.layout = Some(NodeLayout {
        container: Some(AutoLayout {
            padding: [16.; 4],
            ..AutoLayout::default()
        }),
        ..NodeLayout::default()
    });
    let layout = Layout::from_blueprint(source).unwrap();
    let old = values();
    let mut next = old.clone();
    next.insert(3, Value::Color([234, 21, 33, 255]));
    assert!(layout.content_input_bounds_stable(&old, &next));
    parity(&layout, &old, &next);
    next.insert(1, Value::Number(80.));
    assert!(layout.content_input_bounds_stable(&old, &next));
    parity(&layout, &old, &next);
}

#[test]
fn auto_layout_hug_text_change_requires_remeasure_but_color_does_not() {
    let mut source = blueprint();
    source.schema = AUTO_LAYOUT_FORMAT.into();
    source.templates[0].root.children[0].children[1].layout = Some(NodeLayout {
        width: AxisSizing {
            mode: Sizing::Hug,
            ..AxisSizing::default()
        },
        height: AxisSizing {
            mode: Sizing::Hug,
            ..AxisSizing::default()
        },
        ..NodeLayout::default()
    });
    let layout = Layout::from_blueprint(source).unwrap();
    let font = crate::dashboard::font::test_font();
    let old = values();
    let previous = layout
        .resolve_ordered_with_font(&old, [1280., 800.], &[], Some(&font))
        .unwrap();
    let mut next = old.clone();
    next.insert(3, Value::Color([234, 21, 33, 255]));
    let patched = layout
        .update_content(&previous, &old, &next, [1280., 800.])
        .unwrap()
        .unwrap();
    let full = layout
        .resolve_ordered_with_font(&next, [1280., 800.], &[], Some(&font))
        .unwrap();
    assert_eq!(patched.root(), full.root());
    next.insert(2, Value::Text("A much longer label".into()));
    assert!(!layout.content_input_bounds_stable(&old, &next));
    assert!(
        layout
            .update_content(&previous, &old, &next, [1280., 800.])
            .unwrap()
            .is_none()
    );
    let full = layout
        .resolve_ordered_with_font(&next, [1280., 800.], &[], Some(&font))
        .unwrap();
    assert_ne!(
        previous.path(3).unwrap().last().unwrap().node.frame.width,
        full.path(3).unwrap().last().unwrap().node.frame.width
    );
}

#[test]
fn auto_layout_encoding_input_retains_signed_zero_in_bounds_and_padding() {
    let mut source = blueprint();
    source.schema = AUTO_LAYOUT_FORMAT.into();
    source.templates[0].root.layout = Some(NodeLayout {
        container: Some(AutoLayout::default()),
        ..NodeLayout::default()
    });
    let layout = Layout::from_blueprint(source.clone()).unwrap();
    source.templates[0].root.layout.as_mut().unwrap().width.min = -0.;
    assert_eq!(layout.blueprint(), &source);
    assert!(!layout.has_same_encoding_input(&source));
    source.templates[0].root.layout.as_mut().unwrap().width.min = 0.;
    source.templates[0]
        .root
        .layout
        .as_mut()
        .unwrap()
        .container
        .as_mut()
        .unwrap()
        .padding[0] = -0.;
    assert!(!layout.has_same_encoding_input(&source));
}
