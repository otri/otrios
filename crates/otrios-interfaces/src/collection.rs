// SPDX-License-Identifier: Apache-2.0
//! RFC 0024 bounded ordered windows. Keys are stable single-authority commit
//! identities, never array offsets. These values carry no read permission.
use crate::merkle::Hash;

pub const MAX_WINDOW_RECORDS: usize = 64;
pub const MAX_WINDOW_BYTES: usize = 512 * 1024;
pub const MAX_FIELDS: usize = 32;
pub const CHANNEL: u8 = 4;
pub const MAX_COMMAND_BYTES: usize = 1024;
pub const MAX_EVENT_BYTES: usize = 1024 * 1024 + 512;

/// Explicit bounded outcomes. State failures do not change the release-control
/// hello/upgrade envelope or confer admission to any collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    Invalid = 0,
    Limit = 1,
    Schema = 2,
    Forbidden = 3,
    Stale = 4,
    AnchorExpired = 5,
    Backpressure = 6,
    ResyncRequired = 7,
    Unavailable = 8,
}

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
impl Command {
    pub fn generation(&self) -> u64 {
        match self {
            Self::Select { generation, .. }
            | Self::Acknowledge { generation, .. }
            | Self::Resync { generation }
            | Self::Cancel { generation } => *generation,
        }
    }
}
