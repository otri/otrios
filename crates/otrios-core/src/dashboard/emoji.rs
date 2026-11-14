// SPDX-License-Identifier: LGPL-3.0-only
//! Finite Unicode emoji sequence lookup and independently decoded monochrome masks.
//! The pinned build tool performs shaping; hosts allocate GPU pages only on use.
use ruzstd::decoding::StreamingDecoder;
use std::{collections::BTreeMap, io::Read, sync::OnceLock};

const INDEX: &[u8] = include_bytes!("../../../../web/shared/fonts/Emoji.index");
static RASTERS: OnceLock<Vec<u8>> = OnceLock::new();
pub const TILE_SIZE: usize = 64;
const TILE_BYTES: usize = TILE_SIZE * TILE_SIZE;
pub(super) struct Catalog {
    sequences: BTreeMap<&'static str, u32>,
    tiles: BTreeMap<u32, (usize, usize)>,
    max_sequence_by_first: BTreeMap<char, usize>,
}
impl Catalog {
    pub(super) fn shared() -> &'static Self {
        static CATALOG: OnceLock<Catalog> = OnceLock::new();
        CATALOG.get_or_init(|| Self::parse(INDEX).expect("Invalid built-in emoji resource"))
    }
    fn parse(bytes: &'static [u8]) -> Result<Self, String> {
        let u16_at = |i: usize| -> Result<u16, String> {
            Ok(u16::from_le_bytes(
                bytes
                    .get(i..i + 2)
                    .ok_or("Truncated emoji resource")?
                    .try_into()
                    .unwrap(),
            ))
        };
        let u32_at = |i: usize| -> Result<u32, String> {
            Ok(u32::from_le_bytes(
                bytes
                    .get(i..i + 4)
                    .ok_or("Truncated emoji resource")?
                    .try_into()
                    .unwrap(),
            ))
        };
        if bytes.get(..8) != Some(b"OTEMO002")
            || bytes.len() > 32 * 1024 * 1024
            || u32_at(8)? as usize > 32 * 1024 * 1024
        {
            return Err("Invalid emoji resource header".into());
        }
        let count = u32_at(12)? as usize;
        let glyph_count = u32_at(16)? as usize;
        let table_end = 24 + u32_at(20)? as usize;
        if count > 16384 || glyph_count > 8192 || table_end > bytes.len() {
            return Err("Emoji resource exceeds bounds".into());
        }
        let mut sequences = BTreeMap::new();
        let mut at = 24;
        let mut max_sequence_by_first = BTreeMap::new();
        for _ in 0..count {
            let len = u16_at(at)? as usize;
            let id = u32_at(at + 4)?;
            if len == 0 || len > 128 || u16_at(at + 2)? != 0 || id > 65535 {
                return Err("Invalid emoji sequence record".into());
            }
            at += 8;
            let text =
                std::str::from_utf8(bytes.get(at..at + len).ok_or("Truncated emoji sequence")?)
                    .map_err(|_| "Invalid emoji UTF-8")?;
            if text.contains('\u{fe0e}') || sequences.insert(text, id).is_some() {
                return Err("Invalid or duplicate emoji sequence".into());
            }
            let limit = max_sequence_by_first
                .entry(text.chars().next().unwrap())
                .or_insert(0);
            *limit = (*limit).max(len);
            at += len;
        }
        if at != table_end {
            return Err("Emoji sequence table length mismatch".into());
        }
        let payload_start = table_end + glyph_count * 24;
        let mut tiles = BTreeMap::new();
        let mut next_payload = payload_start;
        for _ in 0..glyph_count {
            let id = u32_at(at)?;
            let offset = u32_at(at + 4)? as usize;
            let length = u32_at(at + 8)? as usize;
            if id > 65535
                || offset != next_payload
                || length == 0
                || length > TILE_BYTES + 1024
                || u16_at(at + 12)? != 64
                || u16_at(at + 14)? != 64
                || u16_at(at + 16)? != 0
                || u16_at(at + 18)? != 0
                || u16_at(at + 20)? != 64
                || u16_at(at + 22)? != 0
            {
                return Err("Invalid emoji tile record".into());
            }
            if offset
                .checked_add(length)
                .is_none_or(|end| end > u32_at(8).unwrap() as usize)
            {
                return Err("Emoji tile range exceeds resource".into());
            }
            if tiles.insert(id, (offset, length)).is_some() {
                return Err("Duplicate emoji tile".into());
            }
            next_payload += length;
            at += 24;
        }
        if bytes.len() != payload_start
            || next_payload != u32_at(8)? as usize
            || sequences.values().any(|id| !tiles.contains_key(id))
        {
            return Err("Unreferenced or missing emoji tile".into());
        }
        Ok(Self {
            sequences,
            tiles,
            max_sequence_by_first,
        })
    }
    pub(super) fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.tiles.keys().copied()
    }
    pub(super) fn prefix(&self, value: &str) -> Option<(usize, u32)> {
        let first = value.chars().next()?;
        if first.is_ascii()
            && (!matches!(first, '#' | '*' | '0'..='9')
                || !value[1..].starts_with(['\u{fe0f}', '\u{20e3}']))
        {
            return None;
        }
        // Longest registered sequence wins; explicit text presentation suppresses
        // the preceding pictograph, including when that scalar has an emoji form.
        let limit = *self.max_sequence_by_first.get(&first)?;
        for end in (1..=value.len().min(limit)).rev() {
            if !value.is_char_boundary(end) || value[end..].starts_with('\u{fe0e}') {
                continue;
            }
            if let Some(id) = self.sequences.get(&value[..end]) {
                return Some((end, *id));
            }
        }
        None
    }
    pub(super) fn install(bytes: Vec<u8>) -> Result<(), String> {
        if bytes.len() > 32 * 1024 * 1024
            || !bytes.starts_with(INDEX)
            || super::font::sha256(&bytes) != super::emoji_reference::RESOURCE_SHA256
        {
            return Err("Emoji resource digest or index rejected".into());
        }
        let _ = RASTERS.set(bytes);
        Ok(())
    }
    pub fn raster(&self, id: u32) -> Result<Vec<u8>, String> {
        let (offset, length) = *self.tiles.get(&id).ok_or("Unknown emoji tile")?;
        let resource = RASTERS.get().ok_or("Emoji resource not loaded")?;
        let bytes = &resource[offset..offset + length];
        if super::font::check_zstd_header(bytes)? != TILE_BYTES {
            return Err("Invalid emoji decoded size".into());
        }
        let mut decoder = StreamingDecoder::new(bytes).map_err(|e| format!("Emoji zstd: {e}"))?;
        let mut rgba = Vec::with_capacity(TILE_BYTES);
        (&mut decoder)
            .take(TILE_BYTES as u64 + 1)
            .read_to_end(&mut rgba)
            .map_err(|e| format!("Emoji raster: {e}"))?;
        if rgba.len() != TILE_BYTES
            || !decoder.get_ref().is_empty()
            || !decoder.decoder.is_finished()
            || decoder.decoder.get_checksum_from_data() != decoder.decoder.get_calculated_checksum()
        {
            return Err("Invalid emoji raster frame".into());
        }
        Ok(rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_17_sequences_shape_measure_and_decode_without_missing_tiles() {
        Catalog::install(include_bytes!("../../../../web/shared/fonts/Emoji.otemo").to_vec())
            .unwrap();
        assert!(Catalog::install(vec![0; 32]).is_err());
        let catalog = Catalog::shared();
        let font = crate::dashboard::font::test_font();
        let corpus = include_str!("../../../../assets/fonts/unicode-emoji/emoji-test-17.0.txt");
        let mut fully_qualified = 0;
        for line in corpus
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (definition, _) = line.split_once('#').unwrap();
            let (codes, status) = definition.split_once(';').unwrap();
            if status.trim() != "fully-qualified" {
                continue;
            }
            let sequence: String = codes
                .split_whitespace()
                .map(|s| char::from_u32(u32::from_str_radix(s, 16).unwrap()).unwrap())
                .collect();
            assert_eq!(
                unicode_segmentation::UnicodeSegmentation::graphemes(sequence.as_str(), true)
                    .count(),
                1,
                "{sequence}"
            );
            let glyphs: Vec<_> = font.text_glyphs(&sequence).collect();
            assert_eq!(glyphs.len(), 1, "{sequence}");
            assert!(glyphs[0].emoji_id.is_some(), "{sequence}");
            assert_eq!(
                font.cluster_glyph_index(&sequence)
                    .map(|i| font.glyphs()[i].id),
                Some(glyphs[0].id)
            );
            assert_eq!(
                font.text_width(&sequence, 1.),
                f32::from(font.metrics().units_per_em) * font.metrics().scale(1.)
            );
            fully_qualified += 1;
        }
        assert_eq!(fully_qualified, 3944);
        for id in catalog.ids() {
            let rgba = catalog.raster(id).unwrap();
            assert_eq!(rgba.len(), TILE_BYTES);
            assert!(rgba.iter().any(|a| *a > 0), "Blank emoji {id}");
        }
        let id = |s| catalog.prefix(s).unwrap().1;
        assert_eq!(id("👍"), id("👍🏽"));
        assert_eq!(
            catalog.raster(id("👍")).unwrap(),
            catalog.raster(id("👍🏽")).unwrap()
        );
        assert_eq!(font.text_glyphs("A👩🏽‍💻🇨🇦1️⃣👨‍👩‍👧‍👦B").count(), 6);
        assert!(font.text_glyphs("❤\u{fe0e}").all(|g| g.emoji_id.is_none()));
        assert_eq!(font.text_width("🏽", 1.), 0.);
        assert!(catalog.raster(u32::MAX).is_err());
    }
}
