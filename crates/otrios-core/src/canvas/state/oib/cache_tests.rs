// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::test_support::oib_designer as app;

fn cached_source_document() -> Document {
    let ui = app::ui(&[]);
    let mut document = Document::new(ui.state_schema.clone(), "d2:source-cache").unwrap();
    document.configure_oib(&ui).unwrap();
    document
}

fn replace_source_leaf(document: &mut Document, branch: u64, value: Vec<u8>) {
    document.tree = document
        .tree
        .edit(
            [93; 16],
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
fn validated_source_cache_reuses_unchanged_leaf_and_isolates_sibling_snapshots() {
    let document = cached_source_document();
    let saved = document.save().unwrap();
    let first = document.oib_source_shared().unwrap();
    let first_ui = document.oib_authored_ui().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &document.oib_source_shared().unwrap()
    ));
    let mut sibling = document.clone();
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &sibling.oib_source_shared().unwrap()
    ));

    let mut status = sibling.oib_status().unwrap();
    status.error = Some("A diagnostic without source changes".into());
    replace_source_leaf(&mut sibling, 3, bytes(&status).unwrap());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &sibling.oib_source_shared().unwrap()
    ));

    let mut source = first.as_ref().clone();
    source.next_identity += 1;
    replace_source_leaf(&mut sibling, 4, bytes(&source).unwrap());
    assert!(!std::sync::Arc::ptr_eq(
        &first,
        &sibling.oib_source_shared().unwrap()
    ));
    assert!(std::sync::Arc::ptr_eq(
        &first_ui,
        &sibling.oib_authored_ui().unwrap()
    ));
    source.definitions[0].name = "Changed sibling definition".into();
    replace_source_leaf(&mut sibling, 4, bytes(&source).unwrap());
    let changed = sibling.oib_source_shared().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.definitions[0].name, "Changed sibling definition");
    assert_eq!(document.save().unwrap(), saved);
    assert_eq!(
        document.oib_source_shared().unwrap().definitions[0].name,
        first.definitions[0].name
    );

    let mut changed_ui = document.clone();
    let previous = changed_ui.oib_source_shared().unwrap();
    changed_ui.oib_ui = Some(std::sync::Arc::new(
        changed_ui.oib_ui.as_ref().unwrap().as_ref().clone(),
    ));
    assert!(!std::sync::Arc::ptr_eq(
        &previous,
        &changed_ui.oib_source_shared().unwrap()
    ));
    assert_eq!(changed_ui.save().unwrap(), saved);
}

#[test]
fn validated_source_and_authored_ui_caches_recheck_changed_baseline_and_invalid_source() {
    let document = cached_source_document();
    let saved = document.save().unwrap();
    let original_source = document.oib_source_shared().unwrap();
    let original_ui = document.oib_authored_ui().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &original_ui,
        &document.oib_authored_ui().unwrap()
    ));
    let mut sibling = document.clone();
    let mut binding: Binding = decode(&payload(&sibling.tree, &[4, 0]).unwrap()).unwrap();
    binding.seed.graph.as_mut().unwrap().name = "Changed source validation baseline".into();
    replace_source_leaf(&mut sibling, 0, bytes(&binding).unwrap());
    let changed_source = sibling.oib_source_shared().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&original_source, &changed_source));
    assert!(!std::sync::Arc::ptr_eq(
        &original_ui,
        &sibling.oib_authored_ui().unwrap()
    ));

    replace_source_leaf(&mut sibling, 0, vec![255, 0, 7]);
    assert!(sibling.oib_source_shared().is_err());
    assert!(sibling.oib_authored_ui().is_err());

    let mut malformed = document.clone();
    replace_source_leaf(&mut malformed, 4, vec![255, 0, 7]);
    assert!(malformed.oib_source_shared().is_err());
    assert!(malformed.oib_authored_ui().is_err());

    let mut noncanonical = document.clone();
    let mut source = original_source.as_ref().clone();
    source.definitions.clear();
    replace_source_leaf(&mut noncanonical, 4, bytes(&source).unwrap());
    assert!(noncanonical.oib_source_shared().is_err());
    assert!(noncanonical.oib_authored_ui().is_err());
    assert_eq!(document.save().unwrap(), saved);
    assert!(document.oib_source_shared().is_ok());
    assert!(document.oib_authored_ui().is_ok());
}

#[test]
fn normalized_binding_cache_keys_payload_and_exact_admitted_ui_without_changing_root_or_wire() {
    assert_eq!(app::manifest().id, "oib-designer");
    let ui = app::ui(&[]);
    let mut document = Document::new(ui.state_schema.clone(), "d2:binding-cache").unwrap();
    document.configure_oib(&ui).unwrap();
    let saved = document.save().unwrap();
    let first = document.oib_binding().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &document.oib_binding().unwrap()
    ));
    assert!(
        std::sync::Arc::ptr_eq(&first, &document.clone().oib_binding().unwrap()),
        "clones share one bounded cache entry"
    );
    let mut changed_ui = document.clone();
    changed_ui.oib_ui = Some(std::sync::Arc::new(
        changed_ui.oib_ui.as_ref().unwrap().as_ref().clone(),
    ));
    let second = changed_ui.oib_binding().unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&first, &second),
        "equal UI content with a different admitted Arc is a new key"
    );
    assert_eq!(changed_ui.save().unwrap(), saved);
    changed_ui.oib_ui = None;
    let pure = changed_ui.oib_binding().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&pure, &second));
    assert!(std::sync::Arc::ptr_eq(
        &pure,
        &changed_ui.oib_binding().unwrap()
    ));
    assert_eq!(changed_ui.save().unwrap(), saved);

    let mut candidate = document.clone();
    let mut replacement: Binding = decode(&payload(&candidate.tree, &[4, 0]).unwrap()).unwrap();
    replacement.seed.graph.as_mut().unwrap().name = "Changed baseline source".into();
    candidate.tree = candidate
        .tree
        .edit(
            [92; 16],
            &[Edit {
                path: vec![4, 0],
                schema: candidate.extra_schema(4, 0),
                value: Some(bytes(&replacement).unwrap()),
            }],
        )
        .unwrap()
        .0;
    let changed = candidate.oib_binding().unwrap();
    assert_eq!(
        changed.seed.graph.as_ref().unwrap().name,
        "Changed baseline source"
    );
    assert!(!std::sync::Arc::ptr_eq(&first, &changed));
    assert!(
        candidate.configure_oib(&ui).is_err(),
        "caching cannot bypass baseline admission"
    );
    candidate.tree = candidate
        .tree
        .edit(
            [92; 16],
            &[Edit {
                path: vec![4, 0],
                schema: candidate.extra_schema(4, 0),
                value: Some(vec![255, 0, 7]),
            }],
        )
        .unwrap()
        .0;
    assert!(
        candidate.oib_binding().is_err(),
        "a changed malformed payload must run canonical validation"
    );
    assert_eq!(
        document.save().unwrap(),
        saved,
        "cache replacement in a sibling never mutates a root or wire byte"
    );
    assert_eq!(
        document
            .oib_binding()
            .unwrap()
            .seed
            .graph
            .as_ref()
            .unwrap()
            .name,
        first.seed.graph.as_ref().unwrap().name
    );
}
