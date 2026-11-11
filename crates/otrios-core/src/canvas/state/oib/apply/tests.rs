// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::test_support::oib_designer as app;
use otrios_interfaces::oib::authoring::*;

fn fixture() -> (Document, StateAdapter, Session) {
    let ui = app::ui(&[]);
    let mut authority = Document::new(ui.state_schema.clone(), "oib:atomic-apply").unwrap();
    authority.configure_oib(&ui).unwrap();
    let writer = [81; 16];
    let (mut session, _) = authority.open_session(&writer, 0).unwrap();
    session.oib_edit = true;
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let mut replica = StateAdapter::for_ui(&ui, "oib:atomic-apply", &[], writer).unwrap();
    // Initial admission and frames use the same real Merkle transport paths.
    for _ in 0..30 {
        while let Some(bytes) = authority.generate(&mut session).unwrap() {
            replica.receive(&bytes).unwrap();
        }
        while let Some(bytes) = replica.generate().unwrap() {
            authority.receive(&mut session, &bytes).unwrap();
        }
        if replica.confirmed() {
            break;
        }
    }
    assert!(replica.confirmed());
    (authority, replica, session)
}
fn settle(authority: &mut Document, replica: &mut StateAdapter, session: &mut Session) {
    for _ in 0..40 {
        let mut progress = false;
        while let Some(bytes) = replica.generate().unwrap() {
            authority.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = authority.generate(session).unwrap() {
            replica.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(replica.confirmed());
            return;
        }
    }
    panic!("Apply receipt did not settle");
}
fn color_change(replica: &StateAdapter, color: [u8; 4]) -> Change {
    let seed = replica.doc.oib_resolved_seed().unwrap();
    let cell = seed
        .cells
        .iter()
        .find(|c| c.owner == 20 && matches!(c.default, Value::Color(_)))
        .unwrap();
    let address =
        super::super::super::super::oib::document::address_for_cell(&seed, cell.id).unwrap();
    replica
        .oib_capture_change(&address, Some(Value::Color(color)))
        .unwrap()
}
fn rename(replica: &StateAdapter) -> DocumentEdit {
    let source = replica.doc.oib_source().unwrap();
    let definition = source
        .definitions
        .iter()
        .find(|d| d.name == "SettingsPanel")
        .unwrap();
    DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
        definition: definition.id,
        name: "Renamed panel".into(),
    })
}
#[test]
fn atomic_apply_mixed_source_and_property_activates_once_and_preview_has_no_transport() {
    let (mut authority, mut replica, mut session) = fixture();
    let before = authority.oib_status().unwrap();
    let original = replica.doc.tree.hash();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations.push(rename(&replica));
    apply
        .changes
        .push(color_change(&replica, [240, 20, 30, 255]));
    let mut preview = replica.preview_oib_apply(&apply).unwrap();
    assert_eq!(replica.doc.tree.hash(), original);
    assert!(preview.generate().unwrap().is_none());
    assert!(
        preview
            .doc
            .oib_source()
            .unwrap()
            .definitions
            .iter()
            .any(|d| d.name == "Renamed panel")
    );
    assert!(replica.apply_oib(apply).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(
        authority.oib_status().unwrap().generation,
        before.generation + 1
    );
    assert_eq!(
        authority.oib_status().unwrap().structural_generation,
        before.structural_generation + 1
    );
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Accepted { .. }
    ));
}

#[test]
fn atomic_apply_semantic_failure_publishes_only_rejection_receipt() {
    let (mut authority, mut replica, mut session) = fixture();
    let source = authority.oib_source().unwrap();
    let values = authority.oib_values(2).unwrap();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations.push(rename(&replica));
    let mut bad = color_change(&replica, [240, 20, 30, 255]);
    bad.value = Some(Value::Number(15.));
    apply.changes.push(bad);
    assert!(replica.preview_oib_apply(&apply).is_err());
    assert!(replica.apply_oib(apply).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(authority.oib_source().unwrap(), source);
    assert_eq!(authority.oib_values(2).unwrap(), values);
    let result = replica.oib_apply_result().unwrap().unwrap();
    assert!(matches!(result.result, ApplyResult::Rejected { .. }));
    authority
        .oib_reject_structure("A later unrelated rejection")
        .unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(replica.oib_apply_result().unwrap().unwrap(), result);
    // Ordinary app writes can advance the shared writer without erasing it.
    let field = replica
        .schema()
        .fields
        .iter()
        .find(|f| f.writable && matches!(f.default, Value::Number(_)))
        .unwrap()
        .id;
    replica.set(&[(field, 321)]).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(replica.oib_apply_result().unwrap().unwrap(), result);
}

#[test]
fn atomic_apply_rejects_causal_change_even_when_visible_value_is_equal() {
    let (mut authority, mut replica, mut session) = fixture();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations.push(rename(&replica));
    let mut change = color_change(&replica, [240, 20, 30, 255]);
    apply.changes.push(change.clone());
    let values = authority.oib_values(2).unwrap();
    // Causally clear an already inherited color: its rendered value stays equal.
    change.value = None;
    authority.oib_write([82; 16], &[change]).unwrap();
    assert_eq!(authority.oib_values(2).unwrap(), values);
    let source = authority.oib_source().unwrap();
    assert!(replica.apply_oib(apply).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(authority.oib_source().unwrap(), source);
    assert_eq!(authority.oib_values(2).unwrap(), values);
    assert!(
        matches!(replica.oib_apply_result().unwrap().unwrap().result,ApplyResult::Rejected{message} if message.contains("Property changed"))
    );
}

#[test]
fn atomic_apply_rejects_stale_epoch_generation_and_baseline_without_rebasing() {
    for kind in 0..3 {
        let (mut authority, mut replica, mut session) = fixture();
        let source = authority.oib_source().unwrap();
        let values = authority.oib_values(2).unwrap();
        let mut apply = replica.oib_apply_base().unwrap();
        apply.operations.push(rename(&replica));
        apply
            .changes
            .push(color_change(&replica, [240, 20, 30, 255]));
        match kind {
            0 => apply.epoch[0] ^= 1,
            1 => apply.generation += 1,
            _ => apply.baseline[0] ^= 1,
        }
        assert!(replica.apply_oib(apply).unwrap());
        settle(&mut authority, &mut replica, &mut session);
        assert_eq!(authority.oib_source().unwrap(), source);
        assert_eq!(authority.oib_values(2).unwrap(), values);
        assert!(matches!(
            replica.oib_apply_result().unwrap().unwrap().result,
            ApplyResult::Rejected { .. }
        ));
    }
}

#[test]
fn atomic_apply_lost_reply_replays_once_and_reconnect_recovers_same_result() {
    let (mut authority, mut replica, mut session) = fixture();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations.push(rename(&replica));
    apply
        .changes
        .push(color_change(&replica, [240, 20, 30, 255]));
    assert!(replica.apply_oib(apply.clone()).unwrap());
    let request = replica.generate().unwrap().unwrap();
    assert!(authority.receive(&mut session, &request).unwrap());
    let committed = authority.save().unwrap();
    assert!(!authority.receive(&mut session, &request).unwrap());
    assert_eq!(authority.save().unwrap(), committed);
    let mut changed = apply;
    changed.changes[0].value = Some(Value::Color([1, 2, 3, 255]));
    let changed = encode_state_request(&StateRequest::OibApply {
        sequence: 1,
        apply: changed,
    })
    .unwrap();
    assert!(
        authority
            .receive(&mut session, &changed)
            .unwrap_err()
            .contains("Changed OIB replay")
    );
    assert_eq!(authority.save().unwrap(), committed);
    // Lose the reply and establish a fresh transport replica with the same flight.
    let ui = app::ui(&[]);
    let mut resumed = StateAdapter::for_ui(&ui, "oib:atomic-apply", &[], [81; 16]).unwrap();
    resumed.restore_pending(&replica).unwrap();
    let (mut next_session, _) = authority
        .open_session(&[81; 16], resumed.resume_sequence())
        .unwrap();
    next_session.oib_edit = true;
    next_session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    resumed
        .receive(&authority.generate(&mut next_session).unwrap().unwrap())
        .unwrap();
    settle(&mut authority, &mut resumed, &mut next_session);
    assert!(resumed.confirmed());
    let result = resumed.oib_apply_result().unwrap().unwrap();
    assert_eq!(result.sequence, 1);
    assert!(matches!(result.result, ApplyResult::Accepted { .. }));
    assert_eq!(authority.save().unwrap(), committed);
}

#[test]
fn atomic_apply_keeps_original_snapshot_isolated_and_denies_non_admin() {
    let (mut authority, mut replica, mut session) = fixture();
    let original = authority.clone();
    let snapshot = original.save().unwrap();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations.push(rename(&replica));
    replica.apply_oib(apply).unwrap();
    let request = replica.generate().unwrap().unwrap();
    session.oib_edit = false;
    assert!(
        authority
            .receive(&mut session, &request)
            .unwrap_err()
            .contains("Space Admin")
    );
    assert_eq!(authority.save().unwrap(), snapshot);
    session.oib_edit = true;
    authority.receive(&mut session, &request).unwrap();
    assert_eq!(original.save().unwrap(), snapshot);
    assert_ne!(authority.save().unwrap(), snapshot);
}

#[test]
fn atomic_apply_creation_allocates_only_on_commit_and_replay_never_duplicates() {
    use otrios_interfaces::{
        layout::{Expr, Shape},
        oib::{
            designer::{ClassId, DefinitionId, DefinitionNode, NodeId},
            graph,
        },
    };
    let (mut authority, mut replica, mut session) = fixture();
    let original = authority.oib_source().unwrap();
    let next = original.next_identity;
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations = vec![
        DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
            definition: DefinitionId(next),
            name: "Created once".into(),
            root: DefinitionNode {
                id: NodeId(next + 1),
                name: "Panel".into(),
                class: ClassId(4),
                owner: None,
                visual_children: vec![],
                defaults: vec![],
                reference: None,
                visual: Some(graph::Visual {
                    layout: None,
                    frame: [0., 0., 320., 48.].map(|n| Expr::Literal(Value::Number(n))),
                    resize: None,
                    visible: Expr::Literal(Value::Boolean(true)),
                    shape: Shape::Canvas { clip: true },
                    semantics: None,
                    action: None,
                    parameters: vec![],
                }),
            },
        }),
        DocumentEdit::Preset(PresetEdit::Create {
            preset: graph::Preset {
                id: next + 2,
                name: "Created once".into(),
                definition: DefinitionId(next),
                assignments: vec![],
                frame: None,
                palette: true,
            },
        }),
    ];
    let preview = replica.preview_oib_apply(&apply).unwrap();
    assert_eq!(preview.doc.oib_source().unwrap().next_identity, next + 3);
    assert_eq!(authority.oib_source().unwrap(), original);
    assert_eq!(replica.doc.oib_source().unwrap(), original);
    replica.apply_oib(apply).unwrap();
    let request = replica.generate().unwrap().unwrap();
    authority.receive(&mut session, &request).unwrap();
    let committed = authority.save().unwrap();
    assert!(!authority.receive(&mut session, &request).unwrap());
    assert_eq!(authority.save().unwrap(), committed);
    settle(&mut authority, &mut replica, &mut session);
    let source = authority.oib_source().unwrap();
    assert_eq!(source.next_identity, next + 3);
    assert_eq!(
        source
            .definitions
            .iter()
            .filter(|d| d.id == DefinitionId(next))
            .count(),
        1
    );
    assert_eq!(
        source.presets.iter().filter(|p| p.id == next + 2).count(),
        1
    );
}

#[test]
fn atomic_apply_new_instance_property_uses_exact_seeded_candidate_context() {
    let (mut authority, mut replica, mut session) = fixture();
    let original = replica.doc.oib_resolved_seed().unwrap();
    let prototype = replica
        .doc
        .oib_source()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id == 20)
        .unwrap()
        .prototype;
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations = EditBatch::insert(
        apply.generation,
        prototype,
        None,
        Some("Inserted panel".into()),
    )
    .operations;
    let preview = replica.preview_oib_apply(&apply).unwrap();
    let expanded = preview.doc.oib_resolved_seed().unwrap();
    let cell = expanded
        .cells
        .iter()
        .find(|c| {
            !original.cells.iter().any(|old| old.id == c.id) && matches!(c.default, Value::Color(_))
        })
        .unwrap();
    let address =
        super::super::super::super::oib::document::address_for_cell(&expanded, cell.id).unwrap();
    let change = preview
        .oib_capture_change(&address, Some(Value::Color([12, 34, 56, 255])))
        .unwrap();
    apply.changes.push(change);
    assert!(replica.preview_oib_apply(&apply).is_ok());
    let mut stale = apply.clone();
    stale.changes[0].observed = color_change(&replica, [1, 2, 3, 255]).observed;
    assert!(replica.preview_oib_apply(&stale).is_err());
    let before = authority.oib_status().unwrap().generation;
    replica.apply_oib(apply).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(authority.oib_status().unwrap().generation, before + 1);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Accepted { .. }
    ));
    assert_eq!(
        authority.oib_values(2).unwrap().get(&cell.id),
        Some(&Some(Value::Color([12, 34, 56, 255])))
    );
}

#[test]
fn atomic_apply_repairs_conflicts_privately_and_rejects_partial_authority_repair() {
    let (mut authority, mut replica, mut session) = fixture();
    let seed = authority.oib_resolved_seed().unwrap();
    let addresses: Vec<_> = seed
        .cells
        .iter()
        .filter(|c| c.owner == 20 && matches!(c.default, Value::Color(_)))
        .take(2)
        .map(|c| super::super::super::super::oib::document::address_for_cell(&seed, c.id).unwrap())
        .collect();
    assert_eq!(addresses.len(), 2);
    let left: Vec<_> = addresses
        .iter()
        .map(|a| {
            replica
                .oib_capture_change(a, Some(Value::Color([10, 20, 30, 255])))
                .unwrap()
        })
        .collect();
    let right: Vec<_> = addresses
        .iter()
        .map(|a| {
            replica
                .oib_capture_change(a, Some(Value::Color([40, 50, 60, 255])))
                .unwrap()
        })
        .collect();
    authority.oib_write([88; 16], &left).unwrap();
    authority.oib_write([89; 16], &right).unwrap();
    assert_eq!(authority.oib_status().unwrap().conflicts.len(), 2);
    settle(&mut authority, &mut replica, &mut session);
    let mut repair = replica.oib_apply_base().unwrap();
    assert!(replica.preview_oib_apply(&repair).is_ok());
    repair.changes.push(
        replica
            .oib_capture_change(&addresses[0], left[0].value.clone())
            .unwrap(),
    );
    assert_eq!(
        replica
            .preview_oib_apply(&repair)
            .unwrap()
            .doc
            .oib_status()
            .unwrap()
            .conflicts
            .len(),
        1
    );
    assert!(authority.oib_apply_candidate([81; 16], &repair).is_err());
    let mut structural = repair.clone();
    structural.operations.push(rename(&replica));
    assert!(replica.preview_oib_apply(&structural).is_err());
    repair.changes.push(
        replica
            .oib_capture_change(&addresses[1], left[1].value.clone())
            .unwrap(),
    );
    replica.apply_oib(repair).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert!(authority.oib_status().unwrap().conflicts.is_empty());
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Accepted { .. }
    ));
}

#[test]
fn atomic_apply_adapter_does_not_resolve_changed_payload_from_sequence_alone() {
    let (mut authority, mut replica, mut session) = fixture();
    let mut apply = replica.oib_apply_base().unwrap();
    apply
        .changes
        .push(color_change(&replica, [240, 20, 30, 255]));
    replica.apply_oib(apply).unwrap();
    let request = replica.generate().unwrap().unwrap();
    authority.receive(&mut session, &request).unwrap();
    // A corrupted restored flight must not borrow the accepted operation's result.
    replica
        .flight
        .as_mut()
        .unwrap()
        .oib_apply
        .as_mut()
        .unwrap()
        .changes[0]
        .value = Some(Value::Color([1, 2, 3, 255]));
    let frame = authority.generate(&mut session).unwrap().unwrap();
    assert!(
        replica
            .receive(&frame)
            .unwrap_err()
            .contains("receipt identity changed")
    );
    assert!(replica.flight.is_some());
    assert!(!replica.confirmed());
}
