// SPDX-License-Identifier: Apache-2.0
//! Reusable canonical settings components and binding presets.
use crate::authoring::*;
use otrios_interfaces::{
    layout::{Value, ValueType},
    oib::{designer::*, graph as g},
};
const BLUE: [u8; 4] = [85, 136, 255, 255];
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
        visual: None,
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
            properties: vec![],
            next_property: 0,
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
            properties: vec![],
            next_property: 0,
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
            properties: vec![],
            next_property: 0,
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
            properties: vec![],
            next_property: 0,
            exposed: vec![],
            slots: vec![],
        },
    ]
}

fn expr(n: f32) -> otrios_interfaces::layout::Expr {
    otrios_interfaces::layout::Expr::Literal(Value::Number(n))
}
fn parameter(n: u16) -> otrios_interfaces::layout::Expr {
    otrios_interfaces::layout::Expr::Parameter(n)
}
fn source(
    local: u16,
    node: u64,
    property: u64,
    projection: otrios_interfaces::canvas::Projection,
) -> g::Parameter {
    g::Parameter {
        local,
        source: g::ParameterSource::Property(path(&[], node, property)),
        projection,
    }
}
fn visual(
    frame: [f32; 4],
    shape: otrios_interfaces::layout::Shape,
    parameters: Vec<g::Parameter>,
) -> g::Visual {
    g::Visual {
        layout: None,
        frame: frame.map(expr),
        resize: None,
        visible: otrios_interfaces::layout::Expr::Literal(Value::Boolean(true)),
        shape,
        semantics: None,
        action: None,
        parameters,
    }
}
fn container(frame: [f32; 4]) -> g::Visual {
    visual(
        frame,
        otrios_interfaces::layout::Shape::Canvas { clip: true },
        vec![],
    )
}
fn paint(frame: [f32; 4], color: [u8; 4], radius: f32) -> g::Visual {
    visual(
        frame,
        otrios_interfaces::layout::Shape::Superellipse {
            radius: expr(radius),
            exponent: 4.,
            color: otrios_interfaces::layout::Expr::Literal(Value::Color(color)),
        },
        vec![],
    )
}
fn label(frame: [f32; 4], value: &str) -> g::Visual {
    visual(
        frame,
        otrios_interfaces::layout::Shape::Text {
            value: otrios_interfaces::layout::Expr::Literal(Value::Text(value.into())),
            scale: expr(2.),
            color: otrios_interfaces::layout::Expr::Literal(Value::Color(WHITE)),
        },
        vec![],
    )
}
fn pin(
    visual: &mut g::Visual,
    reference_size: [f32; 2],
    horizontal: otrios_interfaces::layout::AxisResize,
    vertical: otrios_interfaces::layout::AxisResize,
) {
    visual.resize = Some(otrios_interfaces::layout::ResizeRule {
        reference_size,
        horizontal,
        vertical,
    });
}
pub fn graph(sliders_only: bool) -> g::Document {
    use otrios_interfaces::{
        canvas::{GestureKind, Projection as P},
        layout::{AxisResize as A, Expr as E, Semantics, Shape as S},
    };
    let mut defs = definitions();
    let panel = defs.iter_mut().find(|d| d.id == DefinitionId(10)).unwrap();
    panel.nodes[0].visual_children.push(NodeId(12));
    panel
        .nodes
        .push(node(12, "Accessories", VIEW, Some(1), &[], vec![]));
    panel.slots.push(SlotDescriptor {
        id: SlotId(1),
        name: "Accessories".into(),
        references: vec![],
        node: NodeId(12),
        admitted_classes: vec![TOGGLE],
        cardinality: [0, 2],
        layout: SlotLayout::Absolute,
    });
    for def in &mut defs {
        for n in &mut def.nodes {
            let v = match (def.id.0, n.id.0) {
                (10, 1) => container([0., 0., 520., 360.]),
                (10, 2) => {
                    let mut v = paint([0., 0., 520., 360.], CARD, 12.);
                    pin(
                        &mut v,
                        [520., 360.],
                        A::Pins {
                            leading: Some(0.),
                            trailing: Some(0.),
                        },
                        A::Pins {
                            leading: Some(0.),
                            trailing: Some(0.),
                        },
                    );
                    v
                }
                (10, 3) => {
                    let mut v = label([24., 16., 472., 32.], "Settings");
                    if let S::Text { value, .. } = &mut v.shape {
                        *value = parameter(0);
                    }
                    v.parameters.push(g::Parameter {
                        local: 0,
                        source: g::ParameterSource::InstanceName {
                            prefix: String::new(),
                            suffix: String::new(),
                            lowercase: false,
                        },
                        projection: P::Identity,
                    });
                    v
                }
                (10, 7) => container([24., 64., 472., 112.]),
                (10, 8) => container([24., 200., 264., 64.]),
                (10, 9) => {
                    let mut v = container([312., 208., 184., 48.]);
                    v.parameters.push(g::Parameter {
                        local: 0,
                        source: g::ParameterSource::InstanceName {
                            prefix: "Toggle ".into(),
                            suffix: " power".into(),
                            lowercase: true,
                        },
                        projection: P::Identity,
                    });
                    v.semantics = Some(Semantics {
                        label: parameter(0),
                        checked: E::Literal(Value::Boolean(false)),
                    });
                    v.action = Some(g::Action {
                        kind: GestureKind::Activate,
                        operation: g::ActionOperation::Toggle,
                        source: path(&[8], 1, 5),
                    });
                    v
                }
                (10, 10) => {
                    let mut v = paint([0., 0., 184., 48.], BLUE, 10.);
                    if let S::Superellipse { color, .. } = &mut v.shape {
                        *color = parameter(0);
                    }
                    v.parameters.push(source(0, 1, 1, P::Identity));
                    pin(
                        &mut v,
                        [184., 48.],
                        A::Pins {
                            leading: Some(0.),
                            trailing: Some(0.),
                        },
                        A::Pins {
                            leading: Some(0.),
                            trailing: Some(0.),
                        },
                    );
                    v
                }
                (10, 11) => label([16., 10., 152., 28.], "Toggle power"),
                (10, 12) => container([24., 280., 472., 64.]),
                (12, 1) => {
                    let mut v = container([0., 0., 440., 112.]);
                    v.frame[2] = parameter(0);
                    v.parameters = vec![source(0, 1, 2, P::Identity), source(1, 1, 3, P::Identity)];
                    v.semantics = Some(Semantics {
                        label: parameter(1),
                        checked: E::Literal(Value::Boolean(false)),
                    });
                    v.action = Some(g::Action {
                        kind: GestureKind::Slider,
                        operation: g::ActionOperation::Set,
                        source: path(&[], 1, 4),
                    });
                    v
                }
                (12, 2) => {
                    let mut v = label([0., 0., 320., 28.], "Level");
                    if let S::Text { value, .. } = &mut v.shape {
                        *value = parameter(0);
                    }
                    v.parameters = vec![source(0, 1, 3, P::Identity)];
                    v
                }
                (12, 3) => {
                    let mut v = label([344., 0., 96., 28.], "0%");
                    if let S::Text { value, .. } = &mut v.shape {
                        *value = parameter(0);
                    }
                    v.parameters = vec![source(0, 1, 4, P::Percent)];
                    pin(
                        &mut v,
                        [440., 112.],
                        A::Pins {
                            leading: None,
                            trailing: Some(0.),
                        },
                        A::Fixed,
                    );
                    v
                }
                (12, 4) => {
                    let mut v = paint([0., 66., 440., 12.], TRACK, 6.);
                    pin(
                        &mut v,
                        [440., 112.],
                        A::Pins {
                            leading: Some(0.),
                            trailing: Some(0.),
                        },
                        A::Fixed,
                    );
                    v
                }
                (12, 5) => {
                    let mut v = visual(
                        [0., 48., 48., 48.],
                        S::Circle {
                            color: parameter(0),
                        },
                        vec![
                            source(0, 1, 1, P::Identity),
                            source(
                                1,
                                1,
                                4,
                                P::Number {
                                    scale: 0.392,
                                    offset: 0.,
                                },
                            ),
                        ],
                    );
                    v.frame[0] = parameter(1);
                    pin(
                        &mut v,
                        [440., 112.],
                        A::Springs {
                            leading: true,
                            length: false,
                            trailing: true,
                        },
                        A::Fixed,
                    );
                    v
                }
                (13, 1) => {
                    let mut v = container([0., 0., 264., 64.]);
                    v.parameters = vec![
                        source(0, 1, 5, P::Identity),
                        g::Parameter {
                            local: 1,
                            source: g::ParameterSource::InstanceName {
                                prefix: String::new(),
                                suffix: " enabled".into(),
                                lowercase: false,
                            },
                            projection: P::Identity,
                        },
                    ];
                    v.semantics = Some(Semantics {
                        label: parameter(1),
                        checked: parameter(0),
                    });
                    v.action = Some(g::Action {
                        kind: GestureKind::Activate,
                        operation: g::ActionOperation::Toggle,
                        source: path(&[], 1, 5),
                    });
                    v
                }
                (13, 2) => {
                    let mut v = visual(
                        [4., 12., 32., 32.],
                        S::Circle {
                            color: parameter(0),
                        },
                        vec![source(0, 1, 1, P::Identity), source(1, 1, 5, P::Identity)],
                    );
                    v.visible = parameter(1);
                    v
                }
                (13, 3) => {
                    let mut v = label([48., 12., 216., 32.], "Enabled");
                    if let S::Text { value, .. } = &mut v.shape {
                        *value = parameter(0);
                    }
                    v.parameters = vec![source(0, 1, 3, P::Identity)];
                    pin(
                        &mut v,
                        [264., 64.],
                        A::Pins {
                            leading: Some(48.),
                            trailing: Some(0.),
                        },
                        A::Fixed,
                    );
                    v
                }
                _ => continue,
            };
            n.visual = Some(v);
        }
    }
    let mut presets = vec![g::Preset {
        id: 40,
        name: "Shell".into(),
        definition: DefinitionId(11),
        assignments: vec![],
        frame: None,
        palette: false,
    }];
    for (id, name, label, field, enabled) in [
        (20, "Audio", "Volume", 1, 3),
        (30, "Display", "Brightness", 2, 4),
    ] {
        let assignments = if sliders_only {
            vec![
                g::Assignment {
                    target: path(&[], 1, 3),
                    source: g::Source::Literal(Value::Text(label.into())),
                },
                g::Assignment {
                    target: path(&[], 1, 4),
                    source: g::Source::AppBinding {
                        field,
                        projection: P::Identity,
                    },
                },
            ]
        } else {
            vec![
                g::Assignment {
                    target: path(&[7], 1, 3),
                    source: g::Source::Literal(Value::Text(label.into())),
                },
                g::Assignment {
                    target: path(&[7], 1, 4),
                    source: g::Source::AppBinding {
                        field,
                        projection: P::Identity,
                    },
                },
                g::Assignment {
                    target: path(&[8], 1, 5),
                    source: g::Source::AppBinding {
                        field: enabled,
                        projection: P::Nonzero,
                    },
                },
            ]
        };
        presets.push(g::Preset {
            id,
            name: if sliders_only { label } else { name }.into(),
            definition: DefinitionId(if sliders_only { 12 } else { 10 }),
            assignments,
            frame: None,
            palette: true,
        });
    }
    if !sliders_only {
        presets.push(g::Preset {
            id: 50,
            name: "Power".into(),
            definition: DefinitionId(13),
            assignments: vec![
                g::Assignment {
                    target: path(&[], 1, 3),
                    source: g::Source::Literal(Value::Text("Power".into())),
                },
                g::Assignment {
                    target: path(&[], 1, 5),
                    source: g::Source::AppBinding {
                        field: 3,
                        projection: P::Nonzero,
                    },
                },
            ],
            frame: None,
            palette: true,
        });
    }
    presets.sort_by_key(|p| p.id);
    let instances = vec![
        g::Instance {
            id: InstanceId(20),
            name: if sliders_only { "volume" } else { "Audio" }.into(),
            preset: 20,
            owner: Owner::Document,
            visual_parent: Some(address(40, &[], 1)),
            order: 0,
        },
        g::Instance {
            id: InstanceId(30),
            name: if sliders_only {
                "brightness"
            } else {
                "Display"
            }
            .into(),
            preset: 30,
            owner: Owner::Document,
            visual_parent: Some(address(40, &[], 1)),
            order: 1,
        },
        g::Instance {
            id: InstanceId(40),
            name: "Shell".into(),
            preset: 40,
            owner: Owner::Document,
            visual_parent: None,
            order: 0,
        },
    ];
    let mut grants = vec![
        g::ActionGrant {
            field: 1,
            kind: GestureKind::Slider,
            operation: g::ActionOperation::Set,
        },
        g::ActionGrant {
            field: 2,
            kind: GestureKind::Slider,
            operation: g::ActionOperation::Set,
        },
    ];
    if !sliders_only {
        for field in [3, 4] {
            grants.push(g::ActionGrant {
                field,
                kind: GestureKind::Activate,
                operation: g::ActionOperation::Toggle,
            });
        }
    }
    if sliders_only {
        defs.retain(|d| matches!(d.id.0, 11 | 12));
    }
    g::Document {
        id: DocumentId(1),
        name: if sliders_only {
            "Live UI"
        } else {
            "Settings designer"
        }
        .into(),
        class: DOCUMENT,
        presented_root: InstanceId(40),
        classes: classes(),
        definitions: defs,
        imports: None,
        presets,
        instances,
        grants,
        preview_viewport: 0,
        inspector_viewport: 0,
        hierarchy_viewport: None,
        preview_stride: if sliders_only { 184. } else { 360. },
        inspector_content: [360., 1000.],
    }
}
