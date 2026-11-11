// SPDX-License-Identifier: LGPL-3.0-only
//! UI acceptance over one real authority conflict. The geometry case duplicates
//! its display projection only; it does not claim sixteen authority conflicts.
use super::*;
use crate::canvas::client::oib_designer::tests::{
    editor_address, focus_address, mounted_fixture_session,
};
use otrios_interfaces::{
    keyboard::Edit,
    oib::{
        authoring::{DefinitionEdit, DocumentEdit, EditBatch},
        designer::{
            DefinitionId, InstanceId, NodeId, ObjectAddress, PropertyAddress, Selection,
            StructuralProperty, StructuralTarget,
        },
    },
};

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
            return;
        }
    }
    panic!("review fixture failed to settle");
}
fn reviewed() -> (Client, u32) {
    let (client, field, _, _) = reviewed_with_authority();
    (client, field)
}
fn reviewed_with_authority() -> (Client, u32, crate::canvas::Document, Session) {
    let (mut client, mut authority, mut session) = mounted_fixture_session(true);
    let ui = client.active.as_ref().unwrap().ui.clone();
    let address = PropertyAddress::Structural {
        target: StructuralTarget::Definition(DefinitionId(13)),
        property: StructuralProperty::Name,
    };
    client
        .active
        .as_mut()
        .unwrap()
        .stage_property(&address, Some(Value::Text("Own Power".into())))
        .unwrap();
    assert!(
        client
            .activate(client.epoch(), oib_workspace::APPLY)
            .unwrap()
    );
    pump(&mut client, &mut authority, &mut session);
    let mut rival = StateAdapter::for_ui(&ui, "designer-mount", &[], [117; 16]).unwrap();
    let (mut rival_session, _) = authority.open_session(&[117; 16], 0).unwrap();
    rival_session.set_oib_editable(true);
    rival_session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    rival
        .receive(&authority.generate(&mut rival_session).unwrap().unwrap())
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
    assert!(
        rival
            .edit_oib_structure(EditBatch {
                generation: rival.oib_structural_generation().unwrap(),
                operations: vec![DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                    definition: DefinitionId(13),
                    name: "Other Power".into()
                })]
            })
            .unwrap()
    );
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
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let field = focus_address(&mut client, editor_address(3));
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
            .is_some()
    );
    (client, field, authority, session)
}
fn point(scene: &Scene, id: u32) -> [f32; 2] {
    let path = scene.path(id).unwrap();
    let node = path.last().unwrap();
    [
        node.origin[0] + node.node.frame.width * 0.5,
        node.origin[1] + node.node.frame.height * 0.5,
    ]
}
fn pointer(client: &mut Client, id: u32, phase: PointerPhase, position: [f32; 2]) {
    assert!(
        client
            .pointer_at(
                client.epoch(),
                PointerEvent {
                    id,
                    phase,
                    position
                },
                1
            )
            .unwrap()
    );
}
fn tap(client: &mut Client, id: u32) {
    let at = point(&client.scene().unwrap(), id);
    pointer(client, 91, PointerPhase::Down, at);
    pointer(client, 91, PointerPhase::Up, at);
}
fn texts(node: &SceneNode) -> Vec<String> {
    let mut values = vec![];
    if let otrios_interfaces::scene::Primitive::Text { value, .. } = &node.primitive {
        values.push(value.clone());
    }
    for child in &node.children {
        values.extend(texts(child));
    }
    values
}
fn count(node: &SceneNode) -> usize {
    1 + node.children.iter().map(count).sum::<usize>()
}
#[test]
fn oib_history_review_compares_real_sources_and_blocks_underlay_until_close_or_inspect() {
    let (mut client, field) = reviewed();
    let root = client.active.as_ref().unwrap().doc.oib_root();
    let values =
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().doc.typed_value(id).unwrap());
    let scene = client.scene().unwrap();
    let text = texts(scene.root());
    assert!(
        text.iter().any(|s| s == "Before: LabeledToggle"),
        "{text:?}"
    );
    assert!(text.iter().any(|s| s == "Applied: Own Power"));
    assert!(text.iter().any(|s| s == "Current: Other Power"));
    assert!(
        text.iter()
            .all(|s| !s.contains("DefinitionId(") && !s.contains("StructuralTarget"))
    );
    assert!(
        !client
            .activate(client.epoch(), oib_workspace::NEW_COMPONENT)
            .unwrap()
    );
    let underlay = point(&scene, oib_workspace::MODE_TEST);
    pointer(&mut client, 92, PointerPhase::Down, underlay);
    pointer(&mut client, 92, PointerPhase::Up, underlay);
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .workspace_mode(),
        otrios_interfaces::oib::designer::DesignerMode::Edit
    );
    let control = client.active.as_ref().unwrap().ui.handlers[0].control;
    assert!(
        !client
            .dispatch_event(client.epoch(), control, 1000, 0)
            .unwrap()
    );
    assert!(!client.hardware_edit(&Edit::Enter).unwrap());
    assert!(
        !client
            .edit_text_input(client.epoch(), field, &Edit::Enter)
            .unwrap()
    );
    assert!(client.text_inputs().unwrap().is_empty());
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert_eq!(
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().doc.typed_value(id).unwrap()),
        values
    );
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
    let close = point(&scene, CLOSE);
    pointer(&mut client, 93, PointerPhase::Down, close);
    pointer(&mut client, 93, PointerPhase::Cancel, close);
    pointer(&mut client, 93, PointerPhase::Up, close);
    assert!(client.active.as_ref().unwrap().local_modal_open());
    pointer(&mut client, 93, PointerPhase::Down, close);
    pointer(&mut client, 93, PointerPhase::Up, [1., 1.]);
    assert!(client.active.as_ref().unwrap().local_modal_open());
    tap(&mut client, CLOSE);
    assert!(!client.active.as_ref().unwrap().local_modal_open());
    assert!(
        client
            .activate(client.epoch(), oib_workspace::UNDO)
            .unwrap()
    );
    let selection = client
        .active
        .as_ref()
        .unwrap()
        .working_copy
        .as_ref()
        .unwrap()
        .history_review()
        .unwrap()
        .conflicts[0]
        .selection
        .clone()
        .unwrap();
    tap(&mut client, INSPECT);
    assert!(!client.active.as_ref().unwrap().local_modal_open());
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .selection,
        selection
    );
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), root);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}
#[test]
fn oib_history_review_sixteen_display_rows_scroll_with_fixed_footer_and_cached_layout() {
    let (mut client, _field) = reviewed();
    // One real conflict supplies proof and Inspect target. These clones exercise
    // only the sixteen-row UI boundary, never an authority conflict count.
    let conflict = client
        .active
        .as_ref()
        .unwrap()
        .working_copy
        .as_ref()
        .unwrap()
        .history_review()
        .unwrap()
        .conflicts[0]
        .clone();
    for viewport in [[800., 600.], [1280., 800.], [1920., 1080.]] {
        client.set_viewport(viewport).unwrap();
        let review = client
            .active
            .as_mut()
            .unwrap()
            .working_copy
            .as_mut()
            .unwrap()
            .history_review_mut()
            .unwrap();
        review.scroll = 0;
        review.conflicts = (1..=16)
            .map(|i| {
                let mut c = conflict.clone();
                c.target = format!("Control {i:02} name");
                c.before_text = format!("Before value {i:02}");
                c.applied_text = format!("Applied value {i:02}");
                c.current_text = format!("Current value {i:02}");
                c
            })
            .collect();
        client.active.as_mut().unwrap().invalidate();
        let initial = client.scene().unwrap();
        let content = point(&initial, MODAL_CONTENT);
        let footer = point(&initial, CLOSE);
        let layout = client
            .active
            .as_ref()
            .unwrap()
            .layout_scene
            .get()
            .unwrap()
            .clone();
        assert!(texts(initial.root()).iter().any(|v| v == "Control 01 name"));
        assert!(!texts(initial.root()).iter().any(|v| v == "Control 16 name"));
        assert!(
            client
                .scroll_at(client.epoch(), 1_000_000., content)
                .unwrap()
        );
        let bottom = client.scene().unwrap();
        let limit = client
            .active
            .as_ref()
            .unwrap()
            .working_copy
            .as_ref()
            .unwrap()
            .history_review()
            .unwrap()
            .scroll;
        assert!(limit > 0);
        assert!(
            texts(bottom.root())
                .iter()
                .any(|v| v == "Current: Current value 16")
        );
        assert!(!texts(bottom.root()).iter().any(|v| v == "Control 01 name"));
        assert_eq!(point(&bottom, CLOSE), footer);
        assert!(Arc::ptr_eq(
            &layout,
            client.active.as_ref().unwrap().layout_scene.get().unwrap()
        ));
        assert!(client.scroll_at(client.epoch(), -48., content).unwrap());
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .history_review()
                .unwrap()
                .scroll,
            limit - 48
        );
        let after = client.scene().unwrap();
        assert_eq!(point(&after, CLOSE), footer);
        let path = after.path(MODAL_CONTENT).unwrap();
        let node = path.last().unwrap().node;
        assert!(matches!(
            node.primitive,
            otrios_interfaces::scene::Primitive::Canvas { clip: true }
        ));
        assert!(node.children.len() <= ((node.frame.height / 24.).ceil() as usize) + 3);
        assert!(count(after.root()) <= 512);
        assert!(after.controls().len() <= 128);
        pointer(&mut client, 94, PointerPhase::Down, content);
        pointer(
            &mut client,
            94,
            PointerPhase::Move,
            [content[0], content[1] + 120.],
        );
        pointer(
            &mut client,
            94,
            PointerPhase::Cancel,
            [content[0], content[1] + 120.],
        );
        assert!(client.active.as_ref().unwrap().local_modal_open());
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .history_review()
                .unwrap()
                .scroll
                < limit - 48
        );
        assert_eq!(point(&client.scene().unwrap(), CLOSE), footer);
        assert!(Arc::ptr_eq(
            &layout,
            client.active.as_ref().unwrap().layout_scene.get().unwrap()
        ));
    }
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}

#[test]
fn oib_history_review_large_display_values_remain_bounded_and_explicitly_summarized() {
    let (mut client, _) = reviewed();
    client.set_viewport([800., 600.]).unwrap();
    let m = client.active.as_mut().unwrap();
    let review = m
        .working_copy
        .as_mut()
        .unwrap()
        .history_review_mut()
        .unwrap();
    let prototype = review.conflicts[0].clone();
    review.conflicts = (0..16)
        .map(|index| {
            let mut conflict = prototype.clone();
            conflict.target = format!("Long field {index}");
            conflict.before_text = "b".repeat(1024);
            conflict.applied_text = "a".repeat(1024);
            conflict.current_text = "c".repeat(1024);
            conflict
        })
        .collect();
    m.invalidate_presentation();
    let modal = m.history_review_modal().unwrap();
    assert!(oib_workspace::modal_scroll_limit(&modal, 800., 600.).unwrap() <= 8192);
    let scene = client.scene().unwrap();
    assert!(
        texts(scene.root())
            .iter()
            .any(|line| line.contains("1024 characters"))
    );
    assert!(count(scene.root()) <= 512);
    assert!(scene.controls().len() <= 128);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}

#[test]
fn oib_history_review_keeps_pointer_owner_when_another_edit_arrives() {
    let (mut client, _, mut authority, mut session) = reviewed_with_authority();
    let scene = client.scene().unwrap();
    let close = point(&scene, CLOSE);
    pointer(&mut client, 77, PointerPhase::Down, close);
    assert!(client.input_busy());
    let (mut peer, _) = authority.open_session(&[118; 16], 0).unwrap();
    peer.set_oib_editable(true);
    authority
        .receive(
            &mut peer,
            &encode_state_request(&StateRequest::OibInstance {
                epoch: authority.oib_epoch().unwrap(),
                sequence: 1,
                operation: EditBatch {
                    generation: authority.oib_status().unwrap().structural_generation,
                    operations: vec![DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                        definition: DefinitionId(13),
                        name: "Newest Power".into(),
                    })],
                },
            })
            .unwrap(),
        )
        .unwrap();
    pump(&mut client, &mut authority, &mut session);
    let scene = client.scene().unwrap();
    assert!(
        texts(scene.root())
            .iter()
            .any(|s| s.contains("Sources changed again"))
    );
    assert_eq!(
        point(&scene, CLOSE),
        close,
        "remote updates keep the modal action geometry fixed during the contact"
    );
    assert!(client.input_busy());
    pointer(&mut client, 77, PointerPhase::Up, close);
    assert!(!client.input_busy());
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
            .unwrap()
            .conflicts
            .iter()
            .any(|c| c.current_text.contains("Newest Power"))
    );
    assert!(
        !client
            .active
            .as_ref()
            .unwrap()
            .working_copy
            .as_ref()
            .unwrap()
            .dirty()
    );
}
