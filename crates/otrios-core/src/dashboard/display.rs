// SPDX-License-Identifier: LGPL-3.0-only
//! Presentation-only surface rotation, uniform fitting and raw touch calibration.
//! Hit testing follows the active scene extent; viewport geometry is never CRDT state.
use super::Result;
use otrios_interfaces::scene::Rect;
/// Conservative, nonoverlapping texture-copy regions after a solid canvas clear.
/// Empty background tiles need no texture sampling. Compute only when a mesh
/// changes; geometry and responder coordinates remain independent of this cache.
pub fn cached_regions(mesh: &[f32], band: Rect, clear: [u8; 4]) -> Vec<Rect> {
    const COLS: usize = 40;
    const ROWS: usize = 100;
    let mut occupied = [[false; COLS]; ROWS];
    for triangle in mesh.chunks_exact(18) {
        if triangle
            .chunks_exact(6)
            .all(|v| (0..4).all(|c| (v[2 + c] * 255.).round() as u8 == clear[c]))
        {
            continue;
        }
        let x0 = triangle
            .chunks_exact(6)
            .map(|v| v[0])
            .fold(f32::INFINITY, f32::min)
            .max(band.x)
            .clamp(0., 1280.);
        let y0 = triangle
            .chunks_exact(6)
            .map(|v| v[1])
            .fold(f32::INFINITY, f32::min)
            .max(band.y)
            .clamp(0., 800.);
        let x1 = triangle
            .chunks_exact(6)
            .map(|v| v[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .min(band.x + band.width)
            .clamp(0., 1280.);
        let y1 = triangle
            .chunks_exact(6)
            .map(|v| v[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .min(band.y + band.height)
            .clamp(0., 800.);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        for row in occupied
            .iter_mut()
            .take((y1 / 8.).ceil() as usize)
            .skip((y0 / 8.).floor() as usize)
        {
            row[(x0 / 32.).floor() as usize..(x1 / 32.).ceil() as usize].fill(true);
        }
    }
    let mut regions: Vec<Rect> = Vec::new();
    let mut previous: std::collections::BTreeMap<(usize, usize), usize> =
        std::collections::BTreeMap::new();
    for (y, row) in occupied.iter().enumerate() {
        let mut current = std::collections::BTreeMap::new();
        let mut x = 0;
        while x < COLS {
            if !row[x] {
                x += 1;
                continue;
            }
            let begin = x;
            while x < COLS && row[x] {
                x += 1;
            }
            let index = if let Some(&index) = previous.get(&(begin, x)) {
                regions[index].height += 8.;
                index
            } else {
                regions.push(Rect::new(
                    (begin * 32) as f32,
                    (y * 8) as f32,
                    ((x - begin) * 32) as f32,
                    8.,
                ));
                regions.len() - 1
            };
            current.insert((begin, x), index);
        }
        previous = current;
    }
    regions
}
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    turns: u16,
    size: [u32; 2],
}
impl Default for Surface {
    fn default() -> Self {
        Self {
            turns: 0,
            size: [1280, 800],
        }
    }
}
impl Surface {
    pub fn new(degrees: u16) -> Result<Self> {
        Self::with_size(degrees, [1280, 800])
    }
    pub fn with_size(degrees: u16, size: [u32; 2]) -> Result<Self> {
        if size.iter().any(|v| !(1..=8192).contains(v)) {
            return Err("Invalid surface size".into());
        }
        if ![0, 90, 180, 270].contains(&degrees) {
            return Err("Surface rotation must be 0, 90, 180 or 270".into());
        }
        Ok(Self {
            turns: degrees / 90,
            size,
        })
    }
    pub fn degrees(self) -> u16 {
        self.turns * 90
    }
    pub fn extent(self) -> [u32; 2] {
        if self.turns.is_multiple_of(2) {
            self.size
        } else {
            [self.size[1], self.size[0]]
        }
    }
    pub fn project(self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [w, h] = self.size.map(|n| n as f32);
        match self.turns {
            1 => [h - y, x],
            2 => [w - x, h - y],
            3 => [y, w - x],
            _ => [x, y],
        }
    }
    pub fn unproject(self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [w, h] = self.size.map(|n| n as f32);
        match self.turns {
            1 => [y, h - x],
            2 => [w - x, h - y],
            3 => [w - y, x],
            _ => [x, y],
        }
    }
    /// Column-major GLES/WebGL mat3: logical vertices -> normalized device coordinates.
    pub fn projection(self) -> [f32; 9] {
        let [w, h] = self.extent();
        let w = w as f32;
        let h = h as f32;
        let a = self.project([0., 0.]);
        let x = self.project([1., 0.]);
        let y = self.project([0., 1.]);
        [
            2. * (x[0] - a[0]) / w,
            -2. * (x[1] - a[1]) / h,
            0.,
            2. * (y[0] - a[0]) / w,
            -2. * (y[1] - a[1]) / h,
            0.,
            2. * a[0] / w - 1.,
            1. - 2. * a[1] / h,
            1.,
        ]
    }
    fn fit(self, content: [f32; 2]) -> (f32, [f32; 2]) {
        let [w, h] = self.size.map(|n| n as f32);
        let scale = (w / content[0]).min(h / content[1]);
        (
            scale,
            [(w - content[0] * scale) / 2., (h - content[1] * scale) / 2.],
        )
    }
    /// Fit a legacy or responsive logical scene uniformly into the physical target.
    pub fn projection_for(self, content: [f32; 2]) -> [f32; 9] {
        let (scale, offset) = self.fit(content);
        let p = self.projection();
        [
            p[0] * scale,
            p[1] * scale,
            0.,
            p[3] * scale,
            p[4] * scale,
            0.,
            p[0] * offset[0] + p[3] * offset[1] + p[6],
            p[1] * offset[0] + p[4] * offset[1] + p[7],
            1.,
        ]
    }
    pub fn touch_for(
        self,
        matrix: Calibration,
        p: [f32; 2],
        content: [f32; 2],
    ) -> Option<[f32; 2]> {
        let p = self.touch(matrix, p)?;
        let (scale, offset) = self.fit(content);
        Some([(p[0] - offset[0]) / scale, (p[1] - offset[1]) / scale])
    }
    pub fn rect(self, r: Rect) -> Rect {
        let a = self.project([r.x, r.y]);
        let b = self.project([r.x + r.width, r.y + r.height]);
        Rect::new(
            a[0].min(b[0]),
            a[1].min(b[1]),
            (b[0] - a[0]).abs(),
            (b[1] - a[1]).abs(),
        )
    }
    /// Raw normalized evdev position -> calibrated physical surface -> logical.
    pub fn touch(self, matrix: Calibration, p: [f32; 2]) -> Option<[f32; 2]> {
        if p.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let m = matrix.0;
        let [w, h] = self.extent();
        Some(self.unproject([
            (m[0] * p[0] + m[1] * p[1] + m[2]) * w as f32,
            (m[3] * p[0] + m[4] * p[1] + m[5]) * h as f32,
        ]))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Calibration([f32; 6]);
impl Calibration {
    pub fn new(m: [f32; 6]) -> Result<Self> {
        if m.iter().any(|v| !v.is_finite() || v.abs() > 8.)
            || (m[0] * m[4] - m[1] * m[3]).abs() < 0.00001
        {
            return Err("Invalid touch calibration matrix".into());
        }
        Ok(Self(m))
    }
}

#[cfg(test)]
#[test]
fn portrait_and_legacy_projection_agree_with_calibrated_touch_at_every_rotation() {
    for degrees in [0, 90, 180, 270] {
        let surface = Surface::with_size(degrees, [800, 1280]).unwrap();
        for content in [[800., 1280.], [1280., 800.]] {
            let (scale, offset) = surface.fit(content);
            let projection = surface.projection_for(content);
            let [w, h] = surface.extent().map(|n| n as f32);
            for point in [[0., 0.], [content[0], content[1]], [217., 311.]] {
                let physical =
                    surface.project([point[0] * scale + offset[0], point[1] * scale + offset[1]]);
                let recovered = surface
                    .touch_for(
                        Calibration::new([1., 0., 0., 0., 1., 0.]).unwrap(),
                        [physical[0] / w, physical[1] / h],
                        content,
                    )
                    .unwrap();
                assert!(
                    (recovered[0] - point[0]).abs() < 0.001
                        && (recovered[1] - point[1]).abs() < 0.001
                );
                let ndc = [
                    projection[0] * point[0] + projection[3] * point[1] + projection[6],
                    projection[1] * point[0] + projection[4] * point[1] + projection[7],
                ];
                assert!(((ndc[0] + 1.) * w / 2. - physical[0]).abs() < 0.001);
                assert!(((1. - ndc[1]) * h / 2. - physical[1]).abs() < 0.001);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn triangle(mesh: &mut Vec<f32>, points: [[f32; 2]; 3], color: [u8; 4]) {
        for [x, y] in points {
            mesh.extend([
                x,
                y,
                color[0] as f32 / 255.,
                color[1] as f32 / 255.,
                color[2] as f32 / 255.,
                color[3] as f32 / 255.,
            ]);
        }
    }
    #[test]
    fn cached_clear_tiles_are_omitted_but_translucent_or_different_colors_are_kept() {
        let band = Rect::new(0., 0., 1280., 256.);
        let clear = [16, 28, 41, 255];
        let points = [[36., 18.], [60., 18.], [36., 26.]];
        let mut mesh = Vec::new();
        triangle(&mut mesh, [[0., 0.], [1280., 0.], [0., 800.]], clear);
        assert!(cached_regions(&mesh, band, clear).is_empty());
        triangle(&mut mesh, points, [16, 28, 41, 128]);
        assert_eq!(
            cached_regions(&mesh, band, clear),
            vec![Rect::new(32., 16., 32., 16.)]
        );
        mesh.clear();
        triangle(&mut mesh, points, [239, 244, 236, 255]);
        assert!(!cached_regions(&mesh, band, clear).is_empty());
    }
    #[test]
    fn cached_regions_cover_colored_raster_samples_without_overlapping_copies() {
        let band = Rect::new(0., 0., 1280., 256.);
        let clear = [16, 28, 41, 255];
        let mut mesh = Vec::new();
        let triangles = [
            [[12., 11.], [109., 17.], [53., 68.]],
            [[31., 70.], [120., 46.], [69., 129.]],
            [[1008., 201.], [1263., 220.], [1201., 255.]],
        ];
        for points in triangles {
            triangle(&mut mesh, points, [239, 244, 236, 255]);
        }
        triangle(
            &mut mesh,
            [[0., 500.], [1280., 500.], [0., 800.]],
            [185, 238, 143, 255],
        );
        let regions = cached_regions(&mesh, band, clear);
        for (i, a) in regions.iter().enumerate() {
            assert!(a.y >= 0. && a.y + a.height <= 256.);
            for b in regions.iter().skip(i + 1) {
                assert!(
                    a.x + a.width <= b.x
                        || b.x + b.width <= a.x
                        || a.y + a.height <= b.y
                        || b.y + b.height <= a.y
                );
            }
        }
        let inside = |r: &Rect, x: f32, y: f32| {
            x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
        };
        for y in (0..256).step_by(3) {
            for x in (0..1280).step_by(3) {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                for t in triangles {
                    let cross = |a: [f32; 2], b: [f32; 2]| {
                        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
                    };
                    let values = [cross(t[0], t[1]), cross(t[1], t[2]), cross(t[2], t[0])];
                    if values.iter().all(|v| *v >= 0.) || values.iter().all(|v| *v <= 0.) {
                        assert!(regions.iter().any(|r| inside(r, p[0], p[1])));
                    }
                }
            }
        }
        assert!(regions.iter().map(|r| r.width * r.height).sum::<f32>() < 1280. * 128.);
    }
    #[test]
    fn surface_vertices_regions_and_input_round_trip() {
        for rotation in [0, 90, 180, 270] {
            let s = Surface::new(rotation).unwrap();
            for p in [[0., 0.], [1280., 800.], [170., 321.]] {
                assert_eq!(s.unproject(s.project(p)), p);
                let m = s.projection();
                let q = s.project(p);
                let [w, h] = s.extent();
                assert!(
                    (m[0] * p[0] + m[3] * p[1] + m[6] - (2. * q[0] / w as f32 - 1.)).abs()
                        < 0.00001
                );
                assert!(
                    (m[1] * p[0] + m[4] * p[1] + m[7] - (1. - 2. * q[1] / h as f32)).abs()
                        < 0.00001
                );
            }
            let r = s.rect(Rect::new(0., 0., 1280., 256.));
            assert_eq!(r.width * r.height, 1280. * 256.);
        }
        assert_eq!(
            Surface::new(90)
                .unwrap()
                .rect(Rect::new(0., 0., 1280., 256.)),
            Rect::new(544., 0., 256., 1280.)
        );
        assert!(Surface::new(45).is_err());
    }
    #[test]
    fn accepted_raw_calibration_matches_the_existing_landscape_touch_points() {
        let s = Surface::new(90).unwrap();
        let m = Calibration::new([
            0.014180019,
            1.5722592,
            0.03019642,
            -1.4318337,
            0.010876711,
            0.98226434,
        ])
        .unwrap();
        for (raw, target) in [
            ([793., 437.], [128., 80.]),
            ([76., 444.], [1152., 80.]),
            ([73., 34.], [1152., 720.]),
            ([786., 33.], [128., 720.]),
            ([442., 235.], [640., 400.]),
        ] {
            let actual = s.touch(m, [raw[0] / 1280., raw[1] / 800.]).unwrap();
            assert!(
                (actual[0] - target[0]).abs() < 12. && (actual[1] - target[1]).abs() < 12.,
                "{actual:?}"
            );
        }
        assert!(Calibration::new([0.; 6]).is_err());
        assert!(s.touch(m, [f32::NAN, 0.]).is_none());
    }
}
