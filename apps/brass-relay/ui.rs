// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::{GestureKind, Handler, Operation, Presentation, Ui};
pub fn description() -> Ui {
    let mut ui = AppUi::new(
        "brass-relay",
        "The Brass Relay",
        schema("brass-relay-state-v1", &[]),
    )
    .with_composition(
        column(0., vec![heading().minimum([1., 72.])]),
        column(0., vec![heading().minimum([1., 72.])]),
    )
    .with_presentation(Presentation::Chat)
    .build("The Brass Relay", vec![crate::components::keyboard()]);
    for command in 0..=200 {
        ui.handlers.push(Handler {
            control: (command + 40) as u8,
            event: 0,
            kind: GestureKind::Activate,
            operation: Operation::Chat(command),
            fields: vec![],
        });
    }
    ui
}
