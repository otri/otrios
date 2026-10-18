// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::canvas::{StateSchema, Ui};
fn slider(label: &str, field: StateField, y: f32) -> Element {
    canvas(vec![
        rounded_rect(CARD).frame([0., 20., 520., 40.]),
        rounded_rect(ACCENT)
            .frame([0., 20., 520., 40.])
            .width(field.mapped(32., 520.)),
    ])
    .frame([80., y, 520., 80.])
    .on_drag(label, Event::slider(field))
}
pub fn description(state_schema: StateSchema) -> Ui {
    AppUi::new("layout-lab", "Layout Lab", state_schema).build(
        "Layout Lab",
        vec![
            button("Home")
                .navigate_to(Destination::app("home"))
                .frame([1020., 35., 200., 80.]),
            text("Panel width").position([80., 170.]).font_scale(3.),
            slider("Panel width", PANEL_WIDTH, 230.),
            text("Corner radius").position([80., 390.]).font_scale(3.),
            slider("Corner radius", CORNER_RADIUS, 450.),
            rounded_rect(ACCENT)
                .frame([720., 250., 400., 300.])
                .width(PANEL_WIDTH.mapped(160., 510.))
                .corner_radius(CORNER_RADIUS.mapped(0., 75.)),
            text("Independent UI and State roots / standard declarative setters")
                .position([80., 700.]),
        ],
    )
}
