// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use otrios_interfaces::canvas::{StateSchema, Ui};
pub fn description(state_schema: StateSchema) -> Ui {
    crate::components::motion_screen(
        "three-d-lab",
        "3D Animations",
        state_schema,
        true,
        [PLAYING, SPEED, COUNT, LOAD],
        &[("OTriOS", "home"), ("Test Playground", "test-playground")],
    )
}
