// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded read-only document layout. Source, width-independent measurement,
//! sparse wrapping checkpoints and retained GPU-ready pages have separate lives.
mod chunk;
mod clusters;
mod heights;
mod layout;
pub(crate) mod scene;
mod source;
#[cfg(test)]
mod tests;
mod viewport;
use super::Result;
use crate::dashboard::font::Font;
use chunk::{Builder, PAGE_LINES, Page, Templates};
pub use chunk::{Chunk, ClusterRange, LineRange, MAX_CHUNK_GLYPHS, TARGET_GLYPHS};
use heights::Heights;
use layout::{Line, Measure, Scan};
pub use source::{MAX_BLOCKS, MAX_CLUSTER_BYTES, MAX_SOURCE_BYTES, Source, encode_resource};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
};
pub(super) use viewport::Viewport;

/// Core's share of the 16 MiB document CPU cap. The other 8 MiB is reserved
/// for display-owned staging, worker transfers and temporary packing arrays.
pub const CPU_WORKING_BYTES: usize = 8 * 1024 * 1024;
pub const GPU_GEOMETRY_BYTES: usize = 8 * 1024 * 1024;
/// Conservative per-glyph budget covering either host (WebGL's cached stream
/// currently expands a quad to six vertices). Native actual buffers are smaller.
pub const GPU_BYTES_PER_GLYPH: usize = 512;
pub const UPLOAD_BYTES_PER_FRAME: usize = 512 * 1024;
const JOB_RESERVE: usize = 5 * 1024 * 1024;
const MAX_WINDOW_PAGES: usize = 128;
const MAX_RESIDENT_PAGES: usize = 256;

#[derive(Clone, Copy)]
struct Checkpoint {
    line: u32,
    byte: u32,
}
#[derive(Default)]
struct Summary {
    cursor: u32,
    lines: u32,
    exact: bool,
    checkpoints: Vec<Checkpoint>,
}
struct Job {
    block: usize,
    line: u32,
    cursor: u32,
    scan: Scan,
    target: Option<u32>,
    packing: Option<(Line, usize)>,
    builder: Option<Builder>,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Anchor {
    pub block: u32,
    pub source_byte: u32,
    pub viewport_y: f64,
}
#[derive(Clone)]
pub struct Draw {
    pub chunk: Arc<Chunk>,
    /// Same retained view contract as UI text: first,count,x,y,l,t,r,b.
    pub view: [f32; 8],
}
#[derive(Clone, Default)]
pub struct Frame {
    pub epoch: u64,
    pub draws: Vec<Draw>,
    pub loading: bool,
    pub em: f32,
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Stats {
    pub anchor: Option<Anchor>,
    /// Opt-in native timings; zero when disabled or on Wasm. Browser task
    /// durations are measured by the JS worker's monotonic clock.
    pub work_ns: u64,
    pub max_work_ns: u64,
    pub measurement_ns: u64,
    pub breaking_ns: u64,
    pub packing_ns: u64,
    pub source_bytes: usize,
    pub source_index_bytes: usize,
    pub working_bytes: usize,
    pub pinned_mesh_bytes: usize,
    pub resident_pages: usize,
    pub exact_blocks: usize,
    pub blocks: usize,
    pub measured_clusters: u64,
    pub broken_clusters: u64,
    pub packed_glyphs: u64,
    pub laid_out_lines: u64,
    pub evictions: u64,
    pub backpressure: u64,
    pub offset: f64,
    pub extent: f64,
    pub extent_exact: bool,
    pub active: bool,
    pub velocity: f64,
    pub loading: bool,
}

pub struct Reader {
    source: Source,
    measure: Measure,
    templates: Templates,
    scale: f32,
    color: [u8; 4],
    width: f64,
    height: f64,
    line_height: f64,
    ink_top: f64,
    ink_bottom: f64,
    heights: Heights,
    summaries: Vec<Summary>,
    pages: BTreeMap<(usize, u32), Page>,
    allocations: Vec<(Weak<Chunk>, usize)>,
    job: Option<Job>,
    offset: f64,
    direction: f64,
    anchor: Option<Anchor>,
    epoch: u64,
    version: u64,
    stamp: u64,
    background: usize,
    checkpoint_bytes: usize,
    job_reserve: usize,
    counters: Stats,
    profile: bool,
}
impl Reader {
    /// Extend only the source/index tail. Existing pages, glyph meshes and
    /// source anchors retain their identity across a chat append.
    pub fn append_blocks(&mut self, blocks: &[String]) -> Result<()> {
        let old = self.source.block_count();
        self.source.append_blocks(blocks)?;
        let average = f64::from(self.measure.font.glyph('n').advance) * self.measure.factor;
        for range in &self.source.blocks[old..] {
            self.heights.push(
                (((range.end - range.start) as f64 * average / self.width)
                    .ceil()
                    .max(1.))
                    * self.line_height,
            );
            self.summaries.push(Summary {
                cursor: range.start,
                ..Summary::default()
            });
        }
        self.check_index()
    }
    pub fn new(
        source: Source,
        font: Arc<Font>,
        scale: f32,
        color: [u8; 4],
        width: f64,
        height: f64,
    ) -> Result<Self> {
        if !scale.is_finite() || !(0.1..=32.).contains(&scale) {
            return Err("Document scale limit".into());
        }
        let metrics = font.metrics();
        let line_height = (f64::from(metrics.ascent) - f64::from(metrics.descent)
            + f64::from(metrics.line_gap))
            * 8.
            * f64::from(scale)
            / f64::from(metrics.cap_height);
        let measure = Measure::new(font, scale);
        let templates = Templates::new(&measure.font, scale, measure.line_height);
        let baseline = f64::from(metrics.ascent) * measure.factor;
        let ink_top = measure
            .font
            .glyphs()
            .iter()
            .filter(|g| g.curve_count > 0)
            .map(|g| baseline - f64::from(g.ink_bounds[3]) * measure.factor - 1.)
            .fold(0., f64::min);
        let ink_bottom = measure
            .font
            .glyphs()
            .iter()
            .filter(|g| g.curve_count > 0)
            .map(|g| baseline - f64::from(g.ink_bounds[1]) * measure.factor + 1.)
            .fold(line_height, f64::max);
        let mut this = Self {
            source,
            measure,
            templates,
            scale,
            color,
            width: 0.,
            height: 0.,
            line_height,
            ink_top,
            ink_bottom,
            heights: Heights::new(vec![1.]),
            summaries: Vec::new(),
            pages: BTreeMap::new(),
            allocations: Vec::new(),
            job: None,
            offset: 0.,
            direction: 1.,
            anchor: None,
            epoch: 0,
            version: 0,
            stamp: 0,
            background: 0,
            checkpoint_bytes: 0,
            job_reserve: JOB_RESERVE,
            counters: Stats::default(),
            profile: false,
        };
        this.resize(width, height)?;
        Ok(this)
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn set_profiling(&mut self, enabled: bool) {
        self.profile = enabled;
        self.measure.profiling = enabled;
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn extent(&self) -> (f64, bool) {
        (
            self.heights.total(),
            self.background == self.summaries.len(),
        )
    }
    pub fn line_height(&self) -> f64 {
        self.line_height
    }
    /// Bounded resident source maps, independent of retained GPU geometry. This
    /// does not lay out or materialize offscreen document content.
    pub fn resident_lines(&self) -> impl Iterator<Item = (u32, LineRange)> + '_ {
        self.pages
            .iter()
            .flat_map(|((block, _), page)| page.lines.iter().map(|line| (*block as u32, *line)))
    }
    pub fn visible_block_origins(&self) -> Vec<(u32, f64)> {
        let (start, _) = self.heights.locate(self.offset);
        let mut result = Vec::new();
        let mut y = self.heights.prefix(start);
        for i in start..self.summaries.len() {
            if y > self.offset + self.height {
                break;
            }
            result.push((i as u32, y - self.offset));
            y = self.heights.prefix(i + 1);
        }
        result
    }
    pub fn scroll_to(&mut self, offset: f64) {
        if offset.is_finite() {
            self.anchor = None;
            let next = offset.clamp(0., (self.heights.total() - self.height).max(0.));
            if next != self.offset {
                self.direction = (next - self.offset).signum();
            }
            self.offset = next;
        }
    }
    pub fn anchor(&self) -> Anchor {
        if let Some(anchor) = self.anchor {
            return anchor;
        }
        let (block, local) = self.heights.locate(self.offset);
        let line = (local / self.line_height).floor().max(0.) as u32;
        if let Some(page) = self.pages.get(&(block, line / PAGE_LINES * PAGE_LINES))
            && let Some(range) = page.lines.iter().find(|l| l.line == line)
        {
            return Anchor {
                block: block as u32,
                source_byte: range.source.start,
                viewport_y: f64::from(line) * self.line_height - local,
            };
        }
        let cp = self.checkpoint(block, line);
        Anchor {
            block: block as u32,
            source_byte: cp.byte,
            viewport_y: f64::from(cp.line) * self.line_height - local,
        }
    }
    pub fn jump_to(&mut self, anchor: Anchor) -> Result<()> {
        let range = self
            .source
            .blocks
            .get(anchor.block as usize)
            .ok_or("Document anchor block")?;
        if anchor.source_byte < range.start
            || anchor.source_byte > range.end
            || !self
                .source
                .text
                .is_char_boundary(anchor.source_byte as usize)
            || !anchor.viewport_y.is_finite()
            || anchor.viewport_y.abs() > 8192.
            || (anchor.source_byte < range.end
                && !self.source.text[range.start as usize..range.end as usize]
                    .grapheme_indices(true)
                    .any(|(i, _)| range.start + i as u32 == anchor.source_byte))
        {
            return Err("Document anchor must be a source cluster boundary".into());
        }
        self.job = None;
        self.offset = self.heights.prefix(anchor.block as usize);
        self.anchor = Some(anchor);
        Ok(())
    }
    pub fn resize(&mut self, width: f64, height: f64) -> Result<bool> {
        if [width, height]
            .iter()
            .any(|v| !v.is_finite() || !(1. ..=8192.).contains(v))
        {
            return Err("Document viewport limit".into());
        }
        // Bound a screen's geometry independently of document length. The
        // zero-advance safety case is handled by byte accounting/backpressure.
        let smallest = self
            .measure
            .font
            .glyphs()
            .iter()
            .filter(|g| g.curve_count > 0 && g.advance > 0.)
            .map(|g| f64::from(g.advance) * self.measure.factor)
            .fold(f64::INFINITY, f64::min);
        let screen_glyph_bound = ((width / smallest).ceil() + 1.)
            * (((height + self.ink_bottom - self.ink_top) / self.line_height).ceil() + 1.);
        if screen_glyph_bound * GPU_BYTES_PER_GLYPH as f64 > GPU_GEOMETRY_BYTES as f64 {
            return Err("Document viewport exceeds GPU geometry budget at this scale".into());
        }
        if ((height + self.ink_bottom - self.ink_top) / self.line_height).ceil() + 2.
            > MAX_WINDOW_PAGES as f64
        {
            return Err("Document viewport exceeds visible page bound at this scale".into());
        }
        let zero_ink = self
            .measure
            .font
            .glyphs()
            .iter()
            .any(|g| g.curve_count > 0 && g.advance == 0.);
        let line_glyphs = if zero_ink {
            4096.
        } else {
            (width / smallest).ceil() + 2.
        }
        .min(4096.);
        self.job_reserve = ((f64::from(PAGE_LINES) * line_glyphs * 320.) as usize + 512 * 1024)
            .clamp(1024 * 1024, JOB_RESERVE);
        self.height = height;
        if width == self.width {
            self.offset = self
                .offset
                .clamp(0., (self.heights.total() - self.height).max(0.));
            return Ok(false);
        }
        let anchor = (self.width != 0.).then(|| self.anchor());
        self.width = width;
        self.epoch += 1;
        self.job = None;
        self.pages.clear();
        self.background = 0;
        self.checkpoint_bytes = 0;
        self.counters.exact_blocks = 0;
        // Release the obsolete index before allocating its replacement. Held
        // display snapshots own only meshes, never old-width layout summaries.
        self.summaries = Vec::new();
        self.heights = Heights::new(vec![1.]);
        let average = f64::from(self.measure.font.glyph('n').advance) * self.measure.factor;
        let estimates = self
            .source
            .blocks
            .iter()
            .map(|r| {
                (((r.end - r.start) as f64 * average / width).ceil().max(1.)) * self.line_height
            })
            .collect();
        self.heights = Heights::new(estimates);
        self.summaries = self
            .source
            .blocks
            .iter()
            .map(|r| Summary {
                cursor: r.start,
                ..Summary::default()
            })
            .collect();
        self.offset = anchor.map_or(0., |a| self.heights.prefix(a.block as usize));
        self.anchor = anchor;
        self.check_index()?;
        Ok(true)
    }
    fn checkpoint(&self, block: usize, line: u32) -> Checkpoint {
        let s = &self.summaries[block];
        let i = s.checkpoints.partition_point(|p| p.line <= line);
        i.checked_sub(1).map_or(
            Checkpoint {
                line: 0,
                byte: self.source.blocks[block].start,
            },
            |i| s.checkpoints[i],
        )
    }
    fn requests(&self, prefetch: bool) -> Vec<(usize, u32)> {
        let start = ((if prefetch {
            self.offset - self.height
        } else {
            self.offset
        }) - self.ink_bottom)
            .max(0.);
        let end = (self.offset + self.height * if prefetch { 3. } else { 1. } - self.ink_top)
            .min(self.heights.total());
        let (mut block, mut local) = self.heights.locate(start);
        let mut y = start;
        let mut requests = Vec::new();
        while block < self.summaries.len() && y < end && requests.len() < MAX_WINDOW_PAGES {
            let s = &self.summaries[block];
            let height = self.heights.prefix(block + 1) - self.heights.prefix(block);
            let from = (local / self.line_height).floor().max(0.) as u32 / PAGE_LINES * PAGE_LINES;
            let to =
                (((local + end - y).min(height) / self.line_height).ceil() as u32).max(from + 1);
            for line in (from..to).step_by(PAGE_LINES as usize) {
                if (!s.exact || line < s.lines) && requests.len() < MAX_WINDOW_PAGES {
                    requests.push((block, line));
                }
            }
            y += height - local;
            local = 0.;
            block += 1;
        }
        if prefetch {
            requests.sort_by(|a, b| {
                let priority = |&(block, line): &(usize, u32)| {
                    let y = self.heights.prefix(block) + f64::from(line) * self.line_height;
                    let delta = if self.direction >= 0. {
                        y - self.offset - self.height
                    } else {
                        self.offset - y
                    };
                    (u8::from(delta < 0.), delta.abs())
                };
                let a = priority(a);
                let b = priority(b);
                a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1))
            });
        }
        requests
    }
    fn start_job(&self, block: usize, target: Option<u32>) -> Job {
        let s = &self.summaries[block];
        let neighbor = target
            .and_then(|line| line.checked_sub(PAGE_LINES))
            .and_then(|line| self.pages.get(&(block, line)))
            .and_then(|page| page.lines.last())
            .map(|line| Checkpoint {
                line: line.line + 1,
                byte: line.source.end,
            });
        let cp = if let Some(cp) = neighbor {
            cp
        } else if target.is_none_or(|line| line >= s.lines) {
            Checkpoint {
                line: s.lines,
                byte: s.cursor,
            }
        } else {
            self.checkpoint(block, target.unwrap())
        };
        Job {
            block,
            line: cp.line,
            cursor: cp.byte,
            scan: Scan::new(cp.byte),
            target,
            packing: None,
            builder: target.map(|line| Builder::new(line, self.color)),
        }
    }
    fn remember_line(&mut self, job: &Job, value: &Line) -> Result<()> {
        let s = &mut self.summaries[job.block];
        if !s.exact && s.lines == job.line && s.cursor == value.start {
            if job.line != 0 && job.line.is_multiple_of(32) {
                let old = s.checkpoints.capacity();
                s.checkpoints.push(Checkpoint {
                    line: job.line,
                    byte: value.start,
                });
                self.checkpoint_bytes += (s.checkpoints.capacity() - old) * size_of::<Checkpoint>();
            }
            s.lines += 1;
            s.cursor = value.end;
            s.exact = value.end == self.source.blocks[job.block].end;
            self.counters.exact_blocks += usize::from(s.exact);
            let old_height = self.heights.prefix(job.block + 1) - self.heights.prefix(job.block);
            let height = f64::from(s.lines) * self.line_height;
            self.heights.set(
                job.block,
                if s.exact {
                    height
                } else {
                    height.max(old_height)
                },
            );
            while self.background < self.summaries.len() && self.summaries[self.background].exact {
                self.background += 1;
            }
            self.check_index()?;
        }
        if let Some(a) = self.anchor
            && a.block as usize == job.block
            && ((value.start <= a.source_byte && a.source_byte < value.end)
                || (a.source_byte == value.end && value.end == self.source.blocks[job.block].end))
        {
            self.offset = (self.heights.prefix(job.block) + f64::from(job.line) * self.line_height
                - a.viewport_y)
                .max(0.);
            self.anchor = None;
        }
        Ok(())
    }
    /// `max_steps` is a deterministic operation cap for Wasm task turns. Native
    /// also enforces a 2 ms wall budget between 64-cluster breaking/packing steps.
    /// Return true when layout/geometry visible to a consumer may have changed.
    pub fn work(&mut self, max_steps: usize) -> Result<bool> {
        #[cfg(not(target_arch = "wasm32"))]
        let start = std::time::Instant::now();
        self.stamp += 1;
        self.allocations.retain(|(w, _)| w.strong_count() != 0);
        let (anchor_block, anchor_local) = self.heights.locate(self.offset);
        let resolving_anchor = self.anchor.is_some();
        let previous_offset = self.offset;
        let mut changed = false;
        // One bounded viewport plan per scheduling turn. Recomputing/sorting
        // the same window before every 64-cluster step dwarfs glyph packing on
        // Cortex-A7. Input/reflow coalesces at the next turn; epochs still cancel
        // immediately, and each turn retains its 2 ms/operation budget.
        let visible = self.requests(false);
        let prefetch = self.requests(true);
        let wanted: BTreeSet<_> = prefetch.iter().copied().collect();
        for _ in 0..max_steps {
            if !resolving_anchor {
                self.offset = self.heights.prefix(anchor_block) + anchor_local;
            }
            let missing = visible
                .iter()
                .copied()
                .chain(prefetch.iter().copied())
                .find(|key| {
                    !self.pages.contains_key(key)
                        && (!self.summaries[key.0].exact || key.1 < self.summaries[key.0].lines)
                });
            let anchor_target = self.anchor.map(|a| a.block as usize);
            let priority = anchor_target
                .map(|b| (b, None))
                .or_else(|| missing.map(|(b, l)| (b, Some(l))));
            if let Some((block, target)) = priority
                && self
                    .job
                    .as_ref()
                    .is_none_or(|j| j.block != block || j.target != target)
            {
                self.job = None;
                let urgent = anchor_target.is_some()
                    || target.is_some_and(|line| visible.contains(&(block, line)));
                self.evict_for_job(&wanted, &visible, urgent);
                if self.working_bytes() + self.job_reserve > CPU_WORKING_BYTES {
                    self.counters.backpressure += 1;
                    break;
                }
                let mut job = self.start_job(block, target);
                if let Some(a) = self.anchor {
                    let s = &self.summaries[block];
                    let cp = s
                        .checkpoints
                        .iter()
                        .rev()
                        .find(|p| p.byte <= a.source_byte)
                        .copied()
                        .unwrap_or(Checkpoint {
                            line: 0,
                            byte: self.source.blocks[block].start,
                        });
                    job.line = cp.line;
                    job.cursor = cp.byte;
                    job.scan = Scan::new(cp.byte);
                }
                self.job = Some(job);
            }
            if self.job.is_none() && self.background < self.summaries.len() {
                self.job = Some(self.start_job(self.background, None));
            }
            let Some(mut job) = self.job.take() else {
                break;
            };
            let end = self.source.blocks[job.block].end;
            let mut finished = false;
            #[cfg(not(target_arch = "wasm32"))]
            let phase_start = self.profile.then(std::time::Instant::now);
            #[cfg(not(target_arch = "wasm32"))]
            let was_packing = job.packing.is_some();
            if let Some((line, cursor)) = &mut job.packing {
                let before = job.builder.as_ref().unwrap().glyphs();
                if job.builder.as_mut().unwrap().pack(
                    job.line,
                    line,
                    cursor,
                    &self.templates,
                    &mut self.version,
                ) {
                    job.cursor = line.end;
                    job.line += 1;
                    job.packing = None;
                    if job.cursor == end || job.line == job.target.unwrap() + PAGE_LINES {
                        finished = true;
                    } else {
                        job.scan = Scan::new(job.cursor);
                    }
                }
                let after = job.builder.as_ref().unwrap().glyphs();
                self.counters.packed_glyphs += after.saturating_sub(before) as u64;
            } else if let Some(line) =
                job.scan
                    .step(&self.source, end, self.width, &mut self.measure, 64)
            {
                self.counters.laid_out_lines += 1;
                self.counters.broken_clusters += line.glyphs.len() as u64;
                self.remember_line(&job, &line)?;
                if job.target.is_some_and(|target| job.line >= target) {
                    job.builder.as_mut().unwrap().begin_line(job.line, &line);
                    job.packing = Some((line, 0));
                } else {
                    job.cursor = line.end;
                    job.line += 1;
                    finished = job.cursor == end;
                    job.scan = Scan::new(job.cursor);
                }
                changed |= resolving_anchor
                    || (job.block >= anchor_block
                        && self.heights.prefix(job.block) < self.offset + self.height);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(phase_start) = phase_start {
                let ns = phase_start.elapsed().as_nanos() as u64;
                if was_packing {
                    self.counters.packing_ns += ns;
                } else {
                    self.counters.breaking_ns += ns;
                }
            }
            if finished {
                if let Some(builder) = job.builder {
                    let page = builder.finish(&mut self.version, self.stamp);
                    if !page.lines.is_empty() {
                        for c in &page.chunks {
                            self.allocations.push((Arc::downgrade(c), c.bytes()));
                        }
                        self.pages.insert((job.block, job.target.unwrap()), page);
                    }
                }
                changed |= job.target.is_some() || self.background == self.summaries.len();
            } else {
                self.job = Some(job);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if start.elapsed() >= std::time::Duration::from_millis(2) {
                break;
            }
        }
        if !resolving_anchor {
            // Height discovery above the viewport must not move its text.
            // Explicit jump resolution already supplies a new anchored offset.
            self.offset = self.heights.prefix(anchor_block) + anchor_local;
        }
        self.offset = self
            .offset
            .clamp(0., (self.heights.total() - self.height).max(0.));
        #[cfg(not(target_arch = "wasm32"))]
        if self.profile {
            let ns = start.elapsed().as_nanos() as u64;
            self.counters.work_ns += ns;
            self.counters.max_work_ns = self.counters.max_work_ns.max(ns);
        }
        Ok(changed || previous_offset != self.offset)
    }
    fn evict_for_job(
        &mut self,
        wanted: &BTreeSet<(usize, u32)>,
        visible: &[(usize, u32)],
        urgent: bool,
    ) {
        while self.working_bytes() + self.job_reserve > CPU_WORKING_BYTES
            || self.pages.len() >= MAX_RESIDENT_PAGES
        {
            let key = self
                .pages
                .iter()
                .filter(|(key, _)| !wanted.contains(key) || (urgent && !visible.contains(key)))
                .min_by_key(|(key, p)| (u8::from(wanted.contains(key)), p.used))
                .map(|(k, _)| *k);
            let Some(key) = key else {
                break;
            };
            self.pages.remove(&key);
            self.counters.evictions += 1;
            self.allocations.retain(|(w, _)| w.strong_count() != 0);
        }
    }
    fn index_bytes(&self) -> usize {
        self.source.index_bytes()
            + self.heights.bytes()
            + self.summaries.capacity() * size_of::<Summary>()
            + self.checkpoint_bytes
    }
    fn check_index(&self) -> Result<()> {
        if self.index_bytes() > source::MAX_INDEX_BYTES {
            Err("Document source/index metadata limit".into())
        } else {
            Ok(())
        }
    }
    fn working_bytes(&self) -> usize {
        self.measure.bytes()
            + self.templates.bytes()
            + self
                .allocations
                .iter()
                .filter(|(w, _)| w.strong_count() != 0)
                .map(|(_, n)| n)
                .sum::<usize>()
            + self
                .pages
                .values()
                .map(|p| {
                    p.lines.capacity() * size_of::<LineRange>()
                        + p.chunks.capacity() * size_of::<Arc<Chunk>>()
                        + 96
                })
                .sum::<usize>()
            + self.job.as_ref().map_or(0, |j| {
                j.scan.bytes()
                    + j.packing.as_ref().map_or(0, |(l, _)| {
                        l.glyphs.capacity() * size_of::<layout::Placed>()
                    })
                    + j.builder.as_ref().map_or(0, Builder::bytes)
            })
            + self.allocations.capacity() * size_of::<(Weak<Chunk>, usize)>()
    }
    pub fn frame(&mut self) -> Frame {
        let mut frame = Frame {
            epoch: self.epoch,
            draws: Vec::new(),
            loading: self.anchor.is_some(),
            em: self.measure.font.metrics().cache_em(self.scale),
        };
        for key @ (block, page_line) in self.requests(false) {
            let Some(page) = self.pages.get_mut(&key) else {
                frame.loading = true;
                continue;
            };
            page.used = self.stamp;
            let local = self.offset - self.heights.prefix(block);
            let first_line = ((local - self.ink_bottom) / self.line_height)
                .ceil()
                .max(0.) as u32;
            let end_line = ((local + self.height - self.ink_top) / self.line_height)
                .ceil()
                .max(0.) as u32;
            let y = (self.heights.prefix(block) - self.offset
                + f64::from(page_line) * self.line_height) as f32;
            for chunk in &page.chunks {
                if let Some((first, count)) = chunk.range(first_line, end_line) {
                    frame.draws.push(Draw {
                        chunk: chunk.clone(),
                        view: [
                            first as f32,
                            count as f32,
                            0.,
                            y,
                            0.,
                            0.,
                            self.width as f32,
                            self.height as f32,
                        ],
                    });
                }
            }
        }
        frame
    }
    pub fn stats(&self) -> Stats {
        let mut stats = self.counters.clone();
        stats.anchor = Some(self.anchor());
        stats.measurement_ns = self.measure.measurement_ns;
        stats.breaking_ns = stats.breaking_ns.saturating_sub(stats.measurement_ns);
        stats.source_bytes = self.source.source_bytes();
        stats.source_index_bytes = self.index_bytes();
        stats.working_bytes = self.working_bytes();
        let resident: BTreeSet<_> = self
            .pages
            .values()
            .flat_map(|p| p.chunks.iter().map(|c| c.version))
            .collect();
        stats.pinned_mesh_bytes = self
            .allocations
            .iter()
            .filter_map(|(w, n)| {
                let chunk = w.upgrade()?;
                // Discount this temporary strong reference and the page cache's
                // ownership, if present. Evicted chunks held only by a consumer
                // remain pinned and count against the same working-set cap.
                (Arc::strong_count(&chunk) > 1 + usize::from(resident.contains(&chunk.version)))
                    .then_some(*n)
            })
            .sum();
        stats.resident_pages = self.pages.len();
        stats.blocks = self.summaries.len();
        stats.measured_clusters = self.measure.measured;
        stats.offset = self.offset;
        (stats.extent, stats.extent_exact) = self.extent();
        stats.loading = self.anchor.is_some()
            || self
                .requests(false)
                .iter()
                .any(|k| !self.pages.contains_key(k));
        stats
    }
}
use unicode_segmentation::UnicodeSegmentation;
