// SPDX-License-Identifier: Apache-2.0
//! RFC 0035 D1 signed authoring metadata and stable identity proposal.
//!
//! [`super::Seed::designer`] carries these signed records under the required D1
//! package feature. An absent attachment preserves legacy canonical seed bytes.
//! See `docs/OIB_DESIGNER_CONTRACT.md` for canonicalization, lowering, bounds and
//! rejection requirements. Admission/resolution belong in the covered runtime.
use crate::layout::{Binding, Value, ValueType};
use serde::{Deserialize, Serialize};

pub const FEATURE: &str = "otrios-oib-designer-v1";
pub const MAX_DEFINITIONS: usize = 16;
pub const MAX_AUTHORED_OBJECTS: usize = 64;
pub const MAX_CLASS_DESCRIPTORS: usize = 16;
pub const MAX_PROPERTY_DESCRIPTORS: usize = 64;
/// Semantic occurrence ancestry ceiling; existing layout limits can be lower.
pub const MAX_REFERENCE_DEPTH: usize = 16;
pub const MAX_ROOT_CELL_KEYS: usize = 64;

macro_rules! identity {
    ($name:ident) => {
        /// Stable nonzero signed/authority identity, never a name or renderer slot.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}
identity!(DocumentId);
identity!(ClassId);
identity!(DefinitionId);
identity!(NodeId);
identity!(InstanceId);
identity!(PropertyId);
identity!(SlotId);

/// A path relative to a definition. Each boundary is a reference node in the
/// definition reached so far; `node` belongs to the final referenced definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyPath {
    pub references: Vec<NodeId>,
    pub node: NodeId,
    pub property: PropertyId,
}

/// Device-local selection/semantic identity for an expanded object.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectAddress {
    pub instance: InstanceId,
    pub references: Vec<NodeId>,
    pub node: NodeId,
}

/// Local inspector scope only. Object includes visual and nonvisual occurrences;
/// neither selection, expansion nor mode belongs in the signed/live document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Document(DocumentId),
    Definition(DefinitionId),
    DefinitionNode {
        definition: DefinitionId,
        node: NodeId,
    },
    Object(ObjectAddress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignerMode {
    Edit,
    Test,
}

/// Complete register key within one admitted document. A definition edit and an
/// occurrence override are distinct even when they currently resolve equally.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PropertyAddress {
    Definition {
        definition: DefinitionId,
        node: NodeId,
        property: PropertyId,
    },
    Occurrence {
        instance: InstanceId,
        path: PropertyPath,
    },
}

/// Explicit nonvisual owner. Definitions and root instances belong to Document;
/// signed nested references remain template nodes, not separately allocated roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    Document,
    Instance(InstanceId),
}

/// D1 document-owned objects are linked instances of signed definitions. A
/// nonvisual object uses a definition rooted in OIBObject and has no layout node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub class: ClassId,
    pub presented_root: InstanceId,
    /// Optional tooling viewport in the existing layout. Ordinary application
    /// documents can omit designer chrome; inspector/preview use Seed surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy_viewport: Option<u16>,
    pub classes: Vec<ClassDescriptor>,
    pub definitions: Vec<ComponentDefinition>,
    pub instances: Vec<ComponentInstance>,
    pub mappings: Mappings,
}

/// Keys for existing admitted host implementations, not executable class code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassKind {
    OIBObject,
    OIBDocument,
    O2DView,
    O2DPanel,
    O2DShape,
    O2DText,
    O2DButton,
    O2DToggle,
    O2DSlider,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassDescriptor {
    pub id: ClassId,
    pub name: String,
    pub implementation: ClassKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<ClassId>,
    pub properties: Vec<PropertyDescriptor>,
    /// Admitted class IDs (including their admitted subclasses); empty forbids children.
    pub owned_child_classes: Vec<ClassId>,
    pub visual_child_classes: Vec<ClassId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorGroup {
    Identity,
    Placement,
    Layout,
    Appearance,
    Behavior,
    Component,
    Connections,
}

/// Editors supported by existing button/toggle controls and signed Cell.choices.
/// TextSuggestions does not promise an arbitrary free-text editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyEditor {
    ReadOnly,
    TextSuggestions,
    NumberSuggestions,
    Palette,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyDescriptor {
    pub id: PropertyId,
    pub name: String,
    pub label: String,
    pub value_type: ValueType,
    pub default: Value,
    /// Optional tighter inclusive bounds, within the existing OIB numeric range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_range: Option<[f32; 2]>,
    /// Signed positive step for NumberSuggestions, bounded by number_range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_step: Option<f32>,
    /// Display-only units, e.g. "px"; never a conversion or identity key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    pub group: InspectorGroup,
    pub editor: PropertyEditor,
    pub policy: PropertyPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyPolicy {
    pub live_literal: bool,
    pub app_binding: bool,
}

/// Signed fallback only. The corresponding Cell remains the mutable property unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyDefault {
    pub property: PropertyId,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDefinition {
    pub id: DefinitionId,
    pub name: String,
    pub root: NodeId,
    pub nodes: Vec<DefinitionNode>,
    pub exposed: Vec<ExposedProperty>,
    pub slots: Vec<SlotDescriptor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionNode {
    pub id: NodeId,
    pub name: String,
    pub class: ClassId,
    /// Missing only for the definition root. Ownership and visual order are distinct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<NodeId>,
    /// Authored paint/layout order, never identity order. Nonvisual nodes are absent.
    pub visual_children: Vec<NodeId>,
    pub defaults: Vec<PropertyDefault>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<ComponentReference>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentReference {
    pub definition: DefinitionId,
    /// Targets are relative to the referenced definition; Forward sources are
    /// relative to the containing definition. Structure/connections stay signed.
    pub assignments: Vec<Assignment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentInstance {
    pub id: InstanceId,
    pub name: String,
    pub definition: DefinitionId,
    pub owner: Owner,
    /// The presented root and nonvisual instances omit this. Placement uses the
    /// existing parent-local layout; it does not mutate the definition design size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_parent: Option<ObjectAddress>,
    /// Sibling paint order; unique per visual parent. Identity never depends on it.
    pub order: u16,
    pub assignments: Vec<Assignment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub target: PropertyPath,
    pub source: AssignmentSource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AssignmentSource {
    Literal(Value),
    /// Live outside-in forwarding; never copied into a new mutable value store.
    Forward(PropertyPath),
    /// Existing signed layout selector binding. Its app action/responder is
    /// already admitted by the layout; a design edit cannot reconnect it.
    AppBinding(Binding),
}

/// Root property forwards to these internal properties at authored-assignment
/// precedence. It is a real root property/cell, not an alias sharing a register.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposedProperty {
    pub property: PropertyId,
    pub targets: Vec<PropertyPath>,
}

/// Signed declaration only. D1 cannot insert, remove or reparent slot content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDescriptor {
    pub id: SlotId,
    pub name: String,
    pub references: Vec<NodeId>,
    pub node: NodeId,
    pub admitted_classes: Vec<ClassId>,
    /// Inclusive [minimum, maximum] number of owned children.
    pub cardinality: [u16; 2],
    pub layout: SlotLayout,
}

/// Assertions about existing signed layout, not a second layout engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotLayout {
    Absolute,
    ExistingFlow,
}

/// Signed prototype mappings, or a derived admission result for live roots.
/// No values, layout tree or causal context are stored here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mappings {
    pub objects: Vec<ObjectMapping>,
    pub definitions: Vec<DefinitionCellMapping>,
    pub roots: Vec<RootCellMapping>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectMapping {
    pub address: ObjectAddress,
    /// Existing layout TemplateNode.id; absent for nonvisual objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_node: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionCellMapping {
    pub definition: DefinitionId,
    pub node: NodeId,
    pub property: PropertyId,
    /// Existing Seed Cell.id. Render outputs remain that Cell.targets.
    pub cell: u64,
}

/// Immutable signed key table for one root factory. New roots select this table
/// through Allocation.prototype; descriptor/traversal order cannot assign keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootCellMapping {
    pub prototype: InstanceId,
    pub cells: Vec<RootCellKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootCellKey {
    pub path: PropertyPath,
    /// Explicit stable key in 0..64, unique within this factory, never recycled.
    pub key: u8,
    /// Explicit Cell.id alias for the signed initial prototype only. Dynamically
    /// allocated roots use the checked owner/key formula in the contract note.
    pub initial_cell: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::{IntoDeserializer, value::Error};
    use std::collections::BTreeSet;

    #[test]
    fn full_ancestry_and_scope_are_distinct_keys() {
        let occurrence = |instance, references| PropertyAddress::Occurrence {
            instance: InstanceId(instance),
            path: PropertyPath {
                references,
                node: NodeId(3),
                property: PropertyId(1),
            },
        };
        let addresses = BTreeSet::from([
            occurrence(20, vec![NodeId(7), NodeId(9)]),
            occurrence(30, vec![NodeId(7), NodeId(9)]),
            occurrence(20, vec![NodeId(8), NodeId(9)]),
            occurrence(20, vec![NodeId(9)]),
            PropertyAddress::Definition {
                definition: DefinitionId(20),
                node: NodeId(3),
                property: PropertyId(1),
            },
        ]);
        assert_eq!(addresses.len(), 5);
        assert_eq!(
            addresses.iter().next(),
            Some(&PropertyAddress::Definition {
                definition: DefinitionId(20),
                node: NodeId(3),
                property: PropertyId(1),
            })
        );
    }

    #[test]
    fn typed_ids_deserialize_as_existing_unsigned_ids() {
        let result: Result<NodeId, Error> = NodeId::deserialize(7_u64.into_deserializer());
        assert_eq!(result.unwrap(), NodeId(7));
        // Nonzero and admission membership are runtime checks, not codec checks.
        let zero: Result<InstanceId, Error> = InstanceId::deserialize(0_u64.into_deserializer());
        assert_eq!(zero.unwrap(), InstanceId(0));
    }

    #[test]
    fn unknown_fields_and_unadmitted_class_names_are_codec_errors() {
        use serde::de::value::MapDeserializer;
        let fields = [
            ("live_literal", true),
            ("app_binding", false),
            ("script", true),
        ];
        let result =
            PropertyPolicy::deserialize(MapDeserializer::<_, Error>::new(fields.into_iter()));
        assert!(result.is_err());
        let class: Result<ClassKind, Error> =
            ClassKind::deserialize("O2DTextArea".into_deserializer());
        assert!(class.is_err());
    }
}
