// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded authority-owned snapshots; control decisions are never rolled back.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_BACKUPS: usize = 16;
const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedCollection {
    collection: String,
    schema: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedFile {
    path: String,
    bytes: u64,
    sha256: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    root: String,
    id: String,
    collections: Vec<SavedCollection>,
    files: Vec<SavedFile>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackupStep {
    FileCopied,
    ManifestPublished,
    TreeSynced,
    Published,
}
fn backup_time(id: &str) -> Result<u64> {
    if id.len() != 49
        || id.as_bytes()[16] != b'-'
        || !valid_id(&id[17..])
        || !id.as_bytes()[..16]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(Error::Invalid);
    }
    u64::from_str_radix(&id[..16], 16).map_err(|_| Error::Invalid)
}
fn expiry(id: &str) -> Result<u64> {
    backup_time(id)?
        .checked_add(RETENTION_MS)
        .ok_or(Error::Invalid)
}
fn new_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(io_error)
}
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)
        .and_then(|d| d.sync_all())
        .map_err(io_error)
}
fn valid_payload_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    !parts.is_empty()
        && parts.len() <= 9
        && path.len() <= 1024
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part.len() <= 128
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        && parts[0].strip_prefix("collection-").is_some_and(|name| {
            name.len() > 33 && valid_id(&name[..32]) && name.as_bytes()[32] == b'.'
        })
}
fn file_collection(path: &str) -> Result<&str> {
    if !valid_payload_path(path) {
        return Err(Error::Invalid);
    }
    Ok(&path[11..43])
}
fn copy_file(source: &Path, destination: &Path, remaining: &mut u64) -> Result<SavedFile> {
    let metadata = fs::symlink_metadata(source).map_err(io_error)?;
    private(&metadata)?;
    if metadata.len() > MAX_FILE_BYTES || metadata.len() > *remaining {
        return Err(Error::Capacity);
    }
    let mut input = fs::File::open(source).map_err(io_error)?;
    private(&input.metadata().map_err(io_error)?)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(destination).map_err(io_error)?;
    let mut sha = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let count = input.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        bytes = bytes.checked_add(count as u64).ok_or(Error::Capacity)?;
        if bytes > metadata.len() {
            return Err(Error::Conflict);
        }
        output.write_all(&buffer[..count]).map_err(io_error)?;
        sha.update(&buffer[..count]);
    }
    if bytes != metadata.len() {
        return Err(Error::Conflict);
    }
    output.sync_all().map_err(io_error)?;
    *remaining -= bytes;
    Ok(SavedFile {
        path: String::new(),
        bytes,
        sha256: sha.finalize().to_vec(),
    })
}
fn snapshot_files(root: &Path, collections: &BTreeMap<String, String>) -> Result<Vec<String>> {
    fn walk(
        root: &Path,
        path: &Path,
        files: &mut Vec<String>,
        visits: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth > 8 {
            return Err(Error::Capacity);
        }
        directory(path)?;
        for entry in fs::read_dir(path).map_err(io_error)? {
            if *visits == 65536 {
                return Err(Error::Capacity);
            }
            *visits += 1;
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.is_dir() {
                walk(root, &path, files, visits, depth + 1)?;
            } else {
                private(&metadata)?;
                if entry.file_name().to_string_lossy().ends_with(".owner.lock") {
                    continue;
                }
                if files.len() == MAX_FILES {
                    return Err(Error::Capacity);
                }
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| Error::Invalid)?
                    .to_str()
                    .ok_or(Error::Invalid)?
                    .to_owned();
                if !valid_payload_path(&relative) {
                    return Err(Error::Invalid);
                }
                files.push(relative);
            }
        }
        Ok(())
    }
    let mut files = vec![];
    let mut visits = 0;
    for entry in fs::read_dir(root).map_err(io_error)? {
        if visits == 65536 {
            return Err(Error::Capacity);
        }
        visits += 1;
        let entry = entry.map_err(io_error)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::Invalid)?;
        if !valid_payload_path(&name) || !collections.contains_key(file_collection(&name)?) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if metadata.is_dir() {
            walk(root, &entry.path(), &mut files, &mut visits, 1)?;
        } else {
            private(&metadata)?;
            if name.ends_with(".owner.lock") {
                continue;
            }
            if files.len() == MAX_FILES {
                return Err(Error::Capacity);
            }
            files.push(name);
        }
    }
    files.sort();
    Ok(files)
}
fn sync_tree(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if fs::symlink_metadata(entry.path())
            .map_err(io_error)?
            .is_dir()
        {
            sync_tree(&entry.path())?;
        }
    }
    sync_directory(path)
}
impl Store {
    fn backup_root(&self, create: bool) -> Result<PathBuf> {
        self.registry()?;
        let path = suffix(&self.path, ".backups");
        let marker = suffix(&self.path, ".backups.root");
        let exists = match fs::symlink_metadata(&path) {
            Ok(_) => {
                directory(&path)?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error(error)),
        };
        match read(&marker, 32, true)? {
            Some(root) if root == self.current.root.as_bytes() => (),
            Some(_) => return Err(Error::Invalid),
            None if create => {
                if exists && fs::read_dir(&path).map_err(io_error)?.next().is_some() {
                    return Err(Error::Invalid);
                }
                write(&marker, self.current.root.as_bytes())?;
            }
            None if !exists => return Ok(path),
            None => return Err(Error::Invalid),
        }
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                directory(&path)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                new_directory(&path)?;
                sync_directory(path.parent().ok_or(Error::Invalid)?)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(e)),
        }
        Ok(path)
    }
    /// Safe during live service under the authority lock. Names encode immutable
    /// creation time, so a corrupt manifest cannot pin backup bytes indefinitely.
    pub fn prune_backups(&mut self, now_unix_ms: u64) -> Result<usize> {
        let root = self.backup_root(false)?;
        if !root.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        for (index, entry) in fs::read_dir(&root).map_err(io_error)?.enumerate() {
            if index == 1024 {
                return Err(Error::Capacity);
            }
            let entry = entry.map_err(io_error)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            let (id, pending) = name
                .strip_prefix(".pending-")
                .map_or((name.as_str(), false), |id| (id, true));
            let expires = expiry(id)?;
            if !fs::symlink_metadata(entry.path())
                .map_err(io_error)?
                .is_dir()
            {
                return Err(Error::Invalid);
            }
            if pending || now_unix_ms >= expires {
                super::payloads::check_tree(&entry.path(), 0, &mut 65536)?;
                fs::remove_dir_all(entry.path()).map_err(io_error)?;
                removed += 1;
            }
        }
        if removed > 0 {
            sync_directory(&root)?;
        }
        Ok(removed)
    }
    /// Offline snapshot. The caller must have stopped its own producers; the
    /// Store's OS lock excludes another authority process. No user/control state
    /// is copied into clients or signed code replicas.
    pub fn create_backup(&mut self, now_unix_ms: u64, producers_stopped: bool) -> Result<String> {
        self.backup_at(now_unix_ms, producers_stopped, |_| {})
    }
    fn backup_at(
        &mut self,
        now_unix_ms: u64,
        producers_stopped: bool,
        mut step: impl FnMut(BackupStep),
    ) -> Result<String> {
        if !producers_stopped {
            return Err(Error::Unavailable);
        }
        self.checked_storage()?;
        let root = self.backup_root(true)?;
        self.prune_backups(now_unix_ms)?;
        let storage = self.checked_storage()?;
        let mut count = 0;
        let mut total = 0_u64;
        for entry in fs::read_dir(&root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            backup_time(&name)?;
            count += 1;
            let snapshot = self.read_snapshot(&name)?;
            for file in snapshot.files {
                total = total.checked_add(file.bytes).ok_or(Error::Capacity)?;
            }
        }
        if count >= MAX_BACKUPS || total > MAX_TOTAL_BYTES {
            return Err(Error::Capacity);
        }
        let collections = self
            .registry()?
            .spaces
            .iter()
            .flat_map(|s| &s.apps)
            .map(|a| (a.collection.clone(), a.policy.schema.clone()))
            .collect::<BTreeMap<_, _>>();
        let files = snapshot_files(storage, &collections)?;
        let id = format!("{now_unix_ms:016x}-{}", fresh()?);
        expiry(&id)?;
        let staging = root.join(format!(".pending-{id}"));
        new_directory(&staging)?;
        let result = (|| {
            let mut remaining = MAX_BACKUP_BYTES.min(MAX_TOTAL_BYTES - total);
            let mut copied = vec![];
            for relative in files {
                let destination = staging.join(&relative);
                let mut parent = staging.clone();
                for part in Path::new(&relative)
                    .parent()
                    .ok_or(Error::Invalid)?
                    .components()
                {
                    parent.push(part);
                    if parent.exists() {
                        directory(&parent)?;
                    } else {
                        new_directory(&parent)?;
                    }
                }
                let mut file = copy_file(&storage.join(&relative), &destination, &mut remaining)?;
                file.path = relative;
                copied.push(file);
                step(BackupStep::FileCopied);
            }
            let snapshot = Snapshot {
                version: 1,
                root: self.current.root.clone(),
                id: id.clone(),
                collections: collections
                    .into_iter()
                    .map(|(collection, schema)| SavedCollection { collection, schema })
                    .collect(),
                files: copied,
            };
            let bytes = flexbuffers::to_vec(&snapshot).map_err(io_error)?;
            if bytes.len() > MAX_MANIFEST_BYTES {
                return Err(Error::Capacity);
            }
            let mut framed = b"OTSB1".to_vec();
            framed.extend(crate::canvas::hash(&bytes));
            framed.extend(bytes);
            write(&staging.join("manifest.flex"), &framed)?;
            step(BackupStep::ManifestPublished);
            sync_tree(&staging)?;
            step(BackupStep::TreeSynced);
            fs::rename(&staging, root.join(&id)).map_err(io_error)?;
            sync_directory(&root)?;
            step(BackupStep::Published);
            Ok(id)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }
    /// Only completely published snapshots. Pending copies cannot be restored.
    pub fn backup_ids(&self) -> Result<Vec<String>> {
        let root = self.backup_root(false)?;
        if !root.exists() {
            return Ok(vec![]);
        }
        let mut ids = vec![];
        for (index, entry) in fs::read_dir(root).map_err(io_error)?.enumerate() {
            if index == 1024 {
                return Err(Error::Capacity);
            }
            let entry = entry.map_err(io_error)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            if let Some(id) = name.strip_prefix(".pending-") {
                backup_time(id)?;
                continue;
            }
            self.read_snapshot(&name)?;
            ids.push(name);
        }
        ids.sort();
        Ok(ids)
    }
    fn read_snapshot(&self, id: &str) -> Result<Snapshot> {
        expiry(id)?;
        let root = self.backup_root(false)?;
        let directory = root.join(id);
        super::directory(&directory)?;
        let bytes = read(
            &directory.join("manifest.flex"),
            MAX_MANIFEST_BYTES + 37,
            true,
        )?
        .ok_or(Error::Unavailable)?;
        if bytes.len() < 40
            || &bytes[..5] != b"OTSB1"
            || crate::canvas::hash(&bytes[37..]) != bytes[5..37]
        {
            return Err(Error::Invalid);
        }
        crate::flex_guard::validate(&bytes[37..], 12, MAX_MANIFEST_BYTES, MAX_SPACES * MAX_APPS)
            .map_err(|_| Error::Invalid)?;
        let snapshot: Snapshot =
            flexbuffers::from_slice(&bytes[37..]).map_err(|_| Error::Invalid)?;
        if snapshot.version != 1
            || snapshot.root != self.current.root
            || snapshot.id != id
            || snapshot.collections.len() > MAX_SPACES * MAX_APPS
            || snapshot.files.len() > MAX_FILES
        {
            return Err(Error::Invalid);
        }
        let mut collections = BTreeMap::new();
        for collection in &snapshot.collections {
            if !valid_id(&collection.collection)
                || collection.schema.len() != 64
                || !collection
                    .schema
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                || collections
                    .insert(collection.collection.as_str(), &collection.schema)
                    .is_some()
            {
                return Err(Error::Invalid);
            }
        }
        let mut paths = BTreeSet::new();
        let mut total = 0_u64;
        for file in &snapshot.files {
            if !collections.contains_key(file_collection(&file.path)?)
                || !paths.insert(&file.path)
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
        Ok(snapshot)
    }
}

#[path = "legacy_sources.rs"]
pub(super) mod legacy_sources;
#[path = "restore.rs"]
mod restore;
#[cfg(test)]
#[path = "backups_tests.rs"]
mod tests;
