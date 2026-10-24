// SPDX-License-Identifier: Apache-2.0
mod components;
#[path = "state/schema.rs"]
mod state;
mod ui;
pub fn manifest() -> otrios_interfaces::canvas::App {
    crate::authoring::app(
        "layout-lab",
        "Layout Lab",
        "canvas-app",
        &[
            "ui.rs",
            "components.rs",
            "state/schema.rs",
            "sdk:canvas-authoring-v3",
        ],
    )
}
pub fn ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::description(state::schema())
}

pub fn screens() -> Vec<crate::authoring::AppScreen> {
    let root = manifest();
    let child = |path: &str, title: &str| {
        let mut screen = crate::authoring::app(path, title, &root.status, &[]);
        screen.state_ref = root.state_ref.clone();
        screen.dependencies = root.dependencies.clone();
        screen
    };
    vec![
        (root.clone(), ui),
        (child("layout-lab/signal", "Layout Lab · Signal"), signal_ui),
        (child("layout-lab/cards", "Layout Lab · Cards"), cards_ui),
    ]
}

fn signal_ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::screen("layout-lab/signal", "Layout Lab · Signal", true)
}
fn cards_ui(_code: &[u8]) -> otrios_interfaces::canvas::Ui {
    ui::screen("layout-lab/cards", "Layout Lab · Cards", false)
}
