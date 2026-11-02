// SPDX-License-Identifier: Apache-2.0
//! RFC 0028 portable payload metadata. Integrity is not trust or a capability.
use serde::{Deserialize, Serialize};

pub const APP_MEDIA_TYPE: &str = "application/vnd.otrios.app+zip";
pub const DATA_MEDIA_TYPE: &str = "application/vnd.otrios.app-data+zip";
pub const DATA_INFO_HEADER: &[u8; 8] = b"OTDATA\x01\0";
pub const DATA_FORMAT: &str = "otrios-app-data-v1";
pub const DATA_FILES_FORMAT: &str = "otrios-data-files-v1";
pub const MAX_APP_ARCHIVE_BYTES: usize = 17 * 1024 * 1024;
pub const MAX_DATA_ARCHIVE_BYTES: usize = 81 * 1024 * 1024;
pub const MAX_DATA_BYTES: usize = 80 * 1024 * 1024;
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = 64 * 1024;
pub const CANVAS_CHECKPOINT_HEADER: &[u8; 8] = b"OTCVP\x01\0\0";
pub const CANVAS_CODEC: &str = "otrios-canvas-portable-v1";
pub const MAX_CANVAS_CHECKPOINT_BYTES: usize = 1024 * 1024;
pub const CHAT_CHECKPOINT_HEADER: &[u8; 8] = b"OTCHP\x01\0\0";
pub const CHAT_CODEC: &str = "otrios-chat-portable-v1";
pub const MAX_CHAT_RECORDS: usize = 50_000;
pub const MAX_CHAT_RECORD_BYTES: usize = 32 * 1024 * 1024;
pub const COLLECTION_CHECKPOINT_HEADER: &[u8; 8] = b"OTCLP\x01\0\0";
pub const COLLECTION_CODEC: &str = "otrios-collection-portable-v1";

/// An exact dataset reference, including inactive data. Never an access grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetRef {
    pub space: String,
    pub app: String,
    pub collection: String,
    pub schema: String,
}

/// A reviewed binding. A replacement CID is minted by the authority once and
/// never reused; a lost response is resolved with a read, not a mutation replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    pub space: String,
    pub app: String,
    pub collection: String,
    pub replacement: String,
    pub expected_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplacementOutcome {
    Committed,
    Unknown,
}

/// Authority-issued review of a completely qualified, temporary data upload.
/// It grants no access and cannot be used after its actor/binding review expires.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataReview {
    pub stage: String,
    pub replacement: Replacement,
    pub audience: crate::spaces::Audience,
    pub schema_sha256: String,
    pub archive_sha256: String,
    pub bytes: u64,
    pub expires_unix_ms: u64,
}

/// An explicitly published immutable code object. Mutable datasets are excluded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppShare {
    pub app: String,
    pub digest: String,
    pub title: String,
}

/// A verified candidate and exact destination/baseline, awaiting an explicit
/// decision. Neither this review nor its URL grants installation rights.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppReview {
    pub stage: String,
    pub space: String,
    pub app: String,
    pub digest: String,
    pub title: String,
    pub revision: u64,
    pub bytes: u64,
    pub installed: Option<String>,
    pub affected_spaces: u32,
    pub expected_generation: u64,
    pub catalog: Vec<u8>,
    pub release: String,
    pub audience: crate::spaces::Audience,
    pub expires_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppImportPhase {
    Queued,
    Armed,
    Applied,
    Rejected,
}

/// Durable decision status. Applied means the code and reviewed attachment are
/// active. Queued/armed are intermediate states, never an installation success.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppImportState {
    pub operation: String,
    pub phase: AppImportPhase,
    pub release: Option<String>,
}

/// Host-operator bridge metadata; never exposed by a public download route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppInstallJob {
    pub review: AppReview,
    pub state: AppImportState,
}

/// Trusted host I/O boundary. Publisher app code cannot enqueue these tasks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TransferTarget {
    App { space: String },
    Data { dataset: DatasetRef },
}
impl TransferTarget {
    pub fn path(&self) -> String {
        match self {
            Self::App { space } => format!("/transfers/apps/{space}"),
            Self::Data { dataset: d } => {
                format!("/transfers/data/{}/{}/{}", d.space, d.app, d.collection)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TransferIo {
    Copy { text: String },
    Pick,
    Url { url: String },
    File { file: String },
    Export,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferTask {
    pub id: u32,
    pub target: TransferTarget,
    pub io: TransferIo,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferFile {
    pub id: String,
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TransferReply {
    Copied,
    App { review: AppReview },
    Data { review: DataReview },
    Files { files: Vec<TransferFile> },
    Exported { name: String },
    Cancelled,
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasCheckpoint {
    pub format: String,
    pub scope: String,
    pub schema: crate::canvas::StateSchema,
    pub fields: Vec<u8>,
    pub signals: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataInfo {
    pub format: String,
    pub bundle_id: String,
    pub schema_sha256: String,
    pub codec: String,
    pub codec_version: u32,
    pub checkpoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataFiles {
    pub format: String,
    pub files: Vec<DataFile>,
}
