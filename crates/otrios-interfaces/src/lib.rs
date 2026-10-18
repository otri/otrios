// SPDX-License-Identifier: Apache-2.0
//! Experimental contract types. No stable wire format or ABI is promised.

#[cfg(feature = "layout")]
pub mod layout;
pub mod scene;

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

/// Experimental snapshot contract, independent of the CRDT's storage format.
#[derive(Clone, Debug, PartialEq)]
pub struct DashboardView {
    pub heads: Vec<String>,
    pub nodes: Vec<ViewNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewNode {
    pub id: u32,
    /// Bounds in logical canvas coordinates: x, y, width, height.
    pub bounds: [u32; 4],
    pub content: ViewContent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ViewContent {
    Label(String),
    Switch { label: String, enabled: bool },
    Graph { label: String, samples: Vec<u32> },
}
