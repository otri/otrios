// SPDX-License-Identifier: LGPL-3.0-only
//! RFC 0024 single-authority ordered collections. Immutable bounded index pages
//! reference independently bounded causal records. No distributed insertion,
//! editing or tombstone semantics are implied by the ordered commit keys.
mod channel;
mod index;
mod lease;
mod synthetic;
mod window;
mod wire;

pub use channel::{Access, Channel, ChannelSession, Event, Policy};
pub use index::{Budget, Collection, IndexWork, Schema};
pub use lease::{Authority, Lease, Offer, Replica, Session};
pub use otrios_interfaces::collection::*;
pub use synthetic::{Synthetic, synthetic_schema};
pub use window::Window;
pub use wire::{decode_command, encode_command};

use crate::merkle;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Limit,
    Invalid,
    Schema,
    Forbidden,
    Conflict,
    Stale,
    AnchorExpired,
    Backpressure,
    Merkle(merkle::Error),
}
impl From<merkle::Error> for Error {
    fn from(value: merkle::Error) -> Self {
        Self::Merkle(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collection {self:?}")
    }
}
impl std::error::Error for Error {}

/// Trusted server policy. Construct from admission, never from a client plan.
#[derive(Clone, Debug)]
pub struct Grant {
    pub collection: u64,
    pub fields: Vec<u64>,
}
