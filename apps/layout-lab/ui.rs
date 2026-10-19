// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::canvas::{StateSchema, Ui};

fn slider(label: &str, field: StateField, x: f32) -> Element {
    canvas(vec![
        rectangle(TRACK)
            .frame([0., 14., 520., 10.])
            .corner_radius(5.0.into()),
        rounded_rect(ACCENT)
            .frame([0., 14., 520., 10.])
            .width(field.mapped(1., 520.))
            .corner_radius(0.0.into()),
        circle(WHITE)
            .frame([0., 0., 36., 36.])
            .x(field.mapped(0., 484.)),
    ])
    .frame([x, 496., 520., 40.])
    .on_drag(label, Event::slider(field))
}

pub fn description(state_schema: StateSchema) -> Ui {
    let mut graph = vec![
        rounded_rect(CARD).pin_edges(0.).corner_radius(20.0.into()),
        text("Toggle signal").position([32., 24.]).font_scale(3.),
        text("Data from Last 5min")
            .position([32., 72.])
            .color(MUTED),
    ];
    for row in 0..=4 {
        graph.push(rectangle(TRACK).frame([32., 124. + row as f32 * 20., 616., 1.]));
    }
    graph.push(timed_series(TOGGLE.window_signal(300), ACCENT).frame([32., 124., 616., 80.]));
    let toggle = canvas(vec![
        rounded_rect(CARD)
            .color(TOGGLE.choose_color(CARD, ACCENT))
            .pin_edges(0.)
            .corner_radius(20.0.into()),
        text("Toggle")
            .position([32., 30.])
            .font_scale(3.)
            .color(TOGGLE.choose_color(WHITE, BG)),
        text(TOGGLE.choose_text("OFF", "ON"))
            .position([32., 92.])
            .font_scale(5.)
            .color(TOGGLE.choose_color(WHITE, BG)),
        rectangle(TRACK)
            .color(TOGGLE.choose_color(TRACK, [56, 84, 55, 255]))
            .frame([276., 88., 132., 64.])
            .corner_radius(32.0.into()),
        circle(WHITE)
            .frame([284., 96., 48., 48.])
            .x(TOGGLE.mapped(284., 348.)),
        text("TAP TO CHANGE")
            .position([32., 192.])
            .color(TOGGLE.choose_color(MUTED, BG)),
    ])
    .frame([64., 205., 440., 240.])
    .semantic_label("Toggle")
    .on_press(Event::toggle(TOGGLE));

    AppUi::new("layout-lab", "Layout Lab", state_schema).build(
        "Layout Lab",
        vec![
            button("Home")
                .navigate_to(Destination::app("home"))
                .frame([1020., 35., 200., 80.])
                .pin_right(60.)
                .pin_top(35.),
            text("One component. Different sizes. The same shared signal.")
                .position([64., 146.])
                .color(MUTED),
            toggle,
            canvas(graph).frame([536., 205., 680., 240.]),
            text("Panel width").position([64., 463.]).color(MUTED),
            text(PANEL_WIDTH.percent())
                .position([480., 463.])
                .color(ACCENT),
            slider("Panel width", PANEL_WIDTH, 64.),
            text("Corner radius").position([680., 463.]).color(MUTED),
            text(CORNER_RADIUS.format_integer("", " px", 0.075, 0.))
                .position([1090., 463.])
                .color(ACCENT),
            slider("Corner radius", CORNER_RADIUS, 680.),
            super::components::resizable_card(
                "Reusable",
                PANEL_WIDTH,
                CORNER_RADIUS.mapped(0., 75.),
            )
            .frame([64., 550., 460., 240.])
            .width(PANEL_WIDTH.mapped(320., 520.)),
            super::components::resizable_card(
                "Compact",
                PANEL_WIDTH,
                CORNER_RADIUS.mapped(0., 75.),
            )
            .frame([680., 550., 460., 220.])
            .width(PANEL_WIDTH.mapped(280., 420.)),
        ],
    )
}
