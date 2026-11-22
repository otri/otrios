// SPDX-License-Identifier: LGPL-3.0-only
//! Workspace navigation must finish ordinary field focus without hiding errors.
use super::tests::{editor_address, focus_address, mounted_fixture_session};
use super::*;
use otrios_interfaces::keyboard::Edit;
use otrios_interfaces::oib::designer::{NodeId, ObjectAddress};

fn focused_width() -> (Client, crate::canvas::Document, Session, u32) {
    let (mut client, authority, session) = mounted_fixture_session(true);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(7)],
            node: NodeId(1),
        }))
        .unwrap();
    let id = focus_address(&mut client, editor_address(2));
    (client, authority, session, id)
}
fn replace(client: &mut Client, id: u32, text: &str) {
    client
        .edit_text_input(
            client.epoch(),
            id,
            &Edit::Replace {
                text: text.into(),
                cursor: text.len() as u32,
                anchor: text.len() as u32,
                selection_only: false,
            },
        )
        .unwrap();
}
fn settle(client: &mut Client, authority: &mut crate::canvas::Document, session: &mut Session) {
    let mount = client.active.as_mut().unwrap();
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = mount.doc.generate().unwrap() {
            authority.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = authority.generate(session).unwrap() {
            mount.doc.receive(&bytes).unwrap();
            mount.refresh().unwrap();
            progress = true;
        }
        if !progress {
            assert!(mount.doc.confirmed());
            return;
        }
    }
    panic!("workspace field receipt did not settle");
}
#[test]
fn workspace_task_blurs_clean_field_and_hides_keyboard() {
    let (mut client, _authority, _session, _id) = focused_width();
    assert!(
        client
            .designer_action(LocalAction::Workspace(Task::Library))
            .unwrap()
    );
    let mount = client.active.as_ref().unwrap();
    let designer = mount.designer.as_ref().unwrap();
    assert_eq!(designer.workspace_task, Task::Library);
    assert!(!designer.editor_focused());
    assert!(!mount.keyboard.visible);
    assert!(!mount.oib_activation_blocked());
}
#[test]
fn workspace_task_keeps_invalid_field_visible() {
    let (mut client, _authority, _session, id) = focused_width();
    replace(&mut client, id, "invalid");
    assert!(
        !client
            .designer_action(LocalAction::Workspace(Task::Layers))
            .unwrap()
    );
    let mount = client.active.as_ref().unwrap();
    let designer = mount.designer.as_ref().unwrap();
    assert_eq!(designer.workspace_task, Task::Inspect);
    assert!(designer.editor_focused());
    assert!(mount.keyboard.visible);
    assert!(
        designer
            .drafts
            .draft(&editor_address(2))
            .unwrap()
            .error
            .is_some()
    );
    assert!(mount.doc.confirmed());
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let mount = client.active.as_ref().unwrap();
    let designer = mount.designer.as_ref().unwrap();
    assert_eq!(designer.mode, DesignerMode::Edit);
    assert_eq!(designer.workspace_task, Task::Inspect);
    assert!(designer.editor_focused());
    assert!(mount.keyboard.visible);
}
#[test]
fn workspace_mode_blurs_field_into_private_candidate_then_applies_explicitly() {
    let (mut client, mut authority, mut session, id) = focused_width();
    let live_root = client.active.as_ref().unwrap().doc.oib_root();
    replace(&mut client, id, "410");
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let mount = client.active.as_ref().unwrap();
    let designer = mount.designer.as_ref().unwrap();
    assert!(!designer.editor_focused());
    assert_eq!(designer.mode, DesignerMode::Test);
    assert_eq!(designer.pending_mode, None);
    assert!(mount.doc.confirmed());
    assert_eq!(mount.doc.oib_root(), live_root);
    assert!(mount.working_copy.as_ref().unwrap().dirty());
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
    client.set_oib_designer_mode(DesignerMode::Edit).unwrap();
    assert!(
        client
            .designer_action(LocalAction::ApplyWorkingCopy)
            .unwrap()
    );
    assert!(!client.active.as_ref().unwrap().doc.confirmed());
    settle(&mut client, &mut authority, &mut session);
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let mount = client.active.as_ref().unwrap();
    assert_eq!(mount.designer.as_ref().unwrap().mode, DesignerMode::Test);
    assert!(mount.designer_preview.is_some());
    assert!(mount.doc.confirmed());
    client.set_oib_designer_mode(DesignerMode::Edit).unwrap();
    let id = focus_address(&mut client, editor_address(2));
    assert!(
        client
            .text_inputs()
            .unwrap()
            .iter()
            .any(|f| f.id == id && f.focused)
    );
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let mount = client.active.as_ref().unwrap();
    assert_eq!(mount.designer.as_ref().unwrap().mode, DesignerMode::Test);
    assert!(!mount.designer.as_ref().unwrap().editor_focused());
}
#[test]
fn workspace_mode_preserves_focus_while_pointer_is_captured() {
    let (mut client, _authority, _session, _id) = focused_width();
    client
        .active
        .as_mut()
        .unwrap()
        .designer
        .as_mut()
        .unwrap()
        .capture = Some(Capture {
        pointer: 71,
        start: [0., 0.],
        moving: false,
        action: None,
        panel: Some(1),
    });
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let mount = client.active.as_ref().unwrap();
    let designer = mount.designer.as_ref().unwrap();
    assert!(designer.editor_focused());
    assert_eq!(designer.capture.as_ref().unwrap().pointer, 71);
    assert_eq!(designer.mode, DesignerMode::Edit);
    assert_eq!(designer.pending_mode, Some(DesignerMode::Test));
    assert!(mount.doc.confirmed());
}

#[test]
fn shell_appearance_and_account_preserve_invalid_draft_and_preview_mode() {
    let (mut client, _authority, _session, id) = focused_width();
    // This low-level fixture bypasses Client::receive, so supply the same local
    // shell metadata that an admitted app receives in production.
    let mount = client.active.as_mut().unwrap();
    let app_id = mount
        .ui
        .ui_ref
        .strip_prefix("app:")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let catalog = otrios_interfaces::canvas::Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![otrios_interfaces::canvas::CatalogEntry {
            app: otrios_interfaces::canvas::App {
                id: app_id.clone(),
                title: "OIB Designer".into(),
                ui_ref: mount.ui.ui_ref.clone(),
                state_ref: mount.state_ref.clone(),
                status: "canvas-app".into(),
                dependencies: vec![],
            },
            path: "oib-designer".into(),
            private: false,
            ui_hash: vec![0; 32],
        }],
    };
    mount.breadcrumbs =
        Some(breadcrumbs::Trail::new(&catalog, &app_id, None, None, &mount.ui, false).unwrap());
    mount.invalidate();
    client
        .admit_catalog(&encode_catalog(&catalog).unwrap())
        .unwrap();
    replace(&mut client, id, "invalid");
    let value = client.typed_value(1).ok();
    let before = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    let selection = before.selection.clone();
    let preview = before.appearance();
    client.set_appearance("light", true).unwrap();
    assert!(!client.appearance_is_dark());
    assert_eq!(client.typed_value(1).ok(), value);
    let editor = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert_eq!(editor.selection, selection);
    assert_eq!(editor.appearance(), preview);
    assert!(
        editor
            .drafts
            .draft(&editor_address(2))
            .unwrap()
            .error
            .is_some()
    );
    client.shell_command(breadcrumbs::Command::Account).unwrap();
    assert!(client.text_inputs().unwrap().is_empty());
    assert!(
        !client
            .edit_text_input(client.epoch(), id, &Edit::Commit("stale".into()))
            .unwrap()
    );
    client.back().unwrap();
    let field = client
        .text_inputs()
        .unwrap()
        .into_iter()
        .find(|field| field.id == id)
        .unwrap();
    assert_eq!(field.value, "invalid");
    assert!(field.focused);
    let editor = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert_eq!(editor.selection, selection);
    assert_eq!(editor.appearance(), preview);
    assert!(
        editor
            .drafts
            .draft(&editor_address(2))
            .unwrap()
            .error
            .is_some()
    );
    client.configure_spaces(Some(&"a".repeat(32))).unwrap();
    client.spaces_outgoing().unwrap();
    let snapshot = otrios_interfaces::spaces::Discovery {
        version: 1,
        cursor: otrios_interfaces::spaces::DiscoveryCursor {
            authority: "c".repeat(32),
            revision: 1,
        },
        generation: 1,
        catalog: vec![],
        identity: Some("alice".into()),
        profile: None,
        spaces: vec![],
        selected: None,
    };
    client
        .receive_spaces(
            &crate::spaces::encode_event(&otrios_interfaces::spaces::Event::Snapshot {
                observation: 1,
                snapshot: Box::new(snapshot),
                resync: true,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .breadcrumbs
            .as_ref()
            .unwrap()
            .identity
            .as_deref(),
        Some("alice")
    );
    assert_eq!(client.typed_value(1).ok(), value);
    client.shell_command(breadcrumbs::Command::Account).unwrap();
    client.shell_command(breadcrumbs::Command::Profile).unwrap();
    assert!(client.active.as_ref().unwrap().leave.is_some());
}
