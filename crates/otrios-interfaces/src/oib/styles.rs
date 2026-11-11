// SPDX-License-Identifier: Apache-2.0
//! RFC 0037 first color-only style contract; no runtime resolver or authority.
use super::{
    designer::{ClassId, DefinitionId, DocumentId, PropertyPath},
    library::{LibraryId, Revision},
};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "otrios-oib-color-styles-v1";
pub const HEADER: &[u8] = b"OTriOS\0oib-color-styles-v1\0";
pub const MAX_SYSTEMS: usize = 8;
pub const MAX_VARIABLES: usize = 256;
pub const MAX_STYLES: usize = 128;
pub const MAX_CONTRACTS: usize = 128;
pub const MAX_FIELDS: usize = 32;
pub const MAX_ENTRIES: usize = 8192;
pub const MAX_OVERRIDES: usize = 4096;
pub const MAX_BYTES: usize = 256 * 1024;

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}
identity!(SystemId);
identity!(VariableId);
identity!(ContractId);
identity!(StyleId);
identity!(FieldId);

/// A library namespace includes its exact admitted revision. These bytes do not
/// authenticate that owner; package admission must supply the trusted owner set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Owner {
    Document(DocumentId),
    Library {
        library: LibraryId,
        revision: Revision,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemRef {
    pub owner: Owner,
    pub system: SystemId,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableRef {
    pub system: SystemRef,
    pub variable: VariableId,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleRef {
    pub system: SystemRef,
    pub style: StyleId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootMode {
    System,
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeSelection {
    Inherit,
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Availability {
    Active,
    Retired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorModes {
    pub light: [u8; 4],
    pub dark: [u8; 4],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub id: VariableId,
    pub name: String,
    pub availability: Availability,
    pub values: ColorModes,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Domain {
    Body,
    Focus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: FieldId,
    pub name: String,
    pub domain: Domain,
}
/// All fields are required Color fields in v1. Geometry/typography are not
/// silently admitted by this schema; they require a later version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub id: ContractId,
    pub name: String,
    pub classes: Vec<ClassId>,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldValue {
    Literal([u8; 4]),
    Variable(VariableRef),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub field: FieldId,
    pub value: FieldValue,
}
/// Declaration order defines canonical patch priority; focus never enters body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum State {
    Selected,
    Hover,
    Pressed,
    Disabled,
    Focus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub state: State,
    pub fields: Vec<Entry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub id: StyleId,
    pub name: String,
    pub availability: Availability,
    pub contract: ContractId,
    /// Complete defaults, including dormant focus-overlay fields.
    pub defaults: Vec<Entry>,
    pub patches: Vec<Patch>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct System {
    pub reference: SystemRef,
    pub name: String,
    pub variables: Vec<Variable>,
    pub contracts: Vec<Contract>,
    pub styles: Vec<Style>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: String,
    pub systems: Vec<System>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    All,
    State(State),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub field: FieldId,
    pub scope: Scope,
    pub value: FieldValue,
}
/// Local presentation context, never shared authored disabled/focus state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateMask {
    pub selected: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub disabled: bool,
    pub focus_visible: bool,
}

/// Explicit color inputs to a component. Names are display metadata; targets
/// use the existing semantic property paths, never renderer IDs or selectors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyMapping {
    pub field: FieldId,
    /// Canonically sorted, distinct paths relative to ComponentStyle.definition.
    pub targets: Vec<PropertyPath>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentStyle {
    pub definition: DefinitionId,
    pub style: StyleRef,
    /// Complete Body-domain mapping. Focus remains a separate overlay contract.
    pub fields: Vec<PropertyMapping>,
}
