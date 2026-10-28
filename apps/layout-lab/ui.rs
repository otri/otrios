// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::canvas::{StateSchema, Ui};

pub fn description(state_schema: StateSchema) -> Ui {
    let intro = || place("intro", [752., 32.]).flex_width();
    let controls = || {
        row(
            24.,
            vec![
                place("width", [520., 80.])
                    .minimum([320., 80.])
                    .flex_width(),
                place("radius", [520., 80.])
                    .minimum([320., 80.])
                    .flex_width(),
            ],
        )
    };
    let top = || {
        row(
            24.,
            vec![
                place("toggle", [440., 240.])
                    .minimum([300., 240.])
                    .flex_width(),
                place("graph", [680., 240.])
                    .minimum([320., 240.])
                    .flex_width(),
            ],
        )
    };
    let a = || place("reusable", [520., 240.]).flex_width();
    let b = || place("compact", [460., 220.]).flex_width();
    let wide = column(
        16.,
        vec![
            heading(),
            intro(),
            top(),
            controls(),
            row(32., vec![a(), b()]),
        ],
    );
    let tall = column(
        16.,
        vec![
            heading(),
            intro(),
            top(),
            controls(),
            column(16., vec![a(), b()]),
        ],
    );
    AppUi::new("layout-lab", "Layouts", state_schema)
        .with_breadcrumbs(&[("OTriOS", "home"), ("Test Playground", "test-playground")])
        .with_composition(wide, tall)
        .build(
            "Layouts",
            vec![
                text("One component. Different sizes. The same shared signal.")
                    .color(MUTED)
                    .key("intro"),
                super::components::signal_toggle(TOGGLE).key("toggle"),
                super::components::signal_graph(TOGGLE).key("graph"),
                super::components::labeled_slider(
                    "Panel width",
                    PANEL_WIDTH,
                    PANEL_WIDTH.percent(),
                )
                .key("width"),
                super::components::labeled_slider(
                    "Corner radius",
                    CORNER_RADIUS,
                    CORNER_RADIUS.format_integer("", " px", 0.075, 0.),
                )
                .key("radius"),
                component(
                    [520., 240.],
                    vec![
                        super::components::resizable_card(
                            "Reusable",
                            PANEL_WIDTH,
                            CORNER_RADIUS.mapped(0., 75.),
                        )
                        .frame([0., 0., 460., 240.])
                        .width(PANEL_WIDTH.mapped(320., 520.))
                        .center_x(),
                    ],
                )
                .key("reusable"),
                component(
                    [460., 220.],
                    vec![
                        super::components::resizable_card(
                            "Compact",
                            PANEL_WIDTH,
                            CORNER_RADIUS.mapped(0., 75.),
                        )
                        .frame([0., 0., 460., 220.])
                        .width(PANEL_WIDTH.mapped(280., 420.))
                        .center_x(),
                    ],
                )
                .key("compact"),
            ],
        )
}
