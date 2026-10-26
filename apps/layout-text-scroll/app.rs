// SPDX-License-Identifier: Apache-2.0
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "layout-text-scroll",
        "Text scrolling",
        "experimental",
        &[
            "ui.rs",
            "sdk:canvas-authoring-v3",
            "presentation:text-scroll-v1",
        ],
    )
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description()
}
