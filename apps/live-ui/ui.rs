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
                .key(&format!("{name}-label"))
                .semantic_label(&format!("Inspect {name}"))
                .on_press(Event::oib_instance(
                    if name == "volume" { 20 } else { 30 },
                    InstanceAction::Inspect,
                )),
            text(field.percent())
                .frame([416., 0., 104., 28.])
                .pin_right(0.)
                .key(&format!("{name}-percentage"))
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
            .resizing(Resizing {
                horizontal: AxisResize::Springs {
                    leading: false,
                    length: true,
                    trailing: false,
                },
                vertical: AxisResize::Fixed,
            })
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
                authored_override: match (owner, offset) {
                    (20, 0) => Some(Value::Text("Volume".into())),
                    (30, 0) => Some(Value::Text("Brightness".into())),
                    _ => None,
                },
                choices: match offset {
                    0 => match owner {
                        20 => ["Volume", "Sound", "Level"],
                        30 => ["Brightness", "Light", "Level"],
                        _ => ["Level", "Volume", "Sound"],
                    }
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
    for (id, name, default) in [
        (104, "label y", Value::Number(10.)),
        (105, "label width", Value::Number(128.)),
        (106, "track x", Value::Number(144.)),
        (107, "track y", Value::Number(0.)),
        (108, "track width", Value::Number(288.)),
        (109, "percentage y", Value::Number(10.)),
        (110, "percentage width", Value::Number(76.)),
        (111, "content y", Value::Number(0.)),
        (112, "row pitch", Value::Number(80.)),
        (113, "heading visible", Value::Boolean(false)),
    ] {
        cells.push(Cell {
            id,
            owner: 10,
            name: name.into(),
            default,
            authored_override: None,
            choices: vec![],
            inherit: None,
            targets: vec![],
        });
    }
    let seed = Seed {
        designer: None,
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
        instance_surface: None,
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
        for (id, key, property, offset) in [
            (104, format!("{name}-label"), Property::Y, 0.),
            (105, format!("{name}-label"), Property::FrameWidth, 0.),
            (106, format!("{name}-track"), Property::X, 0.),
            (107, format!("{name}-track"), Property::OffsetY, -64.),
            (108, format!("{name}-track"), Property::FrameWidth, 0.),
            (109, format!("{name}-percentage"), Property::Y, 0.),
            (110, format!("{name}-percentage"), Property::FrameWidth, 0.),
            (111, name.into(), Property::Y, 0.),
            (112, format!("{name}-preview"), Property::Height, 0.),
            (113, format!("{name}-heading"), Property::Visible, 0.),
        ] {
            targets.push(target(id, &key, property, offset));
        }
    }
    let mut inspector = vec![];
    for (owner, start, y) in [
        ("Component: LabeledSlider", 100, 0.),
        ("Instance: volume", 200, 344.),
        ("Instance: brightness", 300, 752.),
    ] {
        let title = if start == 200 {
            format!("{owner} · Volume binding")
        } else if start == 300 {
            format!("{owner} · Brightness binding")
        } else {
            owner.into()
        };
        let mut rows = vec![text(title.as_str()).frame([8., 0., 540., 32.]).color(BLUE)];
        let mut row_y = 48.;
        if start != 100 {
            let instance = if start == 200 { 20 } else { 30 };
            rows.push(
                button("Duplicate")
                    .frame([8., 40., 160., 56.])
                    .on_press(Event::oib_instance(instance, InstanceAction::Duplicate)),
            );
            rows.push(
                button("Remove")
                    .frame([184., 40., 160., 56.])
                    .on_press(Event::oib_instance(instance, InstanceAction::Remove)),
            );
            row_y = 112.;
        }
        for (index, label) in ["Label", "Accent", "Width", "Spacing"].iter().enumerate() {
            let id = start + index as u64;
            let property = designer::PropertyId(100 + index as u64);
            let address = if start == 100 {
                designer::PropertyAddress::Definition {
                    definition: designer::DefinitionId(10),
                    node: designer::NodeId(1),
                    property,
                }
            } else {
                designer::PropertyAddress::Occurrence {
                    instance: designer::InstanceId(if start == 200 { 20 } else { 30 }),
                    path: designer::PropertyPath {
                        references: vec![],
                        node: designer::NodeId(1),
                        property,
                    },
                }
            };
            rows.push(
                text(ValueExpr::State(
                    StateField::normalized(0, "oib-cell", 0),
                    Projection::OibCell { cell: id },
                ))
                .frame([8., row_y + 4., 168., 48.])
                .font_scale(1.),
            );
            rows.push(
                button("Change")
                    .frame([176., row_y, 112., 56.])
                    .semantic_label(&format!("{owner} {label} change"))
                    .on_press(Event::oib(address.clone(), InspectorAction::Cycle)),
            );
            if start != 100 {
                rows.push(
                    button("Reset")
                        .frame([296., row_y, 112., 56.])
                        .semantic_label(&format!("{owner} {label} reset"))
                        .on_press(Event::oib(address.clone(), InspectorAction::Clear)),
                );
            }
            rows.push(
                button("Default")
                    .frame([416., row_y, 128., 56.])
                    .semantic_label(&format!("{owner} {label} resolve to default"))
                    .on_press(Event::oib(address.clone(), InspectorAction::Resolve)),
            );
            row_y += 68.;
        }
        inspector.push(
            component([560., 408.], rows)
                .frame([0., y, 560., 408.])
                .key(if start == 100 {
                    "component-inspector"
                } else if start == 200 {
                    "volume-inspector"
                } else {
                    "brightness-inspector"
                }),
        );
    }
    let wide = row(
        24.,
        vec![
            place("preview", [584., 500.]).flex_height(),
            place("inspector", [624., 500.]).flex_height(),
        ],
    )
    .flex_height();
    let tall = column(
        24.,
        vec![
            place("preview", [752., 450.]).flex_height(),
            place("inspector", [752., 640.]).flex_height(),
        ],
    )
    .flex_height();
    AppUi::new(
        "live-ui",
        "Live UI",
        schema("live-ui-state-v2", &[VOLUME, BRIGHTNESS]),
    )
    .with_breadcrumbs(&[("OTriOS", "home"), ("Test Playground", "test-playground")])
    .with_composition(
        column(24., vec![heading(), wide]),
        column(24., vec![heading(), tall]),
    )
    .with_oib(seed, "inspector-scroll", targets)
    .with_oib_instances(NamedOibSurface {
        layout: Some(LayoutSurface {
            preview_pitch_cell: 112,
        }),

        preview: "preview-scroll".into(),
        preview_stride: 216.,
        inspector_start: 344.,
        inspector_stride: 408.,
        prototypes: [(20, "volume"), (30, "brightness")]
            .into_iter()
            .map(|(instance, name)| NamedOibPrototype {
                palette_only: false,
                instance,
                preview: format!("{name}-preview"),
                inspector: Some(format!("{name}-inspector")),
            })
            .collect(),
    })
    .build(
        "Live UI",
        vec![
            component(
                [584., 500.],
                vec![
                    surface(CARD, 16.).pin_edges(0.),
                    canvas(vec![
                        component(
                            [560., 216.],
                            vec![
                                button("volume · Volume binding")
                                    .key("volume-heading")
                                    .on_press(Event::oib_instance(20, InstanceAction::Inspect))
                                    .frame([0., 0., 520., 40.]),
                                slider("volume", VOLUME, 48.).x(0.0.into()),
                            ],
                        )
                        .frame([0., 0., 560., 216.])
                        .key("volume-preview"),
                        component(
                            [560., 216.],
                            vec![
                                button("brightness · Brightness binding")
                                    .key("brightness-heading")
                                    .on_press(Event::oib_instance(30, InstanceAction::Inspect))
                                    .frame([0., 0., 520., 40.]),
                                slider("brightness", BRIGHTNESS, 48.).x(0.0.into()),
                            ],
                        )
                        .frame([0., 216., 560., 216.])
                        .key("brightness-preview"),
                    ])
                    .frame([24., 20., 536., 464.])
                    .pin_left(24.)
                    .pin_right(24.)
                    .pin_top(20.)
                    .pin_bottom(16.)
                    .key("preview-scroll"),
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
                    button("Add LabeledSlider")
                        .frame([16., 52., 280., 56.])
                        .on_press(Event::oib_instance(10, InstanceAction::Add)),
                    button("Remove pending")
                        .frame([16., 120., 280., 56.])
                        .on_press(Event::oib_instance(1, InstanceAction::RemovePending)),
                    button("Restore sliders")
                        .frame([312., 52., 280., 56.])
                        .semantic_label("Restore original slider bindings; keep UI design")
                        .on_press(Event::oib_instance(1, InstanceAction::Restore)),
                    button("Reset UI defaults")
                        .frame([312., 120., 280., 56.])
                        .semantic_label("Reset app UI defaults; keep slider values")
                        .on_press(Event::oib_instance(1, InstanceAction::ResetDefaults)),
                    canvas(inspector)
                        .frame([16., 188., 592., 296.])
                        .key("inspector-scroll")
                        .pin_left(16.)
                        .pin_right(16.)
                        .pin_top(188.)
                        .pin_bottom(16.),
                ],
            )
            .frame([632., 120., 624., 500.])
            .key("inspector"),
        ],
    )
}
