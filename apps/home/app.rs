// SPDX-License-Identifier: Apache-2.0
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "home",
        "Home",
        "canvas-prototype",
        &["ui.rs", "sdk:canvas-authoring-v1"],
    )
}
pub fn ui() -> otrios_interfaces::canvas::Ui {
    ui::description()
}
