// SPDX-License-Identifier: Apache-2.0
//! Bounded D2 definition edits and atomic working-copy Apply.
//!
//! Authority admission checks generation, authorization and durable receipts
//! before publishing the result of a batch. No operation contains
//! executable code, renderer indices or replacement layouts. Apply adds explicit
//! causal property writes to the same private, bounded publication candidate.
use super::designer::{
    DefinitionId, InstanceId, NodeId, ObjectAddress, PropertyId, PropertyPath, SlotId,
};
use crate::{canvas::Projection, layout::Value};
use serde::{Deserialize, Serialize};

pub const MAX_OPERATIONS: usize = 16;
pub const MAX_BATCH_BYTES: usize = 8 * 1024;
pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
pub const MAX_PLACEMENT_BYTES: usize = 4 * 1024;
pub const MAX_CONNECTION_BYTES: usize = 4 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 24 * 1024;

fn zero_property_cursor(value: &u64) -> bool {
    *value == 0
}

/// One private working copy, submitted as a bounded OIB-only atomic candidate.
/// The baseline lock identifies the exact admitted import/source closure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apply {
    pub epoch: Vec<u8>,
    pub generation: u64,
    pub baseline: Vec<u8>,
    pub operations: Vec<DocumentEdit>,
    pub changes: Vec<super::Change>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyReceipt {
    pub sequence: u64,
    pub digest: Vec<u8>,
    pub result: ApplyResult,
    /// Exact post-publication leaf versions, ordered like Apply.changes.
    /// Empty on rejection or legacy receipts; absence is never proof for Undo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_versions: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyResult {
    Accepted {
        generation: u64,
        structural_generation: u64,
    },
    Rejected {
        message: String,
    },
}

/// One generation-guarded structural intent. Definition operations precede
/// instance operations; each phase keeps its authored order. Restore is an
/// explicit singleton command. The authority publishes the whole batch once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditBatch {
    pub generation: u64,
    pub operations: Vec<DocumentEdit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
// Inline graph edits are bounded by MAX_OPERATIONS and MAX_BATCH_BYTES.
#[allow(clippy::large_enum_variant)]
pub enum DocumentEdit {
    Definition(DefinitionEdit),
    Preset(PresetEdit),
    Hierarchy(HierarchyEdit),
    Restore { defaults: bool },
}

impl EditBatch {
    pub fn insert(
        generation: u64,
        prototype: u64,
        before: Option<u64>,
        name: Option<String>,
    ) -> Self {
        Self::hierarchy(HierarchyBatch {
            generation,
            operations: vec![HierarchyEdit::Insert {
                prototype: InstanceId(prototype),
                name,
                destination: Destination::Canvas,
                before: before.map(InstanceId),
                frame: None,
            }],
        })
    }
    pub fn reorder(generation: u64, instance: u64, before: Option<u64>) -> Self {
        Self::hierarchy(HierarchyBatch {
            generation,
            operations: vec![HierarchyEdit::Reorder {
                instance: InstanceId(instance),
                before: before.map(InstanceId),
            }],
        })
    }
    pub fn remove(generation: u64, instance: u64) -> Self {
        Self::hierarchy(HierarchyBatch {
            generation,
            operations: vec![HierarchyEdit::Remove {
                instance: InstanceId(instance),
            }],
        })
    }
    pub fn definitions(batch: DefinitionBatch) -> Self {
        Self {
            generation: batch.generation,
            operations: batch
                .operations
                .into_iter()
                .map(DocumentEdit::Definition)
                .collect(),
        }
    }
    pub fn hierarchy(batch: HierarchyBatch) -> Self {
        Self {
            generation: batch.generation,
            operations: batch
                .operations
                .into_iter()
                .map(DocumentEdit::Hierarchy)
                .collect(),
        }
    }
    pub fn restore(generation: u64, defaults: bool) -> Self {
        Self {
            generation,
            operations: vec![DocumentEdit::Restore { defaults }],
        }
    }
}

/// The one accepted structural source. Layout, semantics, controls, hierarchy
/// projections and compiler tables are derived from this record and the signed
/// class implementation surface. Property registers remain independently causal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSource {
    pub definitions: Vec<super::designer::ComponentDefinition>,
    pub presets: Vec<super::graph::Preset>,
    pub instances: Vec<InstanceRecord>,
    /// Shared high-water mark for authority-created definitions, nodes, slots,
    /// presets and instances. Deleted identities are never recycled.
    pub next_identity: u64,
    /// Authority-maintained mapping from signed default roles to their current
    /// identities after restoration. Missing entries mean identity mapping.
    pub baseline: Vec<BaselineIdentity>,
    pub placements: Vec<Placement>,
    pub connections: Vec<AppConnection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BaselineIdentity {
    Definition {
        source: DefinitionId,
        current: DefinitionId,
    },
    Node {
        definition: DefinitionId,
        source: NodeId,
        current: NodeId,
    },
    /// Contextual endpoint role within the signed/source definition namespace.
    Property {
        definition: DefinitionId,
        source: PropertyId,
        current: PropertyId,
    },
    Slot {
        definition: DefinitionId,
        source: SlotId,
        current: SlotId,
    },
    Preset {
        source: u64,
        current: u64,
    },
    Instance {
        source: InstanceId,
        current: InstanceId,
    },
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
pub struct ConnectionSource {
    pub field: u16,
    pub projection: Projection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConnection {
    pub object: ObjectAddress,
    pub property: PropertyId,
    /// Semantic signed source; selector locals are disposable compiler output.
    pub source: ConnectionSource,
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
        source: Option<ConnectionSource>,
    },
    Insert {
        prototype: InstanceId,
        /// Optional display name; identity is always allocated by the authority.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
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
    /// Reorder within the current destination, retaining placement and ownership.
    Reorder {
        instance: InstanceId,
        before: Option<InstanceId>,
    },
    Rename {
        instance: InstanceId,
        name: String,
    },
    Place {
        instance: InstanceId,
        frame: [f32; 4],
    },
    /// Owned descendants must be moved or removed explicitly in the same batch.
    Remove {
        instance: InstanceId,
    },
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
    /// Both IDs are fresh reservations in the batch's contiguous authority
    /// interval. Phase ordering may claim them before an earlier preset ID.
    CreateDefinition {
        definition: DefinitionId,
        name: String,
        root: super::designer::DefinitionNode,
        /// Initial endpoint namespace for a fresh definition identity. Sparse
        /// tables preserve retired local IDs when recreating a removed graph.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        properties: Vec<super::designer::PropertyDescriptor>,
        #[serde(default, skip_serializing_if = "zero_property_cursor")]
        next_property: u64,
    },
    /// References and presets must be explicitly repaired in this same batch.
    DeleteDefinition { definition: DefinitionId },
    CreateNode {
        definition: DefinitionId,
        node: super::designer::DefinitionNode,
        visual_parent: Option<NodeId>,
        before: Option<NodeId>,
    },
    /// Unlinks direct containment. Other references are never silently repaired.
    DeleteNode {
        definition: DefinitionId,
        node: NodeId,
    },
    SetVisual {
        definition: DefinitionId,
        node: NodeId,
        visual: Option<super::graph::Visual>,
    },
    /// Create the next definition-owned endpoint or update its descriptor in
    /// place. Rename preserves identity; type and capability policy are fixed.
    SetProperty {
        definition: DefinitionId,
        descriptor: super::designer::PropertyDescriptor,
    },
    /// Remove a declared endpoint, its root fallback and outgoing forwarding
    /// row. Other dependencies require explicit repair in the same candidate.
    /// The definition's local allocation cursor never moves backward.
    DeleteProperty {
        definition: DefinitionId,
        property: PropertyId,
    },
    SetSlots {
        definition: DefinitionId,
        slots: Vec<super::designer::SlotDescriptor>,
    },
    /// Change authored fallback without changing causal override registers.
    SetDefault {
        definition: DefinitionId,
        node: NodeId,
        property: PropertyId,
        value: Value,
    },
    /// Remove only the authored fallback; causal overrides remain untouched.
    ClearDefault {
        definition: DefinitionId,
        node: NodeId,
        property: PropertyId,
    },
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

/// Authored palette entries use typed field sources under the signed grants.
/// Changing a preset's definition while instances still reference it requires
/// explicit instance removal/replacement in the same edit batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PresetEdit {
    Create { preset: super::graph::Preset },
    Update { preset: super::graph::Preset },
    Delete { preset: u64 },
}

/// Only existing literal and exposed-property sources. This cannot introduce an
/// app field, action, effect, class implementation or cross-document reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum LiteralSource {
    Literal(Value),
    Forward(PropertyPath),
}
