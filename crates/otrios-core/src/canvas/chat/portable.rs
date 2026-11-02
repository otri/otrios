// SPDX-License-Identifier: LGPL-3.0-only
//! Complete pinned history, not the selected UI transcript or private journal.
use super::*;
use crate::{collection, merkle::Snapshot};
use otrios_interfaces::transfers::{
    CHAT_CHECKPOINT_HEADER, MAX_CHAT_RECORD_BYTES, MAX_CHAT_RECORDS, MAX_CHECKPOINT_BYTES,
};
use std::collections::BTreeMap;

pub struct PinnedHistory {
    history: collection::Collection,
    sequence: u64,
}
impl PinnedHistory {
    pub(super) fn new(history: collection::Collection, sequence: u64) -> Self {
        Self { history, sequence }
    }
    /// Serialization can run after releasing the authority lock; the pin shares
    /// immutable causal records with the accepted collection publication.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.history.len() > MAX_CHAT_RECORDS {
            return Err("Portable history record budget".into());
        }
        let mut result = CHAT_CHECKPOINT_HEADER.to_vec();
        result.extend(message_schema().id);
        result.extend(self.sequence.to_le_bytes());
        result.extend((self.history.len() as u32).to_le_bytes());
        for (sequence, row) in self.history.records() {
            let message = read_message(row)?;
            qualify_message(&message, sequence)?;
            if sequence > self.sequence
                || super::history::canonical_row(&message)?.hash() != row.hash()
            {
                return Err("Portable history causal form is not supported".into());
            }
            let bytes = row.to_bytes().map_err(err)?;
            result.extend(sequence.to_le_bytes());
            result.extend((bytes.len() as u32).to_le_bytes());
            result.extend(bytes);
            if result.len() > MAX_CHECKPOINT_BYTES {
                return Err("Portable history byte budget".into());
            }
        }
        Ok(result)
    }
    pub fn assets(&self) -> Result<BTreeMap<String, Asset>> {
        let mut result = BTreeMap::<String, Asset>::new();
        for (_, row) in self.history.records() {
            collect_assets(&mut result, &read_message(row)?.assets)?;
        }
        Ok(result)
    }
}
fn qualify_message(message: &Message, sequence: u64) -> Result<()> {
    if message.sequence != sequence
        || sequence == 0
        || !valid_hex(&message.sender.id, 16)
        || message.sender.avatar >= AVATARS
        || (!message.nonce.is_empty() && !valid_hex(&message.nonce, 16))
    {
        return Err("Invalid portable message identity".into());
    }
    super::authority::name(&message.sender.name)?;
    super::authority::text(&message.text, 2304, 1200, 18)?;
    validate_message_assets(&message.assets)
}
fn collect_assets(result: &mut BTreeMap<String, Asset>, assets: &[Asset]) -> Result<()> {
    for asset in assets {
        validate_asset(asset)?;
        if let Some(old) = result.get(&asset.sha256)
            && old.bytes != asset.bytes
        {
            return Err("Inconsistent portable attachment length".into());
        }
        result.insert(asset.sha256.clone(), asset.clone());
    }
    if result.len() > 32 {
        return Err("Portable attachment quota".into());
    }
    Ok(())
}

/// Fully qualified immutable messages. No source permissions or writer identity
/// are represented. The caller still checks installed policy and target Admin.
pub struct QualifiedHistory {
    messages: Vec<Message>,
    sequence: u64,
    assets: BTreeMap<String, Asset>,
}
impl QualifiedHistory {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        struct Reader<'a>(&'a [u8]);
        impl<'a> Reader<'a> {
            fn take(&mut self, size: usize) -> Result<&'a [u8]> {
                if size > self.0.len() {
                    return Err("Truncated portable history".into());
                }
                let (value, rest) = self.0.split_at(size);
                self.0 = rest;
                Ok(value)
            }
            fn u64(&mut self) -> Result<u64> {
                Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(err)?))
            }
            fn u32(&mut self) -> Result<u32> {
                Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(err)?))
            }
        }
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err("Portable history byte budget".into());
        }
        let mut reader = Reader(bytes);
        if reader.take(8)? != CHAT_CHECKPOINT_HEADER || reader.take(32)? != message_schema().id {
            return Err("Portable history framing/schema".into());
        }
        let sequence = reader.u64()?;
        let count = reader.u32()? as usize;
        if count > MAX_CHAT_RECORDS || count > reader.0.len() / 12 {
            return Err("Portable history record budget".into());
        }
        let mut collection = collection::Collection::new(
            COLLECTION,
            message_schema(),
            collection::Budget {
                records: MAX_CHAT_RECORDS,
                canonical_bytes: MAX_CHAT_RECORD_BYTES,
            },
        )
        .map_err(err)?;
        let mut result = Self {
            messages: Vec::with_capacity(count),
            sequence,
            assets: BTreeMap::new(),
        };
        for _ in 0..count {
            let key = reader.u64()?;
            let size = reader.u32()? as usize;
            if size > collection::MAX_RECORD_BYTES {
                return Err("Portable causal record budget".into());
            }
            let row = Snapshot::from_bytes(reader.take(size)?).map_err(err)?;
            let message = read_message(&row)?;
            qualify_message(&message, key)?;
            if key > sequence || super::history::canonical_row(&message)?.hash() != row.hash() {
                return Err("Portable history causal form is not supported".into());
            }
            // Enforces ordering, exact field closure and full collection budget;
            // unlike authority retention, importing never drops older messages.
            collection = collection.append(key, row).map_err(err)?;
            collect_assets(&mut result.assets, &message.assets)?;
            result.messages.push(message);
        }
        if !reader.0.is_empty() {
            return Err("Portable history trailing bytes".into());
        }
        Ok(result)
    }
    pub fn assets(&self) -> &BTreeMap<String, Asset> {
        &self.assets
    }
    pub fn len(&self) -> usize {
        self.messages.len()
    }
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
    /// Only into the caller's private, unreachable replacement stage. Existing
    /// journals are rejected. One durable compaction preserves the original
    /// immutable causal rows without importing source admission state.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn write_new_journal(&self, path: &std::path::Path) -> Result<()> {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err("Portable journal target exists".into());
        }
        let mut journal = super::journal::Journal::recover(path.to_path_buf(), |_| {
            Err("Unexpected existing journal".into())
        })?;
        journal.compact(
            self.messages
                .iter()
                .cloned()
                .map(|message| Ok(super::journal::Entry::Message(message)))
                .chain([Ok(super::journal::Entry::Sequence(self.sequence))]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(sequence: u64) -> Message {
        Message {
            sequence,
            at: 1_000_000 + sequence,
            sender: Profile {
                id: "ab".repeat(16),
                name: "Historical Sender".into(),
                avatar: 3,
            },
            kind: Kind::Message,
            text: format!("Dispatch {sequence}"),
            nonce: format!("{sequence:032x}"),
            assets: vec![],
        }
    }
    #[test]
    fn complete_history_pin_preserves_redaction_gaps_high_water_and_restart() {
        let mut history = super::super::history::History::recovering().unwrap();
        for sequence in 1..=90 {
            let mut m = message(sequence);
            if sequence == 1 {
                m.assets.push(Asset {
                    name: "attachment.txt".into(),
                    bytes: 7,
                    sha256: super::super::super::hex(&super::super::super::hash(b"content")),
                });
            }
            history.commit(history.prepare(&m).unwrap()).unwrap();
        }
        for sequence in [44, 90] {
            let revision = history.snapshot().unwrap().record(sequence).unwrap().hash();
            history
                .commit(history.prepare_removal(sequence, revision).unwrap())
                .unwrap();
        }
        let pin = PinnedHistory::new(history.snapshot().unwrap(), 90);
        history
            .commit(history.prepare(&message(91)).unwrap())
            .unwrap();
        let bytes = pin.encode().unwrap();
        let imported = QualifiedHistory::decode(&bytes).unwrap();
        assert_eq!(
            imported.len(),
            88,
            "all retained history, beyond the 64-row UI window"
        );
        assert_eq!(
            imported.sequence, 90,
            "redacted tail must not reuse an ordered key"
        );
        assert_eq!(pin.assets().unwrap(), *imported.assets());
        let path =
            std::env::temp_dir().join(format!("otrios-portable-{}.journal", random(16).unwrap()));
        imported.write_new_journal(&path).unwrap();
        assert!(imported.write_new_journal(&path).is_err());
        let room = Room::open(Some(path.clone()), 2_000_000).unwrap();
        assert_eq!(
            bytes,
            room.pin_portable_history().unwrap().encode().unwrap()
        );
        drop(room);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("journal.owner.lock")).unwrap();
    }
    #[test]
    fn portable_history_rejects_truncation_surplus_wrong_schema_and_duplicate_keys() {
        let mut history = super::super::history::History::recovering().unwrap();
        history
            .commit(history.prepare(&message(1)).unwrap())
            .unwrap();
        let bytes = PinnedHistory::new(history.snapshot().unwrap(), 1)
            .encode()
            .unwrap();
        for end in [0, 7, 39, 51, 63, bytes.len() - 1] {
            assert!(QualifiedHistory::decode(&bytes[..end]).is_err());
        }
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(QualifiedHistory::decode(&bad).is_err());
        let mut bad = bytes.clone();
        bad[8] ^= 1;
        assert!(QualifiedHistory::decode(&bad).is_err());
        let mut bad = bytes.clone();
        bad[48..52].copy_from_slice(&2u32.to_le_bytes());
        bad.extend_from_slice(&bytes[52..]);
        assert!(QualifiedHistory::decode(&bad).is_err());
        let mut bad = bytes;
        bad[40..48].copy_from_slice(&0u64.to_le_bytes());
        assert!(QualifiedHistory::decode(&bad).is_err());
    }
}
