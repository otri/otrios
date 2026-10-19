// SPDX-License-Identifier: LGPL-3.0-only
//! Structural bounds before the FlexBuffers reader's unchecked scalar/key slices.
//! This validates offsets/spans, not application schemas or access policy.
use flexbuffers::FlexBufferType as Type;
type Result<T> = std::result::Result<T, String>;
struct Guard<'a> {
    data: &'a [u8],
    visits: usize,
    depth: usize,
    vector: usize,
}
impl Guard<'_> {
    fn span(&self, at: usize, length: usize) -> Result<&[u8]> {
        self.data
            .get(at..at.checked_add(length).ok_or("Flex span overflow")?)
            .ok_or("Flex span outside packet".into())
    }
    fn word(&self, at: usize, width: usize) -> Result<usize> {
        let mut bytes = [0; 8];
        bytes[..width].copy_from_slice(self.span(at, width)?);
        usize::try_from(u64::from_le_bytes(bytes)).map_err(|_| "Flex word overflow".into())
    }
    fn before(&self, at: usize, length: usize) -> Result<usize> {
        at.checked_sub(length)
            .ok_or("Flex prefix outside packet".into())
    }
    fn target(&self, slot: usize, width: usize) -> Result<usize> {
        self.before(slot, self.word(slot, width)?)
    }
    fn walk(&mut self, slot: usize, parent: usize, packed: u8, depth: usize) -> Result<()> {
        if depth > self.depth || self.visits == 0 {
            return Err("Flex depth/visit budget".into());
        }
        self.visits -= 1;
        self.span(slot, parent)?;
        let ty = Type::try_from(packed >> 2).map_err(|_| "Unknown Flex type")?;
        let width = 1usize << (packed & 3);
        if ty.is_inline() {
            if width > parent {
                return Err("Flex scalar wider than its slot".into());
            }
            self.span(slot, width)?;
            return Ok(());
        }
        let at = self.target(slot, parent)?;
        if ty.to_direct().is_some() {
            self.span(at, width)?;
            return Ok(());
        }
        if ty == Type::Key {
            let tail = self.data.get(at..).ok_or("Flex key outside packet")?;
            if !tail.contains(&0) {
                return Err("Unterminated Flex key".into());
            }
            return Ok(());
        }
        if ty == Type::String || ty == Type::Blob {
            let length = self.word(self.before(at, width)?, width)?;
            self.span(at, length)?;
            if ty == Type::String && self.span(at + length, 1)? != [0] {
                return Err("Unterminated Flex string".into());
            }
            return Ok(());
        }
        if !ty.is_vector() {
            return Err("Unsupported Flex reference".into());
        }
        let length = match ty.fixed_length_vector_length() {
            Some(n) => n,
            None => self.word(self.before(at, width)?, width)?,
        };
        if length > self.vector || (ty == Type::Map && length > 32) {
            return Err("Flex collection budget".into());
        }
        let bytes = length.checked_mul(width).ok_or("Flex vector overflow")?;
        self.span(at, bytes)?;
        let typed = ty.typed_vector_type();
        if typed.is_none() {
            self.span(at + bytes, length)?;
        }
        if ty == Type::Map {
            let key_slot = self.before(at, 3 * width)?;
            let keys = self.target(key_slot, width)?;
            let key_width = self.word(self.before(at, 2 * width)?, width)?;
            if ![1, 2, 4, 8].contains(&key_width) {
                return Err("Invalid Flex key width".into());
            }
            if self.word(self.before(keys, key_width)?, key_width)? != length {
                return Err("Flex map key/value length mismatch".into());
            }
            self.span(
                keys,
                length.checked_mul(key_width).ok_or("Flex keys overflow")?,
            )?;
            for i in 0..length {
                self.walk(
                    keys + i * key_width,
                    key_width,
                    (Type::Key as u8) << 2,
                    depth + 1,
                )?;
            }
        }
        for i in 0..length {
            let packed = typed.map_or_else(
                || self.data[at + bytes + i],
                |ty| ((ty as u8) << 2) | (packed & 3),
            );
            self.walk(at + i * width, width, packed, depth + 1)?;
        }
        Ok(())
    }
}
pub(crate) fn validate(data: &[u8], depth: usize, visits: usize, vector: usize) -> Result<()> {
    if data.len() < 3 {
        return Err("Truncated Flex root".into());
    }
    let end = data.len();
    let parent = usize::from(data[end - 1]);
    if ![1, 2, 4, 8].contains(&parent) {
        return Err("Invalid Flex root width".into());
    }
    let at = end.checked_sub(2 + parent).ok_or("Truncated Flex root")?;
    Guard {
        data,
        visits,
        depth,
        vector,
    }
    .walk(at, parent, data[end - 2], 0)
}

#[cfg(all(test, feature = "canvas"))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn scalar_span_is_checked_before_the_library_boolean_accessor() {
        // Bool with an eight-byte declared width inside a one-byte root slot.
        assert!(validate(&[1, ((Type::Bool as u8) << 2) | 3, 1], 32, 4096, 512).is_err());
    }
    #[test]
    fn seeded_malformed_corpus_never_reaches_an_unchecked_reader_span() {
        use otrios_interfaces::layout::Value;
        let corpus = flexbuffers::to_vec(BTreeMap::from([
            ("flag", Value::Boolean(true)),
            ("text", Value::Text("Typed resource 🦀".into())),
            ("points", Value::TimedSeries(vec![[0., 24.], [1., 78.]])),
            ("color", Value::Color([60, 80, 120, 255])),
            ("series", Value::Series((0..32).map(|n| n as f32).collect())),
        ]))
        .unwrap();
        validate(&corpus, 32, 4096, 512).unwrap();
        let check = |bytes: &[u8]| {
            if validate(bytes, 32, 4096, 512).is_ok() {
                let _ = flexbuffers::from_slice::<serde::de::IgnoredAny>(bytes);
            }
        };
        for end in 0..corpus.len() {
            check(&corpus[..end]);
        }
        let mut seed = 0x7c38a9d2u32;
        for _ in 0..4096 {
            let mut bytes = corpus.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let at = seed as usize % bytes.len();
                bytes[at] ^= (seed >> 24) as u8;
            }
            check(&bytes);
        }
    }
}
