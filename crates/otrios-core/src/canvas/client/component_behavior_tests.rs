// SPDX-License-Identifier: LGPL-3.0-only
//! Actual Client routing, atomic failure and held guest-revision acceptance.
use super::*;
use otrios_interfaces::oib::{
    component_behavior::{Command, Output, encode_output},
    designer::*,
    library::{BehaviorDeclaration, BehaviorOutlet, ComponentCode, LocalProperty},
};
#[allow(dead_code)]
#[path = "../../../tests/support/oib_library_fixture.rs"]
mod fixture;

fn literal(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn module(body: &str, data: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let data: String = data
        .iter()
        .map(|(at, data)| format!("(data (i32.const {at}) \"{}\")", literal(data)))
        .collect();
    wat::parse_str(format!(
        r#"(module
      (memory (export "memory") 1 1)
      (func (export "abi_version") (result i32) i32.const 1)
      (func (export "input_buffer") (result i32) i32.const 0)
      {data}
      (func (export "dispatch") (param $p i32) (param i32) (result i64) {body}))"#
    ))
    .unwrap()
}
fn packed(at: u32, len: usize) -> u64 {
    (len as u64) << 32 | u64::from(at)
}
fn snapping_guest(step: u16) -> Vec<u8> {
    let bytes = encode_output(&Output {
        commands: vec![Command::Emit {
            outlet: 1,
            value: Value::Number(0.),
        }],
    })
    .unwrap();
    let payload = 2048 + bytes.len() - 4;
    module(
        &format!(
            r#"
      i32.const {payload}
      local.get $p i32.load16_u offset=10 i32.const {step} i32.div_u i32.const {step} i32.mul
      f32.convert_i32_u f32.store
      i64.const {}"#,
            packed(2048, bytes.len())
        ),
        &[(2048, bytes)],
    )
}
fn malicious_guest() -> Vec<u8> {
    let bad = encode_output(&Output {
        commands: vec![
            Command::SetLocal {
                property: 1,
                value: Value::Boolean(true),
            },
            Command::Emit {
                outlet: 1,
                value: Value::Number(333.),
            },
            Command::Emit {
                outlet: 999,
                value: Value::Number(444.),
            },
        ],
    })
    .unwrap();
    let good = encode_output(&Output {
        commands: vec![Command::Emit {
            outlet: 1,
            value: Value::Number(0.),
        }],
    })
    .unwrap();
    let empty = encode_output(&Output { commands: vec![] }).unwrap();
    let payload = 3072 + good.len() - 4;
    // No declared context properties. The Boolean local follows the full
    // occurrence-reference vector, its u64 ID and canonical Value header.
    let body = format!(
        r#"
      local.get $p i32.load8_u offset=8 i32.const 4 i32.eq
      if (result i64)
        i32.const {payload}
        local.get $p local.get $p i32.load8_u offset=18 i32.const 8 i32.mul i32.add
        i32.const 60 i32.add i32.load8_u
        if (result f32) f32.const 999 else f32.const 111 end
        f32.store
        i64.const {}
      else
        local.get $p i64.load offset=24 i64.const 30 i64.eq
        if (result i64) i64.const {} else i64.const {} end
      end"#,
        packed(3072, good.len()),
        packed(4096, empty.len()),
        packed(2048, bad.len())
    );
    module(&body, &[(2048, bad), (3072, good), (4096, empty)])
}
fn code(bytes: Vec<u8>) -> ComponentCode {
    ComponentCode {
        abi: 1,
        sha256: hash(&bytes).try_into().unwrap(),
        bytes,
    }
}
fn library_ui(bytes: Vec<u8>) -> Ui {
    let mut ui = fixture::imported();
    let seed = fixture::seed_mut(&mut ui);
    let body = &mut seed.resolved_libraries[0];
    body.graph.format = otrios_interfaces::oib::library::BEHAVIOR_FORMAT.into();
    body.graph.behaviors = vec![BehaviorDeclaration {
        id: 1,
        definition: DefinitionId(12),
        node: NodeId(1),
        properties: vec![],
        local: vec![LocalProperty {
            id: 1,
            default: Value::Boolean(false),
        }],
        outlets: vec![BehaviorOutlet {
            id: 1,
            source: PropertyPath {
                references: vec![],
                node: NodeId(1),
                property: PropertyId(4),
            },
            value_type: otrios_interfaces::layout::ValueType::Number,
        }],
        can_capture: true,
    }];
    body.graph = crate::library::seal_graph(body.graph.clone()).unwrap();
    body.component = Some(code(bytes));
    let exports = body
        .graph
        .exports
        .iter()
        .map(|export| otrios_interfaces::oib::graph::LockedExport {
            export: export.id,
            contract: export.contract,
        })
        .collect();
    seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0].exports = exports;
    ui
}
fn replacement(ui: &Ui, bytes: Vec<u8>) -> Ui {
    let mut next = ui.clone();
    let seed = fixture::seed_mut(&mut next);
    seed.resolved_libraries[0].component = Some(code(bytes));
    seed.resolved_libraries[0].revision = [2; 32];
    seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0].revision = [2; 32];
    next
}
fn mounted_preview(ui: &Ui) -> (Client, crate::canvas::Document, Session) {
    let ui = fixture::admit(ui);
    let scope = "l2:client";
    let mut authority = crate::canvas::Document::new(ui.state_schema.clone(), scope).unwrap();
    authority.configure_oib(&ui).unwrap();
    let writer = [1; 16];
    let (mut session, _) = authority.open_session(&writer, 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let runtime = Arc::new(super::super::component_behavior::Runtime::default());
    let mut mount = qualify(
        ui,
        scope,
        &[],
        &VecDeque::new(),
        &[],
        Qualification {
            component_runtime: runtime.clone(),
            epoch: 1,
            private: false,
            writer,
        },
    )
    .unwrap();
    mount
        .doc
        .receive(&authority.generate(&mut session).unwrap().unwrap())
        .unwrap();
    mount.refresh().unwrap();
    let encoded_ui = crate::canvas::encode_ui(&mount.ui).unwrap();
    let mut manifest = crate::test_support::oib_designer::manifest();
    manifest.state_ref = scope.into();
    let mut client = Client {
        component_runtime: runtime,
        writer: Some(writer),
        ..Client::default()
    };
    client
        .admit_catalog(
            &crate::canvas::encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app: manifest,
                    path: "oib-designer".into(),
                    private: false,
                    ui_hash: hash(&encoded_ui),
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client.active = Some(mount);
    pump(&mut client, &mut authority, &mut session);
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    client.set_viewport([1280., 800.]).unwrap();
    (client, authority, session)
}
// Existing execution/receipt tests exercise an app, independently of Designer.
fn mounted(ui: &Ui) -> (Client, crate::canvas::Document, Session) {
    let (mut client, authority, session) = mounted_preview(ui);
    let mount = client.active.as_mut().unwrap();
    mount.designer = None;
    mount.sync_designer_preview().unwrap();
    mount.invalidate();
    (client, authority, session)
}
fn pump(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
    for _ in 0..40 {
        let now = client.active.as_ref().unwrap().now.saturating_add(33);
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
        if !progress && client.confirmed() {
            return;
        }
    }
    panic!("L2 Client did not settle");
}
fn slider(client: &Client, instance: u64) -> (u8, [f32; 2], f32) {
    let mount = client.active.as_ref().unwrap();
    let address = ObjectAddress {
        instance: InstanceId(instance),
        references: vec![NodeId(7)],
        node: NodeId(1),
    };
    let node = fixture::seed(&mount.ui)
        .designer
        .as_ref()
        .unwrap()
        .mappings
        .objects
        .iter()
        .find(|m| m.address == address)
        .unwrap()
        .layout_node
        .unwrap();
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(node)).unwrap();
    let p = path.last().unwrap();
    let Some(TouchResponder::Drag(Action::AppEvent(control))) = p.node.responder else {
        panic!("admitted slider drag")
    };
    (
        control,
        [
            p.origin[0],
            p.origin[1] + p.node.frame.height * p.scale * 0.5,
        ],
        p.node.frame.width * p.scale,
    )
}
fn pointer(client: &mut Client, id: u32, phase: PointerPhase, point: [f32; 2], at: u64) {
    client
        .pointer_at(
            client.epoch(),
            PointerEvent {
                id,
                phase,
                position: point,
            },
            at,
        )
        .unwrap();
}
fn fields(client: &Client) -> [u16; 4] {
    [1, 2, 3, 4].map(|id| client.value(id).unwrap())
}

#[test]
fn designer_test_guest_writes_only_local_samples_and_reentry_resets() {
    let ui = library_ui(snapping_guest(100));
    let (mut client, authority, _session) = mounted_preview(&ui);
    let baseline = authority.value(1).unwrap();
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 600);
    assert_eq!(
        client.active.as_ref().unwrap().doc.typed_value(1).unwrap(),
        Value::Number(f32::from(baseline))
    );
    assert_eq!(authority.value(1).unwrap(), baseline);
    assert_eq!(client.dispatch_usage().pending_fields, 0);
    assert!(
        client.confirmed(),
        "local samples never wait for authority receipts"
    );
    assert!(client.reload_ready());
    assert!(
        client.outgoing().unwrap().is_none(),
        "sample edits have no wire output"
    );
    client.set_oib_designer_mode(DesignerMode::Edit).unwrap();
    assert_eq!(client.value(1).unwrap(), baseline);
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    assert_eq!(client.value(1).unwrap(), baseline);
    client.event(client.epoch(), control, 437, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 400);
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn designer_test_keeps_captured_guest_until_release_without_authority_edits() {
    let ui = library_ui(snapping_guest(100));
    let next = replacement(&ui, snapping_guest(250));
    let (mut client, mut authority, mut session) = mounted_preview(&ui);
    let baseline_value = authority.value(1).unwrap();
    let (_, origin, width) = slider(&client, 20);
    let presented = client.active.as_ref().unwrap().oib_generation;
    pointer(
        &mut client,
        9,
        PointerPhase::Down,
        [origin[0] + width * 0.437, origin[1]],
        1000,
    );
    assert_eq!(client.value(1).unwrap(), 400);
    assert!(client.active.as_ref().unwrap().input.dragging(9));
    assert!(client.outgoing().unwrap().is_none());
    let (generation, baseline) = authority.oib_baseline().unwrap();
    authority
        .adopt_oib_baseline(&next, generation, baseline)
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(client.active.as_ref().unwrap().oib_generation, presented);
    pointer(
        &mut client,
        9,
        PointerPhase::Move,
        [origin[0] + width * 0.687, origin[1]],
        1010,
    );
    assert_eq!(client.value(1).unwrap(), 600);
    pointer(
        &mut client,
        9,
        PointerPhase::Up,
        [origin[0] + width * 0.687, origin[1]],
        1020,
    );
    pump(&mut client, &mut authority, &mut session);
    assert_ne!(client.active.as_ref().unwrap().oib_generation, presented);
    assert_eq!(client.value(1).unwrap(), baseline_value);
    assert_eq!(authority.value(1).unwrap(), baseline_value);
    assert!(client.confirmed());
}

#[test]
fn designer_test_reconnect_receives_source_without_losing_capture_or_sending_samples() {
    let ui = library_ui(snapping_guest(100));
    let next = replacement(&ui, snapping_guest(250));
    let (mut client, mut authority, mut session) = mounted_preview(&ui);
    let value = authority.value(1).unwrap();
    let (_, origin, width) = slider(&client, 20);
    pointer(
        &mut client,
        8,
        PointerPhase::Down,
        [origin[0] + width * 0.437, origin[1]],
        1000,
    );
    let (generation, baseline) = authority.oib_baseline().unwrap();
    authority
        .adopt_oib_baseline(&next, generation, baseline)
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    let (candidate, mut replacement_session) = qualified_candidate(&client, &mut authority, &next);
    client.activate_candidate(candidate, true, vec![]).unwrap();
    assert!(client.active.as_ref().unwrap().input.dragging(8));
    assert_eq!(client.value(1).unwrap(), 400);
    pointer(
        &mut client,
        8,
        PointerPhase::Move,
        [origin[0] + width * 0.687, origin[1]],
        1010,
    );
    assert_eq!(client.value(1).unwrap(), 600);
    pointer(
        &mut client,
        8,
        PointerPhase::Up,
        [origin[0] + width * 0.687, origin[1]],
        1020,
    );
    pump(&mut client, &mut authority, &mut replacement_session);
    assert_eq!(client.value(1).unwrap(), value);
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 500);
    assert_eq!(authority.value(1).unwrap(), value);
    assert!(client.confirmed());
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn designer_test_discards_guest_local_state_between_runs() {
    let ui = library_ui(stateful_snapping_guest(100));
    let (mut client, _authority, _session) = mounted_preview(&ui);
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 437, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 400);
    client.event(client.epoch(), control, 437, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 500);
    client.set_oib_designer_mode(DesignerMode::Edit).unwrap();
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    client.event(client.epoch(), control, 437, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 400);
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn designer_test_source_refresh_resets_fixture_and_keeps_live_receipts() {
    let ui = library_ui(snapping_guest(100));
    let next = replacement(&ui, snapping_guest(250));
    let (mut client, mut authority, mut session) = mounted_preview(&ui);
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 600);
    authority
        .set_typed_authority(&[(1, Value::Number(123.))])
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        client.active.as_ref().unwrap().doc.typed_value(1).unwrap(),
        Value::Number(123.)
    );
    assert_eq!(
        client.value(1).unwrap(),
        600,
        "live values do not overwrite an active fixture"
    );
    let (generation, baseline) = authority.oib_baseline().unwrap();
    authority
        .adopt_oib_baseline(&next, generation, baseline)
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        client.value(1).unwrap(),
        123,
        "new authored source starts a fresh sample fixture"
    );
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        500,
        "new library code is active locally"
    );
    assert_eq!(authority.value(1).unwrap(), 123);
    assert!(client.confirmed());
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn library_slider_guest_overrides_host_set_and_reports_its_exact_code() {
    let guest = snapping_guest(100);
    let expected = hex(&hash(&guest));
    let ui = library_ui(guest);
    let (mut client, mut authority, mut session) = mounted(&ui);
    let before = fields(&client);
    let (_, origin, width) = slider(&client, 20);
    pointer(
        &mut client,
        10,
        PointerPhase::Down,
        [origin[0] + width * 0.437, origin[1]],
        1000,
    );
    assert_eq!(
        client.value(1).unwrap(),
        400,
        "guest snaps 437; host Set would store 437"
    );
    pointer(
        &mut client,
        10,
        PointerPhase::Move,
        [origin[0] + width * 0.687, origin[1]],
        1010,
    );
    assert_eq!(client.value(1).unwrap(), 600);
    pointer(
        &mut client,
        10,
        PointerPhase::Up,
        [origin[0] + width * 0.687, origin[1]],
        1020,
    );
    pump(&mut client, &mut authority, &mut session);
    let usage = client.resource_usage();
    assert_eq!(usage.component_behavior_instances, 2);
    assert!(usage.component_behavior_calls >= 3);
    assert_eq!(usage.component_behavior_errors, 0);
    assert_eq!(usage.component_behavior_hashes, vec![expected]);
    assert_eq!(&fields(&client)[1..], &before[1..]);
    assert_eq!(authority.value(1).unwrap(), 600);
}
#[test]
fn mixed_unauthorized_guest_result_is_atomic_and_cancels_only_its_capture() {
    let ui = library_ui(malicious_guest());
    let (mut client, _authority, _session) = mounted(&ui);
    // Exercise ordinary app routing with two contacts. Designer itself admits
    // one local interaction at a time, independently of component execution.
    client.active.as_mut().unwrap().designer = None;
    client.active.as_mut().unwrap().invalidate();
    let before = fields(&client);
    let (other, other_origin, other_width) = slider(&client, 30);
    pointer(
        &mut client,
        12,
        PointerPhase::Down,
        [other_origin[0] + other_width * 0.5, other_origin[1]],
        990,
    );
    assert!(client.active.as_ref().unwrap().input.dragging(12));
    let (control, origin, width) = slider(&client, 20);
    pointer(
        &mut client,
        11,
        PointerPhase::Down,
        [origin[0] + width * 0.8, origin[1]],
        1000,
    );
    assert_eq!(fields(&client), before);
    assert!(!client.active.as_ref().unwrap().input.dragging(11));
    assert!(
        client.active.as_ref().unwrap().input.dragging(12),
        "failure must not cancel another pointer"
    );
    assert_eq!(client.resource_usage().component_behavior_errors, 1);
    // A later valid call reads the same occurrence's host local. It emits 111
    // for false and 999 for true, so no partial local write can hide here.
    client.event(client.epoch(), control, 0, 0).unwrap();
    assert_eq!(client.value(1).unwrap(), 111);
    assert_eq!(&fields(&client)[1..], &before[1..]);
    pointer(
        &mut client,
        12,
        PointerPhase::Cancel,
        [other_origin[0] + other_width * 0.5, other_origin[1]],
        1030,
    );
    client.event(client.epoch(), other, 0, 0).unwrap();
    assert_eq!(
        client.value(2).unwrap(),
        111,
        "other occurrence remains callable and isolated"
    );
    assert_eq!(client.resource_usage().component_behavior_errors, 1);
}
#[test]
fn held_slider_pins_old_guest_until_release_and_final_receipt() {
    let ui = library_ui(snapping_guest(100));
    let next = replacement(&ui, snapping_guest(250));
    let old_hash = hex(&fixture::seed(&ui).resolved_libraries[0]
        .component
        .as_ref()
        .unwrap()
        .sha256);
    let next_hash = hex(&hash(
        &fixture::seed(&next).resolved_libraries[0]
            .component
            .as_ref()
            .unwrap()
            .bytes,
    ));
    let (mut client, mut authority, mut session) = mounted(&ui);
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
        .edit_oib_property(&label, Some(Value::Text("Held guest".into())))
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    let head = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_property_register(&label)
        .unwrap()
        .version;
    let (_, origin, width) = slider(&client, 20);
    let old_generation = client.active.as_ref().unwrap().oib_generation;
    let untouched = [2, 3, 4].map(|id| client.value(id).unwrap());
    pointer(
        &mut client,
        12,
        PointerPhase::Down,
        [origin[0] + width * 0.437, origin[1]],
        1000,
    );
    pump(&mut client, &mut authority, &mut session);
    let (generation, baseline) = authority.oib_baseline().unwrap();
    authority
        .adopt_oib_baseline(&next, generation, baseline)
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        client.active.as_ref().unwrap().oib_generation,
        old_generation
    );
    assert_eq!(
        client.resource_usage().component_behavior_hashes,
        vec![old_hash.clone()]
    );
    pointer(
        &mut client,
        12,
        PointerPhase::Move,
        [origin[0] + width * 0.687, origin[1]],
        1010,
    );
    assert_eq!(
        client.value(1).unwrap(),
        600,
        "held gesture still uses old 100-step code"
    );
    pointer(
        &mut client,
        12,
        PointerPhase::Up,
        [origin[0] + width * 0.687, origin[1]],
        1020,
    );
    assert!(!client.confirmed());
    assert_eq!(
        client.resource_usage().component_behavior_hashes,
        vec![old_hash]
    );
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(
        client.resource_usage().component_behavior_hashes,
        vec![next_hash]
    );
    assert_eq!(
        client.value(1).unwrap(),
        600,
        "activation itself preserves app value"
    );
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        500,
        "next input uses the new 250-step code"
    );
    assert_eq!([2, 3, 4].map(|id| client.value(id).unwrap()), untouched);
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .oib_property_register(&label)
            .unwrap()
            .version,
        head
    );
}

#[test]
fn candidate_guest_quota_rejection_keeps_the_active_client_usable() {
    let bytes = snapping_guest(100);
    let ui = library_ui(bytes.clone());
    let (mut client, _authority, _session) = mounted(&ui);
    let old = client.active.as_ref().unwrap().ui.clone();
    let before = fields(&client);
    let runtime = client.component_runtime.clone();
    let compiled = runtime
        .compile(&bytes, hash(&bytes).try_into().unwrap())
        .unwrap();
    let leases: Vec<_> = (0..6)
        .map(|_| runtime.instantiate(compiled.clone()).unwrap())
        .collect();
    assert_eq!(runtime.live_instances(), 8);
    let next = fixture::admit(&replacement(&ui, snapping_guest(250)));
    let rejected = qualify(
        next,
        "l2:replacement",
        &[],
        &VecDeque::new(),
        &[],
        Qualification {
            component_runtime: runtime.clone(),
            epoch: 2,
            private: false,
            writer: [2; 16],
        },
    );
    assert!(
        rejected.is_err(),
        "active plus candidate leases are one bounded budget"
    );
    assert_eq!(
        runtime.live_instances(),
        8,
        "failed staging leaked no guest"
    );
    assert_eq!(client.active.as_ref().unwrap().ui, old);
    assert_eq!(fields(&client), before);
    drop(leases);
    assert_eq!(runtime.live_instances(), 2);
    let (control, _, _) = slider(&client, 20);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        600,
        "old guest remains usable after rejection"
    );
}

fn stateful_snapping_guest(step: u16) -> Vec<u8> {
    let bytes = encode_output(&Output {
        commands: vec![
            Command::SetLocal {
                property: 1,
                value: Value::Boolean(true),
            },
            Command::Emit {
                outlet: 1,
                value: Value::Number(0.),
            },
        ],
    })
    .unwrap();
    let payload = 2048 + bytes.len() - 4;
    module(
        &format!(
            r#"
      i32.const {payload}
      local.get $p i32.load16_u offset=10 i32.const {step} i32.div_u i32.const {step} i32.mul
      local.get $p local.get $p i32.load8_u offset=18 i32.const 8 i32.mul i32.add
      i32.const 60 i32.add i32.load8_u
      if (result i32) i32.const 100 else i32.const 0 end
      i32.add f32.convert_i32_u f32.store
      i64.const {}"#,
            packed(2048, bytes.len())
        ),
        &[(2048, bytes)],
    )
}
fn qualified_candidate(
    client: &Client,
    authority: &mut crate::canvas::Document,
    ui: &Ui,
) -> (Mount, Session) {
    let ui = fixture::admit(ui);
    let writer = [1; 16];
    let (mut session, _) = authority.open_session(&writer, 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let mut candidate = qualify(
        ui,
        "l2:client",
        &[],
        &VecDeque::new(),
        &[],
        Qualification {
            component_runtime: client.component_runtime.clone(),
            epoch: 2,
            private: false,
            writer,
        },
    )
    .unwrap();
    candidate
        .doc
        .receive(&authority.generate(&mut session).unwrap().unwrap())
        .unwrap();
    candidate.refresh().unwrap();
    (candidate, session)
}
fn exercise_direct_activation(reconnecting: bool) {
    let bytes = stateful_snapping_guest(100);
    let ui = library_ui(bytes.clone());
    let next = replacement(&ui, stateful_snapping_guest(250));
    let old_hash = hex(&fixture::seed(&ui).resolved_libraries[0]
        .component
        .as_ref()
        .unwrap()
        .sha256);
    let next_hash = hex(&fixture::seed(&next).resolved_libraries[0]
        .component
        .as_ref()
        .unwrap()
        .sha256);
    let (mut client, mut authority, mut session) = mounted(&ui);
    let (control, origin, width) = slider(&client, 20);
    if reconnecting {
        pointer(
            &mut client,
            77,
            PointerPhase::Down,
            [origin[0] + width * 0.437, origin[1]],
            1000,
        );
    } else {
        client.event(client.epoch(), control, 437, 0).unwrap();
    }
    pump(&mut client, &mut authority, &mut session);
    assert_eq!(client.value(1).unwrap(), 400);
    let initial_calls = client.resource_usage().component_behavior_calls;
    let untouched = [2, 3, 4].map(|id| client.value(id).unwrap());
    let (generation, baseline) = authority.oib_baseline().unwrap();
    authority
        .adopt_oib_baseline(&next, generation, baseline)
        .unwrap();
    if reconnecting {
        // The authoritative document advances while held presentation and guest
        // retain the old closure. This selects the reconnect preservation arm.
        pump(&mut client, &mut authority, &mut session);
        assert!(client.active.as_ref().unwrap().input.dragging(77));
        assert_eq!(
            client.resource_usage().component_behavior_hashes,
            vec![old_hash.clone()]
        );
    }
    let runtime = client.component_runtime.clone();
    let compiled = runtime
        .compile(&bytes, hash(&bytes).try_into().unwrap())
        .unwrap();
    let leases: Vec<_> = (0..4)
        .map(|_| runtime.instantiate(compiled.clone()).unwrap())
        .collect();
    assert_eq!(runtime.live_instances(), 6);
    let (candidate, mut candidate_session) = qualified_candidate(&client, &mut authority, &next);
    assert_eq!(
        runtime.live_instances(),
        8,
        "active and qualified guests fill the shared budget"
    );
    if !reconnecting {
        client.previous_catalog = Some((
            client.catalog.as_ref().unwrap().clone(),
            client.catalog_hash.clone(),
        ));
    }
    client
        .activate_candidate(candidate, reconnecting, vec![])
        .unwrap();
    assert_eq!(
        runtime.live_instances(),
        6,
        "activation does not retain an extra candidate guest set"
    );
    if reconnecting {
        assert!(client.active.as_ref().unwrap().input.dragging(77));
        assert_eq!(
            client.resource_usage().component_behavior_hashes,
            vec![old_hash.clone()]
        );
        pointer(
            &mut client,
            77,
            PointerPhase::Move,
            [origin[0] + width * 0.687, origin[1]],
            1010,
        );
        assert_eq!(
            client.value(1).unwrap(),
            700,
            "held reconnect retains old code and its local state"
        );
        pointer(
            &mut client,
            77,
            PointerPhase::Up,
            [origin[0] + width * 0.687, origin[1]],
            1020,
        );
        assert!(!client.confirmed());
        assert_eq!(
            client.resource_usage().component_behavior_hashes,
            vec![old_hash]
        );
        pump(&mut client, &mut authority, &mut candidate_session);
        assert_eq!(
            client.value(1).unwrap(),
            700,
            "activation itself emits no app edit"
        );
    } else {
        assert_eq!(
            client.value(1).unwrap(),
            400,
            "catalogue activation itself emits no app edit"
        );
    }
    // No Tick follows catalogue activation before these assertions and event.
    assert_eq!(
        client.resource_usage().component_behavior_hashes,
        vec![next_hash]
    );
    assert!(
        client.resource_usage().component_behavior_calls >= initial_calls,
        "cumulative diagnostics survive activation"
    );
    let calls = client.resource_usage().component_behavior_calls;
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        600,
        "new 250-step code plus resumed true local gives600; oldcode gives700 and resetlocal gives500"
    );
    assert_eq!(client.resource_usage().component_behavior_calls, calls + 1);
    assert_eq!([2, 3, 4].map(|id| client.value(id).unwrap()), untouched);
    assert_eq!(client.resource_usage().component_behavior_errors, 0);
    drop(leases);
    assert_eq!(runtime.live_instances(), 2);
}
#[test]
fn catalogue_activation_commits_new_guest_and_resumed_locals_before_any_tick() {
    exercise_direct_activation(false);
}
#[test]
fn held_reconnect_preserves_old_guest_then_activates_without_third_guest_set() {
    exercise_direct_activation(true);
}

fn catalog_for_ui(client: &Client, ui: &Ui) -> Vec<u8> {
    let mut catalog = client.catalog.as_ref().unwrap().clone();
    let entry = catalog
        .apps
        .iter_mut()
        .find(|e| e.app.ui_ref == ui.ui_ref)
        .unwrap();
    entry.ui_hash = hash(&crate::canvas::encode_ui(ui).unwrap());
    crate::canvas::encode_catalog(&catalog).unwrap()
}
fn managed_binding() -> MountBinding {
    MountBinding {
        version: 2,
        actor: [1; 32],
        collection: "a".repeat(32),
        editable: true,
        ui_editable: true,
        comments: false,
        moderate: false,
    }
}
fn adopted_client_with_width_override(
    sync_first: bool,
) -> (Client, crate::canvas::Document, Session, Ui, u8) {
    let ui = library_ui(stateful_snapping_guest(100));
    let next = replacement(&ui, stateful_snapping_guest(250));
    let (mut client, mut authority, mut session) = mounted(&ui);
    client.generation = client.epoch();
    client.active.as_mut().unwrap().binding = Some(managed_binding());
    let width = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(2),
        },
    };
    client
        .active
        .as_mut()
        .unwrap()
        .doc
        .edit_oib_property(&width, Some(Value::Number(420.)))
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    let (control, _, extent) = slider(&client, 20);
    assert_eq!(extent, 420.);
    client.event(client.epoch(), control, 437, 0).unwrap();
    pump(&mut client, &mut authority, &mut session);
    let (generation, baseline) = authority.oib_baseline().unwrap();
    authority
        .adopt_oib_baseline(&next, generation, baseline)
        .unwrap();
    if sync_first {
        pump(&mut client, &mut authority, &mut session);
    }
    let signed = fixture::admit(&next);
    assert_eq!(slider(&client, 20).2, 420.);
    assert_ne!(
        client.active.as_ref().unwrap().ui.layout,
        signed.layout,
        "live Width420 compilation differs from pristine signed geometry"
    );
    assert!(
        compatible_app_update(&client.active.as_ref().unwrap().ui, &signed).is_err(),
        "the observed live-vs-signed comparison is an invalid compatibility comparator"
    );
    (client, authority, session, signed, control)
}
fn respond_to_catalog_mount(
    client: &mut Client,
    authority: &mut crate::canvas::Document,
    signed: &Ui,
    state_selection_ui: &Ui,
) -> (u32, Session, Vec<u8>) {
    let request = decode_client(&client.outgoing().unwrap().unwrap()).unwrap();
    let ClientMessage::Mount {
        epoch,
        writer,
        resume,
        ..
    } = request
    else {
        panic!("catalog update must request a real mount")
    };
    let (mut session, _) = authority.open_session(&writer, resume).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(state_selection_ui).unwrap())
        .unwrap();
    let ui = crate::canvas::encode_ui(signed).unwrap();
    let response = encode_server(&ServerMessage::Mounted {
        epoch,
        ui_hash: hash(&ui),
        ui,
        code: vec![],
        state: authority.generate(&mut session).unwrap().unwrap(),
        binding: Some(managed_binding()),
        initial_view: None,
    })
    .unwrap();
    (epoch, session, response)
}
fn exercise_full_catalog_mount(sync_first: bool) {
    let (mut client, mut authority, _old_session, signed, control) =
        adopted_client_with_width_override(sync_first);
    let before = fields(&client);
    let calls = client.resource_usage().component_behavior_calls;
    let expected = hex(&fixture::seed(&signed).resolved_libraries[0]
        .component
        .as_ref()
        .unwrap()
        .sha256);
    if sync_first {
        assert_eq!(
            client.resource_usage().component_behavior_hashes,
            vec![expected.clone()],
            "live DocSync already adopted this guest before catalog receipt"
        );
    } else {
        assert_ne!(
            client.resource_usage().component_behavior_hashes,
            vec![expected.clone()],
            "catalog qualification precedes the old client's live baseline delta"
        );
    }
    let admitted = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .admitted_oib_ui()
        .unwrap();
    assert!(compatible_catalog_ui(admitted, &signed).is_ok());
    assert!(
        client
            .update_catalog(&catalog_for_ui(&client, &signed))
            .unwrap()
    );
    let old_epoch = client.epoch();
    let (epoch, _candidate_session, response) =
        respond_to_catalog_mount(&mut client, &mut authority, &signed, &signed);
    assert!(epoch > old_epoch);
    assert!(!client.receive(&response).unwrap());
    assert_eq!(
        client.epoch(),
        old_epoch,
        "qualified candidate remains invisible before activation"
    );
    assert!(client.qualified.is_some());
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),ClientMessage::AcceptMount{epoch:accepted}if accepted==epoch)
    );
    assert!(
        client
            .receive(&encode_server(&ServerMessage::Activated { epoch }).unwrap())
            .unwrap()
    );
    assert_eq!(client.epoch(), epoch);
    assert!(!client.catalog_update_pending());
    assert_eq!(client.component_runtime.live_instances(), 2);
    assert_eq!(
        client.resource_usage().component_behavior_hashes,
        vec![expected]
    );
    assert_eq!(fields(&client), before);
    assert_eq!(slider(&client, 20).2, 420.);
    assert!(client.resource_usage().component_behavior_calls >= calls);
    client.event(client.epoch(), control, 687, 0).unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        600,
        "latest250-step guest retains true host local across full handshake"
    );
    assert_eq!(client.resource_usage().component_behavior_calls, calls + 1);
    assert_eq!(client.resource_usage().component_behavior_errors, 0);
}
#[test]
fn full_catalog_mount_uses_admitted_baseline_after_live_adoption_and_width_override() {
    exercise_full_catalog_mount(true);
}
#[test]
fn full_catalog_mount_can_qualify_library_revision_before_live_baseline_delta() {
    exercise_full_catalog_mount(false);
}
#[test]
fn full_catalog_mount_still_rejects_changed_oib_grants_schema_and_delivery_metadata() {
    for mutation in ["grants", "schema", "export", "delivery"] {
        let (mut client, mut authority, _old_session, signed, _control) =
            adopted_client_with_width_override(true);
        let before_ui = client.active.as_ref().unwrap().ui.clone();
        let before_values = fields(&client);
        let before_hashes = client.resource_usage().component_behavior_hashes;
        let before_catalog = client.catalog_hash.clone();
        let mut changed = signed.clone();
        if mutation == "delivery" {
            let mut catalog = client.catalog.as_ref().unwrap().clone();
            catalog.apps[0].private = true;
            assert!(
                client
                    .update_catalog(&crate::canvas::encode_catalog(&catalog).unwrap())
                    .is_err()
            );
        } else {
            if mutation == "grants" {
                fixture::seed_mut(&mut changed)
                    .graph
                    .as_mut()
                    .unwrap()
                    .grants
                    .push(otrios_interfaces::oib::graph::ActionGrant {
                        field: 1,
                        kind: GestureKind::Activate,
                        operation: otrios_interfaces::oib::graph::ActionOperation::Toggle,
                    });
            } else if mutation == "schema" {
                changed.state_schema.id = "l2-different-state-schema".into();
            } else {
                let seed = fixture::seed_mut(&mut changed);
                let body = &mut seed.resolved_libraries[0];
                body.graph.behaviors[0].can_capture = false;
                body.graph = crate::library::seal_graph(body.graph.clone()).unwrap();
                body.revision = [3; 32];
                let exports = body
                    .graph
                    .exports
                    .iter()
                    .map(|export| otrios_interfaces::oib::graph::LockedExport {
                        export: export.id,
                        contract: export.contract,
                    })
                    .collect();
                let pin = &mut seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0];
                pin.revision = [3; 32];
                pin.exports = exports;
            }
            let changed = fixture::admit(&changed);
            assert!(
                client
                    .update_catalog(&catalog_for_ui(&client, &changed))
                    .unwrap()
            );
            let (_epoch, _candidate_session, response) =
                respond_to_catalog_mount(&mut client, &mut authority, &changed, &signed);
            assert!(
                client.receive(&response).is_err(),
                "{mutation} cannot pass signed baseline compatibility"
            );
        }
        assert_eq!(client.active.as_ref().unwrap().ui, before_ui);
        assert_eq!(fields(&client), before_values);
        assert_eq!(
            client.resource_usage().component_behavior_hashes,
            before_hashes
        );
        assert_eq!(client.catalog_hash, before_catalog);
        assert_eq!(slider(&client, 20).2, 420.);
    }
}
