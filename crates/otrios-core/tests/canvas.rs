// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../apps/animation-lab/app.rs"]
mod animation_lab;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../tools/canvas/components.rs"]
pub mod components;
#[path = "../../../apps/home/app.rs"]
mod home;
#[path = "../../../apps/input-lab/app.rs"]
mod input_lab;
#[path = "../../../apps/layout-lab/app.rs"]
mod layout;
#[path = "../../../apps/monitoring/app.rs"]
mod monitoring_app;
#[path = "../../../apps/component-playground/app.rs"]
mod playground;
#[path = "../../../apps/three-d-lab/app.rs"]
mod three_d_lab;
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
    activate_access(c, ui, state, code, false);
}
fn activate_access(c: &mut Client, ui: Ui, state: &str, code: Vec<u8>, private: bool) {
    activate_snapshot(c, ui, state, code, private, vec![]);
}
fn activate_snapshot(
    c: &mut Client,
    ui: Ui,
    state: &str,
    code: Vec<u8>,
    private: bool,
    state_bytes: Vec<u8>,
) {
    let bytes = encode_ui(&ui).unwrap();
    let id = ui
        .ui_ref
        .strip_prefix("app:")
        .unwrap()
        .strip_suffix("/ui")
        .unwrap();
    c.admit_catalog(
        &encode_catalog(&Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: vec![CatalogEntry {
                app: App {
                    id: id.into(),
                    title: ui.title.clone(),
                    ui_ref: ui.ui_ref.clone(),
                    state_ref: state.into(),
                    status: "canvas-app".into(),
                    dependencies: vec![],
                },
                ui_hash: hash(&bytes),
                private,
            }],
        })
        .unwrap(),
    )
    .unwrap();
    c.open(&ui.ui_ref, state);
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("missingmount")
    };
    c.receive(
        &encode_server(&ServerMessage::Mounted {
            epoch,
            ui_hash: hash(&bytes),
            ui: bytes,
            code,
            state: state_bytes,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(
        matches!(decode_client(&c.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::AcceptMount{epoch:e} if e==epoch)
    );
}
fn pump(c: &mut Client, authority: &mut Document, session: &mut Session) {
    for step in 0..20 {
        c.tick(1000 + step * 33, 0).unwrap();
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
    let pending_scene = a.scene().unwrap();
    let slider = pending_scene
        .controls()
        .iter()
        .find(|c| {
            c.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Linear slider")
        })
        .unwrap()
        .node
        .id;
    assert_eq!(a.control_value(slider).unwrap(), Some(87.));
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
    assert_eq!(slider.node.children.iter().find(|n|matches!(n.primitive,otrios_interfaces::scene::Primitive::Superellipse{color,..} if color==authoring::ACCENT)).unwrap().frame.width,870.*0.281+1.);
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
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("missing mount")
    };
    let b = encode_ui(&layout::ui()).unwrap();
    assert!(
        c.receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                state: vec![],
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
    let point = [
        wide_card.frame.x + wide_card.frame.width - 50.,
        wide_card.frame.y + wide_card.frame.height - 48.,
    ];
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
        c.admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app: App {
                        id: "home".into(),
                        title: "Home".into(),
                        ui_ref: bad.ui_ref.clone(),
                        state_ref: "scope".into(),
                        status: "canvas-app".into(),
                        dependencies: vec![],
                    },
                    ui_hash: hash(&data),
                    private: false,
                }],
            })
            .unwrap(),
        )
        .unwrap();
        c.open(&bad.ui_ref, "scope");
        let ClientMessage::Mount { epoch, .. } =
            decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("missing mount")
        };
        assert!(
            c.receive(
                &encode_server(&ServerMessage::Mounted {
                    epoch,
                    state: vec![],
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

fn node(c: &Client, label: &str) -> u32 {
    c.scene()
        .unwrap()
        .controls()
        .iter()
        .find(|n| n.node.semantics.as_ref().is_some_and(|s| s.label == label))
        .unwrap()
        .node
        .id
}
fn text_contains(n: &otrios_interfaces::scene::SceneNode, value: &str) -> bool {
    matches!(&n.primitive,otrios_interfaces::scene::Primitive::Text{value:t,..} if t.contains(value))
        || n.children.iter().any(|n| text_contains(n, value))
}
#[test]
fn animation_and_three_d_own_independent_intent_and_bounded_private_stress_history() {
    use std::sync::Arc;
    let mut animation = Client::default();
    let mut three_d = Client::default();
    let aui = animation_lab::ui();
    let tui = three_d_lab::ui();
    activate(&mut animation, aui.clone(), "animation:state", vec![]);
    activate(&mut three_d, tui.clone(), "three-d:state", vec![]);
    let mut authority = Document::new(aui.state_schema, "animation:state").unwrap();
    let mut session = Session::default();
    pump(&mut animation, &mut authority, &mut session);
    animation
        .activate(animation.epoch(), node(&animation, "Play / Pause"))
        .unwrap();
    edit(&mut animation, "Animation speed", 800, 0);
    pump(&mut animation, &mut authority, &mut session);
    assert!(!animation.motion_intent().unwrap().unwrap().1.playing);
    assert!(three_d.motion_intent().unwrap().unwrap().1.playing);
    assert_eq!(three_d.value(2).unwrap(), 200);
    let before = animation.scene().unwrap();
    let authority_before = authority.save().unwrap();
    animation.set_stress(30).unwrap();
    for step in 0..600 {
        animation.stress_tick(f64::from(step) * 40.).unwrap();
    }
    assert_eq!(animation.stress_updates(), 600);
    assert_eq!(
        animation.value(4).unwrap(),
        0,
        "Diagnostic writes must never enter the app document"
    );
    assert!(animation.confirmed());
    assert!(Arc::ptr_eq(&before, &animation.scene().unwrap()));
    assert_eq!(authority_before, authority.save().unwrap());
    assert_ne!(
        animation_lab::manifest().state_ref,
        three_d_lab::manifest().state_ref
    );
}
#[test]
fn input_shared_text_keeps_focus_and_caret_local_and_keyboard_repeats_cancel() {
    use otrios_interfaces::{
        keyboard::Edit,
        layout::Value,
        scene::{PointerEvent, PointerPhase},
    };
    let ui = input_lab::ui();
    let mut a = Client::default();
    let mut b = Client::default();
    activate(&mut a, ui.clone(), "input:shared", vec![]);
    activate(&mut b, ui.clone(), "input:shared", vec![]);
    let mut authority = Document::new(ui.state_schema, "input:shared").unwrap();
    let mut sa = Session::default();
    let mut sb = Session::default();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    a.activate(a.epoch(), node(&a, "First text field")).unwrap();
    a.hardware_edit(&Edit::Commit("abcdef".into())).unwrap();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    assert_eq!(b.typed_value(1).unwrap(), Value::Text("abcdef".into()));
    assert!(!b.editing());
    assert!(!b.keyboard_visible());
    // Holding Backspace edits on down and repeat, never again on release.
    a.tick(0, 0).unwrap();
    let keyboard_scene = a.scene().unwrap();
    let backspace = keyboard_scene
        .controls()
        .into_iter()
        .find(|n| n.node.semantics.as_ref().is_some_and(|s| s.label == "BKSP"))
        .unwrap();
    let position = [backspace.origin[0] + 20., backspace.origin[1] + 20.];
    a.pointer(
        a.epoch(),
        PointerEvent {
            id: 7,
            phase: PointerPhase::Down,
            position,
        },
    )
    .unwrap();
    assert_eq!(a.typed_value(1).unwrap(), Value::Text("abcde".into()));
    assert!(a.input_busy());
    a.tick(450, 0).unwrap();
    assert_eq!(a.typed_value(1).unwrap(), Value::Text("abcd".into()));
    // A focus change cancels the held key and its repeat timer.
    a.activate(a.epoch(), node(&a, "Second text field"))
        .unwrap();
    a.tick(1000, 0).unwrap();
    assert!(!a.input_busy());
    assert_eq!(a.typed_value(1).unwrap(), Value::Text("abcd".into()));
    a.hardware_edit(&Edit::Commit("xyz".into())).unwrap();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    assert_eq!(b.typed_value(2).unwrap(), Value::Text("xyz".into()));
    // Remote replacement cannot leave the local caret inside a grapheme cluster.
    a.activate(a.epoch(), node(&a, "First text field")).unwrap();
    for _ in 0..4 {
        a.hardware_edit(&Edit::Left { select: false }).unwrap();
    }
    a.hardware_edit(&Edit::Right { select: false }).unwrap();
    authority
        .set_typed(&[(1, Value::Text("a\u{301}z".into()))])
        .unwrap();
    pump(&mut a, &mut authority, &mut sa);
    assert!(text_contains(a.scene().unwrap().root(), "|a\u{301}z"));
    a.back().unwrap();
    assert!(!a.keyboard_visible());
    a.tick(3000, 0).unwrap();
    assert_eq!(a.typed_value(2).unwrap(), Value::Text("xyz".into()));
    assert_eq!(input_lab::manifest().id, "input-lab");
}
#[test]
fn access_revocation_drops_private_view_and_back_cancels_rejected_sign_in() {
    let ui = home::ui();
    let mut c = Client::default();
    activate_access(&mut c, ui, "private", vec![], true);
    let old = c.epoch();
    assert!(c.private());
    assert!(
        c.receive(&encode_server(&ServerMessage::AccessRequired { epoch: old }).unwrap())
            .unwrap()
    );
    assert!(c.ui_ref().is_empty());
    assert!(c.scene().is_err());
    assert!(c.auth_required());
    assert!(!c.confirmed());
    assert_eq!(c.pending_state_ref(), "private");
    c.back().unwrap();
    assert!(!c.auth_required());
    let ClientMessage::Mount {
        ui_ref, state_ref, ..
    } = decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Home mount")
    };
    assert_eq!(
        (ui_ref.as_str(), state_ref.as_str()),
        ("app:home/ui", "app:home/state")
    );
}

#[test]
fn all_seven_app_packages_qualify_default_and_scalar_extremes() {
    use otrios_interfaces::layout::Value;
    let code = code();
    for (ui, code, private) in [
        (home::ui(), vec![], false),
        (playground::ui(&code), code, false),
        (layout::ui(), vec![], false),
        (input_lab::ui(), vec![], false),
        (animation_lab::ui(), vec![], false),
        (three_d_lab::ui(), vec![], false),
        (monitoring_app::ui(), vec![], true),
    ] {
        for high in [false, true] {
            let mut authority = Document::new(ui.state_schema.clone(), "qualify:state").unwrap();
            let commands = ui
                .state_schema
                .fields
                .iter()
                .filter(|f| f.writable)
                .filter_map(|f| match f.default {
                    Value::Number(_) => Some((f.id, Value::Number(if high { 1000. } else { 0. }))),
                    Value::Text(_) => Some((
                        f.id,
                        Value::Text(if high {
                            "é\u{301} 👩‍🚀".into()
                        } else {
                            String::new()
                        }),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            authority.set_typed(&commands).unwrap();
            let mut client = Client::default();
            activate_snapshot(
                &mut client,
                ui.clone(),
                "qualify:state",
                code.clone(),
                private,
                authority.save().unwrap(),
            );
            client.scene().unwrap();
            if let Some((_, intent)) = client.motion_intent().unwrap() {
                intent.validate().unwrap();
            }
            assert_eq!(client.private(), private);
        }
    }
    assert!(
        monitoring_app::manifest()
            .dependencies
            .iter()
            .any(|d| d == "capability:monitoring.read")
    );
}

#[test]
fn touch_pad_moves_are_ephemeral_and_only_release_is_durable_with_matching_coordinates() {
    use otrios_interfaces::{
        ink,
        scene::{PointerEvent, PointerPhase, Primitive},
    };
    let code = code();
    let ui = playground::ui(&code);
    let mut client = Client::default();
    activate(&mut client, ui.clone(), "pad:state", code);
    let mut authority = Document::new(ui.state_schema, "pad:state").unwrap();
    let mut session = Session::default();
    pump(&mut client, &mut authority, &mut session);
    client.tick(0, 0).unwrap();
    let scene = client.scene().unwrap();
    let controls = scene.controls();
    let pad = controls
        .iter()
        .find(|c| {
            c.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Touch pad")
        })
        .unwrap();
    let pad_origin = pad.origin;
    let pad_size = [pad.node.frame.width, pad.node.frame.height];
    for (phase, x, y) in [
        (PointerPhase::Down, 0.2, 0.25),
        (PointerPhase::Move, 0.8, 0.75),
    ] {
        client
            .pointer(
                client.epoch(),
                PointerEvent {
                    id: 9,
                    phase,
                    position: [
                        pad_origin[0] + x * pad_size[0],
                        pad_origin[1] + y * pad_size[1],
                    ],
                },
            )
            .unwrap();
    }
    client.tick(1000, 0).unwrap();
    assert_eq!(
        (client.value(3).unwrap(), client.value(4).unwrap()),
        (800, 750)
    );
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        (authority.value(3).unwrap(), authority.value(4).unwrap()),
        (500, 500)
    );
    let points = client.ink_outgoing(2000);
    let last = ink::Point::decode(&points[points.len() - ink::CLIENT_BYTES..]).unwrap();
    assert_eq!(last.position, [800, 750]);
    let scene = client.scene().unwrap();
    fn contains_point(n: &otrios_interfaces::scene::SceneNode) -> bool {
        matches!(&n.primitive,Primitive::Path{points,..} if points.last().is_some_and(|p|(p[0]-281.6).abs()<0.1&&(p[1]-157.5).abs()<0.1))
            || n.children.iter().any(contains_point)
    }
    assert!(contains_point(scene.root()));
    client
        .pointer(
            client.epoch(),
            PointerEvent {
                id: 9,
                phase: PointerPhase::Up,
                position: [
                    pad_origin[0] + 0.9 * pad_size[0],
                    pad_origin[1] + 0.6 * pad_size[1],
                ],
            },
        )
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        (authority.value(3).unwrap(), authority.value(4).unwrap()),
        (900, 600)
    );
}

#[test]
fn code_reload_waits_for_intents_but_not_an_idle_mount_handshake() {
    let mut fresh = Client::default();
    fresh.open("app:component-playground/ui", "fresh");
    fresh.outgoing().unwrap();
    assert!(!fresh.confirmed());
    assert!(
        fresh.reload_ready(),
        "No user data exists before the first mount"
    );

    let ui = playground::ui(&code());
    let mut client = Client::default();
    activate(&mut client, ui.clone(), "reload-gate", code());
    let mut authority = Document::new(ui.state_schema, "reload-gate").unwrap();
    pump(&mut client, &mut authority, &mut Session::default());
    assert!(client.reload_ready());
    client.reset_connection();
    client.outgoing().unwrap();
    assert!(!client.confirmed());
    assert!(
        client.reload_ready(),
        "A clean offline remount must not strand code"
    );
    edit(&mut client, "Linear slider", 870, 0);
    assert!(
        !client.reload_ready(),
        "The RAM overlay must survive an upgrade offer"
    );
}

#[test]
fn reconnect_preserves_unsent_coalesced_intent_and_mount_waits_for_confirmation() {
    let code = code();
    let ui = playground::ui(&code);
    let mut client = Client::default();
    activate(&mut client, ui.clone(), "reconnect", code.clone());
    let mut authority = Document::new(ui.state_schema, "reconnect").unwrap();
    let mut session = Session::default();
    pump(&mut client, &mut authority, &mut session);
    edit(&mut client, "Linear slider", 870, 0);
    assert_eq!(client.value(1).unwrap(), 870);
    assert_eq!(authority.value(1).unwrap(), 420);
    client.reset_connection();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount")
    };
    assert!(!client.confirmed());
    assert!(!client.reload_ready());
    let bytes = encode_ui(&playground::ui(&code)).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code,
                state: authority.save().unwrap(),
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        870,
        "Unsent preview vanished during reconnect activation"
    );
    pump(&mut client, &mut authority, &mut Session::default());
    assert_eq!(authority.value(1).unwrap(), 870);
}

#[test]
fn staged_reload_freezes_old_editor_and_rejected_candidate_leaves_it_usable() {
    use otrios_interfaces::{keyboard::Edit, layout::Value};
    let ui = input_lab::ui();
    let mut client = Client::default();
    activate(&mut client, ui.clone(), "editor:reload", vec![]);
    let mut authority = Document::new(ui.state_schema.clone(), "editor:reload").unwrap();
    let mut session = Session::default();
    pump(&mut client, &mut authority, &mut session);
    client
        .activate(client.epoch(), node(&client, "First text field"))
        .unwrap();
    client
        .hardware_edit(&Edit::Commit("before".into()))
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    let scene = client.scene().unwrap();
    let active_epoch = client.epoch();
    client.open(&ui.ui_ref, "editor:reload");
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount")
    };
    assert!(!client.hardware_edit(&Edit::Commit("lost".into())).unwrap());
    assert_eq!(client.typed_value(1).unwrap(), Value::Text("before".into()));
    assert!(
        client
            .receive(
                &encode_server(&ServerMessage::Mounted {
                    epoch,
                    ui: encode_ui(&ui).unwrap(),
                    ui_hash: vec![0; 32],
                    code: vec![],
                    state: authority.save().unwrap()
                })
                .unwrap()
            )
            .is_err()
    );
    assert_eq!(client.epoch(), active_epoch);
    assert!(std::sync::Arc::ptr_eq(&scene, &client.scene().unwrap()));
    assert!(client.keyboard_visible());
    assert!(
        client
            .hardware_edit(&Edit::Commit(" after".into()))
            .unwrap()
    );
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        authority.typed_value(1).unwrap(),
        Value::Text("before after".into())
    );
}
#[test]
fn admission_rejects_gesture_arity_and_numeric_operations_on_text() {
    for case in 0..3 {
        let mut ui = input_lab::ui();
        let h = &mut ui.handlers[0];
        match case {
            0 => {
                h.kind = GestureKind::Slider;
                h.operation = Operation::Back;
                h.fields.clear();
            }
            1 => {
                h.kind = GestureKind::Activate;
                h.operation = Operation::Set;
            }
            _ => {
                h.kind = GestureKind::Pad;
                h.operation = Operation::Set;
            }
        }
        let bytes = encode_ui(&ui).unwrap();
        let mut client = Client::default();
        client
            .admit_catalog(
                &encode_catalog(&Catalog {
                    format: "otrios-app-catalog-v1".into(),
                    apps: vec![CatalogEntry {
                        app: input_lab::manifest(),
                        ui_hash: hash(&bytes),
                        private: false,
                    }],
                })
                .unwrap(),
            )
            .unwrap();
        assert!(
            client
                .mount_local(&bytes, "app:input-lab/state", &[], &[])
                .is_err()
        );
        assert!(client.scene().is_err());
    }
}

#[test]
fn mount_decisions_precede_sync_and_rejected_candidate_retains_old_wire_epoch() {
    let code = code();
    let ui = playground::ui(&code);
    let bytes = encode_ui(&ui).unwrap();
    let mut client = Client::default();
    activate(&mut client, ui.clone(), "decisions", code.clone());
    let mut authority = Document::new(ui.state_schema, "decisions").unwrap();
    let mut session = Session::default();
    pump(&mut client, &mut authority, &mut session);
    let old_epoch = client.epoch();
    client.open(&client.ui_ref(), "decisions");
    let ClientMessage::Mount { epoch: request, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount")
    };
    assert!(
        client
            .receive(
                &encode_server(&ServerMessage::Mounted {
                    epoch: request,
                    ui: bytes.clone(),
                    ui_hash: vec![0; 32],
                    code: code.clone(),
                    state: authority.save().unwrap()
                })
                .unwrap()
            )
            .is_err()
    );
    assert!(client.rejected_mount_pending());
    assert_eq!(client.epoch(), old_epoch);
    edit(&mut client, "Linear slider", 870, 0);
    client.tick(3000, 0).unwrap();
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::CancelMount{epoch} if epoch==request)
    );
    let ClientMessage::Sync { epoch, bytes: sync } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Old-app Sync")
    };
    assert_eq!(epoch, old_epoch);
    authority.receive(&mut session, &sync).unwrap();
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(authority.value(1).unwrap(), 870);
    client.open(&client.ui_ref(), "decisions");
    let ClientMessage::Mount { epoch: request, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount")
    };
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch: request,
                ui_hash: hash(&bytes),
                ui: bytes,
                code,
                state: authority.save().unwrap(),
            })
            .unwrap(),
        )
        .unwrap();
    assert!(!client.confirmed());
    assert_eq!(client.epoch(), request);
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::AcceptMount{epoch} if epoch==request)
    );
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::Sync{epoch,..} if epoch==request)
    );
}

#[test]
fn old_private_lease_revocation_preserves_newer_public_mount() {
    let home = home::ui();
    let public = layout::ui();
    let mut client = Client::default();
    activate_access(&mut client, home.clone(), "private:home", vec![], true);
    let mut authority = Document::new(home.state_schema.clone(), "private:home").unwrap();
    pump(&mut client, &mut authority, &mut Session::default());
    let entries = [
        (home, "private:home", true),
        (public.clone(), "public:layout", false),
    ]
    .into_iter()
    .map(|(ui, state, private)| CatalogEntry {
        app: App {
            id: ui
                .ui_ref
                .strip_prefix("app:")
                .unwrap()
                .strip_suffix("/ui")
                .unwrap()
                .into(),
            title: ui.title.clone(),
            ui_ref: ui.ui_ref.clone(),
            state_ref: state.into(),
            status: "canvas-app".into(),
            dependencies: vec![],
        },
        ui_hash: hash(&encode_ui(&ui).unwrap()),
        private,
    })
    .collect();
    client
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: entries,
            })
            .unwrap(),
        )
        .unwrap();
    let old = client.epoch();
    client.open(&public.ui_ref, "public:layout");
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Public mount")
    };
    assert!(
        client
            .receive(&encode_server(&ServerMessage::AccessRequired { epoch: old }).unwrap())
            .unwrap()
    );
    assert!(client.ui_ref().is_empty());
    assert!(!client.auth_required());
    assert_eq!(client.pending_ui_ref(), public.ui_ref);
    let bytes = encode_ui(&public).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(client.ui_ref(), "app:layout-lab/ui");
    assert!(!client.private());
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::AcceptMount{epoch:e} if e==epoch)
    );
}

#[test]
fn aggregate_content_cannot_cover_app_navigation_or_header() {
    use otrios_interfaces::scene::Action;
    let mut c = Client::default();
    activate_access(
        &mut c,
        monitoring_app::ui(),
        "app:monitoring/state",
        vec![],
        true,
    );
    let feed = authoring::AppUi::new("feed", "Feed", authoring::schema("empty", &[]))
        .build("Private aggregate", vec![]);
    c.receive(
        &encode_server(&ServerMessage::View {
            epoch: c.epoch(),
            bytes: feed.layout,
        })
        .unwrap(),
    )
    .unwrap();
    let scene = c.scene().unwrap();
    let home = scene
        .hit_path([1195., 85.])
        .iter()
        .find_map(|n| {
            n.node
                .semantics
                .as_ref()
                .filter(|s| s.label == "Home")
                .map(|_| n.node.id)
        })
        .expect("Home remains tappable where the aggregate overlaps its lower edge");
    assert!(matches!(
        otrios_core::dashboard::input::InputRouter::activate(&scene, home).action,
        Some(Action::AppNavigate(_))
    ));
    let header=scene.root().children.iter().position(|n|matches!(&n.primitive,otrios_interfaces::scene::Primitive::Text{value,..} if value=="Monitoring")).unwrap();
    let content = scene
        .root()
        .children
        .iter()
        .position(|n| {
            matches!(
                n.primitive,
                otrios_interfaces::scene::Primitive::Viewport { .. }
            )
        })
        .unwrap();
    assert!(
        content < header,
        "content paints below the app-owned header"
    );
}

#[test]
fn rotary_pointer_and_indicator_share_clockwise_turns_and_accent_is_only_presentation() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, Primitive};
    let mut client = Client::default();
    activate(&mut client, playground::ui(&code()), "instruments", code());
    for (fraction, expected) in [
        (0., [96., 20.]),
        (250., [172., 96.]),
        (500., [96., 172.]),
        (750., [20., 96.]),
    ] {
        let angle = fraction * std::f32::consts::TAU / 1000.;
        let scene = client.scene().unwrap();
        let controls = scene.controls();
        let dial = controls
            .iter()
            .find(|n| {
                n.node
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.label == "Rotary dial")
            })
            .unwrap();
        let position = [
            dial.origin[0] + 105. + angle.sin() * 76.,
            dial.origin[1] + 105. - angle.cos() * 76.,
        ];
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            client
                .pointer(
                    client.epoch(),
                    PointerEvent {
                        id: 1,
                        phase,
                        position,
                    },
                )
                .unwrap();
        }
        assert_eq!(client.value(2).unwrap(), fraction as u16);
        let scene = client.scene().unwrap();
        let controls = scene.controls();
        let dial = controls
            .iter()
            .find(|n| {
                n.node
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.label == "Rotary dial")
            })
            .unwrap();
        let marker = dial
            .node
            .children
            .iter()
            .find(|n| {
                matches!(n.primitive, Primitive::Circle { color } if color == authoring::ACCENT)
                    && n.frame.width == 18.
            })
            .unwrap();
        assert!(
            (marker.frame.x - expected[0]).abs() < 0.01
                && (marker.frame.y - expected[1]).abs() < 0.01
        );
    }
    edit(&mut client, "Toggle", 0, 0);
    edit(&mut client, "Linear slider", 700, 0);
    assert_eq!(
        client.value(1).unwrap(),
        700,
        "muted accent does not disable the controls"
    );
    let scene = client.scene().unwrap();
    let controls = scene.controls();
    let slider = controls
        .iter()
        .find(|n| {
            n.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Linear slider")
        })
        .unwrap();
    assert!(slider.node.children.iter().any(
        |n| matches!(n.primitive, Primitive::Circle { color } if color == authoring::MUTED)
            && n.frame.width == 40.
    ));
}

#[test]
fn monitoring_session_controls_share_navigation_style_and_only_request_local_logout() {
    use otrios_interfaces::scene::Primitive;
    let ui = monitoring_app::ui();
    let mut c = Client::default();
    activate_access(&mut c, ui.clone(), "private:monitoring", vec![], true);
    let mut authority = Document::new(ui.state_schema, "private:monitoring").unwrap();
    pump(&mut c, &mut authority, &mut Session::default());
    let control = |s: &otrios_core::dashboard::scene::Scene, label: &str| {
        s.controls()
            .into_iter()
            .find(|n| n.node.semantics.as_ref().is_some_and(|s| s.label == label))
            .map(|n| n.node.clone())
    };
    assert!(
        control(&c.scene().unwrap(), "Sign out").is_none(),
        "trusted native mounts do not expose a browser cookie action"
    );
    assert!(c.set_session_controls(true));
    let scene = c.scene().unwrap();
    let home = control(&scene, "Home").unwrap();
    let logout = control(&scene, "Sign out").unwrap();
    assert_eq!(
        (logout.frame.y, logout.frame.width, logout.frame.height),
        (home.frame.y, home.frame.width, home.frame.height)
    );
    assert!(logout.frame.x + logout.frame.width + 20. <= home.frame.x);
    assert_eq!(logout.children[0].primitive, home.children[0].primitive);
    assert!(
        matches!((&logout.children[1].primitive, &home.children[1].primitive), (Primitive::Text { scale: a, color: ac, .. }, Primitive::Text { scale: b, color: bc, .. }) if a == b && ac == bc)
    );
    let before = authority.save().unwrap();
    assert!(c.activate(c.epoch(), logout.id).unwrap());
    assert!(c.take_sign_out());
    assert!(!c.take_sign_out(), "host actions are consumed once");
    assert!(
        c.outgoing().unwrap().is_none(),
        "sign out never emits CRDT writes"
    );
    assert_eq!(authority.save().unwrap(), before);
    c.activate(c.epoch(), logout.id).unwrap();
    c.set_session_controls(false);
    assert!(
        !c.take_sign_out(),
        "session revocation cancels pending local actions"
    );
    assert!(control(&c.scene().unwrap(), "Sign out").is_none());
}

#[test]
fn text_field_taps_place_caret_at_proportional_glyphs_instead_of_restoring_focus_memory() {
    use otrios_core::dashboard::font::Font;
    use otrios_interfaces::{
        keyboard::Edit,
        layout::Value,
        scene::{PointerEvent, PointerPhase, Primitive},
    };
    use std::sync::Arc;
    let font = Arc::new(
        Font::from_zstd(
            include_bytes!("../../../web/dashboard/fonts/Roboto-Regular.otq.zst"),
            otrios_core::dashboard::font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    );
    let ui = input_lab::ui();
    let mut authority = Document::new(ui.state_schema.clone(), "tapped-input").unwrap();
    authority
        .set_typed(&[
            (1, Value::Text("WiWWW".into())),
            (2, Value::Text("alphabet".into())),
        ])
        .unwrap();
    let mut c = Client::default();
    c.set_font(font.clone());
    activate_snapshot(
        &mut c,
        ui,
        "tapped-input",
        vec![],
        false,
        authority.save().unwrap(),
    );
    let mut session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    fn tap(c: &mut Client, label: &str, x: f32) {
        let scene = c.scene().unwrap();
        let controls = scene.controls();
        let field = controls
            .iter()
            .find(|n| n.node.semantics.as_ref().is_some_and(|s| s.label == label))
            .unwrap();
        let text = field
            .node
            .children
            .iter()
            .find(|n| matches!(n.primitive, Primitive::Text { .. }))
            .unwrap();
        let position = [field.origin[0] + text.frame.x + x, field.origin[1] + 35.];
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            c.pointer(
                c.epoch(),
                PointerEvent {
                    id: 23,
                    phase,
                    position,
                },
            )
            .unwrap();
        }
    }
    tap(&mut c, "First text field", font.text_width("Wi", 2.));
    assert!(
        c.outgoing().unwrap().is_none(),
        "caret placement emits no shared update"
    );
    c.hardware_edit(&Edit::Commit("!".into())).unwrap();
    assert_eq!(c.typed_value(1).unwrap(), Value::Text("Wi!WWW".into()));
    tap(&mut c, "Second text field", font.text_width("alph", 2.));
    c.hardware_edit(&Edit::Commit("?".into())).unwrap();
    assert_eq!(c.typed_value(2).unwrap(), Value::Text("alph?abet".into()));
    tap(&mut c, "First text field", font.text_width("W", 2.));
    c.hardware_edit(&Edit::Commit("#".into())).unwrap();
    assert_eq!(c.typed_value(1).unwrap(), Value::Text("W#i!WWW".into()));
    // Repeated taps account for the visible local caret marker's glyph width.
    tap(&mut c, "First text field", font.text_width("W#|i!", 2.));
    c.hardware_edit(&Edit::Commit("$".into())).unwrap();
    assert_eq!(c.typed_value(1).unwrap(), Value::Text("W#i!$WWW".into()));
    tap(&mut c, "First text field", -12.);
    c.hardware_edit(&Edit::Commit("<".into())).unwrap();
    tap(&mut c, "First text field", 1080.);
    c.hardware_edit(&Edit::Commit(">".into())).unwrap();
    assert_eq!(c.typed_value(1).unwrap(), Value::Text("<W#i!$WWW>".into()));
    pump(&mut c, &mut authority, &mut session);
    assert_eq!(
        authority.typed_value(1).unwrap(),
        Value::Text("<W#i!$WWW>".into())
    );
    assert!(!c.input_busy());
}

#[test]
fn text_hit_testing_preserves_graphemes_and_the_displayed_window_offset() {
    use otrios_core::dashboard::{font::Font, keyboard::TextBuffer};
    use otrios_interfaces::keyboard::Edit;
    let font = Font::from_zstd(
        include_bytes!("../../../web/dashboard/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut text = TextBuffer::default();
    text.replace("a\u{301}👩‍👩‍👧‍👦Wi");
    let boundary = "a\u{301}".len();
    let x = font.text_width("a\u{301}", 2.);
    text.place_cursor(text.hit_test(x, &font, 2., false));
    text.edit(&Edit::Commit("!".into()));
    assert_eq!(&text.text()[..boundary + 1], "a\u{301}!");
    let family = "a\u{301}!👩‍👩‍👧‍👦";
    text.place_cursor(text.hit_test(font.text_width(family, 2.), &font, 2., false));
    text.edit(&Edit::Commit("?".into()));
    assert_eq!(text.text(), "a\u{301}!👩‍👩‍👧‍👦?Wi");
    let long = "W".repeat(60);
    text.replace(&long);
    text.place_cursor(long.len());
    assert_eq!(text.display(false).len(), 36);
    text.place_cursor(text.hit_test(0., &font, 2., false));
    text.edit(&Edit::Commit("!".into()));
    assert_eq!(
        text.text(),
        format!("{}!{}", "W".repeat(24), "W".repeat(36))
    );
}

#[test]
fn reconnect_keeps_current_ui_editable_and_rebases_edits_made_while_offline() {
    let ui = playground::ui(&code());
    let mut c = Client::default();
    activate(&mut c, ui.clone(), "offline-input", code());
    let mut authority = Document::new(ui.state_schema, "offline-input").unwrap();
    let mut session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    c.reset_connection();
    let ClientMessage::Mount {
        epoch,
        writer,
        resume,
        ..
    } = decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("missing reconnect")
    };
    edit(&mut c, "Linear slider", 870, 0);
    c.tick(5000, 0).unwrap();
    assert_eq!(c.value(1).unwrap(), 870);
    assert!(!c.confirmed());
    assert_eq!(authority.value(1).unwrap(), 420);
    let (mut next, _) = authority.open_session(&writer, resume).unwrap();
    let state = authority.generate(&mut next).unwrap().unwrap();
    let bytes = encode_ui(&ui_for_cache()).unwrap();
    c.receive(
        &encode_server(&ServerMessage::Mounted {
            epoch,
            ui_hash: hash(&bytes),
            ui: bytes,
            code: vec![],
            state,
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(c.value(1).unwrap(), 870);
    pump(&mut c, &mut authority, &mut next);
    assert_eq!(authority.value(1).unwrap(), 870);
}

// Deliberately do not advance a host clock while delivering messages. These
// exercise dispatch semantics, rather than accidentally flushing via pump().
fn drain_sync(
    c: &mut Client,
    authority: &mut Document,
    session: &mut Session,
) -> Vec<StateRequest> {
    let mut requests = Vec::new();
    while let Some(bytes) = c.outgoing().unwrap() {
        if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes).unwrap() {
            requests.push(decode_state_request(&bytes).unwrap());
            authority.receive(session, &bytes).unwrap();
        }
    }
    requests
}
fn deliver_state(c: &mut Client, authority: &Document, session: &mut Session) {
    let bytes = authority.generate(session).unwrap().unwrap();
    c.receive(
        &encode_server(&ServerMessage::Sync {
            epoch: c.epoch(),
            bytes,
        })
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn continuous_feedback_survives_delayed_confirmation_and_final_release_bypasses_deadline() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    for phase in [PointerPhase::Up, PointerPhase::Cancel] {
        let code = code();
        let ui = playground::ui(&code);
        let mut c = Client::default();
        activate(&mut c, ui.clone(), "dispatch:gesture", code);
        let mut authority = Document::new(ui.state_schema, "dispatch:gesture").unwrap();
        let mut session = Session::default();
        pump(&mut c, &mut authority, &mut session);
        c.set_dispatch_policy(DispatchPolicy::new(1000, MAX_FIELDS).unwrap());
        c.tick(2000, 0).unwrap();
        let scene = c.scene().unwrap();
        let controls = scene.controls();
        let slider = controls
            .iter()
            .find(|n| n.node.id == node(&c, "Linear slider"))
            .unwrap();
        let position = |x| {
            [
                slider.origin[0] + slider.node.frame.width * x,
                slider.origin[1] + 40.,
            ]
        };
        c.pointer(
            c.epoch(),
            PointerEvent {
                id: 91,
                phase: PointerPhase::Down,
                position: position(0.2),
            },
        )
        .unwrap();
        assert!(
            drain_sync(&mut c, &mut authority, &mut session)
                .iter()
                .any(|r| matches!(r, StateRequest::Write { .. }))
        );
        let issued_value = authority.value(1).unwrap();
        c.tick(2001, 0).unwrap();
        c.pointer(
            c.epoch(),
            PointerEvent {
                id: 91,
                phase: PointerPhase::Move,
                position: position(0.8),
            },
        )
        .unwrap();
        assert_ne!(c.value(1).unwrap(), issued_value);
        assert!(text_contains(c.scene().unwrap().root(), "80%"));
        assert!(
            drain_sync(&mut c, &mut authority, &mut session).is_empty(),
            "one immutable issued write"
        );
        assert_eq!(c.dispatch_usage().pending_fields, 1);
        assert_eq!(c.dispatch_usage().issued_fields, 1);
        deliver_state(&mut c, &authority, &mut session);
        let requests = drain_sync(&mut c, &mut authority, &mut session);
        assert!(
            requests
                .iter()
                .all(|r| matches!(r, StateRequest::Ack { .. }))
        );
        assert_eq!(
            c.value(1).unwrap(),
            800,
            "old confirmation cannot erase local feedback"
        );
        assert_eq!(c.dispatch_usage().ready_in_ms, Some(999));
        c.pointer(
            c.epoch(),
            PointerEvent {
                id: 91,
                phase,
                position: position(1.4),
            },
        )
        .unwrap();
        let final_value = c.value(1).unwrap();
        assert_eq!(c.dispatch_usage().ready_in_ms, Some(0));
        let requests = drain_sync(&mut c, &mut authority, &mut session);
        assert!(
            requests
                .iter()
                .any(|r| matches!(r, StateRequest::Write { .. }))
        );
        assert_eq!(authority.value(1).unwrap(), final_value);
        deliver_state(&mut c, &authority, &mut session);
        drain_sync(&mut c, &mut authority, &mut session);
        assert!(c.confirmed());
        assert_eq!(c.dispatch_usage(), DispatchUsage::default());
    }
}

#[test]
fn discrete_assignment_and_text_do_not_wait_for_continuous_dispatch_interval() {
    let code = code();
    let ui = playground::ui(&code);
    let mut c = Client::default();
    activate(&mut c, ui.clone(), "dispatch:discrete", code);
    let mut authority = Document::new(ui.state_schema, "dispatch:discrete").unwrap();
    let mut session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    c.set_dispatch_policy(DispatchPolicy::new(1000, MAX_FIELDS).unwrap());
    c.tick(2000, 0).unwrap();
    edit(&mut c, "Linear slider", 100, 0);
    drain_sync(&mut c, &mut authority, &mut session);
    deliver_state(&mut c, &authority, &mut session);
    drain_sync(&mut c, &mut authority, &mut session);
    c.tick(2001, 0).unwrap();
    edit(&mut c, "Linear slider", 900, 0);
    assert_eq!(c.dispatch_usage().ready_in_ms, Some(999));
    edit(&mut c, "Toggle", 0, 0);
    assert_eq!(c.dispatch_usage().ready_in_ms, Some(0));
    let requests = drain_sync(&mut c, &mut authority, &mut session);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, StateRequest::Write { values, .. } if values.len() == 2))
    );
    assert_eq!(authority.value(1).unwrap(), 900);

    let ui = input_lab::ui();
    let mut text = Client::default();
    activate(&mut text, ui.clone(), "dispatch:text", vec![]);
    let mut doc = Document::new(ui.state_schema, "dispatch:text").unwrap();
    let mut session = Session::default();
    pump(&mut text, &mut doc, &mut session);
    text.set_dispatch_policy(DispatchPolicy::new(1000, MAX_FIELDS).unwrap());
    text.tick(2000, 0).unwrap();
    text.activate(text.epoch(), node(&text, "First text field"))
        .unwrap();
    text.hardware_edit(&otrios_interfaces::keyboard::Edit::Commit("a".into()))
        .unwrap();
    drain_sync(&mut text, &mut doc, &mut session);
    deliver_state(&mut text, &doc, &mut session);
    drain_sync(&mut text, &mut doc, &mut session);
    text.tick(2001, 0).unwrap();
    text.hardware_edit(&otrios_interfaces::keyboard::Edit::Commit("b".into()))
        .unwrap();
    assert_eq!(text.dispatch_usage().ready_in_ms, Some(0));
    drain_sync(&mut text, &mut doc, &mut session);
    assert_eq!(
        doc.typed_value(1).unwrap(),
        otrios_interfaces::layout::Value::Text("ab".into())
    );
}
