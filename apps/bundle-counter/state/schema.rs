// SPDX-License-Identifier: Apache-2.0
pub const COUNT: crate::authoring::StateField =
    crate::authoring::StateField::normalized(1, "count", 0);
pub fn schema() -> otrios_interfaces::canvas::StateSchema {
    crate::authoring::schema("bundle-counter-state-v1", &[COUNT])
}
