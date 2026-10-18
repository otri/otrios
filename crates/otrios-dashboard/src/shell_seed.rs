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
            resize: None,
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
/// The independent Layout Lab app projects time on its host presentation clock.
fn layout_app() -> Blueprint {
    let mut layout = layout_seed::blueprint();
    layout.root.children.retain(|child| child.id != 6);
    layout
        .root
        .children
        .iter_mut()
        .find(|child| child.id == 1)
        .unwrap()
        .properties = vec![prop(1, s("Layout Lab")), prop(3, n(6.))];
    let graph = layout
        .templates
        .iter_mut()
        .find(|template| template.id == 5)
        .unwrap();
    graph
        .root
        .children
        .iter_mut()
        .find(|node| node.id == 30)
        .unwrap()
        .shape = Shape::TimedSeriesPath {
        series: Expr::State(Binding::Local(730)),
        width: 3.,
        color: Expr::Parameter(3),
    };
    layout
}

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
            Screen::Controls,
            "COMPONENT PLAYGROUND",
            "Touch + CRDT state",
            "Changes follow across devices",
        ),
        (
            Screen::Layout,
            "LAYOUT LAB",
            "Reusable components",
            "A five minute toggle signal",
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
            "Tetrahedra + rotation",
            "Shared intent, independent playback",
        ),
        (
            Screen::Monitoring,
            "MONITORING",
            "Live service data",
            "Seven days of service history",
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
        if *screen == Screen::Monitoring {
            // A padlock built from the same primitives on native and web hosts.
            // These children bubble to the card's existing Open responder.
            for (frame, color, radius) in [
                ([336., 16., 24., 28.], MUTED, 12.),
                ([341., 21., 14., 23.], [27, 44, 61, 255], 7.),
                ([331., 33., 34., 26.], MUTED, 5.),
                ([346., 41., 4., 10.], [27, 44, 61, 255], 2.),
            ] {
                a.add(3, frame, vec![prop(2, c(color)), prop(3, n(radius))]);
            }
        }
        a.label(
            [16., 76., 352., 24.],
            if *screen == Screen::Monitoring {
                local(506)
            } else {
                s(line1)
            },
            2.,
            WHITE,
        );
        a.label([16., 112., 352., 32.], s(line2), 2., MUTED);
        a.label([16., 156., 320., 16.], s("OPEN  ->"), 2., GREEN);
        let children = a.b.root.children.drain(button + 1..).collect();
        a.b.root.children[button].children = children;
    }
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
fn playground() -> Blueprint {
    fn add(a: Expr, b: Expr) -> Expr {
        Expr::Add(Box::new(a), Box::new(b))
    }
    fn sub(a: Expr, b: Expr) -> Expr {
        Expr::Subtract(Box::new(a), Box::new(b))
    }
    fn mul(a: Expr, b: Expr) -> Expr {
        Expr::Multiply(Box::new(a), Box::new(b))
    }
    fn choose(condition: Expr, yes: Expr, no: Expr) -> Expr {
        Expr::Select {
            condition: Box::new(condition),
            yes: Box::new(yes),
            no: Box::new(no),
        }
    }
    fn node(id: u16, frame: [Expr; 4], shape: Shape) -> TemplateNode {
        TemplateNode {
            resize: None,
            id,
            frame,
            shape,
            visible: Expr::Literal(Value::Boolean(true)),
            responder: None,
            semantics: None,
            children: vec![],
        }
    }
    fn paint(id: u16, frame: [Expr; 4], color: Expr, radius: f32) -> TemplateNode {
        node(
            id,
            frame,
            Shape::Superellipse {
                radius: n(radius),
                exponent: 4.,
                color,
            },
        )
    }
    fn circle(id: u16, frame: [Expr; 4], color: Expr) -> TemplateNode {
        node(id, frame, Shape::Circle { color })
    }
    let mut a = Author::new(false);
    a.label(
        [48., 112., 1184., 48.],
        s("COMPONENT PLAYGROUND"),
        4.,
        WHITE,
    );
    a.label(
        [48., 174., 1184., 24.],
        s("Drag, rotate, explore. The values follow across devices."),
        2.,
        MUTED,
    );
    for (x, w, title, subtitle) in [
        (48., 370., "01 / LINEAR", "Drag the thumb"),
        (440., 370., "02 / ROTARY", "Turn the ring"),
        (832., 400., "03 / TOUCH PAD", "Draw / fades in 5s"),
    ] {
        a.add(3, [x, 230., w, 382.], vec![]);
        let parent = a.b.root.children.len() - 1;
        a.label([24., 24., w - 48., 26.], s(title), 2., GREEN);
        a.label([24., 62., w - 48., 22.], s(subtitle), 2., MUTED);
        let children = a.b.root.children.drain(parent + 1..).collect();
        a.b.root.children[parent].children = children;
    }
    let mut slider = node(
        0,
        [n(0.), n(0.), Expr::Width, Expr::Height],
        Shape::Canvas { clip: true },
    );
    slider.responder = Some(Responder::DragControl(0));
    slider.semantics = Some(Semantics {
        label: s("Intensity slider"),
        checked: Expr::Literal(Value::Boolean(false)),
    });
    let active = choose(Expr::State(Binding::Enabled), c(GREEN), c(MUTED));
    slider.children = vec![
        paint(
            1,
            [n(20.), n(34.), sub(Expr::Width, n(40.)), n(12.)],
            c([45, 65, 78, 255]),
            6.,
        ),
        paint(
            2,
            [
                n(20.),
                n(34.),
                mul(sub(Expr::Width, n(40.)), local(600)),
                n(12.),
            ],
            active.clone(),
            0.,
        ),
        circle(
            3,
            [
                mul(sub(Expr::Width, n(40.)), local(600)),
                n(20.),
                n(40.),
                n(40.),
            ],
            active.clone(),
        ),
    ];
    slider.children[1].visible = local(712);
    let mut glow = circle(
        4,
        [
            sub(mul(sub(Expr::Width, n(40.)), local(600)), n(3.)),
            n(17.),
            n(46.),
            n(46.),
        ],
        c([185, 238, 143, 80]),
    );
    glow.visible = local(720);
    slider.children.insert(2, glow);
    a.b.templates.push(Template {
        id: 40,
        name: "Slider".into(),
        parameters: vec![],
        root: slider,
    });
    a.add(40, [72., 388., 322., 80.], vec![]);
    a.label([72., 500., 322., 56.], local(610), 5., WHITE);
    a.label([72., 570., 322., 20.], s("DRAG OUTSIDE TO TEST"), 2., MUTED);
    let mut dial = node(
        0,
        [n(0.), n(0.), Expr::Width, Expr::Height],
        Shape::Canvas { clip: true },
    );
    dial.responder = Some(Responder::DragControl(1));
    dial.semantics = Some(Semantics {
        label: s("Rotary dial"),
        checked: Expr::Literal(Value::Boolean(false)),
    });
    dial.children.push(circle(
        1,
        [
            n(16.),
            n(16.),
            sub(Expr::Width, n(32.)),
            sub(Expr::Height, n(32.)),
        ],
        c([16, 28, 41, 255]),
    ));
    for i in 0..12 {
        let angle = (i as f32 / 11. - 0.5) * std::f32::consts::PI * 1.5;
        dial.children.push(circle(
            10 + i,
            [
                sub(mul(Expr::Width, n(0.5 + angle.sin() * 0.44)), n(3.)),
                sub(mul(Expr::Height, n(0.5 - angle.cos() * 0.44)), n(3.)),
                n(6.),
                n(6.),
            ],
            choose(local(700 + i), active.clone(), c([58, 80, 94, 255])),
        ));
    }
    dial.children.push(circle(
        30,
        [
            sub(mul(Expr::Width, local(604)), n(8.)),
            sub(mul(Expr::Height, local(605)), n(8.)),
            n(16.),
            n(16.),
        ],
        active,
    ));
    dial.children.push(circle(
        31,
        [
            sub(mul(Expr::Width, n(0.5)), n(18.)),
            sub(mul(Expr::Height, n(0.5)), n(18.)),
            n(36.),
            n(36.),
        ],
        c([34, 54, 67, 255]),
    ));
    let mut glow = circle(
        2,
        [
            n(10.),
            n(10.),
            sub(Expr::Width, n(20.)),
            sub(Expr::Height, n(20.)),
        ],
        c([185, 238, 143, 70]),
    );
    glow.visible = local(721);
    dial.children.insert(0, glow);
    a.b.templates.push(Template {
        id: 41,
        name: "Dial".into(),
        parameters: vec![],
        root: dial,
    });
    a.add(41, [520., 336., 210., 210.], vec![]);
    a.label([464., 556., 322., 40.], local(611), 3., WHITE);
    let mut pad = node(
        0,
        [n(0.), n(0.), Expr::Width, Expr::Height],
        Shape::Canvas { clip: true },
    );
    pad.responder = Some(Responder::DragControl(2));
    pad.semantics = Some(Semantics {
        label: local(617),
        checked: Expr::Literal(Value::Boolean(false)),
    });
    pad.children.push(paint(
        1,
        [n(0.), n(0.), Expr::Width, Expr::Height],
        choose(local(722), c([22, 44, 58, 255]), c([16, 28, 41, 255])),
        12.,
    ));
    for i in 1..4 {
        let f = i as f32 / 4.;
        pad.children.push(paint(
            10 + i,
            [mul(Expr::Width, n(f)), n(0.), n(1.), Expr::Height],
            c([40, 61, 76, 255]),
            0.,
        ));
        pad.children.push(paint(
            20 + i,
            [n(0.), mul(Expr::Height, n(f)), Expr::Width, n(1.)],
            c([40, 61, 76, 255]),
            0.,
        ));
    }
    let px = add(n(14.), mul(sub(Expr::Width, n(28.)), local(602)));
    let py = add(
        n(14.),
        mul(sub(Expr::Height, n(28.)), sub(n(1.), local(603))),
    );
    pad.children.push(paint(
        60,
        [px.clone(), n(0.), n(1.), Expr::Height],
        c([94, 169, 181, 110]),
        0.,
    ));
    pad.children.push(paint(
        61,
        [n(0.), py.clone(), Expr::Width, n(1.)],
        c([94, 169, 181, 110]),
        0.,
    ));
    pad.children.push(circle(
        62,
        [
            sub(px.clone(), n(12.)),
            sub(py.clone(), n(12.)),
            n(24.),
            n(24.),
        ],
        c([84, 174, 183, 255]),
    ));
    pad.children.push(circle(
        63,
        [sub(px, n(4.)), sub(py, n(4.)), n(8.), n(8.)],
        c(WHITE),
    ));
    a.b.templates.push(Template {
        id: 42,
        name: "TouchPad".into(),
        parameters: vec![],
        root: pad,
    });
    a.add(42, [856., 336., 352., 210.], vec![]);
    a.label([856., 566., 352., 24.], local(612), 2., WHITE);
    for (id, responder, frame, label) in [
        (
            50,
            Responder::ResetControls,
            [48., 636., 176., 64.],
            s("RESET"),
        ),
        (
            51,
            Responder::ToggleFocus,
            [240., 636., 224., 64.],
            local(616),
        ),
    ] {
        a.button_template(id, responder);
        if id == 51 {
            a.b.templates
                .last_mut()
                .unwrap()
                .root
                .semantics
                .as_mut()
                .unwrap()
                .checked = Expr::State(Binding::Enabled);
        }
        a.add(id, frame, vec![prop(1, label)]);
    }
    a.label([48., 726., 1184., 22.], local(613), 2., MUTED);
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
        ("playground", playground()),
        ("layout", layout_app()),
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
                layout_app(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/layout.flex")
                    .as_slice(),
            ),
            (
                playground(),
                include_bytes!("../../otrios-core/src/dashboard/shell-assets/playground.flex")
                    .as_slice(),
            ),
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
