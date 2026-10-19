// SPDX-License-Identifier: Apache-2.0
#[path = "behavior/events.rs"]
mod behavior;
#[path = "state/schema.rs"]
mod state;
mod ui;
use otrios_interfaces::canvas::*;
pub fn manifest() -> App {
    crate::authoring::app(
        "component-playground",
        "Component Playground",
        "canvas-app",
        &[
            "ui.rs",
            "state/schema.rs",
            "behavior/main.wat",
            "behavior/events.rs",
            "sdk:canvas-authoring-v2",
        ],
    )
}
pub fn ui(code: &[u8]) -> Ui {
    ui::description(state::schema(), code)
}
