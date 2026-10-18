// SPDX-License-Identifier: LGPL-3.0-only
//! Display-independent literal keyboard. Focus changes cancel gestures and repeat.
use otrios_interfaces::keyboard::Edit;
use unicode_segmentation::UnicodeSegmentation;
const LETTERS: [&str; 28] = [
    "q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "a", "s", "d", "f", "g", "h", "j", "k", "l",
    "z", "x", "c", "v", "b", "n", "m", ".", "@",
];
const NUMBERS: [&str; 28] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "_", "@", "/", ":", ";", "(", ")", "&",
    ".", ",", "?", "!", "'", "\"", "+", "=", "%",
];
const SYMBOLS: [&str; 28] = [
    "[", "]", "{", "}", "<", ">", "\\", "|", "~", "`", "^", "$", "*", "#", "%", "=", "+", "-", "_",
    "@", "/", ":", ";", "!", "?", ".", ",", "\"",
];
pub const TEXT_KEYS: [u16; 28] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18, 19, 20, 23, 24, 25, 26, 27, 28, 29,
    30, 31,
];
pub const KEY_COUNT: u16 = 38;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Shift {
    #[default]
    Off,
    Once,
    Locked,
}
#[derive(Default)]
pub struct Keyboard {
    pub shift: Shift,
    pub page: u8,
    pub visible: bool,
    contact: Option<(u32, Option<u16>)>,
    repeat_at: Option<u64>,
}
impl Keyboard {
    pub fn cancel(&mut self) {
        self.contact = None;
        self.repeat_at = None;
    }
    pub fn focus(&mut self) {
        self.cancel();
        if self.shift != Shift::Locked {
            self.shift = Shift::Off;
        }
        self.visible = true;
    }
    pub fn hide(&mut self) {
        self.cancel();
        self.visible = false;
    }
    pub fn pressed(&self, key: u16) -> bool {
        self.contact.is_some_and(|(_, k)| k == Some(key))
    }
    pub fn touching(&self, id: u32) -> bool {
        self.contact.is_some_and(|(p, _)| p == id)
    }
    pub fn label(&self, key: u16) -> String {
        if let Some(i) = TEXT_KEYS.iter().position(|k| *k == key) {
            let token = match self.page {
                1 => NUMBERS[i],
                2 => SYMBOLS[i],
                _ => LETTERS[i],
            };
            return if self.page == 0 && self.shift != Shift::Off {
                token.to_ascii_uppercase()
            } else {
                token.into()
            };
        }
        match key {
            10 => "BKSP",
            11 => {
                if self.shift == Shift::Off {
                    "TAB"
                } else {
                    "< TAB"
                }
            }
            21 => "ENTER",
            22 => match self.shift {
                Shift::Off => "SHIFT",
                Shift::Once => "SHIFT1",
                Shift::Locked => "CAPS",
            },
            32 => {
                if self.page == 0 {
                    "123"
                } else {
                    "ABC"
                }
            }
            33 => {
                if self.page == 2 {
                    "123"
                } else {
                    "#+="
                }
            }
            34 => "SPACE",
            35 => "<-",
            36 => "->",
            37 => "HIDE",
            _ => "",
        }
        .into()
    }
    fn repeats(key: u16) -> bool {
        [10, 35, 36].contains(&key)
    }
    pub fn press(&mut self, key: u16) -> Option<Edit> {
        if key >= KEY_COUNT || !self.visible {
            return None;
        }
        if TEXT_KEYS.contains(&key) {
            let text = self.label(key);
            if self.page == 0
                && text.chars().all(|c| c.is_ascii_alphabetic())
                && self.shift == Shift::Once
            {
                self.shift = Shift::Off;
            }
            return Some(Edit::Commit(text));
        }
        match key {
            10 => Some(Edit::Backspace),
            11 => {
                let backward = self.shift != Shift::Off;
                if self.shift == Shift::Once {
                    self.shift = Shift::Off;
                }
                Some(Edit::Tab { backward })
            }
            21 => Some(Edit::Enter),
            22 => {
                self.shift = match self.shift {
                    Shift::Off => Shift::Once,
                    Shift::Once => Shift::Locked,
                    Shift::Locked => Shift::Off,
                };
                None
            }
            32 => {
                self.page = if self.page == 0 { 1 } else { 0 };
                None
            }
            33 => {
                self.page = if self.page == 2 { 1 } else { 2 };
                None
            }
            34 => Some(Edit::Commit(" ".into())),
            35 => Some(Edit::Left { select: false }),
            36 => Some(Edit::Right { select: false }),
            37 => {
                self.hide();
                None
            }
            _ => None,
        }
    }
    pub fn down(&mut self, id: u32, key: u16, now: u64) -> Option<Edit> {
        if !self.visible || self.contact.is_some() || key >= KEY_COUNT {
            return None;
        }
        self.contact = Some((id, Some(key)));
        if Self::repeats(key) {
            let action = self.press(key);
            self.repeat_at = Some(now.saturating_add(450));
            action
        } else {
            None
        }
    }
    pub fn motion(&mut self, id: u32, key: Option<u16>) {
        let Some((owner, old)) = self.contact else {
            return;
        };
        if owner != id || old == key {
            return;
        }
        if old.is_some_and(Self::repeats) || key.is_some_and(Self::repeats) {
            self.cancel();
        } else {
            self.contact = Some((id, key));
        }
    }
    pub fn up(&mut self, id: u32) -> Option<Edit> {
        if !self.touching(id) {
            return None;
        }
        let key = self.contact.and_then(|(_, k)| k);
        self.cancel();
        key.filter(|k| !Self::repeats(*k))
            .and_then(|k| self.press(k))
    }
    pub fn tick(&mut self, now: u64) -> Option<Edit> {
        if self.repeat_at.is_some_and(|t| now >= t) {
            self.repeat_at = Some(now.saturating_add(125));
            self.contact
                .and_then(|(_, k)| k)
                .and_then(|k| self.press(k))
        } else {
            None
        }
    }
}
/// Small US-ASCII physical-key adapter for Wayland when the application owns its OSK.
/// It never activates the compositor's input method; international hardware layouts need an adapter.
pub fn ascii_key(name: &str, shift: bool, caps: bool) -> Option<String> {
    let mut chars = name.chars();
    let ch = chars.next()?;
    if chars.next().is_some() || !ch.is_ascii() || ch.is_ascii_control() {
        return None;
    }
    let output = if ch.is_ascii_alphabetic() {
        if shift ^ caps {
            ch.to_ascii_uppercase()
        } else {
            ch.to_ascii_lowercase()
        }
    } else if shift {
        match ch {
            '1' => '!',
            '2' => '@',
            '3' => '#',
            '4' => '$',
            '5' => '%',
            '6' => '^',
            '7' => '&',
            '8' => '*',
            '9' => '(',
            '0' => ')',
            '-' => '_',
            '=' => '+',
            '[' => '{',
            ']' => '}',
            '\\' => '|',
            ';' => ':',
            '\'' => '"',
            ',' => '<',
            '.' => '>',
            '/' => '?',
            '`' => '~',
            _ => ch,
        }
    } else {
        ch
    };
    Some(output.to_string())
}

/// Example application-side editor, independent of the keyboard. Bounded UTF-8, grapheme boundaries.
#[derive(Default)]
pub struct TextBuffer {
    text: String,
    cursor: usize,
    anchor: usize,
}
impl TextBuffer {
    /// Replace a remote/shared value while preserving a valid local caret.
    pub fn replace(&mut self, text: &str) {
        self.text = text.into();
        let limit = self.cursor.min(self.text.len());
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(self.text.len()))
            .take_while(|index| *index <= limit)
            .last()
            .unwrap_or(0);
        self.anchor = self.cursor;
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Each rendered run maps back to source byte boundaries. The local caret
    /// marker occupies glyph width, but maps both sides to the same text boundary.
    fn display_runs(&self, focused: bool) -> Vec<(usize, usize, &str)> {
        let n = self.text[..self.cursor].graphemes(true).count();
        let start = n.saturating_sub(36);
        let mut runs = Vec::new();
        let mut bytes = 0;
        for (index, g) in self.text.grapheme_indices(true).skip(start).take(38) {
            if focused && index == self.cursor {
                runs.push((index, index, "|"));
                bytes += 1;
            }
            if bytes + g.len() > 88 {
                break;
            }
            runs.push((index, index + g.len(), g));
            bytes += g.len();
        }
        if focused && self.cursor == self.text.len() {
            runs.push((self.cursor, self.cursor, "|"));
        }
        runs
    }
    pub fn display(&self, focused: bool) -> String {
        self.display_runs(focused)
            .into_iter()
            .map(|(_, _, text)| text)
            .collect()
    }
    /// Nearest grapheme boundary in the currently displayed run, using the same
    /// proportional font advances as the renderer, not a character-count estimate.
    pub fn hit_test(&self, x: f32, font: &super::font::Font, scale: f32, focused: bool) -> usize {
        let runs = self.display_runs(focused);
        let mut cursor = runs.first().map_or(0, |r| r.0);
        let mut distance = x.abs();
        let mut pen = 0.;
        for (_, end, text) in runs {
            pen += font.text_width(text, scale);
            let next = (x - pen).abs();
            if next < distance {
                distance = next;
                cursor = end;
            }
        }
        cursor
    }
    pub fn place_cursor(&mut self, byte: usize) -> bool {
        let cursor = self
            .boundaries()
            .into_iter()
            .take_while(|i| *i <= byte)
            .last()
            .unwrap_or(0);
        let changed = self.cursor != cursor || self.anchor != cursor;
        self.cursor = cursor;
        self.anchor = cursor;
        changed
    }
    fn boundaries(&self) -> Vec<usize> {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([self.text.len()])
            .collect()
    }
    pub fn edit(&mut self, edit: &Edit) -> bool {
        let old = (self.text.clone(), self.cursor, self.anchor);
        let start = self.cursor.min(self.anchor);
        let end = self.cursor.max(self.anchor);
        match edit {
            Edit::Commit(s)
                if !s.contains('\0')
                    && !s.chars().any(char::is_control)
                    && self.text.len() - (end - start) + s.len() <= 256 =>
            {
                self.text.replace_range(start..end, s);
                self.cursor = start + s.len();
                self.cursor = self
                    .boundaries()
                    .into_iter()
                    .find(|b| *b >= self.cursor)
                    .unwrap_or(self.text.len());
                self.anchor = self.cursor;
            }
            Edit::Backspace => {
                let from = if start != end {
                    start
                } else {
                    self.boundaries()
                        .into_iter()
                        .filter(|i| *i < start)
                        .next_back()
                        .unwrap_or(0)
                };
                self.text.replace_range(from..end, "");
                self.cursor = from;
                self.anchor = from;
            }
            Edit::Left { select } => {
                self.cursor = if !select && start != end {
                    start
                } else {
                    self.boundaries()
                        .into_iter()
                        .filter(|i| *i < self.cursor)
                        .next_back()
                        .unwrap_or(0)
                };
                if !select {
                    self.anchor = self.cursor;
                }
            }
            Edit::Right { select } => {
                self.cursor = if !select && start != end {
                    end
                } else {
                    self.boundaries()
                        .into_iter()
                        .find(|i| *i > self.cursor)
                        .unwrap_or(self.text.len())
                };
                if !select {
                    self.anchor = self.cursor;
                }
            }
            Edit::SelectAll => {
                self.anchor = 0;
                self.cursor = self.text.len();
            }
            _ => {}
        }
        old != (self.text.clone(), self.cursor, self.anchor)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gestures_slide_cancel_and_focus_do_not_retarget_output() {
        let mut k = Keyboard::default();
        k.focus();
        assert_eq!(k.down(1, 0, 0), None);
        k.down(2, 3, 0);
        k.motion(1, Some(1));
        assert_eq!(k.up(2), None);
        assert_eq!(k.up(1), Some(Edit::Commit("w".into())));
        k.down(1, 0, 0);
        k.focus();
        assert_eq!(k.up(1), None);
        k.down(1, 0, 0);
        k.motion(1, None);
        assert_eq!(k.up(1), None);
    }
    #[test]
    fn repeat_is_immediate_bounded_and_stops_on_slide_or_focus() {
        let mut k = Keyboard::default();
        k.focus();
        assert_eq!(k.down(1, 10, 0), Some(Edit::Backspace));
        assert_eq!(k.tick(449), None);
        assert_eq!(k.tick(5000), Some(Edit::Backspace));
        assert_eq!(k.tick(5000), None);
        k.motion(1, Some(0));
        assert_eq!(k.tick(9000), None);
        assert_eq!(k.up(1), None);
    }
    #[test]
    fn shift_tab_layouts_and_all_printable_ascii_are_literal() {
        let mut k = Keyboard::default();
        k.focus();
        k.press(22);
        assert_eq!(k.press(0), Some(Edit::Commit("Q".into())));
        assert_eq!(k.shift, Shift::Off);
        k.press(22);
        k.press(22);
        k.focus();
        assert_eq!(k.shift, Shift::Locked);
        assert_eq!(k.press(11), Some(Edit::Tab { backward: true }));
        let mut reachable = String::new();
        for page in 0..3 {
            k.page = page;
            for key in TEXT_KEYS {
                reachable.push_str(&k.label(key));
            }
        }
        for c in b'!'..=b'~' {
            if !(c as char).is_ascii_alphabetic() {
                assert!(reachable.contains(c as char), "missing {}", c as char);
            }
        }
    }
    #[test]
    fn physical_ascii_shift_mapping_does_not_depend_on_parallel_string_offsets() {
        for (input, expected) in [
            (";", ":"),
            ("'", "\""),
            ("\\", "|"),
            ("/", "?"),
            ("`", "~"),
            (" ", " "),
        ] {
            assert_eq!(ascii_key(input, true, false).as_deref(), Some(expected));
        }
        assert_eq!(ascii_key("A", false, false).as_deref(), Some("a"));
        assert_eq!(ascii_key("a", true, true).as_deref(), Some("a"));
        assert_eq!(ascii_key("Left", false, false), None);
    }
    #[test]
    fn application_editor_owns_unicode_selection_and_bounds() {
        let mut t = TextBuffer::default();
        t.edit(&Edit::Commit("aé👩‍💻".into()));
        t.edit(&Edit::Backspace);
        assert_eq!(t.text(), "aé");
        t.edit(&Edit::Left { select: true });
        t.edit(&Edit::Commit("X".into()));
        assert_eq!(t.text(), "aX");
        t.edit(&Edit::SelectAll);
        t.edit(&Edit::Backspace);
        assert_eq!(t.text(), "");
        assert!(!t.edit(&Edit::Commit("x".repeat(257))));
    }
}
