// SPDX-License-Identifier: LGPL-3.0-only
//! Explicit migration-original retention. These originals never grant admission.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Original {
    name: String,
    bytes: u64,
    sha256: Vec<u8>,
    collections: Vec<String>,
    removed: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::spaces::store) struct Originals {
    version: u32,
    root: String,
    until_unix_ms: Option<u64>,
    files: Vec<Original>,
}
struct OriginalOwner(Option<fs::File>);
impl Drop for OriginalOwner {
    fn drop(&mut self) {
        if let Some(file) = &self.0 {
            let _ = file.unlock();
        }
    }
}
fn original_owner(path: &Path, name: &str) -> Result<OriginalOwner> {
    if name != "brass-relay.journal" {
        return Ok(OriginalOwner(None));
    }
    let lock = path.with_extension("journal.owner.lock");
    private(&fs::symlink_metadata(&lock).map_err(io_error)?)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock)
        .map_err(io_error)?;
    file.try_lock().map_err(io_error)?;
    Ok(OriginalOwner(Some(file)))
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with("collection-")
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
        && (name == "brass-relay.journal"
            || name.ends_with(".merkle")
            || name.ends_with(".merkle.log")
            || name.ends_with(".presentation.flex"))
}
fn fingerprint(path: &Path) -> Result<(u64, Vec<u8>)> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    private(&metadata)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Capacity);
    }
    let file = fs::File::open(path).map_err(io_error)?;
    private(&file.metadata().map_err(io_error)?)?;
    let mut file = file.take(MAX_FILE_BYTES + 1);
    let mut sha = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
        bytes += count as u64;
    }
    if bytes != metadata.len() {
        return Err(Error::Conflict);
    }
    Ok((bytes, sha.finalize().to_vec()))
}
fn temporary_stem(name: &str) -> Option<&str> {
    if let Some(tail) = name.strip_suffix(".compact") {
        let (stem, nonce) = tail.rsplit_once('.')?;
        return (stem == "brass-relay"
            && nonce.len() == 16
            && nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then_some(stem);
    }
    let (prefix, nonce) = name.strip_suffix(".tmp")?.rsplit_once('.')?;
    let (stem, pid) = prefix.rsplit_once('.')?;
    (!pid.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && pid.parse::<u32>().is_ok_and(|p| p > 0)
        && !nonce.is_empty()
        && nonce.bytes().all(|b| b.is_ascii_digit())
        && nonce.parse::<u128>().is_ok())
    .then_some(stem)
}
fn prune_original_temporaries(root: &Path, originals: &Originals) -> Result<(usize, bool)> {
    let stems = originals
        .files
        .iter()
        .filter_map(|f| Path::new(&f.name).file_stem().and_then(|s| s.to_str()))
        .collect::<BTreeSet<_>>();
    let mut paths = vec![];
    let mut complete = true;
    for (index, entry) in fs::read_dir(root).map_err(io_error)?.enumerate() {
        if index == 65536 {
            return Err(Error::Capacity);
        }
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        if name
            .to_str()
            .and_then(temporary_stem)
            .is_some_and(|stem| stems.contains(stem))
        {
            if paths.len() == 128 {
                complete = false;
                break;
            }
            private(&fs::symlink_metadata(entry.path()).map_err(io_error)?)?;
            paths.push(entry.path());
        }
    }
    let removed = paths.len();
    for path in paths {
        fs::remove_file(path).map_err(io_error)?;
    }
    if removed > 0 {
        sync_directory(root)?;
    }
    Ok((removed, complete))
}
impl Store {
    fn originals(&self) -> Result<Originals> {
        let registry = self.registry()?;
        let Some(originals) = self.current.legacy_sources.clone() else {
            return Ok(Originals {
                version: 1,
                root: self.current.root.clone(),
                until_unix_ms: None,
                files: vec![],
            });
        };
        if originals.version != 1
            || originals.root != self.current.root
            || originals.files.len() > 1024
        {
            return Err(Error::Invalid);
        }
        let known = registry
            .spaces
            .iter()
            .flat_map(|s| &s.apps)
            .map(|a| a.collection.as_str())
            .chain(registry.retained.iter().map(|d| d.collection.as_str()))
            .chain(registry.deletions.iter().map(|d| d.collection.as_str()))
            .collect::<BTreeSet<_>>();
        let mut names = BTreeSet::new();
        for file in &originals.files {
            if !valid_name(&file.name)
                || !names.insert(&file.name)
                || file.bytes > MAX_FILE_BYTES
                || file.sha256.len() != 32
                || file.collections.is_empty()
                || file.collections.len() > MAX_SPACES * MAX_APPS
                || file.collections.iter().any(|c| !known.contains(c.as_str()))
                || file.collections.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(Error::Invalid);
            }
        }
        Ok(originals)
    }
    fn save_originals(&mut self, originals: &Originals) -> Result<()> {
        let mut next = self.current.clone();
        next.legacy_sources = Some(originals.clone());
        // Same latest-first durable publication as admission/deletion decisions.
        // An older primary cannot reopen a finalized migration after recovery.
        self.publish(next, |_| {})
    }
    fn original_name(&self, source: &Path) -> Result<String> {
        let root = self.checked_storage()?;
        if fs::canonicalize(source.parent().ok_or(Error::Invalid)?).map_err(io_error)? != root {
            return Err(Error::Invalid);
        }
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| valid_name(s))
            .ok_or(Error::Invalid)?;
        Ok(name.into())
    }
    pub fn legacy_imports_open(&self) -> Result<bool> {
        Ok(self.originals()?.until_unix_ms.is_none())
    }
    /// Historical document receipts identify their logical State reference.
    /// Look up established cleanup ownership without requiring installed code.
    pub fn legacy_document_registered(&self, collection: &str, state_ref: &str) -> Result<bool> {
        let ending = format!(
            "-{}.merkle",
            crate::canvas::hex(&crate::canvas::hash(state_ref.as_bytes()))
        );
        Ok(self.originals()?.files.iter().any(|file| {
            file.name.ends_with(&ending) && file.collections.iter().any(|cid| cid == collection)
        }))
    }
    pub fn legacy_source_registered(&self, collection: &str, source: &Path) -> Result<bool> {
        let name = self.original_name(source)?;
        Ok(self
            .originals()?
            .files
            .iter()
            .any(|f| f.name == name && f.collections.iter().any(|c| c == collection)))
    }
    /// Check before copying. A migration original is never a supported route to
    /// restore content whose collection has subsequently been deleted.
    pub fn check_legacy_import(&self, collection: &str, source: &Path) -> Result<()> {
        if !self
            .registry()?
            .spaces
            .iter()
            .flat_map(|s| &s.apps)
            .any(|a| a.collection == collection)
        {
            return Err(Error::Denied);
        }
        let name = self.original_name(source)?;
        let originals = self.originals()?;
        if originals.until_unix_ms.is_some()
            || (!self.current.registry.deletions.is_empty()
                && !originals.files.iter().any(|f| f.name == name))
            || originals.files.iter().filter(|f| f.name == name).any(|f| {
                f.removed
                    || f.collections.iter().any(|c| {
                        self.current
                            .registry
                            .deletions
                            .iter()
                            .any(|d| &d.collection == c)
                    })
            })
        {
            return Err(Error::Denied);
        }
        private(&fs::symlink_metadata(source).map_err(io_error)?)
    }
    /// Called after a confirmed import or recovery of its private receipt. It
    /// records cleanup ownership; it never copies payloads or grants membership.
    pub fn register_legacy_sources(&mut self, collection: &str, sources: &[PathBuf]) -> Result<()> {
        let registry = self.registry()?;
        if !registry
            .spaces
            .iter()
            .flat_map(|s| &s.apps)
            .any(|a| a.collection == collection)
            && !registry
                .deletions
                .iter()
                .any(|d| d.collection == collection)
        {
            return Err(Error::Denied);
        }
        if sources.is_empty() || sources.len() > 32 {
            return Err(Error::Invalid);
        }
        let mut originals = self.originals()?;
        if originals.until_unix_ms.is_some() {
            return Err(Error::Denied);
        }
        for source in sources {
            let name = self.original_name(source)?;
            let (bytes, sha256) = fingerprint(source)?;
            if let Some(file) = originals.files.iter_mut().find(|f| f.name == name) {
                if file.bytes != bytes || file.sha256 != sha256 || file.removed {
                    return Err(Error::Conflict);
                }
                if !file.collections.iter().any(|c| c == collection) {
                    if file.collections.len() == MAX_SPACES * MAX_APPS {
                        return Err(Error::Capacity);
                    }
                    file.collections.push(collection.into());
                    file.collections.sort();
                }
            } else {
                if originals.files.len() == 1024 {
                    return Err(Error::Capacity);
                }
                originals.files.push(Original {
                    name,
                    bytes,
                    sha256,
                    collections: vec![collection.into()],
                    removed: false,
                });
            }
        }
        originals.files.sort_by(|a, b| a.name.cmp(&b.name));
        self.save_originals(&originals)
    }
    /// Finalize only after the migration's acceptance evidence is recorded.
    /// Repeating this action cannot extend an existing retention deadline.
    pub fn finalize_legacy_sources(
        &mut self,
        now_unix_ms: u64,
        producers_stopped: bool,
    ) -> Result<u64> {
        if !producers_stopped {
            return Err(Error::Unavailable);
        }
        let mut originals = self.originals()?;
        if let Some(until) = originals.until_unix_ms {
            return Ok(until);
        }
        let root = self.checked_storage()?;
        let _owners = originals
            .files
            .iter()
            .map(|file| original_owner(&root.join(&file.name), &file.name))
            .collect::<Result<Vec<_>>>()?;
        for file in &mut originals.files {
            let path = root.join(&file.name);
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    if fingerprint(&path)? != (file.bytes, file.sha256.clone()) {
                        return Err(Error::Conflict);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => file.removed = true,
                Err(error) => return Err(io_error(error)),
            }
        }
        let until = now_unix_ms
            .checked_add(RETENTION_MS)
            .ok_or(Error::Invalid)?;
        originals.until_unix_ms = Some(until);
        self.save_originals(&originals)?;
        Ok(until)
    }
    pub fn legacy_deadlines(&self) -> Result<BTreeMap<String, Option<u64>>> {
        let originals = self.originals()?;
        let mut deadlines = BTreeMap::new();
        for file in originals.files.into_iter().filter(|f| !f.removed) {
            for collection in file.collections {
                deadlines.insert(collection, originals.until_unix_ms);
            }
        }
        Ok(deadlines)
    }
    pub fn prune_legacy_sources(&mut self, now_unix_ms: u64) -> Result<usize> {
        let mut originals = self.originals()?;
        if originals
            .until_unix_ms
            .is_none_or(|until| now_unix_ms < until)
        {
            return Ok(0);
        }
        let root = self.checked_storage()?.to_path_buf();
        // Preflight every retained original before removing any checkpoint or
        // journal. Keep journal ownership through the complete cleanup decision.
        let _owners = originals
            .files
            .iter()
            .map(|file| original_owner(&root.join(&file.name), &file.name))
            .collect::<Result<Vec<_>>>()?;
        for file in originals.files.iter().filter(|f| !f.removed) {
            let path = root.join(&file.name);
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    if fingerprint(&path)? != (file.bytes, file.sha256.clone()) {
                        return Err(Error::Conflict);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(io_error(error)),
            }
        }
        let (mut removed, complete) = prune_original_temporaries(&root, &originals)?;
        let mut changed = false;
        for file in originals.files.iter_mut().filter(|f| !f.removed) {
            match fs::remove_file(root.join(&file.name)) {
                Ok(()) => {
                    sync_directory(&root)?;
                    removed += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(io_error(error)),
            }
            if complete {
                file.removed = true;
                changed = true;
            }
        }
        if changed {
            self.save_originals(&originals)?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
#[path = "legacy_sources_tests.rs"]
mod tests;
