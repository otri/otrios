// SPDX-License-Identifier: Apache-2.0
//! Signed D1 sample only. Local tooling and graph resolution belong to core.
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::{
    canvas::Ui,
    layout::{Value, ValueType},
    oib::{self, Cell, Object, Property, Seed, designer::*},
};
const BLUE: [u8; 4] = [85, 136, 255, 255];
const GOLD: [u8; 4] = [218, 165, 32, 255];
const ORANGE: [u8; 4] = [255, 136, 85, 255];
// Stable identities, deliberately independent of paint order and authored names.
const OBJECT: ClassId = ClassId(1);
const DOCUMENT: ClassId = ClassId(2);
const VIEW: ClassId = ClassId(3);
const PANEL: ClassId = ClassId(4);
const SHAPE: ClassId = ClassId(5);
const TEXT: ClassId = ClassId(6);
const BUTTON: ClassId = ClassId(7);
const TOGGLE: ClassId = ClassId(8);
const SLIDER: ClassId = ClassId(9);
fn path(references: &[u64], node: u64, property: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
        property: PropertyId(property),
    }
}
fn address(instance: u64, references: &[u64], node: u64) -> ObjectAddress {
    ObjectAddress {
        instance: InstanceId(instance),
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
    }
}
fn descriptor(
    id: u64,
    name: &str,
    default: Value,
    group: InspectorGroup,
    editor: PropertyEditor,
) -> PropertyDescriptor {
    let value_type = match &default {
        Value::Number(_) => ValueType::Number,
        Value::Color(_) => ValueType::Color,
        Value::Text(_) => ValueType::Text,
        Value::Boolean(_) => ValueType::Boolean,
        _ => unreachable!(),
    };
    let numeric = value_type == ValueType::Number && editor == PropertyEditor::NumberSuggestions;
    PropertyDescriptor {
        id: PropertyId(id),
        name: name.into(),
        label: name.into(),
        value_type,
        default,
        number_range: numeric.then_some([320., 440.]),
        number_step: numeric.then_some(40.),
        units: numeric.then(|| "px".into()),
        group,
        editor,
        policy: PropertyPolicy {
            live_literal: editor != PropertyEditor::ReadOnly,
            app_binding: editor == PropertyEditor::ReadOnly,
        },
    }
}
fn classes() -> Vec<ClassDescriptor> {
    use ClassKind::*;
    use InspectorGroup::*;
    use PropertyEditor::*;
    let accent = || descriptor(1, "Accent", Value::Color(BLUE), Appearance, Palette);
    [
        (
            OBJECT,
            "OIBObject",
            OIBObject,
            None,
            vec![],
            vec![OBJECT],
            vec![],
        ),
        (
            DOCUMENT,
            "OIBDocument",
            OIBDocument,
            Some(OBJECT),
            vec![],
            vec![OBJECT],
            vec![],
        ),
        (
            VIEW,
            "O2DView",
            O2DView,
            Some(OBJECT),
            vec![],
            vec![OBJECT],
            vec![VIEW],
        ),
        (
            PANEL,
            "O2DPanel",
            O2DPanel,
            Some(VIEW),
            vec![accent()],
            vec![OBJECT],
            vec![VIEW],
        ),
        (
            SHAPE,
            "O2DShape",
            O2DShape,
            Some(VIEW),
            vec![],
            vec![],
            vec![],
        ),
        (TEXT, "O2DText", O2DText, Some(VIEW), vec![], vec![], vec![]),
        (
            BUTTON,
            "O2DButton",
            O2DButton,
            Some(VIEW),
            vec![],
            vec![VIEW],
            vec![VIEW],
        ),
        (
            TOGGLE,
            "O2DToggle",
            O2DToggle,
            Some(BUTTON),
            vec![
                accent(),
                descriptor(
                    3,
                    "Label",
                    Value::Text("Enabled".into()),
                    Component,
                    TextSuggestions,
                ),
                descriptor(5, "Checked", Value::Boolean(false), Connections, ReadOnly),
            ],
            vec![VIEW],
            vec![VIEW],
        ),
        (
            SLIDER,
            "O2DSlider",
            O2DSlider,
            Some(VIEW),
            vec![
                accent(),
                descriptor(2, "Width", Value::Number(440.), Layout, NumberSuggestions),
                descriptor(
                    3,
                    "Label",
                    Value::Text("Level".into()),
                    Component,
                    TextSuggestions,
                ),
                descriptor(4, "Value", Value::Number(0.), Connections, ReadOnly),
            ],
            vec![VIEW],
            vec![VIEW],
        ),
    ]
    .into_iter()
    .map(
        |(
            id,
            name,
            implementation,
            base,
            properties,
            owned_child_classes,
            visual_child_classes,
        )| ClassDescriptor {
            id,
            name: name.into(),
            implementation,
            base,
            properties,
            owned_child_classes,
            visual_child_classes,
        },
    )
    .collect()
}
fn node(
    id: u64,
    name: &str,
    class: ClassId,
    owner: Option<u64>,
    children: &[u64],
    defaults: Vec<(u64, Value)>,
) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: name.into(),
        class,
        owner: owner.map(NodeId),
        visual_children: children.iter().copied().map(NodeId).collect(),
        defaults: defaults
            .into_iter()
            .map(|(property, value)| PropertyDefault {
                property: PropertyId(property),
                value,
            })
            .collect(),
        reference: None,
    }
}
fn definitions() -> Vec<ComponentDefinition> {
    let mut slider_ref = node(7, "Slider reference", VIEW, Some(1), &[], vec![]);
    slider_ref.reference = Some(ComponentReference {
        definition: DefinitionId(12),
        assignments: vec![],
    });
    let mut toggle_ref = node(8, "Toggle reference", VIEW, Some(1), &[], vec![]);
    toggle_ref.reference = Some(ComponentReference {
        definition: DefinitionId(13),
        assignments: vec![],
    });
    vec![
        ComponentDefinition {
            id: DefinitionId(10),
            name: "SettingsPanel".into(),
            root: NodeId(1),
            nodes: vec![
                node(
                    1,
                    "Panel",
                    PANEL,
                    None,
                    &[2, 3, 7, 8, 9],
                    vec![(1, Value::Color(BLUE))],
                ),
                node(2, "Background", SHAPE, Some(1), &[], vec![]),
                node(3, "Title", TEXT, Some(1), &[], vec![]),
                slider_ref,
                toggle_ref,
                node(9, "Text button", BUTTON, Some(1), &[10, 11], vec![]),
                node(10, "Button surface", SHAPE, Some(9), &[], vec![]),
                node(11, "Button title", TEXT, Some(9), &[], vec![]),
            ],
            exposed: vec![ExposedProperty {
                property: PropertyId(1),
                targets: vec![path(&[7], 1, 1), path(&[8], 1, 1)],
            }],
            slots: vec![],
        },
        ComponentDefinition {
            id: DefinitionId(11),
            name: "Shell".into(),
            root: NodeId(1),
            nodes: vec![
                node(1, "Runtime canvas", VIEW, None, &[], vec![]),
                node(2, "Configuration", OBJECT, Some(1), &[], vec![]),
            ],
            exposed: vec![],
            slots: vec![],
        },
        ComponentDefinition {
            id: DefinitionId(12),
            name: "LabeledSlider".into(),
            root: NodeId(1),
            nodes: vec![
                node(
                    1,
                    "Slider",
                    SLIDER,
                    None,
                    &[2, 3, 4, 5],
                    vec![
                        (1, Value::Color(BLUE)),
                        (2, Value::Number(440.)),
                        (3, Value::Text("Level".into())),
                    ],
                ),
                node(2, "Label", TEXT, Some(1), &[], vec![]),
                node(3, "Value", TEXT, Some(1), &[], vec![]),
                node(4, "Track", SHAPE, Some(1), &[], vec![]),
                node(5, "Knob", SHAPE, Some(1), &[], vec![]),
            ],
            exposed: vec![],
            slots: vec![],
        },
        ComponentDefinition {
            id: DefinitionId(13),
            name: "LabeledToggle".into(),
            root: NodeId(1),
            nodes: vec![
                node(
                    1,
                    "Toggle",
                    TOGGLE,
                    None,
                    &[2, 3],
                    vec![(1, Value::Color(BLUE)), (3, Value::Text("Enabled".into()))],
                ),
                node(2, "Indicator", SHAPE, Some(1), &[], vec![]),
                node(3, "Label", TEXT, Some(1), &[], vec![]),
            ],
            exposed: vec![],
            slots: vec![],
        },
    ]
}
// Immutable identities are explicit for BOTH factories. Row order and names do
// not allocate shared IDs, occurrence aliases, keys or reference boundaries.
struct Editable {
    path: PropertyPath,
    key: u8,
    shared: u64,
    definition: u64,
    audio: u64,
    display: u64,
    name: &'static str,
    default: Value,
    choices: Vec<Value>,
}
impl Editable {
    fn alias(&self, prototype: u64) -> u64 {
        match prototype {
            20 => self.audio,
            30 => self.display,
            _ => panic!("signed panel factory"),
        }
    }
}
fn rows() -> Vec<Editable> {
    let colors = || [BLUE, GOLD, ORANGE].into_iter().map(Value::Color).collect();
    vec![
        Editable {
            path: path(&[], 1, 1),
            key: 2,
            shared: 100,
            definition: 10,
            audio: 200,
            display: 300,
            name: "panel accent",
            default: Value::Color(BLUE),
            choices: colors(),
        },
        Editable {
            path: path(&[7], 1, 1),
            key: 9,
            shared: 101,
            definition: 12,
            audio: 201,
            display: 301,
            name: "slider accent",
            default: Value::Color(BLUE),
            choices: colors(),
        },
        Editable {
            path: path(&[7], 1, 2),
            key: 17,
            shared: 102,
            definition: 12,
            audio: 202,
            display: 302,
            name: "slider width",
            default: Value::Number(440.),
            choices: [440., 400., 360., 320.]
                .into_iter()
                .map(Value::Number)
                .collect(),
        },
        Editable {
            path: path(&[7], 1, 3),
            key: 28,
            shared: 103,
            definition: 12,
            audio: 203,
            display: 303,
            name: "slider label",
            default: Value::Text("Level".into()),
            choices: ["Level", "Volume", "Brightness", "Sound", "Light"]
                .into_iter()
                .map(|s| Value::Text(s.into()))
                .collect(),
        },
        Editable {
            path: path(&[8], 1, 1),
            key: 41,
            shared: 104,
            definition: 13,
            audio: 204,
            display: 304,
            name: "toggle accent",
            default: Value::Color(BLUE),
            choices: colors(),
        },
        Editable {
            path: path(&[8], 1, 3),
            key: 55,
            shared: 105,
            definition: 13,
            audio: 205,
            display: 305,
            name: "toggle label",
            default: Value::Text("Enabled".into()),
            choices: ["Enabled", "Active", "Power"]
                .into_iter()
                .map(|s| Value::Text(s.into()))
                .collect(),
        },
    ]
}
fn document() -> Document {
    let rows = rows();
    let mut definitions: Vec<_> = rows
        .iter()
        .map(|row| DefinitionCellMapping {
            definition: DefinitionId(row.definition),
            node: row.path.node,
            property: row.path.property,
            cell: row.shared,
        })
        .collect();
    definitions.sort_by_key(|m| (m.definition, m.node, m.property));
    let mut instances: Vec<_> = [(20, "Audio", "Volume"), (30, "Display", "Brightness")]
        .into_iter()
        .map(|(id, name, label)| ComponentInstance {
            id: InstanceId(id),
            name: name.into(),
            definition: DefinitionId(10),
            owner: Owner::Document,
            visual_parent: Some(address(40, &[], 1)),
            order: if id == 20 { 0 } else { 1 },
            assignments: vec![Assignment {
                target: path(&[7], 1, 3),
                source: AssignmentSource::Literal(Value::Text(label.into())),
            }],
        })
        .collect();
    instances.push(ComponentInstance {
        id: InstanceId(40),
        name: "Shell".into(),
        definition: DefinitionId(11),
        owner: Owner::Document,
        visual_parent: None,
        order: 0,
        assignments: vec![],
    });
    Document {
        id: DocumentId(1),
        name: "Settings designer".into(),
        class: DOCUMENT,
        presented_root: InstanceId(40),
        hierarchy_viewport: None,
        classes: classes(),
        definitions: self::definitions(),
        instances,
        mappings: Mappings {
            objects: vec![],
            definitions,
            roots: [20, 30]
                .into_iter()
                .map(|id| RootCellMapping {
                    prototype: InstanceId(id),
                    cells: rows
                        .iter()
                        .map(|row| RootCellKey {
                            path: row.path.clone(),
                            key: row.key,
                            initial_cell: row.alias(id),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}
fn seed() -> Seed {
    let rows = rows();
    let mut cells = vec![];
    for prototype in [None, Some(20), Some(30)] {
        for row in &rows {
            cells.push(Cell {
                id: prototype.map_or(row.shared, |id| row.alias(id)),
                owner: prototype.unwrap_or(row.definition),
                name: row.name.into(),
                default: row.default.clone(),
                authored_override: match (prototype, row.shared) {
                    (Some(20), 103) => Some(Value::Text("Volume".into())),
                    (Some(30), 103) => Some(Value::Text("Brightness".into())),
                    _ => None,
                },
                choices: row.choices.clone(),
                inherit: prototype.map(|_| row.shared),
                targets: vec![],
            });
        }
    }
    Seed {
        format: oib::FORMAT.into(),
        designer: Some(document()),
        main_oib: 1,
        main_class: "otrios.declarative-v1".into(),
        components: [
            (10, "SettingsPanel"),
            (11, "Shell"),
            (12, "LabeledSlider"),
            (13, "LabeledToggle"),
        ]
        .into_iter()
        .map(|(id, name)| Object {
            id,
            name: name.into(),
            component: None,
        })
        .collect(),
        instances: [(20, "Audio"), (30, "Display")]
            .into_iter()
            .map(|(id, name)| Object {
                id,
                name: name.into(),
                component: Some(10),
            })
            .collect(),
        cells,
        inspector_viewport: 0,
        inspector_content: [360., 1000.],
        instance_surface: None,
    }
}
fn panel(name: &str, value: StateField, enabled: StateField, y: f32) -> Element {
    // Every mapped object has one real paint/layout representation. Wrapper
    // canvases are distinct from their inherited component roots.
    let slider = component(
        [440., 112.],
        vec![
            text(if name == "Audio" {
                "Volume"
            } else {
                "Brightness"
            })
            .frame([0., 0., 320., 28.])
            .key(&format!("{name}-slider-label")),
            text(value.percent())
                .frame([344., 0., 96., 28.])
                .pin_right(0.)
                .key(&format!("{name}-slider-value")),
            rounded_rect(TRACK)
                .corner_radius(6.0.into())
                .frame([0., 66., 440., 12.])
                .pin_left(0.)
                .pin_right(0.)
                .key(&format!("{name}-track")),
            circle(BLUE)
                .frame([0., 48., 48., 48.])
                .x(value.mapped(0., 392.))
                .resizing(Resizing {
                    horizontal: AxisResize::Springs {
                        leading: true,
                        length: false,
                        trailing: true,
                    },
                    vertical: AxisResize::Fixed,
                })
                .key(&format!("{name}-knob")),
        ],
    )
    .key(&format!("{name}-slider"))
    .on_drag(
        if name == "Audio" {
            "Volume"
        } else {
            "Brightness"
        },
        Event::slider(value),
    );
    let toggle = component(
        [264., 64.],
        vec![
            circle(BLUE)
                .frame([4., 12., 32., 32.])
                .visible(enabled.nonzero())
                .key(&format!("{name}-indicator")),
            text("Enabled")
                .frame([48., 12., 216., 32.])
                .key(&format!("{name}-toggle-label")),
        ],
    )
    .key(&format!("{name}-toggle"))
    .semantic_label(if name == "Audio" {
        "Audio enabled"
    } else {
        "Display enabled"
    })
    .semantic_checked_value(enabled.nonzero())
    .on_press(Event::toggle(enabled));
    let button = component(
        [184., 48.],
        vec![
            rounded_rect(BLUE)
                .corner_radius(10.0.into())
                .pin_edges(0.)
                .key(&format!("{name}-button-surface")),
            text("Toggle power")
                .font_scale(2.)
                .frame([16., 10., 152., 28.])
                .key(&format!("{name}-button-title")),
        ],
    )
    .frame([312., 208., 184., 48.])
    .key(&format!("{name}-button"))
    .semantic_label(if name == "Audio" {
        "Toggle audio power"
    } else {
        "Toggle display power"
    })
    .on_press(Event::toggle(enabled));
    component(
        [520., 280.],
        vec![
            rounded_rect(CARD)
                .corner_radius(12.0.into())
                .pin_edges(0.)
                .key(&format!("{name}-background")),
            text(name)
                .frame([24., 16., 472., 32.])
                .key(&format!("{name}-title")),
            component([472., 112.], vec![slider])
                .frame([24., 64., 472., 112.])
                .key(&format!("{name}-slider-reference")),
            component([264., 64.], vec![toggle])
                .frame([24., 200., 264., 64.])
                .key(&format!("{name}-toggle-reference")),
            button,
        ],
    )
    .frame([0., y, 520., 280.])
    .key(&format!("{name}-panel"))
}
fn tooling_panel(key: &str, title: &str, size: [f32; 2], children: Vec<Element>) -> Element {
    component(
        size,
        vec![
            surface(CARD, 12.).pin_edges(0.),
            text(title)
                .frame([16., 12., size[0] - 32., 28.])
                .color(MUTED),
            canvas(children)
                .frame([16., 52., size[0] - 32., size[1] - 68.])
                .pin_left(16.)
                .pin_right(16.)
                .pin_top(52.)
                .pin_bottom(16.)
                .key(&format!("{key}-scroll")),
        ],
    )
    .key(key)
}
pub fn description() -> Ui {
    let mut targets = vec![];
    let mut objects = vec![
        NamedOibObject {
            address: address(40, &[], 1),
            node: Some("preview-scroll".into()),
        },
        NamedOibObject {
            address: address(40, &[], 2),
            node: None,
        },
    ];
    let mut bindings = vec![];
    for (id, name, start, value, enabled) in [
        (20, "Audio", 200, VOLUME, AUDIO_ENABLED),
        (30, "Display", 300, BRIGHTNESS, DISPLAY_ENABLED),
    ] {
        for (cell, suffix, property) in [
            (start, "button-surface", Property::Color),
            (start + 1, "knob", Property::Color),
            (start + 2, "slider", Property::Width),
            (start + 3, "slider-label", Property::Text),
            (start + 3, "slider", Property::SemanticLabel),
            (start + 4, "indicator", Property::Color),
            (start + 5, "toggle-label", Property::Text),
        ] {
            targets.push(NamedOibTarget {
                cell,
                node: format!("{name}-{suffix}"),
                property,
                scale: 1.,
                offset: 0.,
            });
        }
        for (refs, node, suffix) in [
            (&[][..], 1, "panel"),
            (&[], 2, "background"),
            (&[], 3, "title"),
            (&[], 7, "slider-reference"),
            (&[], 8, "toggle-reference"),
            (&[], 9, "button"),
            (&[], 10, "button-surface"),
            (&[], 11, "button-title"),
            (&[7][..], 1, "slider"),
            (&[7], 2, "slider-label"),
            (&[7], 3, "slider-value"),
            (&[7], 4, "track"),
            (&[7], 5, "knob"),
            (&[8][..], 1, "toggle"),
            (&[8], 2, "indicator"),
            (&[8], 3, "toggle-label"),
        ] {
            objects.push(NamedOibObject {
                address: address(id, refs, node),
                node: Some(format!("{name}-{suffix}")),
            });
        }
        bindings.push(NamedOibAppBinding {
            instance: InstanceId(id),
            target: path(&[7], 1, 4),
            value: value.value(),
        });
        bindings.push(NamedOibAppBinding {
            instance: InstanceId(id),
            target: path(&[8], 1, 5),
            value: enabled.nonzero(),
        });
    }
    let wide = row(
        24.,
        vec![
            place("hierarchy", [240., 656.]).flex_height(),
            place("preview", [568., 656.]).flex_height(),
            place("inspector", [376., 656.]).flex_height(),
        ],
    )
    .flex_height();
    let tall = column(
        16.,
        vec![
            row(
                16.,
                vec![
                    place("hierarchy", [360., 240.])
                        .minimum([280., 200.])
                        .flex_width()
                        .flex_height(),
                    place("inspector", [376., 240.])
                        .minimum([280., 200.])
                        .flex_width()
                        .flex_height(),
                ],
            ),
            place("preview", [752., 520.])
                .minimum([568., 280.])
                .flex_width()
                .flex_height(),
        ],
    )
    .flex_height();
    AppUi::new("oib-designer", "OIB Designer", super::state::description())
        .with_breadcrumbs(&[("OTriOS", "home"), ("Test Playground", "test-playground")])
        .with_composition(
            column(24., vec![heading(), wide]).flex_height(),
            column(24., vec![heading(), tall]).flex_height(),
        )
        .with_oib(seed(), "inspector-scroll", targets)
        .with_oib_designer_objects(objects)
        .with_oib_designer_hierarchy("hierarchy-scroll")
        .with_oib_designer_bindings(bindings)
        .with_oib_instances(NamedOibSurface {
            layout: None,
            previous_ui_sha256: vec![0; 32],
            preview: "preview-scroll".into(),
            preview_stride: 280.,
            inspector_start: 1.,
            inspector_stride: 1.,
            prototypes: [(20, "Audio"), (30, "Display")]
                .into_iter()
                .map(|(instance, name)| NamedOibPrototype {
                    instance,
                    preview: format!("{name}-panel"),
                    inspector: format!("{name}-inspector-placeholder"),
                })
                .collect(),
        })
        .build(
            "OIB Designer",
            vec![
                tooling_panel("hierarchy", "Hierarchy", [240., 656.], vec![])
                    .frame([24., 120., 240., 656.]),
                tooling_panel(
                    "preview",
                    "Runtime canvas",
                    [568., 656.],
                    vec![
                        panel("Audio", VOLUME, AUDIO_ENABLED, 0.),
                        panel("Display", BRIGHTNESS, DISPLAY_ENABLED, 280.),
                    ],
                )
                .frame([288., 120., 568., 656.]),
                tooling_panel(
                    "inspector",
                    "Selected properties",
                    [376., 656.],
                    vec![
                        canvas(vec![])
                            .frame([0., 0., 1., 1.])
                            .key("Audio-inspector-placeholder"),
                        canvas(vec![])
                            .frame([0., 1., 1., 1.])
                            .key("Display-inspector-placeholder"),
                    ],
                )
                .frame([880., 120., 376., 656.]),
            ],
        )
}
