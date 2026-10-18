// SPDX-License-Identifier: Apache-2.0
#[path = "state/schema.rs"]
mod state;
mod ui;
use otrios_interfaces::canvas::*;
pub fn manifest() -> App {
    crate::authoring::app(
        "component-playground",
        "Component Playground",
        "canvas-prototype",
        &[
            "ui.rs",
            "state/schema.rs",
            "behavior/main.wat",
            "sdk:canvas-authoring-v1",
        ],
    )
}
pub fn ui(code: &[u8]) -> Ui {
    ui::description(state::schema(), code)
}
