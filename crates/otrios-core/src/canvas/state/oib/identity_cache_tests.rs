// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
#[allow(dead_code)]
#[path = "../../../../tests/support/oib_library_fixture.rs"]
mod fixture;
#[test]
fn configuring_an_unchanged_admitted_package_keeps_snapshot_cache_identity() {
    let ui = fixture::admit(&fixture::imported());
    let mut document = Document::new(ui.state_schema.clone(), "oib:admitted-identity").unwrap();
    document.configure_oib(&ui).unwrap();
    let saved = document.save().unwrap();
    let admitted = document.oib_ui.as_ref().unwrap().clone();
    let binding = document.oib_binding().unwrap();
    let metadata = document.oib_resolved_shared().unwrap();
    for _ in 0..3 {
        let mut candidate = document.clone();
        assert!(!candidate.configure_oib(&ui).unwrap());
        assert!(std::sync::Arc::ptr_eq(
            &admitted,
            candidate.oib_ui.as_ref().unwrap()
        ));
        assert!(std::sync::Arc::ptr_eq(
            &binding,
            &candidate.oib_binding().unwrap()
        ));
        assert!(std::sync::Arc::ptr_eq(
            &metadata,
            &candidate.oib_resolved_shared().unwrap()
        ));
        assert_eq!(candidate.save().unwrap(), saved);
        document = candidate;
    }
    assert_eq!(document.save().unwrap(), saved);
}
#[test]
fn a_distinct_admission_still_invalidates_binding_identity() {
    let ui = fixture::admit(&fixture::imported());
    let mut document = Document::new(ui.state_schema.clone(), "oib:changed-admission").unwrap();
    document.configure_oib(&ui).unwrap();
    let before = document.oib_ui.as_ref().unwrap().clone();
    let binding = document.oib_binding().unwrap();
    let mut changed = ui.clone();
    changed.title.push_str(" updated");
    assert!(document.configure_oib(&changed).unwrap());
    assert!(!std::sync::Arc::ptr_eq(
        &before,
        document.oib_ui.as_ref().unwrap()
    ));
    assert!(!std::sync::Arc::ptr_eq(
        &binding,
        &document.oib_binding().unwrap()
    ));
}
