// SPDX-License-Identifier: LGPL-3.0-only
//! Compiler-only D2 checks. No authority receipt, process or live acceptance.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::{canvas::oib, dashboard::layout::Layout};
use otrios_interfaces::{
    canvas::Ui,
    layout::{TemplateNode, Value},
    oib::{authoring::*, designer::*},
};
use std::collections::BTreeMap;

use otrios_core::canvas::{Document as StateDocument, Session, StateAdapter, StateSelection};
use otrios_interfaces::oib::InstanceOperation;

fn batch(operations: Vec<DefinitionEdit>) -> DefinitionBatch {
    DefinitionBatch {
        generation: 7,
        operations,
    }
}
fn apply(ui: &Ui, operations: Vec<DefinitionEdit>) -> Ui {
    oib::document::authoring::apply(ui, &batch(operations)).unwrap()
}
fn path(references: &[u64], property: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(1),
        property: PropertyId(property),
    }
}
fn move_node(
    node: u64,
    owner: u64,
    visual_parent: Option<u64>,
    before: Option<u64>,
) -> DefinitionEdit {
    DefinitionEdit::MoveNode {
        definition: DefinitionId(10),
        node: NodeId(node),
        owner: NodeId(owner),
        visual_parent: visual_parent.map(NodeId),
        before: before.map(NodeId),
    }
}
fn render(ui: &Ui, address: &ObjectAddress) -> TemplateNode {
    fn find(n: &TemplateNode, id: u16) -> Option<&TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|n| find(n, id))
        }
    }
    let id = oib::seed(ui)
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .mappings
        .objects
        .iter()
        .find(|m| m.address == *address)
        .unwrap()
        .layout_node
        .unwrap();
    Layout::from_bytes(&ui.layout)
        .unwrap()
        .blueprint()
        .templates
        .iter()
        .find_map(|t| find(&t.root, id))
        .unwrap()
        .clone()
}
fn address(instance: u64, node: u64) -> ObjectAddress {
    ObjectAddress {
        instance: InstanceId(instance),
        references: vec![],
        node: NodeId(node),
    }
}

#[test]
fn shared_definition_reorder_updates_both_instances_without_changing_property_identity() {
    assert!(app::manifest().dependencies.iter().any(|d| d == FEATURE));
    let original = app::ui(&[]);
    let changed = apply(&original, vec![move_node(8, 1, Some(1), Some(7))]);
    let before = oib::seed(&original).unwrap();
    let after = oib::seed(&changed).unwrap();
    assert_eq!(before.cells, after.cells);
    assert_eq!(
        before.designer.as_ref().unwrap().mappings,
        after.designer.as_ref().unwrap().mappings
    );
    for instance in [20, 30] {
        let root = render(&changed, &address(instance, 1));
        let toggle = render(&changed, &address(instance, 8));
        let slider = render(&changed, &address(instance, 7));
        assert!(
            root.children.iter().position(|n| n.id == toggle.id)
                < root.children.iter().position(|n| n.id == slider.id)
        );
        assert_eq!(render(&original, &address(instance, 7)), slider);
    }
    assert_eq!(original.handlers, changed.handlers);
    assert_eq!(original.selectors, changed.selectors);
    assert_eq!(original.state_schema, changed.state_schema);
}

#[test]
fn ownership_and_visual_parent_are_independent_and_geometry_stays_parent_local() {
    let original = app::ui(&[]);
    let changed = apply(&original, vec![move_node(10, 1, Some(9), None)]);
    let doc = oib::seed(&changed).unwrap().designer.as_ref().unwrap();
    let node = doc
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == NodeId(10))
        .unwrap();
    assert_eq!(node.owner, Some(NodeId(1)));
    for instance in [20, 30] {
        let old = render(&original, &address(instance, 10));
        let new = render(&changed, &address(instance, 10));
        assert_eq!(old.frame, new.frame);
        assert_eq!(old.responder, new.responder);
        assert!(
            render(&changed, &address(instance, 9))
                .children
                .iter()
                .any(|n| n.id == new.id)
        );
    }
}

#[test]
fn rename_preserves_all_mappings_bindings_and_layout_bytes() {
    let original = app::ui(&[]);
    let changed = apply(
        &original,
        vec![
            DefinitionEdit::RenameDefinition {
                definition: DefinitionId(12),
                name: "Audio level".into(),
            },
            DefinitionEdit::RenameNode {
                definition: DefinitionId(10),
                node: NodeId(7),
                name: "Level control".into(),
            },
        ],
    );
    let a = oib::seed(&original).unwrap();
    let b = oib::seed(&changed).unwrap();
    assert_eq!(
        a.designer.as_ref().unwrap().mappings,
        b.designer.as_ref().unwrap().mappings
    );
    assert_eq!(a.cells, b.cells);
    assert_eq!(original.layout, changed.layout);
    assert_eq!(original.handlers, changed.handlers);
    assert_eq!(original.selectors, changed.selectors);
    assert_eq!(
        b.components.iter().find(|c| c.id == 12).unwrap().name,
        "Audio level"
    );
}

#[test]
fn explicit_forwarding_repair_retains_live_override_and_reset_reveals_current_source() {
    let original = app::ui(&[]);
    let changed = apply(
        &original,
        vec![
            DefinitionEdit::Expose {
                definition: DefinitionId(10),
                property: PropertyId(1),
                targets: vec![path(&[7], 1)],
            },
            DefinitionEdit::ConnectReference {
                definition: DefinitionId(10),
                reference: NodeId(8),
                target: path(&[], 1),
                source: Some(LiteralSource::Forward(path(&[7], 1))),
            },
        ],
    );
    let seed = oib::seed(&changed).unwrap();
    let mut values = BTreeMap::from([
        (201, Some(Value::Color([10, 20, 30, 255]))),
        (204, Some(Value::Color([40, 50, 60, 255]))),
    ]);
    assert_eq!(
        oib::effective(seed, &values, 204).unwrap(),
        Value::Color([40, 50, 60, 255])
    );
    values.insert(204, None);
    assert_eq!(
        oib::effective(seed, &values, 204).unwrap(),
        Value::Color([10, 20, 30, 255])
    );
    assert_eq!(original.handlers, changed.handlers);
    assert_eq!(original.selectors, changed.selectors);
}

#[test]
fn authored_reference_literal_can_be_removed_without_changing_factory_labels() {
    let original = app::ui(&[]);
    let operations = vec![DefinitionEdit::ConnectReference {
        definition: DefinitionId(10),
        reference: NodeId(8),
        target: path(&[], 3),
        source: Some(LiteralSource::Literal(Value::Text("Power".into()))),
    }];
    let changed = apply(&original, operations);
    let seed = oib::seed(&changed).unwrap();
    assert_eq!(
        oib::effective(seed, &BTreeMap::new(), 205).unwrap(),
        Value::Text("Power".into())
    );
    assert_eq!(
        oib::effective(seed, &BTreeMap::new(), 305).unwrap(),
        Value::Text("Power".into())
    );
    assert_eq!(
        oib::effective(seed, &BTreeMap::new(), 203).unwrap(),
        Value::Text("Volume".into())
    );
    assert_eq!(
        oib::effective(seed, &BTreeMap::new(), 303).unwrap(),
        Value::Text("Brightness".into())
    );
    let restored = apply(
        &changed,
        vec![DefinitionEdit::ConnectReference {
            definition: DefinitionId(10),
            reference: NodeId(8),
            target: path(&[], 3),
            source: None,
        }],
    );
    assert_eq!(
        oib::seed(&restored).unwrap().cells,
        oib::seed(&original).unwrap().cells
    );
}

#[test]
fn invalid_cycles_classes_anchors_and_inherited_moves_leave_original_bytes_unchanged() {
    let original = app::ui(&[]);
    let bytes = flexbuffers::to_vec(&original).unwrap();
    for operations in [
        vec![move_node(1, 9, Some(9), None)],
        vec![move_node(9, 10, Some(10), None)],
        vec![move_node(9, 1, Some(7), None)],
        vec![move_node(9, 1, Some(1), Some(10))],
        vec![move_node(9, 1, Some(1), Some(9))],
        vec![move_node(9, 1, None, Some(7))],
        vec![DefinitionEdit::ConnectReference {
            definition: DefinitionId(10),
            reference: NodeId(7),
            target: path(&[], 4),
            source: None,
        }],
        vec![DefinitionEdit::ConnectReference {
            definition: DefinitionId(10),
            reference: NodeId(7),
            target: path(&[], 1),
            source: Some(LiteralSource::Literal(Value::Number(5.))),
        }],
        vec![
            DefinitionEdit::Expose {
                definition: DefinitionId(10),
                property: PropertyId(1),
                targets: vec![],
            },
            DefinitionEdit::ConnectReference {
                definition: DefinitionId(10),
                reference: NodeId(7),
                target: path(&[], 1),
                source: Some(LiteralSource::Forward(path(&[8], 1))),
            },
            DefinitionEdit::ConnectReference {
                definition: DefinitionId(10),
                reference: NodeId(8),
                target: path(&[], 1),
                source: Some(LiteralSource::Forward(path(&[7], 1))),
            },
        ],
    ] {
        assert!(oib::document::authoring::apply(&original, &batch(operations)).is_err());
        assert_eq!(flexbuffers::to_vec(&original).unwrap(), bytes);
    }
}

#[test]
fn final_closure_validation_allows_explicit_repair_in_one_batch() {
    let original = app::ui(&[]);
    // First move temporarily makes ownership cyclic (button -> surface -> button).
    // The second move explicitly repairs it before complete closure validation.
    let changed = apply(
        &original,
        vec![
            move_node(9, 10, Some(1), None),
            move_node(9, 1, Some(1), Some(8)),
        ],
    );
    oib::validate(&changed).unwrap();
    assert_eq!(
        oib::seed(&original).unwrap().cells,
        oib::seed(&changed).unwrap().cells
    );
}

#[test]
fn operation_count_names_forwarding_order_and_generation_are_bounded() {
    let original = app::ui(&[]);
    let rename = |name| DefinitionEdit::RenameDefinition {
        definition: DefinitionId(10),
        name,
    };
    for operations in [
        vec![],
        vec![rename("ok".into()); MAX_OPERATIONS + 1],
        vec![rename(String::new())],
        vec![rename("x".repeat(65))],
        vec![rename("bad\0name".into())],
        vec![DefinitionEdit::Expose {
            definition: DefinitionId(10),
            property: PropertyId(1),
            targets: vec![path(&[8], 1), path(&[7], 1)],
        }],
        vec![DefinitionEdit::Expose {
            definition: DefinitionId(10),
            property: PropertyId(1),
            targets: vec![path(&[7], 1), path(&[7], 1)],
        }],
    ] {
        assert!(oib::document::authoring::apply(&original, &batch(operations)).is_err());
    }
    let mut invalid = batch(vec![rename("valid".into())]);
    invalid.generation = 0;
    assert!(oib::document::authoring::apply(&original, &invalid).is_err());
}

fn setup() -> (Ui, StateDocument, StateAdapter, Session) {
    let ui = app::ui(&[]);
    let mut doc = StateDocument::new(ui.state_schema.clone(), "d2:definitions").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut session, _) = doc.open_session(&[81; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(&ui, "d2:definitions", &[], [81; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(&mut doc, &mut session, &mut client);
    (ui, doc, client, session)
}
fn sync(doc: &mut StateDocument, session: &mut Session, client: &mut StateAdapter) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = client.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            client.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            assert!(client.confirmed());
            return;
        }
    }
    panic!("definition test did not settle");
}
fn request(
    doc: &StateDocument,
    sequence: u64,
    generation: u64,
    operations: Vec<DefinitionEdit>,
) -> Vec<u8> {
    otrios_core::canvas::encode_state_request(
        &otrios_interfaces::canvas::StateRequest::OibInstance {
            epoch: doc.oib_epoch().unwrap(),
            sequence,
            operation: InstanceOperation::Definitions(DefinitionBatch {
                generation,
                operations,
            }),
        },
    )
    .unwrap()
}

#[test]
fn authority_definition_edits_preserve_exact_causal_registers_and_restart_receipts() {
    let (ui, mut doc, mut client, mut session) = setup();
    assert!(client.set(&[(1, 481), (2, 729)]).unwrap());
    sync(&mut doc, &mut session, &mut client);
    assert!(
        client
            .edit_oib(&[(203, Some(Value::Text("Retained".into())))])
            .unwrap()
    );
    sync(&mut doc, &mut session, &mut client);
    let tree = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let generation = doc.oib_status().unwrap().generation;
    let (mut writer, _) = doc.open_session(&[82; 16], 0).unwrap();
    writer.set_oib_editable(true);
    let accepted = request(&doc, 1, generation, vec![move_node(8, 1, Some(1), Some(7))]);
    assert!(doc.receive(&mut writer, &accepted).unwrap());
    assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
    let after = otrios_core::merkle::Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    for branch in [1, 2, 5, 6] {
        assert_eq!(
            tree.subtree(&[4, branch]).unwrap().to_bytes().unwrap(),
            after.subtree(&[4, branch]).unwrap().to_bytes().unwrap()
        );
    }
    assert_eq!(
        tree.subtree(&[1]).unwrap().hash(),
        after.subtree(&[1]).unwrap().hash()
    );
    let (compiled, _) = doc.oib_compiled(2).unwrap();
    let def = oib::seed(&compiled)
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    assert_eq!(def.nodes[0].visual_children, [2, 3, 8, 7, 9].map(NodeId));
    let status = doc.oib_status().unwrap();
    let saved = doc.save().unwrap();
    let mut recovered =
        StateDocument::load(ui.state_schema.clone(), "d2:definitions", &saved).unwrap();
    recovered.configure_oib(&ui).unwrap();
    assert_eq!(
        recovered.oib_status().unwrap().active_root,
        status.active_root
    );
    let (mut resumed, _) = recovered.open_session(&[82; 16], 1).unwrap();
    resumed.set_oib_editable(true);
    assert!(!recovered.receive(&mut resumed, &accepted).unwrap());
    assert_eq!(recovered.oib_status().unwrap().generation, generation + 1);
    assert_eq!(recovered.oib_compiled(2).unwrap().0, compiled);
    // The existing adapter accepts the same normalized authored metadata.
    sync(&mut doc, &mut session, &mut client);
    assert!(
        client
            .edit_oib_structure(InstanceOperation::Definitions(DefinitionBatch {
                generation: generation + 1,
                operations: vec![move_node(8, 1, Some(1), Some(9))],
            }))
            .unwrap()
    );
    sync(&mut doc, &mut session, &mut client);
    assert_eq!(doc.oib_status().unwrap().generation, generation + 2);
}

#[test]
fn stale_and_invalid_definition_batches_keep_exact_active_graph_and_receipt_rejection() {
    let (ui, mut doc, _client, _session) = setup();
    let generation = doc.oib_status().unwrap().generation;
    let (mut writer, _) = doc.open_session(&[83; 16], 0).unwrap();
    writer.set_oib_editable(true);
    let accepted = request(&doc, 1, generation, vec![move_node(8, 1, Some(1), Some(7))]);
    assert!(doc.receive(&mut writer, &accepted).unwrap());
    let active = doc.oib_status().unwrap().active_root;
    let compiled = doc.oib_compiled(2).unwrap().0;
    let mut last = vec![];
    for (sequence, observed, operations) in [
        (2, generation, vec![move_node(8, 1, Some(1), Some(9))]),
        (3, generation + 1, vec![move_node(9, 1, Some(7), None)]),
        (
            4,
            generation + 1,
            vec![
                DefinitionEdit::RenameDefinition {
                    definition: DefinitionId(10),
                    name: "unpublished".into(),
                },
                move_node(9, 1, Some(7), None),
            ],
        ),
    ] {
        last = request(&doc, sequence, observed, operations);
        assert!(doc.receive(&mut writer, &last).unwrap());
        assert_eq!(doc.oib_status().unwrap().active_root, active);
        assert_eq!(doc.oib_compiled(2).unwrap().0, compiled);
        assert!(
            doc.oib_status()
                .unwrap()
                .error
                .unwrap()
                .contains("Structure rejected")
        );
        assert!(!doc.receive(&mut writer, &last).unwrap());
    }
    let mut recovered = StateDocument::load(
        ui.state_schema.clone(),
        "d2:definitions",
        &doc.save().unwrap(),
    )
    .unwrap();
    recovered.configure_oib(&ui).unwrap();
    let (mut resumed, _) = recovered.open_session(&[83; 16], 4).unwrap();
    resumed.set_oib_editable(true);
    assert!(!recovered.receive(&mut resumed, &last).unwrap());
    let retry = request(
        &doc,
        5,
        generation + 1,
        vec![move_node(8, 1, Some(1), Some(9))],
    );
    assert!(recovered.receive(&mut resumed, &retry).unwrap());
    assert!(recovered.oib_status().unwrap().error.is_none());
    assert_ne!(recovered.oib_status().unwrap().active_root, active);
}

#[test]
fn snapshots_cannot_replace_signed_classes_defaults_references_or_binding_grants() {
    let ui = app::ui(&[]);
    let seed = oib::seed(&ui).unwrap();
    let original = &seed.designer.as_ref().unwrap().definitions;
    for kind in 0..5 {
        let mut definitions = original.clone();
        match kind {
            0 => definitions[0].nodes[0].class = ClassId(1),
            1 => definitions[0].nodes[0].defaults[0].value = Value::Color([1, 2, 3, 4]),
            2 => {
                definitions[0]
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == NodeId(7))
                    .unwrap()
                    .reference
                    .as_mut()
                    .unwrap()
                    .definition = DefinitionId(13)
            }
            3 => definitions[0]
                .nodes
                .iter_mut()
                .find(|n| n.id == NodeId(7))
                .unwrap()
                .reference
                .as_mut()
                .unwrap()
                .assignments
                .push(Assignment {
                    target: path(&[], 4),
                    source: AssignmentSource::AppBinding(
                        otrios_interfaces::layout::Binding::Local(999),
                    ),
                }),
            _ => {
                definitions.pop();
            }
        }
        assert!(oib::document::authoring::restore(&ui, &definitions).is_err());
    }
}
