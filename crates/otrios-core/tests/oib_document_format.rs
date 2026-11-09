// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;

use otrios_core::canvas::{self, Document, StateAdapter, StateSelection, oib};
use otrios_interfaces::{
    canvas::*,
    layout::Value,
    oib::{InstanceOperation, designer::*},
};

fn document(ui: &Ui) -> Document {
    assert_eq!(app::manifest().id, "oib-designer");
    let mut doc = Document::new(ui.state_schema.clone(), "oib:format").unwrap();
    doc.configure_oib(ui).unwrap();
    doc
}
fn address() -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    }
}
fn client(ui: &Ui, doc: &mut Document) -> (StateAdapter, canvas::Session) {
    let (mut session, _) = doc.open_session(&[67; 16], 0).unwrap();
    session.set_oib_editable(true);
    session
        .set_selection(StateSelection::from_ui(ui).unwrap())
        .unwrap();
    let mut client = StateAdapter::for_ui(ui, "oib:format", &[], [67; 16]).unwrap();
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
    panic!("format client did not settle");
}

#[test]
fn register_keys_and_schemas_ignore_all_compiler_cell_handles() {
    let ui = app::ui(&[]);
    let original = document(&ui);
    let mut revised = ui.clone();
    let Some(Presentation::Oib(seed)) = &mut revised.presentation else {
        unreachable!()
    };
    for cell in &mut seed.cells {
        cell.id += 10_000;
        cell.inherit = cell.inherit.map(|id| id + 10_000);
    }
    let metadata = seed.designer.as_mut().unwrap();
    for row in &mut metadata.mappings.definitions {
        row.cell += 10_000;
    }
    for row in metadata
        .mappings
        .roots
        .iter_mut()
        .flat_map(|table| &mut table.cells)
    {
        row.initial_cell += 10_000;
    }
    let revised = document(&revised);
    for branch in [1, 2] {
        for cell in original.oib_values(branch).unwrap().keys() {
            let a = original.oib_property_leaf(branch, *cell).unwrap();
            let b = revised.oib_property_leaf(branch, cell + 10_000).unwrap();
            assert_eq!(
                a.register().unwrap().schema(),
                b.register().unwrap().schema()
            );
            assert_eq!(
                a.register()
                    .unwrap()
                    .values()
                    .map(|(_, _, value)| value)
                    .collect::<Vec<_>>(),
                b.register()
                    .unwrap()
                    .values()
                    .map(|(_, _, value)| value)
                    .collect::<Vec<_>>()
            );
        }
    }
    let snapshot = otrios_core::merkle::Snapshot::from_bytes(&original.save().unwrap()).unwrap();
    assert_eq!(
        snapshot.subtree(&[4, 1, 1, 20, 1, 7, 1, 3]).unwrap().hash(),
        original.oib_property_leaf(1, 203).unwrap().hash()
    );
    assert_eq!(
        snapshot
            .subtree(&[4])
            .unwrap()
            .child_keys()
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5, 6]
    );
}

#[test]
fn property_wire_intents_carry_the_observed_semantic_address_and_epoch() {
    let ui = app::ui(&[]);
    let mut doc = document(&ui);
    let (mut client, mut session) = client(&ui, &mut doc);
    client
        .edit_oib_property(&address(), Some(Value::Text("Semantic label".into())))
        .unwrap();
    let request = client.generate().unwrap().unwrap();
    let StateRequest::OibWrite { epoch, changes, .. } =
        canvas::decode_state_request(&request).unwrap()
    else {
        panic!("property intent")
    };
    assert_eq!(epoch, doc.oib_epoch().unwrap());
    assert_eq!(changes[0].address, address());
    assert!(doc.receive(&mut session, &request).unwrap());
    let accepted = doc.root_hash();
    assert!(!doc.receive(&mut session, &request).unwrap());
    assert_eq!(doc.root_hash(), accepted);
}

#[test]
fn a_fresh_document_rejects_old_epoch_before_any_receipt_or_allocation_changes() {
    let ui = app::ui(&[]);
    let previous = document(&ui);
    let mut fresh = document(&ui);
    let (_, mut session) = client(&ui, &mut fresh);
    let operation = InstanceOperation::Insert {
        prototype: 50,
        before: None,
        generation: fresh.oib_status().unwrap().generation,
    };
    let stale = canvas::encode_state_request(&StateRequest::OibInstance {
        epoch: previous.oib_epoch().unwrap(),
        sequence: 1,
        operation: operation.clone(),
    })
    .unwrap();
    let before = fresh.save().unwrap();
    assert!(
        fresh
            .receive(&mut session, &stale)
            .unwrap_err()
            .contains("document changed")
    );
    assert_eq!(fresh.save().unwrap(), before);
    let current = canvas::encode_state_request(&StateRequest::OibInstance {
        epoch: fresh.oib_epoch().unwrap(),
        sequence: 1,
        operation,
    })
    .unwrap();
    assert!(fresh.receive(&mut session, &current).unwrap());
    let accepted = fresh.save().unwrap();
    assert!(!fresh.receive(&mut session, &current).unwrap());
    assert!(fresh.receive(&mut session, &stale).is_err());
    assert_eq!(fresh.save().unwrap(), accepted);
    let compiled = fresh.oib_compiled(2).unwrap().0;
    assert!(
        oib::seed(&compiled)
            .unwrap()
            .instances
            .iter()
            .any(|i| i.id == 51)
    );
}

#[test]
fn restoring_ram_intents_never_rebases_them_onto_a_fresh_document_epoch() {
    let ui = app::ui(&[]);
    let mut previous = document(&ui);
    let (mut old, _) = client(&ui, &mut previous);
    old.edit_oib_property(&address(), Some(Value::Text("Discarded old edit".into())))
        .unwrap();
    let old_request = old.generate().unwrap().unwrap();
    let mut fresh = document(&ui);
    let (mut current, mut session) = client(&ui, &mut fresh);
    current.restore_pending(&old).unwrap();
    assert!(current.confirmed(), "old OIB RAM intent is discarded");
    assert!(current.generate().unwrap().is_none());
    assert!(fresh.receive(&mut session, &old_request).is_err());
    current
        .edit_oib_property(&address(), Some(Value::Text("New explicit edit".into())))
        .unwrap();
    let request = current.generate().unwrap().unwrap();
    let StateRequest::OibWrite {
        sequence, epoch, ..
    } = canvas::decode_state_request(&request).unwrap()
    else {
        panic!("new property edit")
    };
    assert_eq!(sequence, 1);
    assert_eq!(epoch, fresh.oib_epoch().unwrap());
    assert!(fresh.receive(&mut session, &request).unwrap());
}

#[test]
fn obsolete_format_and_signed_source_rebases_require_fresh_initialization() {
    let mut ui = app::ui(&[]);
    let mut doc = document(&ui);
    let epoch = doc.oib_epoch().unwrap();
    ui.title = "Presentation update".into();
    assert!(doc.configure_oib(&ui).unwrap());
    assert_eq!(doc.oib_epoch().unwrap(), epoch);
    let before = doc.save().unwrap();
    let mut reconnected = ui.clone();
    for selector in &mut reconnected.selectors {
        if selector.field == 1 {
            selector.field = 2;
        }
    }
    for handler in &mut reconnected.handlers {
        for field in &mut handler.fields {
            if *field == 1 {
                *field = 2;
            }
        }
    }
    oib::validate(&reconnected).unwrap();
    assert!(
        doc.configure_oib(&reconnected)
            .unwrap_err()
            .contains("fresh collection")
    );
    assert_eq!(doc.save().unwrap(), before);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed.designer.as_mut().unwrap().name = "Different signed document".into();
    assert!(
        doc.configure_oib(&ui)
            .unwrap_err()
            .contains("fresh collection")
    );
    assert_eq!(doc.save().unwrap(), before);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed.format = "otrios-oib-v1".into();
    assert!(oib::validate(&ui).is_err());
}
