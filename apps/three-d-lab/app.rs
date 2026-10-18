// SPDX-License-Identifier: Apache-2.0
#[path = "state/schema.rs"]
mod state;
mod ui;

pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "three-d-lab",
        "3D Lab",
        "canvas-app",
        &[
            "ui.rs",
            "state/schema.rs",
            "sdk:canvas-authoring-v2",
            "presentation:motion-v1",
        ],
    )
}

pub fn ui() -> otrios_interfaces::canvas::Ui {
    ui::description(state::schema())
}
