// SPDX-License-Identifier: LGPL-3.0-only
//! Prototype typography: LTR, one glyph per extended grapheme, missing clusters
//! map once to glyph 0. Greedy breaks after ASCII spaces/tabs/hyphens; otherwise
//! break before the overflowing grapheme. No normalization or complex shaping.
use super::{Source, source::Key};
use crate::dashboard::font::Font;
use std::{collections::BTreeMap, sync::Arc};

const TOKENS_PER_PAGE: usize = 256;
const MEASURED_PAGES: usize = 128;
pub(super) const MAX_LINE_CLUSTERS: usize = 4096;
#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub start: u32,
    pub end: u32,
    pub glyph: u16,
    pub advance: f64,
    pub tab: bool,
    pub break_after: bool,
    pub hard_break: bool,
}
struct MeasuredPage {
    tokens: Box<[Token]>,
    used: u64,
}
pub(super) struct Measure {
    pub font: Arc<Font>,
    pages: BTreeMap<(Key, u32), MeasuredPage>,
    page_hint: Option<((Key, u32), usize)>,
    stamp: u64,
    fallback: u16,
    ascii: [u16; 128],
    pub factor: f64,
    pub line_height: f64,
    tab_width: f64,
    pub measured: u64,
    pub profiling: bool,
    pub measurement_ns: u64,
}
impl Measure {
    pub fn new(font: Arc<Font>, scale: f32) -> Self {
        let factor = 8. * f64::from(scale) / f64::from(font.metrics().cap_height);
        let fallback_id = font.glyph('\u{fffd}').id;
        let fallback = font
            .glyphs()
            .iter()
            .position(|g| g.id == fallback_id)
            .unwrap() as u16;
        let tab_width = (f64::from(font.glyph(' ').advance) * factor * 4.).max(1.);
        let ascii = std::array::from_fn(|i| {
            font.glyphs()
                .binary_search_by_key(&char::from(i as u8), |g| g.scalar)
                .map_or(fallback, |n| n as u16)
        });
        let metrics = font.metrics();
        let line_height = (f64::from(metrics.ascent) - f64::from(metrics.descent)
            + f64::from(metrics.line_gap))
            * factor;
        Self {
            font,
            pages: BTreeMap::new(),
            page_hint: None,
            stamp: 0,
            fallback,
            ascii,
            factor,
            line_height,
            tab_width,
            measured: 0,
            profiling: false,
            measurement_ns: 0,
        }
    }
    pub fn token(&mut self, source: &Source, block: usize, cursor: u32, end: u32) -> Token {
        self.stamp += 1;
        let identity = source.key(block);
        if let Some((key, index)) = self.page_hint
            && key.0 == identity
            && let Some(page) = self.pages.get_mut(&key)
            && let Some(token) = page
                .tokens
                .get(index)
                .copied()
                .filter(|t| t.start == cursor)
        {
            self.page_hint = Some((key, index + 1));
            page.used = self.stamp;
            return token;
        }
        if let Some((key, page)) = self.pages.range_mut(..=(identity, cursor)).next_back()
            && key.0 == identity
            && let Ok(i) = page.tokens.binary_search_by_key(&cursor, |t| t.start)
        {
            self.page_hint = Some((*key, i + 1));
            page.used = self.stamp;
            return page.tokens[i];
        }
        if self.pages.len() == MEASURED_PAGES {
            let key = *self.pages.iter().min_by_key(|(_, p)| p.used).unwrap().0;
            self.pages.remove(&key);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let start = self.profiling.then(std::time::Instant::now);
        let glyphs = self.font.glyphs();
        let tokens: Box<[_]> = super::clusters::Clusters::new(source.slice(block, cursor..end))
            .take(TOKENS_PER_PAGE)
            .map(|(at, cluster)| {
                let mut chars = cluster.chars();
                let first = chars.next().unwrap();
                let glyph = if cluster.len() == 1 {
                    self.ascii[cluster.as_bytes()[0] as usize]
                } else if chars.next().is_none() {
                    glyphs
                        .binary_search_by_key(&first, |g| g.scalar)
                        .map_or(self.fallback, |n| n as u16)
                } else {
                    self.fallback
                };
                Token {
                    start: cursor + at as u32,
                    end: cursor + (at + cluster.len()) as u32,
                    glyph,
                    advance: f64::from(glyphs[glyph as usize].advance) * self.factor,
                    tab: cluster == "\t",
                    break_after: matches!(cluster, " " | "\t" | "-"),
                    hard_break: cluster == "\n",
                }
            })
            .collect();
        let token = tokens[0];
        self.page_hint = Some(((identity, cursor), 1));
        self.measured += tokens.len() as u64;
        self.pages.insert(
            (identity, cursor),
            MeasuredPage {
                tokens,
                used: self.stamp,
            },
        );
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(start) = start {
            self.measurement_ns += start.elapsed().as_nanos() as u64;
        }
        token
    }
    fn advance(&self, token: Token, x: f64) -> f64 {
        if token.tab {
            ((x / self.tab_width).floor() + 1.) * self.tab_width - x
        } else {
            token.advance
        }
    }
    pub fn bytes(&self) -> usize {
        size_of::<Self>()
            + self
                .pages
                .values()
                .map(|p| {
                    p.tokens.len() * size_of::<Token>()
                        + size_of::<MeasuredPage>()
                        + size_of::<(Key, u32)>()
                        + 48
                })
                .sum::<usize>()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Placed {
    pub token: Token,
    pub x: f64,
}
pub(super) struct Line {
    pub start: u32,
    pub end: u32,
    pub glyphs: Vec<Placed>,
}
pub(super) struct Scan {
    start: u32,
    cursor: u32,
    x: f64,
    last_break: usize,
    glyphs: Vec<Placed>,
}
impl Scan {
    pub fn new(start: u32) -> Self {
        Self {
            start,
            cursor: start,
            x: 0.,
            last_break: 0,
            glyphs: Vec::new(),
        }
    }
    /// At most `quota` clusters per call. A line containing only zero-advance
    /// glyphs gets a declared safety break after 4096 whole clusters.
    pub fn step(
        &mut self,
        source: &Source,
        block: usize,
        end: u32,
        width: f64,
        measure: &mut Measure,
        quota: usize,
    ) -> Option<Line> {
        for _ in 0..quota {
            if self.cursor == end || self.glyphs.len() == MAX_LINE_CLUSTERS {
                return Some(self.finish(self.glyphs.len()));
            }
            let token = measure.token(source, block, self.cursor, end);
            if token.hard_break {
                let mut line = self.finish(self.glyphs.len());
                line.end = token.end;
                return Some(line);
            }
            let advance = measure.advance(token, self.x);
            if self.x + advance > width && !self.glyphs.is_empty() {
                let count = if self.last_break > 0 {
                    self.last_break
                } else {
                    self.glyphs.len()
                };
                return Some(self.finish(count));
            }
            self.glyphs.push(Placed { token, x: self.x });
            self.x += advance;
            self.cursor = token.end;
            if token.break_after {
                self.last_break = self.glyphs.len();
            }
        }
        None
    }
    fn finish(&mut self, count: usize) -> Line {
        self.glyphs.truncate(count);
        let end = self.glyphs.last().map_or(self.start, |g| g.token.end);
        Line {
            start: self.start,
            end,
            glyphs: std::mem::take(&mut self.glyphs),
        }
    }
    pub fn bytes(&self) -> usize {
        self.glyphs.capacity() * size_of::<Placed>()
    }
}
