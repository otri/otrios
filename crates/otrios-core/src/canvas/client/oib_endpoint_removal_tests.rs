// SPDX-License-Identifier: LGPL-3.0-only
use super::tests::{
    activate_studio_tool, editor_address, focus_address, mounted_fixture_session_with_ui,
};
use super::*;
use otrios_interfaces::{
    keyboard::Edit,
    layout::ValueType,
    oib::{authoring::*, designer::*},
};

fn settle(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
    for _ in 0..40 {
        let mut progress = false;
        while let Some(bytes) = client.active.as_mut().unwrap().doc.generate().unwrap() {
            authority.receive(session, &bytes).unwrap();
            progress = true;
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
        if !progress {
            let m = client.active.as_mut().unwrap();
            assert!(m.doc.confirmed());
            m.refresh().unwrap();
            return;
        }
    }
    panic!("endpoint removal fixture did not settle")
}

fn declared() -> (
    Client,
    crate::canvas::Document,
    Session,
    DefinitionId,
    NodeId,
    PropertyId,
) {
    let ui = crate::test_support::oib_designer::ui(&[]);
    let ui = crate::canvas::decode_ui(&crate::canvas::encode_ui(&ui).unwrap()).unwrap();
    let (mut client, mut authority, mut session) = mounted_fixture_session_with_ui(true, 0, ui);
    client.designer_new_component().unwrap();
    let Selection::Definition(id) = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .selection
    else {
        panic!("new definition")
    };
    let seed = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_authoring_seed()
        .unwrap();
    let linked_name = &seed
        .designer
        .as_ref()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(12))
        .unwrap()
        .name;
    activate_studio_tool(&mut client, &format!("Link {linked_name}"));
    let m = client.active.as_mut().unwrap();
    let seed = m.design_doc().oib_authoring_seed().unwrap();
    let metadata = seed.designer.as_ref().unwrap();
    let definition = metadata.definitions.iter().find(|d| d.id == id).unwrap();
    let root = definition.root;
    let reference = definition
        .nodes
        .iter()
        .find(|node| node.reference.is_some())
        .unwrap()
        .id;
    let mut descriptor = metadata
        .classes
        .iter()
        .flat_map(|class| &class.properties)
        .find(|p| p.value_type == ValueType::Text)
        .unwrap()
        .clone();
    descriptor.id =
        PropertyId(crate::canvas::oib::document::next_property_id(metadata, definition).unwrap());
    descriptor.name = "caption".into();
    descriptor.label = "Caption".into();
    descriptor.policy = PropertyPolicy {
        live_literal: true,
        app_binding: false,
    };
    let property = descriptor.id;
    m.stage_structure(EditBatch {
        generation: m.design_doc().oib_structural_generation().unwrap(),
        operations: vec![
            DocumentEdit::Definition(DefinitionEdit::SetProperty {
                definition: id,
                descriptor,
            }),
            DocumentEdit::Definition(DefinitionEdit::SetDefault {
                definition: id,
                node: root,
                property,
                value: Value::Text("Endpoint fallback".into()),
            }),
            DocumentEdit::Definition(DefinitionEdit::Expose {
                definition: id,
                property,
                targets: vec![PropertyPath {
                    references: vec![reference],
                    node: NodeId(1),
                    property: PropertyId(3),
                }],
            }),
        ],
    })
    .unwrap();
    m.stage_property(
        &PropertyAddress::Definition {
            definition: id,
            node: root,
            property,
        },
        Some(Value::Text("Preserved endpoint literal".into())),
    )
    .unwrap();
    assert!(client.designer_apply_working_copy().unwrap());
    settle(&mut client, &mut authority, &mut session);
    client
        .inspect_oib_selection(Selection::Definition(id))
        .unwrap();
    (client, authority, session, id, root, property)
}

fn activate_panel(client: &mut Client, label: &str) {
    let (id, y, viewport, current) = {
        let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
        let panel = d.panels.last().unwrap();
        let row = panel
            .nodes
            .iter()
            .find(|node| node.semantics.as_ref().is_some_and(|s| s.label == label))
            .unwrap_or_else(|| panic!("Missing endpoint control {label}"));
        (
            row.id,
            row.frame.y,
            panel.viewport,
            panel.scroll.info().offset[1],
        )
    };
    let scene = client.scene().unwrap();
    let path = scene.path(u32::from(viewport)).unwrap();
    let origin = path.last().unwrap().origin;
    client
        .scroll_at(
            client.epoch(),
            y - 8. - current,
            [origin[0] + 10., origin[1] + 10.],
        )
        .unwrap();
    assert!(
        client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .any(|node| node.node.id == id)
    );
    assert!(client.activate(client.epoch(), id).unwrap());
}

fn open_review(client: &mut Client) {
    activate_panel(client, "> Component API");
    activate_panel(client, "> caption · Text");
    activate_panel(client, "Remove property...");
}
fn activate_modal(client: &mut Client, label: &str) {
    let id = client
        .active
        .as_ref()
        .unwrap()
        .delete_modal()
        .unwrap()
        .actions
        .iter()
        .find(|action| action.label == label && action.enabled)
        .unwrap()
        .id;
    assert!(client.activate(client.epoch(), id).unwrap());
}

#[test]
fn endpoint_removal_ui_reviews_cancel_and_confirm_without_removing_its_component() {
    let (mut client, mut authority, mut session, id, root, property) = declared();
    let before = authority.save().unwrap();
    let allocations = authority.oib_allocations().unwrap();
    let app_values = [1, 2, 3, 4].map(|field| {
        client
            .active
            .as_ref()
            .unwrap()
            .doc
            .typed_value(field)
            .unwrap()
    });
    let source = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_history_source()
        .unwrap();
    let original = source.definitions.iter().find(|d| d.id == id).unwrap();
    let cursor = original.next_property;
    let outgoing = original
        .exposed
        .iter()
        .find(|row| row.property == property)
        .unwrap()
        .targets
        .clone();
    let node_ids = original
        .nodes
        .iter()
        .map(|node| node.id)
        .collect::<Vec<_>>();
    client
        .active
        .as_mut()
        .unwrap()
        .stage_property(
            &PropertyAddress::Definition {
                definition: id,
                node: root,
                property,
            },
            Some(Value::Text("Pending value removed with endpoint".into())),
        )
        .unwrap();
    open_review(&mut client);
    let modal = client.active.as_ref().unwrap().delete_modal().unwrap();
    assert_eq!(modal.title, "Remove property?");
    assert!(
        modal
            .lines
            .iter()
            .any(|line| line.contains("outgoing forwarding"))
    );
    assert!(client.text_inputs().unwrap().is_empty());
    assert!(
        !client
            .activate(client.epoch(), oib_workspace::UNDO)
            .unwrap(),
        "modal blocks background history"
    );
    for viewport in [[800., 480.], [1280., 800.]] {
        client.set_viewport(viewport).unwrap();
        let scene = client.scene().unwrap();
        assert!(scene.controls().len() <= 128);
        for action in &client
            .active
            .as_ref()
            .unwrap()
            .delete_modal()
            .unwrap()
            .actions
        {
            let path = scene
                .path(action.id)
                .expect("footer remains in the scene after resize");
            let node = path.last().unwrap();
            assert!(
                node.origin[1] >= 0. && node.origin[1] + node.node.frame.height <= viewport[1],
                "footer remains inside the resized viewport"
            );
            let center = [
                node.origin[0] + node.node.frame.width / 2.,
                node.origin[1] + node.node.frame.height / 2.,
            ];
            assert!(
                scene
                    .hit_path(center)
                    .iter()
                    .any(|node| node.node.id == action.id),
                "footer is actually hittable"
            );
        }
    }
    activate_modal(&mut client, "Keep property");
    assert_eq!(authority.save().unwrap(), before);
    // Reopening retains the API disclosure, so use the already-visible action.
    activate_panel(&mut client, "Remove property...");
    activate_modal(&mut client, "Remove property");
    let m = client.active.as_ref().unwrap();
    assert!(m.delete_review.is_none());
    assert!(
        m.working_copy.as_ref().unwrap().apply.changes.is_empty(),
        "confirmed removal drops its staged literal"
    );
    let candidate = m.design_doc().oib_history_source().unwrap();
    let definition = candidate.definitions.iter().find(|d| d.id == id).unwrap();
    assert!(definition.properties.iter().all(|p| p.id != property));
    assert!(definition.exposed.iter().all(|p| p.property != property));
    assert!(
        definition
            .nodes
            .iter()
            .find(|node| node.id == definition.root)
            .unwrap()
            .defaults
            .iter()
            .all(|p| p.property != property)
    );
    assert_eq!(
        definition
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        node_ids
    );
    assert_eq!(definition.next_property, cursor);
    assert_eq!(
        authority.save().unwrap(),
        before,
        "confirmation only stages a private candidate"
    );
    assert!(client.designer_apply_working_copy().unwrap());
    settle(&mut client, &mut authority, &mut session);
    let applied = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_history_source()
        .unwrap();
    assert!(
        applied
            .definitions
            .iter()
            .find(|d| d.id == id)
            .unwrap()
            .properties
            .is_empty()
    );
    assert!(client.designer_history(false).unwrap());
    let fresh = PropertyId(cursor);
    let m = client.active.as_ref().unwrap();
    assert!(m.working_copy.as_ref().unwrap().history_review().is_none());
    let restored = m.design_doc().oib_history_source().unwrap();
    let restored = restored.definitions.iter().find(|d| d.id == id).unwrap();
    assert_eq!(restored.next_property, cursor + 1);
    assert_eq!(restored.properties[0].id, fresh);
    assert_ne!(fresh, property);
    assert_eq!(
        restored
            .exposed
            .iter()
            .find(|row| row.property == fresh)
            .unwrap()
            .targets,
        outgoing
    );
    assert_eq!(
        restored
            .nodes
            .iter()
            .find(|node| node.id == root)
            .unwrap()
            .defaults
            .iter()
            .find(|row| row.property == fresh)
            .unwrap()
            .value,
        Value::Text("Endpoint fallback".into())
    );
    assert_eq!(
        restored
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        node_ids
    );
    assert_eq!(
        m.design_doc()
            .oib_property_register(&PropertyAddress::Definition {
                definition: id,
                node: root,
                property: fresh
            })
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Preserved endpoint literal".into()))]
    );
    assert!(client.designer_apply_working_copy().unwrap());
    settle(&mut client, &mut authority, &mut session);
    assert!(client.designer_history(true).unwrap());
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .working_copy
            .as_ref()
            .unwrap()
            .history_review()
            .is_none()
    );
    assert!(client.designer_apply_working_copy().unwrap());
    settle(&mut client, &mut authority, &mut session);
    let m = client.active.as_ref().unwrap();
    let removed = m.doc.oib_history_source().unwrap();
    let removed = removed.definitions.iter().find(|d| d.id == id).unwrap();
    assert!(removed.properties.is_empty());
    assert_eq!(removed.next_property, cursor + 1);
    assert_eq!(
        removed.nodes.iter().map(|node| node.id).collect::<Vec<_>>(),
        node_ids
    );
    assert_eq!(authority.oib_allocations().unwrap(), allocations);
    assert_eq!(
        [1, 2, 3, 4].map(|field| m.doc.typed_value(field).unwrap()),
        app_values
    );
}

#[test]
fn endpoint_removal_ui_blocks_incoming_forwarding_and_never_offers_class_removal() {
    let (mut client, authority, _session, id, _root, property) = declared();
    let before = authority.save().unwrap();
    let m = client.active.as_mut().unwrap();
    let source = m.design_doc().oib_history_source().unwrap();
    let definition = source.definitions.iter().find(|d| d.id == id).unwrap();
    let mut incoming = definition.properties[0].clone();
    incoming.id = PropertyId(definition.next_property);
    incoming.name = "heading".into();
    incoming.label = "Heading".into();
    m.stage_structure(EditBatch {
        generation: m.design_doc().oib_structural_generation().unwrap(),
        operations: vec![
            DocumentEdit::Definition(DefinitionEdit::SetProperty {
                definition: id,
                descriptor: incoming.clone(),
            }),
            DocumentEdit::Definition(DefinitionEdit::Expose {
                definition: id,
                property: incoming.id,
                targets: vec![PropertyPath {
                    references: vec![],
                    node: definition.root,
                    property,
                }],
            }),
        ],
    })
    .unwrap();
    open_review(&mut client);
    let modal = client.active.as_ref().unwrap().delete_modal().unwrap();
    assert!(
        modal
            .lines
            .iter()
            .any(|line| line.contains("Another endpoint forwards"))
    );
    let confirm = modal
        .actions
        .iter()
        .find(|action| action.label == "Remove property")
        .unwrap();
    assert!(!confirm.enabled);
    assert!(!client.activate(client.epoch(), confirm.id).unwrap());
    activate_modal(&mut client, "Keep property");
    activate_panel(&mut client, "> Accent · Color");
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(!d.panels.last().unwrap().nodes.iter().any(|node| {
        node.semantics
            .as_ref()
            .is_some_and(|s| s.label == "Remove property...")
    }));
    assert_eq!(authority.save().unwrap(), before);
}

#[test]
fn endpoint_removal_ui_discards_only_removed_endpoint_raw_buffers_after_confirmation() {
    let (mut client, authority, _session, id, _root, property) = declared();
    let before = authority.save().unwrap();
    let unrelated = editor_address(2);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let field = focus_address(&mut client, unrelated.clone());
    client
        .edit_text_input(
            client.epoch(),
            field,
            &Edit::Replace {
                text: "oops".into(),
                cursor: 4,
                anchor: 4,
                selection_only: false,
            },
        )
        .unwrap();
    client
        .inspect_oib_selection(Selection::Definition(id))
        .unwrap();
    let name = PropertyAddress::Structural {
        target: StructuralTarget::Property {
            definition: id,
            property,
        },
        property: StructuralProperty::Name,
    };
    let field = focus_address(&mut client, name.clone());
    client
        .edit_text_input(
            client.epoch(),
            field,
            &Edit::Replace {
                text: "".into(),
                cursor: 0,
                anchor: 0,
                selection_only: false,
            },
        )
        .unwrap();
    // Review deliberately suspends invalid text; Cancel must retain it.
    client
        .designer_review_property_delete(id, property)
        .unwrap();
    activate_modal(&mut client, "Keep property");
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(d.drafts.draft(&name).unwrap().dirty);
    assert!(d.drafts.draft(&unrelated).unwrap().dirty);
    client
        .designer_review_property_delete(id, property)
        .unwrap();
    activate_modal(&mut client, "Remove property");
    let d = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(d.drafts.draft(&name).is_none());
    assert!(d.drafts.draft(&unrelated).unwrap().dirty);
    assert!(
        !client.designer_apply_working_copy().unwrap(),
        "unrelated invalid field is preserved for explicit correction"
    );
    assert_eq!(authority.save().unwrap(), before);
}
