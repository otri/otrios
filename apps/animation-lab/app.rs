// SPDX-License-Identifier: Apache-2.0
#[path = "state/schema.rs"]
mod state;
mod ui;

pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "animation-lab",
        "Animation Lab",
        "canvas-app",
        &[
            "ui.rs",
            "state/schema.rs",
            "sdk:canvas-authoring-v3",
            "presentation:motion-v1",
        ],
    )
}

pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description(state::schema())
}

pub fn screens() -> Vec<crate::authoring::AppScreen> {
    vec![(manifest(), ui)]
}
