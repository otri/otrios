// SPDX-License-Identifier: Apache-2.0
#[path = "../animation-lab/app.rs"]
pub mod animation;
#[path = "../component-playground/app.rs"]
pub mod components_demo;
#[path = "../layout-document/app.rs"]
pub mod document;
#[path = "../input-lab/app.rs"]
pub mod input;
#[path = "../layout-lab/app.rs"]
pub mod layouts;
#[path = "../layout-text-scroll/app.rs"]
pub mod panning;
#[path = "state/schema.rs"]
mod state;
#[path = "../three-d-lab/app.rs"]
pub mod three_d;
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "test-playground",
        "Test Playground",
        "canvas-app",
        &["ui.rs", "state/schema.rs", "sdk:canvas-authoring-v3"],
    )
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description()
}
pub fn screens() -> Vec<crate::authoring::AppScreen> {
    let mut screens = vec![(manifest(), ui as fn(&[u8]) -> otrios_interfaces::canvas::Ui)];
    for factory in [
        components_demo::screens,
        layouts::screens,
        input::screens,
        animation::screens,
        three_d::screens,
    ] {
        screens.extend(factory());
    }
    screens.extend([
        (
            document::manifest(),
            document::ui as fn(&[u8]) -> otrios_interfaces::canvas::Ui,
        ),
        (
            panning::manifest(),
            panning::ui as fn(&[u8]) -> otrios_interfaces::canvas::Ui,
        ),
    ]);
    screens
}
