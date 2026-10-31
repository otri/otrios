// SPDX-License-Identifier: LGPL-3.0-only
//! Private durable registry. An uncertain publication poisons admission until
//! restart; callers cannot acknowledge a management decision before directory sync.
use super::*;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct Store {
    registry: Registry,
    path: PathBuf,
    failed: bool,
}
fn io_error(error: impl std::fmt::Display) -> Error {
    Error::Persistence(error.to_string())
}

pub fn encode(registry: &Registry) -> Result<Vec<u8>> {
    validate(registry)?;
    let payload = flexbuffers::to_vec(registry).map_err(io_error)?;
    if payload.len() > MAX_REGISTRY_BYTES {
        return Err(Error::Capacity);
    }
    let mut bytes = b"OTSP1".to_vec();
    bytes.extend(crate::canvas::hash(&payload));
    bytes.extend(payload);
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Registry> {
    if bytes.len() < 40
        || bytes.len() > MAX_REGISTRY_BYTES + 37
        || &bytes[..5] != b"OTSP1"
        || crate::canvas::hash(&bytes[37..]) != bytes[5..37]
    {
        return Err(Error::Invalid);
    }
    let payload = &bytes[37..];
    crate::flex_guard::validate(payload, 16, MAX_REGISTRY_BYTES, MAX_DELETIONS)
        .map_err(|_| Error::Invalid)?;
    let registry: Registry = flexbuffers::from_slice(payload).map_err(|_| Error::Invalid)?;
    validate(&registry)?;
    Ok(registry)
}
impl Store {
    /// Missing state is initialized explicitly; corrupt existing state never resets.
    pub fn open(path: PathBuf) -> Result<Self> {
        let registry = match fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = vec![];
                file.take(MAX_REGISTRY_BYTES as u64 + 38)
                    .read_to_end(&mut bytes)
                    .map_err(io_error)?;
                decode(&bytes)?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => empty(),
            Err(e) => return Err(io_error(e)),
        };
        Ok(Self {
            registry,
            path,
            failed: false,
        })
    }
    pub fn registry(&self) -> Result<&Registry> {
        if self.failed {
            Err(Error::Unavailable)
        } else {
            Ok(&self.registry)
        }
    }
    pub fn commit(
        &mut self,
        actor: &str,
        request: &Request,
        installed: Option<&AppPolicy>,
    ) -> Result<&Registry> {
        let candidate = apply(self.registry()?, actor, request, installed)?;
        let bytes = encode(&candidate)?;
        if let Err(error) = write(&self.path, &bytes) {
            self.failed = true;
            return Err(error);
        }
        self.registry = candidate;
        self.registry()
    }
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
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
        fs::rename(&temp, path).map_err(io_error)?;
        fs::File::open(parent)
            .and_then(|d| d.sync_all())
            .map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
