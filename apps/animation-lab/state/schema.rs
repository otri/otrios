// SPDX-License-Identifier: Apache-2.0
use crate::authoring::StateField;

// App-local durable intent. Animation phase, frame statistics and GPU resources
// belong to the presentation owner and never enter these registers.
pub const PLAYING_ID: u16 = 1;
pub const SPEED_ID: u16 = 2;
pub const COUNT_ID: u16 = 3;
pub const LOAD_ID: u16 = 4;

pub const PLAYING: StateField = StateField::normalized(PLAYING_ID, "playing", 1000);
pub const SPEED: StateField = StateField::normalized(SPEED_ID, "speed", 200);
pub const COUNT: StateField = StateField::normalized(COUNT_ID, "count", 175);

pub const LOAD: StateField = StateField::normalized(LOAD_ID, "load", 0);

pub fn schema() -> otrios_interfaces::canvas::StateSchema {
    crate::authoring::schema("animation-lab-state-v1", &[PLAYING, SPEED, COUNT, LOAD])
}
