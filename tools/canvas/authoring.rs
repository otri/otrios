// SPDX-License-Identifier: Apache-2.0
//! Build-time UI authoring helpers; output is a binary primitive description.
use otrios_interfaces::{canvas::*, layout::*};
pub const BG: [u8; 4] = [16, 28, 41, 255];
pub const CARD: [u8; 4] = [27, 44, 61, 255];
pub const WHITE: [u8; 4] = [239, 244, 236, 255];
pub const ACCENT: [u8; 4] = [185, 238, 143, 255];
pub fn num(v: f32) -> Expr {
    Expr::Literal(Value::Number(v))
}
pub fn color(v: [u8; 4]) -> Expr {
    Expr::Literal(Value::Color(v))
}
pub fn text(v: &str) -> Expr {
    Expr::Literal(Value::Text(v.into()))
}
pub fn binding(v: u16) -> Expr {
    Expr::State(Binding::Local(v))
}
pub fn node(id: u16, frame: [f32; 4], shape: Shape) -> TemplateNode {
    TemplateNode {
        id,
        frame: frame.map(num),
        shape,
        visible: Expr::Literal(Value::Boolean(true)),
        responder: None,
        semantics: None,
        children: vec![],
    }
}
pub fn label(id: u16, xy: [f32; 2], value: Expr, scale: f32) -> TemplateNode {
    node(
        id,
        [xy[0], xy[1], 1100., 48.],
        Shape::Text {
            value,
            scale: num(scale),
            color: color(WHITE),
        },
    )
}
pub fn round(id: u16, frame: [f32; 4], tint: [u8; 4]) -> TemplateNode {
    node(
        id,
        frame,
        Shape::Superellipse {
            radius: num(16.),
            exponent: 4.,
            color: color(tint),
        },
    )
}
pub fn button(id: u16, frame: [f32; 4], title: &str, action: u8) -> TemplateNode {
    let mut b = node(id, frame, Shape::Canvas { clip: true });
    b.responder = Some(Responder::AppNavigate(action));
    b.semantics = Some(Semantics {
        label: text(title),
        checked: Expr::Literal(Value::Boolean(false)),
    });
    b.children = vec![
        round(id + 1, [0., 0., frame[2], frame[3]], CARD),
        label(id + 2, [24., 26.], text(title), 2.),
    ];
    b
}
pub fn control(mut n: TemplateNode, label: &str, control: u8, drag: bool) -> TemplateNode {
    n.responder = Some(Responder::AppEvent { control, drag });
    n.semantics = Some(Semantics {
        label: text(label),
        checked: Expr::Literal(Value::Boolean(false)),
    });
    n
}
pub fn blueprint(title: &str, mut children: Vec<TemplateNode>) -> Vec<u8> {
    let mut root = node(0, [0., 0., 1280., 800.], Shape::Canvas { clip: true });
    root.children = vec![
        round(1, [0., 0., 1280., 800.], BG),
        label(2, [52., 45.], text(title), 4.),
    ];
    root.children.append(&mut children);
    otrios_core::dashboard::layout::Layout::encode(&Blueprint {
        schema: "otrios-layout-v1".into(),
        templates: vec![Template {
            id: 1,
            name: "App canvas".into(),
            parameters: vec![],
            root,
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    })
    .expect("valid app UI")
}
pub fn field(id: u16, name: &str, default: u16) -> Field {
    Field {
        id,
        name: name.into(),
        default,
    }
}
pub fn app(id: &str, title: &str, status: &str, deps: &[&str]) -> App {
    App {
        id: id.into(),
        title: title.into(),
        ui_ref: format!("app:{id}/ui"),
        state_ref: format!("app:{id}/state"),
        status: status.into(),
        dependencies: deps.iter().map(|v| (*v).into()).collect(),
    }
}
pub fn home_nav() -> Navigation {
    Navigation {
        action: 0,
        ui_ref: "app:home/ui".into(),
        state_ref: "app:home/state".into(),
    }
}
