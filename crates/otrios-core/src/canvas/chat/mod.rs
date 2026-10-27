// SPDX-License-Identifier: LGPL-3.0-only
//! Shared room authority, durable journal and common native/browser presentation.
mod authority;
mod client;
pub mod icons;
mod journal;
mod text;
mod view;
use super::{Result, err};
pub use authority::{Delivery, Room};
pub(crate) use client::{Action as UiAction, VIEWPORT};
pub use client::{Client, Info};
pub use otrios_interfaces::chat::*;

fn check(bytes: &[u8], limit: usize) -> Result<()> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err("Chat packet size limit".into());
    }
    crate::flex_guard::validate(bytes, 16, limit * 2, limit)
}
pub fn encode_command(value: &Command) -> Result<Vec<u8>> {
    let bytes = flexbuffers::to_vec(value).map_err(err)?;
    check(&bytes, MAX_COMMAND_BYTES)?;
    Ok(bytes)
}
pub fn decode_command(bytes: &[u8]) -> Result<Command> {
    check(bytes, MAX_COMMAND_BYTES)?;
    flexbuffers::from_slice(bytes).map_err(err)
}
pub fn encode_event(value: &Event) -> Result<Vec<u8>> {
    let bytes = flexbuffers::to_vec(value).map_err(err)?;
    check(&bytes, MAX_EVENT_BYTES)?;
    Ok(bytes)
}
pub fn decode_event(bytes: &[u8]) -> Result<Event> {
    check(bytes, MAX_EVENT_BYTES)?;
    flexbuffers::from_slice(bytes).map_err(err)
}
pub(crate) fn valid_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
pub(crate) fn random(bytes: usize) -> Result<String> {
    let mut value = vec![0; bytes];
    getrandom::fill(&mut value).map_err(err)?;
    Ok(super::hex(&value))
}

#[cfg(test)]
mod tests;
