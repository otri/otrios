// SPDX-License-Identifier: LGPL-3.0-only
//! Unpaged scalar oracle for small qualification fixtures. Deliberately keeps
//! all clusters/lines and searches whole prefixes; never used by the reader.
use otrios_core::dashboard::{font::Font, scene::Scene};
use otrios_interfaces::scene::{Primitive, Rect, SceneNode};
use unicode_segmentation::UnicodeSegmentation;
pub struct ReferenceLine {
    pub block: u32,
    pub number: u32,
    pub start: u32,
    pub end: u32,
    pub top: f64,
    pub runs: Vec<(f64, String)>,
}
pub struct Reference {
    pub lines: Vec<ReferenceLine>,
    pub height: f64,
    scale: u8,
    line_height: f64,
}
impl Reference {
    pub fn new(text: &str, font: &Font, scale: u8, width: f64) -> Self {
        assert!(
            text.len() <= 128 * 1024,
            "full-layout oracle is for small fixtures"
        );
        let factor = 8. * f64::from(scale) / f64::from(font.metrics().cap_height);
        let m = font.metrics();
        let line_height =
            (f64::from(m.ascent) - f64::from(m.descent) + f64::from(m.line_gap)) * factor;
        let tab = (f64::from(font.glyph(' ').advance) * factor * 4.).max(1.);
        let advance = |cluster: &str, x: f64| {
            if cluster == "\t" {
                ((x / tab).floor() + 1.) * tab - x
            } else {
                let mut chars = cluster.chars();
                let ch = chars.next().unwrap();
                f64::from(
                    font.glyph(if chars.next().is_some() {
                        '\u{fffd}'
                    } else {
                        ch
                    })
                    .advance,
                ) * factor
            }
        };
        let mut lines = Vec::new();
        let mut base = 0;
        let mut top = 0.;
        for (block, original) in text.split('\n').enumerate() {
            let paragraph = if base + original.len() < text.len() {
                original.strip_suffix('\r').unwrap_or(original)
            } else {
                original
            };
            let clusters: Vec<_> = paragraph.grapheme_indices(true).collect();
            let mut start = 0;
            let mut number = 0;
            loop {
                let mut x = 0.;
                let mut fit = start;
                // This oracle computes all fitting prefixes, then independently
                // chooses the last legal break; it has no streaming carry state.
                while fit < clusters.len() && fit - start < 4096 {
                    let a = advance(clusters[fit].1, x);
                    if fit != start && x + a > width {
                        break;
                    }
                    x += a;
                    fit += 1;
                }
                let end = if fit == clusters.len() || fit - start == 4096 {
                    fit
                } else {
                    (start..fit)
                        .rev()
                        .find(|&i| matches!(clusters[i].1, " " | "\t" | "-"))
                        .map_or(fit, |i| i + 1)
                };
                let source_start = if start < clusters.len() {
                    clusters[start].0
                } else {
                    paragraph.len()
                };
                let source_end = if end < clusters.len() {
                    clusters[end].0
                } else {
                    paragraph.len()
                };
                let mut runs = Vec::new();
                let mut run = String::new();
                let mut run_x = 0.;
                x = 0.;
                for &(_, cluster) in &clusters[start..end] {
                    let mut chars = cluster.chars();
                    let ch = chars.next().unwrap();
                    let scalar = if chars.next().is_some() {
                        '\u{fffd}'
                    } else {
                        ch
                    };
                    if cluster == "\t" || run.len() + scalar.len_utf8() > 257 {
                        if !run.is_empty() {
                            runs.push((run_x, std::mem::take(&mut run)));
                        }
                        run_x = x + if cluster == "\t" {
                            advance(cluster, x)
                        } else {
                            0.
                        };
                    }
                    if cluster != "\t" {
                        run.push(scalar);
                    }
                    x += advance(cluster, x);
                }
                if !run.is_empty() {
                    runs.push((run_x, run));
                }
                lines.push(ReferenceLine {
                    block: block as u32,
                    number,
                    start: (base + source_start) as u32,
                    end: (base + source_end) as u32,
                    top,
                    runs,
                });
                top += line_height;
                number += 1;
                if end == clusters.len() {
                    break;
                }
                start = end;
            }
            base += original.len() + 1;
        }
        Self {
            lines,
            height: top,
            scale,
            line_height,
        }
    }
    pub fn scene(&self, width: f32, height: f32, offset: f64) -> Scene {
        let mut children = Vec::new();
        let mut id = 2;
        for line in &self.lines {
            let y = line.top - offset;
            if y + self.line_height * 2. < 0. || y - self.line_height > f64::from(height) {
                continue;
            }
            for (x, value) in &line.runs {
                children.push(SceneNode {
                    id,
                    frame: Rect::new(*x as f32, y as f32, 8192., 16.),
                    primitive: Primitive::Text {
                        value: value.clone(),
                        scale: u32::from(self.scale),
                        color: [239, 244, 236, 255],
                    },
                    children: vec![],
                    responder: None,
                    semantics: None,
                });
                id += 1;
            }
        }
        Scene::new(SceneNode {
            id: 0,
            frame: Rect::new(0., 0., 1280., 800.),
            primitive: Primitive::Canvas { clip: true },
            children: vec![SceneNode {
                id: 1,
                frame: Rect::new(48., 100., width, height),
                primitive: Primitive::Canvas { clip: true },
                children,
                responder: None,
                semantics: None,
            }],
            responder: None,
            semantics: None,
        })
        .unwrap()
    }
}
