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

#[test]
fn portrait_compositions_reuse_controls_and_keep_old_snapshots_immutable() {
    use std::sync::Arc;
    let behavior = code();
    for (ui, code) in [
        (home::ui(&[]), vec![]),
        (layout::ui(&[]), vec![]),
        (playground::ui(&behavior), behavior),
    ] {
        let mut client = Client::default();
        activate(&mut client, ui, "responsive", code);
        let wide = client.scene().unwrap();
        let controls = |scene: &otrios_core::dashboard::scene::Scene| {
            scene
                .controls()
                .iter()
                .map(|c| (c.node.id, c.node.responder))
                .collect::<Vec<_>>()
        };
        let before = controls(&wide);
        let epoch = client.epoch();
        assert!(client.set_viewport([800., 1280.]).unwrap());
        let tall = client.scene().unwrap();
        assert_eq!(tall.root().frame.width, 800.);
        assert_eq!(tall.root().frame.height, 1280.);
        assert_eq!(
            controls(&tall),
            before,
            "placement cannot reassign control behavior"
        );
        assert_eq!(client.epoch(), epoch, "resizing is not a new state mount");
        assert_eq!(wide.root().frame.width, 1280.);
        for c in tall.controls() {
            assert!(c.origin[0] >= 0. && c.origin[1] >= 0.);
            assert!(c.origin[0] + c.node.frame.width * c.scale <= 800.1);
            assert!(c.origin[1] + c.node.frame.height * c.scale <= 1280.1);
        }
        assert!(client.set_viewport([100., 100.]).is_err());
        assert!(
            Arc::ptr_eq(&tall, &client.scene().unwrap()),
            "rejected geometry leaves current scene intact"
        );
        client.set_viewport([1280., 800.]).unwrap();
        assert_eq!(
            client.scene().unwrap().root(),
            wide.root(),
            "layout round trip never accumulates drift"
        );
    }
}

#[test]
fn different_viewports_share_values_and_rotation_cancels_capture_without_losing_edits() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let behavior = code();
    let ui = playground::ui(&behavior);
    let mut a = Client::default();
    let mut b = Client::default();
    b.set_viewport([800., 1280.]).unwrap();
    activate(&mut a, ui.clone(), "orientation:shared", behavior.clone());
    activate(&mut b, ui.clone(), "orientation:shared", behavior);
    let mut authority = Document::new(ui.state_schema, "orientation:shared").unwrap();
    let mut sa = Session::default();
    let mut sb = Session::default();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    edit(&mut a, "Linear slider", 780, 0);
    let scene = a.scene().unwrap();
    let dial = scene
        .controls()
        .into_iter()
        .find(|c| {
            c.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Rotary dial")
        })
        .unwrap();
    a.pointer(
        a.epoch(),
        PointerEvent {
            id: 1,
            phase: PointerPhase::Down,
            position: [dial.origin[0] + 105., dial.origin[1] + 40.],
        },
    )
    .unwrap();
    assert!(a.input_busy());
    let value = a.value(1).unwrap();
    a.set_viewport([800., 1280.]).unwrap();
    assert!(!a.input_busy());
    assert_eq!(a.value(1).unwrap(), value);
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    assert_eq!(b.value(1).unwrap(), value);
    b.set_viewport([1280., 800.]).unwrap();
    assert_eq!(b.value(1).unwrap(), value);
    assert!(a.confirmed() && b.confirmed());
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
                path: ui
                    .ui_ref
                    .trim_start_matches("app:")
                    .trim_end_matches("/ui")
                    .into(),
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
    assert_eq!(a.control_value(slider).unwrap(), Some(80.));
    pump(&mut a, &mut da, &mut sa);
    assert_eq!(a.value(1).unwrap(), 800);
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
    assert_eq!(slider.node.children.iter().find(|n|matches!(n.primitive,otrios_interfaces::scene::Primitive::Superellipse{color,..} if color==authoring::ACCENT)).unwrap().frame.width,800.*0.281+1.);
    fn has_text(n: &otrios_interfaces::scene::SceneNode, value: &str) -> bool {
        matches!(&n.primitive,otrios_interfaces::scene::Primitive::Text{value:v,..} if v==value)
            || n.children.iter().any(|c| has_text(c, value))
    }
    assert!(has_text(s.root(), "80%"));
    let old = a.epoch();
    activate(&mut a, layout::ui(&[]), "layout:state", vec![]);
    assert!(!a.event(old, 1, 0, 0).unwrap());
    assert_eq!(a.value(1).unwrap(), 500);
    let mut dl = Document::new(layout::ui(&[]).state_schema, "layout:state").unwrap();
    pump(&mut a, &mut dl, &mut Session::default());
    activate(&mut a, ui_for_cache(), "room:a", vec![]);
    assert_eq!(a.value(1).unwrap(), 420); // Fresh RAM replica, not a persisted local checkpoint.
    pump(&mut a, &mut da, &mut Session::default());
    assert_eq!(a.value(1).unwrap(), 800);
}
fn ui_for_cache() -> Ui {
    playground::ui(&code())
}
#[test]
fn wasm_reducer_and_declarative_app_are_qualified_at_extremes() {
    for (ui, code) in [
        (playground::ui(&code()), code()),
        (layout::ui(&[]), vec![]),
        (home::ui(&[]), vec![]),
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
    assert_eq!(home::manifest().state_ref, "app:home/state-v3");
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
    activate(&mut c, home::ui(&[]), "home", vec![]);
    let mut d = Document::new(home::ui(&[]).state_schema, "home").unwrap();
    pump(&mut c, &mut d, &mut Session::default());
    c.open("app:layout-lab/ui", "layout");
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("missing mount")
    };
    let b = encode_ui(&layout::ui(&[])).unwrap();
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
            ("app:component-playground/ui".into(), String::new())
        );
        assert_eq!(
            navigation(&ui, "Layout"),
            ("app:layout-lab/ui".into(), String::new())
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
        encode_ui(&home::ui(&[])).unwrap(),
        encode_ui(&home::ui(&[])).unwrap(),
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
    let ui = layout::ui(&[]);
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
    let origin = wide.path(wide_card.id).unwrap().last().unwrap().origin;
    let point = [
        origin[0] + wide_card.frame.width - 40.,
        origin[1] + wide_card.frame.height - 48.,
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
fn resize_rules_require_a_versioned_layout_and_component_root_owns_its_size() {
    use otrios_core::dashboard::layout::Layout;
    let ui = layout::ui(&[]);
    let parsed = Layout::from_bytes(&ui.layout).unwrap();
    assert_eq!(parsed.blueprint().schema, "otrios-layout-v3");
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
fn app_scope_rejects_missing_slots_and_unbound_responder_domains() {
    use otrios_core::dashboard::layout::Layout;
    use otrios_interfaces::layout::{Binding, Expr, Responder, Shape};
    let ui = home::ui(&[]);
    let mut blueprint = Layout::from_bytes(&ui.layout).unwrap().blueprint().clone();
    let heading = blueprint.templates[0]
        .root
        .children
        .iter_mut()
        .find(|n| matches!(n.shape, Shape::Text { .. }))
        .unwrap();
    if let Shape::Text { value, .. } = &mut heading.shape {
        *value = Expr::State(Binding::Local(65000));
    }
    let bytes = Layout::encode(&blueprint).unwrap();
    let err = Layout::from_bytes(&bytes)
        .unwrap()
        .resolve_slots(&std::collections::BTreeMap::new())
        .unwrap_err();
    assert!(err.contains("Missing local binding"));
    // Even an unlabeled/hidden unbound responder is rejected, rather than ignored
    // or interpreted as a valid event with the same numeric index.
    for responder in [
        Responder::AppEvent {
            control: 127,
            drag: false,
        },
        Responder::AppNavigate(250),
    ] {
        let mut bad = home::ui(&[]);
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
                    path: "home".into(),
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
    let aui = animation_lab::ui(&[]);
    let tui = three_d_lab::ui(&[]);
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
    let ui = input_lab::ui(&[]);
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
    let ui = home::ui(&[]);
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
        ("app:home/ui", "private")
    );
}

#[test]
fn all_seven_app_packages_qualify_default_and_scalar_extremes() {
    use otrios_interfaces::layout::Value;
    let code = code();
    for (ui, code, private) in [
        (home::ui(&[]), vec![], false),
        (playground::ui(&code), code, false),
        (layout::ui(&[]), vec![], false),
        (input_lab::ui(&[]), vec![], false),
        (animation_lab::ui(&[]), vec![], false),
        (three_d_lab::ui(&[]), vec![], false),
        (monitoring_app::ui(&[]), vec![], true),
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
    assert!(
        fresh.outgoing().unwrap().is_none(),
        "catalog must be admitted before mounting"
    );
    let ui = playground::ui(&code());
    fresh
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    path: ui
                        .ui_ref
                        .trim_start_matches("app:")
                        .trim_end_matches("/ui")
                        .into(),
                    app: App {
                        id: "component-playground".into(),
                        title: ui.title.clone(),
                        ui_ref: ui.ui_ref.clone(),
                        state_ref: "fresh".into(),
                        status: "canvas-app".into(),
                        dependencies: vec![],
                    },
                    ui_hash: hash(&encode_ui(&ui).unwrap()),
                    private: false,
                }],
            })
            .unwrap(),
        )
        .unwrap();
    assert!(fresh.outgoing().unwrap().is_some());
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
    assert_eq!(client.value(1).unwrap(), 800);
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
        800,
        "Unsent preview vanished during reconnect activation"
    );
    pump(&mut client, &mut authority, &mut Session::default());
    assert_eq!(authority.value(1).unwrap(), 800);
}

#[test]
fn staged_reload_freezes_old_editor_and_rejected_candidate_leaves_it_usable() {
    use otrios_interfaces::{keyboard::Edit, layout::Value};
    let ui = input_lab::ui(&[]);
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
        let mut ui = input_lab::ui(&[]);
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
                        path: ui
                            .ui_ref
                            .trim_start_matches("app:")
                            .trim_end_matches("/ui")
                            .into(),
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
    assert_eq!(authority.value(1).unwrap(), 800);
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
    let home = home::ui(&[]);
    let public = layout::ui(&[]);
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
        path: ui
            .ui_ref
            .trim_start_matches("app:")
            .trim_end_matches("/ui")
            .into(),
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
        monitoring_app::ui(&[]),
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
    let ui = monitoring_app::ui(&[]);
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
        scene::{PointerEvent, PointerPhase},
    };
    use std::sync::Arc;
    let font = Arc::new(
        Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            otrios_core::dashboard::font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    );
    let ui = input_lab::ui(&[]);
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
            .find(|n| !n.children.is_empty())
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
fn text_hit_testing_preserves_graphemes_and_pixel_window_offsets() {
    use otrios_core::dashboard::{font::Font, keyboard::TextBuffer};
    use otrios_interfaces::keyboard::Edit;
    let font = Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
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
    // The complete run fills wide fields; no fixed character-count window.
    assert_eq!(text.display(false).len(), 60);
    let offset = font.text_width(&"W".repeat(24), 2.);
    let (window, x) = text.window(offset + 0.5, 300., &font, 2., false);
    assert!(window.len() > 10);
    assert!(x < 0. && x > -font.text_width("W", 2.));
    text.place_cursor(text.hit_test(offset, &font, 2., false));
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
    assert_eq!(c.value(1).unwrap(), 800);
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
    assert_eq!(c.value(1).unwrap(), 800);
    pump(&mut c, &mut authority, &mut next);
    assert_eq!(authority.value(1).unwrap(), 800);
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

    let ui = input_lab::ui(&[]);
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

#[test]
fn admitted_ui_coverage_hides_unused_values_and_other_receipts_but_confirms_own_edits() {
    use otrios_interfaces::layout::Value;
    let code = code();
    let mut ui = playground::ui(&code);
    ui.state_schema.fields.push(Field {
        id: 6,
        name: "unused".into(),
        default: Value::Text("default".into()),
        writable: true,
    });
    let selection = StateSelection::from_ui(&ui).unwrap();
    let mut authority = Document::new(ui.state_schema.clone(), "selection:ui").unwrap();
    authority
        .set_typed(&[(6, Value::Text("not-in-this-view".into()))])
        .unwrap();
    let mut c = Client::default();
    activate(&mut c, ui, "selection:ui", code);
    assert!(
        c.typed_value(6).is_err(),
        "an unrequested value cannot appear as a schema default"
    );
    let mut session = Session::default();
    drain_sync(&mut c, &mut authority, &mut session);
    session.set_selection(selection.clone()).unwrap();
    deliver_state(&mut c, &authority, &mut session);
    drain_sync(&mut c, &mut authority, &mut session);
    assert!(c.confirmed());
    let old_scene = c.scene().unwrap();
    assert!(
        session.set_selection(selection).is_err(),
        "coverage is fixed for a lease"
    );
    authority
        .set_typed(&[(6, Value::Text("changed-but-unrequested".into()))])
        .unwrap();
    assert!(authority.generate(&mut session).unwrap().is_none());
    let (_other, _) = authority.open_session(&[99; 16], 0).unwrap();
    assert!(
        authority.generate(&mut session).unwrap().is_none(),
        "other writer admission is outside coverage"
    );
    assert!(std::sync::Arc::ptr_eq(&old_scene, &c.scene().unwrap()));
    edit(&mut c, "Linear slider", 860, 0);
    c.cancel().unwrap();
    drain_sync(&mut c, &mut authority, &mut session);
    deliver_state(&mut c, &authority, &mut session);
    drain_sync(&mut c, &mut authority, &mut session);
    assert!(c.confirmed());
    assert_eq!(authority.value(1).unwrap(), 800);
    assert_eq!(c.value(1).unwrap(), 800);
    assert!(c.typed_value(6).is_err());
    assert_eq!(
        authority.typed_value(6).unwrap(),
        Value::Text("changed-but-unrequested".into())
    );
}

#[test]
fn retired_snapshot_override_cannot_create_a_second_source_of_app_state() {
    #[derive(serde::Serialize)]
    enum Retired {
        Snapshot {
            epoch: u32,
            values: Vec<(u16, otrios_interfaces::layout::Value)>,
        },
    }
    let code = code();
    let ui = playground::ui(&code);
    let mut c = Client::default();
    activate(&mut c, ui, "retired:snapshot", code);
    let scene = c.scene().unwrap();
    let before = c.value(1).unwrap();
    let bytes = flexbuffers::to_vec(Retired::Snapshot {
        epoch: c.epoch(),
        values: vec![(1, otrios_interfaces::layout::Value::Number(999.))],
    })
    .unwrap();
    assert!(c.receive(&bytes).is_err());
    assert_eq!(c.value(1).unwrap(), before);
    assert!(std::sync::Arc::ptr_eq(&scene, &c.scene().unwrap()));
}

fn ui_entry(ui: &Ui, state: &str) -> CatalogEntry {
    CatalogEntry {
        path: ui
            .ui_ref
            .trim_start_matches("app:")
            .trim_end_matches("/ui")
            .into(),
        app: App {
            id: ui
                .ui_ref
                .trim_start_matches("app:")
                .trim_end_matches("/ui")
                .into(),
            title: ui.title.clone(),
            ui_ref: ui.ui_ref.clone(),
            state_ref: state.into(),
            status: "canvas-app".into(),
            dependencies: vec![],
        },
        ui_hash: hash(&encode_ui(ui).unwrap()),
        private: false,
    }
}
fn home_catalog(entry: CatalogEntry) -> Vec<u8> {
    encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![entry],
    })
    .unwrap()
}
#[test]
fn home_ui_replacement_retains_state_and_old_scene_and_rejects_stale_input() {
    use otrios_interfaces::layout::Value;
    let mut old = home::ui(&[]);
    old.state_schema.fields.push(Field {
        id: 7,
        name: "retained".into(),
        default: Value::Number(0.),
        writable: true,
    });
    old.selectors.push(Selector {
        local: 500,
        field: 7,
        projection: Projection::Identity,
    });
    let mut authority = Document::new(old.state_schema.clone(), "scope").unwrap();
    authority.set(&[(7, 731)]).unwrap();
    let state = authority.save().unwrap();
    let mut client = Client::default();
    activate_snapshot(
        &mut client,
        old.clone(),
        "scope",
        vec![],
        false,
        state.clone(),
    );
    pump(&mut client, &mut authority, &mut Session::default());
    let scene = client.scene().unwrap();
    let old_epoch = client.epoch();
    let mut next = old.clone();
    next.title = "Home updated".into();
    let bytes = encode_ui(&next).unwrap();
    let mut entry = ui_entry(&old, "scope");
    entry.ui_hash = hash(&bytes);
    assert!(client.update_catalog(&home_catalog(entry)).unwrap());
    assert!(client.catalog_update_pending());
    assert_eq!(client.title(), old.title);
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui: bytes.clone(),
                ui_hash: hash(&bytes),
                code: vec![],
                state,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(!client.catalog_update_pending());
    assert_eq!(client.title(), "Home updated");
    assert_eq!(client.value(7).unwrap(), 731);
    assert!(!client.event(old_epoch, 0, 0, 0).unwrap());
    assert_eq!(scene.root(), client.scene().unwrap().root());
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::AcceptMount{epoch:e}if e==epoch)
    );
}
#[test]
fn rejected_home_update_restores_catalog_and_retains_usable_scene() {
    use std::sync::Arc;
    let old = home::ui(&[]);
    let mut c = Client::default();
    activate(&mut c, old.clone(), "scope", vec![]);
    let mut authority = Document::new(old.state_schema.clone(), "scope").unwrap();
    pump(&mut c, &mut authority, &mut Session::default());
    let scene = c.scene().unwrap();
    let original = c.catalog_hash().to_vec();
    let mut next = old.clone();
    next.state_schema.id = "incompatible".into();
    let bytes = encode_ui(&next).unwrap();
    let mut entry = ui_entry(&old, "scope");
    entry.ui_hash = hash(&bytes);
    c.update_catalog(&home_catalog(entry)).unwrap();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    assert!(
        c.receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: vec![]
            })
            .unwrap()
        )
        .is_err()
    );
    assert_eq!(c.catalog_hash(), original);
    assert!(!c.catalog_update_pending());
    assert!(Arc::ptr_eq(&scene, &c.scene().unwrap()));
    assert!(
        matches!(decode_client(&c.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::CancelMount{epoch:e}if e==epoch)
    );
}
#[test]
fn inactive_ui_update_retains_the_other_app_mount_and_rejects_policy_changes() {
    use std::sync::Arc;
    let ui = layout::ui(&[]);
    let mut c = Client::default();
    activate(&mut c, ui.clone(), "other", vec![]);
    let mut authority = Document::new(ui.state_schema.clone(), "other").unwrap();
    pump(&mut c, &mut authority, &mut Session::default());
    let old_home = ui_entry(&home::ui(&[]), "home");
    let other = ui_entry(&ui, "other");
    let catalog = |home, other| {
        encode_catalog(&Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: vec![home, other],
        })
        .unwrap()
    };
    c.admit_catalog(&catalog(old_home.clone(), other.clone()))
        .unwrap();
    let scene = c.scene().unwrap();
    let epoch = c.epoch();
    let mut next = old_home;
    next.ui_hash = vec![7; 32];
    assert!(
        !c.update_catalog(&catalog(next.clone(), other.clone()))
            .unwrap()
    );
    assert!(Arc::ptr_eq(&scene, &c.scene().unwrap()));
    assert_eq!(c.epoch(), epoch);
    assert!(!c.catalog_update_pending());
    let mut changed = other;
    changed.private = true;
    assert!(c.update_catalog(&catalog(next, changed)).is_err());
}

#[test]
fn every_app_uses_the_same_idempotent_ui_replacement_contract() {
    use std::sync::Arc;
    let behavior = code();
    for (old, code) in [
        (home::ui(&[]), vec![]),
        (layout::ui(&[]), vec![]),
        (input_lab::ui(&[]), vec![]),
        (animation_lab::ui(&[]), vec![]),
        (three_d_lab::ui(&[]), vec![]),
        (monitoring_app::ui(&[]), vec![]),
        (playground::ui(&behavior), behavior),
    ] {
        let mut authority = Document::new(old.state_schema.clone(), "scope").unwrap();
        let mut c = Client::default();
        activate(&mut c, old.clone(), "scope", code.clone());
        pump(&mut c, &mut authority, &mut Session::default());
        let values: Vec<_> = old
            .state_schema
            .fields
            .iter()
            .filter_map(|f| c.typed_value(f.id).ok().map(|v| (f.id, v)))
            .collect();
        let mut next = old.clone();
        next.title.push_str(" revised");
        let bytes = encode_ui(&next).unwrap();
        let mut entry = ui_entry(&old, "scope");
        entry.ui_hash = hash(&bytes);
        let catalog = home_catalog(entry);
        let epoch = c.epoch();
        assert!(c.update_catalog(&catalog).unwrap());
        let ClientMessage::Mount {
            epoch: new_epoch, ..
        } = decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("mount")
        };
        c.receive(
            &encode_server(&ServerMessage::Mounted {
                epoch: new_epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code,
                state: authority.save().unwrap(),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(c.epoch() > epoch);
        assert_eq!(c.title(), next.title);
        for (id, value) in values {
            assert_eq!(c.typed_value(id).unwrap(), value);
        }
        let scene = c.scene().unwrap();
        let active_epoch = c.epoch();
        assert!(
            !c.update_catalog(&catalog).unwrap(),
            "duplicate publication must be a no-op even before ACK"
        );
        assert_eq!(c.epoch(), active_epoch);
        assert!(Arc::ptr_eq(&scene, &c.scene().unwrap()));
    }
}

fn revision_guest(revision: u32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
      (global $count (mut i32) (i32.const 0))
      (func (export "revision") (result i32) i32.const {revision})
      (func (export "abi_version") (result i32) i32.const 1)
      (func (export "dispatch") (param i32 i32 i32 i32) (result i64)
        global.get $count i32.const 1 i32.add global.set $count
        global.get $count i64.extend_i32_u i64.const 1125899906842624 i64.or))"#
    ))
    .unwrap()
}
#[test]
fn behavior_replacement_preserves_document_replaces_guest_ram_and_duplicate_is_noop() {
    let before = revision_guest(1);
    let after = revision_guest(2);
    let old = playground::ui(&before);
    let next = playground::ui(&after);
    compatible_behavior(&old, &next).unwrap();
    assert!(compatible_ui(&old, &next).is_err());
    let mut authority = Document::new(old.state_schema.clone(), "scope").unwrap();
    let mut c = Client::default();
    activate(&mut c, old.clone(), "scope", before.clone());
    let mut session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    edit(&mut c, "Linear slider", 100, 0);
    edit(&mut c, "Linear slider", 100, 0);
    pump(&mut c, &mut authority, &mut session);
    assert_eq!(c.value(1).unwrap(), 2);
    assert_eq!(c.resource_usage().behavior_hash, Some(hex(&hash(&before))));
    let old_scene = c.scene().unwrap();
    let old_epoch = c.epoch();
    let bytes = encode_ui(&next).unwrap();
    let mut entry = ui_entry(&old, "scope");
    entry.ui_hash = hash(&bytes);
    let catalog = home_catalog(entry);
    c.update_catalog(&catalog).unwrap();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    c.receive(
        &encode_server(&ServerMessage::Mounted {
            epoch,
            ui_hash: hash(&bytes),
            ui: bytes,
            code: after.clone(),
            state: authority.save().unwrap(),
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        c.value(1).unwrap(),
        2,
        "mounting code must not execute an app edit"
    );
    assert_eq!(old_scene.root(), c.scene().unwrap().root());
    assert!(!c.event(old_epoch, 1, 100, 0).unwrap());
    assert_eq!(c.resource_usage().behavior_hash, Some(hex(&hash(&after))));
    c.outgoing().unwrap();
    session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    edit(&mut c, "Linear slider", 100, 0);
    assert_eq!(c.value(1).unwrap(), 1, "replacement has fresh guest memory");
    assert!(
        !c.update_catalog(&catalog).unwrap(),
        "duplicate catalog is a no-op during pending edits too"
    );
    edit(&mut c, "Linear slider", 100, 0);
    assert_eq!(
        c.value(1).unwrap(),
        2,
        "duplicate cannot reset the active guest"
    );
    c.cancel().unwrap(); // Finish the synthetic continuous gesture before admission.
    pump(&mut c, &mut authority, &mut session);
    let mut ui_only = next.clone();
    ui_only.title.push_str(" UI-only");
    let bytes = encode_ui(&ui_only).unwrap();
    let mut entry = ui_entry(&old, "scope");
    entry.ui_hash = hash(&bytes);
    c.update_catalog(&home_catalog(entry)).unwrap();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    c.receive(
        &encode_server(&ServerMessage::Mounted {
            epoch,
            ui_hash: hash(&bytes),
            ui: bytes,
            code: vec![],
            state: authority.save().unwrap(),
        })
        .unwrap(),
    )
    .unwrap();
    c.outgoing().unwrap();
    session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    edit(&mut c, "Linear slider", 100, 0);
    assert_eq!(
        c.value(1).unwrap(),
        3,
        "a later UI-only update retains the replacement guest's memory"
    );
}
#[test]
fn invalid_behavior_replacement_retains_active_guest_and_permissions() {
    use std::sync::Arc;
    let before = revision_guest(1);
    let old = playground::ui(&before);
    let mut c = Client::default();
    activate(&mut c, old.clone(), "scope", before.clone());
    let mut authority = Document::new(old.state_schema.clone(), "scope").unwrap();
    pump(&mut c, &mut authority, &mut Session::default());
    let scene = c.scene().unwrap();
    let catalog_before = c.catalog_hash().to_vec();
    let bad = wat::parse_str("(module (func $start unreachable) (start $start))").unwrap();
    let next = playground::ui(&bad);
    let bytes = encode_ui(&next).unwrap();
    let mut entry = ui_entry(&old, "scope");
    entry.ui_hash = hash(&bytes);
    c.update_catalog(&home_catalog(entry)).unwrap();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    assert!(
        c.receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: bad,
                state: authority.save().unwrap()
            })
            .unwrap()
        )
        .is_err()
    );
    assert!(Arc::ptr_eq(&scene, &c.scene().unwrap()));
    assert_eq!(c.catalog_hash(), catalog_before);
    assert_eq!(c.resource_usage().behavior_hash, Some(hex(&hash(&before))));
    let mut permissions = playground::ui(&revision_guest(2));
    permissions.behavior.as_mut().unwrap().writes = vec![1];
    assert!(compatible_app_update(&old, &permissions).is_err());
    let mut changed = playground::ui(&revision_guest(2));
    changed.title.push_str(" changed");
    assert!(compatible_app_update(&old, &changed).is_err());
}

fn home_cards(client: &Client) -> Vec<(String, u32, [f32; 2])> {
    let scene = client.scene().unwrap();
    let mut cards: Vec<_> = scene
        .controls()
        .into_iter()
        .filter(|c| {
            matches!(
                c.node.responder,
                Some(otrios_interfaces::scene::TouchResponder::Activate(
                    otrios_interfaces::scene::Action::AppNavigate(_)
                ))
            )
        })
        .map(|c| {
            (
                c.node.semantics.as_ref().unwrap().label.clone(),
                c.node.id,
                [
                    c.origin[0] + c.node.frame.width / 2.,
                    c.origin[1] + c.node.frame.height / 2.,
                ],
            )
        })
        .collect();
    cards.sort_by(|a, b| a.2[1].total_cmp(&b.2[1]).then(a.2[0].total_cmp(&b.2[0])));
    cards
}
fn home_values(client: &Client) -> Vec<otrios_interfaces::layout::Value> {
    (1..=6).map(|id| client.typed_value(id).unwrap()).collect()
}
#[test]
fn home_drag_shares_only_one_position_and_remote_reflow_keeps_editing_local() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let ui = home::ui(&[]);
    let mut a = Client::default();
    let mut b = Client::default();
    activate(&mut a, ui.clone(), "home-edit", vec![]);
    activate(&mut b, ui.clone(), "home-edit", vec![]);
    b.set_viewport([800., 1280.]).unwrap();
    let mut authority = Document::new(ui.state_schema, "home-edit").unwrap();
    let mut sa = Session::default();
    let mut sb = Session::default();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    let initial = home_values(&a);
    let root = authority.root_hash();
    let cards = home_cards(&a);
    let before = b.reflow_frame().unwrap().unwrap();
    edit(&mut a, "Edit cards", 0, 0);
    assert!(a.reflow_frame().unwrap().unwrap().editing);
    assert_eq!(a.pending_count(), 0);
    assert_eq!(home_values(&a), initial);
    assert!(!b.reflow_frame().unwrap().unwrap().editing);
    let epoch = a.epoch();
    for (phase, position) in [
        (PointerPhase::Down, cards[5].2),
        (PointerPhase::Move, cards[1].2),
    ] {
        a.pointer(
            epoch,
            PointerEvent {
                id: 7,
                phase,
                position,
            },
        )
        .unwrap();
    }
    assert!(a.input_busy());
    assert_eq!(a.pending_count(), 1);
    let moved = home_values(&a);
    assert_eq!(moved[..5], initial[..5]);
    assert_ne!(moved[5], initial[5]);
    for _ in 0..20 {
        a.pointer(
            epoch,
            PointerEvent {
                id: 7,
                phase: PointerPhase::Move,
                position: cards[1].2,
            },
        )
        .unwrap();
    }
    assert_eq!(a.pending_count(), 1);
    assert_eq!(home_values(&a), moved);
    a.pointer(
        epoch,
        PointerEvent {
            id: 7,
            phase: PointerPhase::Cancel,
            position: cards[1].2,
        },
    )
    .unwrap();
    assert!(!a.input_busy());
    assert_eq!(a.epoch(), epoch);
    assert_eq!(a.ui_ref(), "app:home/ui");
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    assert_ne!(authority.root_hash(), root);
    assert_eq!(home_values(&b), moved);
    assert_eq!(
        home_cards(&a).iter().map(|v| &v.0).collect::<Vec<_>>(),
        home_cards(&b).iter().map(|v| &v.0).collect::<Vec<_>>()
    );
    assert_eq!(home_cards(&b)[1].0, "Monitoring");
    assert!(!b.reflow_frame().unwrap().unwrap().editing);
    let font = otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut presenter = reflow::Presenter::default();
    presenter.update(before, &font, 0.).unwrap();
    presenter
        .update(b.reflow_frame().unwrap().unwrap(), &font, 100.)
        .unwrap();
    assert!(presenter.active(100.));
    assert_ne!(presenter.mesh(100.).vertices, presenter.mesh(320.).vertices);
    assert!(!presenter.active(320.));
    assert_eq!(b.pending_count(), 0);
    assert_eq!(home_values(&b), moved);
    edit(&mut a, "Done editing cards", 0, 0);
    assert_eq!(a.pending_count(), 0);
    assert!(!a.reflow_frame().unwrap().unwrap().editing);
    // Node identities and their app destinations survive moves and orientation changes.
    let mut ids = home_cards(&a)
        .iter()
        .map(|v| (v.0.clone(), v.1))
        .collect::<Vec<_>>();
    ids.sort();
    let mut original = cards.iter().map(|v| (v.0.clone(), v.1)).collect::<Vec<_>>();
    original.sort();
    assert_eq!(ids, original);
}
#[test]
fn home_concurrent_different_card_moves_survive_and_resize_cancels_capture() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let ui = home::ui(&[]);
    let mut a = Client::default();
    let mut b = Client::default();
    activate(&mut a, ui.clone(), "concurrent-home", vec![]);
    activate(&mut b, ui.clone(), "concurrent-home", vec![]);
    let mut authority = Document::new(ui.state_schema, "concurrent-home").unwrap();
    let mut sa = Session::default();
    let mut sb = Session::default();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    let initial = home_values(&a);
    let cards = home_cards(&a);
    edit(&mut a, "Edit cards", 0, 0);
    edit(&mut b, "Edit cards", 0, 0);
    a.adjust_control(cards[0].1, 50, 0).unwrap();
    b.adjust_control(cards[5].1, -50, 0).unwrap();
    let first = a.typed_value(1).unwrap();
    let last = b.typed_value(6).unwrap();
    pump(&mut a, &mut authority, &mut sa);
    pump(&mut b, &mut authority, &mut sb);
    pump(&mut a, &mut authority, &mut sa);
    assert_eq!(home_values(&a), home_values(&b));
    assert_eq!(a.typed_value(1).unwrap(), first);
    assert_eq!(a.typed_value(6).unwrap(), last);
    assert_eq!(home_values(&a)[1..5], initial[1..5]);
    let cards = home_cards(&a);
    a.pointer(
        a.epoch(),
        PointerEvent {
            id: 1,
            phase: PointerPhase::Down,
            position: cards[0].2,
        },
    )
    .unwrap();
    assert!(a.input_busy());
    a.set_viewport([800., 1280.]).unwrap();
    assert!(!a.input_busy());
    a.pointer(
        a.epoch(),
        PointerEvent {
            id: 1,
            phase: PointerPhase::Up,
            position: cards[0].2,
        },
    )
    .unwrap();
    assert_eq!(a.ui_ref(), "app:home/ui");
    assert_eq!(a.pending_count(), 0);
}

#[test]
fn wrap_requires_its_layout_version_and_nonfinite_touch_cancels_order_capture() {
    use otrios_interfaces::{
        layout::Blueprint,
        scene::{PointerEvent, PointerPhase},
    };
    let ui = home::ui(&[]);
    let mut blueprint: Blueprint = otrios_core::dashboard::layout::Layout::from_bytes(&ui.layout)
        .unwrap()
        .blueprint()
        .clone();
    assert_eq!(blueprint.schema, "otrios-layout-v4");
    blueprint.schema = "otrios-layout-v3".into();
    assert!(otrios_core::dashboard::layout::Layout::encode(&blueprint).is_err());
    let mut client = Client::default();
    activate(&mut client, ui, "finite-touch", vec![]);
    edit(&mut client, "Edit cards", 0, 0);
    let initial = home_values(&client);
    let card = home_cards(&client)[0].2;
    client
        .pointer(
            client.epoch(),
            PointerEvent {
                id: 1,
                phase: PointerPhase::Down,
                position: card,
            },
        )
        .unwrap();
    assert!(client.input_busy());
    client
        .pointer(
            client.epoch(),
            PointerEvent {
                id: 1,
                phase: PointerPhase::Move,
                position: [f32::NAN, card[1]],
            },
        )
        .unwrap();
    assert!(!client.input_busy());
    assert_eq!(home_values(&client), initial);
}

#[test]
fn entry_and_app_links_resolve_app_owned_catalog_state_references() {
    let ui = authoring::AppUi::new(
        "launcher",
        "Launcher",
        authoring::schema("launcher-v1", &[]),
    )
    .build(
        "Launcher",
        vec![authoring::button("Open app").navigate_to(authoring::Destination::app("example-app"))],
    );
    let target = authoring::AppUi::new(
        "example-app",
        "Example",
        authoring::schema("example-v7", &[]),
    )
    .build("Example", vec![]);
    let bytes = encode_ui(&ui).unwrap();
    let target_bytes = encode_ui(&target).unwrap();
    let entry = |id: &str, ui: &Ui, bytes: &[u8], state: &str| CatalogEntry {
        path: ui
            .ui_ref
            .trim_start_matches("app:")
            .trim_end_matches("/ui")
            .into(),
        app: App {
            id: id.into(),
            title: ui.title.clone(),
            ui_ref: ui.ui_ref.clone(),
            state_ref: state.into(),
            status: "app".into(),
            dependencies: vec![],
        },
        ui_hash: hash(bytes),
        private: false,
    };
    let mut client = Client::default();
    client
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![
                    entry("launcher", &ui, &bytes, "launcher:owned-v4"),
                    entry("example-app", &target, &target_bytes, "example:owned-v7"),
                ],
            })
            .unwrap(),
        )
        .unwrap();
    client.open(&ui.ui_ref, "launcher:owned-v4");
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui: bytes.clone(),
                ui_hash: hash(&bytes),
                code: vec![],
                state: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    client.outgoing().unwrap();
    let mut authority = Document::new(ui.state_schema.clone(), "launcher:owned-v4").unwrap();
    pump(&mut client, &mut authority, &mut Session::default());
    let button = client.scene().unwrap().controls()[0].node.id;
    client.activate(client.epoch(), button).unwrap();
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::Mount {ui_ref,state_ref,..}if ui_ref=="app:example-app/ui"&&state_ref=="example:owned-v7")
    );
    client.back().unwrap();
    // Cancel the pending mount before the catalog's entry is opened.
    let mut found = false;
    while let Some(packet) = client.outgoing().unwrap() {
        if let ClientMessage::Mount {
            ui_ref, state_ref, ..
        } = decode_client(&packet).unwrap()
        {
            assert_eq!(ui_ref, "app:launcher/ui");
            assert_eq!(state_ref, "launcher:owned-v4");
            found = true;
            break;
        }
    }
    assert!(found);
}

#[test]
fn shipped_apps_do_not_include_example_subscreens() {
    let screens: Vec<_> = [
        home::screens,
        playground::screens,
        layout::screens,
        input_lab::screens,
        animation_lab::screens,
        three_d_lab::screens,
        monitoring_app::screens,
    ]
    .into_iter()
    .flat_map(|screens| screens())
    .collect();
    assert_eq!(screens.len(), 7);
    for (app, factory) in screens {
        assert!(valid_destination(&app.id));
        assert!(!app.id.contains('/'));
        let code = if app.id == "component-playground" {
            code()
        } else {
            vec![]
        };
        assert_eq!(factory(&code).ui_ref, app.ui_ref);
    }
}

#[test]
fn app_owned_screen_paths_are_canonical_and_parent_back_uses_catalog_bindings() {
    let (child, catalog) = nested_route_fixture();
    let mut client = Client::default();
    client
        .admit_catalog(&encode_catalog(&catalog).unwrap())
        .unwrap();
    client
        .mount_local(&encode_ui(&child).unwrap(), "app:fixture/state", &[], &[])
        .unwrap();
    assert_eq!(client.path(), "fixture/detail");
    client.navigate("fixture/detail").unwrap();
    while let Some(packet) = client.outgoing().unwrap() {
        assert!(
            !matches!(decode_client(&packet).unwrap(), ClientMessage::Mount { .. }),
            "same destination is idempotent"
        );
    }
    client.back().unwrap();
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::Mount {ui_ref, state_ref,..} if ui_ref == "app:fixture/ui" && state_ref == "app:fixture/state")
    );
    for bad in [
        "fixture//detail",
        "fixture/../detail",
        "fixture/Detail",
        "fixture/detail/",
        "fixture/missing",
    ] {
        assert!(client.navigate(bad).is_err());
    }
}

#[test]
fn route_rebase_retains_active_child_state_scene_and_epoch() {
    let (child, mut catalog) = nested_route_fixture();
    let mut client = Client::default();
    client
        .admit_catalog(&encode_catalog(&catalog).unwrap())
        .unwrap();
    client
        .mount_local(&encode_ui(&child).unwrap(), "app:fixture/state", &[], &[])
        .unwrap();
    let scene = client.scene().unwrap();
    let epoch = client.epoch();
    catalog.apps[1].path = "renamed".into();
    catalog.apps[2].path = "renamed/detail".into();
    assert!(
        !client
            .update_catalog(&encode_catalog(&catalog).unwrap())
            .unwrap()
    );
    assert_eq!(client.path(), "renamed/detail");
    assert_eq!(client.screen(), "fixture/detail");
    assert_eq!(client.epoch(), epoch);
    assert!(std::sync::Arc::ptr_eq(&scene, &client.scene().unwrap()));
    client.back().unwrap();
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
      ClientMessage::Mount {ui_ref, state_ref,..} if ui_ref=="app:fixture/ui" && state_ref=="app:fixture/state")
    );
}

// Nested screens exercise the generic contract without shipping demonstration apps.
fn nested_route_fixture() -> (Ui, Catalog) {
    let mut parent_ui = layout::ui(&[]);
    parent_ui.ui_ref = "app:fixture/ui".into();
    let mut child = parent_ui.clone();
    child.ui_ref = "app:fixture/detail/ui".into();
    let mut root = ui_entry(&home::ui(&[]), "home");
    root.path = "".into();
    let parent = ui_entry(&parent_ui, "app:fixture/state");
    let entry = ui_entry(&child, "app:fixture/state");
    let catalog = Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![root, parent, entry],
    };
    (child, catalog)
}

#[test]
fn cached_ui_mount_still_qualifies_fresh_state_and_rejects_wrong_digest() {
    use std::sync::Arc;
    let ui = ui_for_cache();
    let bytes = encode_ui(&ui).unwrap();
    let mut c = Client::default();
    activate(&mut c, ui.clone(), "cached", code());
    let mut authority = Document::new(ui.state_schema.clone(), "cached").unwrap();
    pump(&mut c, &mut authority, &mut Session::default());
    assert_eq!(c.resource_usage().ui_cache_entries, 1);
    c.open(&ui.ui_ref, "cached");
    assert_ne!(c.pending_mount_epoch(), 0);
    let ClientMessage::Mount {
        epoch, known_ui, ..
    } = decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount");
    };
    assert_eq!(known_ui, Some(hash(&bytes)));
    assert_eq!(c.pending_mount_epoch(), 0);
    authority.set(&[(1, 731)]).unwrap();
    let mounted = |epoch, ui_hash| {
        encode_server(&ServerMessage::Mounted {
            epoch,
            ui: vec![],
            ui_hash,
            code: vec![],
            state: authority.save().unwrap(),
        })
        .unwrap()
    };
    c.receive(&mounted(epoch, hash(&bytes))).unwrap();
    assert_eq!(
        c.value(1).unwrap(),
        731,
        "UI reuse cannot reuse stale State"
    );
    assert_eq!(c.resource_usage().ui_cache_hits, 1);
    c.outgoing().unwrap(); // Accept
    c.reset_connection();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount");
    };
    let scene = c.scene().unwrap();
    assert!(c.receive(&mounted(epoch, vec![9; 32])).is_err());
    assert!(Arc::ptr_eq(&scene, &c.scene().unwrap()));
    assert_eq!(c.resource_usage().ui_cache_hits, 1);
}

#[test]
fn ui_cache_excludes_private_documents_and_is_bounded_across_revisions() {
    let mut c = Client::default();
    let mut ui = home::ui(&[]);
    activate_access(&mut c, ui.clone(), "private", vec![], true);
    assert_eq!(c.resource_usage().ui_cache_entries, 0);
    c.reset_connection();
    let ClientMessage::Mount { known_ui, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount");
    };
    assert_eq!(known_ui, None);
    c = Client::default();
    for i in 0..12 {
        ui.title = format!("Home revision {i}");
        let scope = format!("public:{i}");
        activate(&mut c, ui.clone(), &scope, vec![]);
        let mut authority = Document::new(ui.state_schema.clone(), &scope).unwrap();
        pump(&mut c, &mut authority, &mut Session::default());
    }
    assert_eq!(c.resource_usage().ui_cache_entries, 8);
    assert!(c.resource_usage().ui_cache_bytes <= 2 * 1024 * 1024);
    let original = home::ui(&[]);
    c.admit_catalog(&home_catalog(ui_entry(&original, "public")))
        .unwrap();
    c.open(&original.ui_ref, "public");
    let ClientMessage::Mount { known_ui, .. } =
        decode_client(&c.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount");
    };
    assert_eq!(
        known_ui, None,
        "an admitted new hash cannot reuse a different revision"
    );
}

#[test]
fn text_viewport_fills_inner_margins_and_pans_locally_until_an_edit_recovers_caret() {
    use otrios_core::dashboard::{font::Font, render_scene_with_font};
    use otrios_interfaces::{
        keyboard::Edit,
        layout::Value,
        scene::{PointerEvent, PointerPhase, Primitive, Rect},
    };
    use std::sync::Arc;
    let font = Arc::new(
        Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            otrios_core::dashboard::font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    );
    let source: String = (0..60).map(|i| format!("W{i:02} ")).collect();
    let ui = input_lab::ui(&[]);
    let mut authority = Document::new(ui.state_schema.clone(), "text-viewport").unwrap();
    authority
        .set_typed(&[(1, Value::Text(source.clone()))])
        .unwrap();
    let mut c = Client::default();
    c.set_font(font.clone());
    activate_snapshot(
        &mut c,
        ui,
        "text-viewport",
        vec![],
        false,
        authority.save().unwrap(),
    );
    let mut session = Session::default();
    pump(&mut c, &mut authority, &mut session);
    c.activate(c.epoch(), node(&c, "First text field")).unwrap();
    fn window(c: &Client) -> (Rect, String, f32) {
        let scene = c.scene().unwrap();
        let fields = scene.controls();
        let field = fields
            .iter()
            .find(|n| {
                n.node
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.label == "First text field")
            })
            .unwrap();
        let view = field
            .node
            .children
            .iter()
            .find(|n| !n.children.is_empty())
            .unwrap();
        assert_eq!(view.frame.x, 24.);
        assert_eq!(view.frame.width, field.node.frame.width - 48.);
        assert!(matches!(view.primitive, Primitive::Canvas { clip: true }));
        let run = &view.children[0];
        let Primitive::Text { value, .. } = &run.primitive else {
            panic!("missing text run")
        };
        (
            Rect::new(
                field.origin[0] + view.frame.x,
                field.origin[1] + view.frame.y,
                view.frame.width,
                view.frame.height,
            ),
            value.clone(),
            run.frame.x,
        )
    }
    fn offset(c: &Client, display: &str, font: &Font) -> f32 {
        let (_, visible, x) = window(c);
        let start = display
            .find(&visible)
            .expect("visible run belongs to full text");
        font.text_width(&display[..start], 2.) - x
    }
    fn pointer(c: &mut Client, phase: PointerPhase, x: f32, y: f32) {
        c.pointer(
            c.epoch(),
            PointerEvent {
                id: 77,
                phase,
                position: [x, y],
            },
        )
        .unwrap();
    }
    let display = format!("|{source}");
    let (rect, visible, _) = window(&c);
    assert!(
        visible.len() > 38,
        "long fields use more than the former fixed window"
    );
    let mesh = render_scene_with_font(&c.scene().unwrap(), &font);
    let xs: Vec<_> = mesh
        .glyph_vertices
        .chunks_exact(16)
        .filter(|v| v[1] >= rect.y && v[1] <= rect.y + rect.height)
        .map(|v| v[0])
        .collect();
    assert!(xs.iter().all(|x| *x >= rect.x && *x <= rect.x + rect.width));
    assert!(
        xs.iter().any(|x| *x > rect.x + rect.width - 20.),
        "text reaches the inner right margin"
    );
    let y = rect.y + 15.;
    c.tick(100, 0).unwrap();
    pointer(&mut c, PointerPhase::Down, rect.x + 500., y);
    assert!(c.input_busy());
    c.tick(150, 0).unwrap();
    pointer(&mut c, PointerPhase::Move, rect.x + 400., y);
    let dragged = offset(&c, &display, &font);
    assert!((dragged - 100.).abs() < 0.1);
    // A second pointer cannot steal capture or move the caret.
    c.pointer(
        c.epoch(),
        PointerEvent {
            id: 78,
            phase: PointerPhase::Down,
            position: [rect.x, y],
        },
    )
    .unwrap();
    assert!((offset(&c, &display, &font) - dragged).abs() < 0.1);
    pointer(&mut c, PointerPhase::Up, rect.x + 400., y);
    assert!(!c.input_busy());
    c.tick(200, 0).unwrap();
    assert!(
        offset(&c, &display, &font) > dragged,
        "release has momentum"
    );
    c.tick(2000, 0).unwrap();
    let preview = offset(&c, &display, &font);
    c.tick(4000, 0).unwrap();
    assert!(
        (offset(&c, &display, &font) - preview).abs() < 0.1,
        "idle never snaps to caret"
    );
    assert!(
        c.outgoing().unwrap().is_none(),
        "panning emits no shared edits"
    );
    // Left at the unchanged beginning still recovers a manually hidden caret.
    c.hardware_edit(&Edit::Left { select: false }).unwrap();
    c.tick(4200, 0).unwrap();
    assert!(
        offset(&c, &display, &font) < 3.,
        "caret recovered within 200ms: {}",
        offset(&c, &display, &font)
    );
    c.tick(4500, 0).unwrap();
    assert_eq!(offset(&c, &display, &font), 0.);
    // Touch capture survives leaving the field and crossing the keyboard.
    pointer(&mut c, PointerPhase::Down, rect.x + 500., y);
    c.tick(4550, 0).unwrap();
    pointer(&mut c, PointerPhase::Move, rect.x - 5000., 700.);
    pointer(&mut c, PointerPhase::Up, rect.x - 5000., 700.);
    c.tick(5000, 0).unwrap();
    let end = font.text_width(&display, 2.) - rect.width;
    assert!(
        (offset(&c, &display, &font) - end).abs() < 0.1,
        "hard stop at text end"
    );
    // A tap hit-tests the panned run; a drag never moved the original caret.
    let tap = rect.x + 150.;
    let mut expected = otrios_core::dashboard::keyboard::TextBuffer::default();
    expected.replace(&source);
    let at = expected.hit_test(150. + end, &font, 2., true);
    expected.place_cursor(at);
    expected.edit(&Edit::Commit("!".into()));
    pointer(&mut c, PointerPhase::Down, tap, y);
    pointer(&mut c, PointerPhase::Up, tap, y);
    c.hardware_edit(&Edit::Commit("!".into())).unwrap();
    assert_eq!(
        c.typed_value(1).unwrap(),
        Value::Text(expected.text().into())
    );
    pump(&mut c, &mut authority, &mut session);
    assert_eq!(
        authority.typed_value(1).unwrap(),
        Value::Text(expected.text().into())
    );
    // A remote shorter value clamps presentation without following the caret.
    authority
        .set_typed(&[(1, Value::Text("short".into()))])
        .unwrap();
    pump(&mut c, &mut authority, &mut session);
    c.tick(6000, 0).unwrap();
    assert_eq!(window(&c).1, "short|");
    assert_eq!(window(&c).2, 0.);
}
