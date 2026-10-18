// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::{canvas::*, layout::*};
pub fn description(state_schema: StateSchema) -> Ui {
    let mut panel = round(40, [720., 250., 400., 300.], ACCENT);
    panel.frame[2] = binding(1);
    panel.shape = Shape::Superellipse {
        radius: binding(2),
        exponent: 4.,
        color: color(ACCENT),
    };
    let sliders =
        [(20, 230., "Panel width", 1), (30, 450., "Corner radius", 2)].map(|(id, y, title, c)| {
            let mut n = control(
                node(id, [80., y, 520., 80.], Shape::Canvas { clip: true }),
                title,
                c,
                true,
            );
            let mut fill = round(id + 2, [0., 20., 520., 40.], ACCENT);
            fill.frame[2] = binding(if c == 1 { 3 } else { 4 });
            n.children = vec![round(id + 1, [0., 20., 520., 40.], CARD), fill];
            n
        });
    Ui {
        format: "otrios-canvas-v1".into(),
        ui_ref: "app:layout-lab/ui".into(),
        title: "Layout Lab".into(),
        state_schema,
        layout: blueprint(
            "Layout Lab",
            vec![
                button(10, [1020., 35., 200., 80.], "Home", 0),
                label(15, [80., 170.], text("Panel width"), 3.),
                sliders[0].clone(),
                label(25, [80., 390.], text("Corner radius"), 3.),
                sliders[1].clone(),
                panel,
                label(
                    50,
                    [80., 700.],
                    text("Independent UI and State roots / standard declarative setters"),
                    2.,
                ),
            ],
        ),
        selectors: vec![
            Selector {
                local: 1,
                field: 1,
                projection: Projection::Number {
                    scale: 0.35,
                    offset: 160.,
                },
            },
            Selector {
                local: 2,
                field: 2,
                projection: Projection::Number {
                    scale: 0.075,
                    offset: 0.,
                },
            },
            Selector {
                local: 3,
                field: 1,
                projection: Projection::Number {
                    scale: 0.488,
                    offset: 32.,
                },
            },
            Selector {
                local: 4,
                field: 2,
                projection: Projection::Number {
                    scale: 0.488,
                    offset: 32.,
                },
            },
        ],
        handlers: vec![
            Handler {
                control: 1,
                event: 1,
                kind: GestureKind::Slider,
                fields: vec![1],
            },
            Handler {
                control: 2,
                event: 2,
                kind: GestureKind::Slider,
                fields: vec![2],
            },
        ],
        navigation: vec![home_nav()],
        behavior: None,
    }
}
