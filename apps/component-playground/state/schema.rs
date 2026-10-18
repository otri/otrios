// SPDX-License-Identifier: Apache-2.0
use otrios_interfaces::canvas::StateSchema;
pub fn schema() -> StateSchema {
    StateSchema {
        id: "component-playground-state-v1".into(),
        fields: vec![
            crate::authoring::field(1, "level", 420),
            crate::authoring::field(2, "angle", 250),
            crate::authoring::field(3, "padX", 500),
            crate::authoring::field(4, "padY", 500),
            crate::authoring::field(5, "enabled", 1000),
        ],
    }
}
