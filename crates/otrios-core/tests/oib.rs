// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../apps/live-ui/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::*;
use otrios_interfaces::{
    canvas::*,
    layout::Value,
    oib::{
        authoring::*,
        designer::{
            ClassKind, DefinitionId, DesignerMode, InstanceId, PropertyAddress, PropertyId,
            PropertyPath,
        },
    },
};

fn setup() -> (Ui, Document) {
    assert_eq!(app::manifest().id, "live-ui");
    setup_ui(app::ui(&[]))
}
fn setup_stacked() -> (Ui, Document) {
    // Canonical wide-track fixture; no obsolete serialized flat-cell seed.
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("fixture")
    };
    let graph = seed.graph.as_mut().unwrap();
    graph
        .classes
        .iter_mut()
        .find(|c| c.implementation == ClassKind::O2DSlider)
        .unwrap()
        .properties
        .iter_mut()
        .find(|p| p.id == PropertyId(2))
        .unwrap()
        .number_range = Some([320., 600.]);
    graph
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(12))
        .unwrap()
        .nodes[0]
        .defaults
        .iter_mut()
        .find(|p| p.property == PropertyId(2))
        .unwrap()
        .value = Value::Number(520.);
    setup_ui(ui)
}
fn setup_ui(ui: Ui) -> (Ui, Document) {
    // Admit a wide property range so these integration tests can distinguish
    // valid typed edits from scene-level rejection of a tiny drag target.
    // The production editor's tighter numeric range is qualified separately.
    let mut ui = ui;
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("fixture")
    };
    seed.graph
        .as_mut()
        .unwrap()
        .classes
        .iter_mut()
        .find(|class| class.implementation == ClassKind::O2DSlider)
        .unwrap()
        .properties
        .iter_mut()
        .find(|property| property.id == PropertyId(2))
        .unwrap()
        .number_range = Some([1., 1000.]);
    let ui = decode_ui(&encode_ui(&ui).unwrap()).unwrap();
    let mut doc = Document::new(ui.state_schema.clone(), "oib:test").unwrap();
    assert!(doc.configure_oib(&ui).unwrap());
    (ui, doc)
}
#[test]
fn duplicate_derived_targets_are_rejected_instead_of_writing_a_frame_twice() {
    let (mut ui, doc) = setup();
    let width = occurrence(&doc, 20, 2);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("fixture")
    };
    let cell = seed.cells.iter_mut().find(|c| c.id == width).unwrap();
    cell.targets.push(cell.targets[0].clone());
    assert!(oib::validate_seed(seed).is_err());
}

#[test]
fn first_oib_admission_and_creation_replay_from_a_pre_oib_checkpoint() {
    let ui = decode_ui(&encode_ui(&app::ui(&[])).unwrap()).unwrap();
    let mut doc = Document::new(ui.state_schema.clone(), "oib:test").unwrap();
    let saved = doc.save().unwrap();
    let before = doc.clone();
    doc.configure_oib(&ui).unwrap();
    let mut recovered = Document::load(ui.state_schema.clone(), "oib:test", &saved).unwrap();
    recovered
        .replay(&doc.journal_delta(&before).unwrap())
        .unwrap();
    let admitted = doc.clone();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    admin.instantiate_oib(12, None).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    recovered
        .replay(&doc.journal_delta(&admitted).unwrap())
        .unwrap();
    assert!(!recovered.configure_oib(&ui).unwrap());
    assert_eq!(recovered.root_hash(), doc.root_hash());
    assert_eq!(recovered.oib_allocations().unwrap().len(), 3);
}
fn direct_structure(
    doc: &mut Document,
    session: &mut Session,
    sequence: &mut u64,
    operations: Vec<DocumentEdit>,
) {
    *sequence += 1;
    let bytes = encode_state_request(&StateRequest::OibInstance {
        epoch: doc.oib_epoch().unwrap(),
        sequence: *sequence,
        operation: EditBatch {
            generation: doc.oib_status().unwrap().structural_generation,
            operations,
        },
    })
    .unwrap();
    assert!(doc.receive(session, &bytes).unwrap());
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
}
fn source(doc: &Document) -> DocumentSource {
    let snapshot = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let leaf = snapshot.subtree(&[4, 4]).unwrap();
    flexbuffers::from_slice(leaf.register().unwrap().values().next().unwrap().2.unwrap()).unwrap()
}
fn scroll_inspector(client: &mut Client, ui: &Ui, delta: f32) -> bool {
    let scene = client.scene().unwrap();
    let path = scene
        .path(u32::from(oib::seed(ui).unwrap().inspector_viewport))
        .unwrap();
    let panel = path.last().unwrap();
    let at = [
        panel.origin[0] + (panel.node.frame.width - 4.) * panel.scale,
        panel.origin[1] + panel.node.frame.height * panel.scale / 2.,
    ];
    client.scroll_at(client.epoch(), delta, at).unwrap()
}
fn shared_address(doc: &Document, property: u64) -> PropertyAddress {
    let (ui, _) = doc.oib_compiled(2).unwrap();
    let seed = oib::seed(&ui).unwrap();
    let metadata = seed.designer.as_ref().unwrap();
    let class = metadata
        .classes
        .iter()
        .find(|c| c.implementation == ClassKind::O2DSlider)
        .unwrap()
        .id;
    let definition = metadata
        .definitions
        .iter()
        .find(|d| d.nodes.iter().any(|n| n.id == d.root && n.class == class))
        .unwrap();
    PropertyAddress::Definition {
        definition: definition.id,
        node: definition.root,
        property: PropertyId(property),
    }
}
fn occurrence_address(doc: &Document, instance: u64, property: u64) -> PropertyAddress {
    let (ui, _) = doc.oib_compiled(2).unwrap();
    let metadata = oib::seed(&ui).unwrap().designer.as_ref().unwrap();
    let allocation = doc
        .oib_allocations()
        .unwrap()
        .into_iter()
        .find(|a| a.id == instance)
        .unwrap();
    let definition = metadata
        .definitions
        .iter()
        .find(|d| d.id.0 == allocation.component)
        .unwrap();
    PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: vec![],
            node: definition.root,
            property: PropertyId(property),
        },
    }
}
fn handle(doc: &Document, address: &PropertyAddress) -> u64 {
    let (ui, _) = doc.oib_compiled(2).unwrap();
    oib::document::cell_for_address(
        oib::seed(&ui).unwrap(),
        &doc.oib_allocations().unwrap(),
        address,
    )
    .unwrap()
}
fn shared(doc: &Document, property: u64) -> u64 {
    handle(doc, &shared_address(doc, property))
}
fn occurrence(doc: &Document, instance: u64, property: u64) -> u64 {
    handle(doc, &occurrence_address(doc, instance, property))
}
fn compact_changes(doc: &Document) -> Vec<(u64, Option<Value>)> {
    vec![
        (shared(doc, 2), Some(Value::Number(400.))),
        (shared(doc, 1), Some(Value::Color([255, 136, 85, 255]))),
    ]
}
fn width(client: &Client) -> f32 {
    use otrios_interfaces::scene::TouchResponder;
    client
        .scene()
        .unwrap()
        .controls()
        .into_iter()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap()
        .node
        .frame
        .width
}
fn current_control(doc: &Document, field: u16) -> u8 {
    doc.oib_compiled(2)
        .unwrap()
        .0
        .handlers
        .iter()
        .find(|h| h.kind == GestureKind::Slider && h.fields == [field])
        .unwrap()
        .control
}

#[test]
fn restored_brightness_reset_uses_its_own_current_authored_label() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    for id in [20, 30] {
        admin.remove_oib(id).unwrap();
        sync(&mut doc, &mut sa, &mut admin);
    }
    admin.restore_oib(false).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let allocations = doc.oib_allocations().unwrap();
    let volume = allocations.iter().find(|a| a.prototype == 20).unwrap().id;
    let brightness = allocations.iter().find(|a| a.prototype == 30).unwrap().id;
    assert!(volume > 30 && brightness > 30);
    let vcell = occurrence(&doc, volume, 3);
    let bcell = occurrence(&doc, brightness, 3);
    admin
        .edit_oib(&[
            (vcell, Some(Value::Text("My Volume".into()))),
            (bcell, Some(Value::Text("My Brightness".into()))),
        ])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    admin
        .oib_inspector_action(bcell, otrios_interfaces::oib::InspectorAction::Clear)
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let (compiled, _) = doc.oib_compiled(2).unwrap();
    assert_eq!(
        oib::effective(
            oib::seed(&compiled).unwrap(),
            &doc.oib_values(2).unwrap(),
            bcell
        )
        .unwrap(),
        Value::Text("Brightness".into())
    );
    assert_eq!(
        doc.oib_values(2).unwrap()[&vcell],
        Some(Value::Text("My Volume".into()))
    );
    admin.restore_oib(true).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let (compiled, _) = doc.oib_compiled(2).unwrap();
    for (id, label) in [(volume, "Volume"), (brightness, "Brightness")] {
        assert_eq!(
            oib::effective(
                oib::seed(&compiled).unwrap(),
                &doc.oib_values(2).unwrap(),
                occurrence(&doc, id, 3)
            )
            .unwrap(),
            Value::Text(label.into())
        );
    }
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(400.));
    assert_eq!(doc.typed_value(2).unwrap(), Value::Number(700.));
}

#[test]
fn component_defaults_survive_remove_add_restore_and_reset() {
    let (ui, mut doc) = setup();
    doc.set_typed(&[(1, Value::Number(610.)), (2, Value::Number(570.))])
        .unwrap();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    let epoch = client.epoch();
    assert_eq!(width(&client), 440.);
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(400.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    for id in [20, 30] {
        admin.remove_oib(id).unwrap();
        sync(&mut doc, &mut sa, &mut admin);
    }
    admin
        .instantiate_oib(12, Some("replacement".into()))
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(width(&client), 400.);
    admin.restore_oib(false).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(width(&client), 400.);
    admin.restore_oib(true).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(width(&client), 440.);
    assert_eq!(client.epoch(), epoch);
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(610.));
    assert_eq!(doc.typed_value(2).unwrap(), Value::Number(570.));
}

#[test]
fn restore_authored_bindings_uses_fresh_ids_after_reclamation_and_keeps_design() {
    let (ui, mut doc) = setup_stacked();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    admin.edit_oib(&compact_changes(&doc)).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    doc.set_typed(&[(1, Value::Number(610.)), (2, Value::Number(570.))])
        .unwrap();
    let mut retired = std::collections::BTreeSet::from([20, 30]);
    let (mut churn, _) = doc.open_session(&[90; 16], 0).unwrap();
    churn.set_oib_editable(true);
    let mut sequence = 0;
    for _ in 0..24 {
        direct_structure(
            &mut doc,
            &mut churn,
            &mut sequence,
            vec![DocumentEdit::Hierarchy(HierarchyEdit::Insert {
                prototype: InstanceId(20),
                name: None,
                destination: Destination::Canvas,
                before: None,
                frame: None,
            })],
        );
        let last = doc.oib_allocations().unwrap().last().unwrap().id;
        assert!(retired.insert(last));
        direct_structure(
            &mut doc,
            &mut churn,
            &mut sequence,
            vec![DocumentEdit::Hierarchy(HierarchyEdit::Remove {
                instance: InstanceId(last),
            })],
        );
    }
    sync(&mut doc, &mut sa, &mut admin);
    for id in doc.oib_live(2).unwrap() {
        admin.remove_oib(id).unwrap();
        sync(&mut doc, &mut sa, &mut admin);
    }
    let (_, mut member) = adapter(&ui, &mut doc, 7, false);
    let before = doc.root_hash();
    admin.restore_oib(false).unwrap();
    let request = issue_instance(&mut admin);
    assert!(doc.receive(&mut member, &request).is_err());
    assert_eq!(doc.root_hash(), before);
    doc.receive(&mut sa, &request).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let root = doc.root_hash();
    assert!(!doc.receive(&mut sa, &request).unwrap());
    assert_eq!(doc.root_hash(), root);
    let ledger = doc.oib_allocations().unwrap();
    assert_eq!(ledger.len(), 2);
    assert!(doc.oib_live(2).unwrap().is_disjoint(&retired));
    assert_eq!(
        ledger.iter().map(|a| a.prototype).collect::<Vec<_>>(),
        [20, 30]
    );
    assert_eq!(
        doc.oib_values(2).unwrap()[&shared(&doc, 2)],
        Some(Value::Number(400.))
    );
    let (resolved, _) = doc.oib_compiled(2).unwrap();
    assert_eq!(
        resolved
            .handlers
            .iter()
            .filter(|h| h.kind == GestureKind::Slider)
            .map(|h| h.fields.clone())
            .collect::<Vec<_>>(),
        vec![vec![1], vec![2]]
    );
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(1.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_some());
    admin.restore_oib(true).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
    assert_eq!(doc.oib_allocations().unwrap(), ledger);
    assert_eq!(doc.oib_values(2).unwrap()[&shared(&doc, 2)], None);
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(610.));
    assert_eq!(doc.typed_value(2).unwrap(), Value::Number(570.));
}

#[test]
fn live_geometry_batches_preserve_values_and_style_new_instances() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    client.inspect_oib(30).unwrap();
    let epoch = client.epoch();
    let panel = client.resource_usage().panel_scroll.unwrap().offset;
    admin.edit_oib(&compact_changes(&doc)).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(client.epoch(), epoch);
    assert_eq!(client.resource_usage().panel_scroll.unwrap().offset, panel);
    assert_eq!(
        client.resource_usage().oib.unwrap().selected_instance,
        Some(30)
    );
    assert_eq!(width(&client), 400.);
    let scene = client.scene().unwrap();
    let track = scene
        .controls()
        .into_iter()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let origin = track.origin;
    let extent = track.node.frame.width * track.scale;
    let y = origin[1] + track.node.frame.height * track.scale / 2.;
    for (fraction, expected) in [(0., 0.), (1., 1000.)] {
        for (phase, x) in [
            (PointerPhase::Down, origin[0] + extent / 2.),
            (PointerPhase::Move, origin[0] + extent * fraction),
            (PointerPhase::Up, origin[0] + extent * fraction),
        ] {
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 72,
                        phase,
                        position: [x, y],
                    },
                    300,
                )
                .unwrap();
        }
        pump_client(&mut client, &mut doc, &mut session);
        assert_eq!(doc.typed_value(1).unwrap(), Value::Number(expected));
    }
    admin.duplicate_oib(20, Some("copy".into())).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    let (compiled, _) = doc.oib_compiled(2).unwrap();
    assert_eq!(
        compiled
            .handlers
            .iter()
            .filter(|h| h.kind == GestureKind::Slider)
            .count(),
        3
    );
    let root = doc.oib_status().unwrap().active_root;
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(1.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_some());
    assert_eq!(doc.oib_status().unwrap().active_root, root);
    admin.edit_oib(&compact_changes(&doc)).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
    let mut restored =
        Document::load(ui.state_schema.clone(), "oib:test", &doc.save().unwrap()).unwrap();
    restored.configure_oib(&ui).unwrap();
    assert_eq!(restored.oib_status().unwrap().active_root, root);
}

#[test]
fn live_geometry_activation_waits_for_a_held_slider_without_remounting() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let track = scene
        .controls()
        .into_iter()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let point = [track.origin[0] + 200., track.origin[1] + 24.];
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 91,
                phase: PointerPhase::Down,
                position: point,
            },
            100,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    let old = client.resource_usage().oib.unwrap().presented_generation;
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 9, true);
    admin.edit_oib(&compact_changes(&doc)).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(
        client.resource_usage().oib.unwrap().presented_generation,
        old
    );
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 91,
                phase: PointerPhase::Up,
                position: point,
            },
            200,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    let info = client.resource_usage().oib.unwrap();
    assert_eq!(info.active_generation, info.presented_generation);
    assert!(info.presented_generation > old);
    assert_eq!(client.epoch(), epoch);
}

#[test]
fn identical_layout_seed_presentation_updates_keep_active_roots() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    admin.edit_oib(&compact_changes(&doc)).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let root = doc.oib_status().unwrap().active_root;
    let mut next = ui.clone();
    next.title = "Presentation update".into();
    assert!(doc.configure_oib(&next).unwrap());
    assert_eq!(doc.oib_status().unwrap().active_root, root);
}
fn adapter(ui: &Ui, doc: &mut Document, actor: u8, edit: bool) -> (StateAdapter, Session) {
    let (mut session, _) = doc.open_session(&[actor; 16], 0).unwrap();
    session.set_oib_editable(edit);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(ui, "oib:test", &[], [actor; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(doc, &mut session, &mut client);
    (client, session)
}
fn sync(doc: &mut Document, session: &mut Session, client: &mut StateAdapter) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = client.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            client.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("OIB sync did not settle");
}
fn issue(client: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = client.generate().unwrap() {
        if matches!(
            decode_state_request(&bytes).unwrap(),
            StateRequest::OibWrite { .. }
        ) {
            return bytes;
        }
    }
    panic!("Missing OIB operation");
}
#[test]
fn inherited_properties_overrides_and_causal_clear_survive_checkpoint_and_journal() {
    let (ui, mut doc) = setup();
    let before = doc.clone();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    let accent = Value::Color([255, 136, 85, 255]);
    assert!(
        a.edit_oib(&[(shared(&doc, 1), Some(accent.clone()))])
            .unwrap()
    );
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    let seed = oib::seed(&ui).unwrap();
    for id in [occurrence(&doc, 20, 1), occurrence(&doc, 30, 1)] {
        assert_eq!(
            oib::effective(seed, &b.oib_active().unwrap(), id).unwrap(),
            accent
        );
    }
    assert!(
        b.edit_oib(&[(occurrence(&doc, 20, 1), Some(Value::Color([1, 2, 3, 255])))])
            .unwrap()
    );
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    assert_ne!(
        oib::effective(seed, &a.oib_active().unwrap(), occurrence(&doc, 20, 1)).unwrap(),
        accent
    );
    assert_eq!(
        oib::effective(seed, &a.oib_active().unwrap(), occurrence(&doc, 30, 1)).unwrap(),
        accent
    );
    assert!(a.edit_oib(&[(occurrence(&doc, 20, 1), None)]).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    assert_eq!(
        oib::effective(seed, &a.oib_active().unwrap(), occurrence(&doc, 20, 1)).unwrap(),
        accent
    );
    let mut restored =
        Document::load(ui.state_schema.clone(), "oib:test", &doc.save().unwrap()).unwrap();
    assert!(!restored.configure_oib(&ui).unwrap());
    assert_eq!(restored.oib_values(2).unwrap(), doc.oib_values(2).unwrap());
    let mut replayed = before.clone();
    replayed
        .replay(&doc.journal_delta(&before).unwrap())
        .unwrap();
    assert_eq!(replayed.root_hash(), doc.root_hash());
    assert_eq!(before.oib_values(2).unwrap()[&shared(&before, 1)], None);
    assert!(restored.portable_checkpoint().is_err());
}
#[test]
fn concurrent_values_remain_draft_conflicts_until_an_observing_resolution() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.edit_oib(&[(shared(&doc, 3), Some(Value::Text("Alice".into())))])
        .unwrap();
    b.edit_oib(&[(shared(&doc, 3), Some(Value::Text("Bob".into())))])
        .unwrap();
    let a_bytes = issue(&mut a);
    let b_bytes = issue(&mut b);
    doc.receive(&mut sa, &a_bytes).unwrap();
    let active = doc.oib_values(2).unwrap();
    let generation = doc.oib_status().unwrap().generation;
    doc.receive(&mut sb, &b_bytes).unwrap();
    assert_eq!(
        doc.oib_status().unwrap().conflicts,
        vec![
            otrios_core::canvas::oib::document::address_for_cell(
                otrios_core::canvas::oib::seed(&doc.oib_compiled(2).unwrap().0).unwrap(),
                shared(&doc, 3)
            )
            .unwrap()
        ]
    );
    assert_eq!(doc.oib_values(2).unwrap(), active);
    assert_eq!(doc.oib_status().unwrap().generation, generation);
    assert!(
        !doc.receive(&mut sb, &b_bytes).unwrap(),
        "duplicate receipt"
    );
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    assert_eq!(a.oib_conflicts(), vec![shared(&doc, 3)]);
    b.edit_oib(&[(shared(&doc, 3), Some(Value::Text("Resolved".into())))])
        .unwrap();
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    assert!(a.oib_conflicts().is_empty());
    assert_eq!(
        a.oib_active().unwrap()[&shared(&doc, 3)],
        Some(Value::Text("Resolved".into()))
    );
    assert_eq!(a.oib_active().unwrap(), b.oib_active().unwrap());
}
#[test]
fn disjoint_changes_merge_and_clear_conflicts_with_a_concurrent_override() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.edit_oib(&[(shared(&doc, 1), Some(Value::Color([1, 2, 3, 255])))])
        .unwrap();
    b.edit_oib(&[(shared(&doc, 3), Some(Value::Text("Level B".into())))])
        .unwrap();
    doc.receive(&mut sa, &issue(&mut a)).unwrap();
    doc.receive(&mut sb, &issue(&mut b)).unwrap();
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    a.edit_oib(&[(occurrence(&doc, 20, 1), None)]).unwrap();
    b.edit_oib(&[(occurrence(&doc, 20, 1), Some(Value::Color([9, 8, 7, 255])))])
        .unwrap();
    doc.receive(&mut sa, &issue(&mut a)).unwrap();
    doc.receive(&mut sb, &issue(&mut b)).unwrap();
    assert_eq!(
        doc.oib_status().unwrap().conflicts,
        vec![
            otrios_core::canvas::oib::document::address_for_cell(
                otrios_core::canvas::oib::seed(&doc.oib_compiled(2).unwrap().0).unwrap(),
                occurrence(&doc, 20, 1)
            )
            .unwrap()
        ]
    );
}
#[test]
fn invalid_and_unauthorized_edits_keep_the_active_ui_and_receipts_atomic() {
    let (ui, mut doc) = setup();
    let (mut member, mut sm) = adapter(&ui, &mut doc, 1, false);
    // The viewer never receives draft leaves, and the authority still rejects forged writes.
    assert!(
        member
            .edit_oib(&[(shared(&doc, 3), Some(Value::Text("forged".into())))])
            .is_err()
    );
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 2, true);
    admin
        .edit_oib(&[(shared(&doc, 3), Some(Value::Text("forged".into())))])
        .unwrap();
    let request = issue(&mut admin);
    let before = doc.root_hash();
    assert!(doc.receive(&mut sm, &request).is_err());
    assert_eq!(doc.root_hash(), before);
    doc.receive(&mut sa, &request).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let active = doc.oib_values(2).unwrap();
    let generation = admin.oib_generation();
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(1.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_some());
    assert_eq!(doc.oib_values(2).unwrap(), active);
    assert_eq!(admin.oib_generation(), generation);
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(400.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
    let before = doc.root_hash();
    assert!(
        admin
            .edit_oib(&[(shared(&doc, 3), Some(Value::Number(1.)))])
            .is_err()
    );
    assert!(
        admin
            .edit_oib(&[
                (shared(&doc, 3), Some(Value::Text("one".into()))),
                (shared(&doc, 3), Some(Value::Text("two".into()))),
            ])
            .is_err()
    );
    assert!(
        admin.confirmed(),
        "Rejected local intents cannot issue a flight"
    );
    assert!(
        admin
            .edit_oib(&[(9999, Some(Value::Text("unknown".into())))])
            .is_err()
    );
    assert_eq!(doc.root_hash(), before);
    let mut changed = ui.clone();
    if let Some(Presentation::Oib(seed)) = &mut changed.presentation {
        seed.graph
            .as_mut()
            .unwrap()
            .definitions
            .iter_mut()
            .find(|definition| definition.id == DefinitionId(12))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|property| property.property == PropertyId(3))
            .unwrap()
            .value = Value::Text("Wrong seed".into());
    }
    assert!(doc.configure_oib(&changed).is_err());
}
#[test]
fn observed_register_join_converges_without_erasing_concurrent_assignments() {
    use otrios_core::merkle::{Edit, Snapshot};
    let initial = Snapshot::empty()
        .edit(
            [0; 16],
            &[Edit {
                path: vec![1],
                schema: [7; 32],
                value: Some(vec![0]),
            }],
        )
        .unwrap()
        .0;
    let observed = initial.subtree(&[1]).unwrap();
    let a = initial
        .edit_observed(
            [1; 16],
            &Edit {
                path: vec![1],
                schema: [7; 32],
                value: Some(vec![1]),
            },
            &observed,
        )
        .unwrap();
    let b = initial
        .edit_observed(
            [2; 16],
            &Edit {
                path: vec![1],
                schema: [7; 32],
                value: None,
            },
            &observed,
        )
        .unwrap();
    let merged = a.merge(&b).unwrap().0;
    assert_eq!(merged.hash(), b.merge(&a).unwrap().0.hash());
    assert_eq!(merged.hash(), merged.merge(&merged).unwrap().0.hash());
    assert_eq!(
        merged
            .subtree(&[1])
            .unwrap()
            .register()
            .unwrap()
            .values()
            .count(),
        2
    );
    assert_eq!(
        merged.merge(&initial).unwrap().0.hash(),
        initial.merge(&merged).unwrap().0.hash()
    );
}

fn mounted_client(ui: &Ui, doc: &mut Document) -> (Client, Session) {
    let mut client = Client::default();
    let bytes = encode_ui(ui).unwrap();
    let mut app = app::manifest();
    app.state_ref = "oib:test".into();
    client
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app,
                    path: "live-ui".into(),
                    ui_hash: hash(&bytes),
                    private: false,
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client.open(&ui.ui_ref, "oib:test");
    let ClientMessage::Mount { epoch, writer, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount")
    };
    let (mut session, _) = doc.open_session(&writer, 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui: bytes.clone(),
                ui_hash: hash(&bytes),
                code: vec![],
                state: doc.generate(&mut session).unwrap().unwrap(),
                binding: None,
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::AcceptMount { .. }
    ));
    pump_client(&mut client, doc, &mut session);
    // Runtime pointer tests exercise the app, while Edit mode deliberately
    // intercepts the same pointers for authoring selection/manipulation.
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    (client, session)
}
fn pump_client(client: &mut Client, doc: &mut Document, session: &mut Session) {
    for i in 0..30 {
        let mut progress = false;
        client.tick(1000 + i * 33, 0).unwrap();
        while let Some(bytes) = client.outgoing().unwrap() {
            if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes).unwrap() {
                doc.receive(session, &bytes).unwrap();
                progress = true;
            }
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
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
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("Client did not settle");
}
#[test]
fn shared_scene_edits_keep_mount_identity_and_defer_geometry_until_slider_release() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, Primitive, TouchResponder};
    let (ui, mut doc) = setup_stacked();
    let (mut a, mut sa) = mounted_client(&ui, &mut doc);
    let (mut b, mut sb) = mounted_client(&ui, &mut doc);
    let epoch = a.epoch();
    let scene = a.scene().unwrap();
    let slider = scene
        .controls()
        .into_iter()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let id = slider.node.id;
    let position = [slider.origin[0] + 40., slider.origin[1] + 20.];
    a.pointer(
        epoch,
        PointerEvent {
            id: 1,
            phase: PointerPhase::Down,
            position,
        },
    )
    .unwrap();
    b.edit_oib(&[
        (shared(&doc, 2), Some(Value::Number(400.))),
        (
            occurrence(&doc, 20, 3),
            Some(Value::Text("New label".into())),
        ),
    ])
    .unwrap();
    pump_client(&mut b, &mut doc, &mut sb);
    pump_client(&mut a, &mut doc, &mut sa);
    let info = a.resource_usage().oib.unwrap();
    assert!(info.active_generation > info.presented_generation);
    assert_eq!(
        a.scene()
            .unwrap()
            .path(id)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        520.
    );
    a.pointer(
        epoch,
        PointerEvent {
            id: 1,
            phase: PointerPhase::Up,
            position,
        },
    )
    .unwrap();
    pump_client(&mut a, &mut doc, &mut sa);
    let info = a.resource_usage().oib.unwrap();
    assert_eq!(info.active_generation, info.presented_generation);
    assert_eq!(a.epoch(), epoch);
    assert_eq!(
        a.scene()
            .unwrap()
            .path(id)
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        400.
    );
    let has_label = |scene: &otrios_core::dashboard::scene::Scene| {
        fn find(n: &otrios_interfaces::scene::SceneNode) -> bool {
            matches!(&n.primitive,Primitive::Text {value,..} if value=="New label")
                || n.children.iter().any(find)
        }
        find(scene.root())
    };
    assert!(has_label(&a.scene().unwrap()));
    assert!(has_label(&b.scene().unwrap()));
    assert_eq!(a.typed_value(1).unwrap(), doc.typed_value(1).unwrap());
    assert_eq!(a.typed_value(2).unwrap(), Value::Number(700.));
}
#[test]
fn inspector_scroll_is_local_and_visible_buttons_remain_clickable() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let (ui, mut doc) = setup();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    client.inspect_oib(20).unwrap();
    let epoch = client.epoch();
    let generation = client.resource_usage().oib.unwrap().active_generation;
    let scene = client.scene().unwrap();
    let button = scene
        .controls()
        .into_iter()
        .find(|p| {
            p.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Show property info")
        })
        .unwrap();
    let position = [button.origin[0] + 20., button.origin[1] + 20.];
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        client
            .pointer(
                epoch,
                PointerEvent {
                    id: 9,
                    phase,
                    position,
                },
            )
            .unwrap();
    }
    pump_client(&mut client, &mut doc, &mut session);
    assert_eq!(
        client.resource_usage().oib.unwrap().active_generation,
        generation
    );
    let before = doc.root_hash();
    assert!(client.scroll_map(epoch, 64.).unwrap());
    assert_eq!(doc.root_hash(), before);
    for (phase, y) in [
        (PointerPhase::Down, 550.),
        (PointerPhase::Move, 270.),
        (PointerPhase::Up, 270.),
    ] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 10,
                    phase,
                    position: [1200., y],
                },
                2_000_000_000,
            )
            .unwrap();
    }
    assert!(client.resource_usage().panel_scroll.unwrap().offset[1] > 0.);
    assert_eq!(doc.root_hash(), before, "panning cannot mutate shared UI");
    client.set_viewport([800., 1280.]).unwrap();
    assert_eq!(client.epoch(), epoch);
    assert!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .all(|p| p.origin[1] >= 0.)
    );
}

#[test]
fn inspector_panning_retains_uploaded_content_including_initially_hidden_buttons() {
    use std::sync::Arc;
    let (ui, mut doc) = setup();
    let (mut client, _) = mounted_client(&ui, &mut doc);
    assert!(client.inspect_oib(30).unwrap());
    scroll_inspector(&mut client, &ui, -10_000.);
    let font = otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut composer = presentation::Composer::default();
    let before = composer.compose(&mut client, &font).unwrap().clone();
    let vertices = before.base.mesh.vertices.clone();
    let glyphs = before.base.mesh.glyph_vertices.clone();
    let info = client.resource_usage().oib.unwrap();
    let mut retained = 0;
    for _ in 0..50 {
        let previous = composer.frame().unwrap().clone();
        let moved = scroll_inspector(&mut client, &ui, 20.);
        let frame = composer.compose(&mut client, &font).unwrap();
        assert!(
            Arc::ptr_eq(&before.base.mesh, &frame.base.mesh),
            "inspector pans retain app geometry"
        );
        if Arc::ptr_eq(
            &previous.overlay.as_ref().unwrap().mesh,
            &frame.overlay.as_ref().unwrap().mesh,
        ) {
            retained += usize::from(moved);
            assert_eq!(previous.revisions.content, frame.revisions.content);
        } else {
            // Canonical Designer bounds retained chrome in coarse scrolling
            // slabs. Crossing a slab may regenerate overlay content once.
            assert!(frame.revisions.content > previous.revisions.content);
        }
        assert_eq!(
            client.resource_usage().oib.unwrap().properties,
            info.properties
        );
    }
    assert!(
        retained > 0,
        "pans within retained slabs reuse uploaded chrome"
    );
    let after = composer.frame().unwrap();
    assert!(
        before.overlay.as_ref().unwrap().views != after.overlay.as_ref().unwrap().views,
        "pointer-routed inspector pan changes placements/clips"
    );
    assert_eq!(
        before.base.mesh.vertices, vertices,
        "Pinned frames stay immutable"
    );
    assert_eq!(before.base.mesh.glyph_vertices, glyphs);
    // At the lower bound the previously hidden brightness controls are drawn,
    // clipped and interactive, without another content generation.
    let scene = client.scene().unwrap();
    assert!(scene.controls().iter().any(|c| {
        c.node
            .semantics
            .as_ref()
            .is_some_and(|s| s.label == "Apply")
    }));
    assert!(
        after
            .base
            .views
            .chunks_exact(8)
            .any(|v| v[1] > 0. && v[6] > v[4] && v[7] > v[5])
    );
}

#[test]
fn slider_feedback_keeps_inspector_content_and_saving_status_unchanged() {
    use std::sync::Arc;
    let (ui, mut doc) = setup();
    let (mut client, _) = mounted_client(&ui, &mut doc);
    assert!(client.inspect_oib(30).unwrap());
    scroll_inspector(&mut client, &ui, -10_000.);
    let font = otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )
    .unwrap();
    let mut composer = presentation::Composer::default();
    let before = composer.compose(&mut client, &font).unwrap().clone();
    let status = client.resource_usage().oib.unwrap().status;
    let control = current_control(&doc, 1);
    client.event(client.epoch(), control, 650, 0).unwrap();
    let after = composer.compose(&mut client, &font).unwrap();
    assert!(!Arc::ptr_eq(&before.base.mesh, &after.base.mesh));
    assert!(Arc::ptr_eq(
        &before.overlay.as_ref().unwrap().mesh,
        &after.overlay.as_ref().unwrap().mesh
    ));
    assert_eq!(client.resource_usage().oib.unwrap().status, status);
    assert_eq!(client.typed_value(1).unwrap(), Value::Number(650.));
}

#[test]
fn slider_captures_reach_both_endpoints_even_outside_the_track() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    for viewport in [[1280., 800.], [800., 1280.]] {
        let (ui, mut doc) = setup();
        let (mut client, mut session) = mounted_client(&ui, &mut doc);
        client.set_viewport(viewport).unwrap();
        for field in [1, 2] {
            let control = current_control(&doc, field);
            let scene = client.scene().unwrap();
            let track = scene
                .controls()
                .into_iter()
                .find(|c| {
                    c.node.responder
                        == Some(TouchResponder::Drag(
                            otrios_interfaces::scene::Action::AppEvent(control),
                        ))
                })
                .unwrap();
            let origin = track.origin;
            let width = track.node.frame.width * track.scale;
            let y = origin[1] + track.node.frame.height * track.scale / 2.;
            let epoch = client.epoch();
            let mut at = 20_000;
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 11,
                        phase: PointerPhase::Down,
                        position: [origin[0] + width / 2., y],
                    },
                    at,
                )
                .unwrap();
            for (fraction, expected) in [
                (0.15, 150.),
                (0.05, 50.),
                (0., 0.),
                (-0.05, 0.),
                (1., 1000.),
                (1.05, 1000.),
            ] {
                at += 50;
                client
                    .pointer_at(
                        epoch,
                        PointerEvent {
                            id: 11,
                            phase: PointerPhase::Move,
                            position: [origin[0] + width * fraction, y],
                        },
                        at,
                    )
                    .unwrap();
                assert_eq!(client.typed_value(field).unwrap(), Value::Number(expected));
                client.scene().unwrap();
            }
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 11,
                        phase: PointerPhase::Up,
                        position: [origin[0] + width * 1.05, y],
                    },
                    at + 100,
                )
                .unwrap();
            pump_client(&mut client, &mut doc, &mut session);
            assert_eq!(doc.typed_value(field).unwrap(), Value::Number(1000.));
        }
    }
}

fn issue_instance(client: &mut StateAdapter) -> Vec<u8> {
    while let Some(bytes) = client.generate().unwrap() {
        if matches!(
            decode_state_request(&bytes).unwrap(),
            StateRequest::OibInstance { .. }
        ) {
            return bytes;
        }
    }
    panic!("Missing instance intent")
}
#[test]
fn instance_creation_duplication_inheritance_and_removal_share_typed_operations() {
    use otrios_interfaces::oib::InstanceAction;
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    let before = doc.clone();
    assert!(a.instantiate_oib(12, Some("gain".into())).unwrap());
    let request = issue_instance(&mut a);
    doc.receive(&mut sa, &request).unwrap();
    assert!(
        !doc.receive(&mut sa, &request).unwrap(),
        "retry cannot allocate a second object"
    );
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let allocations = doc.oib_allocations().unwrap();
    let gain = allocations[2].id;
    assert_eq!(allocations[2].name, "gain");
    let seed = oib::seed(&ui).unwrap();
    let label = occurrence(&doc, gain, 3);
    let accent = occurrence(&doc, gain, 1);
    b.edit_oib_property(
        &occurrence_address(&doc, gain, 3),
        Some(Value::Text("Gain".into())),
    )
    .unwrap();
    sync(&mut doc, &mut sb, &mut b);
    sync(&mut doc, &mut sa, &mut a);
    a.edit_oib(&[(shared(&doc, 1), Some(Value::Color([1, 2, 3, 255])))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut a);
    sync(&mut doc, &mut sb, &mut b);
    let expanded = oib::instances::expanded_seed(seed, &allocations).unwrap();
    assert_eq!(
        oib::effective(&expanded, &b.oib_active().unwrap(), accent).unwrap(),
        Value::Color([1, 2, 3, 255])
    );
    assert_eq!(
        oib::effective(&expanded, &b.oib_active().unwrap(), label).unwrap(),
        Value::Text("Gain".into())
    );
    assert!(
        a.oib_instance_action(gain, InstanceAction::Duplicate)
            .unwrap()
    );
    sync(&mut doc, &mut sa, &mut a);
    let all = doc.oib_allocations().unwrap();
    let copy = all[3].id;
    let copied_label = occurrence(&doc, copy, 3);
    assert_eq!(
        doc.oib_values(2).unwrap()[&copied_label],
        Some(Value::Text("Gain".into()))
    );
    let (resolved, _) = doc.oib_compiled(2).unwrap();
    assert_eq!(
        resolved
            .handlers
            .iter()
            .filter(|h| h.kind == GestureKind::Slider)
            .count(),
        4
    );
    assert!(a.remove_oib(gain).unwrap());
    sync(&mut doc, &mut sa, &mut a);
    assert!(!doc.oib_live(2).unwrap().contains(&gain));
    assert!(doc.oib_live(2).unwrap().contains(&copy));
    assert_eq!(
        doc.oib_allocations().unwrap().len(),
        3,
        "deleted allocation is reclaimed"
    );
    let mut restored =
        Document::load(ui.state_schema.clone(), "oib:test", &doc.save().unwrap()).unwrap();
    assert!(!restored.configure_oib(&ui).unwrap());
    assert_eq!(restored.oib_live(2).unwrap(), doc.oib_live(2).unwrap());
    let mut replayed = before;
    replayed
        .replay(&doc.journal_delta(&replayed).unwrap())
        .unwrap();
    assert_eq!(replayed.root_hash(), doc.root_hash());
}
#[test]
fn removing_an_instance_wins_over_a_concurrent_property_edit_without_reusing_ids() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.remove_oib(20).unwrap();
    b.edit_oib(&[(
        occurrence(&doc, 20, 3),
        Some(Value::Text("stale edit".into())),
    )])
    .unwrap();
    let remove = issue_instance(&mut a);
    let edit = issue(&mut b);
    doc.receive(&mut sa, &remove).unwrap();
    doc.receive(&mut sb, &edit).unwrap();
    assert!(!doc.oib_live(2).unwrap().contains(&20));
    assert!(doc.oib_status().unwrap().error.is_none());
    sync(&mut doc, &mut sa, &mut a);
    a.instantiate_oib(12, Some("replacement".into())).unwrap();
    sync(&mut doc, &mut sa, &mut a);
    let all = doc.oib_allocations().unwrap();
    assert_ne!(all.last().unwrap().id, 20);
    let (resolved, _) = doc.oib_compiled(2).unwrap();
    assert!(
        !oib::seed(&resolved)
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .instances
            .iter()
            .any(|i| i.id == InstanceId(20))
    );
    assert!(
        resolved
            .handlers
            .iter()
            .any(|h| h.kind == GestureKind::Slider && h.fields == [1])
    );
}
#[test]
fn concurrent_structural_inserts_reject_stale_generations_without_aliasing_names() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (mut b, mut sb) = adapter(&ui, &mut doc, 2, true);
    a.instantiate_oib(12, Some("gain".into())).unwrap();
    b.instantiate_oib(12, Some("gain".into())).unwrap();
    doc.receive(&mut sa, &issue_instance(&mut a)).unwrap();
    let active = doc.oib_live(2).unwrap();
    let root = doc.oib_status().unwrap().active_root;
    doc.receive(&mut sb, &issue_instance(&mut b)).unwrap();
    assert_eq!(doc.oib_status().unwrap().active_root, root);
    assert_eq!(doc.oib_live(2).unwrap(), active);
    assert!(doc.oib_status().unwrap().error.is_some());
    sync(&mut doc, &mut sb, &mut b);
    b.instantiate_oib(12, Some("gain".into())).unwrap();
    sync(&mut doc, &mut sb, &mut b);
    assert!(doc.oib_status().unwrap().error.is_none());
    let allocations = doc.oib_allocations().unwrap();
    let equal = allocations
        .iter()
        .filter(|a| a.name == "gain")
        .collect::<Vec<_>>();
    assert_eq!(equal.len(), 2);
    assert_ne!(equal[0].id, equal[1].id);
}

#[test]
fn delayed_edits_remove_and_duplicate_cannot_address_a_reused_renderer_slot() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    admin.instantiate_oib(12, Some("temporary".into())).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let old = doc.oib_allocations().unwrap().pop().unwrap();
    let old_cell = occurrence(&doc, old.id, 3);
    let pinned = doc.clone();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 2, true);
    let (mut remover, mut sr) = adapter(&ui, &mut doc, 3, true);
    let (mut copier, mut sc) = adapter(&ui, &mut doc, 4, true);
    editor
        .edit_oib(&[(old_cell, Some(Value::Text("Delayed".into())))])
        .unwrap();
    remover.remove_oib(old.id).unwrap();
    copier
        .duplicate_oib(old.id, Some("stale-copy".into()))
        .unwrap();
    let edit = issue(&mut editor);
    let remove = issue_instance(&mut remover);
    let copy = issue_instance(&mut copier);
    admin.remove_oib(old.id).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    admin.instantiate_oib(12, Some("temporary".into())).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let new = doc.oib_allocations().unwrap().pop().unwrap();
    let new_cell = occurrence(&doc, new.id, 3);
    assert!(new.id > old.id);
    assert_eq!(new.slot, old.slot);
    assert_eq!(new_cell, old_cell, "compiler handles may be reused");
    assert_ne!(
        doc.oib_property_leaf(1, new_cell)
            .unwrap()
            .register()
            .unwrap()
            .schema(),
        pinned
            .oib_property_leaf(1, old_cell)
            .unwrap()
            .register()
            .unwrap()
            .schema()
    );
    let active = doc.oib_status().unwrap().active_root;
    let generation = doc.oib_status().unwrap().generation;
    for (session, request) in [(&mut se, &edit), (&mut sr, &remove), (&mut sc, &copy)] {
        assert!(doc.receive(session, request).unwrap());
        assert!(
            !doc.receive(session, request).unwrap(),
            "discarded operations are acknowledged idempotently"
        );
        assert_eq!(doc.oib_status().unwrap().active_root, active);
        assert_eq!(doc.oib_status().unwrap().generation, generation);
        assert_eq!(doc.oib_allocations().unwrap().len(), 3);
        assert!(
            otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap())
                .unwrap()
                .subtree(&[4, 1, 1, old.id, 0, 1, 3])
                .is_err()
        );
        assert_eq!(doc.oib_values(1).unwrap()[&new_cell], None);
    }
    assert!(pinned.oib_values(1).unwrap().contains_key(&old_cell));
    pinned.oib_compiled(2).unwrap();
    doc.oib_compiled(2).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    assert!(editor.confirmed());
}

#[test]
fn invalid_draft_rejects_new_objects_without_consuming_identities() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    let active = doc.oib_status().unwrap().active_root;
    let roster = doc.oib_allocations().unwrap();
    let next = source(&doc).next_identity;
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(1.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    for _ in 0..12 {
        assert!(admin.instantiate_oib(12, Some("pending".into())).unwrap());
        sync(&mut doc, &mut sa, &mut admin);
        assert!(doc.oib_status().unwrap().error.is_some());
        assert_eq!(doc.oib_status().unwrap().active_root, active);
        assert_eq!(doc.oib_allocations().unwrap(), roster);
        assert_eq!(source(&doc).next_identity, next);
    }
    admin.edit_oib(&[(shared(&doc, 2), None)]).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
}

#[test]
fn invalid_restore_retains_the_active_roster_and_does_not_consume_fresh_ids() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    admin.instantiate_oib(12, Some("extra".into())).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    admin.remove_oib(20).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    let roster = doc.oib_allocations().unwrap();
    let active = doc.oib_status().unwrap().active_root;
    let next = source(&doc).next_identity;
    admin
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(1.)))])
        .unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    admin.restore_oib(false).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(admin.confirmed());
    assert_eq!(doc.oib_allocations().unwrap(), roster);
    assert_eq!(source(&doc).next_identity, next);
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    assert!(doc.oib_status().unwrap().error.is_some());
    admin.restore_oib(true).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    assert!(doc.oib_status().unwrap().error.is_none());
    let allocations = doc.oib_allocations().unwrap();
    assert_eq!(allocations.len(), 2);
    assert_eq!(
        allocations[0].id, next,
        "rejected restore consumes no identities"
    );
    assert_eq!(
        allocations.iter().map(|a| a.prototype).collect::<Vec<_>>(),
        [20, 30]
    );
}
#[test]
fn instance_requests_are_authorized_and_budgeted_before_allocation() {
    let (ui, mut doc) = setup();
    let (mut admin, mut sa) = adapter(&ui, &mut doc, 1, true);
    let (_, mut member) = adapter(&ui, &mut doc, 2, false);
    admin.instantiate_oib(12, None).unwrap();
    let wire = issue_instance(&mut admin);
    let before = doc.root_hash();
    assert!(doc.receive(&mut member, &wire).is_err());
    assert_eq!(doc.root_hash(), before);
    let request = decode_state_request(&wire).unwrap();
    assert_eq!(
        decode_state_request(&encode_state_request(&request).unwrap()).unwrap(),
        request
    );
    doc.receive(&mut sa, &wire).unwrap();
    sync(&mut doc, &mut sa, &mut admin);
    while doc.oib_live(2).unwrap().len() < otrios_interfaces::oib::MAX_LIVE_INSTANCES {
        assert!(admin.instantiate_oib(12, None).unwrap());
        sync(&mut doc, &mut sa, &mut admin);
    }
    assert!(!admin.instantiate_oib(12, None).unwrap());
    for id in doc.oib_live(2).unwrap() {
        doc.oib_compiled_selected(2, Some(id)).unwrap();
    }
}

#[test]
fn structural_activation_waits_for_a_held_slider_and_preserves_local_inspection() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    assert!(client.inspect_oib(30).unwrap());
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let slider = scene
        .controls()
        .into_iter()
        .find(|c| matches!(c.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let start = [
        slider.origin[0] + slider.node.frame.width * slider.scale / 2.,
        slider.origin[1] + 24. * slider.scale,
    ];
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 21,
                phase: PointerPhase::Down,
                position: start,
            },
            100,
        )
        .unwrap();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 9, true);
    editor.instantiate_oib(12, Some("gain".into())).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    pump_client(&mut client, &mut doc, &mut session);
    let info = client.resource_usage().oib.unwrap();
    assert!(info.active_generation > info.presented_generation);
    assert_eq!(info.selected_instance, Some(30));
    assert_eq!(client.epoch(), epoch);
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 21,
                phase: PointerPhase::Move,
                position: [start[0] + 40., start[1]],
            },
            200,
        )
        .unwrap();
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 21,
                phase: PointerPhase::Up,
                position: [start[0] + 40., start[1]],
            },
            250,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    client.tick(500, 0).unwrap();
    let info = client.resource_usage().oib.unwrap();
    assert_eq!(info.active_generation, info.presented_generation);
    assert_eq!(info.selected_instance, Some(30));
    assert_eq!(client.epoch(), epoch);
    assert!(!client.input_busy());
    assert_eq!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .filter(|c| matches!(c.node.responder, Some(TouchResponder::Drag(_))))
            .count(),
        3,
        "the taller preview exposes the third slider after activation"
    );
}
#[test]
fn reclaimed_slot_and_deleted_inspector_wait_for_the_held_gesture() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase, TouchResponder};
    let (ui, mut doc) = setup();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 9, true);
    editor
        .instantiate_oib(12, Some("old-control".into()))
        .unwrap();
    sync(&mut doc, &mut se, &mut editor);
    let old = doc.oib_allocations().unwrap().pop().unwrap();
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    client.inspect_oib(old.id).unwrap();
    let epoch = client.epoch();
    let generation = client.resource_usage().oib.unwrap().presented_generation;
    let scene = client.scene().unwrap();
    let track = scene
        .controls()
        .into_iter()
        .rev()
        .find(|p| matches!(p.node.responder, Some(TouchResponder::Drag(_))))
        .unwrap();
    let at = [track.origin[0] + 100., track.origin[1] + 24.];
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 61,
                phase: PointerPhase::Down,
                position: at,
            },
            100,
        )
        .unwrap();
    editor.remove_oib(old.id).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    editor
        .instantiate_oib(12, Some("replacement".into()))
        .unwrap();
    sync(&mut doc, &mut se, &mut editor);
    let new = doc.oib_allocations().unwrap().pop().unwrap();
    assert_eq!(old.slot, new.slot);
    assert_ne!(old.id, new.id);
    pump_client(&mut client, &mut doc, &mut session);
    let held = client.resource_usage().oib.unwrap();
    assert_eq!(held.presented_generation, generation);
    assert!(held.active_generation > generation);
    assert_eq!(held.selected_instance, Some(old.id));
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 61,
                phase: PointerPhase::Up,
                position: at,
            },
            200,
        )
        .unwrap();
    pump_client(&mut client, &mut doc, &mut session);
    let done = client.resource_usage().oib.unwrap();
    assert_eq!(done.active_generation, done.presented_generation);
    assert_ne!(done.selected_instance, Some(old.id));
    assert_eq!(client.epoch(), epoch);
    assert!(!client.input_busy());
}

#[test]
fn local_inspector_selection_and_preview_scrolling_do_not_write_shared_state() {
    let (ui, mut doc) = setup();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 2, true);
    // Three compact rows fit without scrolling; make content overflow explicitly.
    editor
        .edit_oib(&[(shared(&doc, 2), Some(Value::Number(400.)))])
        .unwrap();
    sync(&mut doc, &mut se, &mut editor);
    for name in ["gain", "extra"] {
        editor.instantiate_oib(12, Some(name.into())).unwrap();
        sync(&mut doc, &mut se, &mut editor);
    }
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    pump_client(&mut client, &mut doc, &mut session);
    let root = doc.root_hash();
    assert!(client.inspect_oib(30).unwrap());
    assert_eq!(doc.root_hash(), root);
    let viewport = oib::seed(&ui)
        .unwrap()
        .instance_surface
        .as_ref()
        .unwrap()
        .preview_viewport;
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(viewport)).unwrap();
    let p = path.last().unwrap();
    let position = [
        p.origin[0] + p.node.frame.width * p.scale / 2.,
        p.origin[1] + p.node.frame.height * p.scale / 2.,
    ];
    let inspector = client.resource_usage().panel_scroll.unwrap().offset;
    assert!(client.scroll_at(client.epoch(), 200., position).unwrap());
    let usage = client.resource_usage();
    assert!(usage.oib_preview_scroll.unwrap().offset[1] > 0.);
    assert_eq!(usage.panel_scroll.unwrap().offset, inspector);
    assert_eq!(usage.oib.unwrap().selected_instance, Some(30));
    assert_eq!(doc.root_hash(), root);
    assert!(client.outgoing().unwrap().is_none());

    // Preview card background has no responder in the viewport, but must start a touch
    // pan. A wheel-only check cannot exercise this routing decision.
    let background = [
        p.origin[0] + (p.node.frame.width - 4.) * p.scale,
        p.origin[1] + (p.node.frame.height - 8.) * p.scale,
    ];
    let background_scene = client.scene().unwrap();
    let hit = background_scene.hit_path(background);
    assert!(!hit.iter().any(|p| p.node.id == u32::from(viewport)));
    assert!(hit.iter().all(|p| p.node.responder.is_none()));
    client.scroll_at(client.epoch(), -1000., position).unwrap();
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    for (phase, y, at) in [
        (PointerPhase::Down, background[1], 100),
        (PointerPhase::Move, background[1] - 100., 200),
        (PointerPhase::Up, background[1] - 100., 220),
    ] {
        assert!(
            client
                .pointer_at(
                    client.epoch(),
                    PointerEvent {
                        id: 71,
                        phase,
                        position: [background[0], y],
                    },
                    at,
                )
                .unwrap()
        );
    }
    let usage = client.resource_usage();
    assert!(usage.oib_preview_scroll.unwrap().offset[1] > 0.);
    assert_eq!(usage.panel_scroll.unwrap().offset, inspector);
    assert_eq!(usage.oib.unwrap().selected_instance, Some(30));
    assert_eq!(doc.root_hash(), root);
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn removed_objects_do_not_accumulate_and_an_empty_preview_can_be_repopulated() {
    let (_ui, mut doc) = setup();
    let (mut writer, _) = doc.open_session(&[91; 16], 0).unwrap();
    writer.set_oib_editable(true);
    let mut sequence = 0;
    direct_structure(
        &mut doc,
        &mut writer,
        &mut sequence,
        vec![20, 30]
            .into_iter()
            .map(|id| {
                DocumentEdit::Hierarchy(HierarchyEdit::Remove {
                    instance: InstanceId(id),
                })
            })
            .collect(),
    );
    let baseline = doc.save().unwrap().len();
    let remaining = doc.oib_values(1).unwrap().len();
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..64 {
        direct_structure(
            &mut doc,
            &mut writer,
            &mut sequence,
            vec![DocumentEdit::Hierarchy(HierarchyEdit::Insert {
                prototype: InstanceId(20),
                name: None,
                destination: Destination::Canvas,
                before: None,
                frame: None,
            })],
        );
        let allocation = doc.oib_allocations().unwrap().pop().unwrap();
        assert!(ids.insert(allocation.id));
        assert_eq!(allocation.slot, 0, "renderer slot is reusable");
        doc.oib_compiled_selected(2, Some(allocation.id)).unwrap();
        direct_structure(
            &mut doc,
            &mut writer,
            &mut sequence,
            vec![DocumentEdit::Hierarchy(HierarchyEdit::Remove {
                instance: InstanceId(allocation.id),
            })],
        );
        assert!(doc.oib_allocations().unwrap().is_empty());
        assert_eq!(doc.oib_values(1).unwrap().len(), remaining);
        assert_eq!(doc.oib_values(2).unwrap().len(), remaining);
        assert!(doc.save().unwrap().len() <= baseline + 128);
    }
    assert!(doc.oib_live(2).unwrap().is_empty());
    direct_structure(
        &mut doc,
        &mut writer,
        &mut sequence,
        vec![DocumentEdit::Hierarchy(HierarchyEdit::Insert {
            prototype: InstanceId(20),
            name: None,
            destination: Destination::Canvas,
            before: None,
            frame: None,
        })],
    );
    assert_eq!(doc.oib_allocations().unwrap().len(), 1);
}

#[test]
fn reconnect_keeps_local_inspector_selection_and_both_scroll_offsets() {
    let (ui, mut doc) = setup();
    let (mut editor, mut se) = adapter(&ui, &mut doc, 2, true);
    editor.instantiate_oib(12, None).unwrap();
    sync(&mut doc, &mut se, &mut editor);
    let (mut client, mut session) = mounted_client(&ui, &mut doc);
    pump_client(&mut client, &mut doc, &mut session);
    client.inspect_oib(30).unwrap();
    let surface = oib::seed(&ui).unwrap().instance_surface.as_ref().unwrap();
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(surface.preview_viewport)).unwrap();
    let p = path.last().unwrap();
    client
        .scroll_at(
            client.epoch(),
            150.,
            [p.origin[0] + 40., p.origin[1] + 100.],
        )
        .unwrap();
    let before = client.resource_usage();
    let inspector = before.panel_scroll.unwrap().offset;
    let preview = before.oib_preview_scroll.unwrap().offset;
    client.reset_connection();
    let ClientMessage::Mount {
        epoch,
        writer,
        resume,
        ..
    } = decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("Mount");
    };
    let (mut next, _) = doc.open_session(&writer, resume).unwrap();
    next.set_oib_editable(true);
    next.set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let bytes = encode_ui(&ui).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: doc.generate(&mut next).unwrap().unwrap(),
                binding: None,
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::AcceptMount { .. }
    ));
    pump_client(&mut client, &mut doc, &mut next);
    let after = client.resource_usage();
    assert_eq!(after.oib.unwrap().selected_instance, Some(30));
    assert_eq!(after.panel_scroll.unwrap().offset, inspector);
    assert_eq!(after.oib_preview_scroll.unwrap().offset, preview);
}

#[test]
fn sparse_layout_events_are_versioned_and_keep_the_host_reservation() {
    use otrios_core::dashboard::layout::Layout;
    use otrios_interfaces::layout::{Responder, TemplateNode};
    fn control(root: &mut TemplateNode, value: u8) -> bool {
        if let Some(Responder::AppEvent { control, .. }) = &mut root.responder {
            *control = value;
            return true;
        }
        root.children.iter_mut().any(|c| control(c, value))
    }
    let (mut ui, _) = setup();
    // The first factory control is the optional heading; admit it for this check.
    let values = std::collections::BTreeMap::new();
    let mut blueprint = Layout::from_bytes(&ui.layout).unwrap().blueprint().clone();
    blueprint.schema = "otrios-layout-v4".into();
    assert!(control(&mut blueprint.templates[0].root, 200));
    ui.layout = Layout::encode(&blueprint).unwrap();
    assert!(oib::compile(&ui, &values).is_err());
    blueprint.schema = otrios_interfaces::layout::SPARSE_EVENTS_FORMAT.into();
    ui.layout = Layout::encode(&blueprint).unwrap();
    oib::compile(&ui, &values).unwrap();
    assert!(control(&mut blueprint.templates[0].root, 255));
    ui.layout = Layout::encode(&blueprint).unwrap();
    assert!(oib::compile(&ui, &values).is_err());
}
