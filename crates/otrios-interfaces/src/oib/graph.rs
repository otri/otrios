// SPDX-License-Identifier: Apache-2.0
//! Canonical OIB source. Renderer nodes, controls, selectors and property handles
//! are compiler output and are never document identity.
use super::designer::*;
use crate::{
    canvas::{GestureKind, Projection},
    layout::{Expr, ResizeRule, Semantics, Shape},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub class: ClassId,
    pub presented_root: InstanceId,
    pub classes: Vec<ClassDescriptor>,
    pub definitions: Vec<ComponentDefinition>,
    /// App-owned exact dependency pins and stable definition namespace maps.
    /// Imported bodies remain separate from these editable local definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imports: Option<Imports>,
    pub presets: Vec<Preset>,
    pub instances: Vec<Instance>,
    pub grants: Vec<ActionGrant>,
    /// Attachments to the signed tooling layout, not application object IDs.
    pub preview_viewport: u16,
    pub inspector_viewport: u16,
    pub hierarchy_viewport: Option<u16>,
    pub preview_stride: f32,
    pub inspector_content: [f32; 2],
}

pub const IMPORTS_FORMAT: &str = "otrios-oib-imports-v1";
pub const MAX_LIBRARIES: usize = 4;
pub const MAX_IMPORTED_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Imports {
    pub format: String,
    pub locks: Vec<LibraryLock>,
    pub bindings: Vec<ImportBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryLock {
    pub library: super::library::LibraryId,
    pub revision: super::library::Revision,
    pub exports: Vec<LockedExport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedExport {
    pub export: super::library::ExportId,
    pub contract: super::library::Revision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportBinding {
    pub id: u64,
    pub library: super::library::LibraryId,
    pub export: super::library::ExportId,
    /// Complete reachable definition closure. Map entries are stable source
    /// identities, never inferred from export order or package content hashes.
    pub definitions: Vec<DefinitionBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionBinding {
    pub source: DefinitionId,
    pub target: DefinitionId,
}

/// Verified immutable dependency transport, separate from the authored graph.
/// Package admission matches this body to the independently signed exact pin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedLibrary {
    pub library: super::library::LibraryId,
    pub revision: super::library::Revision,
    pub graph: super::library::Graph,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: u64,
    pub name: String,
    pub definition: DefinitionId,
    pub assignments: Vec<Assignment>,
    /// Optional occurrence-default geometry; absence inherits the definition's
    /// design frame so shared geometry edits propagate to linked instances.
    pub frame: Option<[f32; 4]>,
    pub palette: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: InstanceId,
    pub name: String,
    pub preset: u64,
    /// Initialization admits canvas-owned mutable roots and document-owned
    /// nonvisual fixed objects. Owned slot content is authored in DocumentSource.
    pub owner: Owner,
    pub visual_parent: Option<ObjectAddress>,
    pub order: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub target: PropertyPath,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Source {
    Literal(crate::layout::Value),
    Forward(PropertyPath),
    AppBinding { field: u16, projection: Projection },
}

/// A definition node owns its visual declaration. Children and renderer IDs are
/// derived from DefinitionNode.visual_children and reference expansion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visual {
    pub frame: [Expr; 4],
    pub resize: Option<ResizeRule>,
    pub visible: Expr,
    pub shape: Shape,
    pub semantics: Option<Semantics>,
    pub action: Option<Action>,
    pub parameters: Vec<Parameter>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub local: u16,
    pub source: ParameterSource,
    pub projection: Projection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ParameterSource {
    Property(PropertyPath),
    InstanceName {
        prefix: String,
        suffix: String,
        lowercase: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionOperation {
    Set,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub kind: GestureKind,
    pub operation: ActionOperation,
    pub source: PropertyPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionGrant {
    pub field: u16,
    pub kind: GestureKind,
    pub operation: ActionOperation,
}
