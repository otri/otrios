// SPDX-License-Identifier: Apache-2.0
use super::state::*;
use crate::authoring::*;
use otrios_interfaces::canvas::{Presentation, Ui};
pub fn description() -> Ui {
    let fields = [FIRST, SECOND]
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            canvas(vec![
                surface(BG, 12.).pin_edges(0.),
                text(f.value())
                    .frame([24., 20., 1100., 48.])
                    .pin_left(24.)
                    .pin_right(24.),
            ])
            .frame([24., 160. + i as f32 * 110., 1232., 88.])
            .key(if i == 0 { "first" } else { "second" })
            .semantic_label(if i == 0 {
                "First text field"
            } else {
                "Second text field"
            })
            .on_press(Event::focus(f))
        })
        .collect::<Vec<_>>();
    let mut body = vec![
        text("Shared text / local caret, focus and keyboard")
            .color(MUTED)
            .key("intro"),
    ];
    body.extend(fields);
    body.push(
        crate::components::keyboard()
            .frame([0., 480., 1280., 320.])
            .pin_left(0.)
            .pin_right(0.)
            .pin_bottom(0.),
    );
    AppUi::new("input-lab", "Text Input", super::state::description())
        .with_breadcrumbs(&[("OTriOS", "home"), ("Test Playground", "test-playground")])
        .with_composition(
            column(
                20.,
                vec![
                    heading(),
                    place("intro", [1232., 32.]).minimum([1., 32.]).flex_width(),
                    place("first", [1232., 88.]).minimum([1., 88.]).flex_width(),
                    place("second", [1232., 88.])
                        .minimum([1., 88.])
                        .flex_width(),
                ],
            ),
            column(
                24.,
                vec![
                    heading(),
                    place("intro", [752., 32.]).minimum([1., 32.]).flex_width(),
                    place("first", [752., 88.]).minimum([1., 88.]).flex_width(),
                    place("second", [752., 88.]).minimum([1., 88.]).flex_width(),
                ],
            ),
        )
        .with_presentation(Presentation::Editor {
            fields: vec![FIRST.id(), SECOND.id()],
        })
        .build("Text Input", body)
}
