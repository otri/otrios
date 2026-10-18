// SPDX-License-Identifier: Apache-2.0
//! Authoring fixtures. Runtime loads the generated FlexBuffers assets.
#[path = "layout_seed.rs"]
mod layout_seed;
use otrios_interfaces::{layout::*, scene::Screen};
fn n(v: f32) -> Expr {
    Expr::Literal(Value::Number(v))
}
fn s(v: &str) -> Expr {
    Expr::Literal(Value::Text(v.into()))
}
fn c(v: [u8; 4]) -> Expr {
    Expr::Literal(Value::Color(v))
}
fn local(id: u16) -> Expr {
    Expr::State(Binding::Local(id))
}
fn prop(key: u16, value: Expr) -> Property {
    Property { key, value }
}
fn inst(id: u32, template: u16, frame: [f32; 4], properties: Vec<Property>) -> Instance {
    Instance {
        id,
        template,
        frame,
        properties,
        children: vec![],
    }
}
struct Author {
    b: Blueprint,
    next: u32,
}
impl Author {
    fn new(transparent: bool) -> Self {
        let mut b = layout_seed::blueprint();
        b.templates.retain(|t| [1, 2, 3].contains(&t.id));
        b.root.children.clear();
        if transparent {
            b.templates[0]
                .root
                .children
                .retain(|n| matches!(n.shape, Shape::Slot));
        }
        Self { b, next: 1 }
    }
    fn add(&mut self, t: u16, frame: [f32; 4], p: Vec<Property>) {
        self.b.root.children.push(inst(self.next, t, frame, p));
        self.next += 1;
    }
    fn label(&mut self, frame: [f32; 4], value: Expr, scale: f32, paint: [u8; 4]) {
        self.add(
            2,
            frame,
            vec![prop(1, value), prop(2, c(paint)), prop(3, n(scale))],
        );
    }
    fn button_template(&mut self, id: u16, responder: Responder) {
        let mut t = self.b.templates.iter().find(|t| t.id == 3).unwrap().clone();
        t.id = id;
        t.name = format!("Button {id}");
        t.parameters.push(Parameter {
            key: 1,
            name: "label".into(),
            value_type: ValueType::Text,
            default: s(""),
        });
        t.parameters.push(Parameter {
            key: 4,
            name: "key".into(),
            value_type: ValueType::Number,
            default: n(0.),
        });
        t.parameters.push(Parameter {
            key: 5,
            name: "text-color".into(),
            value_type: ValueType::Color,
            default: c([239, 244, 236, 255]),
        });
        t.parameters.push(Parameter {
            key: 6,
            name: "text-scale".into(),
            value_type: ValueType::Number,
            default: n(2.),
        });
        t.root.responder = Some(responder);
        t.root.semantics = Some(Semantics {
            label: Expr::Parameter(1),
            checked: Expr::Literal(Value::Boolean(false)),
        });

        t.root.children.push(TemplateNode {
            id: 2,
            frame: [
                n(16.),
                n(20.),
                Expr::Subtract(Box::new(Expr::Width), Box::new(n(24.))),
                n(32.),
            ],
            visible: Expr::Literal(Value::Boolean(true)),
            shape: Shape::Text {
                value: Expr::Parameter(1),
                scale: Expr::Parameter(6),
                color: Expr::Parameter(5),
            },
            responder: None,
            semantics: None,
            children: vec![],
        });
        self.b.templates.push(t);
    }
}
const WHITE: [u8; 4] = [239, 244, 236, 255];
const MUTED: [u8; 4] = [150, 171, 185, 255];
const GREEN: [u8; 4] = [185, 238, 143, 255];
fn home() -> Blueprint {
    let mut a = Author::new(false);
    a.label([48., 116., 1184., 64.], s("OTriOS"), 6., WHITE);
    a.label(
        [48., 192., 1184., 24.],
        s("A shared runtime. A place to explore."),
        3.,
        MUTED,
    );
    for (i, (screen, title, line1, line2)) in [
        (
            Screen::Monitoring,
            "MONITORING",
            "Live service data",
            "Seven days of service history",
        ),
        (
            Screen::Controls,
            "LIVE CONTROLS",
            "Touch + CRDT state",
            "Changes follow across devices",
        ),
        (
            Screen::Keyboard,
            "INPUT LAB",
            "Reusable keyboard",
            "Literal typing, focus and editing",
        ),
        (
            Screen::Animation,
            "MOTION LAB",
            "Local animation clock",
            "Stress CRDT without stopping frames",
        ),
        (
            Screen::ThreeD,
            "3D LAB",
            "Perspective + rotation",
            "Shared intent, independent playback",
        ),
    ]
    .iter()
    .enumerate()
    {
        let t = 10 + i as u16;
        a.button_template(t, Responder::Open(*screen));
        let x = 48. + (i % 3) as f32 * 400.;
        let y = 272. + (i / 3) as f32 * 208.;
        a.add(t, [x, y, 384., 192.], vec![prop(1, s(title))]);
        let button = a.b.root.children.len() - 1;
        a.label([16., 76., 352., 24.], s(line1), 2., WHITE);
        a.label([16., 112., 352., 32.], s(line2), 1., MUTED);
        a.label([16., 156., 320., 16.], s("OPEN  ->"), 2., GREEN);
        let children = a.b.root.children.drain(button + 1..).collect();
        a.b.root.children[button].children = children;
    }
    a.label(
        [48., 712., 1184., 24.],
        s("Back and Home on every screen. Navigation stays on this device."),
        2.,
        MUTED,
    );
    a.b
}
fn navigation() -> Blueprint {
    let mut a = Author::new(true);
    a.add(
        3,
        [0., 0., 1280., 80.],
        vec![prop(2, c([12, 22, 33, 255])), prop(3, n(0.))],
    );
    a.button_template(10, Responder::Back);
    a.button_template(11, Responder::Home);
    a.add(10, [16., 8., 196., 64.], vec![prop(1, local(1))]);
    a.label([244., 26., 740., 32.], local(0), 3., WHITE);
    a.add(11, [1088., 8., 176., 64.], vec![prop(1, s("HOME"))]);
    a.b
}
fn editor() -> Blueprint {
    let mut a = Author::new(false);
    a.label([48., 128., 1184., 32.], s("INPUT LAB"), 4., WHITE);
    a.label(
        [48., 188., 1184., 16.],
        s("Tap a field. The app owns text; the keyboard sends edits."),
        2.,
        MUTED,
    );
    for id in 0..2 {
        a.button_template(10 + id as u16, Responder::FocusField(id));
        a.b.templates
            .iter_mut()
            .find(|t| t.id == 10 + id as u16)
            .unwrap()
            .root
            .semantics
            .as_mut()
            .unwrap()
            .label = s(&format!("Edit field {}", id + 1));
        a.add(
            10 + id as u16,
            [48., 240. + id as f32 * 104., 1184., 80.],
            vec![
                prop(1, local(100 + id as u16)),
                prop(2, local(110 + id as u16)),
                prop(6, n(3.)),
            ],
        );
    }
    a.label([48., 444., 1184., 16.], local(102), 2., MUTED);
    a.b
}
fn motion() -> Blueprint {
    let mut a = Author::new(false);
    a.label([48., 112., 1184., 32.], local(504), 2., WHITE);
    a.label(
        [48., 160., 1184., 16.],
        s("Shared: play, speed, count. Local: phase, frames, load test. Tap CRDT LOAD to cycle."),
        1.,
        MUTED,
    );
    for (i, r) in [
        Responder::Motion(0),
        Responder::Motion(1),
        Responder::Motion(2),
        Responder::Stress,
    ]
    .into_iter()
    .enumerate()
    {
        a.button_template(10 + i as u16, r);
        a.add(
            10 + i as u16,
            [48. + i as f32 * 300., 196., 284., 56.],
            vec![prop(1, local(500 + i as u16))],
        );
    }
    a.b
}
fn keyboard() -> Blueprint {
    let mut a = Author::new(true);
    a.add(
        3,
        [0., 464., 1280., 336.],
        vec![prop(2, c([12, 22, 33, 255])), prop(3, n(0.))],
    );
    a.button_template(10, Responder::KeyParameter(4));
    let template = a.b.templates.iter_mut().find(|t| t.id == 10).unwrap();
    template.parameters.push(Parameter {
        key: 7,
        name: "label-width".into(),
        value_type: ValueType::Number,
        default: n(18.),
    });
    let label = template
        .root
        .children
        .iter_mut()
        .find(|n| n.id == 2)
        .unwrap();
    label.frame[0] = Expr::Multiply(
        Box::new(Expr::Subtract(
            Box::new(Expr::Width),
            Box::new(Expr::Parameter(7)),
        )),
        Box::new(n(0.5)),
    );
    label.frame[1] = n(23.);

    let mut keys = vec![];
    for i in 0..10 {
        keys.push((i, [16. + i as f32 * 108., 496., 100., 62.]));
    }
    keys.push((10, [1096., 496., 168., 62.]));
    keys.push((11, [16., 566., 112., 62.]));
    for i in 0..9 {
        keys.push((12 + i, [136. + i as f32 * 108., 566., 100., 62.]));
    }
    keys.push((21, [1108., 566., 156., 62.]));
    keys.push((22, [16., 636., 144., 62.]));
    for i in 0..9 {
        keys.push((23 + i, [168. + i as f32 * 122., 636., 114., 62.]));
    }
    for (id, frame) in [
        (32, [16., 706., 124., 62.]),
        (33, [148., 706., 124., 62.]),
        (34, [280., 706., 584., 62.]),
        (35, [872., 706., 120., 62.]),
        (36, [1000., 706., 120., 62.]),
        (37, [1128., 706., 136., 62.]),
    ] {
        keys.push((id, frame));
    }
    for (id, frame) in keys {
        a.add(
            10,
            frame,
            vec![
                prop(1, local(1000 + id)),
                prop(2, local(2000 + id)),
                prop(5, local(3000 + id)),
                prop(4, n(id as f32)),
                prop(7, local(4000 + id)),
            ],
        );
    }
    a.b
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Usage: dashboard-shell-assets DIRECTORY")?,
    );
    std::fs::create_dir_all(&root)?;
    for (name, b) in [
        ("home", home()),
        ("navigation", navigation()),
        ("editor", editor()),
        ("keyboard", keyboard()),
        ("motion", motion()),
    ] {
        std::fs::write(
            root.join(format!("{name}.flex")),
            otrios_core::dashboard::layout::Layout::encode(&b)?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_system_assets_match_their_authoring_fixtures() {
        for (b, bytes) in [
            (
                motion(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/motion.flex")
                    .as_slice(),
            ),
            (
                home(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/home.flex").as_slice(),
            ),
            (
                navigation(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/navigation.flex")
                    .as_slice(),
            ),
            (
                editor(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/editor.flex")
                    .as_slice(),
            ),
            (
                keyboard(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/keyboard.flex")
                    .as_slice(),
            ),
        ] {
            assert_eq!(
                otrios_core::dashboard::layout::Layout::encode(&b).unwrap(),
                bytes
            );
        }
    }
}
