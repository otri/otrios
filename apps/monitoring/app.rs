// SPDX-License-Identifier: Apache-2.0
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "monitoring",
        "Monitoring",
        "canvas-app",
        &[
            "ui.rs",
            "adapter.rs",
            "capability:monitoring.read",
            "sdk:canvas-authoring-v2",
        ],
    )
}
pub fn ui() -> otrios_interfaces::canvas::Ui {
    ui::description()
}
