// SPDX-License-Identifier: Apache-2.0
//! Monitoring's local immutable view components; no ambient dashboard state.
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

pub fn blueprint() -> Blueprint {
    let bg = [16, 28, 41, 255];
    let card = [27, 44, 61, 255];
    let white = [239, 244, 236, 255];
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
    Blueprint {
        schema: "otrios-layout-v2".into(),
        templates: vec![root, label, panel],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    }
}
