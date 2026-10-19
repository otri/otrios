// SPDX-License-Identifier: Apache-2.0
//! Experimental RFC 0002 identities. Canonical wire format is specified in
//! docs/rfcs/0005-merkle-subtree-prototype.md; not a stable ABI or capability.
pub type Hash = [u8; 32];
pub type Actor = [u8; 16];
/// Stable field/map/item keys; never transient array indices.
pub type Path = Vec<u64>;
/// Host-issued epoch and lease identity; sequence advances only within a lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor {
    pub epoch: u64,
    pub lease: u64,
    pub sequence: u64,
}
