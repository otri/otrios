// SPDX-License-Identifier: Apache-2.0
use super::state::{self, COUNT};
use crate::authoring::*;
pub fn description(code: &[u8], step: u16) -> otrios_interfaces::canvas::Ui {
    let card = component(
        [720., 240.],
        vec![
            rounded_rect(CARD).pin_edges(0.).corner_radius(24.0.into()),
            text("A small shared counter")
                .frame([32., 32., 640., 32.])
                .font_scale(2.)
                .color(MUTED)
                .pin_left(32.)
                .pin_right(32.),
            text(COUNT.format_integer("", "", 1., 0.))
                .frame([32., 100., 640., 80.])
                .font_scale(5.)
                .pin_left(32.)
                .pin_right(32.),
        ],
    )
    .key("counter");
    let increment = component(
        [360., 112.],
        vec![
            rounded_rect(ACCENT)
                .pin_edges(0.)
                .corner_radius(24.0.into()),
            text(format!("+{step}").as_str())
                .frame([32., 36., 280., 40.])
                .font_scale(3.)
                .pin_left(32.)
                .pin_right(32.),
        ],
    )
    .key("increment")
    .semantic_label("Increase counter")
    .on_press(Event::press(COUNT).reduced_by(BehaviorEvent(1)));
    let reset = component(
        [240., 112.],
        vec![
            rounded_rect(TRACK).pin_edges(0.).corner_radius(24.0.into()),
            text("Reset")
                .frame([32., 40., 176., 32.])
                .font_scale(2.)
                .pin_left(32.)
                .pin_right(32.),
        ],
    )
    .key("reset")
    .semantic_label("Reset counter")
    .on_press(Event::press(COUNT).reduced_by(BehaviorEvent(2)));
    let arrangement = || {
        column(
            24.,
            vec![
                heading(),
                place("counter", [720., 240.]).flex_width(),
                row(
                    24.,
                    vec![
                        place("increment", [360., 112.]).flex_width(),
                        place("reset", [240., 112.]).flex_width(),
                    ],
                ),
            ],
        )
    };
    AppUi::new("bundle-counter", "Bundle Counter", state::schema())
        .with_breadcrumbs(&[("OTriOS", "home")])
        .with_composition(arrangement(), arrangement())
        .with_behavior(code, &[COUNT])
        .build("Bundle Counter", vec![card, increment, reset])
}
