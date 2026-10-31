// SPDX-License-Identifier: Apache-2.0
//! Standalone example, intentionally absent from canvas-pack's built-in registry.
#[path = "state/schema.rs"]
mod state;
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "bundle-counter",
        "Bundle Counter",
        "standalone-app",
        &["otrios-canvas-v2", "otrios-canvas-behavior-v1"],
    )
}
pub fn ui(code: &[u8], step: u16) -> otrios_interfaces::canvas::Ui {
    ui::description(code, step)
}
