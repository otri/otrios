// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::canvas::{StateSchema, Ui};

pub fn description(state_schema: StateSchema) -> Ui {
    let home = || {
        row(
            12.,
            vec![
                place("home", [160., 80.]),
                place("signal-link", [180., 80.]),
                place("cards-link", [180., 80.]),
            ],
        )
    };
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
            row(16., vec![heading(), home()]),
            intro(),
            top(),
            controls(),
            row(32., vec![a(), b()]),
        ],
    );
    let tall = column(
        16.,
        vec![
            column(12., vec![heading(), home()]),
            intro(),
            top(),
            controls(),
            column(16., vec![a(), b()]),
        ],
    );
    AppUi::new("layout-lab", "Layout Lab", state_schema)
        .with_composition(wide, tall)
        .build(
            "Layout Lab",
            vec![
                button("Home")
                    .navigate_to(Destination::app("home"))
                    .key("home"),
                button("Signal screen")
                    .navigate_to(Destination::app("layout-lab/signal").route("signal"))
                    .key("signal-link"),
                button("Cards screen")
                    .navigate_to(Destination::app("layout-lab/cards").route("cards"))
                    .key("cards-link"),
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

/// App-owned child documents; both deliberately bind the parent's shared State.
pub fn screen(path: &str, title: &str, signal: bool) -> Ui {
    let mut nodes = vec![button("Back").on_press(Event::back()).key("back")];
    let mut sections = vec![row(16., vec![heading(), place("back", [200., 80.])])];
    if signal {
        nodes.extend([
            super::components::signal_toggle(TOGGLE).key("toggle"),
            super::components::signal_graph(TOGGLE).key("graph"),
        ]);
        sections.extend([
            place("toggle", [440., 240.]).flex_width(),
            place("graph", [680., 240.]).flex_width(),
        ]);
    } else {
        nodes.extend([
            super::components::labeled_slider("Panel width", PANEL_WIDTH, PANEL_WIDTH.percent())
                .key("width"),
            super::components::labeled_slider(
                "Corner radius",
                CORNER_RADIUS,
                CORNER_RADIUS.format_integer("", " px", 0.075, 0.),
            )
            .key("radius"),
            super::components::resizable_card(
                "Reusable",
                PANEL_WIDTH,
                CORNER_RADIUS.mapped(0., 75.),
            )
            .key("card"),
        ]);
        sections.extend([
            place("width", [520., 80.]).flex_width(),
            place("radius", [520., 80.]).flex_width(),
            place("card", [520., 240.]).flex_width(),
        ]);
    }
    let composition = column(24., sections);
    AppUi::new(path, title, super::state::schema())
        .with_composition(composition.clone(), composition)
        .build(title, nodes)
}
