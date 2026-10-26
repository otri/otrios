// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
fn n(v: f32) -> Expr {
    Expr::Literal(Value::Number(v))
}
#[test]
fn fixed_expression_keeps_small_terms_until_the_scene_boundary() {
    let state = BTreeMap::new();
    let params = BTreeMap::new();
    let bindings = Bindings { local: &state };
    let frame = [0., 0., 8192., 4096.];
    let expression = Expr::Subtract(
        Box::new(Expr::Add(Box::new(n(8192.)), Box::new(n(0.0001)))),
        Box::new(n(8192.)),
    );
    let fixed = eval_fixed_number(&expression, frame, &params, &bindings)
        .unwrap()
        .unwrap()
        .to_f32();
    assert!((fixed - 0.0001).abs() < 0.000000001);
    assert_eq!(
        number(eval_float32(&expression, frame, &params, &bindings).unwrap()).unwrap(),
        0.
    );
    let ratio = Expr::Multiply(Box::new(n(0.0001)), Box::new(Expr::Width));
    let fixed = eval_fixed_number(&ratio, frame, &params, &bindings)
        .unwrap()
        .unwrap()
        .to_f32();
    assert!((fixed - 0.0001 * 8192.).abs() < 0.000002);
    // Nonnumeric conditional branches still use the existing typed interpreter.
    let text = Expr::Select {
        condition: Box::new(Expr::Literal(Value::Boolean(true))),
        yes: Box::new(s("yes")),
        no: Box::new(s("no")),
    };
    assert!(
        eval_fixed_number(&text, frame, &params, &bindings)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        eval(&text, frame, &params, &bindings).unwrap(),
        Value::Text("yes".into())
    );
    // A candidate range limit must not reject an expression the old contract
    // accepts. Overflow and sub-grid products fall back as a whole expression.
    for expression in [
        Expr::Subtract(Box::new(n(1e20)), Box::new(n(1e20))),
        Expr::Multiply(Box::new(n(1e-6)), Box::new(n(1e-6))),
        Expr::Add(Box::new(n(1e-30)), Box::new(n(0.))),
    ] {
        assert!(eval_fixed_number(&expression, frame, &params, &bindings).is_err());
        assert_eq!(
            eval(&expression, frame, &params, &bindings).unwrap(),
            eval_float32(&expression, frame, &params, &bindings).unwrap()
        );
    }
}
fn s(v: &str) -> Expr {
    Expr::Literal(Value::Text(v.into()))
}
fn full() -> [Expr; 4] {
    [n(0.), n(0.), Expr::Width, Expr::Height]
}
fn node(id: u16, shape: Shape, children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode {
        id,
        frame: full(),
        shape,
        children,
        visible: Expr::Literal(Value::Boolean(true)),
        resize: None,
        responder: None,
        semantics: None,
    }
}
fn instance(
    id: u32,
    template: u16,
    frame: [f32; 4],
    properties: Vec<Property>,
    children: Vec<Instance>,
) -> Instance {
    Instance {
        id,
        template,
        frame,
        properties,
        children,
    }
}
fn blueprint() -> Blueprint {
    Blueprint {
        composition: None,
        schema: "otrios-layout-v2".into(),
        templates: vec![
            Template {
                id: 1,
                name: "Canvas".into(),
                parameters: vec![],
                root: node(
                    0,
                    Shape::Canvas { clip: true },
                    vec![node(900, Shape::Slot, vec![])],
                ),
            },
            Template {
                id: 2,
                name: "Text".into(),
                parameters: vec![Parameter {
                    key: 1,
                    name: "text".into(),
                    value_type: ValueType::Text,
                    default: s(""),
                }],
                root: node(
                    0,
                    Shape::Text {
                        value: Expr::Parameter(1),
                        scale: n(2.),
                        color: Expr::Literal(Value::Color([255; 4])),
                    },
                    vec![],
                ),
            },
            Template {
                id: 3,
                name: "Panel".into(),
                parameters: vec![Parameter {
                    key: 1,
                    name: "radius".into(),
                    value_type: ValueType::Number,
                    default: n(16.),
                }],
                root: node(
                    0,
                    Shape::Canvas { clip: true },
                    vec![
                        node(
                            1,
                            Shape::Superellipse {
                                radius: Expr::Parameter(1),
                                exponent: 4.,
                                color: Expr::Literal(Value::Color([60, 80, 120, 255])),
                            },
                            vec![],
                        ),
                        node(900, Shape::Slot, vec![]),
                    ],
                ),
            },
        ],
        root: instance(
            0,
            1,
            [0., 0., 1280., 800.],
            vec![],
            vec![
                instance(
                    1,
                    2,
                    [20., 20., 200., 40.],
                    vec![Property {
                        key: 1,
                        value: Expr::State(Binding::Local(1)),
                    }],
                    vec![],
                ),
                instance(
                    2,
                    3,
                    [20., 80., 200., 120.],
                    vec![],
                    vec![instance(
                        3,
                        2,
                        [10., 10., 180., 40.],
                        vec![Property {
                            key: 1,
                            value: s("Nested"),
                        }],
                        vec![],
                    )],
                ),
            ],
        ),
    }
}
fn slots() -> BTreeMap<u16, Value> {
    BTreeMap::from([(1, Value::Text("First".into())), (2, Value::Boolean(true))])
}
#[test]
fn typed_binary_roundtrips_without_json_and_bindings_are_explicit() {
    let bytes = Layout::encode(&blueprint()).unwrap();
    let map = Reader::get_root(bytes.as_slice())
        .unwrap()
        .get_map()
        .unwrap();
    assert_eq!(
        map.index("root")
            .unwrap()
            .get_map()
            .unwrap()
            .index("frame")
            .unwrap()
            .flexbuffer_type(),
        FlexBufferType::VectorFloat4
    );
    let layout = Layout::from_bytes(&bytes).unwrap();
    assert_eq!(Layout::encode(layout.blueprint()).unwrap(), bytes);
    assert!(layout.resolve_slots(&BTreeMap::new()).is_err());
    assert!(layout.resolve_slots(&slots()).is_ok());
    #[derive(serde::Serialize)]
    enum RetiredBinding {
        Enabled,
    }
    #[derive(serde::Serialize)]
    enum RetiredResponder {
        ToggleFocus,
    }
    assert!(
        flexbuffers::from_slice::<Binding>(&flexbuffers::to_vec(RetiredBinding::Enabled).unwrap())
            .is_err()
    );
    assert!(
        flexbuffers::from_slice::<Responder>(
            &flexbuffers::to_vec(RetiredResponder::ToggleFocus).unwrap()
        )
        .is_err()
    );
}
#[test]
fn layout_and_scene_are_immutable_across_reconfiguration_and_selected_values() {
    let original = Layout::from_bytes(&Layout::encode(&blueprint()).unwrap()).unwrap();
    let old = original.resolve_slots(&slots()).unwrap();
    let mut changed = original.blueprint().clone();
    changed.root.children[0].frame[0] = 100.;
    let next = Layout::from_bytes(&Layout::encode(&changed).unwrap()).unwrap();
    let mut values = slots();
    values.insert(1, Value::Text("Second".into()));
    let current = next.resolve_slots(&values).unwrap();
    assert_eq!(old.path(1000).unwrap().last().unwrap().origin, [20., 20.]);
    assert_eq!(
        current.path(1000).unwrap().last().unwrap().origin,
        [100., 20.]
    );
    assert!(
        matches!(&old.path(1000).unwrap().last().unwrap().node.primitive,Primitive::Text {value,..} if value=="First")
    );
    assert!(
        matches!(&current.path(1000).unwrap().last().unwrap().node.primitive,Primitive::Text {value,..} if value=="Second")
    );
    assert_ne!(original.hex_id(), next.hex_id());
    assert!(Layout::from_bytes(b"broken").is_err());
    assert_eq!(original.resolve_slots(&slots()).unwrap().root(), old.root());
}
#[test]
fn template_geometry_slots_and_instance_identity_remain_modular() {
    let mut blueprint = blueprint();
    blueprint.templates[2].parameters[0].default = n(24.);
    let layout = Layout::from_bytes(&Layout::encode(&blueprint).unwrap()).unwrap();
    let scene = layout.resolve_slots(&slots()).unwrap();
    assert!(scene.path(3000).is_some());
    assert!(matches!(
        scene.path(2001).unwrap().last().unwrap().node.primitive,
        Primitive::Superellipse {
            radius: 24.,
            color: [60, 80, 120, 255],
            ..
        }
    ));
    blueprint.root.children[0].id = 3;
    assert!(Layout::encode(&blueprint).is_err());
}
#[test]
fn dynamic_geometry_and_property_types_are_checked_when_resolved() {
    let mut blueprint = blueprint();
    blueprint.root.children[1].properties.push(Property {
        key: 1,
        value: Expr::Select {
            condition: Box::new(Expr::State(Binding::Local(2))),
            yes: Box::new(n(16.)),
            no: Box::new(n(1000.)),
        },
    });
    let layout = Layout::from_bytes(&Layout::encode(&blueprint).unwrap()).unwrap();
    assert!(layout.resolve_slots(&slots()).is_ok());
    let mut values = slots();
    values.insert(2, Value::Boolean(false));
    assert!(layout.resolve_slots(&values).is_err());
    blueprint.root.children[1].properties[0].value = s("wrong radius type");
    let layout = Layout::from_bytes(&Layout::encode(&blueprint).unwrap()).unwrap();
    assert!(layout.resolve_slots(&slots()).is_err());
}
#[test]
fn truncated_mutated_oversize_and_nested_data_fail_safely() {
    let bytes = Layout::encode(&blueprint()).unwrap();
    for end in 0..bytes.len() {
        assert!(Layout::from_bytes(&bytes[..end]).is_err());
    }
    for i in (0..bytes.len()).step_by(11) {
        let mut bad = bytes.clone();
        bad[i] ^= 255;
        let _ = Layout::from_bytes(&bad);
    }
    assert!(Layout::from_bytes(&vec![0; MAX_LAYOUT_BYTES + 1]).is_err());
    let mut deep = Expr::Literal(Value::Number(1.));
    for _ in 0..14 {
        deep = Expr::Add(Box::new(deep), Box::new(n(1.)));
    }
    let mut blueprint = blueprint();
    blueprint.root.children[1].properties.push(Property {
        key: 1,
        value: deep,
    });
    assert!(Layout::encode(&blueprint).is_err());
    blueprint.schema = "unknown".into();
    assert!(Layout::encode(&blueprint).is_err());
}
