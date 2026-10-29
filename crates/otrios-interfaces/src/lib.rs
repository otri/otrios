// SPDX-License-Identifier: Apache-2.0
//! Experimental contract types. No stable wire format or ABI is promised.

pub mod ink;
#[cfg(feature = "layout")]
pub mod layout;
pub mod scene;
pub mod theme;

/// SHA-256 digest of a canonical object envelope, once hashing is implemented.
/// Constructing this type does not authenticate or validate the referenced bytes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContentId(pub [u8; 32]);

/// Logical identity stays stable across immutable revisions.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ObjectId(pub [u8; 16]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevisionRef {
    pub object: ObjectId,
    pub content: ContentId,
}

/// A monotonically increasing generation within one authority's lifetime.
/// Restart epochs and durable cursor encoding are reserved for a later RFC.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Generation(pub u64);

/// A subscription must explicitly report gaps instead of silently losing updates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubscriptionEvent {
    SnapshotAvailable(Generation),
    ResyncRequired { latest: Generation },
}

/// Logical canvas shared by native and browser hosts. Hosts letterbox uniformly.
pub const DASHBOARD_WIDTH: u32 = 1280;
pub const DASHBOARD_HEIGHT: u32 = 800;

pub mod keyboard;

#[cfg(feature = "layout")]
pub mod bundle;
#[cfg(feature = "layout")]
pub mod canvas;
#[cfg(feature = "layout")]
pub mod chat;

pub mod collection;
pub mod merkle;

#[cfg(feature = "release")]
pub mod release;

#[cfg(feature = "release")]
pub mod session;

/// Host-local retained presentation metadata.
pub mod presentation;
