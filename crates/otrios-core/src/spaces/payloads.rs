// SPDX-License-Identifier: LGPL-3.0-only
//! Reclamation of tombstoned authority-owned payloads. Never delete by hash alone.
use super::*;

impl Store {
    /// The authority supplies producer quiescence under its admission lock.
    /// False means pending. True means these owned paths have been reclaimed;
    /// backups, disconnected replicas and unmanaged exports are separate policies.
    pub fn reclaim_deleted(&self, collection: &str, producers_stopped: bool) -> Result<bool> {
        self.reclaim_at(collection, producers_stopped, |_| {})
    }
    fn reclaim_at(
        &self,
        collection: &str,
        producers_stopped: bool,
        mut step: impl FnMut(usize),
    ) -> Result<bool> {
        if !valid_id(collection) {
            return Err(Error::Invalid);
        }
        if !self
            .registry()?
            .deletions
            .iter()
            .any(|d| d.collection == collection)
        {
            return Err(Error::Denied);
        }
        if !producers_stopped {
            return Ok(false);
        }
        self.remove_orphan_restores()?;
        let root = self.checked_storage()?;
        let paths = [
            (format!("collection-{collection}.merkle"), false),
            (format!("collection-{collection}.merkle.log"), false),
            (format!("collection-{collection}.presentation.flex"), false),
            (format!("collection-{collection}.data"), true),
        ];
        for (index, (name, tree)) in paths.into_iter().enumerate() {
            let path = root.join(name);
            let removed = match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    // The collection directory itself must be owned storage,
                    // never an alias into another collection or operator tree.
                    if tree && !metadata.is_dir() || !tree && !metadata.is_file() {
                        return Err(Error::Invalid);
                    }
                    if tree {
                        check_tree(&path, 0, &mut 4096)?;
                        fs::remove_dir_all(&path).map_err(io_error)?;
                    } else {
                        fs::remove_file(&path).map_err(io_error)?;
                    }
                    true
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(io_error(error)),
            };
            if removed {
                fs::File::open(root)
                    .and_then(|d| d.sync_all())
                    .map_err(io_error)?;
            }
            step(index);
        }
        // Interrupted replacements can leave private temporary files beside
        // the primary. Reclaim this exact CID namespace, including derived caches.
        let prefix = format!("collection-{collection}.");
        let mut extras = vec![];
        let mut more = false;
        for (visited, entry) in fs::read_dir(root).map_err(io_error)?.enumerate() {
            if visited == 65536 {
                return Err(Error::Capacity);
            }
            let entry = entry.map_err(io_error)?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                if extras.len() == 128 {
                    more = true;
                    break;
                }
                extras.push(entry.path());
            }
        }
        let changed = !extras.is_empty();
        for path in extras {
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.is_dir() {
                check_tree(&path, 0, &mut 4096)?;
                fs::remove_dir_all(path).map_err(io_error)?;
            } else if metadata.is_file() {
                fs::remove_file(path).map_err(io_error)?;
            } else {
                return Err(Error::Invalid);
            }
        }
        if changed {
            fs::File::open(root)
                .and_then(|d| d.sync_all())
                .map_err(io_error)?;
        }
        step(4);
        Ok(!more)
    }
}

pub(super) fn check_tree(path: &Path, depth: usize, budget: &mut usize) -> Result<()> {
    if depth > 8 {
        return Err(Error::Capacity);
    }
    for entry in fs::read_dir(path).map_err(io_error)? {
        if *budget == 0 {
            return Err(Error::Capacity);
        }
        *budget -= 1;
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if metadata.is_dir() {
            check_tree(&entry.path(), depth + 1, budget)?;
        } else if !metadata.is_file() && !metadata.file_type().is_symlink() {
            return Err(Error::Invalid);
        }
        // remove_dir_all unlinks child symlinks without traversing their targets.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    struct Fixture {
        root: PathBuf,
        path: PathBuf,
        storage: PathBuf,
        deleted: String,
        live: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "otrios-reclaim-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let path = root.join("registry.flex");
            let storage = root.join("state");
            fs::create_dir(&storage).unwrap();
            let mut store = Store::open(path.clone()).unwrap();
            store.bind_storage(&storage).unwrap();
            let mut registry = empty();
            let mut apply_change = |change: Change, policy: Option<&AppPolicy>| {
                registry = apply(
                    &registry,
                    "alice",
                    &Request {
                        version: VERSION,
                        expected_generation: registry.generation,
                        change,
                    },
                    policy,
                )
                .unwrap();
                registry.clone()
            };
            let first = apply_change(
                Change::Create {
                    name: "Household".into(),
                },
                None,
            )
            .spaces[0]
                .id
                .clone();
            let second = apply_change(
                Change::Create {
                    name: "Work".into(),
                },
                None,
            )
            .spaces[1]
                .id
                .clone();
            let policy = AppPolicy {
                app: "notes".into(),
                schema: "b".repeat(64),
                editable: true,
                comments: false,
                closed_only: false,
            };
            let deleted = apply_change(
                Change::Attach {
                    space: first.clone(),
                    app: "notes".into(),
                },
                Some(&policy),
            )
            .spaces[0]
                .apps[0]
                .collection
                .clone();
            let live = apply_change(
                Change::Attach {
                    space: second,
                    app: "notes".into(),
                },
                Some(&policy),
            )
            .spaces[1]
                .apps[0]
                .collection
                .clone();
            apply_change(
                Change::DeleteData {
                    space: first,
                    app: "notes".into(),
                    remove: false,
                },
                None,
            );
            let next = Record {
                registry,
                ..store.current.clone()
            };
            store.publish(next, |_| {}).unwrap();
            for collection in [&deleted, &live] {
                for suffix in ["merkle", "merkle.log", "presentation.flex"] {
                    fs::write(
                        storage.join(format!("collection-{collection}.{suffix}")),
                        b"confirmed private data",
                    )
                    .unwrap();
                }
                let data = storage.join(format!("collection-{collection}.data"));
                fs::create_dir(&data).unwrap();
                for file in [
                    "chat-primary.journal",
                    "chat-primary.journal.owner.lock",
                    "legacy-chat-import.json",
                    "doc-secondary.merkle",
                    "comments.flex",
                ] {
                    fs::write(data.join(file), b"history and indexes").unwrap();
                }
                fs::write(
                    storage.join(format!("collection-{collection}.123.456.tmp")),
                    b"interrupted checkpoint",
                )
                .unwrap();
            }
            Self {
                root,
                path,
                storage,
                deleted,
                live,
            }
        }
        fn paths(&self, collection: &str) -> Vec<PathBuf> {
            ["merkle", "merkle.log", "presentation.flex", "data"]
                .iter()
                .map(|suffix| {
                    self.storage
                        .join(format!("collection-{collection}.{suffix}"))
                })
                .collect()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn child() {
        let Ok(path) = std::env::var("OTRIOS_RECLAIM_TEST_PATH") else {
            return;
        };
        let store = Store::open(path.into()).unwrap();
        let collection = store.registry().unwrap().deletions[0].collection.clone();
        let boundary: usize = std::env::var("OTRIOS_RECLAIM_TEST_BOUNDARY")
            .unwrap()
            .parse()
            .unwrap();
        store
            .reclaim_at(&collection, true, |index| {
                if index == boundary {
                    std::process::exit(74);
                }
            })
            .unwrap();
        panic!("reclamation boundary was not reached");
    }
    #[test]
    fn quiescence_is_required_and_live_collections_cannot_be_reclaimed() {
        let fixture = Fixture::new();
        let store = Store::open(fixture.path.clone()).unwrap();
        assert_eq!(
            store.reclaim_deleted(&fixture.live, true),
            Err(Error::Denied)
        );
        assert!(!store.reclaim_deleted(&fixture.deleted, false).unwrap());
        assert!(fixture.paths(&fixture.deleted).iter().all(|p| p.exists()));
        assert!(store.reclaim_deleted(&fixture.deleted, true).unwrap());
        assert!(fixture.paths(&fixture.deleted).iter().all(|p| !p.exists()));
        assert!(fixture.paths(&fixture.live).iter().all(|p| p.exists()));
        assert!(
            !fixture
                .storage
                .join(format!("collection-{}.123.456.tmp", fixture.deleted))
                .exists()
        );
        assert_eq!(
            fs::read(
                fixture
                    .storage
                    .join(format!("collection-{}.123.456.tmp", fixture.live))
            )
            .unwrap(),
            b"interrupted checkpoint"
        );
        assert!(store.reclaim_deleted(&fixture.deleted, true).unwrap());
    }
    #[test]
    fn crashes_resume_partial_reclamation_without_resurrecting_or_touching_other_spaces() {
        for boundary in 0..5 {
            let fixture = Fixture::new();
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "spaces::store::payloads::tests::child",
                    "--nocapture",
                ])
                .env("OTRIOS_RECLAIM_TEST_PATH", &fixture.path)
                .env("OTRIOS_RECLAIM_TEST_BOUNDARY", boundary.to_string())
                .output()
                .unwrap();
            assert_eq!(
                child.status.code(),
                Some(74),
                "{}",
                String::from_utf8_lossy(&child.stdout)
            );
            let store = Store::open(fixture.path.clone()).unwrap();
            assert!(
                store
                    .registry()
                    .unwrap()
                    .deletions
                    .iter()
                    .any(|d| d.collection == fixture.deleted)
            );
            assert!(store.reclaim_deleted(&fixture.deleted, true).unwrap());
            assert!(fixture.paths(&fixture.deleted).iter().all(|p| !p.exists()));
            assert!(
                !fixture
                    .storage
                    .join(format!("collection-{}.123.456.tmp", fixture.deleted))
                    .exists()
            );
            for path in fixture.paths(&fixture.live).into_iter().take(3) {
                assert_eq!(fs::read(path).unwrap(), b"confirmed private data");
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn shared_blob_links_and_child_symlinks_preserve_another_collections_bytes() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let store = Store::open(fixture.path.clone()).unwrap();
        let deleted = fixture.paths(&fixture.deleted).pop().unwrap();
        let live = fixture.paths(&fixture.live).pop().unwrap();
        let asset = live.join("asset-same-hash.bin");
        fs::write(&asset, b"shared asset bytes").unwrap();
        fs::hard_link(&asset, deleted.join("asset-same-hash.bin")).unwrap();
        symlink(&live, deleted.join("external-tree")).unwrap();
        assert!(store.reclaim_deleted(&fixture.deleted, true).unwrap());
        assert_eq!(fs::read(asset).unwrap(), b"shared asset bytes");
        assert!(live.join("chat-primary.journal").exists());
    }
    #[test]
    fn many_interrupted_replacements_are_reclaimed_in_bounded_batches() {
        let fixture = Fixture::new();
        let store = Store::open(fixture.path.clone()).unwrap();
        for index in 0..130 {
            fs::write(
                fixture
                    .storage
                    .join(format!("collection-{}.old-{index}.tmp", fixture.deleted)),
                b"old private checkpoint",
            )
            .unwrap();
        }
        assert!(!store.reclaim_deleted(&fixture.deleted, true).unwrap());
        assert!(store.reclaim_deleted(&fixture.deleted, true).unwrap());
        assert!(!fs::read_dir(&fixture.storage).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("collection-{}.", fixture.deleted))
        }));
        assert!(fixture.paths(&fixture.live).iter().all(|p| p.exists()));
    }
    #[cfg(unix)]
    #[test]
    fn collection_directory_alias_never_erases_its_target() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let store = Store::open(fixture.path.clone()).unwrap();
        let deleted = fixture.paths(&fixture.deleted).pop().unwrap();
        let live = fixture.paths(&fixture.live).pop().unwrap();
        fs::remove_dir_all(&deleted).unwrap();
        symlink(&live, &deleted).unwrap();
        assert!(store.reclaim_deleted(&fixture.deleted, true).is_err());
        assert!(live.join("chat-primary.journal").exists());
        fs::remove_file(deleted).unwrap();
        assert!(store.reclaim_deleted(&fixture.deleted, true).unwrap());
    }
}
