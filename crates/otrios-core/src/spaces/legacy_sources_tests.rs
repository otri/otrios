// SPDX-License-Identifier: LGPL-3.0-only
use super::super::super::tests::Fixture;
use super::*;
const NOW: u64 = 1_800_000_000_000;
#[test]
fn qualification_precedes_retention_and_original_expiry_preserves_collaborative_data() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    let log = source.with_extension("merkle.log");
    write(&source, b"confirmed migration original").unwrap();
    write(&log, b"confirmed original journal").unwrap();
    let active = fixture
        .storage()
        .join(format!("collection-{}.merkle", binding.collection));
    write(&active, b"later collaborative edits").unwrap();
    store
        .check_legacy_import(&binding.collection, &source)
        .unwrap();
    store
        .register_legacy_sources(&binding.collection, &[source.clone(), log.clone()])
        .unwrap();
    assert!(
        store
            .legacy_source_registered(&binding.collection, &source)
            .unwrap()
    );
    assert_eq!(store.legacy_deadlines().unwrap()[&binding.collection], None);
    assert_eq!(
        store.prune_legacy_sources(NOW + RETENTION_MS * 2).unwrap(),
        0
    );
    assert_eq!(
        store.finalize_legacy_sources(NOW, false),
        Err(Error::Unavailable)
    );
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    assert_eq!(until, NOW + RETENTION_MS);
    assert_eq!(
        store
            .finalize_legacy_sources(NOW + RETENTION_MS, true)
            .unwrap(),
        until
    );
    assert!(!store.legacy_imports_open().unwrap());
    assert!(
        store
            .check_legacy_import(&binding.collection, &source)
            .is_err()
    );
    assert_eq!(store.prune_legacy_sources(until - 1).unwrap(), 0);
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 2);
    assert!(!source.exists() && !log.exists());
    assert!(store.legacy_deadlines().unwrap().is_empty());
    assert_eq!(fs::read(active).unwrap(), b"later collaborative edits");
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 0);
}
#[test]
fn a_deleted_import_cannot_be_replayed_into_the_fresh_collection() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    write(&source, b"old private data").unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    let request = Request {
        version: VERSION,
        expected_generation: store.registry().unwrap().generation,
        change: Change::DeleteData {
            space: binding.space.clone(),
            app: binding.app.clone(),
            remove: false,
        },
    };
    store.commit("alice", &request, None).unwrap();
    let replacement = &store.registry().unwrap().spaces[0].apps[0].collection;
    assert!(store.check_legacy_import(replacement, &source).is_err());
    assert!(source.exists());
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    store.prune_legacy_sources(until).unwrap();
    assert!(!source.exists());
}
#[test]
fn changing_an_original_never_silently_requalifies_it_or_extends_retention() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    write(&source, b"confirmed original").unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    write(&source, b"new unqualified original").unwrap();
    assert_eq!(
        store.finalize_legacy_sources(NOW, true),
        Err(Error::Conflict)
    );
    assert_eq!(
        store.register_legacy_sources(&binding.collection, std::slice::from_ref(&source)),
        Err(Error::Conflict)
    );
    write(&source, b"confirmed original").unwrap();
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    write(&source, b"changed after qualification").unwrap();
    assert_eq!(store.prune_legacy_sources(until), Err(Error::Conflict));
    assert_eq!(store.finalize_legacy_sources(until, true).unwrap(), until);
    assert_eq!(fs::read(source).unwrap(), b"changed after qualification");
}
#[test]
fn active_chat_owner_blocks_qualification_and_expiry() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("brass-relay.journal");
    let room = crate::canvas::chat::Room::open(Some(source.clone()), NOW).unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    assert!(store.finalize_legacy_sources(NOW, true).is_err());
    drop(room);
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    let room = crate::canvas::chat::Room::open(Some(source.clone()), NOW).unwrap();
    assert!(store.prune_legacy_sources(until).is_err());
    assert!(source.exists());
    drop(room);
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 1);
    assert!(!source.exists());
}

#[test]
fn a_changed_journal_cannot_remove_its_checkpoint_during_expiry() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    let log = source.with_extension("merkle.log");
    write(&source, b"checkpoint").unwrap();
    write(&log, b"journal").unwrap();
    store
        .register_legacy_sources(&binding.collection, &[source.clone(), log.clone()])
        .unwrap();
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    write(&log, b"new confirmed journal").unwrap();
    assert_eq!(store.prune_legacy_sources(until), Err(Error::Conflict));
    assert_eq!(fs::read(source).unwrap(), b"checkpoint");
    assert_eq!(fs::read(log).unwrap(), b"new confirmed journal");
}

#[test]
fn restoring_an_older_primary_cannot_reopen_finalized_imports() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    write(&source, b"original").unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    let older = fs::read(fixture.path()).unwrap();
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    drop(store);
    write(&fixture.path(), &older).unwrap();
    let mut recovered = Store::open(fixture.path()).unwrap();
    assert!(!recovered.legacy_imports_open().unwrap());
    assert_eq!(
        recovered.legacy_deadlines().unwrap()[&binding.collection],
        Some(until)
    );
    assert_eq!(
        recovered.check_legacy_import(&binding.collection, &source),
        Err(Error::Denied)
    );
    assert_eq!(
        recovered.finalize_legacy_sources(until, true).unwrap(),
        until
    );
    drop(recovered);
    fs::remove_file(suffix(&fixture.path(), ".latest")).unwrap();
    assert!(
        Store::open(fixture.path()).is_err(),
        "no empty fallback after losing the safety ledger"
    );
}

#[test]
fn missing_originals_do_not_prevent_finalizing_the_remaining_inventory() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    write(&source, b"original").unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    fs::remove_file(&source).unwrap();
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    assert!(store.legacy_deadlines().unwrap().is_empty());
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 0);
}

#[test]
fn interrupted_original_replacements_are_reclaimed_in_bounded_turns() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("notes-old.merkle");
    write(&source, b"original").unwrap();
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    for nonce in 0..130 {
        write(
            &fixture.storage().join(format!("notes-old.27.{nonce}.tmp")),
            b"old payload",
        )
        .unwrap();
    }
    for unrelated in [
        "another.27.1.tmp",
        "notes-old.not-a-pid.1.tmp",
        "notes-old.0.1.tmp",
    ] {
        write(&fixture.storage().join(unrelated), b"independent bytes").unwrap();
    }
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    assert_eq!(store.prune_legacy_sources(until - 1).unwrap(), 0);
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 129);
    assert!(!store.legacy_deadlines().unwrap().is_empty());
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 2);
    assert!(store.legacy_deadlines().unwrap().is_empty());
    for unrelated in [
        "another.27.1.tmp",
        "notes-old.not-a-pid.1.tmp",
        "notes-old.0.1.tmp",
    ] {
        assert_eq!(
            fs::read(fixture.storage().join(unrelated)).unwrap(),
            b"independent bytes"
        );
    }
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 0);
}

#[test]
fn interrupted_chat_compaction_and_checkpoint_temporaries_expire_with_their_originals() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let source = fixture.storage().join("brass-relay.journal");
    let room = crate::canvas::chat::Room::open(Some(source.clone()), NOW).unwrap();
    drop(room);
    store
        .register_legacy_sources(&binding.collection, std::slice::from_ref(&source))
        .unwrap();
    let temporary = fixture
        .storage()
        .join("brass-relay.abcdef0123456789.compact");
    write(&temporary, b"retired chat payload").unwrap();
    let until = store.finalize_legacy_sources(NOW, true).unwrap();
    assert_eq!(store.prune_legacy_sources(until).unwrap(), 2);
    assert!(!source.exists() && !temporary.exists());
    assert!(source.with_extension("journal.owner.lock").exists());
}
