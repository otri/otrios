// SPDX-License-Identifier: Apache-2.0
mod components;
#[path = "state/schema.rs"]
mod state;
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "layout-lab",
        "Layout Lab",
        "canvas-app",
        &[
            "ui.rs",
            "components.rs",
            "state/schema.rs",
            "sdk:canvas-authoring-v3",
        ],
    )
}
pub fn ui() -> otrios_interfaces::canvas::Ui {
    ui::description(state::schema())
}
