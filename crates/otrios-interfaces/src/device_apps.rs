// SPDX-License-Identifier: Apache-2.0
//! Device installation metadata. This is neither Space policy nor user app data.
use serde::{Deserialize, Serialize};

pub const MAX_BYTES: usize = 8192;
pub const MAX_REMOVED: usize = 128;

/// The verified release catalog minus these roots is this device's namespace.
/// Code replicas may remain in the verified release cache for rollback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Namespace {
    pub version: u32,
    pub generation: u64,
    pub removed: Vec<String>,
}
