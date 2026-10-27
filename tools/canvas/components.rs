// SPDX-License-Identifier: Apache-2.0
//! Reusable declarative input components; no app or document ownership.
use crate::authoring::*;
pub fn keyboard() -> Element {
    let mut keys = vec![];
    for i in 0..10 {
        keys.push((i, [16. + f32::from(i) * 108., 496., 100., 62.]));
    }
    keys.extend([(10, [1096., 496., 168., 62.]), (11, [16., 566., 112., 62.])]);
    for i in 0..9 {
        keys.push((12 + i, [136. + f32::from(i) * 108., 566., 100., 62.]));
    }
    keys.extend([(21, [1108., 566., 156., 62.]), (22, [16., 636., 144., 62.])]);
    for i in 0..9 {
        keys.push((23 + i, [168. + f32::from(i) * 122., 636., 114., 62.]));
    }
    keys.extend([
        (32, [16., 706., 124., 62.]),
        (33, [148., 706., 124., 62.]),
        (34, [280., 706., 584., 62.]),
        (35, [872., 706., 120., 62.]),
        (36, [1000., 706., 120., 62.]),
        (37, [1128., 706., 136., 62.]),
    ]);
    let keyboard = otrios_core::dashboard::keyboard::Keyboard::default();
    // Keys do not overlap: paint their backgrounds together, then their labels.
    // This preserves painter order while avoiding a GPU program switch per key.
    let mut children: Vec<_> = keys
        .iter()
        .map(|(_, frame)| rounded_rect(CARD).frame(*frame))
        .collect();
    children.extend(keys.into_iter().map(|(key, frame)| {
        canvas(vec![
            keyboard_label(key)
                .frame([12., 20., frame[2] - 24., 32.])
                .pin_left(12.)
                .pin_right(12.),
        ])
        .frame(frame)
        .semantic_label(&keyboard.label(key))
        .on_press(Event::key(key))
    }));
    canvas(children).visible(keyboard_visible())
}

/// Primitive-only icons keep launcher cards identical on both renderers.
#[derive(Clone, Copy)]
pub enum LaunchIcon {
    Chat,
    Playground,
    Components,
    Layouts,
    Document,
    Panning,
    Input,
    Animation2d,
    Animation3d,
    Monitoring,
}
fn launch_icon(kind: LaunchIcon) -> Element {
    let bar = |x, y, w, h| {
        rounded_rect(ACCENT)
            .frame([x, y, w, h])
            .corner_radius(1.0.into())
    };
    let parts = match kind {
        LaunchIcon::Chat => vec![
            bar(4., 8., 40., 28.),
            rectangle(CARD).frame([8., 12., 32., 20.]),
            bar(10., 34., 5., 10.),
            bar(18., 18., 4., 4.),
            bar(28., 18., 4., 4.),
            bar(18., 27., 14., 3.),
        ],
        LaunchIcon::Playground => vec![
            bar(0., 0., 20., 20.),
            circle(TEAL).frame([28., 0., 20., 20.]),
            bar(0., 28., 20., 20.),
            bar(28., 28., 20., 20.),
        ],
        LaunchIcon::Components => vec![
            bar(0., 8., 48., 4.),
            bar(0., 34., 48., 4.),
            circle(ACCENT).frame([8., 2., 16., 16.]),
            circle(TEAL).frame([28., 28., 16., 16.]),
        ],
        LaunchIcon::Layouts => vec![
            bar(0., 0., 48., 10.),
            bar(0., 18., 18., 30.),
            bar(26., 18., 22., 12.),
            bar(26., 38., 22., 10.),
        ],
        LaunchIcon::Document => vec![
            bar(6., 0., 36., 48.),
            rectangle(CARD).frame([10., 4., 28., 40.]),
            bar(16., 12., 16., 3.),
            bar(16., 22., 16., 3.),
            bar(16., 32., 12., 3.),
        ],
        LaunchIcon::Panning => vec![
            bar(22., 0., 4., 48.),
            bar(0., 22., 48., 4.),
            bar(14., 0., 20., 4.),
            bar(14., 44., 20., 4.),
            bar(0., 14., 4., 20.),
            bar(44., 14., 4., 20.),
        ],
        LaunchIcon::Input => {
            let mut v = vec![
                bar(0., 8., 48., 34.),
                rectangle(CARD).frame([4., 12., 40., 26.]),
            ];
            for y in [17., 25.] {
                for x in [9., 19., 29.] {
                    v.push(bar(x, y, 6., 4.));
                }
            }
            v.push(bar(12., 33., 24., 3.));
            v
        }
        LaunchIcon::Animation2d => vec![
            circle(TRACK).frame([0., 12., 24., 24.]),
            circle(TEAL).frame([12., 12., 24., 24.]),
            circle(ACCENT).frame([24., 12., 24., 24.]),
        ],
        LaunchIcon::Animation3d => vec![
            bar(14., 0., 34., 34.),
            rectangle(CARD).frame([18., 4., 26., 26.]),
            bar(0., 14., 34., 34.),
            rectangle(CARD).frame([4., 18., 26., 26.]),
            bar(14., 14., 20., 20.),
        ],
        LaunchIcon::Monitoring => vec![
            bar(0., 36., 10., 12.),
            bar(19., 20., 10., 28.),
            bar(38., 0., 10., 48.),
        ],
    };
    component([48., 48.], parts).frame([24., 20., 48., 48.])
}
pub fn launch_card(
    id: &str,
    route: &str,
    title: &str,
    subheading: &str,
    icon: LaunchIcon,
) -> Element {
    component(
        [384., 176.],
        vec![
            rounded_rect(CARD).pin_edges(0.).corner_radius(20.0.into()),
            launch_icon(icon),
            text(title)
                .frame([24., 84., 336., 32.])
                .font_scale(2.)
                .pin_left(24.)
                .pin_right(24.),
            text(subheading)
                .frame([24., 128., 336., 24.])
                .color(MUTED)
                .pin_left(24.)
                .pin_right(24.),
        ],
    )
    .key(id)
    .semantic_label(title)
    .navigate_to(Destination::app(id).route(route))
}
