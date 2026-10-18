// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
pub const FIRST: StateField = StateField::text(1, "first");
pub const SECOND: StateField = StateField::text(2, "second");
pub fn description() -> otrios_interfaces::canvas::StateSchema {
    schema("input-lab-state-v2", &[FIRST, SECOND])
}
