// SPDX-License-Identifier: LGPL-3.0-only
use super::super::tests::Fixture;
use super::*;
use std::process::Command;

const NOW: u64 = 1_800_000_000_000;
pub(super) fn data(fixture: &Fixture, binding: &Binding) {
    write(
        &fixture
            .storage()
            .join(format!("collection-{}.merkle", binding.collection)),
        b"confirmed document",
    )
    .unwrap();
    let directory = fixture
        .storage()
        .join(format!("collection-{}.data", binding.collection));
    new_directory(&directory).unwrap();
    write(
        &directory.join("chat-primary.journal"),
        b"confirmed history",
    )
    .unwrap();
    write(
        &directory.join("chat-primary.journal.owner.lock"),
        b"lock metadata",
    )
    .unwrap();
    write(&directory.join("asset-digest.bin"), b"private asset bytes").unwrap();
}
#[test]
fn snapshot_retains_schema_and_assets_but_excludes_control_locks_and_deleted_collections() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    data(&fixture, &binding);
    assert_eq!(store.create_backup(NOW, false), Err(Error::Unavailable));
    assert!(store.backup_ids().unwrap().is_empty());
    let id = store.create_backup(NOW, true).unwrap();
    assert_eq!(
        store.backup_ids().unwrap().as_slice(),
        std::slice::from_ref(&id)
    );
    let snapshot = store.read_snapshot(&id).unwrap();
    assert_eq!(snapshot.collections.len(), 1);
    assert_eq!(snapshot.collections[0].collection, binding.collection);
    assert_eq!(snapshot.collections[0].schema, binding.schema);
    assert_eq!(snapshot.files.len(), 3);
    for file in &snapshot.files {
        let copied =
            fs::read(store.backup_root(false).unwrap().join(&id).join(&file.path)).unwrap();
        assert_eq!(copied.len() as u64, file.bytes);
        assert_eq!(crate::canvas::hash(&copied), file.sha256);
        assert!(!file.path.ends_with(".owner.lock"));
        assert_eq!(
            copied,
            fs::read(fixture.storage().join(&file.path)).unwrap()
        );
    }
    assert_eq!(store.prune_backups(NOW + RETENTION_MS - 1).unwrap(), 0);
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
    let replacement = store.create_backup(NOW + 1, true).unwrap();
    let snapshot = store.read_snapshot(&replacement).unwrap();
    assert!(snapshot.files.is_empty());
    assert!(
        snapshot
            .collections
            .iter()
            .all(|c| c.collection != binding.collection)
    );
    assert_eq!(store.prune_backups(NOW + RETENTION_MS).unwrap(), 1);
    assert_eq!(store.backup_ids().unwrap(), [replacement]);
}
#[test]
fn corrupt_manifests_cannot_pin_expired_bytes_and_foreign_roots_are_rejected() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    data(&fixture, &binding);
    let id = store.create_backup(NOW, true).unwrap();
    let snapshot = store.backup_root(false).unwrap().join(&id);
    write(&snapshot.join("manifest.flex"), b"damaged manifest").unwrap();
    assert!(store.backup_ids().is_err());
    assert_eq!(store.prune_backups(NOW + RETENTION_MS).unwrap(), 1);
    assert!(!snapshot.exists());
    write(
        &suffix(store.path(), ".backups.root"),
        b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    assert!(store.create_backup(NOW, true).is_err());
    assert_eq!(
        fs::read(
            fixture
                .storage()
                .join(format!("collection-{}.merkle", binding.collection))
        )
        .unwrap(),
        b"confirmed document"
    );
}
#[cfg(unix)]
#[test]
fn source_symlinks_and_backup_directory_aliases_cannot_read_or_erase_other_trees() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    data(&fixture, &binding);
    let data = fixture
        .storage()
        .join(format!("collection-{}.data", binding.collection));
    symlink(
        fixture.0.join("registry.flex"),
        data.join("private-control-link"),
    )
    .unwrap();
    assert!(store.create_backup(NOW, true).is_err());
    fs::remove_file(data.join("private-control-link")).unwrap();
    let id = store.create_backup(NOW, true).unwrap();
    let root = store.backup_root(false).unwrap();
    let moved = fixture.0.join("backups-moved");
    fs::rename(&root, &moved).unwrap();
    symlink(&moved, &root).unwrap();
    assert!(store.prune_backups(NOW + RETENTION_MS).is_err());
    assert!(moved.join(&id).join("manifest.flex").exists());
}
#[test]
fn snapshot_count_and_file_size_limits_fail_without_changing_source_data() {
    let fixture = Fixture::new();
    let (mut store, binding) = fixture.seeded();
    data(&fixture, &binding);
    for offset in 0..MAX_BACKUPS {
        store.create_backup(NOW + offset as u64, true).unwrap();
    }
    assert_eq!(store.create_backup(NOW, true), Err(Error::Capacity));
    assert_eq!(store.backup_ids().unwrap().len(), MAX_BACKUPS);
    store
        .prune_backups(NOW + RETENTION_MS + MAX_BACKUPS as u64)
        .unwrap();
    let oversized = fixture
        .storage()
        .join(format!("collection-{}.merkle", binding.collection));
    fs::OpenOptions::new()
        .write(true)
        .open(&oversized)
        .unwrap()
        .set_len(MAX_FILE_BYTES + 1)
        .unwrap();
    assert_eq!(store.create_backup(NOW, true), Err(Error::Capacity));
    assert!(store.backup_ids().unwrap().is_empty());
    assert_eq!(fs::metadata(oversized).unwrap().len(), MAX_FILE_BYTES + 1);
}
#[test]
fn child() {
    let Ok(path) = std::env::var("OTRIOS_BACKUP_TEST_PATH") else {
        return;
    };
    let mut store = Store::open(path.into()).unwrap();
    let index: usize = std::env::var("OTRIOS_BACKUP_TEST_BOUNDARY")
        .unwrap()
        .parse()
        .unwrap();
    let boundary = [
        BackupStep::FileCopied,
        BackupStep::ManifestPublished,
        BackupStep::TreeSynced,
        BackupStep::Published,
    ][index];
    store
        .backup_at(NOW, true, |step| {
            if step == boundary {
                std::process::exit(75);
            }
        })
        .unwrap();
    panic!("backup publication boundary was not reached");
}
#[test]
fn interrupted_snapshots_are_unavailable_until_publication_and_partial_copies_are_reclaimed() {
    for index in 0..4 {
        let fixture = Fixture::new();
        let (store, binding) = fixture.seeded();
        data(&fixture, &binding);
        drop(store);
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "spaces::store::backups::tests::child",
                "--nocapture",
            ])
            .env("OTRIOS_BACKUP_TEST_PATH", fixture.path())
            .env("OTRIOS_BACKUP_TEST_BOUNDARY", index.to_string())
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(75),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        let mut recovered = Store::open(fixture.path()).unwrap();
        assert_eq!(
            recovered.backup_ids().unwrap().len(),
            usize::from(index == 3)
        );
        assert_eq!(
            recovered.prune_backups(NOW).unwrap(),
            usize::from(index != 3)
        );
        assert_eq!(
            fs::read(
                fixture
                    .storage()
                    .join(format!("collection-{}.merkle", binding.collection))
            )
            .unwrap(),
            b"confirmed document"
        );
        assert_eq!(
            recovered.prune_backups(NOW + RETENTION_MS).unwrap(),
            usize::from(index == 3)
        );
        assert!(recovered.backup_ids().unwrap().is_empty());
    }
}
