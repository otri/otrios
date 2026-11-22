// SPDX-License-Identifier: LGPL-3.0-only
//! Latest committed local app snapshot. One writer lock; readers pin complete bytes.
use super::*;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
pub fn load(path: &Path) -> Result<Snapshot> {
    let file = fs::File::open(path).map_err(err)?;
    let mut bytes = Vec::new();
    file.take(MAX_SNAPSHOT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    Snapshot::from_bytes(&bytes)
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Missing Monitoring directory")?;
    let temporary = path.with_extension(format!("pending-{}", std::process::id()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(err)?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(err)?;
        fs::rename(&temporary, path).map_err(err)?;
        fs::File::open(parent)
            .and_then(|p| p.sync_all())
            .map_err(err)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub fn collect(path: &Path, feed: &str, producer: &str, data: Data) -> Result<Snapshot> {
    let lock_path = path.with_extension("writer.lock");
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(lock_path).map_err(err)?;
    lock.lock().map_err(err)?;
    let previous = if path.exists() {
        Some(load(path)?)
    } else {
        None
    };
    let snapshot = Snapshot::collect(previous.as_ref(), feed, producer, data)?;
    if previous
        .as_ref()
        .is_none_or(|p| p.root() != snapshot.root())
    {
        atomic(path, &snapshot.bytes()?)?;
    }
    Ok(snapshot)
}
