// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;
pub fn description() -> Ui {
    AppUi::new("home", "Home", schema("home-state-v1", &[])).build(
        "OTriOS / Apps",
        vec![
            button("Component Playground")
                .navigate_to(Destination::app("component-playground"))
                .frame([55., 230., 550., 120.]),
            button("Layout Lab")
                .navigate_to(Destination::app("layout-lab"))
                .frame([660., 230., 550., 120.]),
        ],
    )
}
