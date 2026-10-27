// SPDX-License-Identifier: LGPL-3.0-only
use super::layout::Line;
use crate::dashboard::Mesh;
use crate::dashboard::font::Font;
use std::sync::Arc;

pub const TARGET_GLYPHS: usize = 1024;
pub const MAX_CHUNK_GLYPHS: usize = 2048;
pub(super) const PAGE_LINES: u32 = 4;
/// Source byte ranges refer to complete original graphemes, including a single
/// replacement marker for an unsupported multi-scalar grapheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterRange {
    pub start: u32,
    pub end: u32,
}
#[derive(Clone, Copy)]
struct Run {
    line: u32,
    first: u32,
    count: u32,
}
pub struct Chunk {
    /// Never reused within one reader, including after reflow or eviction.
    pub version: u64,
    pub mesh: Mesh,
    pub clusters: Vec<ClusterRange>,
    /// Conservative local ink bounds for rejecting an entire chunk before GPU
    /// admission, including additional clipping by the viewport's ancestors.
    pub bounds: [f32; 4],
    runs: Vec<Run>,
}
impl Chunk {
    pub fn bytes(&self) -> usize {
        self.mesh.glyph_vertices.capacity() * 4
            + self.mesh.draws.capacity() * 4
            + self.clusters.capacity() * size_of::<ClusterRange>()
            + self.runs.capacity() * size_of::<Run>()
            + size_of::<Self>()
            + 2 * size_of::<usize>() // Arc allocation control block
    }
    /// Complete lines outside the conservative font-ink window produce no draw.
    pub(super) fn range(&self, first_line: u32, end_line: u32) -> Option<(u32, u32)> {
        let first = self.runs.partition_point(|r| r.line < first_line);
        let end = self.runs.partition_point(|r| r.line < end_line);
        if first == end {
            return None;
        }
        let a = self.runs[first];
        let b = self.runs[end - 1];
        Some((a.first, b.first + b.count - a.first))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineRange {
    pub line: u32,
    pub source: ClusterRange,
}
pub(super) struct Page {
    pub chunks: Vec<Arc<Chunk>>,
    pub lines: Vec<LineRange>,
    pub used: u64,
}
struct Template {
    ink: [f32; 4],
    attributes: [f32; 8],
    drawable: bool,
}
/// Width-independent font geometry, shared by all chunks and paint colors.
pub(super) struct Templates {
    glyphs: Vec<Template>,
    em: f32,
    baseline: f32,
    line_height: f64,
}
impl Templates {
    pub fn new(font: &Font, scale: f32, line_height: f64) -> Self {
        let factor = font.metrics().scale(scale);
        let glyphs = font
            .glyphs()
            .iter()
            .map(|g| {
                let mut attributes = [0.; 8];
                attributes[..4].copy_from_slice(&g.band_transform);
                attributes[4..].copy_from_slice(&g.band_data());
                Template {
                    ink: g.ink_bounds.map(|b| b * factor),
                    attributes,
                    drawable: g.curve_count != 0,
                }
            })
            .collect();
        Self {
            glyphs,
            em: factor * f32::from(font.metrics().units_per_em),
            baseline: font.metrics().baseline_from_top(scale),
            line_height,
        }
    }
    pub fn bytes(&self) -> usize {
        size_of::<Self>() + self.glyphs.capacity() * size_of::<Template>()
    }
}
pub(super) struct Builder {
    pub chunks: Vec<Arc<Chunk>>,
    pub lines: Vec<LineRange>,
    current: Chunk,
    page_line: u32,
    color: [f32; 4],
}
impl Builder {
    pub fn new(page_line: u32, color: [u8; 4]) -> Self {
        Self {
            chunks: Vec::new(),
            lines: Vec::new(),
            current: Self::empty(),
            page_line,
            color: color.map(|v| f32::from(v) / 255.),
        }
    }
    fn empty() -> Chunk {
        Chunk {
            version: 0,
            mesh: Mesh::default(),
            clusters: Vec::new(),
            bounds: [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            runs: Vec::new(),
        }
    }
    pub fn begin_line(&mut self, line: u32, value: &Line) {
        self.lines.push(LineRange {
            line,
            source: ClusterRange {
                start: value.start,
                end: value.end,
            },
        });
    }
    /// Packing is resumable too: no more than 64 clusters on one owner turn.
    pub fn pack(
        &mut self,
        line: u32,
        value: &Line,
        cursor: &mut usize,
        templates: &Templates,
        version: &mut u64,
    ) -> bool {
        let end = (*cursor + 64).min(value.glyphs.len());
        if self.color[3] == 0. {
            *cursor = value.glyphs.len();
            return true;
        }
        let em = templates.em;
        let baseline =
            ((line - self.page_line) as f64 * templates.line_height) as f32 + templates.baseline;
        for placed in &value.glyphs[*cursor..end] {
            let token = placed.token;
            let g = &templates.glyphs[token.glyph as usize];
            if !g.drawable || token.tab {
                continue;
            }
            let pen = placed.x as f32;
            let [xmin, ymin, xmax, ymax] = g.ink;
            let left = pen + xmin - 1.;
            let right = pen + xmax + 1.;
            let top = baseline - ymax - 1.;
            let bottom = baseline - ymin + 1.;
            let bounds = &mut self.current.bounds;
            bounds[0] = bounds[0].min(left);
            bounds[1] = bounds[1].min(top);
            bounds[2] = bounds[2].max(right);
            bounds[3] = bounds[3].max(bottom);
            let first = (self.current.mesh.glyph_vertices.len() / 16) as u32;
            let mut quad = [0.; 64];
            for (corner, (x, y)) in [(left, top), (right, top), (left, bottom), (right, bottom)]
                .into_iter()
                .enumerate()
            {
                let v = &mut quad[corner * 16..(corner + 1) * 16];
                v[..4].copy_from_slice(&[x, y, (x - pen) / em, (baseline - y) / em]);
                v[4..8].copy_from_slice(&self.color);
                v[8..].copy_from_slice(&g.attributes);
            }
            // One checked reservation/copy per complete quad. The contiguous
            // float record is also suitable for compiler SIMD without changing
            // canonical line-breaking arithmetic or adding an unsafe boundary.
            self.current.mesh.glyph_vertices.extend_from_slice(&quad);
            self.current.clusters.push(ClusterRange {
                start: token.start,
                end: token.end,
            });
            if let Some(run) = self.current.runs.last_mut().filter(|r| r.line == line) {
                run.count += 4;
            } else {
                self.current.runs.push(Run {
                    line,
                    first,
                    count: 4,
                });
            }
            if self.current.clusters.len() == MAX_CHUNK_GLYPHS {
                self.flush(version);
            }
        }
        *cursor = end;
        let done = end == value.glyphs.len();
        if done && self.current.clusters.len() >= TARGET_GLYPHS {
            self.flush(version);
        }
        done
    }
    fn flush(&mut self, version: &mut u64) {
        if self.current.clusters.is_empty() {
            return;
        }
        let mut chunk = std::mem::replace(&mut self.current, Self::empty());
        *version += 1;
        chunk.version = *version;
        chunk
            .mesh
            .draws
            .extend([1, 0, (chunk.mesh.glyph_vertices.len() / 16) as u32]);
        // Vec growth can otherwise double capacity right at the chunk cap.
        chunk.mesh.glyph_vertices.shrink_to_fit();
        chunk.clusters.shrink_to_fit();
        chunk.runs.shrink_to_fit();
        self.chunks.push(Arc::new(chunk));
    }
    pub fn finish(mut self, version: &mut u64, used: u64) -> Page {
        self.flush(version);
        Page {
            chunks: self.chunks,
            lines: self.lines,
            used,
        }
    }
    pub fn glyphs(&self) -> usize {
        self.current.clusters.len() + self.chunks.iter().map(|c| c.clusters.len()).sum::<usize>()
    }
    pub fn bytes(&self) -> usize {
        self.current.bytes()
            + self.chunks.iter().map(|c| c.bytes()).sum::<usize>()
            + self.lines.capacity() * size_of::<LineRange>()
            + self.chunks.capacity() * size_of::<Arc<Chunk>>()
    }
}
