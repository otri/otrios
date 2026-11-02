// SPDX-License-Identifier: Apache-2.0
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app("live-ui", "Live UI", "canvas-app", &["otrios-oib-v1"])
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description()
}
