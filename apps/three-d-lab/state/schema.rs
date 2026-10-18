// SPDX-License-Identifier: Apache-2.0
use crate::authoring::StateField;

// This schema has its own State root; matching IDs in Animation Lab do not share
// intent. The presenter owns its local phase and GPU resources separately.
pub const PLAYING_ID: u16 = 1;
pub const SPEED_ID: u16 = 2;
pub const COUNT_ID: u16 = 3;
pub const LOAD_ID: u16 = 4;

pub const PLAYING: StateField = StateField::normalized(PLAYING_ID, "playing", 1000);
pub const SPEED: StateField = StateField::normalized(SPEED_ID, "speed", 200);
pub const COUNT: StateField = StateField::normalized(COUNT_ID, "count", 175);

pub const LOAD: StateField = StateField::normalized(LOAD_ID, "load", 0);

pub fn schema() -> otrios_interfaces::canvas::StateSchema {
    crate::authoring::schema("three-d-lab-state-v1", &[PLAYING, SPEED, COUNT, LOAD])
}
