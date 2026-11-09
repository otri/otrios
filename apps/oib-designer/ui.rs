// SPDX-License-Identifier: Apache-2.0
//! Canonical graph source with independent runtime tooling chrome.
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;
#[path = "components.rs"]
// Independently packaged apps reuse this same canonical composition source.
#[allow(clippy::duplicate_mod)]
mod components;
fn tooling_panel(key: &str, title: &str, size: [f32; 2]) -> Element {
    component(
        size,
        vec![
            surface(CARD, 12.).pin_edges(0.),
            text(title)
                .frame([16., 12., size[0] - 32., 28.])
                .color(MUTED),
            canvas(vec![])
                .frame([16., 52., size[0] - 32., size[1] - 68.])
                .pin_left(16.)
                .pin_right(16.)
                .pin_top(52.)
                .pin_bottom(16.)
                .key(&format!("{key}-scroll")),
        ],
    )
    .key(key)
}
pub fn description() -> Ui {
    let wide = row(
        24.,
        vec![
            place("hierarchy", [240., 656.]).flex_height(),
            place("preview", [568., 656.]).flex_height(),
            place("inspector", [376., 656.]).flex_height(),
        ],
    )
    .flex_height();
    let tall = column(
        16.,
        vec![
            row(
                16.,
                vec![
                    place("hierarchy", [360., 240.])
                        .minimum([280., 200.])
                        .flex_width()
                        .flex_height(),
                    place("inspector", [376., 240.])
                        .minimum([280., 200.])
                        .flex_width()
                        .flex_height(),
                ],
            ),
            place("preview", [752., 520.])
                .minimum([568., 280.])
                .flex_width()
                .flex_height(),
        ],
    )
    .flex_height();
    AppUi::new("oib-designer", "OIB Designer", super::state::description())
        .with_breadcrumbs(&[("OTriOS", "home"), ("Test Playground", "test-playground")])
        .with_composition(
            column(24., vec![heading(), wide]).flex_height(),
            column(24., vec![heading(), tall]).flex_height(),
        )
        .with_oib_graph(
            components::graph(false),
            "preview-scroll",
            "inspector-scroll",
            Some("hierarchy-scroll"),
        )
        .build(
            "OIB Designer",
            vec![
                tooling_panel("hierarchy", "Hierarchy", [240., 656.])
                    .frame([24., 120., 240., 656.]),
                tooling_panel("preview", "Runtime canvas", [568., 656.])
                    .frame([288., 120., 568., 656.]),
                tooling_panel("inspector", "Selected properties", [376., 656.])
                    .frame([880., 120., 376., 656.]),
            ],
        )
}
