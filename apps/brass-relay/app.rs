// SPDX-License-Identifier: Apache-2.0
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "brass-relay",
        "The Brass Relay",
        "canvas-app",
        &[
            "ui.rs",
            "sdk:canvas-authoring-v5",
            "presentation:chat-v1",
            "session:chat-v1",
        ],
    )
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description()
}
pub fn screens() -> Vec<crate::authoring::AppScreen> {
    vec![(manifest(), ui)]
}
