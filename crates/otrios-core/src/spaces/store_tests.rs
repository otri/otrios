// SPDX-License-Identifier: LGPL-3.0-only
//! Real process exits at registry publication boundaries, not a power-loss model.
use super::*;
use std::process::{Command, Output};

pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "otrios-space-ledger-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(root)
    }
    pub(super) fn path(&self) -> PathBuf {
        self.0.join("registry.flex")
    }
    pub(super) fn storage(&self) -> PathBuf {
        self.0.join("state")
    }
    pub(super) fn seeded(&self) -> (Store, Binding) {
        let mut store = Store::open(self.path()).unwrap();
        fs::create_dir(self.storage()).unwrap();
        store.bind_storage(&self.storage()).unwrap();
        let create = Request {
            version: VERSION,
            expected_generation: 0,
            change: Change::Create {
                name: "Household".into(),
            },
        };
        store.commit("alice", &create, None).unwrap();
        let space = store.registry().unwrap().spaces[0].id.clone();
        let policy = AppPolicy {
            app: "notes".into(),
            schema: "b".repeat(64),
            closed_only: false,
            editable: true,
            comments: false,
        };
        let attach = Request {
            version: VERSION,
            expected_generation: 1,
            change: Change::Attach {
                space: space.clone(),
                app: "notes".into(),
            },
        };
        store.commit("alice", &attach, Some(&policy)).unwrap();
        let binding = bind(
            store.registry().unwrap(),
            Some("alice"),
            &space,
            "notes",
            &policy.schema,
        )
        .unwrap();
        (store, binding)
    }
    fn child(&self, action: &str, boundary: usize) -> Output {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "spaces::store::tests::child", "--nocapture"])
            .env("OTRIOS_STORE_TEST_ACTION", action)
            .env("OTRIOS_STORE_TEST_PATH", self.path())
            .env("OTRIOS_STORE_TEST_BOUNDARY", boundary.to_string())
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const BOUNDARIES: [Boundary; 8] = [
    Boundary::BeforeLatest,
    Boundary::Latest(FileStep::TempSynced),
    Boundary::Latest(FileStep::Renamed),
    Boundary::Latest(FileStep::DirectorySynced),
    Boundary::Primary(FileStep::TempSynced),
    Boundary::Primary(FileStep::Renamed),
    Boundary::Primary(FileStep::DirectorySynced),
    Boundary::BeforePublication,
];
fn deletion(store: &Store) -> Record {
    let current = store.registry().unwrap();
    let request = Request {
        version: VERSION,
        expected_generation: current.generation,
        change: Change::DeleteData {
            space: current.spaces[0].id.clone(),
            app: "notes".into(),
            collection: current.spaces[0].apps[0].collection.clone(),
            remove: false,
        },
    };
    Record {
        registry: apply(current, "alice", &request, None).unwrap(),
        ..store.current.clone()
    }
}
fn assert_denied(store: &Store, binding: &Binding) {
    assert_eq!(
        authorize(
            store.registry().unwrap(),
            Some("alice"),
            binding,
            Operation::Edit
        ),
        Err(Error::Denied)
    );
    assert_ne!(
        store.registry().unwrap().spaces[0].apps[0].collection,
        binding.collection
    );
}
fn write_private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn child() {
    let Ok(action) = std::env::var("OTRIOS_STORE_TEST_ACTION") else {
        return;
    };
    let path = PathBuf::from(std::env::var_os("OTRIOS_STORE_TEST_PATH").unwrap());
    if action == "claim" {
        assert!(
            Store::open(path).is_err(),
            "a second authority acquired the registry"
        );
        return;
    }
    assert_eq!(action, "delete");
    let index: usize = std::env::var("OTRIOS_STORE_TEST_BOUNDARY")
        .unwrap()
        .parse()
        .unwrap();
    let mut store = Store::open(path).unwrap();
    let next = deletion(&store);
    store
        .publish(next, |boundary| {
            if boundary == BOUNDARIES[index] {
                std::process::exit(73);
            }
        })
        .unwrap();
    panic!("crash boundary was not reached");
}

#[test]
fn exclusive_owner_survives_atomic_replacements_and_missing_lock_never_creates_another_owner() {
    let fixture = Fixture::new();
    let (mut store, _) = fixture.seeded();
    assert!(fixture.child("claim", 0).status.success());
    store.publish(deletion(&store), |_| {}).unwrap();
    assert!(fixture.child("claim", 0).status.success());
    fs::remove_file(suffix(&fixture.path(), ".owner.lock")).unwrap();
    assert!(Store::open(fixture.path()).is_err());
    assert!(!suffix(&fixture.path(), ".owner.lock").exists());
}

#[test]
fn every_publication_boundary_recovers_latest_decision_before_serving_old_primary_or_backup() {
    for (index, boundary) in BOUNDARIES.iter().enumerate() {
        let fixture = Fixture::new();
        let (store, binding) = fixture.seeded();
        let root = store.root_id().to_owned();
        let backup_primary = fs::read(store.path()).unwrap();
        drop(store);
        let child = fixture.child("delete", index);
        assert_eq!(
            child.status.code(),
            Some(73),
            "{boundary:?}: {}",
            String::from_utf8_lossy(&child.stdout)
        );
        let recovered = Store::open(fixture.path()).unwrap();
        assert_eq!(recovered.root_id(), root);
        // The persisted pairing resolves aliases such as macOS /var -> /private/var.
        let storage = fs::canonicalize(fixture.storage()).unwrap();
        assert_eq!(recovered.storage_path(), Some(storage.as_path()));
        if index < 2 {
            assert!(recovered.registry().unwrap().deletions.is_empty());
            assert!(
                authorize(
                    recovered.registry().unwrap(),
                    Some("alice"),
                    &binding,
                    Operation::Edit
                )
                .is_ok()
            );
        } else {
            assert_denied(&recovered, &binding);
            assert_eq!(recovered.registry().unwrap().deletions.len(), 1);
            drop(recovered);
            write_private(&fixture.path(), &backup_primary);
            let restored = Store::open(fixture.path()).unwrap();
            assert_denied(&restored, &binding);
            assert_eq!(
                fs::read(restored.path()).unwrap(),
                fs::read(restored.latest_path()).unwrap()
            );
        }
    }
}

#[test]
fn missing_primary_recovers_but_missing_latest_cannot_fall_back_to_a_legacy_backup() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    let payload = flexbuffers::to_vec(store.registry().unwrap()).unwrap();
    let mut old_backup = b"OTSP2".to_vec();
    old_backup.extend(crate::canvas::hash(&payload));
    old_backup.extend(payload);
    store.publish(deletion(&store), |_| {}).unwrap();
    let latest = store.latest_path().to_path_buf();
    drop(store);
    fs::remove_file(fixture.path()).unwrap();
    let recovered = Store::open(fixture.path()).unwrap();
    assert_denied(&recovered, &binding);
    drop(recovered);
    fs::remove_file(latest).unwrap();
    write_private(&fixture.path(), &old_backup);
    assert!(Store::open(fixture.path()).is_err());
    assert_eq!(fs::read(fixture.path()).unwrap(), old_backup);
}

#[test]
fn uncertain_primary_publication_poisons_until_latest_ledger_recovery() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    fs::remove_file(store.path()).unwrap();
    fs::create_dir(store.path()).unwrap();
    assert!(store.publish(deletion(&store), |_| {}).is_err());
    assert!(matches!(store.registry(), Err(Error::Unavailable)));
    drop(store);
    fs::remove_dir(fixture.path()).unwrap();
    let recovered = Store::open(fixture.path()).unwrap();
    assert_denied(&recovered, &binding);
}

#[test]
fn storage_pairing_rejects_missing_or_substituted_initialized_roots() {
    let fixture = Fixture::new();
    let (mut store, _) = fixture.seeded();
    let marker = fixture.storage().join("spaces.root");
    assert!(store.bind_storage(&fixture.storage()).is_ok());
    fs::remove_file(&marker).unwrap();
    assert!(store.bind_storage(&fixture.storage()).is_err());
    write_private(&marker, b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert!(store.bind_storage(&fixture.storage()).is_err());
    let other = fixture.0.join("another-state");
    fs::create_dir(&other).unwrap();
    write_private(&other.join("spaces.root"), store.root_id().as_bytes());
    assert!(store.bind_storage(&other).is_err());
}

#[test]
fn corrupt_or_foreign_latest_never_rolls_back_and_partial_initial_anchor_can_finish() {
    let fixture = Fixture::new();
    let (store, _) = fixture.seeded();
    let anchor = suffix(store.path(), ".owner.lock");
    let complete_anchor = fs::read(&anchor).unwrap();
    let ledger = fs::read(store.latest_path()).unwrap();
    let latest = store.latest_path().to_path_buf();
    drop(store);
    for length in [0, 1, 5, 20, 36] {
        write_private(&anchor, &complete_anchor[..length]);
        let recovered = Store::open(fixture.path()).unwrap();
        assert_eq!(fs::read(&anchor).unwrap(), complete_anchor);
        drop(recovered);
    }
    let mut corrupted = ledger.clone();
    corrupted[39] ^= 1;
    write_private(&latest, &corrupted);
    assert!(Store::open(fixture.path()).is_err());
    let foreign = Fixture::new();
    let (other, _) = foreign.seeded();
    write_private(&latest, &fs::read(other.latest_path()).unwrap());
    assert!(Store::open(fixture.path()).is_err());
    write_private(&latest, &ledger);
    drop(Store::open(fixture.path()).unwrap());
}

#[cfg(unix)]
#[test]
fn control_metadata_is_private_and_never_follows_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let (store, _) = fixture.seeded();
    let files = [
        store.path().to_path_buf(),
        store.latest_path().to_path_buf(),
        suffix(store.path(), ".owner.lock"),
    ];
    drop(store);
    for file in files {
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o077, 0);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Store::open(fixture.path()).is_err());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let moved = suffix(&file, ".test-moved");
        fs::rename(&file, &moved).unwrap();
        symlink(&moved, &file).unwrap();
        assert!(Store::open(fixture.path()).is_err());
        fs::remove_file(&file).unwrap();
        fs::rename(moved, &file).unwrap();
    }
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o770)).unwrap();
    assert!(Store::open(fixture.path()).is_err());
}
