// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use crate::components::{LaunchIcon, launch_card};
use otrios_interfaces::canvas::Ui;
pub fn description() -> Ui {
    let arrangement = || {
        column(
            24.,
            vec![
                heading(),
                wrap_cards(
                    &["brass-relay", "test-playground", "monitoring"],
                    [384., 176.],
                    [336., 176.],
                    [16., 24.],
                ),
            ],
        )
    };
    AppUi::new("home", "Home", super::state::schema())
        .with_brass_theme()
        .with_composition(arrangement(), arrangement())
        .build(
            "OTriOS",
            vec![
                launch_card(
                    "brass-relay",
                    "brass-relay",
                    "The Brass Relay",
                    "A shared room on the wire",
                    LaunchIcon::Chat,
                ),
                launch_card(
                    "test-playground",
                    "test-playground",
                    "Test Playground",
                    "Explore the demos",
                    LaunchIcon::Playground,
                ),
                launch_card(
                    "monitoring",
                    "monitoring",
                    "Monitoring",
                    "Live service data",
                    LaunchIcon::Monitoring,
                ),
            ],
        )
}
