// SPDX-License-Identifier: Apache-2.0
mod components;
#[path = "state/schema.rs"]
mod state;
#[path = "../layout-text-scroll/app.rs"]
mod text_scroll;
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
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description(state::schema())
}

pub fn screens() -> Vec<crate::authoring::AppScreen> {
    vec![(manifest(), ui), (text_scroll::manifest(), text_scroll::ui)]
}
