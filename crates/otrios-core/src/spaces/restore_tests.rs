// SPDX-License-Identifier: LGPL-3.0-only
use super::super::super::tests::Fixture;
use super::*;
use crate::transfers::ReplacementOutcome;
use std::process::Command;
const NOW: u64 = 1_800_000_000_000;

fn portable_schema() -> otrios_interfaces::canvas::StateSchema {
    use otrios_interfaces::{
        canvas::{Field, StateSchema},
        layout::Value,
    };
    StateSchema {
        id: "portable-replacement-test-v1".into(),
        fields: vec![Field {
            id: 1,
            name: "value".into(),
            default: Value::Number(0.),
            writable: true,
        }],
    }
}
fn portable_import() -> (AppPolicy, crate::transfers::QualifiedCollection) {
    use crate::transfers::*;
    let root = "app:notes/state";
    let schema = portable_schema();
    let hash = collection_schema(&[RootSchema {
        state_ref: root.into(),
        schema: schema.clone(),
    }])
    .unwrap();
    let policy = AppPolicy {
        app: "notes".into(),
        schema: hash.clone(),
        closed_only: false,
        editable: true,
        comments: false,
    };
    let mut doc = crate::canvas::Document::new(schema.clone(), root).unwrap();
    doc.set(&[(1, 123)]).unwrap();
    let checkpoint = encode_collection(&[OwnedRoot {
        state_ref: root.into(),
        canvas: doc.portable_checkpoint().unwrap(),
        chat: None,
    }])
    .unwrap();
    let data = encode_data(
        DataInfo {
            format: DATA_FORMAT.into(),
            bundle_id: "notes".into(),
            schema_sha256: hash.clone(),
            codec: COLLECTION_CODEC.into(),
            codec_version: 1,
            checkpoint: "State/checkpoint.bin".into(),
        },
        checkpoint,
        BTreeMap::new(),
    )
    .unwrap();
    let data = inspect_data(archive::decode(archive::Kind::Data, &data).unwrap()).unwrap();
    let qualified = QualifiedCollection::qualify(
        data,
        "notes",
        &hash,
        &[RootPolicy {
            state_ref: root.into(),
            schema,
            chat: false,
        }],
    )
    .unwrap();
    (policy, qualified)
}
fn portable_prepared() -> (Fixture, Store, crate::transfers::Replacement, AppPolicy) {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.path()).unwrap();
    new_directory(&fixture.storage()).unwrap();
    store.bind_storage(&fixture.storage()).unwrap();
    store
        .commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: 0,
                change: Change::Create {
                    name: "Test".into(),
                },
            },
            None,
        )
        .unwrap();
    let space = store.registry().unwrap().spaces[0].id.clone();
    let (policy, _) = portable_import();
    store
        .commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: 1,
                change: Change::Attach {
                    space: space.clone(),
                    app: "notes".into(),
                },
            },
            Some(&policy),
        )
        .unwrap();
    let collection = store.registry().unwrap().spaces[0].apps[0]
        .collection
        .clone();
    write(
        &fixture
            .storage()
            .join(format!("collection-{collection}.merkle")),
        b"outgoing confirmed bytes",
    )
    .unwrap();
    let review = crate::transfers::Replacement {
        space,
        app: "notes".into(),
        collection,
        replacement: "c".repeat(32),
        expected_generation: store.registry().unwrap().generation,
    };
    (fixture, store, review, policy)
}
#[test]
fn portable_replacement_rechecks_admin_generation_and_binding_and_retains_outgoing_bytes() {
    let (fixture, mut store, review, policy) = portable_prepared();
    let (_, imported) = portable_import();
    assert_eq!(
        store.replace_portable("bob", &review, &policy, "app:notes/state", &imported),
        Err(Error::Denied)
    );
    let mut stale = review.clone();
    stale.expected_generation -= 1;
    assert_eq!(
        store.replace_portable("alice", &stale, &policy, "app:notes/state", &imported),
        Err(Error::Conflict)
    );
    let mut wrong = review.clone();
    wrong.collection = "a".repeat(32);
    assert_eq!(
        store.replace_portable("alice", &wrong, &policy, "app:notes/state", &imported),
        Err(Error::Conflict)
    );
    assert_eq!(
        store
            .replacement_committed("alice", &review.space, &review.replacement)
            .unwrap(),
        ReplacementOutcome::Unknown
    );
    let generation = store
        .replace_portable("alice", &review, &policy, "app:notes/state", &imported)
        .unwrap();
    assert_eq!(generation, review.expected_generation + 1);
    assert_eq!(
        store.registry().unwrap().retained[0].collection,
        review.collection
    );
    assert_eq!(
        store.registry().unwrap().retained[0].origin,
        DatasetOrigin::BeforeReplacement
    );
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{}.merkle", review.collection))
        )
        .unwrap(),
        b"outgoing confirmed bytes"
    );
    assert_eq!(
        store
            .replacement_committed("alice", &review.space, &review.replacement)
            .unwrap(),
        ReplacementOutcome::Committed
    );
    assert_eq!(
        store.replacement_committed("bob", &review.space, &review.replacement),
        Err(Error::Denied)
    );
    assert!(
        store
            .replace_portable("alice", &review, &policy, "app:notes/state", &imported)
            .is_err(),
        "uncertain mutation is not replayed"
    );
}
#[test]
fn portable_replacement_child() {
    let Ok(path) = std::env::var("OTRIOS_REPLACEMENT_TEST_PATH") else {
        return;
    };
    let mut store = Store::open(path.into()).unwrap();
    let registry = store.registry().unwrap();
    let space = &registry.spaces[0];
    let review = crate::transfers::Replacement {
        space: space.id.clone(),
        app: "notes".into(),
        collection: space.apps[0].collection.clone(),
        replacement: "c".repeat(32),
        expected_generation: registry.generation,
    };
    let (policy, imported) = portable_import();
    let index: usize = std::env::var("OTRIOS_REPLACEMENT_TEST_BOUNDARY")
        .unwrap()
        .parse()
        .unwrap();
    store
        .replace_portable_at(
            "alice",
            &review,
            &policy,
            "app:notes/state",
            &imported,
            |step| {
                if step == RETAINED_STEPS[index] {
                    std::process::exit(79);
                }
            },
        )
        .unwrap();
    panic!("replacement boundary not reached");
}
#[test]
fn portable_replacement_process_exits_never_publish_partial_data_or_lose_outgoing_history() {
    for index in 0..RETAINED_STEPS.len() {
        let (fixture, store, review, _) = portable_prepared();
        drop(store);
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "spaces::store::backups::restore::tests::portable_replacement_child",
                "--nocapture",
            ])
            .env("OTRIOS_REPLACEMENT_TEST_PATH", fixture.path())
            .env("OTRIOS_REPLACEMENT_TEST_BOUNDARY", index.to_string())
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(79),
            "{} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        let mut store = Store::open(fixture.path()).unwrap();
        if (1..5).contains(&index) {
            assert!(store.registry().is_err());
        }
        store.recover_storage().unwrap();
        let committed = index > 0;
        assert_eq!(
            store
                .replacement_committed("alice", &review.space, &review.replacement)
                .unwrap(),
            if committed {
                ReplacementOutcome::Committed
            } else {
                ReplacementOutcome::Unknown
            }
        );
        assert_eq!(
            store.registry().unwrap().generation,
            review.expected_generation + u64::from(committed)
        );
        assert_eq!(
            fs::read(
                fixture
                    .storage()
                    .join(format!("collection-{}.merkle", review.collection))
            )
            .unwrap(),
            b"outgoing confirmed bytes"
        );
        if committed {
            let bytes = fs::read(
                fixture
                    .storage()
                    .join(format!("collection-{}.merkle", review.replacement)),
            )
            .unwrap();
            let doc = crate::canvas::Document::load(portable_schema(), "app:notes/state", &bytes)
                .unwrap();
            assert_eq!(doc.value(1).unwrap(), 123);
            assert_eq!(
                store.registry().unwrap().retained[0].collection,
                review.collection
            );
        }
        assert!(!suffix(store.path(), ".restore").exists());
    }
}

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
    assert_eq!(&fs::read(recovered.path()).unwrap()[..5], b"OTSP9");
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

fn move_prepared(
    mode: MoveMode,
    occupied: bool,
) -> (Fixture, Store, Binding, String, AppPolicy, Request) {
    let f = Fixture::new();
    let (mut store, binding) = f.seeded();
    super::super::tests::data(&f, &binding);
    let policy = store.registry().unwrap().spaces[0].apps[0].policy.clone();
    store
        .commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: store.registry().unwrap().generation,
                change: Change::Create {
                    name: "Destination".into(),
                },
            },
            None,
        )
        .unwrap();
    let destination = store.registry().unwrap().spaces[1].id.clone();
    if occupied {
        store
            .commit(
                "alice",
                &Request {
                    version: VERSION,
                    expected_generation: store.registry().unwrap().generation,
                    change: Change::Attach {
                        space: destination.clone(),
                        app: policy.app.clone(),
                    },
                },
                Some(&policy),
            )
            .unwrap();
        let cid = &store.registry().unwrap().spaces[1].apps[0].collection;
        write(
            &f.storage().join(format!("collection-{cid}.merkle")),
            b"outgoing confirmed checkpoint",
        )
        .unwrap();
    }
    let r = store.registry().unwrap();
    let req = Request {
        version: VERSION,
        expected_generation: r.generation,
        change: Change::Move {
            operation: "d".repeat(32),
            source: binding.space.clone(),
            destination: destination.clone(),
            app: policy.app.clone(),
            collection: binding.collection.clone(),
            destination_collection: r.spaces[1].apps.first().map(|a| a.collection.clone()),
            mode,
        },
    };
    (f, store, binding, destination, policy, req)
}
#[test]
fn moves_publish_owned_bytes_and_outcomes_once_and_source_cleanup_cannot_remove_the_destination() {
    for mode in [MoveMode::AppAndData, MoveMode::AppOnly, MoveMode::DataOnly] {
        for occupied in [false, true] {
            let (f, mut store, binding, destination, p, request) = move_prepared(mode, occupied);
            let outgoing = store.registry().unwrap().spaces[1]
                .apps
                .first()
                .map(|a| a.collection.clone());
            store.commit("alice", &request, Some(&p)).unwrap();
            let generation = store.registry().unwrap().generation;
            let target = store.registry().unwrap().spaces[1].apps[0]
                .collection
                .clone();
            assert_eq!(
                store
                    .move_committed("alice", &binding.space, &destination, &"d".repeat(32))
                    .unwrap(),
                Some(generation)
            );
            assert!(
                store
                    .move_committed("bob", &binding.space, &destination, &"d".repeat(32))
                    .is_err()
            );
            assert!(
                store
                    .move_committed("alice", &binding.space, &binding.space, &"d".repeat(32))
                    .is_err()
            );
            store.commit("alice", &request, None).unwrap();
            assert_eq!(store.registry().unwrap().generation, generation);
            if mode != MoveMode::AppOnly {
                assert_eq!(
                    fs::read(f.storage().join(format!("collection-{target}.merkle"))).unwrap(),
                    b"confirmed document"
                );
                store.reclaim_deleted(&binding.collection, true).unwrap();
                assert!(!primary(&f, &binding).exists());
                assert_eq!(
                    fs::read(
                        f.storage()
                            .join(format!("collection-{target}.data/asset-digest.bin"))
                    )
                    .unwrap(),
                    b"private asset bytes"
                );
            } else {
                assert_eq!(
                    fs::read(primary(&f, &binding)).unwrap(),
                    b"confirmed document"
                );
            }
            if let Some(old) = outgoing {
                assert_eq!(
                    fs::read(f.storage().join(format!("collection-{old}.merkle"))).unwrap(),
                    b"outgoing confirmed checkpoint"
                );
            }
            let mut changed = request.clone();
            if let Change::Move { mode, .. } = &mut changed.change {
                *mode = if *mode == MoveMode::AppOnly {
                    MoveMode::DataOnly
                } else {
                    MoveMode::AppOnly
                };
            }
            assert!(matches!(
                store.commit("alice", &changed, Some(&p)),
                Err(Error::Conflict)
            ));
            drop(store);
            let mut store = Store::open(f.path()).unwrap();
            store.recover_storage().unwrap();
            assert_eq!(
                store
                    .move_committed("alice", &binding.space, &destination, &"d".repeat(32))
                    .unwrap(),
                Some(generation)
            );
            assert_eq!(
                store.registry().unwrap().spaces[1].apps[0].collection,
                target
            );
        }
    }
}
const MOVE_STEPS: [RestoreStep; 7] = [
    RestoreStep::Staged,
    RestoreStep::Fenced,
    RestoreStep::FilePromoted,
    RestoreStep::DecisionPublished,
    RestoreStep::ReceiptPublished,
    RestoreStep::StageRemoved,
    RestoreStep::Unfenced,
];
#[test]
fn move_child() {
    let Ok(path) = std::env::var("OTRIOS_MOVE_TEST_PATH") else {
        return;
    };
    let mut store = Store::open(path.into()).unwrap();
    let r = store.registry().unwrap();
    let source = &r.spaces[0];
    let destination = &r.spaces[1];
    let a = &source.apps[0];
    let request = Request {
        version: VERSION,
        expected_generation: r.generation,
        change: Change::Move {
            operation: "d".repeat(32),
            source: source.id.clone(),
            destination: destination.id.clone(),
            app: a.policy.app.clone(),
            collection: a.collection.clone(),
            destination_collection: destination.apps.first().map(|a| a.collection.clone()),
            mode: MoveMode::AppAndData,
        },
    };
    let registry = apply(r, "alice", &request, Some(&a.policy)).unwrap();
    let next = Record {
        registry,
        ..store.current.clone()
    };
    let index: usize = std::env::var("OTRIOS_MOVE_TEST_BOUNDARY")
        .unwrap()
        .parse()
        .unwrap();
    store
        .stage_move_at(next, "alice", &request, |step| {
            if step == MOVE_STEPS[index] {
                std::process::exit(78)
            }
        })
        .unwrap();
    panic!("move boundary not reached");
}
#[test]
fn process_exits_recover_one_move_decision_and_its_durable_operation_outcome_before_admission() {
    for index in 0..MOVE_STEPS.len() {
        let (f, store, binding, destination, p, request) =
            move_prepared(MoveMode::AppAndData, true);
        let generation = store.registry().unwrap().generation;
        drop(store);
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "spaces::store::backups::restore::tests::move_child",
                "--nocapture",
            ])
            .env("OTRIOS_MOVE_TEST_PATH", f.path())
            .env("OTRIOS_MOVE_TEST_BOUNDARY", index.to_string())
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(78),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        let mut store = Store::open(f.path()).unwrap();
        if (1..6).contains(&index) {
            assert!(store.registry().is_err());
        }
        store.recover_storage().unwrap();
        assert_eq!(
            store.registry().unwrap().generation,
            generation + u64::from(index > 0)
        );
        let outcome = store
            .move_committed("alice", &binding.space, &destination, &"d".repeat(32))
            .unwrap();
        if index == 0 {
            assert_eq!(outcome, None);
            assert_eq!(
                store.registry().unwrap().spaces[0].apps[0].collection,
                binding.collection
            );
        } else {
            assert_eq!(outcome, Some(generation + 1));
            let target = store.registry().unwrap().spaces[1].apps[0]
                .collection
                .clone();
            assert_eq!(
                fs::read(
                    f.storage()
                        .join(format!("collection-{target}.data/asset-digest.bin"))
                )
                .unwrap(),
                b"private asset bytes"
            );
            store.commit("alice", &request, Some(&p)).unwrap();
            assert_eq!(store.registry().unwrap().generation, generation + 1);
        }
        assert!(!suffix(store.path(), ".restore").exists());
    }
}

#[test]
fn portable_replacement_of_a_sealed_ghost_preserves_the_active_slot_and_both_inactive_datasets() {
    let (fixture, mut store, review, policy) = portable_prepared();
    let (_, imported) = portable_import();
    store
        .replace_portable("alice", &review, &policy, "app:notes/state", &imported)
        .unwrap();
    let active_before = fs::read(
        fixture
            .storage()
            .join(format!("collection-{}.merkle", review.replacement)),
    )
    .unwrap();
    let sealed = crate::transfers::Replacement {
        replacement: "d".repeat(32),
        expected_generation: store.registry().unwrap().generation,
        ..review.clone()
    };
    store
        .replace_portable("alice", &sealed, &policy, "app:notes/state", &imported)
        .unwrap();
    assert_eq!(
        store.registry().unwrap().spaces[0].apps[0].collection,
        review.replacement
    );
    assert_eq!(store.registry().unwrap().retained.len(), 2);
    assert_eq!(
        store.registry().unwrap().retained[0].collection,
        review.collection
    );
    assert_eq!(
        store.registry().unwrap().retained[1].collection,
        sealed.replacement
    );
    assert_eq!(
        store.registry().unwrap().retained[1].origin,
        DatasetOrigin::Imported
    );
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{}.merkle", review.replacement))
        )
        .unwrap(),
        active_before
    );
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{}.merkle", review.collection))
        )
        .unwrap(),
        b"outgoing confirmed bytes"
    );
    drop(store);
    let mut recovered = Store::open(fixture.path()).unwrap();
    recovered.bind_storage(&fixture.storage()).unwrap();
    recovered.recover_storage().unwrap();
    assert_eq!(
        recovered.registry().unwrap().spaces[0].apps[0].collection,
        review.replacement
    );
    assert_eq!(
        recovered
            .replacement_committed("alice", &review.space, &sealed.replacement)
            .unwrap(),
        ReplacementOutcome::Committed
    );
}
