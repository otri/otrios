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
#[derive(Clone, Default)]
pub struct Keyboard {
    pub shift: Shift,
    pub page: u8,
    pub visible: bool,
    disabled: bool,
    contact: Option<(u32, Option<u16>)>,
    repeat_at: Option<u64>,
    // One latest key pulse survives a short down/up batch and renderer
    // backpressure. Its timeout starts only when a frame includes it.
    feedback: Option<(u16, Option<u64>)>,
}
impl Keyboard {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.disabled = !enabled;
        if !enabled {
            self.hide();
        }
    }
    pub fn cancel(&mut self) {
        self.contact = None;
        self.repeat_at = None;
        self.feedback = None;
    }
    pub fn focus(&mut self) {
        if self.disabled {
            return;
        }
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
    pub fn feedback_key(&self) -> Option<u16> {
        self.contact
            .and_then(|(_, key)| key)
            .or_else(|| self.feedback.map(|(key, _)| key))
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
        self.feedback = Some((key, None));
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
            self.feedback = key.map(|key| (key, None));
        }
    }
    pub fn up(&mut self, id: u32) -> Option<Edit> {
        if !self.touching(id) {
            return None;
        }
        let key = self.contact.and_then(|(_, k)| k);
        self.cancel();
        let edit = key
            .filter(|k| !Self::repeats(*k))
            .and_then(|k| self.press(k));
        if self.visible {
            self.feedback = key.map(|key| (key, None));
        }
        edit
    }
    pub fn tick(&mut self, now: u64) -> Option<Edit> {
        if self.contact.is_none()
            && let Some((key, until)) = self.feedback
        {
            self.feedback = match until {
                None => Some((key, None)),
                Some(until) if now < until => Some((key, Some(until))),
                Some(_) => None,
            };
        }
        if self.repeat_at.is_some_and(|t| now >= t) {
            self.repeat_at = Some(now.saturating_add(125));
            self.contact
                .and_then(|(_, k)| k)
                .and_then(|k| self.press(k))
        } else {
            None
        }
    }
    pub fn feedback_prepared(&mut self, now: u64) {
        if self.contact.is_none()
            && let Some((key, None)) = self.feedback
        {
            self.feedback = Some((key, Some(now.saturating_add(60))));
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
#[derive(Clone)]
pub struct TextBuffer {
    text: String,
    cursor: usize,
    anchor: usize,
    max_bytes: usize,
    multiline: bool,
}
impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            anchor: 0,
            max_bytes: 256,
            multiline: false,
        }
    }
}
impl TextBuffer {
    pub fn with_limit(max_bytes: usize) -> Self {
        Self {
            max_bytes: max_bytes.min(4096),
            ..Self::default()
        }
    }
    pub fn multiline(max_bytes: usize) -> Self {
        Self {
            multiline: true,
            ..Self::with_limit(max_bytes)
        }
    }
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
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn host_input(&self, id: u32, focused: bool) -> otrios_interfaces::keyboard::TextInput {
        otrios_interfaces::keyboard::TextInput {
            id,
            value: self.text.clone(),
            cursor: self.text[..self.cursor].encode_utf16().count() as u32,
            anchor: self.text[..self.anchor].encode_utf16().count() as u32,
            focused,
            multiline: self.multiline,
            secret: false,
            max_bytes: self.max_bytes,
            bounds: [0.; 4],
            inset: [0.; 4],
            font_size: 0.,
            line_height: 0.,
        }
    }
    /// Each rendered run maps back to source byte boundaries. The local caret
    /// marker occupies glyph width, but maps both sides to the same text boundary.
    fn display_runs(&self, focused: bool) -> Vec<(usize, usize, &str)> {
        let mut runs = Vec::new();
        for (index, g) in self.text.grapheme_indices(true) {
            if focused && index == self.cursor {
                runs.push((index, index, "|"));
            }
            runs.push((index, index + g.len(), g));
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
    pub fn caret_x(&self, font: &super::font::Font, scale: f32) -> f32 {
        font.text_width(&self.text[..self.cursor], scale)
    }
    /// A pixel window may begin inside a glyph; preserve its fractional offset
    /// and clip the ink rather than snapping the viewport to character boundaries.
    pub fn window(
        &self,
        offset: f32,
        width: f32,
        font: &super::font::Font,
        scale: f32,
        focused: bool,
    ) -> (String, f32) {
        let mut pen = 0.;
        let mut start = None;
        let mut value = String::new();
        for (_, _, run) in self.display_runs(focused) {
            let advance = font.text_width(run, scale);
            if pen + advance >= offset && pen <= offset + width {
                start.get_or_insert(pen);
                value.push_str(run);
            }
            pen += advance;
        }
        (value, start.unwrap_or(offset) - offset)
    }
    /// Nearest grapheme boundary in the complete text run, using the same
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
            Edit::Replace {
                text,
                cursor,
                anchor,
                selection_only,
            } => {
                if (*selection_only && text != &self.text)
                    || text.len() > self.max_bytes
                    || text
                        .chars()
                        .any(|c| c.is_control() && !(self.multiline && matches!(c, '\n' | '\t')))
                {
                    return false;
                }
                // Reject split surrogate pairs and out-of-range offsets. Snap
                // valid scalar boundaries back to complete graphemes.
                let byte_at = |offset: u32| {
                    let mut units = 0;
                    for (byte, c) in text.char_indices() {
                        if units == offset {
                            return Some(byte);
                        }
                        units += c.len_utf16() as u32;
                    }
                    (units == offset).then_some(text.len())
                };
                let (Some(cursor), Some(anchor)) = (byte_at(*cursor), byte_at(*anchor)) else {
                    return false;
                };
                let boundary = |byte| {
                    text.grapheme_indices(true)
                        .map(|(i, _)| i)
                        .chain([text.len()])
                        .take_while(|i| *i <= byte)
                        .last()
                        .unwrap_or(0)
                };
                self.cursor = boundary(cursor);
                self.anchor = boundary(anchor);
                self.text.clone_from(text);
            }
            Edit::Commit(s)
                if !s.contains('\0')
                    && !s.chars().any(|c| {
                        c.is_control() && !(self.multiline && matches!(c, '\n' | '\t'))
                    })
                    && self.text.len() - (end - start) + s.len() <= self.max_bytes =>
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
    #[test]
    fn browser_replacement_validates_utf16_limits_and_complete_graphemes_atomically() {
        let mut t = TextBuffer::multiline(24);
        let replacement = |text: &str, cursor, anchor| Edit::Replace {
            text: text.into(),
            cursor,
            anchor,
            selection_only: false,
        };
        assert!(t.edit(&replacement("a😀e\u{301}\n", 1, 5)));
        let host = t.host_input(1, true);
        assert_eq!((host.cursor, host.anchor), (1, 5));
        assert!(t.edit(&Edit::Commit("X".into())));
        assert_eq!(
            t.text(),
            "aX\n",
            "backward selection replaces the whole range"
        );
        for invalid in [
            replacement("😀", 1, 1),
            replacement("abc", 9, 9),
            replacement(&"é".repeat(13), 0, 0),
            replacement("a\0b", 0, 0),
        ] {
            assert!(!t.edit(&invalid));
            assert_eq!(t.text(), "aX\n");
        }
        assert!(t.edit(&replacement("e\u{301}", 1, 1)));
        assert_eq!(t.cursor(), 0, "scalar offsets snap to a grapheme boundary");
        let mut single = TextBuffer::default();
        assert!(!single.edit(&replacement("line\nbreak", 0, 0)));
        single.replace("Remote edit");
        assert!(!single.edit(&Edit::Replace {
            text: "Obsolete value".into(),
            cursor: 1,
            anchor: 0,
            selection_only: true
        }));
        assert_eq!(single.text(), "Remote edit");
    }
}
