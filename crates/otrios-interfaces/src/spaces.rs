// SPDX-License-Identifier: Apache-2.0
//! RFC 0027 management v1. Identifiers and bindings are references, never grants.
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const MAX_SPACES: usize = 64;
pub const MAX_MEMBERS: usize = 128;
pub const MAX_APPS: usize = 128;
pub const MAX_DELETIONS: usize = 4096;
pub const MAX_REGISTRY_BYTES: usize = 2 * 1024 * 1024;
pub const CHANNEL: u8 = 5;
pub const MAX_COMMAND_BYTES: usize = 8192;
pub const MAX_EVENT_BYTES: usize = 128 * 1024;
pub const MAX_INSTALLED_APPS: usize = 16;
pub const MAX_ROOTS: usize = 16;
pub const MAX_RETAINED_DATASETS: usize = 512;
pub const MAX_COLLECTION_SCHEMA_BYTES: usize = 512 * 1024;

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
    /// Current continuous membership interval. Zero is an older registry member;
    /// live authority-issued bindings are RAM-only and renewed after restart.
    #[serde(default)]
    pub joined_generation: u64,
}
/// One explicitly edited row in the bounded, single-Space access form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipEdit {
    pub identity: String,
    pub role: Option<Role>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserProfile {
    pub name: String,
    pub revision: u64,
}

/// Admitted installed-app policy, supplied by the host after bundle verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppPolicy {
    pub app: String,
    /// Digest of the complete canonical collection schema, including its roots.
    pub schema: String,
    pub closed_only: bool,
    pub editable: bool,
    pub comments: bool,
}

/// Multiple screens may share a root; distinct roots remain separate documents
/// inside one Space-owned app collection. References are not access grants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootSchema {
    pub state_ref: String,
    pub schema: crate::canvas::StateSchema,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppData {
    pub policy: AppPolicy,
    pub collection: String,
    pub attached: bool,
    pub attachment_epoch: u64,
    /// Space-local label; empty means the installed bundle's canonical title.
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Background {
    #[default]
    None,
    Sand,
    Sage,
    Stone,
    Sky,
    Lilac,
    Rose,
    Rgb([u8; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceLayout {
    pub width: u8,
    pub height: u8,
    pub background: Background,
}
impl Default for SpaceLayout {
    fn default() -> Self {
        Self {
            width: 3,
            height: 1,
            background: Background::None,
        }
    }
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
    #[serde(default)]
    pub layout: SpaceLayout,
    /// A recoverable Space keeps its current authorization and data references.
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DatasetOrigin {
    BeforeReplacement,
    BeforeRestore,
}

/// An inactive sealed dataset is distinct from the bundle's single app slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedDataset {
    pub space: String,
    pub policy: AppPolicy,
    pub collection: String,
    pub label: String,
    pub origin: DatasetOrigin,
    pub generation: u64,
}

/// Minimal permanent stale-reference rejection record. No deleted payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deletion {
    /// Administrative cleanup scope. Empty only for earlier private registries.
    #[serde(default)]
    pub space: String,
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
    #[serde(default)]
    pub retained: Vec<RetainedDataset>,
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

/// A cross-Space move never creates a second active instance of one bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoveMode {
    AppAndData,
    AppOnly,
    DataOnly,
}

/// Mutations are serialized by one authority and require an expected generation.
/// Identity is supplied by authentication, never by this request body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    Profile {
        name: String,
        expected_revision: u64,
    },
    Access {
        space: String,
        audience: Audience,
        changes: Vec<MembershipEdit>,
    },
    Create {
        name: String,
    },
    ConfigureSpace {
        space: String,
        name: String,
        layout: SpaceLayout,
    },
    PlaceSpace {
        space: String,
        before: Option<String>,
    },
    Move {
        operation: String,
        source: String,
        destination: String,
        app: String,
        collection: String,
        destination_collection: Option<String>,
        mode: MoveMode,
    },
    DeleteSpace {
        space: String,
        remove_data: bool,
    },
    RestoreSpace {
        space: String,
    },
    RestoreDataset {
        space: String,
        app: String,
        collection: String,
    },
    DeleteDataset {
        space: String,
        app: String,
        collection: String,
    },
    LabelApp {
        space: String,
        app: String,
        name: String,
    },
    LabelDataset {
        space: String,
        app: String,
        collection: String,
        name: String,
    },
    PlaceApp {
        space: String,
        app: String,
        before: Option<String>,
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
        /// The collection shown in the trusted confirmation, never its successor.
        collection: String,
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

/// One authority incarnation and its serialized discovery revision. Observation
/// gaps always replace the snapshot; cursors confer no access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryCursor {
    pub authority: String,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    MoveStatus {
        version: u32,
        id: u32,
        operation: String,
        source: String,
        destination: String,
    },
    Watch {
        version: u32,
        observation: u32,
        selected: Option<String>,
        cursor: Option<DiscoveryCursor>,
    },
    Manage {
        version: u32,
        id: u32,
        request: Request,
    },
}

/// Verified installed launch metadata. Reading it never starts app execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledApp {
    pub code_bytes: Option<u64>,
    pub revision: Option<u64>,
    pub policy: AppPolicy,
    pub title: String,
    pub ui_ref: String,
    pub state_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub audience: Audience,
    pub role: Option<Role>,
    pub layout: SpaceLayout,
    /// Attached app previews only. Collection IDs and retained data are excluded.
    pub apps: Vec<MapApp>,
    pub attached_count: u16,
    pub archived: bool,
    /// Only an authenticated current Admin receives retained identities.
    pub retained: Vec<DatasetSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetSummary {
    pub app: String,
    pub collection: String,
    pub title: String,
    pub availability: AppAvailability,
    pub stored_bytes: Option<u64>,
    /// None identifies the preferred dataset retained by ordinary app removal.
    pub origin: Option<DatasetOrigin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapApp {
    pub app: String,
    pub title: String,
    /// Code compatibility and known failures; collection readiness is qualified
    /// on selection, before launch. A preview is never a mount grant.
    pub availability: AppAvailability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppAvailability {
    Available,
    Unavailable,
    Incompatible,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSummary {
    pub active_users: Option<u16>,
    pub app: String,
    pub title: String,
    pub collection: String,
    pub attached: bool,
    pub availability: AppAvailability,
    /// Managed checkpoint/journal/presentation bytes, excluding shared runtime
    /// code and backups. None means unavailable, never an invented zero.
    pub stored_bytes: Option<u64>,
    pub launch: Option<InstalledApp>,
}

/// Admin-only deletion progress. Unmanaged/offline copies cannot be verified.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupSummary {
    pub deleted_collections: u32,
    pub local_pending: u32,
    pub backup_until_unix_ms: Option<u64>,
    pub legacy_until_unix_ms: Option<u64>,
    pub legacy_qualification_required: bool,
    pub storage_unknown: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceDetail {
    pub active_users: Option<u16>,
    pub retained: Vec<DatasetSummary>,
    pub space: String,
    pub cleanup: Option<Box<CleanupSummary>>,
    pub apps: Vec<AppSummary>,
    pub members: Option<Vec<Member>>,
    pub installed: Vec<InstalledApp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discovery {
    pub profile: Option<UserProfile>,
    /// Verified installed catalog identity, independent of presence revisions.
    pub catalog: Vec<u8>,
    pub version: u32,
    pub cursor: DiscoveryCursor,
    pub generation: u64,
    pub identity: Option<String>,
    pub spaces: Vec<SpaceSummary>,
    pub selected: Option<SpaceDetail>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagementStatus {
    Applied,
    Invalid,
    Denied,
    Conflict,
    LastAdmin,
    Incompatible,
    Capacity,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Event {
    MoveStatus {
        version: u32,
        id: u32,
        operation: String,
        status: ManagementStatus,
        generation: Option<u64>,
    },
    ResyncRequired {
        version: u32,
    },
    Snapshot {
        observation: u32,
        snapshot: Box<Discovery>,
        resync: bool,
    },
    Result {
        version: u32,
        id: u32,
        status: ManagementStatus,
        created: Option<String>,
    },
    Unavailable {
        version: u32,
    },
}
