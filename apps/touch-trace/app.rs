// SPDX-License-Identifier: Apache-2.0
mod ui;
use otrios_interfaces::canvas::*;
pub fn manifest() -> App {
    crate::authoring::app(
        "touch-trace",
        "Touch Tracing",
        "canvas-app",
        &["ui.rs", "sdk:canvas-authoring-v3"],
    )
}
pub fn ui(_: &[u8]) -> Ui {
    ui::description()
}
