// SPDX-License-Identifier: LGPL-3.0-only
//! Skip Unicode property lookup only at proven ASCII grapheme boundaries.
//! CR/LF is the sole nonbreaking ASCII pair; mixed context stays with the pinned
//! Unicode segmenter. Neither source bytes nor cluster boundaries are changed.
use unicode_segmentation::UnicodeSegmentation;
fn ascii_break(a: u8, b: u8) -> bool {
    a.is_ascii() && b.is_ascii() && !(a == b'\r' && b == b'\n')
}
pub(super) fn valid_context(text: &str, max: usize) -> bool {
    if text.is_ascii() {
        return max >= 2 || (max == 1 && !text.contains("\r\n")) || text.is_empty();
    }
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut mixed = !bytes[0].is_ascii();
    for i in 1..bytes.len() {
        if ascii_break(bytes[i - 1], bytes[i]) {
            if mixed && text[start..i].graphemes(true).any(|g| g.len() > max) {
                return false;
            }
            start = i;
            mixed = false;
        }
        mixed |= !bytes[i].is_ascii();
    }
    !mixed || text[start..].graphemes(true).all(|g| g.len() <= max)
}
pub(super) struct Clusters<'a> {
    rest: &'a str,
    offset: usize,
}
impl<'a> Clusters<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            rest: text,
            offset: 0,
        }
    }
}
impl<'a> Iterator for Clusters<'a> {
    type Item = (usize, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.rest.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let length = if bytes[0].is_ascii() && (bytes.len() == 1 || ascii_break(bytes[0], bytes[1]))
        {
            1
        } else {
            self.rest.graphemes(true).next()?.len()
        };
        let (cluster, rest) = self.rest.split_at(length);
        let offset = self.offset;
        self.offset += length;
        self.rest = rest;
        Some((offset, cluster))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimized_boundaries_and_admission_agree_with_full_unicode_context() {
        let pieces = [
            "a", " ", "\r", "\n", "\t", "É", "\u{301}", "\u{600}", "👩", "\u{200d}", "💻", "🇨",
            "🇦", "क", "्", "ᄀ", "ᅡ", "ᆨ",
        ];
        let mut seed = 17_u32;
        for _ in 0..400 {
            let mut text = String::new();
            for _ in 0..500 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                text.push_str(pieces[seed as usize % pieces.len()]);
            }
            assert_eq!(
                Clusters::new(&text).collect::<Vec<_>>(),
                text.grapheme_indices(true).collect::<Vec<_>>()
            );
            for max in [8, 32, 256] {
                assert_eq!(
                    valid_context(&text, max),
                    text.graphemes(true).all(|g| g.len() <= max)
                );
            }
        }
        for text in [
            format!("a{}b", "\u{301}".repeat(200)),
            format!("{}ab", "\u{600}".repeat(200)),
        ] {
            assert!(!valid_context(&text, 256));
        }
    }
}
