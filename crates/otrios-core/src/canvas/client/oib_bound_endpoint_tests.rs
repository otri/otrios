// SPDX-License-Identifier: LGPL-3.0-only
//! Shared Designer composition, admitted bindings and real Wasmi sample behavior.
use super::tests::{activate_api_control, activate_studio_tool, mounted_fixture_session_with_ui};
use super::*;
use otrios_interfaces::oib::authoring::{DefinitionEdit, DocumentEdit, EditBatch};
use otrios_interfaces::oib::designer::*;

#[allow(dead_code)]
#[path = "../../../tests/support/oib_library_fixture.rs"]
mod fixture;

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
    panic!("bound endpoint fixture did not settle");
}
fn activate_visible(client: &mut Client, label: &str) {
    let id = client
        .scene()
        .unwrap()
        .controls()
        .iter()
        .find(|p| p.node.semantics.as_ref().is_some_and(|s| s.label == label))
        .unwrap_or_else(|| panic!("Missing visible control {label}"))
        .node
        .id;
    assert!(
        client.activate(client.epoch(), id).unwrap(),
        "activate {label}"
    );
}
fn composed() -> (
    Client,
    crate::canvas::Document,
    Session,
    DefinitionId,
    NodeId,
    NodeId,
    PropertyId,
) {
    let ui = super::super::component_behavior_tests::library_ui(
        super::super::component_behavior_tests::snapping_guest(100),
    );
    let ui = fixture::admit(&ui);
    let export = fixture::seed(&ui).resolved_libraries[0]
        .graph
        .exports
        .iter()
        .find(|export| export.definition == DefinitionId(12))
        .unwrap()
        .name
        .clone();
    let (mut client, authority, session) = mounted_fixture_session_with_ui(true, 0, ui);
    client.set_viewport([1280., 800.]).unwrap();
    activate_studio_tool(&mut client, "New component");
    let definition = client
        .active
        .as_ref()
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .definition_scope()
        .unwrap();
    activate_studio_tool(&mut client, &format!("Link {export}"));
    activate_visible(&mut client, "Layers");
    activate_visible(&mut client, "Linked component");
    activate_api_control(&mut client, "> Component API");
    activate_api_control(&mut client, "> Value · Number");
    activate_api_control(&mut client, "Expose as value");
    let source = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_history_source()
        .unwrap();
    let d = source
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let child = d.nodes.iter().find(|n| n.reference.is_some()).unwrap().id;
    let descriptor = d.properties.iter().find(|p| p.name == "value").unwrap();
    assert!(!descriptor.policy.live_literal && descriptor.policy.app_binding);
    assert_eq!(descriptor.editor, PropertyEditor::ReadOnly);
    (
        client,
        authority,
        session,
        definition,
        d.root,
        child,
        descriptor.id,
    )
}
fn slider(client: &Client) -> (u8, [f32; 2], f32) {
    let scene = client.scene().unwrap();
    let p = scene
        .controls()
        .into_iter()
        .find(|p| {
            matches!(
                p.node.responder,
                Some(TouchResponder::Drag(Action::AppEvent(_)))
            )
        })
        .expect("forwarded Slider is interactive in Test");
    let Some(TouchResponder::Drag(Action::AppEvent(id))) = p.node.responder else {
        unreachable!()
    };
    (
        id,
        [
            p.origin[0],
            p.origin[1] + p.node.frame.height * p.scale * 0.5,
        ],
        p.node.frame.width * p.scale,
    )
}

#[test]
fn bound_endpoint_definition_test_runs_real_guest_on_one_private_outer_sample() {
    let (mut client, authority, _session, definition, root, _child, property) = composed();
    let before = authority.save().unwrap();
    let fields =
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().doc.typed_value(id).unwrap());
    let address = PropertyAddress::Definition {
        definition,
        node: root,
        property,
    };
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .design_doc()
            .oib_property_register(&address)
            .is_err(),
        "bound endpoint has no editable causal literal"
    );
    assert!(client.set_oib_designer_mode(DesignerMode::Test).unwrap());
    let (control, _, _) = slider(&client);
    client.event(client.epoch(), control, 687, 0).unwrap();
    let m = client.active.as_ref().unwrap();
    let preview = m.definition_presentation.as_ref().unwrap();
    assert_eq!(
        preview.samples.typed_value(1).unwrap(),
        Value::Number(600.),
        "Wasmi guest snaps the sample; host Set would write 687"
    );
    assert_eq!(
        preview.preview.ui.state_schema.fields.len(),
        1,
        "outer binding and child outlet share a sample source"
    );
    assert_eq!(
        [1, 2, 3, 4].map(|id| m.doc.typed_value(id).unwrap()),
        fields
    );
    assert_eq!(authority.save().unwrap(), before);
    assert!(client.outgoing().unwrap().is_none());
    assert_eq!(client.resource_usage().component_behavior_errors, 0);
    assert!(client.resource_usage().component_behavior_calls > 0);
    client.set_oib_designer_mode(DesignerMode::Edit).unwrap();
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    assert_eq!(
        client
            .active
            .as_ref()
            .unwrap()
            .definition_presentation
            .as_ref()
            .unwrap()
            .samples
            .typed_value(1)
            .unwrap(),
        Value::Number(0.),
        "sample values reset without editing authored defaults"
    );
}

#[test]
fn bound_endpoint_can_be_inserted_and_connected_at_outer_occurrence_with_signed_grant() {
    let (mut client, mut authority, mut session, definition, _root, _child, property) = composed();
    let fields =
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().doc.typed_value(id).unwrap());
    client
        .inspect_oib_selection(Selection::Document(DocumentId(1)))
        .unwrap();
    activate_studio_tool(&mut client, "Insert component...");
    activate_studio_tool(&mut client, "Insert New component");
    let source = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_history_source()
        .unwrap();
    let instance = source
        .instances
        .iter()
        .find(|i| i.component == definition.0)
        .unwrap()
        .id;
    let root = source
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap()
        .root;
    let address = PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: vec![],
            node: root,
            property,
        },
    };
    let choices = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_connection_choices(&address)
        .unwrap();
    let (_, connection) = choices
        .iter()
        .find(|(label, _)| label == "Bind to volume")
        .expect("outer endpoint offers admitted volume source");
    client.designer_stage_structure(connection.clone()).unwrap();
    assert!(client.designer_apply_working_copy().unwrap());
    settle(&mut client, &mut authority, &mut session);
    let m = client.active.as_ref().unwrap();
    assert_eq!(
        [1, 2, 3, 4].map(|id| m.doc.typed_value(id).unwrap()),
        fields
    );
    let source = m.doc.oib_history_source().unwrap();
    let connection = source
        .connections
        .iter()
        .find(|c| c.object.instance == InstanceId(instance))
        .unwrap();
    assert_eq!(connection.object.references, vec![]);
    assert_eq!(connection.property, property);
    assert_eq!(connection.source.field, 1);
    assert_eq!(client.resource_usage().component_behavior_errors, 0);
    client
        .inspect_oib_selection(Selection::Object(ObjectAddress {
            instance: InstanceId(instance),
            references: vec![],
            node: root,
        }))
        .unwrap();
    fn contains_text(node: &SceneNode, expected: &str) -> bool {
        matches!(&node.primitive, Primitive::Text { value, .. } if value == expected)
            || node
                .children
                .iter()
                .any(|child| contains_text(child, expected))
    }
    let designer = client.active.as_ref().unwrap().designer.as_ref().unwrap();
    assert!(
        designer
            .panels
            .iter()
            .flat_map(|panel| &panel.nodes)
            .any(|node| contains_text(node, "App value")),
        "a resolved app binding is not an unavailable property"
    );
    assert!(
        !designer
            .panels
            .iter()
            .flat_map(|panel| &panel.nodes)
            .any(|node| contains_text(node, "Unavailable"))
    );
}

#[test]
fn bound_endpoint_published_undo_keeps_owned_fallback_and_control_route() {
    let (mut client, mut authority, mut session, definition, root, child, property) = composed();
    let m = client.active.as_mut().unwrap();
    m.stage_structure(EditBatch {
        generation: m.design_doc().oib_structural_generation().unwrap(),
        operations: vec![DocumentEdit::Definition(DefinitionEdit::SetDefault {
            definition,
            node: root,
            property,
            value: Value::Number(123.),
        })],
    })
    .unwrap();
    client.designer_apply_working_copy().unwrap();
    settle(&mut client, &mut authority, &mut session);
    let fields =
        [1, 2, 3, 4].map(|id| client.active.as_ref().unwrap().doc.typed_value(id).unwrap());
    let m = client.active.as_mut().unwrap();
    m.stage_structure(EditBatch {
        generation: m.design_doc().oib_structural_generation().unwrap(),
        operations: vec![DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
            definition,
            property,
        })],
    })
    .unwrap();
    client.designer_apply_working_copy().unwrap();
    settle(&mut client, &mut authority, &mut session);
    assert!(client.designer_history(false).unwrap());
    client.designer_apply_working_copy().unwrap();
    settle(&mut client, &mut authority, &mut session);
    let m = client.active.as_ref().unwrap();
    let source = m.doc.oib_history_source().unwrap();
    let d = source
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .unwrap();
    let restored = d.properties.iter().find(|p| p.name == "value").unwrap();
    assert!(restored.id.0 > property.0);
    assert!(!restored.policy.live_literal && restored.policy.app_binding);
    assert!(
        d.nodes
            .iter()
            .find(|n| n.id == root)
            .unwrap()
            .defaults
            .iter()
            .any(|value| value.property == restored.id && value.value == Value::Number(123.))
    );
    assert!(d.exposed.iter().any(|route| {
        route.property == restored.id
            && route
                .targets
                .iter()
                .any(|path| path.references == vec![child] && path.property == PropertyId(4))
    }));
    assert_eq!(
        [1, 2, 3, 4].map(|id| m.doc.typed_value(id).unwrap()),
        fields
    );
    assert_eq!(client.resource_usage().component_behavior_errors, 0);
}

#[test]
fn style_light_preview_before_apply_retains_live_command_and_receipt() {
    use otrios_interfaces::oib::styles::{
        ColorModes, FieldId, FieldValue, StyleEdit, StyleId, SystemId, SystemRef, VariableId,
        VariableRef,
    };
    let ui = fixture::admit(&super::super::component_behavior_tests::library_ui(
        super::super::component_behavior_tests::snapping_guest(100),
    ));
    let (mut client, mut authority, mut session) = mounted_fixture_session_with_ui(true, 0, ui);
    let source = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_history_source()
        .unwrap();
    let style = source.next_identity;
    let owner = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_authoring_seed()
        .unwrap()
        .graph
        .unwrap()
        .id;
    let generation = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_structural_generation()
        .unwrap();
    client
        .designer_stage_structure(EditBatch {
            generation,
            operations: vec![
                DocumentEdit::Style(StyleEdit::CreateStyle {
                    id: style,
                    name: "panel.primary".into(),
                    definition: DefinitionId(10),
                }),
                DocumentEdit::Style(StyleEdit::CreateVariable {
                    id: style + 1,
                    name: "surface".into(),
                    values: ColorModes {
                        light: [120, 150, 240, 255],
                        dark: [220, 170, 40, 255],
                    },
                }),
                DocumentEdit::Style(StyleEdit::SetField {
                    style: StyleId(style),
                    field: FieldId(1),
                    value: FieldValue::Variable(VariableRef {
                        system: SystemRef {
                            owner: otrios_interfaces::oib::styles::Owner::Document(owner),
                            system: SystemId(1),
                        },
                        variable: VariableId(style + 1),
                    }),
                }),
            ],
        })
        .unwrap();
    assert!(client.designer_apply_working_copy().unwrap());
    settle(&mut client, &mut authority, &mut session);
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(10)))
        .unwrap();
    let generation = client
        .active
        .as_ref()
        .unwrap()
        .design_doc()
        .oib_structural_generation()
        .unwrap();
    client
        .designer_stage_structure(EditBatch {
            generation,
            operations: vec![
                DocumentEdit::Definition(DefinitionEdit::ClearDefault {
                    definition: DefinitionId(10),
                    node: NodeId(1),
                    property: PropertyId(1),
                }),
                DocumentEdit::Style(StyleEdit::Assign {
                    definition: DefinitionId(10),
                    style: Some(StyleId(style)),
                }),
            ],
        })
        .unwrap();
    assert!(
        client
            .designer_action(LocalAction::Appearance(Appearance::Light))
            .unwrap()
    );
    assert!(client.designer_apply_working_copy().unwrap());
    assert_eq!(
        client.active.as_ref().unwrap().doc.pending_count(),
        1,
        "submitted Apply must remain on live transport adapter"
    );
    settle(&mut client, &mut authority, &mut session);
    let mount = client.active.as_ref().unwrap();
    assert!(
        !mount.working_copy.as_ref().unwrap().applying(),
        "durable receipt releases Applying"
    );
    assert!(
        mount
            .doc
            .oib_history_source()
            .unwrap()
            .styles
            .unwrap()
            .components
            .iter()
            .any(|binding| binding.definition == DefinitionId(10))
    );
}
