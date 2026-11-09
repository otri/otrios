// SPDX-License-Identifier: LGPL-3.0-only
//! Stable semantic admission, separate from an immutable graph/library body.
use super::super::{Result, err, hash};
use otrios_interfaces::{
    canvas::{GestureKind, StateSchema, Ui},
    layout::ValueType,
    oib::{
        designer::{ClassId, ClassKind, PropertyId, PropertyPolicy},
        graph::{ActionGrant, ActionOperation},
    },
};
use serde::Serialize;

#[derive(Serialize)]
struct PropertySignature {
    id: PropertyId,
    value_type: ValueType,
    number_range: Option<[f32; 2]>,
    policy: PropertyPolicy,
}

#[derive(Serialize)]
struct ClassSignature {
    id: ClassId,
    implementation: ClassKind,
    base: Option<ClassId>,
    properties: Vec<PropertySignature>,
    owned_child_classes: Vec<ClassId>,
    visual_child_classes: Vec<ClassId>,
}

#[derive(Serialize)]
struct Admission {
    state_schema: StateSchema,
    classes: Vec<ClassSignature>,
    grants: Vec<ActionGrant>,
}

fn class_set(ids: &[ClassId]) -> Vec<ClassId> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn grant_key(grant: &ActionGrant) -> (u16, u8, u8) {
    let kind = match grant.kind {
        GestureKind::Slider => 0,
        GestureKind::Dial => 1,
        GestureKind::Pad => 2,
        GestureKind::Activate => 3,
    };
    let operation = match grant.operation {
        ActionOperation::Set => 0,
        ActionOperation::Toggle => 1,
    };
    (grant.field, kind, operation)
}

fn class_signature(class: &otrios_interfaces::oib::designer::ClassDescriptor) -> ClassSignature {
    let mut properties = class
        .properties
        .iter()
        .map(|property| PropertySignature {
            id: property.id,
            value_type: property.value_type,
            number_range: property.number_range,
            policy: property.policy,
        })
        .collect::<Vec<_>>();
    properties.sort_unstable_by_key(|property| property.id);
    ClassSignature {
        id: class.id,
        implementation: class.implementation,
        base: class.base,
        properties,
        owned_child_classes: class_set(&class.owned_child_classes),
        visual_child_classes: class_set(&class.visual_child_classes),
    }
}

/// Exact semantic requirement for a library's admitted host class. Defaults,
/// names and editor hints are body/presentation data and do not supply grants.
pub fn class_contract(
    class: &otrios_interfaces::oib::designer::ClassDescriptor,
) -> Result<[u8; 32]> {
    let mut canonical = b"OTriOS\0oib-class-contract-v1\0".to_vec();
    canonical.extend(flexbuffers::to_vec(class_signature(class)).map_err(err)?);
    hash(&canonical)
        .try_into()
        .map_err(|_| "Invalid SHA-256 length".into())
}

/// Hash an already-admitted OIB UI's semantic contract, never its graph body.
///
/// The caller still validates the complete signed UI and compares baseline
/// source separately. Equal admission is not permission to adopt a changed
/// graph. Document incarnation, scope, ui_ref and graph.id belong in the epoch
/// envelope, not this reusable contract digest. Library revisions likewise
/// belong in a separately verified baseline pin.
pub fn admission(ui: &Ui) -> Result<Vec<u8>> {
    let graph = super::seed(ui)
        .and_then(|seed| seed.graph.as_ref())
        .ok_or("Semantic admission requires a canonical OIB graph")?;
    let mut state_schema = ui.state_schema.clone();
    state_schema.fields.sort_unstable_by_key(|field| field.id);
    let mut classes = graph
        .classes
        .iter()
        .map(class_signature)
        .collect::<Vec<_>>();
    classes.sort_unstable_by_key(|class| class.id);
    let mut grants = graph.grants.clone();
    grants.sort_unstable_by_key(grant_key);
    grants.dedup();
    let mut canonical = b"OTriOS\0oib-admission-v1\0".to_vec();
    canonical.extend(
        flexbuffers::to_vec(&Admission {
            state_schema,
            classes,
            grants,
        })
        .map_err(err)?,
    );
    Ok(hash(&canonical))
}
