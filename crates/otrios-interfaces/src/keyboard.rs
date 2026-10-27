// SPDX-License-Identifier: Apache-2.0
//! Input methods emit edits. The focused application owns its text and selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Commit(String),
    /// Atomic native-browser value/selection update. Offsets are UTF-16 code
    /// units (as in HTML inputs); the editor validates limits and boundaries.
    Replace {
        text: String,
        cursor: u32,
        anchor: u32,
        /// Selection notifications must not restore an obsolete shared value.
        selection_only: bool,
    },
    Backspace,
    Left {
        select: bool,
    },
    Right {
        select: bool,
    },
    SelectAll,
    Tab {
        backward: bool,
    },
    Enter,
}

/// Mount-local host editing surface. This is never part of replicated state.
#[cfg(feature = "layout")]
#[derive(Clone, Debug, serde::Serialize)]
pub struct TextInput {
    pub id: u32,
    pub value: String,
    pub cursor: u32,
    pub anchor: u32,
    pub focused: bool,
    pub multiline: bool,
    pub secret: bool,
    pub max_bytes: usize,
    pub bounds: [f32; 4],
    /// Text inset inside the control: left, top, right, bottom.
    pub inset: [f32; 4],
    pub font_size: f32,
    pub line_height: f32,
}
