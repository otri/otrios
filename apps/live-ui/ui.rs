// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::{
    canvas::{Projection, Ui},
    layout::Value,
    oib::*,
};
const VOLUME: StateField = StateField::normalized(1, "volume", 400);
const BRIGHTNESS: StateField = StateField::normalized(2, "brightness", 700);
const BLUE: [u8; 4] = [85, 136, 255, 255];

fn slider(name: &str, field: StateField, y: f32) -> Element {
    component(
        [520., 148.],
        vec![
            text("Level")
                .frame([0., 0., 360., 28.])
                .key(&format!("{name}-label")),
            text(field.percent())
                .frame([416., 0., 104., 28.])
                .pin_right(0.)
                .color(BLUE),
            component(
                [520., 48.],
                vec![
                    rounded_rect(TRACK)
                        .corner_radius(6.0.into())
                        .frame([0., 18., 520., 12.])
                        .pin_left(0.)
                        .pin_right(0.),
                    circle(BLUE)
                        .frame([0., 0., 48., 48.])
                        .x(field.mapped(0., 472.))
                        .key(&format!("{name}-knob"))
                        .resizing(Resizing {
                            horizontal: AxisResize::Springs {
                                leading: true,
                                length: false,
                                trailing: true,
                            },
                            vertical: AxisResize::Fixed,
                        }),
                ],
            )
            .frame([0., 64., 520., 48.])
            .pin_left(0.)
            .pin_right(0.)
            .key(&format!("{name}-track"))
            .on_drag(name, Event::slider(field)),
        ],
    )
    .frame([24., y, 520., 148.])
    .key(name)
}
fn target(cell: u64, node: &str, property: Property, offset: f32) -> NamedOibTarget {
    NamedOibTarget {
        cell,
        node: node.into(),
        property,
        scale: 1.,
        offset,
    }
}
pub fn description() -> Ui {
    let mut cells = Vec::new();
    for (owner, start) in [(10, 100), (20, 200), (30, 300)] {
        for (offset, name, default) in [
            (0, "label", Value::Text("Level".into())),
            (1, "accent", Value::Color(BLUE)),
            (2, "width", Value::Number(520.)),
            (3, "spacing", Value::Number(24.)),
        ] {
            cells.push(Cell {
                id: start + offset,
                owner,
                name: name.into(),
                default,
                choices: match offset {
                    0 => ["Level", "Volume", "Sound"]
                        .iter()
                        .map(|v| Value::Text((*v).into()))
                        .collect(),
                    1 => [BLUE, [218, 165, 32, 255], [255, 136, 85, 255]]
                        .into_iter()
                        .map(Value::Color)
                        .collect(),
                    2 => [520., 400.].into_iter().map(Value::Number).collect(),
                    _ => [24., 56.].into_iter().map(Value::Number).collect(),
                },
                inherit: (owner != 10).then_some(100 + offset),
                targets: vec![],
            });
        }
    }
    let seed = Seed {
        format: FORMAT.into(),
        main_oib: 1,
        main_class: "otrios.declarative-v1".into(),
        components: vec![Object {
            id: 10,
            name: "LabeledSlider".into(),
            component: None,
        }],
        instances: vec![
            Object {
                id: 20,
                name: "volume".into(),
                component: Some(10),
            },
            Object {
                id: 30,
                name: "brightness".into(),
                component: Some(10),
            },
        ],
        cells,
        inspector_viewport: 0,
        inspector_content: [560., 1120.],
    };
    let mut targets = Vec::new();
    for (name, start) in [("volume", 200), ("brightness", 300)] {
        targets.extend([
            target(start, &format!("{name}-label"), Property::Text, 0.),
            target(start, &format!("{name}-track"), Property::SemanticLabel, 0.),
            target(start + 1, &format!("{name}-knob"), Property::Color, 0.),
            target(start + 2, name, Property::Width, 0.),
            target(start + 3, &format!("{name}-track"), Property::Y, 40.),
        ]);
    }
    let mut inspector = vec![];
    let mut y = 0.;
    for (owner, start) in [
        ("Component: LabeledSlider", 100),
        ("Instance: volume", 200),
        ("Instance: brightness", 300),
    ] {
        inspector.push(text(owner).frame([8., y, 540., 32.]).color(BLUE));
        y += 48.;
        for (index, label) in ["Label", "Accent", "Width", "Spacing"].iter().enumerate() {
            let id = start + index as u64;
            inspector.push(
                text(ValueExpr::State(
                    StateField::normalized(0, "oib-cell", 0),
                    Projection::OibCell { cell: id },
                ))
                .frame([8., y + 4., 168., 48.])
                .font_scale(1.),
            );
            inspector.push(
                button("Change")
                    .frame([176., y, 112., 56.])
                    .semantic_label(&format!("{owner} {label} change"))
                    .on_press(Event::oib(id, InspectorAction::Cycle)),
            );
            if start != 100 {
                inspector.push(
                    button("Reset")
                        .frame([296., y, 112., 56.])
                        .semantic_label(&format!("{owner} {label} reset"))
                        .on_press(Event::oib(id, InspectorAction::Clear)),
                );
            }
            inspector.push(
                button("Default")
                    .frame([416., y, 128., 56.])
                    .semantic_label(&format!("{owner} {label} resolve to default"))
                    .on_press(Event::oib(id, InspectorAction::Resolve)),
            );
            y += 68.;
        }
        y += 24.;
    }
    let wide = row(
        24.,
        vec![
            place("preview", [584., 500.]),
            place("inspector", [624., 500.]),
        ],
    );
    let tall = column(
        24.,
        vec![
            place("preview", [752., 450.]),
            place("inspector", [752., 640.]),
        ],
    );
    AppUi::new(
        "live-ui",
        "Live UI",
        schema("live-ui-state-v1", &[VOLUME, BRIGHTNESS]),
    )
    .with_breadcrumbs(&[("OTriOS", "home"), ("Test Playground", "test-playground")])
    .with_composition(
        column(24., vec![heading(), wide]),
        column(24., vec![heading(), tall]),
    )
    .with_oib(seed, "inspector-scroll", targets)
    .build(
        "Live UI",
        vec![
            component(
                [584., 500.],
                vec![
                    surface(CARD, 16.).pin_edges(0.),
                    text("volume · LabeledSlider")
                        .frame([24., 20., 536., 28.])
                        .color(MUTED),
                    slider("volume", VOLUME, 68.),
                    text("brightness · LabeledSlider")
                        .frame([24., 236., 536., 28.])
                        .color(MUTED),
                    slider("brightness", BRIGHTNESS, 284.),
                ],
            )
            .frame([24., 120., 584., 500.])
            .key("preview"),
            component(
                [624., 500.],
                vec![
                    surface(CARD, 16.).pin_edges(0.),
                    text(ValueExpr::State(
                        StateField::normalized(0, "oib-status", 0),
                        Projection::OibStatus,
                    ))
                    .frame([16., 12., 592., 32.])
                    .color(MUTED)
                    .pin_right(16.),
                    canvas(inspector)
                        .frame([16., 56., 592., 428.])
                        .key("inspector-scroll")
                        .pin_left(16.)
                        .pin_right(16.)
                        .pin_top(56.)
                        .pin_bottom(16.),
                ],
            )
            .frame([632., 120., 624., 500.])
            .key("inspector"),
        ],
    )
}
