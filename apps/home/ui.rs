// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::*;
pub fn description() -> Ui {
    Ui {
        format: "otrios-canvas-v1".into(),
        ui_ref: "app:home/ui".into(),
        title: "Home".into(),
        state_schema: StateSchema {
            id: "home-state-v1".into(),
            fields: vec![],
        },
        layout: blueprint(
            "OTriOS / Apps",
            vec![
                label(
                    5,
                    [55., 125.],
                    text("Canvas(uiRef, stateRef) / app package prototype"),
                    2.,
                ),
                button(10, [55., 230., 550., 120.], "Component Playground", 1),
                button(20, [660., 230., 550., 120.], "Layout Lab", 2),
                label(30, [55., 425.], text("Migration queue"), 3.),
                label(
                    31,
                    [55., 500.],
                    text("Input Lab   /   Animation Lab   /   3D Lab"),
                    2.,
                ),
                label(
                    32,
                    [55., 560.],
                    text("Monitoring: private capability required"),
                    2.,
                ),
                label(
                    33,
                    [55., 690.],
                    text("Live dashboard stays available in its existing viewer"),
                    2.,
                ),
            ],
        ),
        selectors: vec![],
        handlers: vec![],
        navigation: vec![
            Navigation {
                action: 1,
                ui_ref: "app:component-playground/ui".into(),
                state_ref: "app:component-playground/state".into(),
            },
            Navigation {
                action: 2,
                ui_ref: "app:layout-lab/ui".into(),
                state_ref: "app:layout-lab/state".into(),
            },
        ],
        behavior: None,
    }
}
