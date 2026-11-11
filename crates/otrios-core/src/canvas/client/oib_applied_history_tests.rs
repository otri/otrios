// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::canvas::{Document, Session, StateSelection};
use otrios_interfaces::oib::designer::{InstanceId, NodeId, PropertyId, PropertyPath};
fn address() -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    }
}
fn pump(live: &mut StateAdapter, authority: &mut Document, session: &mut Session) {
    for _ in 0..40 {
        let mut progress = false;
        while let Some(bytes) = live.generate().unwrap() {
            authority.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = authority.generate(session).unwrap() {
            live.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(live.confirmed());
            return;
        }
    }
    panic!("history fixture failed to settle");
}
fn connect(authority: &mut Document, writer: u8) -> (StateAdapter, Session) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    let mut live = StateAdapter::for_ui(&ui, "history:manager", &[], [writer; 16]).unwrap();
    let (mut session, _) = authority.open_session(&[writer; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    live.receive(&authority.generate(&mut session).unwrap().unwrap())
        .unwrap();
    pump(&mut live, authority, &mut session);
    (live, session)
}
fn setup() -> (StateAdapter, Document, Session, WorkingCopy) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    let mut authority = Document::new(ui.state_schema.clone(), "history:manager").unwrap();
    authority.configure_oib(&ui).unwrap();
    let (live, session) = connect(&mut authority, 101);
    let copy = WorkingCopy::new(&live).unwrap();
    (live, authority, session, copy)
}
fn publish(
    copy: &mut WorkingCopy,
    live: &mut StateAdapter,
    authority: &mut Document,
    session: &mut Session,
) {
    assert!(copy.submit(live).unwrap());
    pump(live, authority, session);
    assert!(copy.synchronize(live).unwrap());
    assert!(!copy.dirty());
}
fn stage(copy: &mut WorkingCopy, text: &str) {
    assert!(
        copy.stage_property(&address(), Some(Value::Text(text.into())))
            .unwrap()
    );
}
fn text(live: &StateAdapter) -> String {
    let property = crate::canvas::oib::document::properties(
        &live.oib_authoring_seed().unwrap(),
        &live.oib_allocations().unwrap(),
        &otrios_interfaces::oib::designer::Selection::Object(
            otrios_interfaces::oib::designer::ObjectAddress {
                instance: InstanceId(20),
                references: vec![NodeId(7)],
                node: NodeId(1),
            },
        ),
        &live.oib_active().unwrap(),
    )
    .unwrap()
    .into_iter()
    .find(|p| p.address.as_ref() == Some(&address()))
    .unwrap();
    let Some(Value::Text(value)) = property.value else {
        panic!("text property")
    };
    value
}
#[test]
fn accepted_apply_is_one_private_inverse_unit_and_redo_uses_accepted_undo() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    let original = text(&live);
    stage(&mut copy, "First");
    stage(&mut copy, "Final");
    let original_apply = copy.apply.clone();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(copy.published.counts(), (1, 0));
    assert!(copy.can_undo());
    assert!(copy.undo().unwrap());
    assert!(copy.dirty());
    assert_eq!(text(&copy.candidate), original);
    assert_eq!(text(&live), "Final");
    assert!(live.generate().unwrap().is_none());
    assert_eq!(copy.published.counts(), (1, 0));
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(copy.published.counts(), (0, 1));
    assert_eq!(text(&live), original);
    assert!(copy.redo().unwrap());
    assert_eq!(text(&copy.candidate), "Final");
    assert_ne!(
        copy.apply.changes[0].observed,
        original_apply.changes[0].observed
    );
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(text(&live), "Final");
    assert_eq!(copy.published.counts(), (1, 0));
}
#[test]
fn known_accepted_inverse_reanchors_overlapping_earlier_undo_only() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    let original = text(&live);
    stage(&mut copy, "First");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    stage(&mut copy, "Second");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(copy.undo().unwrap());
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(text(&live), "First");
    assert!(copy.undo().unwrap());
    assert!(copy.history_review().is_none());
    assert_eq!(text(&copy.candidate), original);
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(copy.published.counts(), (0, 2));
    assert!(copy.redo().unwrap());
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(copy.redo().unwrap());
    assert!(copy.history_review().is_none());
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(text(&live), "Second");
}
#[test]
fn equal_remote_aba_value_does_not_refresh_receipt_guards() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    stage(&mut copy, "First");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let (mut rival, mut rival_session) = connect(&mut authority, 102);
    assert!(
        rival
            .edit_oib_property(&address(), Some(Value::Text("Other".into())))
            .unwrap()
    );
    pump(&mut rival, &mut authority, &mut rival_session);
    assert!(
        rival
            .edit_oib_property(&address(), Some(Value::Text("First".into())))
            .unwrap()
    );
    pump(&mut rival, &mut authority, &mut rival_session);
    pump(&mut live, &mut authority, &mut session);
    assert!(copy.synchronize(&live).unwrap());
    assert!(copy.undo().unwrap());
    assert!(!copy.dirty());
    let review = copy.history_review().unwrap();
    assert!(!review.conflicts.is_empty());
    assert_eq!(copy.published.counts(), (1, 0));
    assert_eq!(text(&live), "First");
    assert!(live.generate().unwrap().is_none());
}
#[test]
fn delayed_or_rejected_inverse_receipts_do_not_move_stacks() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    stage(&mut copy, "First");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(copy.undo().unwrap());
    assert!(copy.submit(&mut live).unwrap());
    while let Some(bytes) = live.generate().unwrap() {
        authority.receive(&mut session, &bytes).unwrap();
    }
    let delayed = authority.generate(&mut session).unwrap().unwrap();
    assert!(!copy.synchronize(&live).unwrap());
    assert_eq!(copy.published.counts(), (1, 0));
    assert!(copy.applying());
    live.receive(&delayed).unwrap();
    pump(&mut live, &mut authority, &mut session);
    assert!(copy.synchronize(&live).unwrap());
    assert_eq!(copy.published.counts(), (0, 1));
    assert!(copy.redo().unwrap());
    let (mut rival, mut rival_session) = connect(&mut authority, 103);
    assert!(
        rival
            .edit_oib_property(&address(), Some(Value::Text("Other".into())))
            .unwrap()
    );
    pump(&mut rival, &mut authority, &mut rival_session);
    assert!(copy.submit(&mut live).unwrap());
    pump(&mut live, &mut authority, &mut session);
    assert!(copy.synchronize(&live).unwrap());
    assert!(copy.error.is_some());
    assert!(copy.dirty());
    assert_eq!(copy.published.counts(), (0, 1));
    assert_eq!(text(&live), "Other");
}
#[test]
fn normal_edit_after_prepared_inverse_is_a_new_apply_not_a_history_move() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    let original = text(&live);
    stage(&mut copy, "First");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(copy.undo().unwrap());
    let prepared_digest = StateAdapter::oib_apply_digest(&copy.apply).unwrap();
    stage(&mut copy, "New edit");
    // Locally undo the extra edit, returning to the exact earlier inverse bytes.
    // That does not revive its canceled published-history intent.
    assert!(copy.undo().unwrap());
    assert_eq!(
        StateAdapter::oib_apply_digest(&copy.apply).unwrap(),
        prepared_digest
    );
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(copy.published.counts(), (2, 0));
    assert!(copy.undo().unwrap());
    assert_eq!(text(&copy.candidate), "First");
    copy.reset_to_live(&live).unwrap();
    assert_eq!(copy.published.counts(), (2, 0));
    assert_eq!(text(&copy.candidate), original);
}

#[test]
fn accepted_creation_undo_retains_explicit_unsupported_redo_without_resurrection() {
    use otrios_interfaces::layout::{Expr, Shape};
    use otrios_interfaces::oib::{
        designer::{ClassKind, DefinitionId, DefinitionNode},
        graph::Visual,
    };
    let (mut live, mut authority, mut session, mut copy) = setup();
    let source = copy.candidate.oib_authoring_seed().unwrap();
    let class = source
        .graph
        .as_ref()
        .unwrap()
        .classes
        .iter()
        .find(|c| c.implementation == ClassKind::O2DPanel)
        .unwrap()
        .id;
    let definition = DefinitionId(copy.candidate.oib_next_identity().unwrap());
    let n = |v| Expr::Literal(Value::Number(v));
    let root = DefinitionNode {
        id: NodeId(definition.0 + 1),
        name: "Panel".into(),
        class,
        owner: None,
        visual_children: vec![],
        defaults: vec![],
        reference: None,
        visual: Some(Visual {
            layout: None,
            frame: [n(0.), n(0.), n(320.), n(48.)],
            resize: None,
            visible: Expr::Literal(Value::Boolean(true)),
            shape: Shape::Canvas { clip: true },
            semantics: None,
            action: None,
            parameters: vec![],
        }),
    };
    assert!(
        copy.stage_structure(EditBatch {
            generation: copy.candidate.oib_structural_generation().unwrap(),
            operations: vec![DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
                definition,
                name: "Created".into(),
                root
            })]
        })
        .unwrap()
    );
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(copy.undo().unwrap());
    assert!(copy.history_review().is_none());
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(copy.published.counts(), (0, 1));
    assert!(copy.redo().unwrap());
    assert!(!copy.dirty());
    assert!(copy.history_review().unwrap().reason.contains("fresh"));
    assert!(
        !live
            .oib_history_source()
            .unwrap()
            .definitions
            .iter()
            .any(|d| d.id == definition)
    );
    assert!(live.generate().unwrap().is_none());
}
#[test]
fn local_and_applied_history_share_one_memory_budget() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    for i in 0..12 {
        stage(&mut copy, &format!("Applied {i}"));
        publish(&mut copy, &mut live, &mut authority, &mut session);
    }
    for i in 0..500 {
        stage(&mut copy, &format!("Draft {i}"));
    }
    let bytes = copy
        .undo
        .iter()
        .chain(&copy.redo)
        .map(HistoryEntry::bytes)
        .sum::<usize>()
        + copy.published.bytes();
    assert!(bytes <= HISTORY_BYTES);
    assert!(copy.undo.len() < 500);
    assert!(copy.can_undo());
    assert!(live.generate().unwrap().is_none());
}

#[test]
fn coalesced_receipt_captures_submitted_source_not_later_other_authors_source() {
    use otrios_interfaces::oib::designer::DefinitionId;
    let (mut live, mut authority, mut session, mut copy) = setup();
    let rename = |generation, name: &str| EditBatch {
        generation,
        operations: vec![DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
            definition: DefinitionId(10),
            name: name.into(),
        })],
    };
    assert!(
        copy.stage_structure(rename(
            copy.candidate.oib_structural_generation().unwrap(),
            "Own name"
        ))
        .unwrap()
    );
    assert!(copy.submit(&mut live).unwrap());
    // Accept this participant's Apply, but deliver no receipt to it yet.
    while let Some(bytes) = live.generate().unwrap() {
        authority.receive(&mut session, &bytes).unwrap();
    }
    let (mut rival, mut rival_session) = connect(&mut authority, 104);
    assert!(
        rival
            .edit_oib_structure(rename(
                rival.oib_structural_generation().unwrap(),
                "Other name"
            ))
            .unwrap()
    );
    pump(&mut rival, &mut authority, &mut rival_session);
    // This update contains the exact accepted receipt and the later source.
    pump(&mut live, &mut authority, &mut session);
    assert!(copy.synchronize(&live).unwrap());
    assert_eq!(copy.published.counts(), (1, 0));
    assert!(copy.undo().unwrap());
    assert!(!copy.dirty());
    let review = copy.history_review().unwrap();
    assert!(
        review
            .conflicts
            .iter()
            .any(|c| c.applied_text.contains("Own name") && c.current_text.contains("Other name"))
    );
    assert!(live.generate().unwrap().is_none());
}

#[test]
fn history_clean_base_observes_conflicted_draft_even_when_presented_generation_does_not_change() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    stage(&mut copy, "Own");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let (mut first, mut first_session) = connect(&mut authority, 111);
    let (mut second, mut second_session) = connect(&mut authority, 112);
    first
        .edit_oib_property(&address(), Some(Value::Text("Other A".into())))
        .unwrap();
    second
        .edit_oib_property(&address(), Some(Value::Text("Other B".into())))
        .unwrap();
    while let Some(bytes) = first.generate().unwrap() {
        authority.receive(&mut first_session, &bytes).unwrap();
    }
    pump(&mut live, &mut authority, &mut session);
    copy.synchronize(&live).unwrap();
    let generation = live.oib_generation();
    while let Some(bytes) = second.generate().unwrap() {
        authority.receive(&mut second_session, &bytes).unwrap();
    }
    pump(&mut live, &mut authority, &mut session);
    assert_eq!(
        live.oib_generation(),
        generation,
        "conflicted drafts keep the last valid presentation"
    );
    assert!(copy.synchronize(&live).unwrap());
    assert_eq!(copy.base.oib_root(), live.oib_root());
    assert_eq!(
        copy.base
            .oib_property_register(&address())
            .unwrap()
            .alternatives
            .len(),
        2
    );
    assert!(copy.undo().unwrap());
    assert!(!copy.dirty());
    let review = copy.history_review().unwrap();
    assert!(
        review
            .conflicts
            .iter()
            .any(|c| c.current_text.contains("Other A") && c.current_text.contains("Other B"))
    );
    assert!(live.generate().unwrap().is_none());
}
