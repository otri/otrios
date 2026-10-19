// SPDX-License-Identifier: Apache-2.0
//! Reusable control definition. Its design canvas is independent of each instance frame.
use crate::authoring::*;
const PANEL: [u8; 4] = [37, 59, 78, 255];
pub fn resizable_card(title: &str, width: StateField, radius: ValueExpr) -> Element {
    component(
        [460., 240.],
        vec![
            rounded_rect(PANEL).corner_radius(radius).pin_edges(0.),
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
