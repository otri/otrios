// SPDX-License-Identifier: LGPL-3.0-only
//! Exclusively owned registry and latest decision ledger. The ledger is outside
//! rollbackable app payloads; recovery reconciles it before any admission.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

const MAX_STORE_BYTES: usize = MAX_REGISTRY_BYTES + 2 * 1024 * 1024 + 8192;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    root: String,
    storage: Option<String>,
    registry: Registry,
    #[serde(default)]
    legacy_sources: Option<backups::legacy_sources::Originals>,
}
pub struct Store {
    current: Record,
    path: PathBuf,
    latest: PathBuf,
    // Stable inode, including across atomic replacement of both state files.
    _owner: Owner,
    failed: bool,
    restoring: bool,
    maintenance: cleanup::Maintenance,
}
// Release locks explicitly on every exit, including rejected recovery before a
// Store exists. Concurrent fork-before-exec children can briefly inherit a file
// description, so close alone need not release ownership immediately.
struct Owner(fs::File);
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileStep {
    TempSynced,
    Renamed,
    DirectorySynced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    BeforeLatest,
    Latest(FileStep),
    Primary(FileStep),
    BeforePublication,
}
fn io_error(error: impl std::fmt::Display) -> Error {
    Error::Persistence(error.to_string())
}
fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}
fn validate_record(record: &Record) -> Result<()> {
    validate(&record.registry)?;
    if flexbuffers::to_vec(&record.registry)
        .map_err(io_error)?
        .len()
        > MAX_REGISTRY_BYTES
    {
        return Err(Error::Capacity);
    }
    if record.legacy_sources.as_ref().is_some_and(|originals| {
        flexbuffers::to_vec(originals).map_or(true, |bytes| bytes.len() > 2 * 1024 * 1024)
    }) {
        return Err(Error::Capacity);
    }
    if !valid_id(&record.root)
        || record
            .storage
            .as_ref()
            .is_some_and(|s| s.len() > 4096 || !Path::new(s).is_absolute())
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn encode(record: &Record) -> Result<Vec<u8>> {
    validate_record(record)?;
    let payload = flexbuffers::to_vec(record).map_err(io_error)?;
    if payload.len() > MAX_STORE_BYTES - 37 {
        return Err(Error::Capacity);
    }
    // Earlier writers reject this framing rather than ignore the safety ledger.
    let mut bytes = b"OTSP6".to_vec();
    bytes.extend(crate::canvas::hash(&payload));
    bytes.extend(payload);
    Ok(bytes)
}
fn payload<'a>(bytes: &'a [u8], headers: &[&[u8]]) -> Result<&'a [u8]> {
    if bytes.len() < 40
        || bytes.len() > MAX_STORE_BYTES
        || !headers.contains(&&bytes[..5])
        || crate::canvas::hash(&bytes[37..]) != bytes[5..37]
    {
        return Err(Error::Invalid);
    }
    let payload = &bytes[37..];
    crate::flex_guard::validate(payload, 18, MAX_STORE_BYTES, MAX_SPACES * MAX_APPS)
        .map_err(|_| Error::Invalid)?;
    Ok(payload)
}
fn decode(bytes: &[u8]) -> Result<Record> {
    let record: Record =
        flexbuffers::from_slice(payload(bytes, &[b"OTSP3", b"OTSP4", b"OTSP5", b"OTSP6"])?)
            .map_err(|_| Error::Invalid)?;
    validate_record(&record)?;
    Ok(record)
}
fn current_framing(bytes: &[u8]) -> bool {
    bytes.starts_with(b"OTSP3")
        || bytes.starts_with(b"OTSP4")
        || bytes.starts_with(b"OTSP5")
        || bytes.starts_with(b"OTSP6")
}
fn legacy(bytes: &[u8]) -> Result<Registry> {
    let registry: Registry = flexbuffers::from_slice(payload(bytes, &[b"OTSP1", b"OTSP2"])?)
        .map_err(|_| Error::Invalid)?;
    validate(&registry)?;
    Ok(registry)
}
fn private(metadata: &fs::Metadata) -> Result<()> {
    if !metadata.is_file() {
        return Err(Error::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_dir() {
        return Err(Error::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
fn read(path: &Path, limit: usize, require_private: bool) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(Error::Invalid);
    }
    if require_private {
        private(&metadata)?;
    }
    let file = fs::File::open(path).map_err(io_error)?;
    if require_private {
        private(&file.metadata().map_err(io_error)?)?;
    }
    let mut bytes = vec![];
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err(Error::Capacity);
    }
    Ok(Some(bytes))
}
impl Store {
    /// Maintenance never initializes an apparently empty authority root.
    pub fn open_existing(path: PathBuf) -> Result<Self> {
        if read(&suffix(&path, ".latest"), MAX_STORE_BYTES, true)?.is_none() {
            return Err(Error::Unavailable);
        }
        Self::open(path)
    }
    /// Upgrade legacy registries once. An initialized root cannot be recovered
    /// from an old primary alone when its latest decision ledger is missing.
    pub fn open(path: PathBuf) -> Result<Self> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(Error::Invalid)?;
        directory(parent)?;
        let lock_path = suffix(&path, ".owner.lock");
        // Only a trusted operator can mutate this containing directory. Refuse
        // symlinks and non-files before opening any control metadata.
        let anchor = read(&lock_path, 37, true)?;
        let latest = suffix(&path, ".latest");
        if anchor.is_none() && fs::symlink_metadata(&latest).is_ok() {
            // Never replace the stable inode underneath a potentially live owner.
            return Err(Error::Unavailable);
        }
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut owner = Owner(options.open(&lock_path).map_err(io_error)?);
        private(&owner.0.metadata().map_err(io_error)?)?;
        owner.0.try_lock().map_err(io_error)?;
        // Re-read after acquiring ownership: a preceding owner may have finished
        // initialization between our inspection and lock acquisition.
        let anchor = read(&lock_path, 37, true)?.or(anchor).unwrap_or_default();
        let root = if anchor.len() < 37 {
            None
        } else {
            if &anchor[..5] != b"OTSO1" {
                return Err(Error::Invalid);
            }
            let root = std::str::from_utf8(&anchor[5..])
                .map_err(|_| Error::Invalid)?
                .to_owned();
            if !valid_id(&root) {
                return Err(Error::Invalid);
            }
            Some(root)
        };
        let primary = read(&path, MAX_STORE_BYTES, false)?;
        if primary.as_deref().is_some_and(current_framing) {
            private(&fs::symlink_metadata(&path).map_err(io_error)?)?;
        }
        let ledger = read(&latest, MAX_STORE_BYTES, true)?;
        let current = if let Some(bytes) = &ledger {
            let current = decode(bytes)?;
            let expected_anchor = [b"OTSO1".as_slice(), current.root.as_bytes()].concat();
            if !expected_anchor.starts_with(&anchor) {
                return Err(Error::Invalid);
            }
            if root.as_ref().is_some_and(|root| root != &current.root) {
                return Err(Error::Invalid);
            }
            if let Some(bytes) = &primary {
                let previous = if current_framing(bytes) {
                    let previous = decode(bytes)?;
                    if previous.root != current.root
                        || previous
                            .storage
                            .as_ref()
                            .is_some_and(|s| Some(s) != current.storage.as_ref())
                    {
                        return Err(Error::Invalid);
                    }
                    previous.registry
                } else {
                    legacy(bytes)?
                };
                if previous.generation > current.registry.generation
                    || previous.generation == current.registry.generation
                        && previous != current.registry
                {
                    return Err(Error::Invalid);
                }
            }
            current
        } else {
            if !anchor.is_empty() || primary.as_deref().is_some_and(current_framing) {
                return Err(Error::Unavailable);
            }
            Record {
                root: fresh()?,
                storage: None,
                legacy_sources: None,
                registry: primary
                    .as_ref()
                    .map(|b| legacy(b))
                    .transpose()?
                    .unwrap_or_else(empty),
            }
        };
        let bytes = encode(&current)?;
        if ledger.as_ref() != Some(&bytes) {
            write(&latest, &bytes)?;
        }
        if root.is_none() {
            owner.0.rewind().map_err(io_error)?;
            owner
                .0
                .write_all(b"OTSO1")
                .and_then(|()| owner.0.write_all(current.root.as_bytes()))
                .map_err(io_error)?;
            owner
                .0
                .set_len(37)
                .and_then(|()| owner.0.sync_all())
                .map_err(io_error)?;
        }
        if primary.as_ref() != Some(&bytes) {
            write(&path, &bytes)?;
        }
        let restoring = match fs::symlink_metadata(suffix(&path, ".restore")) {
            Ok(metadata) => {
                private(&metadata)?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error(error)),
        };
        Ok(Self {
            current,
            path,
            latest,
            _owner: owner,
            failed: false,
            restoring,
            maintenance: Default::default(),
        })
    }
    pub fn registry(&self) -> Result<&Registry> {
        if self.failed || self.restoring {
            Err(Error::Unavailable)
        } else {
            Ok(&self.current.registry)
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn latest_path(&self) -> &Path {
        &self.latest
    }
    pub fn root_id(&self) -> &str {
        &self.current.root
    }
    pub fn storage_path(&self) -> Option<&Path> {
        self.current.storage.as_deref().map(Path::new)
    }
    fn checked_storage(&self) -> Result<&Path> {
        self.registry()?;
        self.checked_storage_unfenced()
    }
    fn checked_storage_unfenced(&self) -> Result<&Path> {
        if self.failed {
            return Err(Error::Unavailable);
        }
        let path = self.storage_path().ok_or(Error::Unavailable)?;
        directory(path)?;
        if read(&path.join("spaces.root"), 32, true)?.as_deref()
            != Some(self.current.root.as_bytes())
        {
            return Err(Error::Unavailable);
        }
        Ok(path)
    }
    /// Pair an authority's app storage with its safety root once. Missing or
    /// substituted initialized storage must not silently become empty data.
    pub fn bind_storage(&mut self, path: &Path) -> Result<()> {
        if self.failed {
            return Err(Error::Unavailable);
        }
        directory(path)?;
        let path = fs::canonicalize(path).map_err(io_error)?;
        let marker = path.join("spaces.root");
        let existing = read(&marker, 32, true)?;
        if existing
            .as_deref()
            .is_some_and(|b| b != self.current.root.as_bytes())
        {
            return Err(Error::Invalid);
        }
        if let Some(expected) = self.storage_path() {
            return if expected == path && existing.is_some() {
                Ok(())
            } else {
                Err(Error::Unavailable)
            };
        }
        self.registry()?;
        if existing.is_none()
            && let Err(error) = write(&marker, self.current.root.as_bytes())
        {
            self.failed = true;
            return Err(error);
        }
        let mut next = self.current.clone();
        next.storage = Some(path.to_str().ok_or(Error::Invalid)?.into());
        self.publish(next, |_| {})
    }
    pub fn commit(
        &mut self,
        actor: &str,
        request: &Request,
        installed: Option<&AppPolicy>,
    ) -> Result<&Registry> {
        let registry = apply(self.registry()?, actor, request, installed)?;
        let next = Record {
            registry,
            ..self.current.clone()
        };
        self.publish(next, |_| {})?;
        self.registry()
    }
    pub fn commit_catalog(
        &mut self,
        actor: &str,
        request: &Request,
        installed: &[AppPolicy],
    ) -> Result<&Registry> {
        let registry = apply_catalog(self.registry()?, actor, request, installed)?;
        self.publish(
            Record {
                registry,
                ..self.current.clone()
            },
            |_| {},
        )?;
        self.registry()
    }
    fn publish(&mut self, next: Record, mut boundary: impl FnMut(Boundary)) -> Result<()> {
        self.registry()?;
        let bytes = encode(&next)?;
        boundary(Boundary::BeforeLatest);
        let result = (|| {
            write_at(&self.latest, &bytes, |step| {
                boundary(Boundary::Latest(step))
            })?;
            write_at(&self.path, &bytes, |step| boundary(Boundary::Primary(step)))
        })();
        if let Err(error) = result {
            self.failed = true;
            return Err(error);
        }
        boundary(Boundary::BeforePublication);
        self.current = next;
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

#[path = "backups.rs"]
mod backups;
#[path = "payloads.rs"]
mod payloads;
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    write_at(path, bytes, |_| {})
}
fn write_at(path: &Path, bytes: &[u8], mut step: impl FnMut(FileStep)) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(Error::Invalid)?;
    // Provision the containing private directory separately. Never create an
    // apparently fresh hierarchy after an operator accidentally removes a mount.
    let temp = parent.join(format!(".spaces-{}.tmp", fresh()?));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp).map_err(io_error)?;
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        step(FileStep::TempSynced);
        fs::rename(&temp, path).map_err(io_error)?;
        step(FileStep::Renamed);
        fs::File::open(parent)
            .and_then(|d| d.sync_all())
            .map_err(io_error)?;
        step(FileStep::DirectorySynced);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[path = "cleanup.rs"]
mod cleanup;
