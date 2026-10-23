// SPDX-License-Identifier: Apache-2.0
use super::{
    behavior::*,
    state::{self, *},
};
use crate::authoring::*;
use otrios_interfaces::canvas::{StateSchema, Ui};

fn instrument(title: &str, subtitle: &str, design: [f32; 2], mut content: Vec<Element>) -> Element {
    let mut children = vec![
        rounded_rect(CARD).pin_edges(0.).corner_radius(20.0.into()),
        text(title)
            .frame([24., 24., design[0] - 48., 28.])
            .color(ACCENT)
            .pin_left(24.)
            .pin_right(24.),
        text(subtitle)
            .frame([24., 62., design[0] - 48., 28.])
            .color(MUTED)
            .pin_left(24.)
            .pin_right(24.),
    ];
    children.append(&mut content);
    component(design, children)
}

pub fn description(state_schema: StateSchema, code: &[u8]) -> Ui {
    let active = ENABLED.choose_color(MUTED, ACCENT);
    let slider = canvas(vec![
        rectangle(TRACK)
            .frame([20., 34., 282., 12.])
            .corner_radius(6.0.into()),
        rounded_rect(ACCENT)
            .color(active.clone())
            .frame([20., 34., 282., 12.])
            .width(LEVEL.mapped(1., 282.))
            .corner_radius(0.0.into()),
        circle(ACCENT)
            .color(active.clone())
            .frame([0., 20., 40., 40.])
            .x(LEVEL.mapped(0., 282.)),
        circle(WHITE)
            .frame([14., 34., 12., 12.])
            .x(LEVEL.mapped(14., 296.)),
    ])
    .frame([24., 158., 322., 80.])
    .center_x()
    .on_drag("Linear slider", Event::slider(LEVEL).reduced_by(SET_LEVEL));

    let mut dial_parts = vec![
        circle(TRACK).frame([12., 12., 186., 186.]),
        circle(BG).frame([22., 22., 166., 166.]),
    ];
    for tick in 0..16 {
        let a = tick as f32 * std::f32::consts::TAU / 16.;
        dial_parts.push(circle(if tick % 4 == 0 { MUTED } else { TRACK }).frame([
            102. + a.sin() * 98.,
            102. - a.cos() * 98.,
            6.,
            6.,
        ]));
    }
    dial_parts.extend([
        circle(ACCENT)
            .color(active)
            .frame([0., 0., 18., 18.])
            .x(ANGLE.turn_x(96., 76.))
            .y(ANGLE.turn_y(96., 76.)),
        circle(CARD).frame([87., 87., 36., 36.]),
        circle(WHITE).frame([101., 101., 8., 8.]),
    ]);
    let dial = canvas(dial_parts)
        .frame([80., 112., 210., 210.])
        .center_x()
        .on_drag("Rotary dial", Event::dial(ANGLE).reduced_by(SET_ANGLE));

    let mut pad_parts = vec![rounded_rect(BG).pin_edges(0.).corner_radius(12.0.into())];
    for division in 1..4 {
        let fraction = division as f32 / 4.;
        pad_parts.extend([
            rectangle(TRACK).frame([352. * fraction, 0., 1., 210.]),
            rectangle(TRACK).frame([0., 210. * fraction, 352., 1.]),
        ]);
    }
    pad_parts.extend([
        rectangle([94, 169, 181, 110])
            .frame([0., 0., 1., 210.])
            .x(PAD_X.mapped(0., 351.)),
        rectangle([94, 169, 181, 110])
            .frame([0., 0., 352., 1.])
            .y(PAD_Y.mapped(0., 209.)),
        circle(TEAL)
            .frame([0., 0., 24., 24.])
            .x(PAD_X.mapped(-12., 340.))
            .y(PAD_Y.mapped(-12., 198.)),
        circle(WHITE)
            .frame([0., 0., 8., 8.])
            .x(PAD_X.mapped(-4., 348.))
            .y(PAD_Y.mapped(-4., 206.)),
    ]);
    let pad = canvas(pad_parts)
        .frame([24., 112., 352., 210.])
        .center_x()
        .key("pad-control")
        .on_drag("Touch pad", Event::pad(PAD_X, PAD_Y).reduced_by(SET_PAD));

    let output = component(
        [370., 116.],
        vec![
            rounded_rect(CARD).pin_edges(0.).corner_radius(18.0.into()),
            text("Accent").position([24., 21.]).color(MUTED),
            text(ENABLED.choose_text("Muted", "Live"))
                .position([24., 55.])
                .font_scale(2.),
            canvas(vec![
                rectangle(TRACK)
                    .color(ENABLED.choose_color(TRACK, [56, 84, 55, 255]))
                    .frame([0., 0., 112., 56.])
                    .corner_radius(28.0.into()),
                circle(WHITE)
                    .frame([8., 8., 40., 40.])
                    .x(ENABLED.mapped(8., 64.)),
            ])
            .frame([220., 30., 112., 56.])
            .pin_right(38.),
        ],
    )
    .key("accent")
    .semantic_label("Toggle")
    .on_press(Event::press(ENABLED).reduced_by(TOGGLE));

    let home = || place("home", [200., 80.]);
    let intro = || place("intro", [752., 60.]).flex_width();
    let linear = || place("linear", [370., 410.]);
    let rotary = || place("rotary", [370., 410.]);
    let pad_slot = || place("pad", [400., 410.]).flex_width();
    let accent = || place("accent", [370., 116.]).flex_width();
    let wide = column(
        24.,
        vec![
            row(16., vec![heading(), home()]),
            intro(),
            row(22., vec![linear(), rotary(), pad_slot()]),
            accent(),
        ],
    );
    let tall = column(
        16.,
        vec![
            column(12., vec![heading(), home()]),
            intro(),
            row(12., vec![linear(), rotary()]),
            pad_slot(),
            accent(),
        ],
    );
    AppUi::new("component-playground", "Component Playground", state_schema)
        .with_composition(wide, tall)
        .with_touch_pad("pad-control")
        .with_behavior(code, &state::FIELDS)
        .build(
            "Component Playground",
            vec![
                button("Home")
                    .navigate_to(Destination::app("home"))
                    .frame([1020., 35., 200., 80.])
                    .key("home"),
                text("Drag, rotate, explore. Shared across devices.")
                    .position([52., 146.])
                    .color(MUTED)
                    .key("intro"),
                instrument(
                    "01 / LINEAR",
                    "Drag the thumb",
                    [370., 410.],
                    vec![
                        slider,
                        text(LEVEL.percent()).position([24., 280.]).font_scale(5.),
                        text("DRAG OUTSIDE TO TEST")
                            .position([24., 368.])
                            .font_scale(2.)
                            .color(MUTED),
                    ],
                )
                .key("linear"),
                instrument(
                    "02 / ROTARY",
                    "Turn the ring",
                    [370., 410.],
                    vec![
                        dial,
                        text(ANGLE.format_integer("", " deg", 0.36, 0.))
                            .position([24., 350.])
                            .font_scale(3.),
                    ],
                )
                .key("rotary"),
                instrument(
                    "03 / TOUCH PAD",
                    "Draw / fades in 5s",
                    [400., 410.],
                    vec![
                        pad,
                        text("Leave a note for a ghost.")
                            .position([24., 350.])
                            .font_scale(2.)
                            .color(MUTED),
                    ],
                )
                .key("pad"),
                output,
            ],
        )
}
