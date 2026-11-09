// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::test_support::oib_designer as app;

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
