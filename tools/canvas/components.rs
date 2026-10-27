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
