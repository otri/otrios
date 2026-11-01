// SPDX-License-Identifier: LGPL-3.0-only
use super::super::tests::Fixture;
use super::*;
const NOW: u64 = 1_800_000_000_000;
const WEEK: u64 = 7 * 24 * 60 * 60 * 1000;

fn delete(store: &mut Store, binding: &Binding) {
    let request = Request {
        version: VERSION,
        expected_generation: store.registry().unwrap().generation,
        change: Change::DeleteData {
            space: binding.space.clone(),
            app: binding.app.clone(),
            collection: binding.collection.clone(),
            remove: false,
        },
    };
    store.commit("alice", &request, None).unwrap();
}

#[test]
fn replica_disposal_payload_reclamation_and_retention_have_separate_boundaries() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let payload = fixture
        .storage()
        .join(format!("collection-{}.merkle", binding.collection));
    write(&payload, b"confirmed collaborative payload").unwrap();
    let original = fixture.storage().join("notes-old.merkle");
    write(&original, b"confirmed migration original").unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&original))
        .unwrap();
    store.create_backup(NOW, true).unwrap();
    delete(&mut store, &binding);
    let active = BTreeSet::from([binding.collection.clone()]);
    let summary = &store.maintain_deleted(NOW, &active).unwrap()[&binding.space];
    assert_eq!(summary.deleted_collections, 1);
    assert_eq!(summary.local_pending, 1);
    assert_eq!(summary.backup_until_unix_ms, Some(NOW + WEEK));
    assert!(summary.legacy_qualification_required);
    assert!(!summary.storage_unknown);
    assert!(payload.exists());

    let summary = &store.maintain_deleted(NOW, &BTreeSet::new()).unwrap()[&binding.space];
    assert_eq!(summary.local_pending, 0);
    assert_eq!(summary.backup_until_unix_ms, Some(NOW + WEEK));
    assert!(!payload.exists());
    assert!(original.exists());
    let until = store.finalize_legacy_sources(NOW + 1, true).unwrap();
    drop(store);
    let mut store = Store::open(fixture.path()).unwrap();
    let replacement = store.registry().unwrap().spaces[0].apps[0]
        .collection
        .clone();
    let live = fixture
        .storage()
        .join(format!("collection-{replacement}.merkle"));
    write(&live, b"independent replacement").unwrap();
    let summary = &store.maintain_deleted(NOW + 1, &BTreeSet::new()).unwrap()[&binding.space];
    assert_eq!(summary.legacy_until_unix_ms, Some(until));
    assert!(!summary.legacy_qualification_required);
    let summary = &store
        .maintain_deleted(until + 60_000, &BTreeSet::new())
        .unwrap()[&binding.space];
    assert_eq!(summary.local_pending, 0);
    assert_eq!(summary.backup_until_unix_ms, None);
    assert_eq!(summary.legacy_until_unix_ms, None);
    assert!(!summary.storage_unknown);
    assert!(!original.exists());
    assert!(store.backup_ids().unwrap().is_empty());
    assert_eq!(fs::read(live).unwrap(), b"independent replacement");
    assert_eq!(
        store.registry().unwrap().deletions[0].collection,
        binding.collection
    );
}

#[test]
fn bounded_gc_rotates_past_blocked_clients_and_survives_restart() {
    let fixture = Fixture::new();
    let (mut store, first) = fixture.seeded();
    for _ in 0..20 {
        let binding = bind(
            store.registry().unwrap(),
            Some("alice"),
            &first.space,
            "notes",
            &first.schema,
        )
        .unwrap();
        write(
            &fixture
                .storage()
                .join(format!("collection-{}.merkle", binding.collection)),
            b"old payload",
        )
        .unwrap();
        delete(&mut store, &binding);
    }
    let blocked = BTreeSet::from([first.collection.clone()]);
    assert_eq!(
        store.maintain_deleted(NOW, &blocked).unwrap()[&first.space].local_pending,
        13
    );
    for _ in 0..3 {
        store.maintain_deleted(NOW, &blocked).unwrap();
    }
    assert_eq!(
        store.maintain_deleted(NOW, &blocked).unwrap()[&first.space].local_pending,
        1
    );
    drop(store);
    let mut store = Store::open(fixture.path()).unwrap();
    for _ in 0..3 {
        store.maintain_deleted(NOW, &BTreeSet::new()).unwrap();
    }
    assert_eq!(
        store.maintain_deleted(NOW, &BTreeSet::new()).unwrap()[&first.space].local_pending,
        0
    );
}

#[test]
fn corrupt_backup_metadata_never_reports_retention_as_complete() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    write(
        &fixture
            .storage()
            .join(format!("collection-{}.merkle", binding.collection)),
        b"payload",
    )
    .unwrap();
    let backup = store.create_backup(NOW, true).unwrap();
    let manifest = suffix(store.path(), ".backups")
        .join(backup)
        .join("manifest.flex");
    write(&manifest, b"corrupt manifest").unwrap();
    delete(&mut store, &binding);
    let summary = &store.maintain_deleted(NOW, &BTreeSet::new()).unwrap()[&binding.space];
    assert_eq!(summary.local_pending, 0);
    assert!(summary.storage_unknown);
    assert!(
        !store
            .maintain_deleted(NOW + WEEK, &BTreeSet::new())
            .unwrap()[&binding.space]
            .storage_unknown
    );
}
