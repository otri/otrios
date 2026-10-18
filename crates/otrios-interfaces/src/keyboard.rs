// SPDX-License-Identifier: Apache-2.0
//! Input methods emit edits. The focused application owns its text and selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Commit(String),
    Backspace,
    Left { select: bool },
    Right { select: bool },
    SelectAll,
    Tab { backward: bool },
    Enter,
}
