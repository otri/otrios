// SPDX-License-Identifier: Apache-2.0
//! RFC 0036 signed graph/style and bounded component-behavior libraries.
use super::designer::{ClassDescriptor, ComponentDefinition, DefinitionId, NodeId, PropertyPath};
use crate::layout::{Value, ValueType};
use serde::{Deserialize, Serialize};

pub const INFO_HEADER: &[u8; 8] = b"OTLIB\0\x01\0";
pub const INFO_FORMAT: &str = "otrios-oib-library-package-v1";
pub const FORMAT: &str = "otrios-oib-library-v1";
pub const FEATURE: &str = "otrios-oib-library-v1";
pub const GRAPH_PATH: &str = "OIB/library.flex";
pub const BEHAVIOR_INFO_HEADER: &[u8; 8] = b"OTLIB\0\x02\0";
pub const BEHAVIOR_INFO_FORMAT: &str = "otrios-oib-library-package-v2";
pub const BEHAVIOR_FORMAT: &str = "otrios-oib-library-v2";
pub const CODE_PATH: &str = "Code/component.wasm";
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

/// Closed versioned metadata. Version one is data-only; version two requires
/// the component behavior feature and an exact admitted executable closure.
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
    /// Signed color styles and variables, owned by this stable LibraryId.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styles: Option<super::styles::DocumentStyles>,
    /// Version-two declarations; version one must contain none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behaviors: Vec<BehaviorDeclaration>,
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

/// A signed attachment to an existing admitted input node. Paths are relative
/// to this definition; imports map only definition identity, never these paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorDeclaration {
    pub id: u64,
    pub definition: DefinitionId,
    pub node: NodeId,
    pub properties: Vec<BehaviorProperty>,
    pub local: Vec<LocalProperty>,
    pub outlets: Vec<BehaviorOutlet>,
    pub can_capture: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorProperty {
    pub id: u64,
    pub source: PropertyPath,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProperty {
    pub id: u64,
    pub default: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorOutlet {
    pub id: u64,
    pub source: PropertyPath,
    pub value_type: ValueType,
}
/// Exact independently signed guest transport. SHA-256 is identity, not trust.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentCode {
    pub abi: u16,
    pub sha256: Revision,
    pub bytes: Vec<u8>,
}
