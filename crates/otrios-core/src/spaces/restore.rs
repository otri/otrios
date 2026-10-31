// SPDX-License-Identifier: LGPL-3.0-only
//! Offline payload restore with a durable admission barrier. Latest policy wins.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestorePlan {
    version: u32,
    root: String,
    stage: String,
    collections: Vec<SavedCollection>,
    files: Vec<SavedFile>,
    remove: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreStep {
    Staged,
    Fenced,
    FilePromoted,
    FileRemoved,
    StageRemoved,
    Unfenced,
}
fn parents(root: &Path, relative: &str, create: bool) -> Result<PathBuf> {
    if !valid_payload_path(relative) {
        return Err(Error::Invalid);
    }
    let mut parent = root.to_path_buf();
    for part in Path::new(relative)
        .parent()
        .ok_or(Error::Invalid)?
        .components()
    {
        let next = parent.join(part);
        match fs::symlink_metadata(&next) {
            Ok(_) => directory(&next)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                new_directory(&next)?;
                sync_directory(&parent)?;
            }
            Err(error) => return Err(io_error(error)),
        }
        parent = next;
    }
    Ok(parent)
}
fn verify_file(path: &Path, expected: &SavedFile) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    private(&metadata)?;
    if metadata.len() != expected.bytes {
        return Err(Error::Invalid);
    }
    let file = fs::File::open(path).map_err(io_error)?;
    private(&file.metadata().map_err(io_error)?)?;
    let mut input = file.take(expected.bytes + 1);
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let count = input.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        bytes += count as u64;
    }
    if bytes != expected.bytes || digest.finalize().as_slice() != expected.sha256 {
        return Err(Error::Invalid);
    }
    Ok(())
}
impl Store {
    /// Restore only snapshot collections whose identities and schemas still
    /// match latest policy. Deleted/replaced collections never become sources.
    /// A safety snapshot preserves current payloads before any replacement.
    pub fn restore_backup(
        &mut self,
        id: &str,
        now_unix_ms: u64,
        producers_stopped: bool,
    ) -> Result<usize> {
        self.restore_at(id, now_unix_ms, producers_stopped, |_| {})
    }
    fn restore_at(
        &mut self,
        id: &str,
        now_unix_ms: u64,
        producers_stopped: bool,
        mut step: impl FnMut(RestoreStep),
    ) -> Result<usize> {
        self.registry()?;
        if !producers_stopped || now_unix_ms >= expiry(id)? {
            return Err(Error::Unavailable);
        }
        let snapshot = self.read_snapshot(id)?;
        let current = self
            .current
            .registry
            .spaces
            .iter()
            .flat_map(|s| &s.apps)
            .map(|a| (a.collection.clone(), a.policy.schema.clone()))
            .collect::<BTreeMap<_, _>>();
        let deleted = self
            .current
            .registry
            .deletions
            .iter()
            .map(|d| d.collection.as_str())
            .collect::<BTreeSet<_>>();
        let mut selected = BTreeMap::new();
        for collection in snapshot.collections {
            if deleted.contains(collection.collection.as_str()) {
                continue;
            }
            if current.get(&collection.collection) != Some(&collection.schema) {
                return Err(Error::Incompatible);
            }
            selected.insert(collection.collection, collection.schema);
        }
        let files = snapshot
            .files
            .into_iter()
            .filter(|f| file_collection(&f.path).is_ok_and(|c| selected.contains_key(c)))
            .collect::<Vec<_>>();
        let storage = self.checked_storage()?.to_path_buf();
        let incoming = files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<BTreeSet<_>>();
        let remove = snapshot_files(&storage, &selected)?
            .into_iter()
            .filter(|f| !incoming.contains(f.as_str()))
            .collect::<Vec<_>>();
        self.create_backup(now_unix_ms, true)?;
        let stage_id = fresh()?;
        let stage = storage.join(format!(".spaces-restore-{stage_id}"));
        new_directory(&stage)?;
        let result = (|| {
            let source = self.backup_root(false)?.join(id);
            let mut remaining = MAX_BACKUP_BYTES;
            for file in &files {
                parents(&source, &file.path, false)?;
                parents(&stage, &file.path, true)?;
                let copied = copy_file(
                    &source.join(&file.path),
                    &stage.join(&file.path),
                    &mut remaining,
                )?;
                if copied.bytes != file.bytes || copied.sha256 != file.sha256 {
                    return Err(Error::Invalid);
                }
            }
            sync_tree(&stage)?;
            sync_directory(&storage)?;
            step(RestoreStep::Staged);
            let plan = RestorePlan {
                version: 1,
                root: self.current.root.clone(),
                stage: stage_id,
                collections: selected
                    .into_iter()
                    .map(|(collection, schema)| SavedCollection { collection, schema })
                    .collect(),
                files,
                remove,
            };
            let bytes = flexbuffers::to_vec(&plan).map_err(io_error)?;
            if bytes.len() > MAX_MANIFEST_BYTES {
                return Err(Error::Capacity);
            }
            let mut framed = b"OTSR1".to_vec();
            framed.extend(crate::canvas::hash(&bytes));
            framed.extend(bytes);
            self.restoring = true;
            if let Err(error) = write(&suffix(self.path(), ".restore"), &framed) {
                self.failed = true;
                return Err(error);
            }
            step(RestoreStep::Fenced);
            self.resume_restore(&mut step)
        })();
        if result.is_err() && !self.restoring {
            let _ = fs::remove_dir_all(stage);
        }
        result
    }
    /// Called before app admission on every startup, including offline
    /// provisioning. An incomplete restore keeps registry admission unavailable.
    pub fn recover_storage(&mut self) -> Result<usize> {
        if !self.restoring {
            if self.current.storage.is_some() {
                self.remove_orphan_restores()?;
            }
            return Ok(0);
        }
        self.resume_restore(&mut |_| {})
    }
    pub(in crate::spaces::store) fn remove_orphan_restores(&self) -> Result<()> {
        self.registry()?;
        let storage = self.checked_storage()?;
        let mut changed = false;
        for (visited, entry) in fs::read_dir(storage).map_err(io_error)?.enumerate() {
            if visited == 65536 {
                return Err(Error::Capacity);
            }
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(nonce) = name.strip_prefix(".spaces-restore-") else {
                continue;
            };
            if !valid_id(nonce) {
                return Err(Error::Invalid);
            }
            directory(&entry.path())?;
            super::super::payloads::check_tree(&entry.path(), 0, &mut 65536)?;
            fs::remove_dir_all(entry.path()).map_err(io_error)?;
            changed = true;
        }
        if changed {
            sync_directory(storage)?;
        }
        Ok(())
    }
    fn resume_restore(&mut self, step: &mut impl FnMut(RestoreStep)) -> Result<usize> {
        let storage = self.checked_storage_unfenced()?.to_path_buf();
        let marker = suffix(self.path(), ".restore");
        let bytes = read(&marker, MAX_MANIFEST_BYTES + 37, true)?.ok_or(Error::Unavailable)?;
        if bytes.len() < 40
            || &bytes[..5] != b"OTSR1"
            || crate::canvas::hash(&bytes[37..]) != bytes[5..37]
        {
            return Err(Error::Invalid);
        }
        crate::flex_guard::validate(&bytes[37..], 12, MAX_MANIFEST_BYTES, MAX_SPACES * MAX_APPS)
            .map_err(|_| Error::Invalid)?;
        let plan: RestorePlan =
            flexbuffers::from_slice(&bytes[37..]).map_err(|_| Error::Invalid)?;
        if plan.version != 1
            || plan.root != self.current.root
            || !valid_id(&plan.stage)
            || plan.collections.len() > MAX_SPACES * MAX_APPS
            || plan.files.len() > MAX_FILES
            || plan.remove.len() > MAX_FILES
        {
            return Err(Error::Invalid);
        }
        let current = self
            .current
            .registry
            .spaces
            .iter()
            .flat_map(|s| &s.apps)
            .map(|a| (a.collection.as_str(), a.policy.schema.as_str()))
            .collect::<BTreeMap<_, _>>();
        let deleted = self
            .current
            .registry
            .deletions
            .iter()
            .map(|d| d.collection.as_str())
            .collect::<BTreeSet<_>>();
        let mut collections = BTreeSet::new();
        for collection in &plan.collections {
            if !valid_id(&collection.collection)
                || !collections.insert(collection.collection.as_str())
                || !deleted.contains(collection.collection.as_str())
                    && current.get(collection.collection.as_str()).copied()
                        != Some(collection.schema.as_str())
            {
                return Err(Error::Invalid);
            }
        }
        let mut paths = BTreeSet::new();
        let mut total = 0_u64;
        for file in &plan.files {
            if !collections.contains(file_collection(&file.path)?)
                || !paths.insert(file.path.as_str())
                || file.bytes > MAX_FILE_BYTES
                || file.sha256.len() != 32
                || file.path.ends_with(".owner.lock")
            {
                return Err(Error::Invalid);
            }
            total = total.checked_add(file.bytes).ok_or(Error::Capacity)?;
        }
        if total > MAX_BACKUP_BYTES {
            return Err(Error::Capacity);
        }
        for relative in &plan.remove {
            if !collections.contains(file_collection(relative)?)
                || !paths.insert(relative.as_str())
                || relative.ends_with(".owner.lock")
            {
                return Err(Error::Invalid);
            }
        }
        let stage = storage.join(format!(".spaces-restore-{}", plan.stage));
        let stage_exists = match fs::symlink_metadata(&stage) {
            Ok(_) => {
                directory(&stage)?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error(error)),
        };
        let mut restored = 0;
        for file in &plan.files {
            if deleted.contains(file_collection(&file.path)?) {
                continue;
            }
            let target_parent = parents(&storage, &file.path, true)?;
            let target = storage.join(&file.path);
            let source = stage.join(&file.path);
            let staged = stage_exists
                && match fs::symlink_metadata(&source) {
                    Ok(_) => true,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => return Err(io_error(error)),
                };
            if staged {
                parents(&stage, &file.path, false)?;
                verify_file(&source, file)?;
                if let Some(metadata) = read_metadata(&target)? {
                    private(&metadata)?;
                }
                fs::rename(source, &target).map_err(io_error)?;
                sync_directory(&target_parent)?;
            } else {
                verify_file(&target, file)?;
            }
            restored += 1;
            step(RestoreStep::FilePromoted);
        }
        for relative in &plan.remove {
            let target = storage.join(relative);
            if let Some(metadata) = read_metadata(&target)? {
                private(&metadata)?;
                let parent = parents(&storage, relative, false)?;
                fs::remove_file(target).map_err(io_error)?;
                sync_directory(&parent)?;
            }
            step(RestoreStep::FileRemoved);
        }
        if stage_exists {
            super::super::payloads::check_tree(&stage, 0, &mut 65536)?;
            fs::remove_dir_all(stage).map_err(io_error)?;
            sync_directory(&storage)?;
        }
        step(RestoreStep::StageRemoved);
        fs::remove_file(marker).map_err(io_error)?;
        sync_directory(self.path.parent().ok_or(Error::Invalid)?)?;
        self.restoring = false;
        step(RestoreStep::Unfenced);
        Ok(restored)
    }
}
fn read_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
