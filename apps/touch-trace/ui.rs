// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::{InkOptions, Ui};
pub fn description() -> Ui {
    let mut grid = vec![rectangle(BG).pin_edges(0.)];
    for i in 1..16 {
        grid.push(
            rectangle(TRACK)
                .frame([i as f32 * 80., 0., 1., 800.])
                .pin_top(0.)
                .pin_bottom(0.),
        );
    }
    for i in 1..10 {
        grid.push(
            rectangle(TRACK)
                .frame([0., i as f32 * 80., 1280., 1.])
                .pin_left(0.)
                .pin_right(0.),
        );
    }
    grid.extend([
        rectangle(ACCENT)
            .frame([0., 0., 1280., 2.])
            .pin_left(0.)
            .pin_right(0.)
            .pin_top(0.),
        rectangle(ACCENT)
            .frame([0., 798., 1280., 2.])
            .pin_left(0.)
            .pin_right(0.)
            .pin_bottom(0.),
        rectangle(ACCENT)
            .frame([0., 0., 2., 800.])
            .pin_left(0.)
            .pin_top(0.)
            .pin_bottom(0.),
        rectangle(ACCENT)
            .frame([1278., 0., 2., 800.])
            .pin_right(0.)
            .pin_top(0.)
            .pin_bottom(0.),
        text("Touch Tracing")
            .frame([0., 100., 280., 40.])
            .center_x()
            .color(MUTED),
        text("Trace the grid and edges.")
            .frame([0., 148., 300., 32.])
            .center_x()
            .color(MUTED),
    ]);
    let pad = canvas(grid)
        .pin_edges(0.)
        .key("drawing-control")
        .on_drag("Drawing surface", Event::ink());
    let toolbar = canvas(vec![
        button("Clear")
            .frame([0., 0., 112., 60.])
            .on_press(Event::clear_ink()),
        button("Exit")
            .frame([124., 0., 112., 60.])
            .on_press(Event::back()),
    ])
    .frame([0., 704., 236., 60.])
    .center_x()
    .pin_bottom(36.);
    AppUi::new(
        "touch-trace",
        "Touch Tracing",
        schema("touch-trace-state-v1", &[]),
    )
    .with_touch_pad("drawing-control")
    .with_ink_options(InkOptions {
        lifetime_ms: None,
        max_segments: 64,
        fullscreen: true,
    })
    .with_responsive_viewport([320., 300.])
    .build("Touch Tracing", vec![pad, toolbar])
}
