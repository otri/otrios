// SPDX-License-Identifier: Apache-2.0
//! Authoring fixture for the committed binary seed. Runtime reads default.flex, not this code.
use otrios_interfaces::layout::*;
fn n(v: f32) -> Expr {
    Expr::Literal(Value::Number(v))
}
fn s(v: &str) -> Expr {
    Expr::Literal(Value::Text(v.into()))
}
fn c(v: [u8; 4]) -> Expr {
    Expr::Literal(Value::Color(v))
}
fn p(v: u16) -> Expr {
    Expr::Parameter(v)
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::Subtract(Box::new(a), Box::new(b))
}
fn add(a: Expr, b: Expr) -> Expr {
    Expr::Add(Box::new(a), Box::new(b))
}
fn mul(a: Expr, b: Expr) -> Expr {
    Expr::Multiply(Box::new(a), Box::new(b))
}
fn choose(test: Expr, yes: Expr, no: Expr) -> Expr {
    Expr::Select {
        condition: Box::new(test),
        yes: Box::new(yes),
        no: Box::new(no),
    }
}
fn node(id: u16, frame: [Expr; 4], shape: Shape, children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode {
        resize: None,
        id,
        frame,
        visible: Expr::Literal(Value::Boolean(true)),
        shape,
        responder: None,
        semantics: None,
        children,
    }
}
fn full() -> [Expr; 4] {
    [n(0.), n(0.), Expr::Width, Expr::Height]
}
fn slot() -> TemplateNode {
    node(900, full(), Shape::Slot, vec![])
}
fn paint(id: u16, color: Expr, radius: Expr) -> TemplateNode {
    node(
        id,
        full(),
        Shape::Superellipse {
            radius,
            exponent: 4.,
            color,
        },
        vec![],
    )
}
fn text(id: u16, frame: [Expr; 4], value: Expr, scale: f32, color: Expr) -> TemplateNode {
    node(
        id,
        frame,
        Shape::Text {
            value,
            scale: n(scale),
            color,
        },
        vec![],
    )
}
fn param(key: u16, name: &str, value_type: ValueType, default: Expr) -> Parameter {
    Parameter {
        key,
        name: name.into(),
        value_type,
        default,
    }
}
fn template(
    id: u16,
    name: &str,
    parameters: Vec<Parameter>,
    children: Vec<TemplateNode>,
) -> Template {
    Template {
        id,
        name: name.into(),
        parameters,
        root: node(0, full(), Shape::Canvas { clip: true }, children),
    }
}
fn prop(key: u16, value: Expr) -> Property {
    Property { key, value }
}
fn instance(
    id: u32,
    template: u16,
    frame: [f32; 4],
    properties: Vec<Property>,
    children: Vec<Instance>,
) -> Instance {
    Instance {
        id,
        template,
        frame,
        properties,
        children,
    }
}
pub fn blueprint() -> Blueprint {
    let bg = [16, 28, 41, 255];
    let card = [27, 44, 61, 255];
    let white = [239, 244, 236, 255];
    let muted = [150, 171, 185, 255];
    let accent = [185, 238, 143, 255];
    let mut root = template(
        1,
        "Screen",
        vec![param(2, "background", ValueType::Color, c(bg))],
        vec![paint(1, p(2), n(0.)), slot()],
    );
    root.root.responder = Some(Responder::Bubble);
    let label = Template {
        id: 2,
        name: "Text".into(),
        parameters: vec![
            param(1, "text", ValueType::Text, s("")),
            param(2, "color", ValueType::Color, c(white)),
            param(3, "scale", ValueType::Number, n(2.)),
        ],
        root: node(
            0,
            full(),
            Shape::Text {
                value: p(1),
                scale: p(3),
                color: p(2),
            },
            vec![],
        ),
    };
    let panel = template(
        3,
        "Panel",
        vec![
            param(2, "background", ValueType::Color, c(card)),
            param(3, "radius", ValueType::Number, n(16.)),
        ],
        vec![paint(1, p(2), p(3)), slot()],
    );
    let fg = choose(p(4), c(bg), c(white));
    let mut switch = template(
        4,
        "Switch",
        vec![
            param(1, "label", ValueType::Text, s("Toggle")),
            param(2, "on-color", ValueType::Color, c(accent)),
            param(3, "off-color", ValueType::Color, c(card)),
            param(
                4,
                "checked",
                ValueType::Boolean,
                Expr::State(Binding::Enabled),
            ),
        ],
        vec![
            paint(1, choose(p(4), p(2), p(3)), n(16.)),
            text(
                2,
                [n(32.), n(32.), sub(Expr::Width, n(64.)), n(24.)],
                p(1),
                3.,
                fg.clone(),
            ),
            text(
                3,
                [n(32.), n(88.), n(160.), n(40.)],
                choose(p(4), s("ON"), s("OFF")),
                5.,
                fg.clone(),
            ),
            node(
                20,
                [sub(Expr::Width, n(164.)), n(84.), n(132.), n(64.)],
                Shape::Canvas { clip: true },
                vec![
                    node(
                        21,
                        [n(0.), n(0.), n(132.), n(64.)],
                        Shape::Superellipse {
                            radius: n(32.),
                            exponent: 4.,
                            color: choose(p(4), c([56, 84, 55, 255]), c([74, 94, 113, 255])),
                        },
                        vec![],
                    ),
                    node(
                        22,
                        [choose(p(4), n(72.), n(8.)), n(8.), n(48.), n(48.)],
                        Shape::Circle { color: c(white) },
                        vec![],
                    ),
                ],
            ),
            text(
                4,
                [
                    n(32.),
                    sub(Expr::Height, n(42.)),
                    sub(Expr::Width, n(64.)),
                    n(16.),
                ],
                s("TAP TO CHANGE"),
                2.,
                fg,
            ),
            slot(),
        ],
    );
    switch.root.responder = Some(Responder::ToggleFocus);
    switch.root.semantics = Some(Semantics {
        label: p(1),
        checked: p(4),
    });
    let gw = sub(Expr::Width, n(80.));
    let gh = sub(Expr::Height, n(184.));
    let mut graphchildren = vec![
        paint(1, p(2), n(16.)),
        text(
            2,
            [n(32.), n(32.), sub(Expr::Width, n(64.)), n(24.)],
            p(1),
            3.,
            c(white),
        ),
        text(
            3,
            [n(32.), n(76.), sub(Expr::Width, n(64.)), n(16.)],
            s("Data from Last 5min"),
            2.,
            c(muted),
        ),
    ];
    for i in 0..=4 {
        graphchildren.push(node(
            10 + i,
            [
                n(40.),
                add(n(136.), mul(n(i as f32 / 4.), gh.clone())),
                gw.clone(),
                n(1.),
            ],
            Shape::Superellipse {
                radius: n(0.),
                exponent: 2.,
                color: c([48, 68, 85, 255]),
            },
            vec![],
        ));
    }
    graphchildren.push(node(
        30,
        [n(40.), n(136.), gw, gh.clone()],
        Shape::SeriesPath {
            series: Expr::State(Binding::Samples),
            width: 3.,
            color: p(3),
        },
        vec![],
    ));
    graphchildren.push(slot());
    let graph = template(
        5,
        "Graph",
        vec![
            param(1, "label", ValueType::Text, s("Toggle signal")),
            param(2, "background", ValueType::Color, c(card)),
            param(3, "line-color", ValueType::Color, c(accent)),
        ],
        graphchildren,
    );
    Blueprint {
        schema: "otrios-layout-v1".into(),
        templates: vec![root, label, panel, switch, graph],
        root: instance(
            0,
            1,
            [0., 0., 1280., 800.],
            vec![],
            vec![
                instance(
                    6,
                    2,
                    [64., 25., 1152., 16.],
                    vec![prop(1, s("OTRIOS  /  LIVE SPACES")), prop(2, c(muted))],
                    vec![],
                ),
                instance(
                    1,
                    2,
                    [64., 88., 1152., 48.],
                    vec![prop(1, Expr::State(Binding::Title)), prop(3, n(6.))],
                    vec![],
                ),
                instance(2, 4, [64., 208., 440., 240.], vec![], vec![]),
                instance(3, 5, [536., 208., 680., 400.], vec![], vec![]),
                instance(
                    4,
                    3,
                    [64., 500., 440., 108.],
                    vec![],
                    vec![instance(
                        7,
                        2,
                        [24., 38., 416., 32.],
                        vec![prop(1, Expr::State(Binding::TouchLabel)), prop(3, n(4.))],
                        vec![],
                    )],
                ),
            ],
        ),
    }
}
