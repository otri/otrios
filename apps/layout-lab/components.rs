// SPDX-License-Identifier: Apache-2.0
//! Reusable control definition. Its design canvas is independent of each instance frame.
use crate::authoring::*;
const PANEL: [u8; 4] = CARD;
pub fn labeled_slider(label: &str, field: StateField, value: ValueExpr) -> Element {
    component(
        [520., 80.],
        vec![
            text(label).frame([0., 0., 256., 24.]).color(MUTED),
            text(value)
                .frame([416., 0., 104., 24.])
                .color(ACCENT)
                .pin_right(0.),
            component(
                [520., 40.],
                vec![
                    rectangle(TRACK)
                        .frame([0., 14., 520., 10.])
                        .corner_radius(5.0.into())
                        .pin_left(0.)
                        .pin_right(0.),
                    rounded_rect(ACCENT)
                        .frame([0., 14., 520., 10.])
                        .width(field.mapped(1., 520.))
                        .corner_radius(0.0.into())
                        .resizing(Resizing {
                            horizontal: AxisResize::Springs {
                                leading: false,
                                length: true,
                                trailing: true,
                            },
                            vertical: AxisResize::Fixed,
                        }),
                    circle(WHITE)
                        .frame([0., 0., 36., 36.])
                        .x(field.mapped(0., 484.))
                        .resizing(Resizing {
                            horizontal: AxisResize::Springs {
                                leading: true,
                                length: false,
                                trailing: true,
                            },
                            vertical: AxisResize::Fixed,
                        }),
                ],
            )
            .frame([0., 33., 520., 40.])
            .pin_left(0.)
            .pin_right(0.)
            .on_drag(label, Event::slider(field)),
        ],
    )
}
pub fn signal_graph(field: StateField) -> Element {
    let mut children = vec![
        surface(CARD, 20.).pin_edges(0.),
        text("Toggle signal").position([32., 24.]).font_scale(3.),
        text("Data from Last 5min")
            .position([32., 72.])
            .color(MUTED),
    ];
    for row in 0..=4 {
        children.push(
            rectangle(TRACK)
                .frame([32., 124. + row as f32 * 20., 616., 1.])
                .pin_left(32.)
                .pin_right(32.),
        );
    }
    children.push(
        timed_series(field.window_signal(300), ACCENT)
            .frame([32., 124., 616., 80.])
            .pin_left(32.)
            .pin_right(32.),
    );
    component([680., 240.], children)
}
pub fn signal_toggle(field: StateField) -> Element {
    component(
        [440., 240.],
        vec![
            rounded_rect(TRACK).pin_edges(0.).corner_radius(20.0.into()),
            rounded_rect(CARD)
                .color(field.choose_color(CARD, ACCENT))
                .pin_edges(1.5)
                .corner_radius(20.0.into()),
            text("Toggle")
                .position([32., 30.])
                .font_scale(3.)
                .color(field.choose_color(WHITE, BG)),
            text(field.choose_text("OFF", "ON"))
                .position([32., 92.])
                .font_scale(5.)
                .color(field.choose_color(WHITE, BG)),
            component(
                [132., 64.],
                vec![
                    rectangle(TRACK)
                        .color(field.choose_color(TRACK, [112, 91, 55, 255]))
                        .frame([0., 0., 132., 64.])
                        .corner_radius(32.0.into()),
                    circle(WHITE)
                        .frame([8., 8., 48., 48.])
                        .x(field.mapped(8., 72.)),
                ],
            )
            .frame([276., 88., 132., 64.])
            .pin_right(32.),
            text("TAP TO CHANGE")
                .position([32., 192.])
                .color(field.choose_color(MUTED, BG)),
        ],
    )
    .semantic_label("Toggle")
    .on_press(Event::toggle(field))
}
pub fn resizable_card(title: &str, width: StateField, radius: ValueExpr) -> Element {
    component(
        [460., 240.],
        vec![
            rounded_rect(TRACK)
                .corner_radius(radius.clone())
                .pin_edges(0.),
            rounded_rect(PANEL).corner_radius(radius).pin_edges(1.5),
            text(title)
                .frame([24., 16., 340., 16.])
                .pin_left(24.)
                .pin_right(72.)
                .pin_top(16.),
            circle(ACCENT)
                .frame([412., 16., 24., 24.])
                .pin_right(24.)
                .pin_top(16.),
            canvas(vec![
                rounded_rect(BG).pin_edges(0.),
                circle(ACCENT)
                    .frame([194., 32., 24., 24.])
                    .center_x()
                    .center_y(),
            ])
            .frame([24., 64., 412., 88.])
            .spring_width()
            .spring_height(),
            button("Min width")
                .on_press(Event::press(width))
                .frame([24., 168., 412., 48.])
                .pin_left(24.)
                .pin_right(24.)
                .pin_bottom(24.),
        ],
    )
}
