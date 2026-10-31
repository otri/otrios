// SPDX-License-Identifier: Apache-2.0
//! RFC 0027 management v1. Identifiers and bindings are references, never grants.
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const MAX_SPACES: usize = 64;
pub const MAX_MEMBERS: usize = 128;
pub const MAX_APPS: usize = 128;
pub const MAX_DELETIONS: usize = 4096;
pub const MAX_REGISTRY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Member,
    Admin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Audience {
    #[default]
    Closed,
    PublicView,
    PublicComment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub identity: String,
    pub role: Role,
}

/// Admitted installed-app policy, supplied by the host after bundle verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppPolicy {
    pub app: String,
    /// Digest of the complete canonical state schema, not just its display name.
    pub schema: String,
    pub closed_only: bool,
    pub editable: bool,
    pub comments: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppData {
    pub policy: AppPolicy,
    pub collection: String,
    pub attached: bool,
    pub attachment_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub audience: Audience,
    pub members: Vec<Member>,
    pub apps: Vec<AppData>,
    pub admission_epoch: u64,
}

/// Minimal permanent stale-reference rejection record. No deleted payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deletion {
    pub collection: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub version: u32,
    pub generation: u64,
    pub spaces: Vec<Space>,
    pub deletions: Vec<Deletion>,
}

/// A host-issued scope. Every operation must revalidate it against current policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub version: u32,
    pub space: String,
    pub app: String,
    pub collection: String,
    pub schema: String,
    pub admission_epoch: u64,
    pub attachment_epoch: u64,
}

/// Provisioned host service grant, never a human membership role or client claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceBinding {
    pub identity: String,
    pub binding: Binding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Read,
    Edit,
    Comment,
    Moderate,
}

/// Mutations are serialized by one authority and require an expected generation.
/// Identity is supplied by authentication, never by this request body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    Create {
        name: String,
    },
    Membership {
        space: String,
        identity: String,
        role: Option<Role>,
    },
    Audience {
        space: String,
        audience: Audience,
    },
    Attach {
        space: String,
        app: String,
    },
    Remove {
        space: String,
        app: String,
    },
    DeleteData {
        space: String,
        app: String,
        remove: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub expected_generation: u64,
    pub change: Change,
}
