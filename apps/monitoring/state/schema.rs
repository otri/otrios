// SPDX-License-Identifier: Apache-2.0
use otrios_interfaces::canvas::StateSchema;
pub fn schema() -> StateSchema {
    crate::authoring::schema("monitoring-state-v2", &[])
}
