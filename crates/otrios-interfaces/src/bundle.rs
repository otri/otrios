// SPDX-License-Identifier: Apache-2.0
//! Experimental offline bundle contract (RFC 0025), independent of state/control protocols.
use crate::canvas::App;
use serde::{Deserialize, Serialize};

/// Six-byte magic followed by the little-endian bundle major version.
pub const INFO_HEADER: &[u8; 8] = b"OTAPP\0\x01\0";
pub const FILES_FORMAT: &str = "otrios-bundle-files-v1";
pub const INFO_PATH: &str = "Info.flex";
pub const FILES_PATH: &str = "Integrity/files.flex";
pub const SIGNATURE_PATH: &str = "Integrity/publisher.flex";
pub const SIGNATURE_FORMAT: &str = "otrios-app-signature-v1";
pub const UI_PATH: &str = "UI/main.flex";
pub const SCHEMA_PATH: &str = "Schemas/state.flex";
pub const CODE_PATH: &str = "Code/behavior.wasm";

/// Unknown optional top-level metadata may be ignored. New execution requirements
/// MUST be named in required_features/capabilities, never hidden in optional keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleInfo {
    /// The existing, separately versioned Canvas package contract, not a new app ABI.
    pub app: App,
    pub revision: u64,
    /// Delivery classification copied from the catalog; not an access grant.
    pub private: bool,
    pub required_features: Vec<String>,
    pub capabilities: Vec<String>,
    pub entry_ui: String,
    pub state_schema: String,
    pub behavior: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// Closed integrity schema. Entries are strictly sorted by path and cover every
/// regular file except this manifest. Signatures are a later milestone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleFiles {
    pub format: String,
    pub files: Vec<BundleFile>,
}

/// Detached Ed25519 receipt, outside its own hash closure. The key ID is SHA-256
/// of the raw 32-byte public key; the trust key must be supplied independently.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleSignature {
    pub format: String,
    pub key_sha256: String,
    pub signature: Vec<u8>,
}
