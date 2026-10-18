// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
mod authoring;
#[path = "../../../apps/home/app.rs"]
mod home;
#[path = "../../../apps/layout-lab/app.rs"]
mod layout;
#[path = "../../../apps/component-playground/app.rs"]
mod playground;
use otrios_core::canvas::*;
use otrios_interfaces::canvas::*;
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
    a.event(a.epoch(), 1, 870, 0).unwrap();
    pump(&mut a, &mut da, &mut sa);
    assert_eq!(a.value(1).unwrap(), 870);
    assert_eq!(b.value(1).unwrap(), 420);
    let s = a.scene().unwrap();
    let slider = s.path(22).unwrap();
    assert_eq!(slider.last().unwrap().node.frame.width, 870. * 0.488 + 32.);
    assert!(
        matches!(&s.path(16).unwrap().last().unwrap().node.primitive,otrios_interfaces::scene::Primitive::Text{value,..} if value=="87%")
    );
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
    assert_eq!(playground::manifest().dependencies.len(), 4);
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
