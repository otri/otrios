// SPDX-License-Identifier: LGPL-3.0-only
//! Runtime adoption over synthetic already-verified library dependency bodies.
//! Independent package tests establish signatures, trust and package-root checks.
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "support/oib_library_fixture.rs"]
mod fixture;
use otrios_core::{
    canvas::{self, Document, StateAdapter, StateSelection, oib},
    merkle::{Edit, Snapshot},
};
use otrios_interfaces::{
    canvas::{StateRequest, Ui},
    layout::Value,
    oib::{Seed, authoring::*, designer::*},
};

fn setup() -> (Ui, Document) {
    let ui = fixture::admit(&fixture::imported());
    let mut doc = Document::new(ui.state_schema.clone(), "l1:adoption").unwrap();
    doc.configure_oib(&ui).unwrap();
    (ui, doc)
}
fn styled(ui: &Ui) -> Ui {
    fixture::admit(&fixture::update(ui, |graph| {
        graph
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(12))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|d| d.property == PropertyId(2))
            .unwrap()
            .value = Value::Number(400.);
        graph
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(13))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|d| d.property == PropertyId(3))
            .unwrap()
            .value = Value::Text("New toggle default".into());
    }))
}
fn client(ui: &Ui, doc: &mut Document, writer: u8) -> (StateAdapter, canvas::Session) {
    let (mut session, _) = doc.open_session(&[writer; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut state = StateAdapter::for_ui(ui, "l1:adoption", &[], [writer; 16]).unwrap();
    state
        .receive(&doc.generate(&mut session).unwrap().unwrap())
        .unwrap();
    sync(&mut state, doc, &mut session);
    (state, session)
}
fn sync(state: &mut StateAdapter, doc: &mut Document, session: &mut canvas::Session) {
    for _ in 0..30 {
        let mut progress = false;
        while let Some(bytes) = state.generate().unwrap() {
            doc.receive(session, &bytes).unwrap();
            progress = true;
        }
        if let Some(bytes) = doc.generate(session).unwrap() {
            state.receive(&bytes).unwrap();
            progress = true;
        }
        if !progress {
            return;
        }
    }
    panic!("sync did not settle");
}
fn definition(property: u64) -> PropertyAddress {
    PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(property),
    }
}
fn occurrence(instance: u64, references: &[u64], property: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: references.iter().copied().map(NodeId).collect(),
            node: NodeId(1),
            property: PropertyId(property),
        },
    }
}
fn effective(doc: &Document, address: &PropertyAddress) -> Value {
    let (ui, _) = doc.oib_compiled(2).unwrap();
    let seed = oib::seed(&ui).unwrap();
    let cell =
        oib::document::cell_for_address(seed, &doc.oib_allocations().unwrap(), address).unwrap();
    oib::effective(seed, &doc.oib_values(2).unwrap(), cell).unwrap()
}
fn tree(doc: &Document) -> Snapshot {
    Snapshot::from_bytes(&doc.save().unwrap()).unwrap()
}
fn source(doc: &Document) -> DocumentSource {
    let leaf = tree(doc).subtree(&[4, 4]).unwrap();
    flexbuffers::from_slice(leaf.register().unwrap().values().next().unwrap().2.unwrap()).unwrap()
}

#[test]
fn compatible_adoption_preserves_local_source_registers_receipts_epoch_and_old_snapshot() {
    let (ui, mut doc) = setup();
    let (new_ui, epoch) = (styled(&ui), doc.oib_epoch().unwrap());
    let (mut state, mut session) = client(&ui, &mut doc, 81);
    state
        .edit_oib_property(&definition(2), Some(Value::Number(360.)))
        .unwrap();
    sync(&mut state, &mut doc, &mut session);
    state
        .edit_oib_property(
            &occurrence(20, &[8], 3),
            Some(Value::Text("My toggle".into())),
        )
        .unwrap();
    sync(&mut state, &mut doc, &mut session);
    state
        .edit_oib_structure(EditBatch::definitions(DefinitionBatch {
            generation: state.oib_structural_generation().unwrap(),
            operations: vec![DefinitionEdit::RenameDefinition {
                definition: DefinitionId(10),
                name: "My local wrapper".into(),
            }],
        }))
        .unwrap();
    sync(&mut state, &mut doc, &mut session);
    let previous = doc.clone();
    let saved = doc.save().unwrap();
    let old_tree = tree(&doc);
    let old_layout = doc.oib_compiled(2).unwrap().0.layout;
    let (generation, lock) = doc.oib_baseline().unwrap();
    assert!(doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap());
    assert_eq!(doc.oib_epoch().unwrap(), epoch);
    assert_eq!(doc.oib_baseline().unwrap().0, generation + 1);
    assert_ne!(doc.oib_baseline().unwrap().1, lock);
    for path in [
        vec![1],
        vec![2],
        vec![3],
        vec![4, 1],
        vec![4, 2],
        vec![4, 4],
        vec![4, 5],
        vec![4, 6],
    ] {
        match (old_tree.subtree(&path), tree(&doc).subtree(&path)) {
            (Ok(old), Ok(new)) => assert_eq!(
                old.to_bytes().unwrap(),
                new.to_bytes().unwrap(),
                "preserve {path:?}"
            ),
            (Err(_), Err(_)) => {}
            _ => panic!("changed branch {path:?}"),
        }
    }
    assert_eq!(
        effective(&doc, &occurrence(30, &[7], 2)),
        Value::Number(360.)
    );
    assert_eq!(
        effective(&doc, &occurrence(20, &[8], 3)),
        Value::Text("My toggle".into())
    );
    assert_eq!(
        effective(&doc, &occurrence(30, &[8], 3)),
        Value::Text("New toggle default".into())
    );
    assert_eq!(previous.save().unwrap(), saved);
    assert_eq!(previous.oib_compiled(2).unwrap().0.layout, old_layout);
    assert_eq!(
        source(&doc)
            .definitions
            .iter()
            .map(|d| d.id.0)
            .collect::<Vec<_>>(),
        [10, 11]
    );
}

#[test]
fn queued_old_epoch_literal_still_applies_and_old_structural_generation_is_rejected() {
    let (ui, mut doc) = setup();
    let (new_ui, epoch) = (styled(&ui), doc.oib_epoch().unwrap());
    let (mut state, mut session) = client(&ui, &mut doc, 82);
    state
        .edit_oib_property(&definition(2), Some(Value::Number(420.)))
        .unwrap();
    let pending = state.generate().unwrap().unwrap();
    let (generation, lock) = doc.oib_baseline().unwrap();
    doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap();
    assert_eq!(
        effective(&doc, &occurrence(20, &[7], 2)),
        Value::Number(400.)
    );
    doc.receive(&mut session, &pending).unwrap();
    assert_eq!(doc.oib_epoch().unwrap(), epoch);
    assert_eq!(
        effective(&doc, &occurrence(20, &[7], 2)),
        Value::Number(420.)
    );
    let before = source(&doc);
    let request = canvas::encode_state_request(&StateRequest::OibInstance {
        epoch,
        sequence: 2,
        operation: EditBatch::insert(generation, 30, None, None),
    })
    .unwrap();
    doc.receive(&mut session, &request).unwrap();
    assert_eq!(source(&doc), before);
    assert!(
        doc.oib_status()
            .unwrap()
            .error
            .unwrap()
            .contains("Document changed")
    );
}

#[test]
fn rejection_and_compatible_rollback_leave_unrelated_state_and_observed_heads_unchanged() {
    let (ui, mut doc) = setup();
    let new_ui = styled(&ui);
    let (generation, lock) = doc.oib_baseline().unwrap();
    let saved = doc.save().unwrap();
    for (generation, lock) in [(generation + 1, lock), (generation, [9; 32])] {
        assert!(doc.adopt_oib_baseline(&new_ui, generation, lock).is_err());
        assert_eq!(doc.save().unwrap(), saved);
    }
    let incompatible = fixture::update(&ui, |graph| {
        graph.definitions[0].nodes[0]
            .visual
            .as_mut()
            .unwrap()
            .action = None
    });
    assert!(
        doc.adopt_oib_baseline(&incompatible, generation, lock)
            .is_err()
    );
    assert_eq!(doc.save().unwrap(), saved);
    doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap();
    let (generation, lock) = doc.oib_baseline().unwrap();
    doc.adopt_oib_baseline(&ui, generation, lock).unwrap();
    assert_eq!(
        effective(&doc, &occurrence(20, &[7], 2)),
        Value::Number(440.)
    );
}

#[test]
fn adoption_replays_exactly_and_missing_transition_witness_rejects() {
    let (ui, mut doc) = setup();
    let new_ui = styled(&ui);
    let checkpoint = doc.clone();
    let (generation, lock) = doc.oib_baseline().unwrap();
    doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap();
    let record = doc.journal_delta(&checkpoint).unwrap();
    let mut recovered = Document::load(
        ui.state_schema.clone(),
        "l1:adoption",
        &checkpoint.save().unwrap(),
    )
    .unwrap();
    let root = recovered.root_hash();
    recovered.prepare_oib_replay(&new_ui).unwrap();
    assert_eq!(recovered.root_hash(), root);
    recovered.replay(&record).unwrap();
    assert_eq!(recovered.root_hash(), doc.root_hash());
    assert!(!recovered.configure_oib(&new_ui).unwrap());
    assert!(recovered.replay(&record).is_err());
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Binding {
        format: String,
        identity_floor: u64,
        adoption: Option<serde_json::Value>,
        seed: Seed,
        admission: Vec<u8>,
        ui_hash: Vec<u8>,
        ui_ref: String,
    }
    let candidate = tree(&doc);
    let leaf = candidate.subtree(&[4, 0]).unwrap();
    let mut binding: Binding =
        flexbuffers::from_slice(leaf.register().unwrap().values().next().unwrap().2.unwrap())
            .unwrap();
    binding.adoption = None;
    let forged = candidate
        .edit(
            [99; 16],
            &[Edit {
                path: vec![4, 0],
                schema: leaf.register().unwrap().schema(),
                value: Some(flexbuffers::to_vec(&binding).unwrap()),
            }],
        )
        .unwrap()
        .0;
    let delta = forged.delta_bytes(&tree(&checkpoint)).unwrap();
    let mut old = checkpoint.clone();
    assert!(old.replay(&delta).is_err());
    assert_eq!(old.root_hash(), checkpoint.root_hash());
}

#[test]
fn compatible_adoption_preserves_unresolved_causal_heads_until_explicit_resolution() {
    let (ui, mut doc) = setup();
    let new_ui = styled(&ui);
    let (mut a, mut sa) = client(&ui, &mut doc, 83);
    let (mut b, mut sb) = client(&ui, &mut doc, 84);
    a.edit_oib_property(&definition(2), Some(Value::Number(360.)))
        .unwrap();
    b.edit_oib_property(&definition(2), Some(Value::Number(420.)))
        .unwrap();
    doc.receive(&mut sa, &a.generate().unwrap().unwrap())
        .unwrap();
    doc.receive(&mut sb, &b.generate().unwrap().unwrap())
        .unwrap();
    assert_eq!(doc.oib_status().unwrap().conflicts, vec![definition(2)]);
    let previous = tree(&doc);
    let old_status = doc.oib_status().unwrap();
    let (generation, lock) = doc.oib_baseline().unwrap();
    doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap();
    assert_eq!(doc.oib_status().unwrap().conflicts, old_status.conflicts);
    assert_eq!(
        doc.oib_status().unwrap().source_draft,
        old_status.source_draft
    );
    for branch in [1, 2] {
        assert_eq!(
            tree(&doc)
                .subtree(&[4, branch])
                .unwrap()
                .to_bytes()
                .unwrap(),
            previous.subtree(&[4, branch]).unwrap().to_bytes().unwrap()
        );
    }
    assert_eq!(
        effective(&doc, &occurrence(20, &[7], 2)),
        Value::Number(360.)
    );
    let (mut current, mut session) = client(&new_ui, &mut doc, 85);
    current.edit_oib_property(&definition(2), None).unwrap();
    sync(&mut current, &mut doc, &mut session);
    assert!(doc.oib_status().unwrap().conflicts.is_empty());
    assert_eq!(
        effective(&doc, &occurrence(20, &[7], 2)),
        Value::Number(400.)
    );
}

#[test]
fn reset_after_adoption_restores_locals_and_current_import_defaults_without_copying_library_bodies()
{
    let (ui, mut doc) = setup();
    assert_eq!(fixture::seed(&ui).resolved_libraries.len(), 1);
    assert!(
        source(&doc).next_identity > 71,
        "import binding identities participate in the authority highwater"
    );
    let new_ui = styled(&ui);
    let (mut state, mut session) = client(&ui, &mut doc, 86);
    state
        .edit_oib_property(&definition(2), Some(Value::Number(360.)))
        .unwrap();
    sync(&mut state, &mut doc, &mut session);
    let (generation, lock) = doc.oib_baseline().unwrap();
    doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap();
    let (mut state, mut session) = client(&new_ui, &mut doc, 87);
    assert!(state.restore_oib(true).unwrap());
    sync(&mut state, &mut doc, &mut session);
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    assert_eq!(
        source(&doc)
            .definitions
            .iter()
            .map(|definition| definition.id.0)
            .collect::<Vec<_>>(),
        [10, 11]
    );
    assert_eq!(
        effective(&doc, &occurrence(20, &[7], 2)),
        Value::Number(400.)
    );
    assert_eq!(
        state
            .oib_property_register(&definition(2))
            .unwrap()
            .alternatives,
        vec![None]
    );
}

#[test]
fn malformed_adoption_metadata_in_a_full_checkpoint_is_rejected_before_recovery() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Adoption {
        format: String,
        generation: u64,
        previous: [u8; 32],
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Binding {
        format: String,
        identity_floor: u64,
        adoption: Option<Adoption>,
        seed: Seed,
        admission: Vec<u8>,
        ui_hash: Vec<u8>,
        ui_ref: String,
    }
    let (ui, mut doc) = setup();
    let new_ui = styled(&ui);
    let (generation, lock) = doc.oib_baseline().unwrap();
    doc.adopt_oib_baseline(&new_ui, generation, lock).unwrap();
    let snapshot = tree(&doc);
    let leaf = snapshot.subtree(&[4, 0]).unwrap();
    let data = leaf.register().unwrap().values().next().unwrap().2.unwrap();
    for fault in 0..3 {
        let mut binding: Binding = flexbuffers::from_slice(data).unwrap();
        let adoption = binding.adoption.as_mut().unwrap();
        match fault {
            0 => adoption.format = "unknown-future-adoption-v99".into(),
            1 => adoption.generation = 0,
            _ => adoption.generation = doc.oib_status().unwrap().structural_generation,
        }
        let forged = snapshot
            .edit(
                [98; 16],
                &[Edit {
                    path: vec![4, 0],
                    schema: leaf.register().unwrap().schema(),
                    value: Some(flexbuffers::to_vec(&binding).unwrap()),
                }],
            )
            .unwrap()
            .0;
        let error = match Document::load(
            ui.state_schema.clone(),
            "l1:adoption",
            &forged.to_bytes().unwrap(),
        ) {
            Ok(_) => panic!("malformed adoption metadata accepted"),
            Err(error) => error,
        };
        assert!(error.contains("baseline adoption"), "{error}");
    }
}
