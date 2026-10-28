// SPDX-License-Identifier: LGPL-3.0-only
use super::super::{Result, hash};
use otrios_interfaces::canvas::TextResource;
use std::{collections::BTreeSet, ops::Range, sync::Arc};

pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 65_536;
pub const MAX_CLUSTER_BYTES: usize = 256;
pub const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_WINDOW_BLOCKS: usize = 256;
pub const MAX_WINDOW_TEXT_BYTES: usize = 512 * 1024;
pub const MAX_INLINE_BYTES: usize = 128 * 1024;
pub const MAX_WINDOW_INLINE_BYTES: usize = 4 * 1024 * 1024;

/// Logical source identity, independent of selection order and layout width.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct BlockId {
    pub object: u64,
    pub part: u32,
}
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Key(pub BlockId, pub [u8; 32]);

/// Immutable inline triangles in block-local coordinates. Text and inline
/// geometry are packed into the same retained chunk and receive the same clip.
pub struct Inline {
    pub(super) vertices: Vec<f32>,
    pub(super) bounds: [f32; 4],
    revision: [u8; 32],
}
impl Inline {
    pub fn bytes(&self) -> usize {
        self.vertices.capacity() * 4 + size_of::<Self>()
    }
    pub fn new(mut vertices: Vec<f32>, offset: [f32; 2]) -> Result<Arc<Self>> {
        if vertices.is_empty()
            || !vertices.len().is_multiple_of(18)
            || vertices.capacity() * 4 > MAX_INLINE_BYTES
            || offset.iter().any(|v| !v.is_finite() || v.abs() > 8192.)
        {
            return Err("Document inline geometry limit".into());
        }
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        let mut bytes = b"OTriOS\0document-inline\0v1\0".to_vec();
        for vertex in vertices.chunks_exact_mut(6) {
            if vertex.iter().any(|v| !v.is_finite())
                || vertex[2..].iter().any(|v| !(0. ..=1.).contains(v))
            {
                return Err("Invalid document inline vertex".into());
            }
            for axis in 0..2 {
                vertex[axis] += offset[axis];
                if vertex[axis].abs() > 8192. {
                    return Err("Document inline coordinate limit".into());
                }
                bounds[axis] = bounds[axis].min(vertex[axis]);
                bounds[axis + 2] = bounds[axis + 2].max(vertex[axis]);
            }
            for value in vertex {
                bytes.extend(value.to_bits().to_le_bytes());
            }
        }
        vertices.shrink_to_fit();
        Ok(Arc::new(Self {
            vertices,
            bounds,
            revision: hash(&bytes).try_into().unwrap(),
        }))
    }
}

/// One immutable logical block, including its sent label/body and inline art.
/// Newlines are hard breaks inside the block, with local UTF-8 source positions.
pub struct Block {
    id: BlockId,
    revision: [u8; 32],
    text: String,
    pub(super) inset: f32,
    pub(super) inline: Option<Arc<Inline>>,
    anchor_to: Option<BlockId>,
}
impl Block {
    pub fn new(
        id: BlockId,
        text: String,
        inset: f32,
        inline: Option<Arc<Inline>>,
    ) -> Result<Arc<Self>> {
        Self::build(id, text, inset, inline, None)
    }
    /// Short prefix labels follow the next logical content block for reading
    /// anchors. Their local position never becomes the identity of a message.
    pub fn label(id: BlockId, text: String, inset: f32, target: BlockId) -> Result<Arc<Self>> {
        if text.len() > 128 || text.contains('\n') {
            return Err("Document prefix label limit".into());
        }
        Self::build(id, text, inset, None, Some(target))
    }
    fn build(
        id: BlockId,
        text: String,
        inset: f32,
        inline: Option<Arc<Inline>>,
        anchor_to: Option<BlockId>,
    ) -> Result<Arc<Self>> {
        if text.capacity() > 64 * 1024
            || !super::clusters::valid_context(&text, MAX_CLUSTER_BYTES)
            || text.contains('\r')
            || !inset.is_finite()
            || !(0. ..=512.).contains(&inset)
        {
            return Err("Document block admission limit".into());
        }
        let mut bytes = b"OTriOS\0document-block\0v1\0".to_vec();
        bytes.extend((text.len() as u32).to_le_bytes());
        bytes.extend(text.as_bytes());
        bytes.extend(inset.to_bits().to_le_bytes());
        bytes.push(u8::from(inline.is_some()));
        if let Some(inline) = &inline {
            bytes.extend(inline.revision);
        }
        bytes.push(u8::from(anchor_to.is_some()));
        if let Some(target) = anchor_to {
            bytes.extend(target.object.to_le_bytes());
            bytes.extend(target.part.to_le_bytes());
        }
        Ok(Arc::new(Self {
            id,
            revision: hash(&bytes).try_into().unwrap(),
            text,
            inset,
            inline,
            anchor_to,
        }))
    }
    pub fn id(&self) -> BlockId {
        self.id
    }
    pub fn revision(&self) -> [u8; 32] {
        self.revision
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Flat resources keep their original bytes and paragraph ranges. Structured
/// windows share independent immutable blocks; their positions are local to
/// each block and never require constructing a whole-document string.
#[derive(Clone)]
pub struct Source {
    pub(super) text: Arc<String>,
    pub(super) blocks: Arc<Vec<Range<u32>>>,
    structured: Option<Arc<Vec<Arc<Block>>>>,
}
impl Source {
    pub fn from_blocks(mut blocks: Vec<Arc<Block>>) -> Result<Self> {
        if blocks.is_empty() {
            blocks.push(Block::new(
                BlockId {
                    object: 0,
                    part: u32::MAX,
                },
                String::new(),
                0.,
                None,
            )?);
        }
        if blocks.len() > MAX_WINDOW_BLOCKS {
            return Err("Document window block limit".into());
        }
        let mut identities = BTreeSet::new();
        let mut inlines = BTreeSet::new();
        let mut text_bytes = 0;
        let mut inline_bytes = 0;
        for block in &blocks {
            if !identities.insert(block.id) {
                return Err("Duplicate document block identity".into());
            }
            text_bytes += block.text.capacity();
            // Allocation identity is only for memory accounting. Equal content
            // in two distinct allocations must count twice; revisions use SHA-256.
            if let Some(inline) = &block.inline
                && inlines.insert(Arc::as_ptr(inline) as usize)
            {
                inline_bytes += inline.vertices.capacity() * 4;
            }
        }
        for (i, block) in blocks.iter().enumerate() {
            if let Some(target) = block.anchor_to
                && !blocks
                    .iter()
                    .skip(i + 1)
                    .take(2)
                    .any(|b| b.id == target && b.anchor_to.is_none())
            {
                return Err("Document prefix must reference following content".into());
            }
        }
        if text_bytes > MAX_WINDOW_TEXT_BYTES || inline_bytes > MAX_WINDOW_INLINE_BYTES {
            return Err("Document window source budget".into());
        }
        let ranges = blocks
            .iter()
            .map(|block| 0..block.text.len() as u32)
            .collect();
        blocks.shrink_to_fit();
        Ok(Self {
            text: Arc::new(String::new()),
            blocks: Arc::new(ranges),
            structured: Some(Arc::new(blocks)),
        })
    }
    pub(super) fn structured(&self) -> bool {
        self.structured.is_some()
    }
    pub(super) fn key(&self, block: usize) -> Key {
        self.structured.as_ref().map_or(
            Key(
                BlockId {
                    object: block as u64,
                    part: u32::MAX,
                },
                [0; 32],
            ),
            |blocks| Key(blocks[block].id, blocks[block].revision),
        )
    }
    pub fn block_id(&self, block: u32) -> Option<BlockId> {
        ((block as usize) < self.block_count()).then(|| self.key(block as usize).0)
    }
    pub(super) fn inset(&self, block: usize) -> f32 {
        self.structured.as_ref().map_or(0., |b| b[block].inset)
    }
    pub(super) fn inline(&self, block: usize) -> Option<&Arc<Inline>> {
        self.structured.as_ref()?.get(block)?.inline.as_ref()
    }
    pub(super) fn anchor_target(&self, block: usize) -> Option<usize> {
        let blocks = self.structured.as_ref()?;
        let target = blocks[block].anchor_to?;
        blocks.iter().position(|b| b.id == target)
    }
    pub(super) fn ink_bounds(&self, font: [f64; 2]) -> [f64; 2] {
        (0..self.block_count())
            .filter_map(|i| self.inline(i))
            .fold(font, |bounds, inline| {
                [
                    bounds[0].min(f64::from(inline.bounds[1])),
                    bounds[1].max(f64::from(inline.bounds[3])),
                ]
            })
    }
    pub(super) fn slice(&self, block: usize, range: Range<u32>) -> &str {
        let text = self
            .structured
            .as_ref()
            .map_or(self.text.as_str(), |blocks| blocks[block].text.as_str());
        &text[range.start as usize..range.end as usize]
    }
    /// Append immutable paragraph blocks without touching existing source ranges.
    /// Each string is one block; callers split explicit newlines themselves.
    pub fn append_blocks(&mut self, blocks: &[String]) -> Result<()> {
        if self.structured.is_some() {
            return Err("Replace keyed document windows by identity".into());
        }
        let extra = blocks.iter().try_fold(0usize, |sum, line| {
            if line.contains(['\n', '\r'])
                || !super::clusters::valid_context(line, MAX_CLUSTER_BYTES)
            {
                return Err("Invalid appended paragraph".to_string());
            }
            sum.checked_add(line.len() + 1)
                .ok_or("Document size overflow".into())
        })?;
        if self.text.len() + extra > MAX_SOURCE_BYTES
            || self.blocks.len() + blocks.len() > MAX_BLOCKS
        {
            return Err("Appended document exceeds source limits".into());
        }
        let text = Arc::make_mut(&mut self.text);
        text.reserve_exact(extra);
        let ranges = Arc::make_mut(&mut self.blocks);
        ranges.reserve_exact(blocks.len());
        for line in blocks {
            text.push('\n');
            let start = text.len() as u32;
            text.push_str(line);
            ranges.push(start..text.len() as u32);
        }
        Ok(())
    }
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
            structured: None,
        })
    }
    pub fn original_text(&self) -> Option<&str> {
        self.structured.is_none().then_some(self.text.as_str())
    }
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
    pub fn block(&self, id: u32) -> Option<&str> {
        let range = self.blocks.get(id as usize)?;
        Some(self.slice(id as usize, range.clone()))
    }
    pub fn source_bytes(&self) -> usize {
        let Some(blocks) = &self.structured else {
            return self.text.capacity();
        };
        let mut inlines = BTreeSet::new();
        blocks
            .iter()
            .map(|b| {
                b.text.capacity()
                    + size_of::<Block>()
                    + b.inline.as_ref().map_or(0, |i| {
                        if inlines.insert(Arc::as_ptr(i) as usize) {
                            i.vertices.capacity() * 4 + size_of::<Inline>()
                        } else {
                            0
                        }
                    })
            })
            .sum()
    }
    pub(super) fn index_bytes(&self) -> usize {
        self.blocks.capacity() * size_of::<Range<u32>>()
            + self
                .structured
                .as_ref()
                .map_or(0, |b| b.capacity() * size_of::<Arc<Block>>())
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
    fn structured_sources_bound_allocations_validate_anchors_and_hash_content() {
        let id = BlockId { object: 1, part: 0 };
        let block = Block::new(id, "Header\nBody\n\n".into(), 24., None).unwrap();
        let same = Block::new(
            BlockId { object: 2, part: 0 },
            block.text().into(),
            24.,
            None,
        )
        .unwrap();
        assert_eq!(
            block.revision(),
            same.revision(),
            "content revision is separate from logical identity"
        );
        let changed = Block::new(id, block.text().into(), 25., None).unwrap();
        assert_ne!(block.revision(), changed.revision());
        assert!(Source::from_blocks(vec![block.clone(), block.clone()]).is_err());
        let label = Block::label(BlockId { object: 1, part: 1 }, "Today".into(), 24., id).unwrap();
        assert!(Source::from_blocks(vec![label.clone()]).is_err());
        let source = Source::from_blocks(vec![label, block]).unwrap();
        assert!(source.original_text().is_none());
        assert_eq!(source.anchor_target(0), Some(1));
        assert!(
            Source::from_blocks(
                (0..257)
                    .map(
                        |i| Block::new(BlockId { object: i, part: 0 }, String::new(), 0., None)
                            .unwrap()
                    )
                    .collect()
            )
            .is_err()
        );
        assert!(
            Source::from_blocks(
                (0..9)
                    .map(|i| Block::new(
                        BlockId { object: i, part: 0 },
                        "x".repeat(65536),
                        0.,
                        None
                    )
                    .unwrap())
                    .collect()
            )
            .is_err()
        );
        assert!(Block::new(id, "x".repeat(65537), 0., None).is_err());
        assert!(Block::new(id, "x".into(), f32::NAN, None).is_err());
        assert!(Inline::new(vec![f32::NAN; 18], [0.; 2]).is_err());
    }
    #[test]
    fn identity_bound_decode_and_original_line_endings() {
        let text = "a\r\n\nÉ👩‍💻\n";
        let mut packet = encode_resource(text).unwrap();
        let source = Source::admit(&packet).unwrap();
        assert_eq!(source.original_text(), Some(text));
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
