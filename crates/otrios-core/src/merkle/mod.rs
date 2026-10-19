// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded immutable hierarchy and causal registers; Canvas uses app-root leases.
//!
//! SHA-256 covers canonical values AND causal metadata. Logical keys are stable
//! u64 identifiers, never array positions. This first slice provides register
//! assignment/deletion, not sequence edits, subtree deletion or history pruning.
mod coverage;
mod register;
mod replication;
mod tree;
mod wire;
pub use coverage::{Coverage, MAX_COVERAGE_PATHS};
pub use otrios_interfaces::merkle::{Actor, Cursor, Hash, Path};
pub use register::Register;
pub use replication::{Authority, Offer, Replica, Subscription};
pub use tree::{Edit, Snapshot, Work, Workspace};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Limit,
    Invalid,
    Schema,
    ActorCollision,
    ClockExhausted,
    Forbidden,
    Missing,
    Stale,
    ResyncRequired,
    Conflict,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Merkle {self:?}")
    }
}
impl std::error::Error for Error {}

pub const MAX_DEPTH: usize = 16;
pub const MAX_CHILDREN: usize = 32;
pub const MAX_ACTORS: usize = 32;
pub const MAX_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_NODE_BYTES: usize = 256 * 1024;
pub const MAX_TREE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_NODES: usize = 16_384;
pub const MAX_EDITS: usize = 256;

fn path_check(path: &[u64]) -> Result<()> {
    if path.len() > MAX_DEPTH {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
