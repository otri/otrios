// SPDX-License-Identifier: Apache-2.0
//! Stable bootstrap control and explicit channels outside canonical state bytes.
use crate::release::Release;
use serde::{Deserialize, Serialize};
pub const VERSION: u32 = 1;
pub const MAX_CONTROL_BYTES: usize = 8192;
pub const STATE_PROTOCOL: &str = "otrios-canvas-sync-v3";
pub const STATE: u8 = 1;
pub const INK: u8 = 2;
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientControl {
    Hello {
        version: u32,
        release: Option<String>,
        state_protocol: String,
        #[serde(default)]
        state: bool,
    },
    Ink {
        enabled: bool,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerControl {
    Hello {
        version: u32,
        release: Option<Release>,
        refresh: bool,
        compatible: bool,
        state_available: bool,
    },
    Upgrade {
        version: u32,
        release: Option<Release>,
        refresh: bool,
        compatible: bool,
        state_available: bool,
    },
    StateStatus {
        version: u32,
        available: bool,
    },
}
impl ServerControl {
    pub fn release(&self) -> Option<&Release> {
        match self {
            Self::Hello { release, .. } | Self::Upgrade { release, .. } => release.as_ref(),
            _ => None,
        }
    }
}
pub fn frame(channel: u8, mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.insert(0, channel);
    bytes
}
