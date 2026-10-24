// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::{Presentation, Ui};
pub fn description() -> Ui {
    AppUi::new("monitoring", "Monitoring", super::state::schema())
        .with_presentation(Presentation::View {
            frame: [64., 80., 1152., 720.],
        })
        .build(
            "Monitoring",
            vec![
                button("Sign out")
                    .frame([800., 20., 200., 70.])
                    .visible(session_controls_visible())
                    .on_press(Event::sign_out()),
                button("Home")
                    .navigate_to(Destination::app("home"))
                    .frame([1020., 20., 200., 70.]),
            ],
        )
}
