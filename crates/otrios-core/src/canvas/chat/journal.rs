// SPDX-License-Identifier: LGPL-3.0-only
//! Checksummed bounded append journal. Publish only after sync; compact atomically.
use super::{Result, err};
use otrios_interfaces::chat::{Message, Profile};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

pub(super) const MAX_JOURNAL: u64 = 64 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Guest {
    pub profile: Profile,
    pub digest: String,
    /// Authority-clock milliseconds since the Unix epoch. Preserve the journal key.
    #[serde(rename = "seen")]
    pub last_seen_unix_ms: u64,
    pub cooldown: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Receipt {
    pub participant: String,
    pub epoch: u64,
    pub nonce: String,
    pub digest: Vec<u8>,
    pub sequence: u64,
}
#[derive(Serialize, Deserialize)]
pub(super) enum Entry {
    Guest(Guest),
    Sent { message: Message, receipt: Receipt },
    Kick { guest: Guest, message: Message },
    Message(Message),
    Receipt(Receipt),
    Sequence(u64),
}
pub(super) struct Journal {
    path: PathBuf,
    file: fs::File,
    pub bytes: u64,
    failed: bool,
}
fn create(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(err)
}
fn record(entry: &Entry) -> Result<Vec<u8>> {
    let data = flexbuffers::to_vec(entry).map_err(err)?;
    super::check(&data, super::MAX_EVENT_BYTES)?;
    let mut bytes = (data.len() as u32).to_le_bytes().to_vec();
    bytes.extend(super::super::hash(&data));
    bytes.extend(data);
    Ok(bytes)
}
impl Journal {
    pub fn open(path: PathBuf) -> Result<(Self, Vec<Entry>)> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(err)?;
        }
        let mut file = match fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut file = create(&path)?;
                file.write_all(b"OTCHAT01")
                    .and_then(|_| file.sync_all())
                    .map_err(err)?;
                if let Some(parent) = path.parent() {
                    fs::File::open(parent)
                        .and_then(|d| d.sync_all())
                        .map_err(err)?;
                }
                file
            }
            Err(error) => return Err(err(error)),
        };
        let length = file.metadata().map_err(err)?.len();
        if length > MAX_JOURNAL {
            return Err("Chat journal byte limit".into());
        }
        file.rewind().map_err(err)?;
        let mut magic = [0; 8];
        file.read_exact(&mut magic).map_err(err)?;
        if &magic != b"OTCHAT01" {
            return Err("Chat journal format".into());
        }
        let mut at = 8_u64;
        let mut entries = Vec::new();
        while at < length {
            if length - at < 36 {
                break;
            }
            let mut header = [0; 36];
            file.read_exact(&mut header).map_err(err)?;
            let size = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
            if size == 0 || size > super::MAX_EVENT_BYTES {
                return Err("Chat journal record limit".into());
            }
            if length - at - 36 < size as u64 {
                break;
            }
            let mut data = vec![0; size];
            file.read_exact(&mut data).map_err(err)?;
            if super::super::hash(&data) != header[4..] {
                return Err("Chat journal checksum".into());
            }
            super::check(&data, super::MAX_EVENT_BYTES)?;
            entries.push(flexbuffers::from_slice(&data).map_err(err)?);
            at += 36 + size as u64;
        }
        // A crash may leave an incomplete final write. Complete but corrupt
        // records fail closed above; only the incomplete tail is discarded.
        if at != length {
            file.set_len(at)
                .and_then(|_| file.sync_all())
                .map_err(err)?;
        }
        file.seek(std::io::SeekFrom::End(0)).map_err(err)?;
        Ok((
            Self {
                path,
                file,
                bytes: at,
                failed: false,
            },
            entries,
        ))
    }
    pub fn append(&mut self, entry: &Entry) -> Result<()> {
        if self.failed {
            return Err("Chat journal requires recovery".into());
        }
        let bytes = record(entry)?;
        if self.bytes + bytes.len() as u64 > MAX_JOURNAL {
            return Err("Chat journal requires compaction".into());
        }
        if let Err(error) = self
            .file
            .write_all(&bytes)
            .and_then(|_| self.file.sync_all())
        {
            self.failed = true;
            return Err(err(error));
        }
        self.bytes += bytes.len() as u64;
        Ok(())
    }
    pub fn compact(&mut self, entries: impl IntoIterator<Item = Entry>) -> Result<()> {
        if self.failed {
            return Err("Chat journal requires recovery".into());
        }
        let next = self
            .path
            .with_extension(format!("{}.compact", super::random(8)?));
        let result = (|| {
            let mut file = create(&next)?;
            file.write_all(b"OTCHAT01").map_err(err)?;
            let mut size = 8;
            for entry in entries {
                let bytes = record(&entry)?;
                size += bytes.len() as u64;
                if size > MAX_JOURNAL {
                    return Err("Chat compacted journal limit".into());
                }
                file.write_all(&bytes).map_err(err)?;
            }
            file.sync_all().map_err(err)?;
            fs::rename(&next, &self.path).map_err(err)?;
            // Once renamed, a failed directory sync must stop publication.
            self.failed = true;
            if let Some(parent) = self.path.parent() {
                fs::File::open(parent)
                    .and_then(|d| d.sync_all())
                    .map_err(err)?;
            }
            self.file = file;
            self.bytes = size;
            self.failed = false;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(next);
        }
        result
    }
}
