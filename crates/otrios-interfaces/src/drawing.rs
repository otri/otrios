// SPDX-License-Identifier: Apache-2.0
//! Bounded shared drawing commands; camera state is deliberately not replicated.
use serde::{Deserialize, Serialize};
pub const MAX_SEGMENTS: usize = 64;
pub const MAX_POINTS: usize = 192;
pub const COORDINATE_LIMIT: i32 = 262_144; // quarter-pixel world units
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub reference: [u16; 2],
    pub grid: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub epoch: [u8; 16],
    pub id: [u8; 16],
    pub points: Vec<[i32; 2]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Put(Stroke),
    Clear { expected: [u8; 16], next: [u8; 16] },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewAction {
    Fit,
}
