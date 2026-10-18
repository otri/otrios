// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::{canvas::*, layout::*};
pub fn description(state_schema: StateSchema, code: &[u8]) -> Ui {
    let mut slider = control(
        node(20, [80., 260., 520., 80.], Shape::Canvas { clip: true }),
        "Linear slider",
        1,
        true,
    );
    slider.children = vec![round(21, [0., 22., 520., 36.], CARD), {
        let mut fill = round(22, [0., 22., 520., 36.], ACCENT);
        fill.frame[2] = binding(1);
        fill
    }];
    let mut dial = control(
        node(30, [720., 230., 180., 180.], Shape::Canvas { clip: true }),
        "Rotary dial",
        2,
        true,
    );
    let mut dot = node(
        32,
        [80., 0., 20., 20.],
        Shape::Circle {
            color: color(ACCENT),
        },
    );
    dot.frame[1] = binding(3);
    dial.children = vec![
        node(
            31,
            [0., 0., 180., 180.],
            Shape::Circle { color: color(CARD) },
        ),
        dot,
    ];
    let mut pad = control(
        node(40, [80., 460., 520., 230.], Shape::Canvas { clip: true }),
        "Touch pad",
        3,
        true,
    );
    let mut cursor = node(
        42,
        [0., 0., 20., 20.],
        Shape::Circle {
            color: color(ACCENT),
        },
    );
    cursor.frame[0] = binding(5);
    cursor.frame[1] = binding(6);
    pad.children = vec![round(41, [0., 0., 520., 230.], CARD), cursor];
    // An ordinary activating responder routes through the same app event table.
    let mut toggle = button(50, [720., 480., 380., 100.], "Toggle", 5);
    toggle.responder = Some(Responder::AppEvent {
        control: 5,
        drag: false,
    });
    let layout = blueprint(
        "Component Playground",
        vec![
            button(10, [1020., 35., 200., 80.], "Home", 0),
            label(15, [80., 180.], text("01 / Linear"), 2.),
            label(16, [440., 180.], binding(2), 3.),
            slider,
            label(25, [720., 180.], text("02 / Dial"), 2.),
            dial,
            label(35, [920., 290.], binding(4), 3.),
            label(36, [80., 410.], text("03 / Touch pad"), 2.),
            pad,
            toggle,
            label(55, [760., 610.], binding(7), 3.),
            label(
                60,
                [80., 740.],
                text("Binary UI + shared CRDT state + loaded WASM behavior"),
                2.,
            ),
        ],
    );
    Ui {
        format: "otrios-canvas-v1".into(),
        ui_ref: "app:component-playground/ui".into(),
        title: "Component Playground".into(),
        state_schema,
        layout,
        selectors: vec![
            Selector {
                local: 1,
                field: 1,
                projection: Projection::Number {
                    scale: 0.488,
                    offset: 32.,
                },
            },
            Selector {
                local: 2,
                field: 1,
                projection: Projection::Percent,
            },
            Selector {
                local: 3,
                field: 2,
                projection: Projection::Number {
                    scale: 0.16,
                    offset: 0.,
                },
            },
            Selector {
                local: 4,
                field: 2,
                projection: Projection::Percent,
            },
            Selector {
                local: 5,
                field: 3,
                projection: Projection::Number {
                    scale: 0.5,
                    offset: 0.,
                },
            },
            Selector {
                local: 6,
                field: 4,
                projection: Projection::Number {
                    scale: 0.21,
                    offset: 0.,
                },
            },
            Selector {
                local: 7,
                field: 5,
                projection: Projection::EnabledText,
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
                kind: GestureKind::Dial,
                fields: vec![2],
            },
            Handler {
                control: 3,
                event: 3,
                kind: GestureKind::Pad,
                fields: vec![3, 4],
            },
            Handler {
                control: 5,
                event: 5,
                kind: GestureKind::Activate,
                fields: vec![5],
            },
        ],
        navigation: vec![home_nav()],
        behavior: Some(BehaviorRef {
            sha256: otrios_core::canvas::hash(code),
            abi: 1,
            writes: vec![1, 2, 3, 4, 5],
        }),
    }
}
