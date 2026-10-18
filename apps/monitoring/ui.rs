// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::{Presentation, Ui};
pub fn description() -> Ui {
    AppUi::new(
        "monitoring",
        "Monitoring",
        schema("monitoring-state-v2", &[]),
    )
    .with_presentation(Presentation::View {
        frame: [64., 80., 1152., 720.],
    })
    .build(
        "Monitoring",
        vec![
            button("Home")
                .navigate_to(Destination::app("home"))
                .frame([1020., 20., 200., 70.]),
        ],
    )
}
