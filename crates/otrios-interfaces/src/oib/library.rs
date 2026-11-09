// SPDX-License-Identifier: Apache-2.0
//! RFC 0036 L1 data-only library package and stable export identities.
use super::designer::{ClassDescriptor, ComponentDefinition, DefinitionId};
use serde::{Deserialize, Serialize};

pub const INFO_HEADER: &[u8; 8] = b"OTLIB\0\x01\0";
pub const INFO_FORMAT: &str = "otrios-oib-library-package-v1";
pub const FORMAT: &str = "otrios-oib-library-v1";
pub const FEATURE: &str = "otrios-oib-library-v1";
pub const GRAPH_PATH: &str = "OIB/library.flex";
pub const SIGNATURE_FORMAT: &str = "otrios-oib-library-signature-v1";

/// `<SHA256 of trusted publisher public key>:<lowercase library slug>`.
/// The package revision never becomes this logical identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LibraryId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExportId(pub u64);

/// SHA-256 of exact canonical bytes, not a signature or permission grant.
pub type Revision = [u8; 32];

/// Closed L1 metadata. Later executable/resource requirements need a new admitted
/// package contract; optional metadata cannot smuggle execution permissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryInfo {
    pub format: String,
    pub library: LibraryId,
    /// Human-readable release label; dependency resolution pins Revision.
    pub version: String,
    pub required_features: Vec<String>,
    pub entry_graph: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub format: String,
    /// Requirements for app-owned admitted classes, never replacements for the
    /// app's class table. Library appearance defaults live on definition nodes.
    pub required_classes: Vec<ClassDescriptor>,
    pub definitions: Vec<ComponentDefinition>,
    pub exports: Vec<Export>,
}

pub type LibraryGraph = Graph;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub id: ExportId,
    pub name: String,
    pub definition: DefinitionId,
    /// Derived structural/class/action ABI, independent of style/default bytes.
    pub contract: Revision,
}
