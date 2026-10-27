// SPDX-License-Identifier: LGPL-3.0-only
//! One immutable collection owner, privately staged during journal replay.
use super::*;
use crate::{
    collection as collections,
    merkle::{Edit, Hash, Snapshot},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const RECORDS: usize = 50_000;
const BYTES: usize = 32 * 1024 * 1024;
pub fn message_schema() -> collections::Schema {
    let hash: Hash = super::super::hash(MESSAGE_SCHEMA)
        .try_into()
        .expect("SHA-256 length");
    collections::Schema {
        id: hash,
        fields: BTreeMap::from([(MESSAGE_FIELD, hash)]),
    }
}
pub fn message_selection(anchor: collections::Anchor) -> collections::Selection {
    collections::Selection {
        collection: COLLECTION,
        schema: message_schema().id,
        anchor,
        record_limit: 64,
        byte_limit: 256 * 1024,
        fields: vec![MESSAGE_FIELD],
    }
}
pub fn read_message(row: &Snapshot) -> Result<Message> {
    let leaf = row.subtree(&[MESSAGE_FIELD]).map_err(err)?;
    let register = leaf.register().ok_or("Missing message record")?;
    if register.schema() != message_schema().id {
        return Err("Message record schema".into());
    }
    let mut values = register.values();
    let bytes = values
        .next()
        .and_then(|(_, _, value)| value)
        .ok_or("Missing message content")?;
    if values.next().is_some() {
        return Err("Conflicting immutable message versions".into());
    }
    super::check(bytes, MAX_COMMAND_BYTES)?;
    flexbuffers::from_slice(bytes).map_err(err)
}

pub(super) struct Prepared {
    expected: Hash,
    next: collections::Collection,
}
pub(super) enum History {
    Recovering(collections::Collection),
    Live(Arc<Mutex<collections::Channel>>),
}
impl History {
    pub fn recovering() -> Result<Self> {
        Ok(Self::Recovering(
            collections::Collection::new(
                COLLECTION,
                message_schema(),
                collections::Budget {
                    records: RECORDS,
                    canonical_bytes: BYTES,
                },
            )
            .map_err(err)?,
        ))
    }
    pub fn snapshot(&self) -> Result<collections::Collection> {
        match self {
            Self::Recovering(snapshot) => Ok(snapshot.clone()),
            Self::Live(channel) => channel
                .lock()
                .map_err(|_| "Room collection lock")?
                .collection(COLLECTION)
                .cloned()
                .map_err(err),
        }
    }
    pub fn activate(&mut self, channel: Arc<Mutex<collections::Channel>>) -> Result<()> {
        let snapshot = self.snapshot()?;
        channel
            .lock()
            .map_err(|_| "Room collection lock")?
            .install(
                snapshot,
                collections::Policy {
                    admitted_fields: vec![MESSAGE_FIELD],
                    ..Default::default()
                },
            )
            .map_err(err)?;
        *self = Self::Live(channel);
        Ok(())
    }
    pub fn prepare(&self, message: &Message) -> Result<Prepared> {
        let bytes = flexbuffers::to_vec(message).map_err(err)?;
        super::check(&bytes, MAX_COMMAND_BYTES)?;
        let row = Snapshot::empty()
            .edit(
                [22; 16],
                &[Edit {
                    path: vec![MESSAGE_FIELD],
                    schema: message_schema().id,
                    value: Some(bytes),
                }],
            )
            .map_err(err)?
            .0;
        let mut current = self.snapshot()?;
        let expected = current.revision();
        loop {
            match current.append(message.sequence, row.clone()) {
                Ok(next) => return Ok(Prepared { expected, next }),
                Err(collections::Error::Limit) if !current.is_empty() => {
                    let next = current.records().nth(1).map_or(u64::MAX, |(key, _)| key);
                    current = current.retain_from(next);
                }
                Err(error) => return Err(err(error)),
            }
        }
    }
    pub fn commit(&mut self, prepared: Prepared) -> Result<()> {
        match self {
            Self::Recovering(current) => {
                if current.revision() != prepared.expected {
                    return Err("Room replay publication conflict".into());
                }
                *current = prepared.next;
                Ok(())
            }
            Self::Live(channel) => channel
                .lock()
                .map_err(|_| "Room collection lock")?
                .publish(COLLECTION, prepared.expected, prepared.next)
                .map_err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_prepares_a_bounded_new_root_without_changing_a_pin() {
        let mut history = History::Recovering(
            collections::Collection::new(
                COLLECTION,
                message_schema(),
                collections::Budget {
                    records: 3,
                    canonical_bytes: 4096,
                },
            )
            .unwrap(),
        );
        let mut pinned = None;
        for sequence in 1..=10 {
            let message = Message {
                sequence,
                at: 1_000_000,
                sender: Profile {
                    id: "0".repeat(32),
                    name: "Captain Test".into(),
                    avatar: 1,
                },
                kind: Kind::Message,
                text: format!("Dispatch {sequence}"),
                nonce: format!("{sequence:032x}"),
            };
            let prepared = history.prepare(&message).unwrap();
            if sequence == 4 {
                assert_eq!(
                    history
                        .snapshot()
                        .unwrap()
                        .records()
                        .map(|(key, _)| key)
                        .collect::<Vec<_>>(),
                    vec![1, 2, 3]
                );
            }
            history.commit(prepared).unwrap();
            assert!(history.snapshot().unwrap().len() <= 3);
            if sequence == 3 {
                pinned = Some(history.snapshot().unwrap());
            }
        }
        assert_eq!(
            history
                .snapshot()
                .unwrap()
                .records()
                .map(|(key, _)| key)
                .collect::<Vec<_>>(),
            vec![8, 9, 10]
        );
        assert_eq!(
            pinned
                .unwrap()
                .records()
                .map(|(key, _)| key)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }
}
