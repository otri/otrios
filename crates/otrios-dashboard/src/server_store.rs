// SPDX-License-Identifier: Apache-2.0
//! Root-chained, checksummed append journal. Flush before publish/ACK; checkpoint
//! rotation is recoverable even between checkpoint rename and journal truncation.
use super::*;
use otrios_core::canvas::{self, Document};
use std::io::{Read, Seek, SeekFrom};
const MAX_JOURNAL: usize = 16 * 1024 * 1024;
const CHECKPOINT_AT: u64 = 4 * 1024 * 1024;
fn log_path(file: &Path) -> PathBuf {
    file.with_extension("merkle.log")
}
fn failure(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(super) fn load(file: &Path, mut doc: Document) -> Result<Document, String> {
    let path = log_path(file);
    if !path.exists() {
        return Ok(doc);
    }
    let mut log = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(failure)?;
    let mut bytes = Vec::new();
    std::io::Read::take(&mut log, MAX_JOURNAL as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() > MAX_JOURNAL {
        return Err("Canvas journal bound exceeded".into());
    }
    let mut at = 0;
    let mut records = Vec::new();
    while at < bytes.len() {
        if bytes.len() - at < 36 {
            break;
        }
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().map_err(failure)?) as usize;
        if len > canvas::MAX_BYTES {
            return Err("Canvas journal record bound exceeded".into());
        }
        let end = at
            .checked_add(36)
            .and_then(|n| n.checked_add(len))
            .ok_or("Journal offset overflow")?;
        if end > bytes.len() {
            break;
        }
        let record = &bytes[at + 36..end];
        if canvas::hash(record) != bytes[at + 4..at + 36] {
            return Err("Canvas journal checksum mismatch".into());
        }
        let (base, root) = Document::journal_roots(record)?;
        records.push((base, root, record));
        at = end;
    }
    // A crash may leave the old journal after the new checkpoint was renamed.
    let start = records
        .iter()
        .rposition(|(_, root, _)| *root == doc.root_hash())
        .map_or(0, |i| i + 1);
    for (base, _, record) in &records[start..] {
        if *base != doc.root_hash() {
            return Err("Canvas checkpoint/journal root gap".into());
        }
        doc.replay(record)?;
    }
    if at < bytes.len() {
        log.set_len(at as u64).map_err(failure)?;
        log.sync_all().map_err(failure)?;
    }
    Ok(doc)
}
pub(super) fn append(file: &Path, old: &Document, next: &Document) -> Result<(), String> {
    let path = log_path(file);
    if fs::metadata(&path).is_ok_and(|m| m.len() >= CHECKPOINT_AT) {
        persist(file, &old.save()?).map_err(failure)?;
        let log = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(failure)?;
        log.set_len(0).map_err(failure)?;
        log.sync_all().map_err(failure)?;
    }
    let record = next.journal_delta(old)?;
    let mut options = fs::OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut log = options.open(&path).map_err(failure)?;
    let prior = log.seek(SeekFrom::End(0)).map_err(failure)?;
    if prior.saturating_add(record.len() as u64 + 36) > MAX_JOURNAL as u64 {
        return Err("Canvas journal capacity exhausted".into());
    }
    log.write_all(&(record.len() as u32).to_le_bytes())
        .map_err(failure)?;
    log.write_all(&canvas::hash(&record)).map_err(failure)?;
    log.write_all(&record).map_err(failure)?;
    log.sync_all().map_err(failure)?;
    if prior == 0
        && let Some(parent) = file.parent()
    {
        fs::File::open(parent)
            .and_then(|d| d.sync_all())
            .map_err(failure)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, Document) {
        let p = std::env::temp_dir().join(format!(
            "otrios-merkle-journal-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        let file = p.join("test.merkle");
        let doc = Document::new(
            otrios_interfaces::canvas::StateSchema {
                id: "journal-test".into(),
                fields: vec![otrios_interfaces::canvas::Field {
                    id: 1,
                    name: "value".into(),
                    default: otrios_interfaces::layout::Value::Number(0.),
                    writable: true,
                }],
            },
            "journal",
        )
        .unwrap();
        persist(&file, &doc.save().unwrap()).unwrap();
        (file, doc)
    }
    #[test]
    fn torn_tail_and_checkpoint_rotation_recover_without_dropping_commits() {
        let (file, initial) = fixture();
        let mut next = initial.clone();
        next.set(&[(1, 700)]).unwrap();
        append(&file, &initial, &next).unwrap();
        let mut log = fs::OpenOptions::new()
            .append(true)
            .open(log_path(&file))
            .unwrap();
        log.write_all(&[2, 3]).unwrap();
        log.sync_all().unwrap();
        let recovered = load(&file, initial.clone()).unwrap();
        assert_eq!(recovered.root_hash(), next.root_hash());
        let length = fs::metadata(log_path(&file)).unwrap().len();
        assert!(length > 36);
        // Crash after checkpoint rename, before journal truncation: old records
        // remain, but the checkpoint is an validated root-chain anchor.
        persist(&file, &next.save().unwrap()).unwrap();
        let recovered = load(&file, next.clone()).unwrap();
        assert_eq!(recovered.root_hash(), next.root_hash());
        let mut later = next.clone();
        later.set(&[(1, 900)]).unwrap();
        append(&file, &next, &later).unwrap();
        assert_eq!(load(&file, next).unwrap().value(1).unwrap(), 900);
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
    #[test]
    fn complete_corrupt_records_fail_closed_instead_of_silently_rewinding() {
        let (file, initial) = fixture();
        let mut next = initial.clone();
        next.set(&[(1, 700)]).unwrap();
        append(&file, &initial, &next).unwrap();
        let path = log_path(&file);
        let mut data = fs::read(&path).unwrap();
        data[10] ^= 1;
        fs::write(path, data).unwrap();
        assert!(load(&file, initial).is_err());
        fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
}
