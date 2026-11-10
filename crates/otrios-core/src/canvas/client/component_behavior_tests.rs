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
fn mounted(ui: &Ui) -> (Client, crate::canvas::Document, Session) {
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
    client.set_oib_designer_studio(false).unwrap();
    client.set_viewport([1280., 800.]).unwrap();
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
