// SPDX-License-Identifier: LGPL-3.0-only
use super::ViewSnapshot;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use otrios_interfaces::{DASHBOARD_HEIGHT, DASHBOARD_WIDTH, ViewContent};

/// Triangle list: six f32 values per vertex (logical x/y, linear RGBA in 0..1).
/// Native GLES2 and browser WebGL consume this exact same geometry.
pub struct Mesh {
    pub vertices: Vec<f32>,
}
type Color = [u8; 4];
const BG: Color = [16, 28, 41, 255];
const CARD: Color = [27, 44, 61, 255];
const WHITE: Color = [239, 244, 236, 255];
const MUTED: Color = [150, 171, 185, 255];
const ACCENT: Color = [185, 238, 143, 255];

/// Small deterministic layout compositor. Hosts draw triangles on their GPU and route input.
/// Repaint on a new snapshot; this demo does not continuously redraw idle frames.
pub fn render(snapshot: &ViewSnapshot) -> Mesh {
    let mut f = Mesh {
        vertices: Vec::new(),
    };
    f.rect(0, 0, DASHBOARD_WIDTH, DASHBOARD_HEIGHT, BG);
    f.text(64, 25, "OTRIOS  /  LIVE SPACES", 2, MUTED);
    for node in &snapshot.view().nodes {
        let [x, y, w, h] = node.bounds;
        match &node.content {
            ViewContent::Label(s) => {
                let scale = if node.id == 1 {
                    6
                } else if node.id == 4 {
                    4
                } else {
                    2
                };
                if node.id == 4 {
                    f.rounded(x, y, w, h, CARD);
                }
                let inset = if node.id == 4 { 24 } else { 0 };
                f.text(
                    x + inset,
                    y + (h - 8 * scale) / 2,
                    s,
                    scale,
                    if node.id == 5 { MUTED } else { WHITE },
                );
            }
            ViewContent::Switch { label, enabled } => {
                f.rounded(x, y, w, h, if *enabled { ACCENT } else { CARD });
                let fg = if *enabled { BG } else { WHITE };
                f.text(x + 32, y + 32, label, 3, fg);
                f.text(x + 32, y + 88, if *enabled { "ON" } else { "OFF" }, 5, fg);
                f.rounded(
                    x + w - 164,
                    y + 84,
                    132,
                    64,
                    if *enabled {
                        [56, 84, 55, 255]
                    } else {
                        [74, 94, 113, 255]
                    },
                );
                f.circle(
                    (x + w - 132 + if *enabled { 64 } else { 0 }) as i32,
                    (y + 116) as i32,
                    24,
                    WHITE,
                );
                f.text(x + 32, y + h - 42, "TAP TO CHANGE", 2, fg);
            }
            ViewContent::Graph { label, samples } => {
                f.rounded(x, y, w, h, CARD);
                f.text(x + 32, y + 32, label, 3, WHITE);
                f.text(x + 32, y + 76, "DEMO DATA  /  LAST 32 EVENTS", 2, MUTED);
                let gx = x + 40;
                let gy = y + 136;
                let gw = w - 80;
                let gh = h - 184;
                for i in 0..=4 {
                    f.rect(gx, gy + i * gh / 4, gw, 1, [48, 68, 85, 255]);
                }
                let pts: Vec<_> = samples
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        (
                            (gx + (i as u32 * gw) / ((samples.len().max(2) - 1) as u32)) as i32,
                            (gy + gh - v * gh / 100) as i32,
                        )
                    })
                    .collect();
                for pair in pts.windows(2) {
                    for d in -1..=1 {
                        f.line(
                            (pair[0].0, pair[0].1 + d),
                            (pair[1].0, pair[1].1 + d),
                            ACCENT,
                        );
                    }
                }
                if let Some(&(px, py)) = pts.last() {
                    f.circle(px, py, 7, ACCENT);
                }
            }
        }
    }
    f
}
impl Mesh {
    fn vertex(&mut self, x: f32, y: f32, c: Color) {
        self.vertices.extend([
            x,
            y,
            c[0] as f32 / 255.,
            c[1] as f32 / 255.,
            c[2] as f32 / 255.,
            c[3] as f32 / 255.,
        ]);
    }
    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, c: Color) {
        let (x, y, r, b) = (
            x as f32,
            y as f32,
            (x + w).min(DASHBOARD_WIDTH) as f32,
            (y + h).min(DASHBOARD_HEIGHT) as f32,
        );
        if x >= r || y >= b {
            return;
        }
        for (px, py) in [(x, y), (r, y), (x, b), (x, b), (r, y), (r, b)] {
            self.vertex(px, py, c);
        }
    }
    fn circle(&mut self, x: i32, y: i32, r: i32, c: Color) {
        for i in 0..32 {
            let a = i as f32 * std::f32::consts::TAU / 32.;
            let b = (i + 1) as f32 * std::f32::consts::TAU / 32.;
            self.vertex(x as f32, y as f32, c);
            self.vertex(
                x as f32 + a.cos() * r as f32,
                y as f32 + a.sin() * r as f32,
                c,
            );
            self.vertex(
                x as f32 + b.cos() * r as f32,
                y as f32 + b.sin() * r as f32,
                c,
            );
        }
    }
    fn rounded(&mut self, x: u32, y: u32, w: u32, h: u32, c: Color) {
        let r = 16;
        self.rect(x + r, y, w - 2 * r, h, c);
        self.rect(x, y + r, w, h - 2 * r, c);
        for cx in [x + r, x + w - r - 1] {
            for cy in [y + r, y + h - r - 1] {
                self.circle(cx as i32, cy as i32, r as i32, c);
            }
        }
    }
    fn text(&mut self, x: u32, y: u32, s: &str, scale: u32, c: Color) {
        for (i, ch) in s.chars().enumerate() {
            let glyph = BASIC_FONTS
                .get(ch)
                .or_else(|| BASIC_FONTS.get('?'))
                .unwrap_or([0; 8]);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        self.rect(
                            x + (i as u32 * 9 + col) * scale,
                            y + row as u32 * scale,
                            scale,
                            scale,
                            c,
                        );
                    }
                }
            }
        }
    }
    fn line(&mut self, a: (i32, i32), b: (i32, i32), c: Color) {
        let dx = (b.0 - a.0) as f32;
        let dy = (b.1 - a.1) as f32;
        let len = dx.hypot(dy);
        if len == 0. {
            return;
        }
        let (nx, ny) = (-dy / len / 2., dx / len / 2.);
        let p = [
            (a.0 as f32 + nx, a.1 as f32 + ny),
            (b.0 as f32 + nx, b.1 as f32 + ny),
            (a.0 as f32 - nx, a.1 as f32 - ny),
            (b.0 as f32 - nx, b.1 as f32 - ny),
        ];
        for i in [0, 1, 2, 2, 1, 3] {
            self.vertex(p[i].0, p[i].1, c);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_snapshot_produces_same_mesh_and_touch_changes_it() {
        let mut r = crate::dashboard::Replica::new().unwrap();
        let a = render(r.snapshot());
        let b = render(&ViewSnapshot::from_bytes(r.snapshot().bytes()).unwrap());
        assert_eq!(a.vertices.len() % 18, 0);
        assert!(a.vertices.iter().all(|v| v.is_finite()));
        assert_eq!(a.vertices, b.vertices);
        r.touch(200, 300).unwrap();
        assert_ne!(a.vertices, render(r.snapshot()).vertices);
    }
}
