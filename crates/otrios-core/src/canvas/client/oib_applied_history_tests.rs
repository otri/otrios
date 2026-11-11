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
    connect_with_ui(authority, writer, &ui)
}
fn connect_with_ui(authority: &mut Document, writer: u8, ui: &Ui) -> (StateAdapter, Session) {
    let mut live = StateAdapter::for_ui(ui, "history:manager", &[], [writer; 16]).unwrap();
    let (mut session, _) = authority.open_session(&[writer; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    live.receive(&authority.generate(&mut session).unwrap().unwrap())
        .unwrap();
    pump(&mut live, authority, &mut session);
    (live, session)
}
fn setup() -> (StateAdapter, Document, Session, WorkingCopy) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    setup_with_ui(ui)
}
fn setup_with_ui(ui: Ui) -> (StateAdapter, Document, Session, WorkingCopy) {
    let mut authority = Document::new(ui.state_schema.clone(), "history:manager").unwrap();
    authority.configure_oib(&ui).unwrap();
    let (live, session) = connect_with_ui(&mut authority, 101, &ui);
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

pub(in crate::canvas::client) fn owned_slot_ui() -> Ui {
    use otrios_interfaces::layout::{Expr, Shape};
    use otrios_interfaces::oib::designer::{DefinitionId, SlotDescriptor, SlotId, SlotLayout};
    let ui = crate::test_support::oib_designer::ui(&[]);
    let graph = crate::canvas::oib::seed(&ui)
        .unwrap()
        .graph
        .as_ref()
        .unwrap();
    let mut definitions = graph.definitions.clone();
    let panel = definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let class = panel
        .nodes
        .iter()
        .find(|n| n.id == panel.root)
        .unwrap()
        .class;
    let host = NodeId(999);
    panel
        .nodes
        .iter_mut()
        .find(|n| n.id == panel.root)
        .unwrap()
        .visual_children
        .push(host);
    panel
        .nodes
        .push(otrios_interfaces::oib::designer::DefinitionNode {
            id: host,
            name: "Slot host".into(),
            class,
            owner: Some(panel.root),
            visual_children: vec![],
            defaults: vec![],
            reference: None,
            visual: Some(otrios_interfaces::oib::graph::Visual {
                layout: None,
                frame: [0., 0., 320., 200.].map(|v| Expr::Literal(Value::Number(v))),
                resize: None,
                visible: Expr::Literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                semantics: None,
                action: None,
                parameters: vec![],
            }),
        });
    panel.slots.push(SlotDescriptor {
        id: SlotId(1000),
        name: "Owned content".into(),
        references: vec![],
        node: host,
        admitted_classes: vec![class],
        cardinality: [0, 4],
        layout: SlotLayout::Absolute,
    });
    crate::canvas::oib::graph::with_source(&ui, &definitions, &graph.presets).unwrap()
}

#[test]
fn applied_removal_restores_owned_instances_connections_and_values_without_app_rollback() {
    use otrios_interfaces::oib::authoring::{Destination, HierarchyBatch, HierarchyEdit};
    use otrios_interfaces::oib::designer::{ObjectAddress, Selection, SlotId};
    let ui = owned_slot_ui();
    let (mut live, mut authority, mut session, mut copy) = setup_with_ui(ui.clone());
    let original = live.oib_history_source().unwrap();
    let parent = original
        .instances
        .iter()
        .find(|i| i.id == 20)
        .unwrap()
        .clone();
    let survivor = original
        .instances
        .iter()
        .find(|i| i.id == 30)
        .unwrap()
        .clone();
    let root = original
        .definitions
        .iter()
        .find(|d| d.id.0 == parent.component)
        .unwrap()
        .root;
    let destination = Destination::Slot {
        component: ObjectAddress {
            instance: InstanceId(20),
            references: vec![],
            node: root,
        },
        slot: SlotId(1000),
    };
    let child_a = InstanceId(original.next_identity);
    let child_b = InstanceId(original.next_identity + 1);
    copy.stage_structure(EditBatch::hierarchy(HierarchyBatch {
        generation: copy.candidate.oib_structural_generation().unwrap(),
        operations: vec![
            HierarchyEdit::Insert {
                prototype: InstanceId(parent.prototype),
                name: Some("ChildA".into()),
                destination: destination.clone(),
                before: None,
                frame: Some([8., 8., 200., 80.]),
            },
            HierarchyEdit::Insert {
                prototype: InstanceId(parent.prototype),
                name: Some("ChildB".into()),
                destination: destination.clone(),
                before: None,
                frame: Some([8., 96., 200., 80.]),
            },
        ],
    }))
    .unwrap();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let parent_label = address();
    let child_label = match address() {
        PropertyAddress::Occurrence { path, .. } => PropertyAddress::Occurrence {
            instance: child_a,
            path,
        },
        _ => unreachable!(),
    };
    copy.stage_property(&parent_label, Some(Value::Text("Restored parent".into())))
        .unwrap();
    copy.stage_property(&child_label, Some(Value::Text("Restored child".into())))
        .unwrap();
    let child_control = ObjectAddress {
        instance: child_a,
        references: vec![NodeId(7)],
        node: NodeId(1),
    };
    let seed = copy.candidate.oib_authoring_seed().unwrap();
    let metadata = crate::canvas::oib::document::expanded_metadata(
        &seed,
        &copy.candidate.oib_allocations().unwrap(),
    )
    .unwrap();
    let connection = crate::canvas::oib::document::connections::choices(
        &ui,
        &metadata,
        &child_control,
        PropertyId(4),
    )
    .unwrap()
    .into_iter()
    .find(|(_, source)| source.field == 2)
    .unwrap()
    .1;
    copy.stage_structure(EditBatch::hierarchy(HierarchyBatch {
        generation: copy.candidate.oib_structural_generation().unwrap(),
        operations: vec![HierarchyEdit::Connect {
            object: child_control.clone(),
            property: PropertyId(4),
            source: Some(connection.clone()),
        }],
    }))
    .unwrap();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let before = live.oib_history_source().unwrap();
    copy.remember_location(
        Selection::Object(child_control),
        oib_workspace::Task::Inspect,
    );
    copy.stage_structure(EditBatch::hierarchy(HierarchyBatch {
        generation: copy.candidate.oib_structural_generation().unwrap(),
        operations: vec![
            HierarchyEdit::Remove { instance: child_b },
            HierarchyEdit::Remove { instance: child_a },
            HierarchyEdit::Remove {
                instance: InstanceId(20),
            },
        ],
    }))
    .unwrap();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let removed = live.oib_history_source().unwrap();
    assert!(
        removed
            .instances
            .iter()
            .all(|i| ![20, child_a.0, child_b.0].contains(&i.id))
    );
    assert!(
        removed
            .connections
            .iter()
            .all(|c| c.object.instance != child_a)
    );
    // These app writes happen after deletion, outside the history unit.
    live.set(&[(1, 812), (2, 643), (3, 0), (4, 1000)]).unwrap();
    pump(&mut live, &mut authority, &mut session);
    let survivor_label = match address() {
        PropertyAddress::Occurrence { path, .. } => PropertyAddress::Occurrence {
            instance: InstanceId(30),
            path,
        },
        _ => unreachable!(),
    };
    live.edit_oib_property(
        &survivor_label,
        Some(Value::Text("Surviving change".into())),
    )
    .unwrap();
    pump(&mut live, &mut authority, &mut session);
    copy.synchronize(&live).unwrap();
    let app_values = [1, 2, 3, 4].map(|field| live.typed_value(field).unwrap());
    let next = removed.next_identity;
    assert!(copy.undo().unwrap());
    assert!(
        copy.history_review().is_none(),
        "{:?}",
        copy.history_review().map(|r| &r.reason)
    );
    assert!(copy.dirty());
    assert!(
        live.generate().unwrap().is_none(),
        "Undo is private until Apply"
    );
    let restored = copy.candidate.oib_history_source().unwrap();
    let fresh_parent = restored
        .instances
        .iter()
        .find(|i| i.name == parent.name)
        .unwrap();
    let fresh_a = restored
        .instances
        .iter()
        .find(|i| i.name == "ChildA")
        .unwrap();
    let fresh_b = restored
        .instances
        .iter()
        .find(|i| i.name == "ChildB")
        .unwrap();
    assert!(fresh_parent.id >= next && fresh_a.id >= next && fresh_b.id >= next);
    assert_ne!(fresh_parent.id, 20);
    assert_ne!(fresh_a.id, child_a.0);
    assert_ne!(fresh_b.id, child_b.0);
    assert_eq!(fresh_parent.prototype, parent.prototype);
    assert_eq!(
        restored.instances.iter().find(|i| i.id == 30),
        Some(&survivor)
    );
    let siblings:Vec<_>=restored.instances.iter().filter(|i|restored.placements.iter().any(|p|p.instance.0==i.id && matches!(&p.destination,Destination::Slot {component,slot} if component.instance.0==fresh_parent.id && *slot==SlotId(1000)))).map(|i|i.name.as_str()).collect();
    assert_eq!(siblings, vec!["ChildA", "ChildB"]);
    for (old, new) in [(child_a, fresh_a), (child_b, fresh_b)] {
        assert_eq!(
            restored
                .placements
                .iter()
                .find(|p| p.instance.0 == new.id)
                .unwrap()
                .frame,
            before
                .placements
                .iter()
                .find(|p| p.instance == old)
                .unwrap()
                .frame
        );
    }
    assert!(
        restored
            .connections
            .iter()
            .any(|c| c.object.instance.0 == fresh_a.id && c.source == connection)
    );
    assert!(
        matches!(copy.location().unwrap().selection,Selection::Object(ref object) if object.instance.0==fresh_a.id && object.references==vec![NodeId(7)])
    );
    let remapped = |address: &PropertyAddress, instance: u64| match address {
        PropertyAddress::Occurrence { path, .. } => PropertyAddress::Occurrence {
            instance: InstanceId(instance),
            path: path.clone(),
        },
        _ => unreachable!(),
    };
    let restored_parent_label = remapped(&parent_label, fresh_parent.id);
    let restored_child_label = remapped(&child_label, fresh_a.id);
    assert_eq!(
        copy.candidate
            .oib_property_register(&restored_parent_label)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Restored parent".into()))]
    );
    assert_eq!(
        copy.candidate
            .oib_property_register(&restored_child_label)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Restored child".into()))]
    );
    let ids = [fresh_parent.id, fresh_a.id, fresh_b.id];
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert_eq!(
        live.oib_property_register(&survivor_label)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Surviving change".into()))]
    );
    assert_eq!(
        [1, 2, 3, 4].map(|field| live.typed_value(field).unwrap()),
        app_values
    );
    assert!(live.oib_capture_change(&parent_label, None).is_err());
    assert!(copy.redo().unwrap());
    assert!(
        copy.history_review().is_none(),
        "{:?}",
        copy.history_review().map(|review| (
            &review.reason,
            review
                .conflicts
                .iter()
                .map(|c| &c.target)
                .collect::<Vec<_>>()
        ))
    );
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(
        live.oib_history_source()
            .unwrap()
            .instances
            .iter()
            .all(|i| !ids.contains(&i.id))
    );
    assert_eq!(
        [1, 2, 3, 4].map(|field| live.typed_value(field).unwrap()),
        app_values
    );
    assert_eq!(
        live.oib_history_source()
            .unwrap()
            .instances
            .iter()
            .find(|i| i.id == 30),
        Some(&survivor)
    );
}
#[test]
fn applied_removal_review_preserves_later_sibling_instead_of_overwriting_order() {
    use otrios_interfaces::oib::authoring::{Destination, HierarchyBatch, HierarchyEdit};
    let (mut live, mut authority, mut session, mut copy) = setup();
    let prototype = live
        .oib_history_source()
        .unwrap()
        .instances
        .iter()
        .find(|i| i.id == 20)
        .unwrap()
        .prototype;
    copy.stage_structure(EditBatch::remove(
        copy.candidate.oib_structural_generation().unwrap(),
        20,
    ))
    .unwrap();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let (mut rival, mut rival_session) = connect(&mut authority, 106);
    rival
        .edit_oib_structure(EditBatch::hierarchy(HierarchyBatch {
            generation: rival.oib_structural_generation().unwrap(),
            operations: vec![HierarchyEdit::Insert {
                prototype: InstanceId(prototype),
                name: Some("Rival".into()),
                destination: Destination::Canvas,
                before: None,
                frame: None,
            }],
        }))
        .unwrap();
    pump(&mut rival, &mut authority, &mut rival_session);
    pump(&mut live, &mut authority, &mut session);
    copy.synchronize(&live).unwrap();
    let before = live.oib_history_source().unwrap();
    assert!(copy.undo().unwrap());
    assert!(!copy.dirty());
    assert!(
        copy.history_review()
            .unwrap()
            .conflicts
            .iter()
            .any(|conflict| conflict.current_text.contains("Rival"))
    );
    assert_eq!(live.oib_history_source().unwrap(), before);
    assert!(live.generate().unwrap().is_none());
}

#[test]
fn recreated_occurrence_does_not_reanchor_earlier_equal_value_aba() {
    let (mut live, mut authority, mut session, mut copy) = setup();
    stage(&mut copy, "Same");
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let (mut rival, mut rival_session) = connect(&mut authority, 107);
    for value in ["Other", "Same"] {
        rival
            .edit_oib_property(&address(), Some(Value::Text(value.into())))
            .unwrap();
        pump(&mut rival, &mut authority, &mut rival_session);
    }
    pump(&mut live, &mut authority, &mut session);
    copy.synchronize(&live).unwrap();
    copy.stage_structure(EditBatch::remove(
        copy.candidate.oib_structural_generation().unwrap(),
        20,
    ))
    .unwrap();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let fresh = live.oib_history_source().unwrap().next_identity;
    assert!(copy.undo().unwrap());
    assert!(copy.history_review().is_none());
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let remapped = match address() {
        PropertyAddress::Occurrence { path, .. } => PropertyAddress::Occurrence {
            instance: InstanceId(fresh),
            path,
        },
        _ => unreachable!(),
    };
    assert_eq!(
        live.oib_property_register(&remapped).unwrap().alternatives,
        vec![Some(Value::Text("Same".into()))]
    );
    let source = live.oib_history_source().unwrap();
    assert!(copy.undo().unwrap());
    assert!(!copy.dirty());
    assert!(copy.history_review().is_some());
    assert_eq!(live.oib_history_source().unwrap(), source);
    assert!(live.generate().unwrap().is_none());
    assert_eq!(
        live.oib_property_register(&remapped).unwrap().alternatives,
        vec![Some(Value::Text("Same".into()))]
    );
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
fn accepted_creation_undo_redo_restores_fresh_identity_properties_and_selection() {
    use otrios_interfaces::layout::{Expr, Shape, ValueType};
    use otrios_interfaces::oib::{
        designer::{ClassKind, DefinitionId, DefinitionNode, Selection},
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
    copy.remember_location(
        Selection::Document(source.designer.as_ref().unwrap().id),
        oib_workspace::Task::Library,
    );
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
    let selected = Selection::Definition(definition);
    copy.remember_location(selected.clone(), oib_workspace::Task::Inspect);
    let property = crate::canvas::oib::document::properties(
        &copy.candidate.oib_authoring_seed().unwrap(),
        &copy.candidate.oib_allocations().unwrap(),
        &selected,
        &copy.candidate.oib_active().unwrap(),
    )
    .unwrap()
    .into_iter()
    .find(|p| p.descriptor.value_type == ValueType::Color)
    .unwrap();
    let color = Value::Color([17, 34, 51, 255]);
    copy.stage_property(property.address.as_ref().unwrap(), Some(color.clone()))
        .unwrap();
    publish(&mut copy, &mut live, &mut authority, &mut session);
    let mut previous = definition;
    for _ in 0..2 {
        assert!(copy.undo().unwrap());
        assert!(copy.history_review().is_none());
        publish(&mut copy, &mut live, &mut authority, &mut session);
        assert_eq!(copy.published.counts(), (0, 1));
        assert!(
            !live
                .oib_history_source()
                .unwrap()
                .definitions
                .iter()
                .any(|d| d.id == previous)
        );
        let next = live.oib_history_source().unwrap().next_identity;
        assert!(copy.redo().unwrap());
        assert!(copy.dirty());
        assert!(copy.history_review().is_none());
        let restored = copy
            .candidate
            .oib_history_source()
            .unwrap()
            .definitions
            .into_iter()
            .find(|d| d.name == "Created")
            .unwrap();
        assert_eq!(restored.id.0, next);
        assert!(restored.id.0 > previous.0);
        assert!(restored.root.0 > previous.0);
        let address = PropertyAddress::Definition {
            definition: restored.id,
            node: restored.root,
            property: property.descriptor.id,
        };
        assert_eq!(
            copy.candidate
                .oib_property_register(&address)
                .unwrap()
                .alternatives,
            vec![Some(color.clone())]
        );
        assert!(
            matches!(copy.location().unwrap().selection, Selection::Definition(id) if id == restored.id)
        );
        assert!(
            live.generate().unwrap().is_none(),
            "restoration is private before Apply"
        );
        publish(&mut copy, &mut live, &mut authority, &mut session);
        assert_eq!(copy.published.counts(), (1, 0));
        assert_eq!(
            live.oib_property_register(&address).unwrap().alternatives,
            vec![Some(color.clone())]
        );
        previous = restored.id;
    }

    // A property-only publication can race ahead of deletion without changing
    // the structural generation. The accepted receipt must not turn our older
    // captured color into proof of the actual removed value.
    let source = live.oib_history_source().unwrap();
    let removed = source
        .definitions
        .iter()
        .find(|d| d.id == previous)
        .unwrap();
    let address = PropertyAddress::Definition {
        definition: removed.id,
        node: removed.root,
        property: property.descriptor.id,
    };
    copy.stage_structure(EditBatch {
        generation: copy.candidate.oib_structural_generation().unwrap(),
        operations: vec![DocumentEdit::Definition(DefinitionEdit::DeleteDefinition {
            definition: previous,
        })],
    })
    .unwrap();
    let (mut rival, mut rival_session) = connect(&mut authority, 105);
    rival
        .edit_oib_property(&address, Some(Value::Color([98, 76, 54, 255])))
        .unwrap();
    pump(&mut rival, &mut authority, &mut rival_session);
    publish(&mut copy, &mut live, &mut authority, &mut session);
    assert!(
        !live
            .oib_history_source()
            .unwrap()
            .definitions
            .iter()
            .any(|d| d.id == previous)
    );
    assert!(copy.undo().unwrap());
    assert!(!copy.dirty());
    assert!(
        copy.history_review()
            .unwrap()
            .reason
            .contains("changed before deletion")
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
