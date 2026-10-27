// SPDX-License-Identifier: Apache-2.0
use otrios_interfaces::{
    canvas::{Field, StateSchema},
    layout::Value,
};
pub const CARDS: [&str; 7] = [
    "component-playground",
    "layout-lab",
    "layout-document",
    "layout-text-scroll",
    "input-lab",
    "animation-lab",
    "three-d-lab",
];
pub fn schema() -> StateSchema {
    StateSchema {
        id: "test-playground-state-v1".into(),
        fields: CARDS
            .iter()
            .enumerate()
            .map(|(i, card)| Field {
                id: i as u16 + 1,
                name: format!("{card}-position"),
                default: Value::Text(format!("{}:{:04x}", (b'B' + i as u8 * 8) as char, i + 1)),
                writable: true,
            })
            .collect(),
    }
}
