// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::test_support::oib_designer as app;
use otrios_interfaces::oib::authoring::*;
use otrios_interfaces::oib::designer::{
    DefinitionId, InstanceId, NodeId, PropertyAddress, PropertyDescriptor, PropertyId, PropertyPath,
};
#[allow(dead_code)]
#[path = "../../../../../tests/support/oib_library_fixture.rs"]
mod imported_fixture;

fn fixture() -> (Document, StateAdapter, Session) {
    fixture_ui(app::ui(&[]))
}
fn fixture_ui(ui: Ui) -> (Document, StateAdapter, Session) {
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

fn interleaved_creation(replica: &StateAdapter, implicit: bool) -> Apply {
    use otrios_interfaces::{
        layout::{Expr, Shape},
        oib::{designer::*, graph},
    };
    let source = replica.oib_history_source().unwrap();
    let next = source.next_identity;
    let resolved = replica.doc.oib_resolved_seed().unwrap();
    let metadata = resolved.designer.as_ref().unwrap();
    let (class, property) = metadata
        .classes
        .iter()
        .find_map(|class| {
            class
                .properties
                .iter()
                .find(|p| p.policy.live_literal && matches!(p.default, Value::Text(_)))
                .map(|p| (class.id, p.id))
        })
        .unwrap();
    let text = DefinitionNode {
        id: NodeId(next + if implicit { 4 } else { 3 }),
        name: "Caption".into(),
        class,
        owner: Some(NodeId(next + 1)),
        visual_children: vec![],
        defaults: vec![PropertyDefault {
            property,
            value: Value::Text("Caption".into()),
        }],
        reference: None,
        visual: Some(graph::Visual {
            layout: None,
            frame: [0., 0., 160., 32.].map(|v| Expr::Literal(Value::Number(v))),
            resize: None,
            visible: Expr::Literal(Value::Boolean(true)),
            shape: Shape::Text {
                value: Expr::Literal(Value::Text("Caption".into())),
                scale: Expr::Literal(Value::Number(1.)),
                color: Expr::Literal(Value::Color([255; 4])),
            },
            semantics: None,
            action: None,
            parameters: vec![],
        }),
    };
    let mut link = source
        .definitions
        .iter()
        .flat_map(|d| &d.nodes)
        .find(|n| {
            n.reference
                .as_ref()
                .is_some_and(|r| r.definition == DefinitionId(12))
        })
        .unwrap()
        .clone();
    link.id = NodeId(next + if implicit { 5 } else { 4 });
    link.owner = Some(NodeId(next + 1));
    link.name = "Slider".into();
    link.visual_children.clear();
    link.reference.as_mut().unwrap().assignments.clear();
    let root = DefinitionNode {
        id: NodeId(next + 1),
        name: "LabelledSlider".into(),
        class: ClassId(4),
        owner: None,
        visual_children: vec![],
        defaults: vec![],
        reference: None,
        visual: Some(graph::Visual {
            layout: None,
            frame: [0., 0., 320., 96.].map(|v| Expr::Literal(Value::Number(v))),
            resize: None,
            visible: Expr::Literal(Value::Boolean(true)),
            shape: Shape::Canvas { clip: true },
            semantics: None,
            action: None,
            parameters: vec![],
        }),
    };
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations = vec![
        DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
            definition: DefinitionId(next),
            name: "LabelledSlider".into(),
            root,
            properties: vec![],
            next_property: 0,
        }),
        DocumentEdit::Definition(DefinitionEdit::CreateNode {
            definition: DefinitionId(next),
            node: text,
            visual_parent: Some(NodeId(next + 1)),
            before: None,
        }),
        DocumentEdit::Definition(DefinitionEdit::CreateNode {
            definition: DefinitionId(next),
            node: link,
            visual_parent: Some(NodeId(next + 1)),
            before: None,
        }),
        DocumentEdit::Preset(PresetEdit::Create {
            preset: graph::Preset {
                id: next + 2,
                name: "LabelledSlider".into(),
                definition: DefinitionId(next),
                assignments: vec![],
                frame: None,
                palette: true,
            },
        }),
    ];
    if implicit {
        apply.operations.extend(
            EditBatch::insert(
                apply.generation,
                source
                    .instances
                    .iter()
                    .find(|i| i.id == 20)
                    .unwrap()
                    .prototype,
                None,
                Some("inserted-panel".into()),
            )
            .operations,
        );
    }
    apply
}

#[test]
fn atomic_apply_interleaved_new_text_import_link_and_new_property_claim_once() {
    use otrios_interfaces::oib::designer::{DefinitionId, NodeId, PropertyAddress};
    let ui = imported_fixture::admit(&imported_fixture::imported());
    let (mut authority, mut replica, mut session) = fixture_ui(ui);
    let next = replica.oib_history_source().unwrap().next_identity;
    let mut apply = interleaved_creation(&replica, false);
    let preview = replica.preview_oib_apply(&apply).unwrap();
    let seed = preview.doc.oib_resolved_seed().unwrap();
    let address=seed.cells.iter().filter(|c|matches!(c.default,Value::Text(_))).find_map(|c|{
        let address=super::super::super::super::oib::document::address_for_cell(&seed,c.id).ok()?;
        matches!(address,PropertyAddress::Definition{definition:DefinitionId(d),node:NodeId(n),..} if d==next&&n==next+3).then_some(address)
    }).unwrap();
    apply.changes.push(
        preview
            .oib_capture_change(&address, Some(Value::Text("Volume level".into())))
            .unwrap(),
    );
    assert_eq!(replica.oib_history_source().unwrap().next_identity, next);
    replica.apply_oib(apply).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Accepted { .. }
    ));
    let source = replica.oib_history_source().unwrap();
    assert_eq!(source.next_identity, next + 5);
    assert!(source.presets.iter().any(|p| p.id == next + 2));
    let root = source
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(next))
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == NodeId(next + 1))
        .unwrap();
    assert_eq!(
        root.visual_children,
        vec![NodeId(next + 3), NodeId(next + 4)]
    );
    let cell = replica.oib_property_register(&address).unwrap().cell;
    assert_eq!(
        replica.oib_active().unwrap()[&cell],
        Some(Value::Text("Volume level".into()))
    );
    assert!(authority.oib_identity_plan.is_none());
}

#[test]
fn atomic_apply_reservations_fill_implicit_holes_and_reject_future_or_duplicate_ids() {
    let (mut authority, mut replica, mut session) = fixture();
    let next = replica.oib_history_source().unwrap().next_identity;
    let valid = interleaved_creation(&replica, true);
    let preview = replica.preview_oib_apply(&valid).unwrap();
    assert_eq!(
        preview.oib_history_source().unwrap().next_identity,
        next + 6
    );
    assert!(
        preview
            .oib_allocations()
            .unwrap()
            .iter()
            .any(|i| i.id == next + 3)
    );
    let mut hole = valid.clone();
    hole.operations.pop();
    assert!(
        replica
            .preview_oib_apply(&hole)
            .err()
            .unwrap()
            .contains("contiguous")
    );
    let mut duplicate = valid.clone();
    if let DocumentEdit::Preset(PresetEdit::Create { preset }) = &mut duplicate.operations[3] {
        preset.id = next + 4;
    }
    assert!(
        replica
            .preview_oib_apply(&duplicate)
            .err()
            .unwrap()
            .contains("duplicate")
    );
    replica.apply_oib(valid).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Accepted { .. }
    ));
    assert!(
        replica
            .oib_allocations()
            .unwrap()
            .iter()
            .any(|i| i.id == next + 3)
    );
    assert_eq!(
        replica.oib_history_source().unwrap().next_identity,
        next + 6
    );
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

fn endpoint_descriptor(
    replica: &StateAdapter,
    definition: DefinitionId,
    offset: u64,
    name: &str,
) -> PropertyDescriptor {
    let seed = replica.doc.oib_resolved_seed().unwrap();
    let metadata = seed.designer.as_ref().unwrap();
    let component = metadata
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let mut descriptor = metadata
        .classes
        .iter()
        .flat_map(|class| &class.properties)
        .find(|p| p.policy.live_literal && matches!(p.default, Value::Text(_)))
        .unwrap()
        .clone();
    descriptor.id = PropertyId(
        crate::canvas::oib::document::next_property_id(metadata, component).unwrap() + offset,
    );
    descriptor.name = name.into();
    descriptor.label = name.into();
    descriptor.default = Value::Text(format!("{name} fallback"));
    descriptor.policy.app_binding = false;
    descriptor
}

fn endpoint_apply(
    authority: &mut Document,
    replica: &mut StateAdapter,
    session: &mut Session,
    operations: Vec<DocumentEdit>,
) {
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations = operations;
    replica.preview_oib_apply(&apply).unwrap();
    assert!(replica.apply_oib(apply).unwrap());
    settle(authority, replica, session);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Accepted { .. }
    ));
}

#[test]
fn endpoint_contract_removal_keeps_nodes_cursor_holes_and_retires_late_writes() {
    let (mut authority, mut replica, mut session) = fixture();
    let definition = DefinitionId(10);
    let first = endpoint_descriptor(&replica, definition, 0, "caption");
    let second = endpoint_descriptor(&replica, definition, 1, "description");
    let root = replica
        .oib_history_source()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap()
        .root;
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![
            DocumentEdit::Definition(DefinitionEdit::SetProperty {
                definition,
                descriptor: first.clone(),
            }),
            DocumentEdit::Definition(DefinitionEdit::SetProperty {
                definition,
                descriptor: second.clone(),
            }),
            DocumentEdit::Definition(DefinitionEdit::SetDefault {
                definition,
                node: root,
                property: first.id,
                value: Value::Text("Authored caption".into()),
            }),
            DocumentEdit::Definition(DefinitionEdit::Expose {
                definition,
                property: first.id,
                targets: vec![PropertyPath {
                    references: vec![],
                    node: root,
                    property: second.id,
                }],
            }),
        ],
    );
    let shared = PropertyAddress::Definition {
        definition,
        node: root,
        property: first.id,
    };
    let occurrence = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![],
            node: root,
            property: first.id,
        },
    };
    let stale: Vec<_> = [&shared, &occurrence]
        .into_iter()
        .map(|address| {
            replica
                .oib_capture_change(address, Some(Value::Text("Delayed write".into())))
                .unwrap()
        })
        .collect();
    let before = replica.oib_history_source().unwrap();
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
            definition,
            property: first.id,
        })],
    );
    let after = replica.oib_history_source().unwrap();
    let mut expected = before
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap()
        .clone();
    expected.properties.retain(|p| p.id != first.id);
    expected
        .nodes
        .iter_mut()
        .find(|node| node.id == root)
        .unwrap()
        .defaults
        .retain(|p| p.property != first.id);
    expected.exposed.retain(|p| p.property != first.id);
    assert_eq!(
        after
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .unwrap(),
        &expected,
        "removal changes only the declared endpoint and its owned fallback/forwarding row"
    );
    assert_eq!(after.next_identity, before.next_identity);
    assert_eq!(expected.next_property, second.id.0 + 1);
    assert_eq!(
        expected.properties.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![second.id]
    );
    assert!(replica.oib_property_register(&shared).is_err());
    assert!(replica.oib_property_register(&occurrence).is_err());
    let committed = authority.save().unwrap();
    authority.oib_write([82; 16], &stale).unwrap();
    assert_eq!(
        authority.save().unwrap(),
        committed,
        "ordinary delayed writes cannot recreate retired leaves while the node remains live"
    );
    let mut late_apply = replica.oib_apply_base().unwrap();
    late_apply.changes = stale.clone();
    assert!(
        replica
            .preview_oib_apply(&late_apply)
            .err()
            .unwrap()
            .contains("removed")
    );
    assert!(replica.apply_oib(late_apply).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert!(
        matches!(replica.oib_apply_result().unwrap().unwrap().result,ApplyResult::Rejected {message} if message.contains("removed"))
    );
    assert_eq!(authority.oib_source().unwrap(), after);
    let mut reuse = replica.oib_apply_base().unwrap();
    reuse
        .operations
        .push(DocumentEdit::Definition(DefinitionEdit::SetProperty {
            definition,
            descriptor: first.clone(),
        }));
    assert!(replica.preview_oib_apply(&reuse).is_err());
    assert!(replica.apply_oib(reuse).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Rejected { .. }
    ));
    assert_eq!(
        authority.oib_source().unwrap(),
        after,
        "SetProperty cannot reclaim a retired local ID"
    );
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
            definition,
            property: second.id,
        })],
    );
    let empty = replica.oib_history_source().unwrap();
    let component = empty
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    assert!(component.properties.is_empty());
    assert_eq!(
        component.next_property,
        second.id.0 + 1,
        "deleting the last endpoint retains the high-water mark"
    );
    let third = endpoint_descriptor(&replica, definition, 0, "new_caption");
    assert_eq!(third.id.0, component.next_property);
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::SetProperty {
            definition,
            descriptor: third.clone(),
        })],
    );
    assert!(third.id > second.id);
    let committed = authority.save().unwrap();
    authority.oib_write([83; 16], &stale).unwrap();
    assert_eq!(
        authority.save().unwrap(),
        committed,
        "a later declaration does not reopen the old semantic address"
    );
    assert!(replica.oib_property_register(&shared).is_err());
    assert!(replica.oib_property_register(&occurrence).is_err());
}

#[test]
fn endpoint_contract_inherited_and_incoming_uses_require_explicit_repair() {
    let (mut authority, mut replica, mut session) = fixture();
    let definition = DefinitionId(10);
    let target = endpoint_descriptor(&replica, definition, 0, "caption");
    let forwarding = endpoint_descriptor(&replica, definition, 1, "public_caption");
    let root = replica
        .oib_history_source()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap()
        .root;
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![
            DocumentEdit::Definition(DefinitionEdit::SetProperty {
                definition,
                descriptor: target.clone(),
            }),
            DocumentEdit::Definition(DefinitionEdit::SetProperty {
                definition,
                descriptor: forwarding.clone(),
            }),
            DocumentEdit::Definition(DefinitionEdit::Expose {
                definition,
                property: forwarding.id,
                targets: vec![PropertyPath {
                    references: vec![],
                    node: root,
                    property: target.id,
                }],
            }),
        ],
    );
    let before = authority.oib_source().unwrap();
    let seed = replica.doc.oib_resolved_seed().unwrap();
    let metadata = seed.designer.as_ref().unwrap();
    let component = metadata
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let root_class = component
        .nodes
        .iter()
        .find(|node| node.id == root)
        .unwrap()
        .class;
    let inherited = crate::canvas::oib::document::class_descriptors(&metadata.classes, root_class)
        .unwrap()[0]
        .id;
    for property in [target.id, inherited] {
        let mut apply = replica.oib_apply_base().unwrap();
        apply
            .operations
            .push(DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
                definition,
                property,
            }));
        let preview = replica.preview_oib_apply(&apply);
        assert!(
            preview.is_err(),
            "Unexpected removable property {property:?}; preview {:?}",
            preview.as_ref().map(|p| {
                let source = p.oib_history_source().unwrap();
                let d = source
                    .definitions
                    .iter()
                    .find(|d| d.id == definition)
                    .unwrap();
                (
                    d.properties.clone(),
                    d.exposed.clone(),
                    p.doc.oib_status().unwrap().error,
                )
            })
        );
        assert!(replica.apply_oib(apply).unwrap());
        settle(&mut authority, &mut replica, &mut session);
        assert!(matches!(
            replica.oib_apply_result().unwrap().unwrap().result,
            ApplyResult::Rejected { .. }
        ));
        assert_eq!(authority.oib_source().unwrap(), before);
    }
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![
            DocumentEdit::Definition(DefinitionEdit::Expose {
                definition,
                property: forwarding.id,
                targets: vec![],
            }),
            DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
                definition,
                property: target.id,
            }),
        ],
    );
    let after = authority.oib_source().unwrap();
    let component = after
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    assert_eq!(
        component
            .properties
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        vec![forwarding.id]
    );
    assert_eq!(
        component
            .exposed
            .iter()
            .find(|p| p.property == forwarding.id)
            .unwrap()
            .targets,
        vec![]
    );
}

#[test]
fn endpoint_contract_unused_visual_reader_blocks_removal_until_explicit_repair() {
    use otrios_interfaces::{
        canvas::Projection,
        layout::{Expr, Shape},
        oib::{designer::DefinitionNode, graph},
    };
    let (mut authority, mut replica, mut session) = fixture();
    let descriptor = endpoint_descriptor(&replica, DefinitionId(10), 0, "caption");
    let source = replica.oib_history_source().unwrap();
    let existing = source
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let class = existing
        .nodes
        .iter()
        .find(|node| node.id == existing.root)
        .unwrap()
        .class;
    let next = source.next_identity;
    let definition = DefinitionId(next);
    let root = NodeId(next + 1);
    let visual = graph::Visual {
        layout: None,
        frame: [0., 0., 320., 48.].map(|n| Expr::Literal(Value::Number(n))),
        resize: None,
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: true },
        semantics: None,
        action: None,
        // The dependency belongs to authored source even though this Canvas
        // does not consume the parameter and no preset ever instantiates it.
        parameters: vec![graph::Parameter {
            local: 1,
            source: graph::ParameterSource::Property(PropertyPath {
                references: vec![],
                node: root,
                property: descriptor.id,
            }),
            projection: Projection::Identity,
        }],
    };
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
            definition,
            name: "Unused reader".into(),
            root: DefinitionNode {
                id: root,
                name: "Panel".into(),
                class,
                owner: None,
                visual_children: vec![],
                defaults: vec![],
                reference: None,
                visual: Some(visual.clone()),
            },
            properties: vec![descriptor.clone()],
            next_property: descriptor.id.0 + 1,
        })],
    );
    let before = authority.oib_source().unwrap();
    assert!(
        !before
            .presets
            .iter()
            .any(|preset| preset.definition == definition)
    );
    assert!(
        !before
            .instances
            .iter()
            .any(|instance| instance.component == definition.0)
    );
    let mut removal = replica.oib_apply_base().unwrap();
    removal
        .operations
        .push(DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
            definition,
            property: descriptor.id,
        }));
    assert!(
        replica.preview_oib_apply(&removal).is_err(),
        "unused visual property paths must be admitted, not skipped with renderer expansion"
    );
    assert!(replica.apply_oib(removal).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Rejected { .. }
    ));
    assert_eq!(authority.oib_source().unwrap(), before);
    let mut repaired_visual = visual;
    repaired_visual.parameters.clear();
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![
            DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
                definition,
                property: descriptor.id,
            }),
            DocumentEdit::Definition(DefinitionEdit::SetVisual {
                definition,
                node: root,
                visual: Some(repaired_visual.clone()),
            }),
        ],
    );
    let after = authority.oib_source().unwrap();
    let component = after
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    assert!(component.properties.is_empty());
    assert_eq!(component.next_property, descriptor.id.0 + 1);
    assert_eq!(
        component
            .nodes
            .iter()
            .find(|node| node.id == root)
            .unwrap()
            .visual
            .as_ref(),
        Some(&repaired_visual)
    );
    assert_eq!(after.next_identity, before.next_identity);
}

#[test]
fn endpoint_contract_sparse_initial_namespace_requires_fresh_global_definition_identity() {
    use otrios_interfaces::{
        layout::{Expr, Shape},
        oib::{designer::DefinitionNode, graph},
    };
    let (mut authority, mut replica, mut session) = fixture();
    let first = endpoint_descriptor(&replica, DefinitionId(10), 0, "caption");
    let second = endpoint_descriptor(&replica, DefinitionId(10), 2, "description");
    let next = replica.oib_history_source().unwrap().next_identity;
    let definition = DefinitionId(next);
    let root_class = replica
        .oib_history_source()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.owner.is_none())
        .unwrap()
        .class;
    let create = DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
        definition,
        name: "Sparse endpoint component".into(),
        root: DefinitionNode {
            id: NodeId(next + 1),
            name: "Panel".into(),
            class: root_class,
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
        properties: vec![first.clone(), second.clone()],
        next_property: second.id.0 + 3,
    });
    let preset = DocumentEdit::Preset(PresetEdit::Create {
        preset: graph::Preset {
            id: next + 2,
            name: "Sparse endpoint component".into(),
            definition,
            assignments: vec![],
            frame: None,
            palette: true,
        },
    });
    let mut malformed = replica.oib_apply_base().unwrap();
    malformed.operations = vec![create.clone(), preset.clone()];
    if let DocumentEdit::Definition(DefinitionEdit::CreateDefinition { next_property, .. }) =
        &mut malformed.operations[0]
    {
        *next_property = second.id.0;
    }
    assert!(
        replica.preview_oib_apply(&malformed).is_err(),
        "every live ID must be below the cursor"
    );
    assert!(replica.apply_oib(malformed).unwrap());
    settle(&mut authority, &mut replica, &mut session);
    assert!(matches!(
        replica.oib_apply_result().unwrap().unwrap().result,
        ApplyResult::Rejected { .. }
    ));
    assert_eq!(authority.oib_source().unwrap().next_identity, next);
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![create.clone(), preset],
    );
    let source = authority.oib_source().unwrap();
    let component = source
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    assert_eq!(component.properties, vec![first, second.clone()]);
    assert_eq!(component.next_property, second.id.0 + 3);
    assert_eq!(source.next_identity, next + 3);
    for removed in [false, true] {
        if removed {
            endpoint_apply(
                &mut authority,
                &mut replica,
                &mut session,
                vec![
                    DocumentEdit::Definition(DefinitionEdit::DeleteDefinition { definition }),
                    DocumentEdit::Preset(PresetEdit::Delete { preset: next + 2 }),
                ],
            );
        }
        let before = authority.oib_source().unwrap();
        let mut reuse = replica.oib_apply_base().unwrap();
        reuse.operations = vec![create.clone()];
        assert!(replica.preview_oib_apply(&reuse).is_err());
        assert!(replica.apply_oib(reuse).unwrap());
        settle(&mut authority, &mut replica, &mut session);
        assert!(matches!(
            replica.oib_apply_result().unwrap().unwrap().result,
            ApplyResult::Rejected { .. }
        ));
        assert_eq!(
            authority.oib_source().unwrap(),
            before,
            "existing and retired global definition IDs cannot initialize a namespace again"
        );
    }
}

#[test]
fn endpoint_contract_reset_of_empty_baseline_preserves_retired_cursor() {
    let (mut authority, mut replica, mut session) = fixture();
    let definition = DefinitionId(10);
    let descriptor = endpoint_descriptor(&replica, definition, 0, "temporary_caption");
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::SetProperty {
            definition,
            descriptor: descriptor.clone(),
        })],
    );
    let source = authority.oib_source().unwrap();
    let component = source
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let root = component.root;
    let cursor = component.next_property;
    let address = PropertyAddress::Definition {
        definition,
        node: root,
        property: descriptor.id,
    };
    let delayed = replica
        .oib_capture_change(&address, Some(Value::Text("Old caption".into())))
        .unwrap();
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
            definition,
            property: descriptor.id,
        })],
    );
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Restore { defaults: true }],
    );
    let restored = authority.oib_source().unwrap();
    let component = restored
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    assert_eq!(component.root, root);
    assert!(component.properties.is_empty());
    assert_eq!(
        component.next_property, cursor,
        "Reset must not reopen a surviving definition's retired property interval"
    );
    let committed = authority.save().unwrap();
    authority.oib_write([82; 16], &[delayed]).unwrap();
    assert_eq!(authority.save().unwrap(), committed);
    let next = endpoint_descriptor(&replica, definition, 0, "new_caption");
    assert_eq!(next.id.0, cursor);
    assert!(next.id > descriptor.id);
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Definition(DefinitionEdit::SetProperty {
            definition,
            descriptor: next,
        })],
    );
    assert!(replica.oib_property_register(&address).is_err());
}

#[test]
fn endpoint_contract_reset_restores_signed_property_at_fresh_id_and_remaps_uses() {
    use otrios_interfaces::{
        canvas::Projection,
        oib::{
            designer::{ExposedProperty, PropertyDefault},
            graph,
        },
    };
    let (_, template, _) = fixture();
    let definition = DefinitionId(10);
    let first = endpoint_descriptor(&template, definition, 0, "baseline_caption");
    let second = endpoint_descriptor(&template, definition, 1, "baseline_description");
    let mut ui = app::ui(&[]);
    let graph = imported_fixture::seed_mut(&mut ui).graph.as_mut().unwrap();
    let component = graph
        .definitions
        .iter_mut()
        .find(|d| d.id == definition)
        .unwrap();
    let root = component.root;
    let path = PropertyPath {
        references: vec![],
        node: root,
        property: first.id,
    };
    component.properties = vec![first.clone(), second.clone()];
    component.next_property = second.id.0 + 1;
    component.exposed.push(ExposedProperty {
        property: first.id,
        targets: vec![PropertyPath {
            references: vec![],
            node: root,
            property: second.id,
        }],
    });
    component.exposed.sort_by_key(|p| p.property);
    let root_node = component
        .nodes
        .iter_mut()
        .find(|node| node.id == root)
        .unwrap();
    root_node.defaults.push(PropertyDefault {
        property: first.id,
        value: Value::Text("Signed root fallback".into()),
    });
    root_node.defaults.sort_by_key(|p| p.property);
    let visual = root_node.visual.as_mut().unwrap();
    let local = visual.parameters.iter().map(|p| p.local).max().unwrap_or(0) + 1;
    visual.parameters.push(graph::Parameter {
        local,
        source: graph::ParameterSource::Property(path.clone()),
        projection: Projection::Identity,
    });
    for preset in graph
        .presets
        .iter_mut()
        .filter(|p| p.definition == definition)
    {
        preset.assignments.push(graph::Assignment {
            target: path.clone(),
            source: graph::Source::Literal(Value::Text("Signed preset caption".into())),
        });
        preset.assignments.sort_by(|a, b| a.target.cmp(&b.target));
    }
    let (mut authority, mut replica, mut session) = fixture_ui(imported_fixture::admit(&ui));
    let shared = PropertyAddress::Definition {
        definition,
        node: root,
        property: first.id,
    };
    let occurrence = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: path.clone(),
    };
    let delayed: Vec<_> = [&shared, &occurrence]
        .into_iter()
        .map(|address| {
            replica
                .oib_capture_change(address, Some(Value::Text("Old signed address".into())))
                .unwrap()
        })
        .collect();
    let original = authority.oib_source().unwrap();
    let component = original
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let mut repaired_visual = component
        .nodes
        .iter()
        .find(|node| node.id == root)
        .unwrap()
        .visual
        .clone()
        .unwrap();
    repaired_visual.parameters.retain(|p| p.local != local);
    let mut repair = vec![
        DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
            definition,
            property: first.id,
        }),
        DocumentEdit::Definition(DefinitionEdit::SetVisual {
            definition,
            node: root,
            visual: Some(repaired_visual),
        }),
    ];
    for preset in original
        .presets
        .iter()
        .filter(|p| p.definition == definition)
    {
        let mut preset = preset.clone();
        preset.assignments.retain(|a| a.target != path);
        repair.push(DocumentEdit::Preset(PresetEdit::Update { preset }));
    }
    endpoint_apply(&mut authority, &mut replica, &mut session, repair);
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Restore { defaults: true }],
    );
    let restored = authority.oib_source().unwrap();
    let component = restored
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let fresh = component
        .properties
        .iter()
        .find(|p| p.name == first.name)
        .unwrap()
        .id;
    assert_eq!(
        fresh.0,
        original
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .unwrap()
            .next_property
    );
    assert!(fresh > second.id);
    assert_eq!(component.next_property, fresh.0 + 1);
    assert_eq!(component.root, root);
    assert!(restored.baseline.contains(&BaselineIdentity::Property {
        definition,
        source: first.id,
        current: fresh
    }));
    let root_node = component.nodes.iter().find(|node| node.id == root).unwrap();
    assert!(
        root_node
            .defaults
            .iter()
            .any(|p| p.property == fresh && p.value == Value::Text("Signed root fallback".into()))
    );
    let fresh_path = PropertyPath {
        references: vec![],
        node: root,
        property: fresh,
    };
    assert_eq!(
        component
            .exposed
            .iter()
            .find(|p| p.property == fresh)
            .unwrap()
            .targets,
        vec![PropertyPath {
            references: vec![],
            node: root,
            property: second.id
        }]
    );
    assert!(
        root_node
            .visual
            .as_ref()
            .unwrap()
            .parameters
            .iter()
            .any(|p| p.local == local
                && p.source == graph::ParameterSource::Property(fresh_path.clone()))
    );
    assert!(
        restored
            .presets
            .iter()
            .filter(|p| p.definition == definition)
            .all(|p| p.assignments.iter().any(|a| a.target == fresh_path))
    );
    let new_occurrence = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: fresh_path,
    };
    assert!(replica.oib_property_register(&new_occurrence).is_ok());
    assert!(replica.oib_property_register(&shared).is_err());
    assert!(replica.oib_property_register(&occurrence).is_err());
    let committed = authority.save().unwrap();
    authority.oib_write([82; 16], &delayed).unwrap();
    assert_eq!(
        authority.save().unwrap(),
        committed,
        "Reset cannot reanimate old signed definition or occurrence addresses"
    );
    replica
        .edit_oib_property(&new_occurrence, Some(Value::Text("Fresh override".into())))
        .unwrap();
    settle(&mut authority, &mut replica, &mut session);
    endpoint_apply(
        &mut authority,
        &mut replica,
        &mut session,
        vec![DocumentEdit::Restore { defaults: true }],
    );
    let repeated = authority.oib_source().unwrap();
    let component = repeated
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    assert_eq!(
        component
            .properties
            .iter()
            .find(|p| p.name == first.name)
            .unwrap()
            .id,
        fresh
    );
    assert_eq!(
        component.next_property,
        fresh.0 + 1,
        "repeated Reset preserves its live baseline role"
    );
    assert_eq!(
        replica
            .oib_property_register(&new_occurrence)
            .unwrap()
            .alternatives,
        vec![None]
    );
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
    let address = color_change(&replica, [240, 20, 30, 255]).address;
    assert_eq!(
        replica
            .oib_apply_result()
            .unwrap()
            .unwrap()
            .property_versions,
        vec![
            replica
                .oib_property_register(&address)
                .unwrap()
                .version
                .to_vec()
        ]
    );
}

#[test]
fn history_conditional_inverse_keeps_unrelated_source_and_rejects_equal_value_aba() {
    use crate::canvas::oib::history::Entry;
    let (mut authority, mut replica, mut session) = fixture();
    let before = replica.oib_history_source().unwrap();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations.push(rename(&replica));
    apply
        .changes
        .push(color_change(&replica, [240, 20, 30, 255]));
    replica.apply_oib(apply.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let receipt = replica.oib_apply_result().unwrap().unwrap();
    let entry = Entry::capture(
        &before,
        &replica.oib_history_source().unwrap(),
        &apply,
        &receipt,
    )
    .unwrap();
    assert!(entry.encoded_bytes().unwrap() <= crate::canvas::oib::history::MAX_HISTORY_BYTES);
    let other = before
        .definitions
        .iter()
        .find(|d| d.name != "SettingsPanel")
        .unwrap()
        .id;
    let mut remote = replica.oib_apply_base().unwrap();
    remote
        .operations
        .push(DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
            definition: other,
            name: "Other author name".into(),
        }));
    replica.apply_oib(remote).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let changes: Vec<_> = entry
        .property_values()
        .into_iter()
        .map(|(a, v)| replica.oib_capture_change(&a, v).unwrap())
        .collect();
    let inverse = entry
        .plan(
            &replica.oib_history_source().unwrap(),
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &changes,
        )
        .unwrap();
    replica.apply_oib(inverse).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let restored = replica.oib_history_source().unwrap();
    assert!(
        restored
            .definitions
            .iter()
            .any(|d| d.name == "SettingsPanel")
    );
    assert!(
        restored
            .definitions
            .iter()
            .any(|d| d.id == other && d.name == "Other author name")
    );
    // Another author writes exactly the history's expected visible property.
    // The actual accepted receipt proof must still detect the newer context.
    let mut second = replica.oib_apply_base().unwrap();
    second
        .changes
        .push(color_change(&replica, [22, 33, 44, 255]));
    let second_before = replica.oib_history_source().unwrap();
    replica.apply_oib(second.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let receipt = replica.oib_apply_result().unwrap().unwrap();
    let entry = Entry::capture(
        &second_before,
        &replica.oib_history_source().unwrap(),
        &second,
        &receipt,
    )
    .unwrap();
    replica
        .edit_oib_property(&second.changes[0].address, second.changes[0].value.clone())
        .unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let changes: Vec<_> = entry
        .property_values()
        .into_iter()
        .map(|(a, v)| replica.oib_capture_change(&a, v).unwrap())
        .collect();
    assert!(
        !entry
            .review(
                &replica.oib_history_source().unwrap(),
                replica.oib_generation().unwrap(),
                &changes
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        entry
            .plan(
                &replica.oib_history_source().unwrap(),
                replica.oib_generation().unwrap(),
                replica.oib_apply_base().unwrap(),
                &changes
            )
            .is_err()
    );
    assert_eq!(replica.oib_apply_result().unwrap(), Some(receipt));
}

#[test]
fn history_default_inverse_restores_absence_without_changing_override_registers() {
    use crate::canvas::oib::history::Entry;
    let (mut authority, mut replica, mut session) = fixture();
    let source = replica.oib_history_source().unwrap();
    let (definition, node, property) = source
        .definitions
        .iter()
        .find_map(|d| {
            d.nodes.iter().find_map(|n| {
                n.defaults
                    .iter()
                    .find(|v| matches!(v.value, Value::Text(_)))
                    .map(|v| (d.id, n.id, v.property))
            })
        })
        .unwrap();
    let mut clear = replica.oib_apply_base().unwrap();
    clear
        .operations
        .push(DocumentEdit::Definition(DefinitionEdit::ClearDefault {
            definition,
            node,
            property,
        }));
    replica.apply_oib(clear).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let before = replica.oib_history_source().unwrap();
    assert!(
        !before
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.id == node)
            .unwrap()
            .defaults
            .iter()
            .any(|v| v.property == property)
    );
    let address = otrios_interfaces::oib::designer::PropertyAddress::Definition {
        definition,
        node,
        property,
    };
    let version = replica.oib_property_register(&address).unwrap().version;
    let mut apply = replica.oib_apply_base().unwrap();
    apply
        .operations
        .push(DocumentEdit::Definition(DefinitionEdit::SetDefault {
            definition,
            node,
            property,
            value: Value::Text("Authored value".into()),
        }));
    replica.apply_oib(apply.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let entry = Entry::capture(
        &before,
        &replica.oib_history_source().unwrap(),
        &apply,
        &replica.oib_apply_result().unwrap().unwrap(),
    )
    .unwrap();
    let inverse = entry
        .plan(
            &replica.oib_history_source().unwrap(),
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &[],
        )
        .unwrap();
    assert!(matches!(
        inverse.operations.as_slice(),
        [DocumentEdit::Definition(
            DefinitionEdit::ClearDefault { .. }
        )]
    ));
    replica.apply_oib(inverse).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(
        replica.oib_history_source().unwrap().definitions,
        before.definitions
    );
    assert_eq!(
        replica.oib_property_register(&address).unwrap().version,
        version
    );
}

#[test]
fn history_legacy_receipt_has_no_inferred_property_proof() {
    use crate::canvas::oib::history::Entry;
    let (mut authority, mut replica, mut session) = fixture();
    let before = replica.oib_history_source().unwrap();
    let mut apply = replica.oib_apply_base().unwrap();
    apply
        .changes
        .push(color_change(&replica, [12, 34, 56, 255]));
    replica.apply_oib(apply.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let mut receipt = replica.oib_apply_result().unwrap().unwrap();
    receipt.property_versions.clear();
    let decoded: ApplyReceipt =
        flexbuffers::from_slice(&flexbuffers::to_vec(&receipt).unwrap()).unwrap();
    assert!(decoded.property_versions.is_empty());
    assert!(
        Entry::capture(
            &before,
            &replica.oib_history_source().unwrap(),
            &apply,
            &decoded
        )
        .unwrap_err()
        .contains("proof is unavailable")
    );
}

#[test]
fn history_reanchors_only_the_acknowledged_local_undo_chain() {
    use crate::canvas::oib::history::Entry;
    let (mut authority, mut replica, mut session) = fixture();
    let mut entries = vec![];
    for color in [[10, 20, 30, 255], [40, 50, 60, 255]] {
        let before = replica.oib_history_source().unwrap();
        let mut apply = replica.oib_apply_base().unwrap();
        apply.changes.push(color_change(&replica, color));
        replica.apply_oib(apply.clone()).unwrap();
        settle(&mut authority, &mut replica, &mut session);
        entries.push(
            Entry::capture(
                &before,
                &replica.oib_history_source().unwrap(),
                &apply,
                &replica.oib_apply_result().unwrap().unwrap(),
            )
            .unwrap(),
        );
    }
    let newer = entries.pop().unwrap();
    let mut older = entries.pop().unwrap();
    let changes: Vec<_> = newer
        .property_values()
        .into_iter()
        .map(|(a, v)| replica.oib_capture_change(&a, v).unwrap())
        .collect();
    let source = replica.oib_history_source().unwrap();
    let inverse = newer
        .plan(
            &source,
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &changes,
        )
        .unwrap();
    replica.apply_oib(inverse.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let acknowledged = Entry::capture(
        &source,
        &replica.oib_history_source().unwrap(),
        &inverse,
        &replica.oib_apply_result().unwrap().unwrap(),
    )
    .unwrap();
    let changes: Vec<_> = older
        .property_values()
        .into_iter()
        .map(|(a, v)| replica.oib_capture_change(&a, v).unwrap())
        .collect();
    assert!(
        older
            .plan(
                &replica.oib_history_source().unwrap(),
                replica.oib_generation().unwrap(),
                replica.oib_apply_base().unwrap(),
                &changes
            )
            .is_err()
    );
    assert_eq!(older.reanchor_after_undo(&newer, &acknowledged), 1);
    assert!(
        older
            .plan(
                &replica.oib_history_source().unwrap(),
                replica.oib_generation().unwrap(),
                replica.oib_apply_base().unwrap(),
                &changes
            )
            .is_ok()
    );
}

#[test]
fn history_reorder_restores_original_anchor_dependencies() {
    use crate::canvas::oib::history::Entry;
    let (mut authority, mut replica, mut session) = fixture();
    let before = replica.oib_history_source().unwrap();
    let definition = before
        .definitions
        .iter()
        .find(|d| d.name == "SettingsPanel")
        .unwrap();
    let root = definition
        .nodes
        .iter()
        .find(|n| n.id == definition.root)
        .unwrap();
    assert!(root.visual_children.len() >= 3);
    let mut apply = replica.oib_apply_base().unwrap();
    for n in root.visual_children.iter().take(2) {
        apply
            .operations
            .push(DocumentEdit::Definition(DefinitionEdit::MoveNode {
                definition: definition.id,
                node: *n,
                owner: definition.root,
                visual_parent: Some(definition.root),
                before: None,
            }));
    }
    replica.apply_oib(apply.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let entry = Entry::capture(
        &before,
        &replica.oib_history_source().unwrap(),
        &apply,
        &replica.oib_apply_result().unwrap().unwrap(),
    )
    .unwrap();
    let inverse = entry
        .plan(
            &replica.oib_history_source().unwrap(),
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &[],
        )
        .unwrap();
    replica.apply_oib(inverse).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(
        replica.oib_history_source().unwrap().definitions,
        before.definitions
    );
}

#[test]
fn history_visual_node_name_preset_inverse_and_human_review_preserve_fields() {
    use crate::canvas::oib::history::Entry;
    let (mut authority, mut replica, mut session) = fixture();
    let before = replica.oib_history_source().unwrap();
    let definition = before
        .definitions
        .iter()
        .find(|d| d.name == "SettingsPanel")
        .unwrap();
    let node = definition
        .nodes
        .iter()
        .find(|n| n.id == definition.root)
        .unwrap();
    let mut visual = node.visual.clone().unwrap();
    visual.visible = otrios_interfaces::layout::Expr::Literal(Value::Boolean(false));
    let mut preset = before
        .presets
        .iter()
        .find(|p| p.definition == definition.id)
        .unwrap()
        .clone();
    preset.name = "History preset".into();
    let mut apply = replica.oib_apply_base().unwrap();
    apply.operations = vec![
        DocumentEdit::Definition(DefinitionEdit::SetVisual {
            definition: definition.id,
            node: node.id,
            visual: Some(visual),
        }),
        DocumentEdit::Definition(DefinitionEdit::RenameNode {
            definition: definition.id,
            node: node.id,
            name: "History root".into(),
        }),
        DocumentEdit::Preset(PresetEdit::Update { preset }),
    ];
    replica.apply_oib(apply.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let after = replica.oib_history_source().unwrap();
    let entry = Entry::capture(
        &before,
        &after,
        &apply,
        &replica.oib_apply_result().unwrap().unwrap(),
    )
    .unwrap();
    let mut changed = after.clone();
    changed
        .definitions
        .iter_mut()
        .find(|d| d.id == definition.id)
        .unwrap()
        .nodes
        .iter_mut()
        .find(|n| n.id == node.id)
        .unwrap()
        .name = "Another author".into();
    let review = entry
        .review(&changed, replica.oib_generation().unwrap(), &[])
        .unwrap();
    assert_eq!(review.len(), 1);
    assert!(review[0].target.contains("SettingsPanel"));
    assert_eq!(review[0].before_text, node.name);
    assert_eq!(review[0].applied_text, "History root");
    assert_eq!(review[0].current_text, "Another author");
    let inverse = entry
        .plan(
            &after,
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &[],
        )
        .unwrap();
    replica.apply_oib(inverse).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let restored = replica.oib_history_source().unwrap();
    assert_eq!(restored.definitions, before.definitions);
    assert_eq!(restored.presets, before.presets);
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
    use crate::canvas::oib::history::Entry;
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
            properties: vec![],
            next_property: 0,
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
    replica.apply_oib(apply.clone()).unwrap();
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
    let entry = Entry::capture(
        &original,
        &source,
        &apply,
        &replica.oib_apply_result().unwrap().unwrap(),
    )
    .unwrap();
    assert!(
        entry
            .plan(
                &source,
                replica.oib_generation().unwrap() + 1,
                replica.oib_apply_base().unwrap(),
                &[]
            )
            .is_err()
    );
    let inverse = entry
        .plan(
            &source,
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &[],
        )
        .unwrap();
    replica.apply_oib(inverse.clone()).unwrap();
    settle(&mut authority, &mut replica, &mut session);
    let removed = replica.oib_history_source().unwrap();
    assert_eq!(removed.definitions, original.definitions);
    assert_eq!(removed.presets, original.presets);
    assert_eq!(removed.next_identity, next + 3);
    let redo_entry = Entry::capture(
        &source,
        &removed,
        &inverse,
        &replica.oib_apply_result().unwrap().unwrap(),
    )
    .unwrap();
    let redo = redo_entry
        .plan(
            &removed,
            replica.oib_generation().unwrap(),
            replica.oib_apply_base().unwrap(),
            &[],
        )
        .unwrap();
    let preview = replica.preview_oib_apply(&redo).unwrap();
    assert_eq!(authority.oib_source().unwrap(), removed);
    let restored = preview.oib_history_source().unwrap();
    let definition = restored
        .definitions
        .iter()
        .find(|d| d.name == "Created once")
        .unwrap();
    assert_eq!(definition.id, DefinitionId(next + 3));
    assert_eq!(definition.root, NodeId(next + 4));
    let preset = restored
        .presets
        .iter()
        .find(|p| p.name == "Created once")
        .unwrap();
    assert_eq!(preset.id, next + 5);
    assert_eq!(preset.definition, definition.id);
    assert!(
        !restored
            .definitions
            .iter()
            .any(|d| d.id == DefinitionId(next))
    );
    replica.apply_oib(redo).unwrap();
    let request = replica.generate().unwrap().unwrap();
    authority.receive(&mut session, &request).unwrap();
    let committed = authority.save().unwrap();
    assert!(!authority.receive(&mut session, &request).unwrap());
    assert_eq!(authority.save().unwrap(), committed);
    settle(&mut authority, &mut replica, &mut session);
    assert_eq!(authority.oib_source().unwrap(), restored);
    assert_eq!(restored.next_identity, next + 6);
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
