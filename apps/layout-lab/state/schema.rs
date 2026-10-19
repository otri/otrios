// SPDX-License-Identifier: Apache-2.0
use crate::authoring::StateField;
pub const PANEL_WIDTH: StateField = StateField::normalized(1, "panelWidth", 500);
pub const CORNER_RADIUS: StateField = StateField::normalized(2, "cornerRadius", 400);
pub const TOGGLE: StateField = StateField::normalized(3, "toggle", 1000);
pub fn schema() -> otrios_interfaces::canvas::StateSchema {
    crate::authoring::schema("layout-lab-state-v1", &[PANEL_WIDTH, CORNER_RADIUS, TOGGLE])
}
