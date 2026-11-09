// SPDX-License-Identifier: Apache-2.0
//! Bounded D2 definition edits, carried by state protocol v29.
//!
//! Authority admission checks generation, authorization and durable receipts
//! before publishing the result of a batch. No operation contains
//! executable code, renderer indices, replacement layouts or property registers.
use super::designer::{
    DefinitionId, InstanceId, NodeId, ObjectAddress, PropertyId, PropertyPath, SlotId,
};
use crate::layout::{Binding, Value};
use serde::{Deserialize, Serialize};

pub const MAX_OPERATIONS: usize = 16;
pub const MAX_BATCH_BYTES: usize = 8 * 1024;
pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
pub const MAX_PLACEMENT_BYTES: usize = 4 * 1024;
pub const MAX_CONNECTION_BYTES: usize = 4 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 24 * 1024;

/// The one accepted structural source. Layout, semantics, controls, hierarchy
/// projections and compiler tables are derived from this record and the signed
/// class implementation surface. Property registers remain independently causal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSource {
    pub definitions: Vec<super::designer::ComponentDefinition>,
    pub instances: Vec<InstanceRecord>,
    pub next_instance: u64,
    pub placements: Vec<Placement>,
    pub connections: Vec<AppConnection>,
}

/// Durable root identity and preset selection. Renderer slots are derived for
/// each admitted revision and deliberately absent from this record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceRecord {
    pub id: u64,
    pub name: String,
    pub component: u64,
    pub prototype: u64,
}
impl From<&super::Allocation> for InstanceRecord {
    fn from(value: &super::Allocation) -> Self {
        Self {
            id: value.id,
            name: value.name.clone(),
            component: value.component,
            prototype: value.prototype,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConnection {
    pub object: ObjectAddress,
    pub property: PropertyId,
    /// An existing signed local selector and its field identity. Recovery rejects
    /// a later signed package that reassigns this selector to a different field.
    pub source: Binding,
    pub field: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Destination {
    Canvas,
    /// `component` is the full occurrence root declaring this slot. The slot's
    /// signed relative host path supplies the actual visual attachment.
    Slot {
        component: ObjectAddress,
        slot: SlotId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub instance: InstanceId,
    pub destination: Destination,
    /// Parent-local placement, separate from the definition's design properties.
    /// None uses the existing factory size and automatic canvas/slot origin.
    pub frame: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HierarchyBatch {
    pub generation: u64,
    pub operations: Vec<HierarchyEdit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum HierarchyEdit {
    /// None restores the signed binding. This never clears or copies app values.
    Connect {
        object: ObjectAddress,
        property: PropertyId,
        source: Option<Binding>,
    },
    Insert {
        prototype: InstanceId,
        destination: Destination,
        before: Option<InstanceId>,
        frame: Option<[f32; 4]>,
    },
    Move {
        instance: InstanceId,
        destination: Destination,
        before: Option<InstanceId>,
        frame: Option<[f32; 4]>,
    },
    Place {
        instance: InstanceId,
        frame: [f32; 4],
    },
    /// Owned descendants must be moved or removed explicitly in the same batch.
    Remove { instance: InstanceId },
    /// Duplicates the owned subtree with fresh identities and remapped ownership.
    Duplicate {
        instance: InstanceId,
        name: Option<String>,
        destination: Destination,
        before: Option<InstanceId>,
        frame: Option<[f32; 4]>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionBatch {
    /// Observed active document generation. Never silently refreshed on replay.
    pub generation: u64,
    pub operations: Vec<DefinitionEdit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DefinitionEdit {
    RenameDefinition {
        definition: DefinitionId,
        name: String,
    },
    RenameNode {
        definition: DefinitionId,
        node: NodeId,
        name: String,
    },
    /// Ownership and visual containment are distinct, explicit relationships.
    /// `before` must identify a child of `visual_parent`; None appends. A
    /// nonvisual node has no visual parent and cannot name a visual anchor.
    MoveNode {
        definition: DefinitionId,
        node: NodeId,
        owner: NodeId,
        visual_parent: Option<NodeId>,
        before: Option<NodeId>,
    },
    /// Replace this existing exposed property's forwarding targets. This never
    /// creates a new descriptor, property identity or grant. An empty vector
    /// explicitly disconnects the forwarding edges.
    Expose {
        definition: DefinitionId,
        property: PropertyId,
        targets: Vec<PropertyPath>,
    },
    /// Set or explicitly remove an authored reference assignment.
    /// Live occurrence overrides are retained by the caller, not copied here.
    /// App binding/action reconnection requires a separate signed admission API.
    ConnectReference {
        definition: DefinitionId,
        reference: NodeId,
        target: PropertyPath,
        source: Option<LiteralSource>,
    },
}

/// Only existing literal and exposed-property sources. This cannot introduce an
/// app field, action, effect, class implementation or cross-document reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum LiteralSource {
    Literal(Value),
    Forward(PropertyPath),
}
