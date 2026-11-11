// SPDX-License-Identifier: LGPL-3.0-only
//! Definition scope navigation is an actual local control, not a source edit.
use super::tests::{activate_studio_tool, focus_address, mounted_fixture_session_with_ui};
use super::*;
use otrios_interfaces::{
    keyboard::Edit,
    layout::Expr,
    oib::designer::{DefinitionId, StructuralProperty, StructuralTarget},
};

fn fixture() -> (Client, crate::canvas::Document, Session) {
    let ui = crate::canvas::decode_ui(
        &crate::canvas::encode_ui(&crate::test_support::oib_designer::ui(&[])).unwrap(),
    )
    .unwrap();
    mounted_fixture_session_with_ui(true, 0, ui)
}
fn activate(client: &mut Client, label: &str) {
    let id = client
        .scene()
        .unwrap()
        .controls()
        .iter()
        .find(|path| {
            path.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == label)
        })
        .unwrap_or_else(|| panic!("Missing visible control {label}"))
        .node
        .id;
    assert!(client.activate(client.epoch(), id).unwrap());
}
fn nodes(node: &SceneNode) -> usize {
    1 + node.children.iter().map(nodes).sum::<usize>()
}
fn root(client: &Client) -> (DefinitionId, otrios_interfaces::oib::designer::NodeId) {
    let m = client.active.as_ref().unwrap();
    let definition = m.selected_definition().unwrap();
    let source = m.design_doc().oib_history_source().unwrap();
    (
        definition,
        source
            .definitions
            .iter()
            .find(|d| d.id == definition)
            .unwrap()
            .root,
    )
}
fn replace(client: &mut Client, id: u32, value: &str) {
    client
        .edit_text_input(
            client.epoch(),
            id,
            &Edit::Replace {
                text: value.into(),
                cursor: value.len() as u32,
                anchor: value.len() as u32,
                selection_only: false,
            },
        )
        .unwrap();
}

#[test]
fn scope_navigation_returns_from_child_to_document_and_inserts_private_component() {
    let (mut client, authority, _session) = fixture();
    let authority_before = authority.save().unwrap();
    activate_studio_tool(&mut client, "New component");
    let (definition, _) = root(&client);
    activate_studio_tool(&mut client, "Add text");
    activate(&mut client, "Layers");
    activate(&mut client, "Add text");
    assert!(
        matches!(client.active.as_ref().unwrap().designer.as_ref().unwrap().selection,
        Selection::DefinitionNode {definition: id,..} if id == definition)
    );
    for viewport in [[800., 480.], [1280., 800.], [1920., 1080.]] {
        client.set_viewport(viewport).unwrap();
        let scene = client.scene().unwrap();
        let path = scene.path(oib_workspace::BACK_DOCUMENT).unwrap();
        let button = path.last().unwrap();
        assert_eq!(button.node.frame.height, 48.);
        let point = [
            button.origin[0] + button.node.frame.width * button.scale * 0.5,
            button.origin[1] + button.node.frame.height * button.scale * 0.5,
        ];
        assert!(
            scene
                .hit_path(point)
                .iter()
                .any(|p| p.node.id == oib_workspace::BACK_DOCUMENT)
        );
        assert!(nodes(scene.root()) <= 512 && scene.controls().len() <= 128);
        assert_eq!(
            client
                .active
                .as_ref()
                .unwrap()
                .editor_chrome()
                .component_name
                .as_deref(),
            Some("New component")
        );
    }
    let task = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .workspace_task;
    let source_before = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_history_source()
        .unwrap();
    activate(&mut client, "Back to document");
    let m = client.active.as_ref().unwrap();
    assert!(matches!(
        m.designer.as_ref().unwrap().selection,
        Selection::Document(_)
    ));
    assert_eq!(m.designer.as_ref().unwrap().workspace_task, task);
    assert_eq!(m.designer.as_ref().unwrap().mode, DesignerMode::Edit);
    assert_eq!(m.design_doc().oib_history_source().unwrap(), source_before);
    assert!(m.editor_chrome().component_name.is_none());
    assert!(
        client
            .scene()
            .unwrap()
            .path(oib_workspace::BACK_DOCUMENT)
            .is_none()
    );
    assert!(
        !client
            .activate(client.epoch(), oib_workspace::BACK_DOCUMENT)
            .unwrap(),
        "retired scope control is inert"
    );
    activate_studio_tool(&mut client, "Insert component...");
    activate_studio_tool(&mut client, "Insert New component");
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .design_doc()
            .oib_instances()
            .unwrap()
            .iter()
            .any(|instance| instance.active && instance.component == definition.0)
    );
    assert_eq!(
        authority.save().unwrap(),
        authority_before,
        "scope and insertion remain private until Apply"
    );
}

#[test]
fn scope_navigation_keeps_invalid_field_and_commits_valid_field_to_private_source() {
    let (mut client, authority, _session) = fixture();
    let before = authority.save().unwrap();
    activate_studio_tool(&mut client, "New component");
    let (definition, node) = root(&client);
    let address = PropertyAddress::Structural {
        target: StructuralTarget::Node { definition, node },
        property: StructuralProperty::Width,
    };
    let field = focus_address(&mut client, address.clone());
    replace(&mut client, field, "invalid");
    assert!(
        !client
            .activate(client.epoch(), oib_workspace::BACK_DOCUMENT)
            .unwrap()
    );
    let m = client.active.as_ref().unwrap();
    assert_eq!(m.selected_definition(), Some(definition));
    assert!(m.designer.as_ref().unwrap().editor_focused());
    assert!(m.keyboard.visible);
    assert!(
        m.designer
            .as_ref()
            .unwrap()
            .drafts
            .draft(&address)
            .unwrap()
            .error
            .is_some()
    );
    replace(&mut client, field, "360");
    activate(&mut client, "Back to document");
    let m = client.active.as_ref().unwrap();
    assert!(matches!(
        m.designer.as_ref().unwrap().selection,
        Selection::Document(_)
    ));
    assert!(!m.keyboard.visible);
    assert!(!m.designer.as_ref().unwrap().editor_focused());
    let source = m.design_doc().oib_history_source().unwrap();
    let visual = source
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == node)
        .unwrap()
        .visual
        .as_ref()
        .unwrap();
    assert_eq!(visual.frame[2], Expr::Literal(Value::Number(360.)));
    assert_eq!(authority.save().unwrap(), before);
}

#[test]
fn scope_navigation_blocks_capture_and_preserves_test_mode() {
    let (mut client, authority, _session) = fixture();
    let before = authority.save().unwrap();
    activate_studio_tool(&mut client, "New component");
    let definition = root(&client).0;
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let scene = client.scene().unwrap();
    let path = scene.path(oib_workspace::BACK_DOCUMENT).unwrap();
    let p = path.last().unwrap();
    let point = [
        p.origin[0] + p.node.frame.width * p.scale * 0.5,
        p.origin[1] + 24. * p.scale,
    ];
    client
        .pointer_at(
            client.epoch(),
            PointerEvent {
                id: 91,
                phase: PointerPhase::Down,
                position: point,
            },
            1,
        )
        .unwrap();
    assert!(
        !client
            .activate(client.epoch(), oib_workspace::BACK_DOCUMENT)
            .unwrap()
    );
    assert_eq!(
        client.active.as_ref().unwrap().selected_definition(),
        Some(definition)
    );
    client
        .pointer_at(
            client.epoch(),
            PointerEvent {
                id: 91,
                phase: PointerPhase::Cancel,
                position: point,
            },
            2,
        )
        .unwrap();
    activate(&mut client, "Back to document");
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .mode,
        DesignerMode::Test
    );
    assert_eq!(authority.save().unwrap(), before);
}
