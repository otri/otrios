// SPDX-License-Identifier: Apache-2.0
use crate::authoring::{StateField, schema};
pub const VOLUME: StateField = StateField::normalized(1, "volume", 400);
pub const BRIGHTNESS: StateField = StateField::normalized(2, "brightness", 700);
pub const AUDIO_ENABLED: StateField = StateField::normalized(3, "audio-enabled", 1000);
pub const DISPLAY_ENABLED: StateField = StateField::normalized(4, "display-enabled", 0);
pub fn description() -> otrios_interfaces::canvas::StateSchema {
    schema(
        "oib-designer-state-v1",
        &[VOLUME, BRIGHTNESS, AUDIO_ENABLED, DISPLAY_ENABLED],
    )
}
