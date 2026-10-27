// SPDX-License-Identifier: Apache-2.0
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "layout-document",
        "Long Document",
        "experimental",
        &[
            "ui.rs",
            "sdk:canvas-authoring-v4",
            "presentation:text-document-v1",
        ],
    )
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description()
}
