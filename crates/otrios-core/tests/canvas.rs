// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/home/app.rs"]
mod home;
#[path = "../../../apps/layout-lab/app.rs"]
mod layout;
#[path = "../../../apps/component-playground/app.rs"]
mod playground;
use otrios_core::canvas::*;
use otrios_interfaces::canvas::*;
fn edit(c: &mut Client, label: &str, a: u16, b: u16) {
    let scene = c.scene().unwrap();
    let node = scene
        .controls()
        .iter()
        .find(|n| n.node.semantics.as_ref().is_some_and(|s| s.label == label))
        .unwrap()
        .node
        .id;
    c.event_for_node(c.epoch(), node, a, b).unwrap();
}
fn code() -> Vec<u8> {
    wat::parse_str(include_str!(
        "../../../apps/component-playground/behavior/main.wat"
    ))
    .unwrap()
}
fn activate(c: &mut Client, ui: Ui, state: &str, code: Vec<u8>) {
    c.open(&ui.ui_ref, state);
    assert!(matches!(
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::Mount { .. }
    ));
    let bytes = encode_ui(&ui).unwrap();
    c.receive(
        &encode_server(&ServerMessage::Mounted {
            epoch: c.epoch(),
            ui_hash: hash(&bytes),
            ui: bytes,
            code,
        })
        .unwrap(),
    )
    .unwrap();
}
fn pump(c: &mut Client, authority: &mut Document, session: &mut Session) {
    for _ in 0..20 {
        let mut progress = false;
        while let Some(b) = c.outgoing().unwrap() {
            if let ClientMessage::Sync { bytes, .. } = decode_client(&b).unwrap() {
                authority.receive(session, &bytes).unwrap();
                progress = true;
            }
        }
        if let Some(bytes) = authority.generate(session).unwrap() {
            c.receive(
                &encode_server(&ServerMessage::Sync {
                    epoch: c.epoch(),
                    bytes,
                })
                .unwrap(),
            )
            .unwrap();
            progress = true;
        }
        if !progress {
            assert!(c.confirmed());
            return;
        }
    }
    panic!("sync did not settle")
}
#[test]
fn two_states_reuse_ui_code_without_shared_mutable_scope_and_selectors_agree() {
    let code = code();
    let ui = playground::ui(&code);
    let mut a = Client::default();
    let mut b = Client::default();
    activate(&mut a, ui.clone(), "room:a", code.clone());
    activate(&mut b, ui.clone(), "room:b", code);
    let mut da = Document::new(ui.state_schema.clone(), "room:a").unwrap();
    let mut db = Document::new(ui.state_schema, "room:b").unwrap();
    let mut sa = Session::default();
    let mut sb = Session::default();
    pump(&mut a, &mut da, &mut sa);
    pump(&mut b, &mut db, &mut sb);
    edit(&mut a, "Linear slider", 870, 0);
    pump(&mut a, &mut da, &mut sa);
    assert_eq!(a.value(1).unwrap(), 870);
    assert_eq!(b.value(1).unwrap(), 420);
    let s = a.scene().unwrap();
    let controls = s.controls();
    let slider = controls
        .iter()
        .find(|c| {
            c.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Linear slider")
        })
        .unwrap();
    assert_eq!(slider.node.children.iter().find(|n|matches!(n.primitive,otrios_interfaces::scene::Primitive::Superellipse{color,..} if color==authoring::ACCENT)).unwrap().frame.width,870.*0.488+32.);
    fn has_text(n: &otrios_interfaces::scene::SceneNode, value: &str) -> bool {
        matches!(&n.primitive,otrios_interfaces::scene::Primitive::Text{value:v,..} if v==value)
            || n.children.iter().any(|c| has_text(c, value))
    }
    assert!(has_text(s.root(), "87%"));
    let old = a.epoch();
    activate(&mut a, layout::ui(), "layout:state", vec![]);
    assert!(!a.event(old, 1, 0, 0).unwrap());
    assert_eq!(a.value(1).unwrap(), 500);
    let mut dl = Document::new(layout::ui().state_schema, "layout:state").unwrap();
    pump(&mut a, &mut dl, &mut Session::default());
    activate(&mut a, ui_for_cache(), "room:a", vec![]);
    assert_eq!(a.value(1).unwrap(), 420); // Fresh RAM replica, not a persisted local checkpoint.
    pump(&mut a, &mut da, &mut Session::default());
    assert_eq!(a.value(1).unwrap(), 870);
}
fn ui_for_cache() -> Ui {
    playground::ui(&code())
}
#[test]
fn wasm_reducer_and_declarative_app_are_qualified_at_extremes() {
    for (ui, code) in [
        (playground::ui(&code()), code()),
        (layout::ui(), vec![]),
        (home::ui(), vec![]),
    ] {
        for v in [0, 1000] {
            let mut c = Client::default();
            activate(&mut c, ui.clone(), "scope", code.clone());
            for h in &ui.handlers {
                c.event(c.epoch(), h.control, v, v).unwrap();
            }
            c.scene().unwrap();
        }
    }
    assert_eq!(playground::manifest().dependencies.len(), 5);
    assert_eq!(layout::manifest().id, "layout-lab");
    assert_eq!(home::manifest().state_ref, "app:home/state");
}
fn guest(wat: &str, writes: Vec<u16>) -> Result<Behavior> {
    let b = wat::parse_str(wat).unwrap();
    Behavior::load(
        &b,
        &BehaviorRef {
            sha256: hash(&b),
            abi: 1,
            writes,
        },
    )
}
#[test]
fn wasm_bounds_imports_abi_digest_fuel_and_commands_are_enforced() {
    let b = code();
    let mut r = playground::ui(&b).behavior.unwrap();
    r.sha256[0] ^= 1;
    assert!(Behavior::load(&b, &r).is_err());
    r.sha256 = hash(&b);
    r.abi = 2;
    assert!(Behavior::load(&b, &r).is_err());
    assert!(guest("(module (import \"host\" \"write\" (func)))", vec![]).is_err());
    assert!(guest("(module (memory 2) (func (export \"abi_version\") (result i32) i32.const 1) (func (export \"dispatch\") (param i32 i32 i32 i32) (result i64) i64.const 0))",vec![]).is_err());
    assert!(guest("(module (func (export \"abi_version\") (result i32) i32.const 2) (func (export \"dispatch\") (param i32 i32 i32 i32) (result i64) i64.const 0))",vec![]).is_err());
    let mut infinite=guest("(module (func (export \"abi_version\") (result i32) i32.const 1) (func (export \"dispatch\") (param i32 i32 i32 i32) (result i64) (loop br 0) i64.const 0))",vec![]).unwrap();
    assert!(infinite.call(1, 0, 0, 0).is_err());
    let mut bad=guest("(module (func (export \"abi_version\") (result i32) i32.const 1) (func (export \"dispatch\") (param i32 i32 i32 i32) (result i64) i64.const 1125899906842625))",vec![2]).unwrap();
    assert!(bad.call(1, 0, 0, 0).is_err());
    let mut actual = Behavior::load(&b, &playground::ui(&b).behavior.unwrap()).unwrap();
    assert!(actual.call(999, 0, 0, 0).is_err());
    assert_eq!(
        actual.call(3, 123, 456, 0).unwrap(),
        vec![(3, 123), (4, 456)]
    );
}
#[test]
fn guest_instances_never_share_mutable_globals() {
    let bytes=wat::parse_str("(module (global $n (mut i64) (i64.const 0)) (func (export \"abi_version\") (result i32) i32.const 1) (func (export \"dispatch\") (param i32 i32 i32 i32) (result i64) global.get $n i64.const 1 i64.add global.set $n global.get $n i64.const 1125899906842624 i64.or))").unwrap();
    let r = BehaviorRef {
        sha256: hash(&bytes),
        abi: 1,
        writes: vec![1],
    };
    let mut a = Behavior::load(&bytes, &r).unwrap();
    let mut b = Behavior::load(&bytes, &r).unwrap();
    assert_eq!(a.call(1, 0, 0, 0).unwrap(), vec![(1, 1)]);
    assert_eq!(a.call(1, 0, 0, 0).unwrap(), vec![(1, 2)]);
    assert_eq!(b.call(1, 0, 0, 0).unwrap(), vec![(1, 1)]);
}
#[test]
fn corrupt_candidate_retains_previous_view_without_applying_code() {
    let mut c = Client::default();
    activate(&mut c, home::ui(), "home", vec![]);
    let mut d = Document::new(home::ui().state_schema, "home").unwrap();
    pump(&mut c, &mut d, &mut Session::default());
    c.open("app:layout-lab/ui", "layout");
    c.outgoing().unwrap();
    let b = encode_ui(&layout::ui()).unwrap();
    assert!(
        c.receive(
            &encode_server(&ServerMessage::Mounted {
                epoch: c.epoch(),
                ui: b,
                ui_hash: vec![0; 32],
                code: vec![]
            })
            .unwrap()
        )
        .is_err()
    );
    assert_eq!(c.title(), "Home");
    c.scene().unwrap();
}

#[test]
fn declarative_navigation_and_bindings_survive_reordering_without_manual_ids() {
    use otrios_interfaces::{
        layout::{Blueprint, Responder, TemplateNode},
        scene::TouchResponder,
    };
    fn navigation(ui: &Ui, label: &str) -> (String, String) {
        let layout = otrios_core::dashboard::layout::Layout::from_bytes(&ui.layout).unwrap();
        fn find<'a>(node: &'a TemplateNode, label: &str) -> Option<&'a TemplateNode> {
            if node.semantics.as_ref().is_some_and(|s|matches!(&s.label,otrios_interfaces::layout::Expr::Literal(otrios_interfaces::layout::Value::Text(t)) if t==label)){return Some(node)}
            node.children.iter().find_map(|c| find(c, label))
        }
        let Blueprint { templates, .. } = layout.blueprint();
        let node = find(&templates[0].root, label).unwrap();
        let Some(Responder::AppNavigate(action)) = node.responder else {
            panic!("missing navigation")
        };
        let destination = ui.navigation.iter().find(|n| n.action == action).unwrap();
        (destination.ui_ref.clone(), destination.state_ref.clone())
    }
    for reverse in [false, true] {
        let mut children = vec![
            authoring::button("Playground")
                .navigate_to(authoring::Destination::app("component-playground"))
                .frame([40., 200., 300., 90.]),
            authoring::button("Layout")
                .navigate_to(authoring::Destination::app("layout-lab"))
                .frame([40., 350., 300., 90.]),
        ];
        if reverse {
            children.reverse();
        }
        let ui = authoring::AppUi::new("launcher", "Launcher", authoring::schema("empty-v1", &[]))
            .build("Apps", children);
        assert_eq!(
            navigation(&ui, "Playground"),
            (
                "app:component-playground/ui".into(),
                "app:component-playground/state".into()
            )
        );
        assert_eq!(
            navigation(&ui, "Layout"),
            ("app:layout-lab/ui".into(), "app:layout-lab/state".into())
        );
        let mut c = Client::default();
        activate(&mut c, ui, "launcher", vec![]);
        let scene = c.scene().unwrap();
        assert!(
            scene
                .controls()
                .iter()
                .all(|n| matches!(n.node.responder, Some(TouchResponder::Activate(_))))
        );
        for control in scene.controls() {
            assert_eq!(
                control.node.children[0].frame.width, control.node.frame.width,
                "button background follows its own canvas, not the screen"
            );
        }
    }
    let level = authoring::StateField::normalized(1, "level", 500);
    let ui = authoring::AppUi::new(
        "labels",
        "Labels",
        authoring::schema("label-state-v1", &[level]),
    )
    .build(
        "Labels",
        vec![
            authoring::text(level.percent()).position([50., 200.]),
            authoring::text(level.percent()).position([50., 300.]),
        ],
    );
    assert_eq!(
        ui.selectors.len(),
        1,
        "shared field/projection is interned once"
    );
    assert_eq!(
        encode_ui(&home::ui()).unwrap(),
        encode_ui(&home::ui()).unwrap(),
        "same source compiles deterministically"
    );
}

#[test]
fn reusable_pinned_components_reflow_without_changing_state_or_input_identity() {
    use otrios_core::dashboard::input::InputRouter;
    use otrios_interfaces::scene::{Action, PointerEvent, PointerPhase, Primitive, SceneNode};
    fn card<'a>(n: &'a SceneNode, title: &str) -> Option<&'a SceneNode> {
        if matches!(n.primitive, Primitive::Canvas { .. })
            && n.children
                .iter()
                .any(|c| matches!(&c.primitive,Primitive::Text{value,..} if value==title))
        {
            return Some(n);
        }
        n.children.iter().find_map(|c| card(c, title))
    }
    let mut c = Client::default();
    let ui = layout::ui();
    activate(&mut c, ui, "lab", vec![]);
    let old = c.scene().unwrap();
    let old_card = card(old.root(), "Reusable").unwrap();
    assert_eq!(old_card.frame.width, 420.);
    let old_id = old_card.id;
    edit(&mut c, "Panel width", 1000, 0);
    let wide = c.scene().unwrap();
    let wide_card = card(wide.root(), "Reusable").unwrap();
    assert_eq!(wide_card.id, old_id);
    assert_eq!(wide_card.frame.width, 520.);
    assert_eq!(
        card(old.root(), "Reusable").unwrap().frame.width,
        420.,
        "pinned old snapshot is unchanged"
    );
    // Two instances preserve the same design, with different width/height inputs.
    let compact = card(wide.root(), "Compact").unwrap();
    assert_eq!((compact.frame.width, compact.frame.height), (420., 220.));
    for card in [wide_card, compact] {
        let dot = card
            .children
            .iter()
            .find(|c| matches!(c.primitive, Primitive::Circle { .. }))
            .unwrap();
        assert_eq!(card.frame.width - dot.frame.x - dot.frame.width, 24.);
        let button = card
            .children
            .iter()
            .find(|c| c.semantics.as_ref().is_some_and(|s| s.label == "Min width"))
            .unwrap();
        assert_eq!(button.frame.width, card.frame.width - 48.);
        assert_eq!(
            card.frame.height - button.frame.y - button.frame.height,
            24.
        );
        let body = card
            .children
            .iter()
            .find(|c| matches!(c.primitive, Primitive::Canvas { .. }) && c.semantics.is_none())
            .unwrap();
        let center = body
            .children
            .iter()
            .find(|c| matches!(c.primitive, Primitive::Circle { .. }))
            .unwrap();
        assert_eq!(
            center.frame.x + center.frame.width / 2.,
            body.frame.width / 2.
        );
        assert_eq!(
            center.frame.y + center.frame.height / 2.,
            body.frame.height / 2.
        );
    }
    let point = [1150., wide_card.frame.y + wide_card.frame.height - 48.];
    let has_button = |s: &otrios_core::dashboard::scene::Scene| {
        s.hit_path(point).iter().any(|n| {
            n.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Min width")
        })
    };
    assert!(!has_button(&old));
    assert!(has_button(&wide));
    let mut input = InputRouter::default();
    input.dispatch(
        wide.clone(),
        PointerEvent {
            id: 1,
            phase: PointerPhase::Down,
            position: point,
        },
    );
    let up = input.dispatch(
        wide,
        PointerEvent {
            id: 1,
            phase: PointerPhase::Up,
            position: point,
        },
    );
    assert!(
        matches!(up.action, Some(Action::AppEvent(_))),
        "hit testing uses the newly resolved component frame"
    );
    assert_eq!(
        c.value(1).unwrap(),
        1000,
        "layout does not write resolved geometry into app state"
    );
}

#[test]
fn legacy_layout_bytes_are_unchanged_and_resize_rules_require_v2() {
    use otrios_core::dashboard::layout::{DEFAULT_LAYOUT, Layout};
    let legacy = Layout::bootstrap().unwrap();
    assert_eq!(Layout::encode(legacy.blueprint()).unwrap(), DEFAULT_LAYOUT);
    let ui = layout::ui();
    let parsed = Layout::from_bytes(&ui.layout).unwrap();
    assert_eq!(parsed.blueprint().schema, "otrios-layout-v2");
    let mut downgraded = parsed.blueprint().clone();
    downgraded.schema = "otrios-layout-v1".into();
    assert!(
        Layout::encode(&downgraded).is_err(),
        "old format cannot silently reinterpret new geometry rules"
    );
    let mut invalid = parsed.blueprint().clone();
    invalid.templates[0].root.resize = Some(otrios_interfaces::layout::ResizeRule {
        reference_size: [1280., 800.],
        horizontal: otrios_interfaces::layout::AxisResize::Fixed,
        vertical: otrios_interfaces::layout::AxisResize::Fixed,
    });
    assert!(
        Layout::encode(&invalid).is_err(),
        "component root size belongs to its instance"
    );
}

#[test]
fn authored_spring_masks_are_serialized_and_resolved_at_instance_size() {
    let graphic = authoring::component(
        [200., 100.],
        vec![
            authoring::rounded_rect(authoring::CARD)
                .frame([20., 20., 100., 40.])
                .resizing(authoring::Resizing {
                    horizontal: authoring::AxisResize::Springs {
                        leading: true,
                        length: false,
                        trailing: true,
                    },
                    vertical: authoring::AxisResize::Fixed,
                }),
        ],
    )
    .frame([40., 200., 300., 100.]);
    let ui = authoring::AppUi::new("springs", "Springs", authoring::schema("empty-v1", &[]))
        .build("Springs", vec![graphic]);
    let mut client = Client::default();
    activate(&mut client, ui, "springs", vec![]);
    let scene = client.scene().unwrap();
    let instance = scene
        .root()
        .children
        .iter()
        .find(|n| n.frame.width == 300.)
        .unwrap();
    assert_eq!(
        instance.children[0].frame,
        otrios_interfaces::scene::Rect::new(40., 20., 100., 40.)
    );
}

#[test]
fn projected_scene_is_reused_until_selected_state_changes() {
    use std::sync::Arc;
    let level = authoring::StateField::normalized(1, "level", 500);
    let other = authoring::StateField::normalized(2, "other", 100);
    let ui = authoring::AppUi::new(
        "cache",
        "Cache",
        authoring::schema("cache-v1", &[level, other]),
    )
    .build(
        "Cache",
        vec![
            authoring::text(level.percent()).position([40., 200.]),
            authoring::canvas(vec![authoring::rounded_rect(authoring::CARD)])
                .frame([40., 300., 200., 100.])
                .on_drag("Other", authoring::Event::slider(other)),
            authoring::canvas(vec![authoring::rounded_rect(authoring::CARD)])
                .frame([300., 300., 200., 100.])
                .on_drag("Level", authoring::Event::slider(level)),
        ],
    );
    let schema = ui.state_schema.clone();
    let mut c = Client::default();
    activate(&mut c, ui, "cache", vec![]);
    let before = c.scene().unwrap();
    assert!(Arc::ptr_eq(&before, &c.scene().unwrap()));
    edit(&mut c, "Other", 900, 0);
    assert_eq!(c.value(2).unwrap(), 900);
    assert!(
        Arc::ptr_eq(&before, &c.scene().unwrap()),
        "unselected fields do not rebuild UI"
    );
    let mut authority = Document::new(schema, "cache").unwrap();
    pump(&mut c, &mut authority, &mut Session::default());
    assert_eq!(
        authority.value(2).unwrap(),
        900,
        "nonvisual edits still sync"
    );
    assert!(
        Arc::ptr_eq(&before, &c.scene().unwrap()),
        "protocol acknowledgements retain the frame"
    );
    edit(&mut c, "Level", 500, 0);
    assert!(
        Arc::ptr_eq(&before, &c.scene().unwrap()),
        "no-op edit retains the frame"
    );
    edit(&mut c, "Level", 700, 0);
    let changed = c.scene().unwrap();
    assert!(!Arc::ptr_eq(&before, &changed));
    assert!(Arc::ptr_eq(&changed, &c.scene().unwrap()));
}
#[test]
fn app_scope_rejects_dashboard_bindings_and_legacy_responder_domains() {
    use otrios_core::dashboard::layout::Layout;
    use otrios_interfaces::layout::{Binding, Expr, Responder, Shape};
    let ui = home::ui();
    let mut blueprint = Layout::from_bytes(&ui.layout).unwrap().blueprint().clone();
    let heading = blueprint.templates[0]
        .root
        .children
        .iter_mut()
        .find(|n| matches!(n.shape, Shape::Text { .. }))
        .unwrap();
    if let Shape::Text { value, .. } = &mut heading.shape {
        *value = Expr::State(Binding::Enabled);
    }
    let bytes = Layout::encode(&blueprint).unwrap();
    let err = Layout::from_bytes(&bytes)
        .unwrap()
        .resolve_slots(&std::collections::BTreeMap::new())
        .unwrap_err();
    assert!(err.contains("Dashboard bindings are unavailable"));
    // Even an unlabeled/hidden old responder is rejected, rather than ignored or
    // interpreted as an event with the same numeric index.
    for responder in [Responder::ToggleFocus, Responder::AppNavigate(250)] {
        let mut bad = home::ui();
        let mut b = Layout::from_bytes(&bad.layout).unwrap().blueprint().clone();
        let root = &mut b.templates[0].root;
        root.children[0].responder = Some(responder);
        root.children[0].semantics = None;
        bad.layout = Layout::encode(&b).unwrap();
        let data = encode_ui(&bad).unwrap();
        let mut c = Client::default();
        c.open(&bad.ui_ref, "scope");
        c.outgoing().unwrap();
        assert!(
            c.receive(
                &encode_server(&ServerMessage::Mounted {
                    epoch: c.epoch(),
                    ui_hash: hash(&data),
                    ui: data,
                    code: vec![]
                })
                .unwrap()
            )
            .is_err()
        );
    }
}
