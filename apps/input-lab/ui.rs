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
                rounded_rect(CARD).pin_edges(0.),
                text(f.value())
                    .frame([24., 20., 1100., 48.])
                    .pin_left(24.)
                    .pin_right(24.),
            ])
            .frame([64., 180. + i as f32 * 110., 1152., 88.])
            .semantic_label(if i == 0 {
                "First text field"
            } else {
                "Second text field"
            })
            .on_press(Event::focus(f))
        })
        .collect::<Vec<_>>();
    let mut body = vec![
        button("Back")
            .on_press(Event::back())
            .frame([830., 35., 140., 80.]),
        button("Home")
            .navigate_to(Destination::app("home"))
            .frame([1000., 35., 216., 80.]),
        text("Shared text / local caret, focus and keyboard").position([64., 130.]),
    ];
    body.extend(fields);
    body.push(crate::components::keyboard());
    AppUi::new("input-lab", "Text Input", super::state::description())
        .with_presentation(Presentation::Editor {
            fields: vec![FIRST.id(), SECOND.id()],
        })
        .build("Text Input", body)
}
