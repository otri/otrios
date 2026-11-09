// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

#[path = "../../../../tests/support/oib_library_fixture.rs"]
mod fixture;

fn imported_document() -> Document {
    let ui = fixture::admit(&fixture::imported());
    let mut document =
        Document::new(ui.state_schema.clone(), "oib:immutable-import-cache").unwrap();
    document.configure_oib(&ui).unwrap();
    document
}

fn replace_leaf(document: &mut Document, branch: u64, value: Vec<u8>) {
    document.tree = document
        .tree
        .edit(
            [97; 16],
            &[Edit {
                path: vec![4, branch],
                schema: document.extra_schema(4, branch.try_into().unwrap()),
                value: Some(value),
            }],
        )
        .unwrap()
        .0;
}

#[test]
fn immutable_import_cache_matches_resolution_and_reuses_property_source_siblings() {
    let document = imported_document();
    let saved = document.save().unwrap();
    let binding = document.oib_binding().unwrap();
    let first = document.oib_baseline_graph().unwrap();
    let full = super::super::super::oib::imports::resolve(&binding.seed).unwrap();
    assert_eq!(bytes(first.as_ref()).unwrap(), bytes(&full).unwrap());
    assert_eq!(
        document.oib_library_bytes().unwrap(),
        bytes(&binding.seed.resolved_libraries).unwrap().len()
    );
    assert!(first.definitions.len() > binding.seed.graph.as_ref().unwrap().definitions.len());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &document.oib_baseline_graph().unwrap()
    ));

    let mut sibling = document.clone();
    let seed = sibling.oib_resolved_shared().unwrap();
    let cell = seed
        .cells
        .iter()
        .find(|cell| matches!(cell.default, Value::Color(_)))
        .unwrap();
    sibling.tree = sibling
        .tree
        .edit(
            [97; 16],
            &[Edit {
                path: address::cell_path(&seed, 2, cell.id).unwrap(),
                schema: address::schema(&sibling.scope, &seed, cell.id).unwrap(),
                value: Some(bytes(&Value::Color([17, 81, 167, 255])).unwrap()),
            }],
        )
        .unwrap()
        .0;
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &sibling.oib_baseline_graph().unwrap()
    ));

    let mut source = sibling.oib_source().unwrap();
    source.definitions[0].name = "Changed local design".into();
    replace_leaf(&mut sibling, 4, bytes(&source).unwrap());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &sibling.oib_baseline_graph().unwrap()
    ));
    assert_eq!(document.save().unwrap(), saved);
}

#[test]
fn immutable_import_cache_invalidates_binding_and_admitted_ui_without_cross_snapshot_reuse() {
    let document = imported_document();
    let saved = document.save().unwrap();
    let first = document.oib_baseline_graph().unwrap();
    let mut admitted = document.clone();
    admitted.oib_ui = Some(std::sync::Arc::new(
        admitted.oib_ui.as_ref().unwrap().as_ref().clone(),
    ));
    let second = admitted.oib_baseline_graph().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(
        bytes(first.as_ref()).unwrap(),
        bytes(second.as_ref()).unwrap()
    );

    let mut changed = document.clone();
    let mut replacement: Binding = decode(&payload(&changed.tree, &[4, 0]).unwrap()).unwrap();
    replacement.seed.graph.as_mut().unwrap().name = "Changed signed baseline".into();
    replace_leaf(&mut changed, 0, bytes(&replacement).unwrap());
    let current = changed.oib_baseline_graph().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &current));
    assert_eq!(current.name, "Changed signed baseline");
    let full =
        super::super::super::oib::imports::resolve(&changed.oib_binding().unwrap().seed).unwrap();
    assert_eq!(bytes(current.as_ref()).unwrap(), bytes(&full).unwrap());
    assert_eq!(
        bytes(first.as_ref()).unwrap(),
        bytes(document.oib_baseline_graph().unwrap().as_ref()).unwrap()
    );
    assert_ne!(first.name, current.name);

    let ui = fixture::admit(&fixture::update(
        document.oib_ui.as_ref().unwrap(),
        |library| {
            library.definitions[0].name = "Updated library control".into();
        },
    ));
    let mut revision = document.clone();
    let mut replacement: Binding = decode(&payload(&revision.tree, &[4, 0]).unwrap()).unwrap();
    replacement.seed = fixture::seed(&ui).clone();
    replace_leaf(&mut revision, 0, bytes(&replacement).unwrap());
    let updated = revision.oib_baseline_graph().unwrap();
    assert!(
        updated
            .definitions
            .iter()
            .any(|definition| definition.name == "Updated library control")
    );
    assert!(
        !first
            .definitions
            .iter()
            .any(|definition| definition.name == "Updated library control")
    );
    assert_eq!(document.save().unwrap(), saved);
}

#[test]
fn warmed_import_cache_does_not_admit_changed_malformed_closure_contract_or_class() {
    let document = imported_document();
    let saved = document.save().unwrap();
    let original = document.oib_baseline_graph().unwrap();
    for bad in 0..4 {
        let mut candidate = document.clone();
        let mut replacement: Binding = decode(&payload(&candidate.tree, &[4, 0]).unwrap()).unwrap();
        match bad {
            0 => replacement.seed.resolved_libraries[0].revision = [213; 32],
            1 => replacement.seed.resolved_libraries[0].graph.exports[0].contract = [213; 32],
            2 => replacement
                .seed
                .graph
                .as_mut()
                .unwrap()
                .imports
                .as_mut()
                .unwrap()
                .bindings[0]
                .definitions
                .clear(),
            3 => replacement.seed.graph.as_mut().unwrap().classes.clear(),
            _ => unreachable!(),
        }
        replace_leaf(&mut candidate, 0, bytes(&replacement).unwrap());
        assert!(
            candidate.oib_baseline_graph().is_err(),
            "invalid immutable input {bad}"
        );
        assert!(
            candidate.oib_library_bytes().is_err(),
            "invalid bytes cannot reuse a size admission"
        );
        assert_eq!(
            bytes(original.as_ref()).unwrap(),
            bytes(document.oib_baseline_graph().unwrap().as_ref()).unwrap()
        );
        assert_eq!(document.save().unwrap(), saved);
    }
}

#[test]
fn current_property_needs_no_retired_baseline_resolution() {
    let document = imported_document();
    let entry = document.oib_binding_entry().unwrap();
    assert!(entry.resolved.lock().unwrap().is_none());
    let seed = document.oib_resolved_shared().unwrap();
    for cell in &seed.cells {
        let address = super::super::super::oib::document::address_for_cell(&seed, cell.id).unwrap();
        assert!(!document.oib_retired_property(&address).unwrap());
    }
    assert!(
        entry.resolved.lock().unwrap().is_none(),
        "current metadata answers live property checks without reconstructing retired baseline roles"
    );
}
