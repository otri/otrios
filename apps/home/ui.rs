// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;
pub fn description() -> Ui {
    AppUi::new("home", "Home", schema("home-state-v1", &[])).build(
        "OTriOS / Apps",
        vec![
            text("Canvas(uiRef, stateRef) / app package prototype").position([55., 125.]),
            button("Component Playground")
                .navigate_to(Destination::app("component-playground"))
                .frame([55., 230., 550., 120.]),
            button("Layout Lab")
                .navigate_to(Destination::app("layout-lab"))
                .frame([660., 230., 550., 120.]),
            text("Migration queue").position([55., 425.]).font_scale(3.),
            text("Input Lab   /   Animation Lab   /   3D Lab").position([55., 500.]),
            text("Monitoring: private capability required").position([55., 560.]),
            text("Live dashboard stays available in its existing viewer").position([55., 690.]),
        ],
    )
}
