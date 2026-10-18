// SPDX-License-Identifier: Apache-2.0
use crate::authoring::StateField;
use otrios_interfaces::canvas::StateSchema;
// These IDs belong to the persisted schema and guest register ABI. Never reuse them.
pub const LEVEL: StateField = StateField::normalized(1, "level", 420);
pub const ANGLE: StateField = StateField::normalized(2, "angle", 250);
pub const PAD_X: StateField = StateField::normalized(3, "padX", 500);
pub const PAD_Y: StateField = StateField::normalized(4, "padY", 500);
pub const ENABLED: StateField = StateField::normalized(5, "enabled", 1000);
pub const FIELDS: [StateField; 5] = [LEVEL, ANGLE, PAD_X, PAD_Y, ENABLED];
pub fn schema() -> StateSchema {
    crate::authoring::schema("component-playground-state-v1", &FIELDS)
}
