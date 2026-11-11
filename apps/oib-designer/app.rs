// SPDX-License-Identifier: Apache-2.0
#[path = "state/schema.rs"]
mod state;
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "oib-designer",
        "OIB Designer",
        "canvas-app",
        &[
            "otrios-oib-v2",
            "otrios-oib-instances-v1",
            "otrios-oib-designer-v2",
            "otrios-auto-layout-v1",
        ],
    )
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description()
}
