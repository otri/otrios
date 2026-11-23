// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::{InkOptions, Ui};
pub fn description() -> Ui {
    let pad = canvas(vec![
        text("Touch Tracing")
            .frame([0., 20., 280., 40.])
            .center_x()
            .font_scale(2.)
            .color(MUTED),
        text("Draw. Pan or pinch to explore.")
            .frame([0., 56., 300., 24.])
            .center_x()
            .font_scale(1.)
            .color(MUTED),
    ])
    .pin_edges(0.)
    .key("drawing-control")
    .on_drag("Drawing surface", Event::ink());
    let toolbar = canvas(vec![
        button("Fit Drawing")
            .frame([0., 0., 126., 60.])
            .on_press(Event::fit_drawing()),
        button("Clear")
            .frame([134., 0., 82., 60.])
            .on_press(Event::clear_ink()),
        button("Exit")
            .frame([224., 0., 82., 60.])
            .on_press(Event::back()),
    ])
    .frame([0., 716., 306., 60.])
    .center_x()
    .pin_bottom(24.);
    AppUi::new(
        "touch-trace",
        "Touch Tracing",
        schema("touch-trace-state-v1", &[]),
    )
    .with_touch_pad("drawing-control")
    .with_ink_options(InkOptions {
        drawing: Some(otrios_interfaces::drawing::Options {
            reference: [1280, 800],
            grid: 80,
        }),
        lifetime_ms: None,
        max_segments: 64,
        fullscreen: true,
    })
    .with_responsive_viewport([320., 300.])
    .build(
        "Touch Tracing",
        vec![rectangle(BG).pin_edges(0.), pad, toolbar],
    )
}
