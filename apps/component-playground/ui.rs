// SPDX-License-Identifier: Apache-2.0
use super::{
    behavior::*,
    state::{self, *},
};
use crate::authoring::*;
use otrios_interfaces::canvas::{StateSchema, Ui};
pub fn description(state_schema: StateSchema, code: &[u8]) -> Ui {
    let slider = canvas(vec![
        rounded_rect(CARD).frame([0., 22., 520., 36.]),
        rounded_rect(ACCENT)
            .frame([0., 22., 520., 36.])
            .width(LEVEL.mapped(32., 520.)),
    ])
    .frame([80., 260., 520., 80.])
    .on_drag("Linear slider", Event::slider(LEVEL).reduced_by(SET_LEVEL));
    let dial = canvas(vec![
        circle(CARD).frame([0., 0., 180., 180.]),
        circle(ACCENT)
            .frame([80., 0., 20., 20.])
            .y(ANGLE.mapped(0., 160.)),
    ])
    .frame([720., 230., 180., 180.])
    .on_drag("Rotary dial", Event::dial(ANGLE).reduced_by(SET_ANGLE));
    let pad = canvas(vec![
        rounded_rect(CARD).frame([0., 0., 520., 230.]),
        circle(ACCENT)
            .frame([0., 0., 20., 20.])
            .x(PAD_X.mapped(0., 500.))
            .y(PAD_Y.mapped(0., 210.)),
    ])
    .frame([80., 460., 520., 230.])
    .on_drag("Touch pad", Event::pad(PAD_X, PAD_Y).reduced_by(SET_PAD));
    AppUi::new("component-playground", "Component Playground", state_schema)
        .with_presentation(otrios_interfaces::canvas::Presentation::TouchPad { control: 2 })
        .with_behavior(code, &state::FIELDS)
        .build(
            "Component Playground",
            vec![
                button("Home")
                    .navigate_to(Destination::app("home"))
                    .frame([1020., 35., 200., 80.]),
                text("01 / Linear").position([80., 180.]),
                text(LEVEL.percent()).position([440., 180.]).font_scale(3.),
                slider,
                text("02 / Dial").position([720., 180.]),
                dial,
                text(ANGLE.percent()).position([920., 290.]).font_scale(3.),
                text("03 / Touch pad").position([80., 410.]),
                pad,
                button("Toggle")
                    .on_press(Event::press(ENABLED).reduced_by(TOGGLE))
                    .frame([720., 480., 380., 100.]),
                text(ENABLED.on_off()).position([760., 610.]).font_scale(3.),
                text("Binary UI + shared CRDT state + loaded WASM behavior").position([80., 740.]),
            ],
        )
}
