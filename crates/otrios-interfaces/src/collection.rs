// SPDX-License-Identifier: Apache-2.0
//! RFC 0024 bounded ordered windows. Keys are stable single-authority commit
//! identities, never array offsets. These values carry no read permission.
use crate::merkle::Hash;

pub const MAX_WINDOW_RECORDS: usize = 64;
pub const MAX_WINDOW_BYTES: usize = 512 * 1024;
pub const MAX_FIELDS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "layout", derive(serde::Serialize, serde::Deserialize))]
pub struct Cursor {
    pub epoch: u64,
    pub collection: u64,
    pub key: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "layout", derive(serde::Serialize, serde::Deserialize))]
pub enum Anchor {
    Tail,
    Before(Cursor),
    Around(u64),
}

/// A compiled flat-record projection. Schema and field IDs are checked against
/// trusted authority policy before any index traversal or record retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "layout", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "layout", serde(deny_unknown_fields))]
pub struct Selection {
    pub collection: u64,
    pub schema: Hash,
    pub anchor: Anchor,
    pub record_limit: u16,
    /// Canonical selected record bytes, including causal metadata. Index and
    /// transport overhead are bounded separately by MAX_WINDOW_RECORDS/FIELDS.
    pub byte_limit: u32,
    pub fields: Vec<u64>,
}

/// Binary control carried by the shared negotiated synchronization session.
/// Cancelling a request also releases an unacknowledged replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Select {
        generation: u64,
        plan: Selection,
    },
    Acknowledge {
        generation: u64,
        cursor: crate::merkle::Cursor,
        root: Option<Hash>,
    },
    Resync {
        generation: u64,
    },
    Cancel {
        generation: u64,
    },
}
