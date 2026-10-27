// SPDX-License-Identifier: Apache-2.0
//! Local host presentation metadata; independent of the state wire protocol.
/// Paint order, independent of an app or a backend's GPU slot allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    Base,
    Document,
    Overlay,
    Effects,
}

/// Revisions are local to this composer. A graphics reset must replay resources.
#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub struct Revisions {
    pub content: u32,
    pub placement: u32,
    pub semantics: u32,
    pub effects: u32,
}
