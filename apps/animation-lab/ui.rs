// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::canvas::{Presentation, StateSchema, Ui};

fn slider(label: &str, field: StateField, frame: [f32; 4]) -> Element {
    canvas(vec![
        rounded_rect(CARD)
            .frame([0., 20., frame[2], 24.])
            .corner_radius(12.0.into()),
        rounded_rect(ACCENT)
            .frame([0., 20., frame[2], 24.])
            .corner_radius(12.0.into())
            .width(field.mapped(24., frame[2])),
    ])
    .frame(frame)
    .on_drag(label, Event::slider(field))
}

pub fn description(state_schema: StateSchema) -> Ui {
    AppUi::new("animation-lab", "Animation Lab", state_schema)
        .with_presentation(Presentation::Motion {
            three_d: false,
            playing: PLAYING_ID,
            speed: SPEED_ID,
            count: COUNT_ID,
            load: LOAD_ID,
        })
        .build(
            "Animation Lab",
            vec![
                button("Home")
                    .navigate_to(Destination::app("home"))
                    .frame([1020., 35., 200., 80.])
                    .pin_right(60.)
                    .pin_top(35.),
                button("Play / Pause")
                    .on_press(Event::toggle(PLAYING))
                    .frame([80., 170., 220., 80.]),
                text(PLAYING.choose_text("Paused", "Playing")).position([80., 130.]),
                text(SPEED.format_number("Speed ", "×", 0.00375, 0.25)).position([360., 145.]),
                slider("Animation speed", SPEED, [360., 180., 330., 70.]),
                text(COUNT.format_integer("Objects ", "", 0.063, 1.)).position([770., 145.]),
                slider("Animated object count", COUNT, [770., 180., 350., 70.]),
                canvas(vec![]).frame([80., 280., 1120., 400.]),
                button("State load")
                    .on_press(Event::stress())
                    .frame([980., 695., 220., 45.]),
            ],
        )
}
