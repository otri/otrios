// SPDX-License-Identifier: LGPL-3.0-only
//! Opt-in app-scoped Canvas runtime. No filesystem or network access in guest code.
mod behavior;
mod client;
pub mod reflow;
mod selection;
mod state;
pub mod text_scroll;
mod text_viewport;
mod transport;
pub use behavior::Behavior;
pub use client::{Client, ResourceUsage};
use otrios_interfaces::canvas::*;
pub use selection::StateSelection;
use sha2::{Digest, Sha256};
pub use state::{
    DispatchPolicy, DispatchUsage, Document, MAX_INTENT_PAYLOAD_BYTES, Session, StateAdapter,
    StateUsage,
};
pub use transport::{
    DecodeBudget, WireUsage, compress_packet_into, decode_client, decode_server,
    decode_state_request, decode_state_response, decoded_packet_bytes, encode_client,
    encode_client_into, encode_server, encode_server_into, encode_state_request,
    encode_state_request_into, encode_state_response, encode_state_response_into, wire_usage,
};
pub type Result<T> = std::result::Result<T, String>;
pub const MAX_BYTES: usize = 512 * 1024;
/// UI schema version is independent of the unchanged state sync protocol.
pub fn validate_ui_format(ui: &Ui) -> Result<()> {
    if !matches!(ui.format.as_str(), "otrios-canvas-v2" | "otrios-canvas-v3")
        || (matches!(ui.presentation, Some(Presentation::TextScroll { .. }))
            && ui.format != "otrios-canvas-v3")
    {
        return Err("Unsupported Canvas UI format".into());
    }
    Ok(())
}

pub fn hash(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|x| format!("{x:02x}")).collect()
}
pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn check(bytes: &[u8]) -> Result<()> {
    use flexbuffers::{FlexBufferType, Reader};
    fn walk(r: Reader<&[u8]>, depth: usize, budget: &mut usize) -> Result<()> {
        if depth > 32 || *budget == 0 {
            return Err("Canvas binary nesting/visit limit".into());
        }
        *budget -= 1;
        if r.flexbuffer_type() == FlexBufferType::Map {
            let m = r.get_map().map_err(err)?;
            if m.len() > 32 {
                return Err("Canvas map limit".into());
            }
            for i in 0..m.len() {
                walk(m.index(i).map_err(err)?, depth + 1, budget)?;
            }
        } else if r.flexbuffer_type().is_vector() {
            let v = r.get_vector().map_err(err)?;
            if v.len() > MAX_BYTES {
                return Err("Canvas vector limit".into());
            }
            for i in 0..v.len() {
                walk(v.index(i).map_err(err)?, depth + 1, budget)?;
            }
        } else if r.length() > MAX_BYTES {
            return Err("Canvas scalar limit".into());
        }
        Ok(())
    }
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err("Canvas message size limit".into());
    }
    crate::flex_guard::validate(bytes, 32, MAX_BYTES * 2, MAX_BYTES)?;
    walk(
        Reader::get_root(bytes).map_err(err)?,
        0,
        &mut (MAX_BYTES * 2),
    )
}
macro_rules! codec {
    ($en:ident,$de:ident,$ty:ty) => {
        pub fn $en(value: &$ty) -> Result<Vec<u8>> {
            let b = flexbuffers::to_vec(value).map_err(err)?;
            check(&b)?;
            Ok(b)
        }
        pub fn $de(bytes: &[u8]) -> Result<$ty> {
            check(bytes)?;
            flexbuffers::from_slice(bytes).map_err(err)
        }
    };
}
codec!(encode_ui, decode_ui, Ui);
codec!(encode_app, decode_app, App);
codec!(encode_catalog, decode_catalog, Catalog);
codec!(encode_schema, decode_schema, StateSchema);

pub const MAX_FIELDS: usize = 64;
/// UI data replacement keeps execution, permissions and state ownership fixed.
pub fn compatible_ui(old: &Ui, next: &Ui) -> Result<()> {
    if next.ui_ref != old.ui_ref
        || old.format != next.format
        || old.state_schema != next.state_schema
        || old.behavior != next.behavior
        || old.handlers != next.handlers
        || old.navigation.len() != next.navigation.len()
        || old.navigation.iter().zip(&next.navigation).any(|(a, b)| {
            a.action != b.action || a.ui_ref != b.ui_ref || a.state_ref != b.state_ref
        })
        || old.presentation != next.presentation
    {
        return Err("UI update requires matching state, behavior and action contracts".into());
    }
    Ok(())
}
/// A guest replacement changes only the code digest, keeping its ABI, grants
/// and the complete UI/state contract fixed. New instances start with fresh RAM.
pub fn compatible_behavior(old: &Ui, next: &Ui) -> Result<()> {
    let (Some(before), Some(after)) = (&old.behavior, &next.behavior) else {
        return Err("Behavior replacement requires an existing declared module".into());
    };
    if before.abi != after.abi || before.writes != after.writes {
        return Err("Behavior replacement changed its ABI or field permissions".into());
    }
    let mut expected = old.clone();
    expected.behavior = next.behavior.clone();
    if &expected != next {
        return Err("Behavior replacement changed UI or state data".into());
    }
    Ok(())
}
pub fn compatible_app_update(old: &Ui, next: &Ui) -> Result<()> {
    compatible_ui(old, next).or_else(|_| compatible_behavior(old, next))
}
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_SERIES_POINTS: usize = 2016;

pub fn validate_schema(s: &StateSchema) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    if s.id.is_empty() || s.id.len() > 96 || s.fields.len() > MAX_FIELDS {
        return Err("Invalid Canvas schema".into());
    }
    for f in &s.fields {
        if !(1..=MAX_FIELDS as u16).contains(&f.id)
            || f.name.is_empty()
            || f.name.len() > 64
            || !ids.insert(f.id)
            || !names.insert(&f.name)
        {
            return Err("Invalid Canvas field".into());
        }
        validate_value(f, &f.default)?;
    }
    Ok(())
}
/// A field's default fixes its variant. Numbers are normalized integer registers;
/// layout projections and series retain their independent floating-point domains.
pub fn validate_value(field: &Field, value: &otrios_interfaces::layout::Value) -> Result<()> {
    use otrios_interfaces::layout::Value;
    if std::mem::discriminant(&field.default) != std::mem::discriminant(value) {
        return Err("Canvas field type mismatch".into());
    }
    let valid = match value {
        Value::Number(n) => n.is_finite() && n.fract() == 0. && (0. ..=1000.).contains(n),
        Value::Text(s) => s.len() <= MAX_TEXT_BYTES,
        Value::Boolean(_) | Value::Color(_) => true,
        Value::Series(values) => {
            values.len() <= MAX_SERIES_POINTS && values.iter().all(|n| n.is_finite())
        }
        Value::TimedSeries(points) => {
            points.len() <= MAX_SERIES_POINTS
                && points.iter().all(|p| {
                    p[0].is_finite()
                        && p[1].is_finite()
                        && (0. ..=1.).contains(&p[0])
                        && (0. ..=100.).contains(&p[1])
                })
        }
    };
    if !valid {
        return Err("Canvas field value exceeds its typed bounds".into());
    }
    Ok(())
}
