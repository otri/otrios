// SPDX-License-Identifier: LGPL-3.0-only
use super::super::super::tests::Fixture;
use super::*;
use std::process::Command;
const NOW: u64 = 1_800_000_000_000;

fn prepared() -> (Fixture, Store, Binding, String) {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    super::super::tests::data(&fixture, &binding);
    let id = store.create_backup(NOW, true).unwrap();
    write(&primary(&fixture, &binding), b"later document").unwrap();
    write(
        &primary(&fixture, &binding).with_extension("merkle.log"),
        b"later journal",
    )
    .unwrap();
    write(
        &fixture.storage().join(format!(
            "collection-{}.data/later.merkle",
            binding.collection
        )),
        b"later child",
    )
    .unwrap();
    (fixture, store, binding, id)
}
fn primary(fixture: &Fixture, binding: &Binding) -> PathBuf {
    fixture
        .storage()
        .join(format!("collection-{}.merkle", binding.collection))
}
fn assert_restored(fixture: &Fixture, binding: &Binding) {
    assert_eq!(
        fs::read(primary(fixture, binding)).unwrap(),
        b"confirmed document"
    );
    assert!(
        !primary(fixture, binding)
            .with_extension("merkle.log")
            .exists()
    );
    assert!(
        !fixture
            .storage()
            .join(format!(
                "collection-{}.data/later.merkle",
                binding.collection
            ))
            .exists()
    );
    assert_eq!(
        fs::read(fixture.storage().join(format!(
            "collection-{}.data/asset-digest.bin",
            binding.collection
        )))
        .unwrap(),
        b"private asset bytes"
    );
}
#[test]
fn restore_preserves_current_policy_and_unrelated_new_collections_with_a_safety_snapshot() {
    let (fixture, mut store, binding, id) = prepared();
    let request = Request {
        version: VERSION,
        expected_generation: store.registry().unwrap().generation,
        change: Change::Membership {
            space: binding.space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    };
    store.commit("alice", &request, None).unwrap();
    let mut registry = store.registry().unwrap().clone();
    registry = apply(
        &registry,
        "alice",
        &Request {
            version: VERSION,
            expected_generation: registry.generation,
            change: Change::Create {
                name: "New work".into(),
            },
        },
        None,
    )
    .unwrap();
    let space = registry.spaces[1].id.clone();
    let policy = registry.spaces[0].apps[0].policy.clone();
    registry = apply(
        &registry,
        "alice",
        &Request {
            version: VERSION,
            expected_generation: registry.generation,
            change: Change::Attach {
                space,
                app: "notes".into(),
            },
        },
        Some(&policy),
    )
    .unwrap();
    let other = registry.spaces[1].apps[0].collection.clone();
    store
        .publish(
            Record {
                registry,
                ..store.current.clone()
            },
            |_| {},
        )
        .unwrap();
    let other_path = fixture.storage().join(format!("collection-{other}.merkle"));
    write(&other_path, b"independent new data").unwrap();
    let before = store.registry().unwrap().clone();
    assert_eq!(store.restore_backup(&id, NOW + 100, true).unwrap(), 3);
    assert_restored(&fixture, &binding);
    assert_eq!(store.registry().unwrap(), &before);
    assert_eq!(
        role(&store.registry().unwrap().spaces[0], Some("bob")),
        Some(Role::Member)
    );
    assert_eq!(fs::read(other_path).unwrap(), b"independent new data");
    let ids = store.backup_ids().unwrap();
    assert_eq!(ids.len(), 2);
    let safety = ids.iter().find(|other| *other != &id).unwrap();
    let manifest = store.read_snapshot(safety).unwrap();
    let file = manifest
        .files
        .iter()
        .find(|file| file.path == format!("collection-{}.merkle", binding.collection))
        .unwrap();
    assert_eq!(file.sha256, crate::canvas::hash(b"later document"));
}
#[test]
fn older_backup_never_recreates_deleted_data_or_overwrites_the_fresh_collection() {
    let (fixture, mut store, binding, id) = prepared();
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
    store.reclaim_deleted(&binding.collection, true).unwrap();
    let current = store.registry().unwrap().spaces[0].apps[0]
        .collection
        .clone();
    let replacement = fixture
        .storage()
        .join(format!("collection-{current}.merkle"));
    write(&replacement, b"new identity data").unwrap();
    let saved = store.backup_root(false).unwrap().join(&id);
    fs::remove_file(saved.join(format!("collection-{}.merkle", binding.collection))).unwrap();
    fs::remove_dir_all(saved.join(format!("collection-{}.data", binding.collection))).unwrap();
    assert_eq!(store.restore_backup(&id, NOW + 100, true).unwrap(), 0);
    assert!(!primary(&fixture, &binding).exists());
    assert!(
        !fixture
            .storage()
            .join(format!("collection-{}.data", binding.collection))
            .exists()
    );
    assert_eq!(fs::read(replacement).unwrap(), b"new identity data");
    assert_eq!(
        authorize(
            store.registry().unwrap(),
            Some("alice"),
            &binding,
            Operation::Edit
        ),
        Err(Error::Denied)
    );
}
#[test]
fn expired_or_corrupt_snapshot_cannot_change_payloads_or_admit_a_partial_restore() {
    let (fixture, mut store, binding, id) = prepared();
    assert_eq!(
        store.restore_backup(&id, NOW, false),
        Err(Error::Unavailable)
    );
    assert_eq!(
        store.restore_backup(&id, NOW + RETENTION_MS, true),
        Err(Error::Unavailable)
    );
    write(
        &store
            .backup_root(false)
            .unwrap()
            .join(&id)
            .join(format!("collection-{}.merkle", binding.collection)),
        b"corrupt backup",
    )
    .unwrap();
    assert!(store.restore_backup(&id, NOW + 100, true).is_err());
    assert_eq!(
        fs::read(primary(&fixture, &binding)).unwrap(),
        b"later document"
    );
    assert!(store.registry().is_ok());
    assert!(!suffix(store.path(), ".restore").exists());
}
const STEPS: [RestoreStep; 6] = [
    RestoreStep::Staged,
    RestoreStep::Fenced,
    RestoreStep::FilePromoted,
    RestoreStep::FileRemoved,
    RestoreStep::StageRemoved,
    RestoreStep::Unfenced,
];
fn crash(fixture: &Fixture, id: &str, index: usize) {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "spaces::store::backups::restore::tests::child",
            "--nocapture",
        ])
        .env("OTRIOS_RESTORE_TEST_PATH", fixture.path())
        .env("OTRIOS_RESTORE_TEST_ID", id)
        .env("OTRIOS_RESTORE_TEST_BOUNDARY", index.to_string())
        .output()
        .unwrap();
    assert_eq!(
        child.status.code(),
        Some(76),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
}
#[test]
fn child() {
    let Ok(path) = std::env::var("OTRIOS_RESTORE_TEST_PATH") else {
        return;
    };
    let mut store = Store::open(path.into()).unwrap();
    let id = std::env::var("OTRIOS_RESTORE_TEST_ID").unwrap();
    let index: usize = std::env::var("OTRIOS_RESTORE_TEST_BOUNDARY")
        .unwrap()
        .parse()
        .unwrap();
    store
        .restore_at(&id, NOW + 100, true, |step| {
            if step == STEPS[index] {
                std::process::exit(76);
            }
        })
        .unwrap();
    panic!("restore boundary was not reached");
}
#[test]
fn process_exits_recover_all_restore_boundaries_before_admission() {
    for index in 0..STEPS.len() {
        let (fixture, store, binding, id) = prepared();
        drop(store);
        crash(&fixture, &id, index);
        let mut recovered = Store::open(fixture.path()).unwrap();
        if (1..5).contains(&index) {
            assert!(matches!(recovered.registry(), Err(Error::Unavailable)));
        }
        recovered.recover_storage().unwrap();
        assert!(recovered.registry().is_ok());
        if index == 0 {
            assert_eq!(
                fs::read(primary(&fixture, &binding)).unwrap(),
                b"later document"
            );
        } else {
            assert_restored(&fixture, &binding);
        }
        assert!(!suffix(recovered.path(), ".restore").exists());
        assert!(!fs::read_dir(fixture.storage()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".spaces-restore-")
        }));
    }
}
#[test]
fn corrupt_staged_bytes_keep_admission_closed_until_verified_recovery() {
    let (fixture, store, binding, id) = prepared();
    drop(store);
    crash(&fixture, &id, 1);
    let mut recovered = Store::open(fixture.path()).unwrap();
    let staged = fs::read_dir(fixture.storage())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".spaces-restore-")
        })
        .unwrap();
    let staged_primary = staged.join(format!("collection-{}.merkle", binding.collection));
    write(&staged_primary, b"corrupt staged bytes").unwrap();
    assert!(recovered.recover_storage().is_err());
    assert!(matches!(recovered.registry(), Err(Error::Unavailable)));
    write(&staged_primary, b"confirmed document").unwrap();
    recovered.recover_storage().unwrap();
    assert_restored(&fixture, &binding);
}
#[test]
fn initial_ledger_framing_upgrades_without_changing_root_or_membership() {
    let fixture = Fixture::new();
    let (store, _) = fixture.seeded();
    let root = store.root_id().to_owned();
    let mut registry = store.registry().unwrap().clone();
    registry.spaces[0].layout.width = 1;
    let latest = store.latest_path().to_path_buf();
    drop(store);
    for path in [fixture.path(), latest.clone()] {
        let mut bytes = fs::read(&path).unwrap();
        bytes[..5].copy_from_slice(b"OTSP3");
        write(&path, &bytes).unwrap();
    }
    let recovered = Store::open(fixture.path()).unwrap();
    assert_eq!(recovered.root_id(), root);
    assert_eq!(recovered.registry().unwrap(), &registry);
    assert_eq!(&fs::read(recovered.path()).unwrap()[..5], b"OTSP7");
    assert_eq!(
        fs::read(recovered.path()).unwrap(),
        fs::read(latest).unwrap()
    );
}

fn retained_prepared() -> (Fixture, Store, Binding, String, AppPolicy) {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    super::super::tests::data(&fixture, &binding);
    let policy = store.registry().unwrap().spaces[0].apps[0].policy.clone();
    let source = "f".repeat(32);
    let mut registry = store.registry().unwrap().clone();
    registry.retained.push(RetainedDataset {
        space: binding.space.clone(),
        policy: policy.clone(),
        collection: source.clone(),
        label: "Earlier Notes".into(),
        origin: DatasetOrigin::BeforeReplacement,
        generation: registry.generation,
    });
    store
        .publish(
            Record {
                registry,
                ..store.current.clone()
            },
            |_| {},
        )
        .unwrap();
    let path = fixture
        .storage()
        .join(format!("collection-{source}.merkle"));
    write(&path, b"sealed confirmed checkpoint").unwrap();
    let data = fixture.storage().join(format!("collection-{source}.data"));
    new_directory(&data).unwrap();
    write(&data.join("blob.bin"), b"sealed asset").unwrap();
    (fixture, store, binding, source, policy)
}
fn retained_next(store: &Store, binding: &Binding, source: &str, policy: &AppPolicy) -> Record {
    let registry = apply(
        store.registry().unwrap(),
        "alice",
        &Request {
            version: VERSION,
            expected_generation: store.registry().unwrap().generation,
            change: Change::RestoreDataset {
                space: binding.space.clone(),
                app: policy.app.clone(),
                collection: source.into(),
            },
        },
        Some(policy),
    )
    .unwrap();
    Record {
        registry,
        ..store.current.clone()
    }
}
#[test]
fn retained_restore_copies_confirmed_data_and_assets_into_a_fresh_binding_and_keeps_both_originals()
{
    let (fixture, mut store, binding, source, policy) = retained_prepared();
    let generation = store.registry().unwrap().generation;
    let next = retained_next(&store, &binding, &source, &policy);
    store
        .clone_retained(next, &binding.space, &policy.app, &source)
        .unwrap();
    let registry = store.registry().unwrap();
    assert_eq!(registry.generation, generation + 1);
    let target = &registry.spaces[0].apps[0].collection;
    assert_ne!(target, &source);
    assert_ne!(target, &binding.collection);
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{target}.merkle"))
        )
        .unwrap(),
        b"sealed confirmed checkpoint"
    );
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{target}.data/blob.bin"))
        )
        .unwrap(),
        b"sealed asset"
    );
    assert_eq!(
        fs::read(primary(&fixture, &binding)).unwrap(),
        b"confirmed document"
    );
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{source}.merkle"))
        )
        .unwrap(),
        b"sealed confirmed checkpoint"
    );
    assert!(
        registry
            .retained
            .iter()
            .any(|d| d.collection == binding.collection)
    );
    assert!(registry.retained.iter().any(|d| d.collection == source));
    assert!(authorize(registry, Some("alice"), &binding, Operation::Edit).is_err());
    let backup = store.create_backup(NOW, true).unwrap();
    let snapshot = store.read_snapshot(&backup).unwrap();
    assert_eq!(snapshot.collections.len(), 3);
    assert!(
        snapshot
            .files
            .iter()
            .any(|f| f.path == format!("collection-{source}.data/blob.bin"))
    );
}
const RETAINED_STEPS: [RestoreStep; 6] = [
    RestoreStep::Staged,
    RestoreStep::Fenced,
    RestoreStep::FilePromoted,
    RestoreStep::DecisionPublished,
    RestoreStep::StageRemoved,
    RestoreStep::Unfenced,
];
#[test]
fn retained_child() {
    let Ok(path) = std::env::var("OTRIOS_RETAINED_TEST_PATH") else {
        return;
    };
    let mut store = Store::open(path.into()).unwrap();
    let space = &store.registry().unwrap().spaces[0];
    let policy = space.apps[0].policy.clone();
    let binding = bind(
        store.registry().unwrap(),
        Some("alice"),
        &space.id,
        &policy.app,
        &policy.schema,
    )
    .unwrap();
    let source = store.registry().unwrap().retained[0].collection.clone();
    let next = retained_next(&store, &binding, &source, &policy);
    let index: usize = std::env::var("OTRIOS_RETAINED_TEST_BOUNDARY")
        .unwrap()
        .parse()
        .unwrap();
    store
        .clone_retained_at(next, &binding.space, &policy.app, &source, |step| {
            if step == RETAINED_STEPS[index] {
                std::process::exit(77)
            }
        })
        .unwrap();
    panic!("retained restore boundary not reached");
}
#[test]
fn process_exits_never_admit_partial_retained_restore_or_lose_current_data() {
    for index in 0..RETAINED_STEPS.len() {
        let (fixture, store, binding, source, _) = retained_prepared();
        let generation = store.registry().unwrap().generation;
        drop(store);
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "spaces::store::backups::restore::tests::retained_child",
                "--nocapture",
            ])
            .env("OTRIOS_RETAINED_TEST_PATH", fixture.path())
            .env("OTRIOS_RETAINED_TEST_BOUNDARY", index.to_string())
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(77),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        let mut store = Store::open(fixture.path()).unwrap();
        if (1..5).contains(&index) {
            assert!(store.registry().is_err());
        }
        store.recover_storage().unwrap();
        let registry = store.registry().unwrap();
        assert_eq!(registry.generation, generation + u64::from(index > 0));
        assert_eq!(
            fs::read(primary(&fixture, &binding)).unwrap(),
            b"confirmed document"
        );
        assert_eq!(
            fs::read(
                fixture
                    .storage()
                    .join(format!("collection-{source}.merkle"))
            )
            .unwrap(),
            b"sealed confirmed checkpoint"
        );
        if index > 0 {
            let target = &registry.spaces[0].apps[0].collection;
            assert_eq!(
                fs::read(
                    fixture
                        .storage()
                        .join(format!("collection-{target}.data/blob.bin"))
                )
                .unwrap(),
                b"sealed asset"
            );
            assert_eq!(
                fs::read(
                    fixture
                        .storage()
                        .join(format!("collection-{target}.merkle"))
                )
                .unwrap(),
                b"sealed confirmed checkpoint"
            );
        }
        assert!(!suffix(store.path(), ".restore").exists());
    }
}
