// SPDX-License-Identifier: LGPL-3.0-only
use super::super::{Result, hash};
use otrios_interfaces::canvas::TextResource;
use std::{ops::Range, sync::Arc};

pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 65_536;
pub const MAX_CLUSTER_BYTES: usize = 256;
pub const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;

/// Original bytes remain unchanged, including line endings. Block identities are
/// source-order identities for this immutable reader; editing is a later API.
#[derive(Clone)]
pub struct Source {
    pub(super) text: Arc<String>,
    pub(super) blocks: Arc<Vec<Range<u32>>>,
}
impl Source {
    pub fn admit(resource: &TextResource) -> Result<Self> {
        let size = resource.decoded_bytes as usize;
        if size > MAX_SOURCE_BYTES
            || resource.sha256.len() != 32
            || resource.lz4.is_empty()
            || resource.lz4.len() > super::super::MAX_BYTES
        {
            return Err("Document resource limits".into());
        }
        let mut decoded = vec![0; size];
        let length = lz4_flex::block::decompress_into(&resource.lz4, &mut decoded)
            .map_err(|e| format!("Document LZ4: {e}"))?;
        if length != size || hash(&decoded) != resource.sha256 {
            return Err("Document size or SHA-256 mismatch".into());
        }
        Self::new(String::from_utf8(decoded).map_err(|_| "Document UTF-8")?)
    }
    pub fn new(text: String) -> Result<Self> {
        if text.len() > MAX_SOURCE_BYTES || text.capacity() > MAX_SOURCE_BYTES {
            return Err("Document source byte limit".into());
        }
        // A cluster is never cut at a scheduling/chunk boundary. Bound context
        // admission instead of allowing an adversarial combining sequence to
        // turn one segmentation operation into an unbounded job.
        if !super::clusters::valid_context(&text, MAX_CLUSTER_BYTES) {
            return Err("Document grapheme context limit".into());
        }
        let mut blocks = Vec::new();
        let mut start = 0;
        for (at, b) in text.bytes().enumerate() {
            if b == b'\n' {
                if blocks.len() == MAX_BLOCKS - 1 {
                    return Err("Document block limit".into());
                }
                let end = if at > start && text.as_bytes()[at - 1] == b'\r' {
                    at - 1
                } else {
                    at
                };
                blocks.push(start as u32..end as u32);
                start = at + 1;
            }
        }
        blocks.push(start as u32..text.len() as u32);
        blocks.shrink_to_fit();
        Ok(Self {
            text: Arc::new(text),
            blocks: Arc::new(blocks),
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
    pub fn block(&self, id: u32) -> Option<&str> {
        let range = self.blocks.get(id as usize)?;
        Some(&self.text[range.start as usize..range.end as usize])
    }
    pub fn source_bytes(&self) -> usize {
        self.text.capacity()
    }
    pub(super) fn index_bytes(&self) -> usize {
        self.blocks.capacity() * size_of::<Range<u32>>()
    }
}

/// Authoring helper. Uses the same admission policy as runtime decoding; this
/// does not grant trust to unsigned resources or change the UI wire size cap.
pub fn encode_resource(text: &str) -> Result<TextResource> {
    if text.len() > MAX_SOURCE_BYTES {
        return Err("Document source byte limit".into());
    }
    Source::new(text.to_owned())?;
    let lz4 = lz4_flex::block::compress(text.as_bytes());
    if lz4.len() > super::super::MAX_BYTES {
        return Err("Compressed document exceeds UI resource bound".into());
    }
    Ok(TextResource {
        decoded_bytes: text.len() as u32,
        sha256: hash(text.as_bytes()),
        lz4,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_bound_decode_and_original_line_endings() {
        let text = "a\r\n\nÉ👩‍💻\n";
        let mut packet = encode_resource(text).unwrap();
        let source = Source::admit(&packet).unwrap();
        assert_eq!(source.text(), text);
        assert_eq!(
            (0..4).map(|i| source.block(i).unwrap()).collect::<Vec<_>>(),
            ["a", "", "É👩‍💻", ""]
        );
        packet.sha256[0] ^= 1;
        assert!(Source::admit(&packet).is_err());
        packet.decoded_bytes = (MAX_SOURCE_BYTES + 1) as u32;
        assert!(Source::admit(&packet).is_err());
        assert!(Source::new("\n".repeat(MAX_BLOCKS)).is_err());
        assert!(Source::new(format!("a{}", "\u{301}".repeat(128))).is_err());
    }
}
