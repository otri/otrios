// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::canvas::client::oib_designer::tests::{
    activate_studio_tool, editor_address, mounted_fixture_session,
};
use otrios_interfaces::oib::designer::Selection;

fn pump(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
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
            assert!(client.active.as_ref().unwrap().doc.confirmed());
            return;
        }
    }
    panic!("delete fixture did not settle")
}
fn created() -> (Client, crate::canvas::Document, Session, DefinitionId, Ui) {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    let ui = client.active.as_ref().unwrap().ui.clone();
    assert!(
        client
            .activate(client.epoch(), oib_workspace::NEW_COMPONENT)
            .unwrap()
    );
    let Selection::Definition(definition) = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .selection
    else {
        panic!("created selection")
    };
    assert!(
        client
            .activate(client.epoch(), oib_workspace::APPLY)
            .unwrap()
    );
    pump(&mut client, &mut authority, &mut session);
    (client, authority, session, definition, ui)
}
fn point(scene: &Scene, id: u32) -> [f32; 2] {
    let path = scene.path(id).unwrap();
    let node = path.last().unwrap();
    [
        node.origin[0] + node.node.frame.width / 2.,
        node.origin[1] + node.node.frame.height / 2.,
    ]
}

#[test]
fn deletion_of_unapplied_component_cancels_its_values_and_keeps_other_edits() {
    let (mut client, authority, _session) = mounted_fixture_session(true);
    let before = authority.save().unwrap();
    let other = editor_address(3);
    client
        .active
        .as_mut()
        .unwrap()
        .stage_property(&other, Some(Value::Text("Other private edit".into())))
        .unwrap();
    client
        .activate(client.epoch(), oib_workspace::NEW_COMPONENT)
        .unwrap();
    let m = client.active.as_mut().unwrap();
    let selection = m.designer.as_ref().unwrap().selection.clone();
    let Selection::Definition(definition) = selection else {
        panic!("new component")
    };
    let property = crate::canvas::oib::document::properties(
        &m.design_doc().oib_authoring_seed().unwrap(),
        &m.design_doc().oib_allocations().unwrap(),
        &selection,
        &m.design_doc().oib_active().unwrap(),
    )
    .unwrap()
    .into_iter()
    .find(|p| p.descriptor.value_type == otrios_interfaces::layout::ValueType::Color)
    .unwrap();
    let address = property.address.unwrap();
    let color = Value::Color([24, 48, 96, 255]);
    m.stage_property(&address, Some(color.clone())).unwrap();
    client.designer_review_delete(definition).unwrap();
    tap(&mut client, DELETE);
    let m = client.active.as_mut().unwrap();
    assert!(
        m.delete_review.is_none(),
        "confirmed deletion must remove its local value edits"
    );
    let copy = m.working_copy.as_ref().unwrap();
    assert_eq!(copy.apply.changes.len(), 1);
    assert_eq!(copy.apply.changes[0].address, other);
    assert_eq!(
        copy.candidate
            .oib_property_register(&other)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Other private edit".into()))]
    );
    assert_eq!(authority.save().unwrap(), before);
    assert!(m.doc.generate().unwrap().is_none());
    assert!(
        client
            .activate(client.epoch(), oib_workspace::UNDO)
            .unwrap()
    );
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .design_doc()
            .oib_property_register(&address)
            .unwrap()
            .alternatives,
        vec![Some(color)]
    );
}
fn pointer(client: &mut Client, phase: PointerPhase, at: [f32; 2]) {
    assert!(
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id: 73,
                    phase,
                    position: at
                },
                1
            )
            .unwrap()
    );
}
fn tap(client: &mut Client, id: u32) {
    let at = point(&client.scene().unwrap(), id);
    pointer(client, PointerPhase::Down, at);
    pointer(client, PointerPhase::Up, at);
}
#[test]
fn deletion_unused_confirmation_stages_privately_and_apply_can_be_undone() {
    let (mut client, mut authority, mut session, definition, _) = created();
    let before = authority.save().unwrap();
    activate_studio_tool(&mut client, "Delete component...");
    let review = client
        .active
        .as_ref()
        .unwrap()
        .delete_review
        .as_ref()
        .unwrap();
    assert!(review.report.can_delete());
    assert!(review.can_stage);
    assert!(!review.report.removable_presets.is_empty());
    assert!(
        !client
            .activate(client.epoch(), oib_workspace::NEW_COMPONENT)
            .unwrap()
    );
    assert!(client.text_inputs().unwrap().is_empty());
    let at = point(&client.scene().unwrap(), DELETE);
    pointer(&mut client, PointerPhase::Down, at);
    pointer(&mut client, PointerPhase::Cancel, at);
    assert!(client.active.as_ref().unwrap().delete_review.is_some());
    assert!(!client.input_busy());
    tap(&mut client, DELETE);
    let m = client.active.as_mut().unwrap();
    assert!(m.delete_review.is_none());
    assert!(m.working_copy.as_ref().unwrap().dirty());
    assert!(m.doc.generate().unwrap().is_none());
    assert_eq!(authority.save().unwrap(), before);
    assert!(
        !m.design_doc()
            .oib_history_source()
            .unwrap()
            .definitions
            .iter()
            .any(|d| d.id == definition)
    );
    assert!(
        client
            .activate(client.epoch(), oib_workspace::APPLY)
            .unwrap()
    );
    pump(&mut client, &mut authority, &mut session);
    let next = client
        .active
        .as_ref()
        .unwrap()
        .doc
        .oib_history_source()
        .unwrap()
        .next_identity;
    assert!(
        client
            .activate(client.epoch(), oib_workspace::UNDO)
            .unwrap()
    );
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
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .design_doc()
            .oib_history_source()
            .unwrap()
            .definitions
            .iter()
            .any(|d| d.id.0 == next)
    );
    assert!(
        client
            .activate(client.epoch(), oib_workspace::APPLY)
            .unwrap()
    );
    pump(&mut client, &mut authority, &mut session);
    assert!(client.scene().unwrap().controls().len() <= 128);
}
#[test]
fn deletion_used_component_is_visible_blocked_and_links_to_named_dependencies() {
    let (mut client, authority, _session) = mounted_fixture_session(true);
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(10)))
        .unwrap();
    activate_studio_tool(&mut client, "Delete component...");
    let before = authority.save().unwrap();
    let m = client.active.as_ref().unwrap();
    let review = m.delete_review.as_ref().unwrap();
    assert!(!review.can_stage);
    assert!(!review.report.can_delete());
    assert!(
        review
            .report
            .items
            .iter()
            .any(|item| item.label.contains("Audio"))
    );
    assert!(
        review
            .report
            .items
            .iter()
            .any(|item| item.label.contains("Display"))
    );
    assert!(!client.activate(client.epoch(), DELETE).unwrap());
    assert!(client.active.as_ref().unwrap().delete_review.is_some());
    let m = client.active.as_ref().unwrap();
    let review = m.delete_review.as_ref().unwrap();
    let modal = review.modal(m);
    let link = modal
        .links
        .iter()
        .find(|link| matches!(review.link_selection(link.id), Some(Selection::Object(_))))
        .unwrap()
        .id;
    assert!(client.scene().unwrap().path(link).is_some());
    tap(&mut client, link);
    let m = client.active.as_ref().unwrap();
    assert!(m.delete_review.is_none());
    assert!(matches!(
        m.designer.as_ref().unwrap().selection,
        Selection::Object(_)
    ));
    assert_eq!(authority.save().unwrap(), before);
}
#[test]
fn deletion_stale_review_keeps_capture_and_requires_explicit_rescan() {
    for dirty in [false, true] {
        let (mut client, mut authority, mut session, definition, ui) = created();
        if dirty {
            client
                .active
                .as_mut()
                .unwrap()
                .stage_property(
                    &editor_address(3),
                    Some(Value::Text("Retained private edit".into())),
                )
                .unwrap();
        }
        client.designer_review_delete(definition).unwrap();
        let at = point(&client.scene().unwrap(), DELETE);
        pointer(&mut client, PointerPhase::Down, at);
        let mut rival = StateAdapter::for_ui(&ui, "designer-mount", &[], [121; 16]).unwrap();
        let (mut rival_session, _) = authority.open_session(&[121; 16], 0).unwrap();
        rival_session.set_oib_editable(true);
        rival_session
            .set_selection(StateSelection::from_ui(&ui).unwrap())
            .unwrap();
        rival
            .receive(&authority.generate(&mut rival_session).unwrap().unwrap())
            .unwrap();
        rival
            .edit_oib_structure(EditBatch {
                generation: rival.oib_structural_generation().unwrap(),
                operations: vec![DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                    definition,
                    name: "Latest component".into(),
                })],
            })
            .unwrap();
        for _ in 0..30 {
            let mut progress = false;
            while let Some(bytes) = rival.generate().unwrap() {
                authority.receive(&mut rival_session, &bytes).unwrap();
                progress = true;
            }
            if let Some(bytes) = authority.generate(&mut rival_session).unwrap() {
                rival.receive(&bytes).unwrap();
                progress = true;
            }
            if !progress {
                break;
            }
        }
        pump(&mut client, &mut authority, &mut session);
        assert!(client.input_busy());
        assert!(
            !client
                .active
                .as_ref()
                .unwrap()
                .delete_review
                .as_ref()
                .unwrap()
                .current(client.active.as_ref().unwrap())
        );
        pointer(&mut client, PointerPhase::Up, at);
        assert!(!client.input_busy());
        assert!(client.active.as_ref().unwrap().delete_review.is_some());
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .dirty(),
            dirty
        );
        assert!(client.activate(client.epoch(), REFRESH).unwrap());
        let m = client.active.as_ref().unwrap();
        let review = m.delete_review.as_ref().unwrap();
        assert!(review.current(m));
        if dirty {
            assert!(!review.can_stage);
            assert!(review.error.as_ref().unwrap().contains("older source"));
            assert_eq!(
                m.design_doc()
                    .oib_property_register(&editor_address(3))
                    .unwrap()
                    .alternatives,
                vec![Some(Value::Text("Retained private edit".into()))]
            );
        } else {
            assert_eq!(review.report.name, "Latest component");
            assert!(review.can_stage);
        }
        assert!(client.back().unwrap());
        assert!(client.active.as_ref().unwrap().delete_review.is_none());
    }
}

#[test]
fn deletion_dependency_scroll_retains_layout_and_pages_only_at_existing_limits() {
    let (mut client, _authority, _session) = mounted_fixture_session(true);
    client.designer_review_delete(DefinitionId(10)).unwrap();
    for viewport in [[800., 480.], [1280., 800.], [1920., 1080.]] {
        client.set_viewport(viewport).unwrap();
        let before = client.scene().unwrap();
        let cached = client
            .active
            .as_ref()
            .unwrap()
            .layout_scene
            .get()
            .unwrap()
            .clone();
        let close = point(&before, CLOSE);
        let content = point(&before, MODAL_CONTENT);
        assert!(
            client
                .delete_review_scroll(content, 8192.)
                .unwrap()
                .unwrap()
        );
        let after = client.scene().unwrap();
        assert_eq!(close, point(&after, CLOSE));
        assert!(Arc::ptr_eq(
            &cached,
            client.active.as_ref().unwrap().layout_scene.get().unwrap()
        ));
        fn count(node: &otrios_interfaces::scene::SceneNode) -> usize {
            1 + node.children.iter().map(count).sum::<usize>()
        }
        assert!(count(after.root()) <= 512);
        assert!(after.controls().len() <= 128);
    }
    let m = client.active.as_mut().unwrap();
    let review = m.delete_review.as_mut().unwrap();
    assert_eq!(review.pages.len(), 1, "the real document uses scrolling");
    // Duplicate display rows only to qualify the coordinate/link envelope;
    // this is not a claim of 300 admitted document instances.
    let item = review
        .report
        .items
        .iter()
        .find(|i| i.selection.is_some())
        .unwrap()
        .clone();
    review.report.items = (0..300)
        .map(|n| {
            let mut item = item.clone();
            item.label = format!("Use {n}");
            item.detail = "Bounded dependency description with owner and reference path".into();
            item
        })
        .collect();
    review.pages.clear();
    review.paginate().unwrap();
    assert!(review.pages.len() > 1);
    let indices: Vec<_> = review.pages.iter().cloned().flatten().collect();
    assert_eq!(indices, (0..300).collect::<Vec<_>>());
    for range in &review.pages {
        let modal = review.modal_for(range.clone(), true, Some((0, review.pages.len())));
        assert!(oib_workspace::modal_content_height(&modal, 800.).unwrap() <= 8192.);
        assert!(modal.links.len() <= 256);
        assert!(
            modal
                .links
                .iter()
                .all(|link| (MODAL_LINK_START..=oib_workspace::MODAL_LINK_END).contains(&link.id))
        );
    }
    review.report.items.truncate(12);
    review.pages.clear();
    review.paginate().unwrap();
    assert_eq!(
        review.pages.len(),
        1,
        "fitting reports must not be paginated"
    );
    review.report.items[0].selection = None;
    let modal = review.modal_for(0..12, true, None);
    assert_eq!(modal.links[0].id, MODAL_LINK_START);
    assert_eq!(
        review.link_selection(modal.links[0].id),
        review.report.items[1].selection
    );
}
