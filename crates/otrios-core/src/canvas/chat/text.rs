// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded editor/preview wrapping. One cached source map drives paint, hit tests
//! and effect anchors; moving the viewport never rewraps unchanged text.
use crate::dashboard::font::Font;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(super) struct Wrapped {
    text: String,
    width: f32,
    font: String,
    pub lines: Vec<Range<usize>>,
    pub line_height: f32,
}
impl Wrapped {
    pub fn update(&mut self, text: &str, width: f32, font: &Font) -> bool {
        if self.text == text && self.width == width && self.font == font.hash() {
            return false;
        }
        self.text = text.into();
        self.width = width;
        self.font = font.hash().into();
        self.line_height = font.metrics().line_height(2.).max(20.);
        self.lines.clear();
        let limit = (width - font.text_width("|", 2.)).max(1.);
        let mut start = 0;
        while start < text.len() {
            let mut end = start;
            let mut break_at = None;
            let mut x = 0.;
            let mut newline = None;
            for (byte, g) in text[start..].grapheme_indices(true) {
                let byte = start + byte;
                if matches!(g, "\n" | "\r" | "\r\n") {
                    end = byte;
                    newline = Some(byte + g.len());
                    break;
                }
                let advance = font.text_width(g, 2.);
                if end > start && (x + advance > limit || byte + g.len() - start > 256) {
                    end = break_at.unwrap_or(byte);
                    break;
                }
                x += advance;
                end = byte + g.len();
                if matches!(g, " " | "\t" | "-") {
                    break_at = Some(end);
                }
            }
            self.lines.push(start..end);
            start = newline.unwrap_or(end);
            if newline == Some(text.len()) {
                self.lines.push(start..start);
            }
        }
        if self.lines.is_empty() {
            self.lines.push(0..0);
        }
        true
    }
    pub fn line_at(&self, byte: usize) -> usize {
        self.lines
            .iter()
            .enumerate()
            .find(|(i, r)| {
                byte < r.end
                    || (byte == r.end && self.lines.get(i + 1).is_none_or(|n| n.start > byte))
            })
            .map_or(self.lines.len().saturating_sub(1), |(i, _)| i)
    }
    pub fn position(&self, byte: usize, font: &Font) -> [f32; 2] {
        let line = self.line_at(byte);
        let Some(range) = self.lines.get(line) else {
            return [0., 0.];
        };
        [
            font.text_width(
                &self.text[range.start..byte.clamp(range.start, range.end)],
                2.,
            ),
            line as f32 * self.line_height,
        ]
    }
    pub fn display(&self, line: usize, cursor: Option<usize>) -> String {
        let r = &self.lines[line];
        let mut value = self.text[r.clone()].to_owned();
        if let Some(byte) = cursor.filter(|b| self.line_at(*b) == line) {
            value.insert(byte.clamp(r.start, r.end) - r.start, '|');
        }
        value
    }
    pub fn hit_test(&self, point: [f32; 2], font: &Font) -> usize {
        let line = ((point[1] / self.line_height).floor().max(0.) as usize)
            .min(self.lines.len().saturating_sub(1));
        let Some(range) = self.lines.get(line) else {
            return 0;
        };
        let mut at = range.start;
        let mut x = 0.;
        for (byte, g) in self.text[range.clone()].grapheme_indices(true) {
            let advance = font.text_width(g, 2.);
            if point[0] < x + advance * 0.5 {
                break;
            }
            x += advance;
            at = range.start + byte + g.len();
        }
        at
    }
}

/// End of the replaced/inserted range, excluding a common unchanged suffix.
pub(super) fn changed_end(old: &str, new: &str) -> Option<usize> {
    if old == new {
        return None;
    }
    let prefix = old
        .graphemes(true)
        .zip(new.graphemes(true))
        .take_while(|(a, b)| a == b)
        .map(|(_, g)| g.len())
        .sum::<usize>();
    let suffix = old[prefix..]
        .graphemes(true)
        .rev()
        .zip(new[prefix..].graphemes(true).rev())
        .take_while(|(a, b)| a == b)
        .map(|(_, g)| g.len())
        .sum::<usize>();
    Some(new.len() - suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapping_preserves_source_boundaries_and_maps_edits_and_hits() {
        let font = crate::dashboard::font::test_font();
        let mut layout = Wrapped::default();
        let value = "A brass relay with a veryveryverylongword 👩‍💻\nSecond paragraph.\n";
        assert!(layout.update(value, 120., &font));
        assert!(!layout.update(value, 120., &font));
        assert!(layout.lines.len() > 3);
        assert_eq!(layout.lines.last().unwrap(), &(value.len()..value.len()));
        for (line, range) in layout.lines.iter().enumerate() {
            assert!(value.is_char_boundary(range.start) && value.is_char_boundary(range.end));
            assert!(font.text_width(&value[range.clone()], 2.) <= 120.);
            let p = layout.position(range.start, &font);
            assert_eq!(p[1], line as f32 * layout.line_height);
            assert_eq!(layout.hit_test([0., p[1]], &font), range.start);
        }
        assert_eq!(changed_end("abc TAIL", "aXc TAIL"), Some(2));
        assert_eq!(changed_end("abc TAIL", "ac TAIL"), Some(1));
        assert_eq!(changed_end("e", "e\u{301}"), Some(3));
        assert_eq!(changed_end(value, value), None);
    }
}
