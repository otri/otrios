// SPDX-License-Identifier: LGPL-3.0-only
//! One structural contract and one authority publication per accepted batch.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;

use otrios_core::{
    canvas::{self, Document, StateAdapter, StateSelection, oib},
    merkle::Snapshot,
};
use otrios_interfaces::{
    canvas::{StateRequest, Ui},
    layout::Value,
    oib::{InstanceOperation, authoring::*, designer::*},
};

fn setup() -> (Ui, Document) {
    assert_eq!(app::manifest().id, "oib-designer");
    let ui = app::ui(&[]);
    let mut doc = Document::new(ui.state_schema.clone(), "d2:batch").unwrap();
    doc.configure_oib(&ui).unwrap();
    (ui, doc)
}
fn session(doc: &mut Document, writer: u8) -> canvas::Session {
    let (mut session, _) = doc.open_session(&[writer; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
}
fn batch(doc: &Document, operations: Vec<DocumentEdit>) -> InstanceOperation {
    EditBatch {
        generation: doc.oib_status().unwrap().structural_generation,
        operations,
    }
}
fn request(doc: &Document, sequence: u64, operation: InstanceOperation) -> Vec<u8> {
    canvas::encode_state_request(&StateRequest::OibInstance {
        epoch: doc.oib_epoch().unwrap(),
        sequence,
        operation,
    })
    .unwrap()
}
fn next_id(doc: &Document) -> u64 {
    let snapshot = Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let leaf = snapshot.subtree(&[4, 4]).unwrap();
    let value = leaf.register().unwrap().values().next().unwrap().2.unwrap();
    flexbuffers::from_slice::<DocumentSource>(value)
        .unwrap()
        .next_identity
}
fn source(doc: &Document) -> Vec<u8> {
    Snapshot::from_bytes(&doc.save().unwrap())
        .unwrap()
        .subtree(&[4, 4])
        .unwrap()
        .to_bytes()
        .unwrap()
}
fn insert(name: &str) -> DocumentEdit {
    DocumentEdit::Hierarchy(HierarchyEdit::Insert {
        prototype: InstanceId(50),
        name: Some(name.into()),
        destination: Destination::Canvas,
        before: None,
        frame: None,
    })
}

#[test]
fn mixed_definition_and_instance_edits_publish_one_generation_and_preserve_property_registers() {
    let (_ui, mut doc) = setup();
    let mut writer = session(&mut doc, 11);
    let original = doc.clone();
    let expected_id = next_id(&doc);
    let generation = doc.oib_status().unwrap().generation;
    let operation = batch(
        &doc,
        vec![
            DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                definition: DefinitionId(10),
                name: "Shared settings".into(),
            }),
            insert("Auxiliary power"),
            DocumentEdit::Hierarchy(HierarchyEdit::Reorder {
                instance: InstanceId(30),
                before: Some(InstanceId(20)),
            }),
        ],
    );
    let bytes = request(&doc, 1, operation);
    assert!(doc.receive(&mut writer, &bytes).unwrap());
    assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
    assert!(doc.oib_status().unwrap().error.is_none());
    assert_eq!(
        doc.oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![30, 20, expected_id]
    );
    assert_eq!(
        doc.oib_allocations().unwrap().last().unwrap().name,
        "Auxiliary power"
    );
    let compiled = doc.oib_compiled(2).unwrap().0;
    assert_eq!(
        oib::seed(&compiled)
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .definitions
            .iter()
            .find(|d| d.id == DefinitionId(10))
            .unwrap()
            .name,
        "Shared settings"
    );
    for branch in [1, 2] {
        for cell in original.oib_values(branch).unwrap().keys() {
            assert_eq!(
                original
                    .oib_property_leaf(branch, *cell)
                    .unwrap()
                    .to_bytes()
                    .unwrap(),
                doc.oib_property_leaf(branch, *cell)
                    .unwrap()
                    .to_bytes()
                    .unwrap()
            );
        }
    }
    assert_eq!(
        (1..=4)
            .map(|id| original.typed_value(id).unwrap())
            .collect::<Vec<_>>(),
        (1..=4)
            .map(|id| doc.typed_value(id).unwrap())
            .collect::<Vec<_>>()
    );
}

#[test]
fn failure_after_allocation_discards_every_change_and_replays_after_restart() {
    let (ui, mut doc) = setup();
    let mut writer = session(&mut doc, 12);
    let before = source(&doc);
    let expected_id = next_id(&doc);
    let generation = doc.oib_status().unwrap().generation;
    let rejected = request(
        &doc,
        1,
        batch(
            &doc,
            vec![
                insert("Discarded"),
                DocumentEdit::Hierarchy(HierarchyEdit::Reorder {
                    instance: InstanceId(20),
                    before: Some(InstanceId(999_999)),
                }),
            ],
        ),
    );
    assert!(doc.receive(&mut writer, &rejected).unwrap());
    assert_eq!(
        source(&doc),
        before,
        "failed candidate consumes no identity or source changes"
    );
    assert_eq!(doc.oib_status().unwrap().generation, generation);
    assert!(doc.oib_status().unwrap().error.is_some());
    let after_rejection = doc.save().unwrap();
    assert!(!doc.receive(&mut writer, &rejected).unwrap());
    assert_eq!(doc.save().unwrap(), after_rejection);
    let accepted = request(&doc, 2, batch(&doc, vec![insert("Accepted")]));
    assert!(doc.receive(&mut writer, &accepted).unwrap());
    assert_eq!(
        doc.oib_allocations().unwrap().last().unwrap().id,
        expected_id
    );
    assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
    let saved = doc.save().unwrap();
    let mut recovered = Document::load(ui.state_schema.clone(), "d2:batch", &saved).unwrap();
    recovered.configure_oib(&ui).unwrap();
    let (mut resumed, _) = recovered.open_session(&[12; 16], 2).unwrap();
    resumed.set_oib_editable(true);
    assert!(!recovered.receive(&mut resumed, &accepted).unwrap());
    assert_eq!(recovered.save().unwrap(), saved);
}

#[test]
fn racing_batches_accept_one_generation_and_reject_without_allocation() {
    let (_ui, mut doc) = setup();
    let mut a = session(&mut doc, 13);
    let mut b = session(&mut doc, 14);
    let generation = doc.oib_status().unwrap().generation;
    let accepted = request(
        &doc,
        1,
        InstanceOperation::reorder(generation, 30, Some(20)),
    );
    let stale = request(&doc, 1, batch(&doc, vec![insert("Stale")]));
    assert!(doc.receive(&mut a, &accepted).unwrap());
    let accepted_source = source(&doc);
    assert!(doc.receive(&mut b, &stale).unwrap());
    assert_eq!(source(&doc), accepted_source);
    assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
    assert!(
        doc.oib_status()
            .unwrap()
            .error
            .unwrap()
            .contains("Document changed")
    );
}

#[test]
fn envelope_bounds_phase_order_and_restore_exclusivity_reject_atomically() {
    let (_ui, mut doc) = setup();
    let mut writer = session(&mut doc, 15);
    let before = source(&doc);
    let definition = DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
        definition: DefinitionId(10),
        name: "No change".into(),
    });
    let operations = [
        vec![],
        vec![definition.clone(); MAX_OPERATIONS + 1],
        vec![insert("Discarded"), definition],
        vec![
            DocumentEdit::Restore { defaults: true },
            insert("Discarded"),
        ],
    ];
    for (index, operations) in operations.into_iter().enumerate() {
        let bytes = request(&doc, index as u64 + 1, batch(&doc, operations));
        assert!(doc.receive(&mut writer, &bytes).unwrap());
        assert_eq!(source(&doc), before);
        assert_eq!(doc.oib_status().unwrap().generation, 1);
        assert!(doc.oib_status().unwrap().error.is_some());
    }
}

fn client(ui: &Ui, doc: &mut Document, writer: u8) -> (StateAdapter, canvas::Session) {
    let mut session = session(doc, writer);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(ui, "d2:batch", &[], [writer; 16]).unwrap();
    client
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    for _ in 0..20 {
        let mut progress = false;
        while let Some(bytes) = client.generate().unwrap() {
            doc.receive(&mut session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(&mut session).unwrap() {
            client.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            return (client, session);
        }
    }
    panic!("client did not settle");
}

#[test]
fn unrelated_confirmed_property_edit_does_not_stale_a_captured_structural_batch() {
    let (ui, mut doc) = setup();
    let (mut property, mut property_session) = client(&ui, &mut doc, 19);
    let mut structural = session(&mut doc, 20);
    let captured = batch(&doc, vec![insert("After property edit")]);
    let address = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    property
        .edit_oib_property(
            &address,
            Some(Value::Text("Confirmed independently".into())),
        )
        .unwrap();
    assert!(
        doc.receive(
            &mut property_session,
            &property.generate().unwrap().unwrap()
        )
        .unwrap()
    );
    assert_eq!(doc.oib_status().unwrap().generation, 2);
    assert_eq!(doc.oib_status().unwrap().structural_generation, 1);
    let compiled = doc.oib_compiled(2).unwrap().0;
    let cell = oib::document::cell_for_address(
        oib::seed(&compiled).unwrap(),
        &doc.oib_allocations().unwrap(),
        &address,
    )
    .unwrap();
    let before = doc.oib_property_leaf(1, cell).unwrap().to_bytes().unwrap();
    let bytes = request(&doc, 1, captured);
    assert!(doc.receive(&mut structural, &bytes).unwrap());
    assert!(doc.oib_status().unwrap().error.is_none());
    assert_eq!(doc.oib_status().unwrap().generation, 3);
    assert_eq!(doc.oib_status().unwrap().structural_generation, 2);
    assert_eq!(
        doc.oib_property_leaf(1, cell).unwrap().to_bytes().unwrap(),
        before
    );
}

#[test]
fn property_conflicts_block_structure_and_explicit_reset_all_recovers_once() {
    let (ui, mut doc) = setup();
    let (mut a, mut sa) = client(&ui, &mut doc, 16);
    let (mut b, mut sb) = client(&ui, &mut doc, 17);
    let address = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    };
    a.edit_oib_property(&address, Some(Value::Text("Alpha".into())))
        .unwrap();
    b.edit_oib_property(&address, Some(Value::Text("Beta".into())))
        .unwrap();
    assert!(
        doc.receive(&mut sa, &a.generate().unwrap().unwrap())
            .unwrap()
    );
    assert!(
        doc.receive(&mut sb, &b.generate().unwrap().unwrap())
            .unwrap()
    );
    assert_eq!(doc.oib_status().unwrap().conflicts, vec![address]);
    let before = source(&doc);
    let generation = doc.oib_status().unwrap().generation;
    let mut structural = session(&mut doc, 18);
    let rejected = request(&doc, 1, batch(&doc, vec![insert("Blocked")]));
    assert!(doc.receive(&mut structural, &rejected).unwrap());
    assert_eq!(source(&doc), before);
    let reset = request(
        &doc,
        2,
        InstanceOperation::restore(doc.oib_status().unwrap().structural_generation, true),
    );
    assert!(doc.receive(&mut structural, &reset).unwrap());
    assert_eq!(doc.oib_status().unwrap().generation, generation + 1);
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
    assert!(doc.oib_status().unwrap().error.is_none());
    assert_eq!(doc.typed_value(1).unwrap(), Value::Number(400.));
}

fn stored_source(doc: &Document) -> DocumentSource {
    let snapshot = Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    let leaf = snapshot.subtree(&[4, 4]).unwrap();
    flexbuffers::from_slice(leaf.register().unwrap().values().next().unwrap().2.unwrap()).unwrap()
}
fn submit(
    doc: &mut Document,
    session: &mut canvas::Session,
    sequence: u64,
    operations: Vec<DocumentEdit>,
) {
    let bytes = request(doc, sequence, batch(doc, operations));
    doc.receive(session, &bytes).unwrap();
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
}
fn plain_visual() -> otrios_interfaces::oib::graph::Visual {
    use otrios_interfaces::layout::{Expr, Shape};
    otrios_interfaces::oib::graph::Visual {
        frame: [0., 0., 100., 30.].map(|n| Expr::Literal(Value::Number(n))),
        resize: None,
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: true },
        semantics: None,
        action: None,
        parameters: vec![],
    }
}
fn slider_node(id: u64, owner: Option<NodeId>) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: "Authored slider".into(),
        class: ClassId(9),
        owner,
        visual_children: vec![],
        defaults: vec![],
        reference: None,
        visual: Some(plain_visual()),
    }
}
fn property_value(doc: &Document, address: &PropertyAddress) -> Value {
    let compiled = doc.oib_compiled(2).unwrap().0;
    let seed = oib::seed(&compiled).unwrap();
    let id =
        oib::document::cell_for_address(seed, &doc.oib_allocations().unwrap(), address).unwrap();
    oib::effective(seed, &doc.oib_values(2).unwrap(), id).unwrap()
}
fn literal(
    ui: &Ui,
    doc: &mut Document,
    writer: u8,
    address: &PropertyAddress,
    value: Option<Value>,
) {
    let (mut adapter, mut session) = client(ui, doc, writer);
    adapter.edit_oib_property(address, value).unwrap();
    doc.receive(&mut session, &adapter.generate().unwrap().unwrap())
        .unwrap();
    assert!(doc.oib_status().unwrap().error.is_none());
}

#[test]
fn authored_defaults_flow_through_untouched_registers_and_clears_preserve_explicit_overrides() {
    let (ui, mut doc) = setup();
    let mut structural = session(&mut doc, 40);
    let definition = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    let occurrence = |instance| PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(2),
        },
    };
    let default = |n| {
        DocumentEdit::Definition(DefinitionEdit::SetDefault {
            definition: DefinitionId(12),
            node: NodeId(1),
            property: PropertyId(2),
            value: Value::Number(n),
        })
    };
    submit(&mut doc, &mut structural, 1, vec![default(400.)]);
    assert_eq!(property_value(&doc, &occurrence(20)), Value::Number(400.));
    assert_eq!(property_value(&doc, &occurrence(30)), Value::Number(400.));
    literal(&ui, &mut doc, 41, &definition, Some(Value::Number(360.)));
    literal(
        &ui,
        &mut doc,
        42,
        &occurrence(20),
        Some(Value::Number(320.)),
    );
    submit(&mut doc, &mut structural, 2, vec![default(440.)]);
    assert_eq!(property_value(&doc, &occurrence(20)), Value::Number(320.));
    assert_eq!(property_value(&doc, &occurrence(30)), Value::Number(360.));
    literal(&ui, &mut doc, 43, &definition, None);
    assert_eq!(property_value(&doc, &occurrence(20)), Value::Number(320.));
    assert_eq!(property_value(&doc, &occurrence(30)), Value::Number(440.));
    literal(&ui, &mut doc, 44, &occurrence(20), None);
    assert_eq!(property_value(&doc, &occurrence(20)), Value::Number(440.));
}

#[test]
fn create_and_delete_a_definition_preset_and_live_instance_validate_only_the_final_closure() {
    let (_ui, mut doc) = setup();
    let mut structural = session(&mut doc, 45);
    let next = next_id(&doc);
    let preset = otrios_interfaces::oib::graph::Preset {
        id: next + 2,
        name: "Local slider".into(),
        definition: DefinitionId(next),
        assignments: vec![],
        frame: None,
        palette: true,
    };
    submit(
        &mut doc,
        &mut structural,
        1,
        vec![
            DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
                definition: DefinitionId(next),
                name: "Local component".into(),
                root: slider_node(next + 1, None),
            }),
            DocumentEdit::Preset(PresetEdit::Create { preset }),
            DocumentEdit::Hierarchy(HierarchyEdit::Insert {
                prototype: InstanceId(next + 2),
                name: None,
                destination: Destination::Canvas,
                before: None,
                frame: None,
            }),
        ],
    );
    assert_eq!(doc.oib_allocations().unwrap().last().unwrap().id, next + 3);
    assert_eq!(next_id(&doc), next + 4);
    let before = source(&doc);
    let failed = request(
        &doc,
        2,
        batch(
            &doc,
            vec![DocumentEdit::Definition(DefinitionEdit::DeleteDefinition {
                definition: DefinitionId(next),
            })],
        ),
    );
    doc.receive(&mut structural, &failed).unwrap();
    assert!(doc.oib_status().unwrap().error.is_some());
    assert_eq!(source(&doc), before);
    submit(
        &mut doc,
        &mut structural,
        3,
        vec![
            DocumentEdit::Definition(DefinitionEdit::DeleteDefinition {
                definition: DefinitionId(next),
            }),
            DocumentEdit::Preset(PresetEdit::Delete { preset: next + 2 }),
            DocumentEdit::Hierarchy(HierarchyEdit::Remove {
                instance: InstanceId(next + 3),
            }),
        ],
    );
    assert_eq!(doc.oib_allocations().unwrap().len(), 2);
    assert!(
        !stored_source(&doc)
            .definitions
            .iter()
            .any(|d| d.id == DefinitionId(next))
    );
    assert_eq!(
        next_id(&doc),
        next + 4,
        "deletion never returns identities to the allocator"
    );
}

#[test]
fn deleting_a_connected_node_accepts_explicit_later_repair_and_acknowledges_delayed_literal_edits()
{
    let (ui, mut doc) = setup();
    let mut structural = session(&mut doc, 46);
    let next = next_id(&doc);
    let object = ObjectAddress {
        instance: InstanceId(20),
        references: vec![],
        node: NodeId(next),
    };
    submit(
        &mut doc,
        &mut structural,
        1,
        vec![
            DocumentEdit::Definition(DefinitionEdit::CreateNode {
                definition: DefinitionId(10),
                node: slider_node(next, Some(NodeId(1))),
                visual_parent: Some(NodeId(1)),
                before: None,
            }),
            DocumentEdit::Hierarchy(HierarchyEdit::Connect {
                object: object.clone(),
                property: PropertyId(4),
                source: Some(ConnectionSource {
                    field: 1,
                    projection: otrios_interfaces::canvas::Projection::Identity,
                }),
            }),
        ],
    );
    let (mut delayed, mut delayed_session) = client(&ui, &mut doc, 47);
    delayed
        .edit_oib_property(
            &PropertyAddress::Occurrence {
                instance: InstanceId(20),
                path: PropertyPath {
                    references: vec![],
                    node: NodeId(next),
                    property: PropertyId(3),
                },
            },
            Some(Value::Text("Late edit".into())),
        )
        .unwrap();
    let delayed_request = delayed.generate().unwrap().unwrap();
    submit(
        &mut doc,
        &mut structural,
        2,
        vec![
            DocumentEdit::Definition(DefinitionEdit::DeleteNode {
                definition: DefinitionId(10),
                node: NodeId(next),
            }),
            DocumentEdit::Hierarchy(HierarchyEdit::Connect {
                object,
                property: PropertyId(4),
                source: None,
            }),
        ],
    );
    let generation = doc.oib_status().unwrap().generation;
    let before = source(&doc);
    assert!(doc.receive(&mut delayed_session, &delayed_request).unwrap());
    assert_eq!(source(&doc), before);
    assert_eq!(doc.oib_status().unwrap().generation, generation);
    assert!(
        !doc.receive(&mut delayed_session, &delayed_request).unwrap(),
        "discard is durably acknowledged"
    );
}

#[test]
fn repeated_explicit_default_restore_recreates_deleted_roles_with_fresh_ids() {
    let (_ui, mut doc) = setup();
    let mut structural = session(&mut doc, 48);
    for sequence in [1, 3] {
        let old = stored_source(&doc);
        let toggle = old
            .baseline
            .iter()
            .find_map(|mapping| match mapping {
                BaselineIdentity::Definition { source, current } if *source == DefinitionId(13) => {
                    Some(*current)
                }
                _ => None,
            })
            .unwrap_or(DefinitionId(13));
        // First remove both the panel reference and the factory that name this role.
        let panel = old
            .definitions
            .iter()
            .find(|d| d.id == DefinitionId(10))
            .unwrap();
        let reference = old
            .baseline
            .iter()
            .find_map(|mapping| match mapping {
                BaselineIdentity::Node {
                    definition,
                    source,
                    current,
                } if *definition == DefinitionId(10) && *source == NodeId(8) => Some(*current),
                _ => None,
            })
            .unwrap_or(NodeId(8));
        let mut exposed = panel.exposed[0].targets.clone();
        exposed.retain(|path| path.references != [reference]);
        let mut button_visual = panel
            .nodes
            .iter()
            .find(|node| node.id == NodeId(9))
            .unwrap()
            .visual
            .clone()
            .unwrap();
        button_visual.action = None;
        button_visual.semantics = None;
        let power = old
            .baseline
            .iter()
            .find_map(|mapping| match mapping {
                BaselineIdentity::Preset {
                    source: 50,
                    current,
                } => Some(*current),
                _ => None,
            })
            .unwrap_or(50);
        submit(
            &mut doc,
            &mut structural,
            sequence,
            vec![
                DocumentEdit::Definition(DefinitionEdit::Expose {
                    definition: DefinitionId(10),
                    property: PropertyId(1),
                    targets: exposed,
                }),
                DocumentEdit::Definition(DefinitionEdit::SetVisual {
                    definition: DefinitionId(10),
                    node: NodeId(9),
                    visual: Some(button_visual),
                }),
                DocumentEdit::Definition(DefinitionEdit::DeleteNode {
                    definition: DefinitionId(10),
                    node: reference,
                }),
                DocumentEdit::Definition(DefinitionEdit::DeleteDefinition { definition: toggle }),
                // Panel presets contain app assignments to the deleted reference.
                DocumentEdit::Preset(PresetEdit::Update {
                    preset: {
                        let mut p = old.presets.iter().find(|p| p.id == 20).unwrap().clone();
                        p.assignments.retain(|a| a.target.references != [reference]);
                        p
                    },
                }),
                DocumentEdit::Preset(PresetEdit::Update {
                    preset: {
                        let mut p = old.presets.iter().find(|p| p.id == 30).unwrap().clone();
                        p.assignments.retain(|a| a.target.references != [reference]);
                        p
                    },
                }),
                DocumentEdit::Preset(PresetEdit::Delete { preset: power }),
            ],
        );
        let highwater = next_id(&doc);
        submit(
            &mut doc,
            &mut structural,
            sequence + 1,
            vec![DocumentEdit::Restore { defaults: true }],
        );
        let restored = stored_source(&doc);
        let replacement = restored
            .baseline
            .iter()
            .find_map(|mapping| match mapping {
                BaselineIdentity::Definition { source, current } if *source == DefinitionId(13) => {
                    Some(*current)
                }
                _ => None,
            })
            .unwrap();
        assert!(replacement.0 >= highwater);
        assert_ne!(replacement, toggle);
        assert_eq!(
            doc.oib_allocations()
                .unwrap()
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![20, 30],
            "surviving real roots keep their identities"
        );
    }
}

#[test]
fn reset_instances_uses_exact_baseline_roles_and_allocates_only_missing_real_roots() {
    let (_ui, mut doc) = setup();
    let mut structural = session(&mut doc, 49);
    for sequence in [1, 3] {
        let source = stored_source(&doc);
        let audio = source
            .baseline
            .iter()
            .find_map(|mapping| match mapping {
                BaselineIdentity::Instance { source, current } if *source == InstanceId(20) => {
                    Some(*current)
                }
                _ => None,
            })
            .unwrap_or(InstanceId(20));
        submit(
            &mut doc,
            &mut structural,
            sequence,
            vec![DocumentEdit::Hierarchy(HierarchyEdit::Remove {
                instance: audio,
            })],
        );
        let highwater = next_id(&doc);
        submit(
            &mut doc,
            &mut structural,
            sequence + 1,
            vec![DocumentEdit::Restore { defaults: false }],
        );
        let allocations = doc.oib_allocations().unwrap();
        assert_eq!(
            allocations.iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![highwater, 30]
        );
        assert_eq!(
            next_id(&doc),
            highwater + 1,
            "restoring instances consumes no phantom definition or node identities"
        );
        assert_eq!(stored_source(&doc).definitions, source.definitions);
    }
}

#[test]
fn reset_all_clears_new_definition_overrides_even_when_source_and_roster_are_already_restored() {
    let (ui, mut doc) = setup();
    let mut structural = session(&mut doc, 50);
    let address = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    submit(
        &mut doc,
        &mut structural,
        1,
        vec![DocumentEdit::Restore { defaults: true }],
    );
    literal(&ui, &mut doc, 51, &address, Some(Value::Number(360.)));
    assert_eq!(property_value(&doc, &address), Value::Number(360.));
    submit(
        &mut doc,
        &mut structural,
        2,
        vec![DocumentEdit::Restore { defaults: true }],
    );
    assert_eq!(property_value(&doc, &address), Value::Number(440.));
    let compiled = doc.oib_compiled(2).unwrap().0;
    let id = oib::document::cell_for_address(
        oib::seed(&compiled).unwrap(),
        &doc.oib_allocations().unwrap(),
        &address,
    )
    .unwrap();
    let leaf = doc.oib_property_leaf(1, id).unwrap();
    assert!(
        leaf.register()
            .unwrap()
            .values()
            .all(|(_, _, value)| value.is_none())
    );
}

#[test]
fn semantic_admission_keeps_epoch_stable_across_compatible_baselines_but_configure_cannot_replace_one()
 {
    let ui = app::ui(&[]);
    let mut changed = ui.clone();
    let Some(otrios_interfaces::canvas::Presentation::Oib(seed)) = &mut changed.presentation else {
        panic!("OIB fixture")
    };
    let graph = seed.graph.as_mut().unwrap();
    let slider = graph
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(12))
        .unwrap();
    slider.name = "Updated shared slider".into();
    slider
        .nodes
        .iter_mut()
        .find(|n| n.id == NodeId(1))
        .unwrap()
        .defaults
        .iter_mut()
        .find(|p| p.property == PropertyId(2))
        .unwrap()
        .value = Value::Number(400.);
    let base = Document::new(ui.state_schema.clone(), "d2:compatible-admission").unwrap();
    let mut first = base.clone();
    let mut second = base;
    first.configure_oib(&ui).unwrap();
    second.configure_oib(&changed).unwrap();
    assert_eq!(
        first.oib_epoch().unwrap(),
        second.oib_epoch().unwrap(),
        "semantic admission excludes compatible graph body/default changes"
    );
    let before = first.save().unwrap();
    assert!(
        first.configure_oib(&changed).is_err(),
        "baseline adoption requires an explicit future operation"
    );
    assert_eq!(
        first.save().unwrap(),
        before,
        "configure must not clobber a host collection"
    );
}

#[test]
fn live_readonly_projection_omits_draft_but_cannot_be_loaded_as_a_complete_checkpoint() {
    let (ui, mut document) = setup();
    let (mut session, _) = document.open_session(&[70; 16], 0).unwrap();
    session.set_oib_editable(false);
    session
        .set_selection(StateSelection::from_ui(&ui).unwrap())
        .unwrap();
    let mount = document.generate(&mut session).unwrap().unwrap();
    let state = StateAdapter::for_ui(&ui, "d2:batch", &mount, [70; 16]).unwrap();
    assert_eq!(state.oib_active().unwrap(), document.oib_values(2).unwrap());
    let otrios_interfaces::canvas::StateResponse::Mount {
        authority,
        lease,
        frame,
    } = canvas::decode_state_response(&mount).unwrap()
    else {
        panic!("mount")
    };
    let mut replica = otrios_core::merkle::Replica::for_lease(authority, lease, vec![]).unwrap();
    replica.apply(&frame).unwrap();
    let projected = replica.snapshot().unwrap();
    assert!(matches!(
        projected.subtree(&[4, 1]),
        Err(otrios_core::merkle::Error::Missing)
    ));
    assert!(matches!(
        projected.subtree(&[4, 5]),
        Err(otrios_core::merkle::Error::Missing)
    ));
    assert!(
        Document::load(
            ui.state_schema.clone(),
            "d2:batch",
            &projected.to_bytes().unwrap()
        )
        .is_err(),
        "a projected view is not a complete durable authority checkpoint"
    );
    let full = Snapshot::from_bytes(&document.save().unwrap()).unwrap();
    let coverage = otrios_core::merkle::Coverage::new(
        &[vec![]],
        &[
            vec![1],
            vec![2],
            vec![3],
            vec![4, 0],
            vec![4, 1],
            vec![4, 2],
            vec![4, 3],
            vec![4, 4],
            vec![4, 6],
        ],
    )
    .unwrap();
    let missing_presence = full.project(&coverage).unwrap().0;
    assert!(
        Document::load(
            ui.state_schema.clone(),
            "d2:batch",
            &missing_presence.to_bytes().unwrap()
        )
        .is_err(),
        "a full checkpoint requires draft presence even when all property registers remain"
    );
}

#[test]
fn replay_preparation_preserves_exact_and_compatible_chrome_roots_and_unseeded_admission_chain() {
    let (ui, mut document) = setup();
    let mut writer = session(&mut document, 71);
    let checkpoint = document.clone();
    submit(
        &mut document,
        &mut writer,
        1,
        vec![DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
            definition: DefinitionId(10),
            name: "Journal-authored component".into(),
        })],
    );
    let journal = document.journal_delta(&checkpoint).unwrap();
    let mut chrome = ui.clone();
    chrome.title = "Compatible updated host chrome".into();
    for installed in [&ui, &chrome] {
        let mut recovered = Document::load(
            ui.state_schema.clone(),
            "d2:batch",
            &checkpoint.save().unwrap(),
        )
        .unwrap();
        let before = recovered.save().unwrap();
        recovered.prepare_oib_replay(installed).unwrap();
        assert_eq!(
            recovered.save().unwrap(),
            before,
            "cache preparation cannot change the checkpoint/journal base"
        );
        recovered.replay(&journal).unwrap();
        assert_eq!(recovered.root_hash(), document.root_hash());
        assert!(
            recovered.replay(&journal).is_err(),
            "journal root chaining remains exact"
        );
        assert_eq!(
            recovered.configure_oib(installed).unwrap(),
            installed.title != ui.title
        );
        assert_eq!(
            recovered.oib_values(2).unwrap(),
            document.oib_values(2).unwrap()
        );
        for field in 1..=4 {
            assert_eq!(
                recovered.typed_value(field).unwrap(),
                document.typed_value(field).unwrap()
            );
        }
    }
    let empty = Document::new(ui.state_schema.clone(), "d2:unseeded-replay").unwrap();
    let mut seeded = empty.clone();
    assert!(seeded.configure_oib(&ui).unwrap());
    let admission = seeded.journal_delta(&empty).unwrap();
    let mut recovered = Document::load(
        ui.state_schema.clone(),
        "d2:unseeded-replay",
        &empty.save().unwrap(),
    )
    .unwrap();
    let before = recovered.save().unwrap();
    recovered.prepare_oib_replay(&ui).unwrap();
    assert_eq!(
        recovered.save().unwrap(),
        before,
        "preparation cannot seed an unseeded checkpoint"
    );
    recovered.replay(&admission).unwrap();
    assert_eq!(recovered.root_hash(), seeded.root_hash());
    assert!(!recovered.configure_oib(&ui).unwrap());
}
