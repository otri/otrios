// SPDX-License-Identifier: Apache-2.0
pub fn schema() -> otrios_interfaces::canvas::StateSchema {
    otrios_interfaces::canvas::StateSchema {
        id: "layout-lab-state-v1".into(),
        fields: vec![
            crate::authoring::field(1, "panelWidth", 500),
            crate::authoring::field(2, "cornerRadius", 400),
        ],
    }
}
