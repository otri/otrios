// SPDX-License-Identifier: Apache-2.0
use otrios_interfaces::{
    canvas::{Field, StateSchema},
    layout::Value,
};
pub const CARDS: [&str; 6] = [
    "component-playground",
    "layout-lab",
    "input-lab",
    "animation-lab",
    "three-d-lab",
    "monitoring",
];
pub fn schema() -> StateSchema {
    StateSchema {
        id: "home-state-v3".into(),
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
