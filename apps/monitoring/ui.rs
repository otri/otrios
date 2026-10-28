// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::{Presentation, Ui};
pub fn description() -> Ui {
    AppUi::new("monitoring", "Monitoring", super::state::schema())
        .with_breadcrumbs(&[("OTriOS", "home")])
        .with_presentation(Presentation::View {
            frame: [24., 112., 1232., 680.],
        })
        .build(
            "Monitoring",
            vec![
                button("Sign out")
                    .frame([1060., 24., 196., 60.])
                    .visible(session_controls_visible())
                    .on_press(Event::sign_out()),
            ],
        )
}
