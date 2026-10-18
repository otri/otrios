// SPDX-License-Identifier: LGPL-3.0-only
//! Opt-in app-scoped Canvas runtime. No filesystem or network access in guest code.
mod behavior;
mod client;
mod state;
pub use behavior::Behavior;
pub use client::Client;
use otrios_interfaces::canvas::*;
use sha2::{Digest, Sha256};
pub use state::{Document, Session};
pub type Result<T> = std::result::Result<T, String>;
pub const MAX_BYTES: usize = 512 * 1024;
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
codec!(encode_client, decode_client, ClientMessage);
codec!(encode_server, decode_server, ServerMessage);
codec!(encode_ui, decode_ui, Ui);
codec!(encode_app, decode_app, App);
codec!(encode_schema, decode_schema, StateSchema);

pub fn validate_schema(s: &StateSchema) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    if s.id.is_empty() || s.id.len() > 96 || s.fields.len() > 5 {
        return Err("Invalid Canvas schema".into());
    }
    for f in &s.fields {
        if !(1..=5).contains(&f.id) || f.default > 1000 || f.name.len() > 64 || !ids.insert(f.id) {
            return Err("Invalid Canvas field".into());
        }
    }
    Ok(())
}
