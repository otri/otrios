// SPDX-License-Identifier: Apache-2.0
//! The only unsafe Rust module in the project. Invariants: the opaque C owner
//! stays on its creating thread; calls borrow slices synchronously; Drop runs
//! once after the final borrow. C includes the platform ABI headers. No raw
//! pointer, callback, or ownership transfer is exposed to callers (RFC 0001).
use crate::validate_mesh;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Flip {
    pub usec: u64,
    pub sequence: u32,
    pub crtc: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub stride: u32,
    pub red: i32,
    pub green: i32,
    pub blue: i32,
    pub alpha: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Touch {
    pub phase: i32,
    pub x: f32,
    pub y: f32,
}
unsafe extern "C" {
    fn ot_open(
        card: *const c_char,
        input: *const c_char,
        width: u32,
        height: u32,
        rgb565: c_int,
        headless: c_int,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn ot_close(d: *mut c_void);
    fn ot_error(d: *mut c_void) -> *const c_char;
    fn ot_get_info(d: *mut c_void, info: *mut Info);
    fn ot_upload(d: *mut c_void, slot: u32, vertices: *const f32, floats: usize) -> c_int;
    fn ot_draw(d: *mut c_void, projection: *const f32, animated: c_int) -> c_int;
    fn ot_set_font(
        d: *mut c_void,
        cw: u32,
        ch: u32,
        curves: *const f32,
        cf: usize,
        bw: u32,
        bh: u32,
        bands: *const u8,
        bb: usize,
    ) -> c_int;
    fn ot_upload_text(
        d: *mut c_void,
        slot: u32,
        vertices: *const f32,
        floats: usize,
        draws: *const u32,
        draw_words: usize,
    ) -> c_int;
    fn ot_linear_scene(d: *mut c_void) -> c_int;
    fn ot_text_stats(d: *mut c_void, stats: *mut u64);
    fn ot_present(d: *mut c_void, flip: *mut Flip) -> c_int;
    fn ot_input(d: *mut c_void, events: *mut Touch, capacity: usize) -> c_int;
    fn ot_pixels(d: *mut c_void, rgba: *mut u8, length: usize) -> c_int;
}
pub struct Display {
    owner: NonNull<c_void>,
    _thread: PhantomData<Rc<()>>,
}
impl Display {
    pub fn open(card: &str, input: &str, size: [u32; 2], rgb565: bool) -> Result<Self, String> {
        Self::create(card, input, size, rgb565, false)
    }
    /// Mesa-only pbuffer for shader/pixel acceptance without DRM ownership.
    pub fn headless(size: [u32; 2], rgb565: bool) -> Result<Self, String> {
        Self::create("", "", size, rgb565, true)
    }
    fn create(
        card: &str,
        input: &str,
        [w, h]: [u32; 2],
        rgb565: bool,
        headless: bool,
    ) -> Result<Self, String> {
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return Err("Invalid surface dimensions".into());
        }
        let card = CString::new(card).map_err(|e| e.to_string())?;
        let input = CString::new(input).map_err(|e| e.to_string())?;
        let mut error = [0 as c_char; 512];
        // SAFETY: C borrows valid strings/error buffer synchronously and returns
        // either NULL or sole ownership of a fully initialized opaque object.
        let pointer = unsafe {
            ot_open(
                card.as_ptr(),
                input.as_ptr(),
                w,
                h,
                rgb565.into(),
                headless.into(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let owner = NonNull::new(pointer).ok_or_else(|| {
            unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        })?;
        Ok(Self {
            owner,
            _thread: PhantomData,
        })
    }
    fn result(&self, status: i32) -> Result<(), String> {
        if status < 0 {
            // SAFETY: owner is live; adapter returns a NUL-terminated internal buffer.
            Err(unsafe { CStr::from_ptr(ot_error(self.owner.as_ptr())) }
                .to_string_lossy()
                .into_owned())
        } else {
            Ok(())
        }
    }
    pub fn info(&self) -> Info {
        let mut info = Info::default();
        // SAFETY: live owner, correctly sized initialized output, no retention.
        unsafe { ot_get_info(self.owner.as_ptr(), &mut info) };
        info
    }
    pub fn upload(&mut self, slot: u32, vertices: &[f32]) -> Result<(), String> {
        if slot >= 3 {
            return Err("Invalid VBO slot".into());
        }
        validate_mesh(vertices)?;
        // SAFETY: bounded, finite, complete mesh; C copies before returning.
        self.result(unsafe {
            ot_upload(self.owner.as_ptr(), slot, vertices.as_ptr(), vertices.len())
        })
    }
    /// Upload the immutable quadratic/band resource once; no font ownership crosses FFI.
    pub fn set_font(
        &mut self,
        cw: u32,
        ch: u32,
        curves: &[f32],
        bw: u32,
        bh: u32,
        bands: &[u8],
    ) -> Result<(), String> {
        for (w, h, len) in [(cw, ch, curves.len()), (bw, bh, bands.len())] {
            if w == 0
                || h == 0
                || w > 4096
                || h > 4096
                || u64::from(w) * u64::from(h) > 65536
                || len as u64 != u64::from(w) * u64::from(h) * 4
            {
                return Err("Invalid font texture dimensions".into());
            }
        }
        if curves.iter().any(|v| !v.is_finite() || v.abs() > 4.) {
            return Err("Invalid font curve coordinate".into());
        }
        // SAFETY: validated dimensions exactly bound both borrowed arrays. C
        // converts/copies before return and retains only owned GL textures.
        self.result(unsafe {
            ot_set_font(
                self.owner.as_ptr(),
                cw,
                ch,
                curves.as_ptr(),
                curves.len(),
                bw,
                bh,
                bands.as_ptr(),
                bands.len(),
            )
        })
    }
    /// Retained colored triangles and indexed glyph quads share one ordered plan.
    pub fn upload_scene(
        &mut self,
        slot: u32,
        vertices: &[f32],
        glyphs: &[f32],
        draws: &[u32],
    ) -> Result<(), String> {
        crate::validate_text_mesh(glyphs, draws, vertices.len() / 6)?;
        self.upload(slot, vertices)?;
        // SAFETY: complete bounded quads and validated ranges; C copies the
        // arrays synchronously and creates its own vertex/index buffers.
        self.result(unsafe {
            ot_upload_text(
                self.owner.as_ptr(),
                slot,
                glyphs.as_ptr(),
                glyphs.len(),
                draws.as_ptr(),
                draws.len(),
            )
        })
    }
    /// Exact linear-light composition is an explicit diagnostic mode. The
    /// default directly blends authored sRGB colors using coverage alpha.
    pub fn linear_scene_enabled(&self) -> bool {
        // SAFETY: live, same-thread owner; reads a construction-time flag.
        unsafe { ot_linear_scene(self.owner.as_ptr()) != 0 }
    }
    pub fn glyph_cache_stats(&self) -> [u64; 3] {
        let mut stats = [0; 3];
        // SAFETY: fixed ABI writes exactly three u64 counters, no retention.
        unsafe { ot_text_stats(self.owner.as_ptr(), stats.as_mut_ptr()) };
        stats
    }
    pub fn draw(&mut self, projection: [f32; 9], animated: bool) -> Result<(), String> {
        if projection
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 100_000.)
        {
            return Err("Invalid projection".into());
        }
        // SAFETY: live same-thread context; C consumes exactly nine floats.
        self.result(unsafe { ot_draw(self.owner.as_ptr(), projection.as_ptr(), animated.into()) })
    }
    pub fn present(&mut self) -> Result<Flip, String> {
        let mut flip = Flip::default();
        // SAFETY: output ABI matches C and remains borrowed until flip completes.
        self.result(unsafe { ot_present(self.owner.as_ptr(), &mut flip) })?;
        Ok(flip)
    }
    pub fn input(&mut self) -> Result<Vec<Touch>, String> {
        let mut events = [Touch::default(); 64];
        // SAFETY: exactly capacity writable events, not retained. C returns at most capacity.
        let count = unsafe { ot_input(self.owner.as_ptr(), events.as_mut_ptr(), events.len()) };
        self.result(count)?;
        let count = usize::try_from(count).map_err(|e| e.to_string())?;
        if count > events.len() {
            return Err("Invalid input event count".into());
        }
        Ok(events[..count].to_vec())
    }
    pub fn pixels(&mut self) -> Result<Vec<u8>, String> {
        let info = self.info();
        let mut pixels = vec![0; info.width as usize * info.height as usize * 4];
        // SAFETY: dimensions bounded at construction; buffer holds full RGBA readback.
        self.result(unsafe { ot_pixels(self.owner.as_ptr(), pixels.as_mut_ptr(), pixels.len()) })?;
        Ok(pixels)
    }
}
impl Drop for Display {
    fn drop(&mut self) {
        // SAFETY: sole owner, !Send/!Sync, no outstanding borrows or retained Rust pointers.
        unsafe { ot_close(self.owner.as_ptr()) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_initialization_rejects_missing_device() {
        for _ in 0..32 {
            assert!(Display::open("/no/otrios-card", "/no/touch", [64, 64], false).is_err());
        }
        assert!(Display::headless([0, 64], false).is_err());
    }
    #[test]
    #[ignore = "Requires Mesa EGL; includes real font only in the test binary"]
    fn mesa_real_roboto_coverage_survives_fp16_addresses_lines_and_rotation() {
        use otrios_core::dashboard::{
            display::Surface, font::Font, font_reference::RESOURCE_SHA256,
        };
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/dashboard/fonts/Roboto-Regular.otq.zst"
        ));
        let font = Font::from_zstd(bytes, RESOURCE_SHA256).unwrap();
        let atlas = font.atlas();
        assert!(atlas.curve_height > 32 && atlas.band_height > 32);
        let factor = font.metrics().scale(6.);
        let baseline = 80. + font.metrics().baseline_from_top(6.);
        let mut vertices = Vec::new();
        let mut references = Vec::new();
        for (index, ch) in ['H', 'O', 'g', 'R', '7', 'y'].into_iter().enumerate() {
            let glyph = font.glyph(ch);
            let pen = 80. + index as f32 * 180.;
            let b = glyph.ink_bounds;
            let left = pen + b[0] * factor - 1.;
            let right = pen + b[2] * factor + 1.;
            let top = baseline - b[3] * factor - 1.;
            let bottom = baseline - b[1] * factor + 1.;
            for [x, y] in [[left, top], [right, top], [left, bottom], [right, bottom]] {
                vertices.extend([
                    x,
                    y,
                    (x - pen) / (factor * f32::from(font.metrics().units_per_em)),
                    (baseline - y) / (factor * f32::from(font.metrics().units_per_em)),
                    1.,
                    1.,
                    1.,
                    1.,
                ]);
                vertices.extend(glyph.band_transform);
                vertices.extend(glyph.band_data());
            }
            // A test-only independent winding reference. Conservative samples
            // exclude pixels near flattened edges; no reference rasterizer ships.
            let mut segments = Vec::new();
            for curve in &font.curves()
                [glyph.curve_start as usize..(glyph.curve_start + glyph.curve_count) as usize]
            {
                let at = |t: f32| {
                    let u = 1. - t;
                    let point = std::array::from_fn::<_, 2, _>(|axis| {
                        u * u * curve.points[0][axis]
                            + 2. * u * t * curve.points[1][axis]
                            + t * t * curve.points[2][axis]
                    });
                    [pen + point[0] * factor, baseline - point[1] * factor]
                };
                for step in 0..16 {
                    segments.push((at(step as f32 / 16.), at((step + 1) as f32 / 16.)));
                }
            }
            references.push((ch, [left, top, right, bottom], segments));
        }
        assert!(
            vertices.chunks_exact(16).any(|v| v[13] > 0.),
            "Exercise metadata beyond the first texture row"
        );
        for angle in [0, 90] {
            let surface = Surface::new(angle).unwrap();
            let [w, h] = surface.extent();
            let mut display = Display::headless([w, h], false).unwrap();
            display
                .set_font(
                    atlas.curve_width,
                    atlas.curve_height,
                    &atlas.curves,
                    atlas.band_width,
                    atlas.band_height,
                    &atlas.bands,
                )
                .unwrap();
            display
                .upload_scene(0, &[], &vertices, &[1, 0, (vertices.len() / 16) as u32])
                .unwrap();
            display.draw(surface.projection(), false).unwrap();
            let pixels = display.pixels().unwrap();
            let mut inside = 0usize;
            let mut inside_good = 0usize;
            let mut outside = 0usize;
            let mut outside_good = 0usize;
            for (ch, bounds, segments) in &references {
                let previous_inside = inside;
                let previous_good = inside_good;
                for y in ((bounds[1].floor() as i32)..=(bounds[3].ceil() as i32)).step_by(3) {
                    for x in ((bounds[0].floor() as i32)..=(bounds[2].ceil() as i32)).step_by(3) {
                        let p = [x as f32 + 0.5, y as f32 + 0.5];
                        let mut winding = 0i32;
                        let mut distance2 = f32::INFINITY;
                        for &(a, b) in segments {
                            let dx = b[0] - a[0];
                            let dy = b[1] - a[1];
                            let length2 = dx * dx + dy * dy;
                            let t = if length2 == 0. {
                                0.
                            } else {
                                ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2
                            }
                            .clamp(0., 1.);
                            distance2 = distance2.min(
                                (p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2),
                            );
                            let cross = dx * (p[1] - a[1]) - dy * (p[0] - a[0]);
                            if a[1] <= p[1] && b[1] > p[1] && cross > 0. {
                                winding += 1;
                            }
                            if a[1] > p[1] && b[1] <= p[1] && cross < 0. {
                                winding -= 1;
                            }
                        }
                        if distance2 < 2.25 {
                            continue;
                        }
                        let at = surface.project(p);
                        let px = at[0].floor() as usize;
                        let py = at[1].floor() as usize;
                        let red = pixels[((h as usize - 1 - py) * w as usize + px) * 4];
                        if winding != 0 {
                            inside += 1;
                            inside_good += usize::from(red > 220);
                        } else {
                            outside += 1;
                            outside_good += usize::from(red < 45);
                        }
                    }
                }
                let glyph_inside = inside - previous_inside;
                let glyph_good = inside_good - previous_good;
                assert!(
                    glyph_inside > 0 && glyph_good * 100 >= glyph_inside * 98,
                    "Glyph {ch} interior shredded at {angle}deg: {glyph_good}/{glyph_inside}"
                );
            }
            eprintln!(
                "Real Roboto {angle}deg: interior {inside_good}/{inside}; exterior {outside_good}/{outside}"
            );
            assert!(inside > 100 && outside > 100);
            assert!(
                inside_good * 100 >= inside * 98,
                "Real glyph interiors were shredded at {angle}deg"
            );
            assert!(
                outside_good * 100 >= outside * 98,
                "Real glyph holes/background were filled at {angle}deg"
            );
            let before = display.glyph_cache_stats();
            display.draw(surface.projection(), false).unwrap();
            assert_eq!(display.pixels().unwrap(), pixels);
            assert_eq!(display.glyph_cache_stats()[1], before[1]);
        }
    }

    #[test]
    #[ignore = "Requires Mesa EGL surfaceless platform; exercised by Linux CI"]
    fn mesa_slug_quadratic_holes_cache_and_selected_color_pipeline() {
        let mut d = Display::headless([128, 128], false).unwrap();
        // Two oppositely wound quadratic contours. A hole must survive analytic
        // coverage and cached composition; this is not a bitmap/SDF fixture.
        let curves = [
            0.5, 0.1, 0.9, 0.1, 0.9, 0.5, 0., 0., 0.9, 0.5, 0.9, 0.9, 0.5, 0.9, 0., 0., 0.5, 0.9,
            0.1, 0.9, 0.1, 0.5, 0., 0., 0.1, 0.5, 0.1, 0.1, 0.5, 0.1, 0., 0., 0.5, 0.3, 0.3, 0.3,
            0.3, 0.5, 0., 0., 0.3, 0.5, 0.3, 0.7, 0.5, 0.7, 0., 0., 0.5, 0.7, 0.7, 0.7, 0.7, 0.5,
            0., 0., 0.7, 0.5, 0.7, 0.3, 0.5, 0.3, 0., 0.,
        ];
        let mut bands = vec![0; 128];
        let mut band = |index: usize, a: u16, b: u16| {
            bands[index * 4..index * 4 + 2].copy_from_slice(&a.to_le_bytes());
            bands[index * 4 + 2..index * 4 + 4].copy_from_slice(&b.to_le_bytes());
        };
        for i in 0..8 {
            band(i, 8 | (16 << 8), 0);
            band(8 + i, 8 | (24 << 8), 0);
            band(16 + i, [0, 1, 6, 7, 2, 3, 4, 5][i] * 2, 0);
            band(24 + i, [1, 2, 5, 6, 0, 3, 4, 7][i] * 2, 0);
        }
        d.set_font(16, 1, &curves, 32, 1, &bands).unwrap();
        let mut glyphs = Vec::new();
        for (position, em) in [
            ([-1., -1.], [0., 0.]),
            ([1., -1.], [1., 0.]),
            ([-1., 1.], [0., 1.]),
            ([1., 1.], [1., 1.]),
        ] {
            glyphs.extend([
                position[0],
                position[1],
                em[0],
                em[1],
                1.,
                1.,
                1.,
                1.,
                8.,
                8.,
                0.,
                0.,
                0.,
                0.,
                7.,
                7.,
            ]);
        }
        let projection = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
        d.upload_scene(0, &[], &glyphs, &[1, 0, 4]).unwrap();
        d.draw(projection, false).unwrap();
        let first = d.pixels().unwrap();
        let stats = d.glyph_cache_stats();
        assert_eq!(stats[1..], [1, 4 * 1024 * 1024]);
        assert!(first[(64 * 128 + 64) * 4] < 25); // central hole
        assert!(first[(24 * 128 + 64) * 4] > 240); // opaque ring
        let fractional = first
            .chunks_exact(4)
            .filter(|p| p[0] > 25 && p[0] < 250)
            .count();
        assert!(
            fractional > 100,
            "Analytic edges must have fractional coverage"
        );
        d.draw(projection, false).unwrap();
        assert_eq!(d.pixels().unwrap(), first);
        assert_eq!(d.glyph_cache_stats()[1], 1); // no rerasterization on a cache hit
        assert!(d.glyph_cache_stats()[0] > stats[0]);
        // Exact subpixel phase and a new font generation cause fresh coverage.
        for v in glyphs.chunks_exact_mut(16) {
            v[0] += 0.25 / 64.;
        }
        d.upload_scene(0, &[], &glyphs, &[1, 0, 4]).unwrap();
        d.draw(projection, false).unwrap();
        assert_eq!(d.glyph_cache_stats()[1], 2);
        assert_ne!(d.pixels().unwrap(), first);
        d.set_font(16, 1, &curves, 32, 1, &bands).unwrap();
        d.draw(projection, false).unwrap();
        assert_eq!(d.glyph_cache_stats()[1], 3);
        // Recoloring reuses coverage. Premultiplied RGB/alpha must each receive
        // coverage once, including fractional edge pixels and non-unit tint alpha.
        let opaque = d.pixels().unwrap();
        for vertex in glyphs.chunks_exact_mut(16) {
            vertex[7] = 0.5;
        }
        d.upload_scene(0, &[], &glyphs, &[1, 0, 4]).unwrap();
        d.draw(projection, false).unwrap();
        let tinted = d.pixels().unwrap();
        assert_eq!(d.glyph_cache_stats()[1], 3);
        if !d.linear_scene_enabled() {
            for (full, half) in opaque.chunks_exact(4).zip(tinted.chunks_exact(4)) {
                for channel in 0..3 {
                    let expected =
                        (u16::from(full[channel]) + [16_u16, 28, 41][channel]).div_ceil(2) as u8;
                    assert!(
                        half[channel].abs_diff(expected) <= 2,
                        "Premultiplied coverage was applied more than once"
                    );
                }
            }
        }
        // The default uses conventional sRGB alpha. The explicit diagnostic
        // mode must instead encode linear-light50% near188.
        let mut base = Vec::new();
        for [x, y] in [
            [-1., -1.],
            [1., -1.],
            [-1., 1.],
            [-1., 1.],
            [1., -1.],
            [1., 1.],
        ] {
            base.extend([x, y, 0., 0., 0., 1.]);
        }
        d.upload(0, &base).unwrap();
        for v in base.chunks_exact_mut(6) {
            v[2] = 1.;
            v[3] = 1.;
            v[4] = 1.;
            v[5] = 0.5;
        }
        d.upload(1, &base).unwrap();
        d.draw(projection, true).unwrap();
        let pixels = d.pixels().unwrap();
        let range = if d.linear_scene_enabled() {
            180..=196
        } else {
            120..=136
        };
        assert!(
            pixels
                .chunks_exact(4)
                .all(|v| range.contains(&v[0]) && range.contains(&v[1]) && range.contains(&v[2]))
        );
    }
    #[test]
    #[ignore = "Requires Mesa EGL surfaceless platform; exercised by Linux CI"]
    fn mesa_color_transfer_matches_selected_pipeline_all_256_bytes() {
        let mut display = Display::headless([256, 8], false).unwrap();
        let corners = [
            [-1., -1.],
            [1., -1.],
            [-1., 1.],
            [-1., 1.],
            [1., -1.],
            [1., 1.],
        ];
        let mut background = Vec::new();
        for [x, y] in corners {
            background.extend([x, y, 0., 0., 0., 1.]);
        }
        display.upload(0, &background).unwrap();
        let mut ramp = Vec::new();
        for byte in 0..256 {
            for [x, y] in corners {
                let position = (byte as f32 + (x + 1.) * 0.5) * 2. / 256. - 1.;
                ramp.extend([position, y, 1., 1., 1., byte as f32 / 255.]);
            }
        }
        display.upload(1, &ramp).unwrap();
        display
            .draw([1., 0., 0., 0., 1., 0., 0., 0., 1.], true)
            .unwrap();
        let pixels = display.pixels().unwrap();
        for byte in 0..256 {
            let linear = byte as f32 / 255.;
            let encoded = if !display.linear_scene_enabled() {
                linear
            } else if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1. / 2.4) - 0.055
            };
            let expected = (encoded * 255.).round() as u8;
            for actual in &pixels[(4 * 256 + byte) * 4..(4 * 256 + byte) * 4 + 3] {
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "linear byte {byte}: expected {expected}, got {actual}"
                );
            }
        }
    }
    #[test]
    #[ignore = "Requires Mesa EGL surfaceless platform; exercised by Linux CI"]
    fn mesa_retained_geometry_and_alpha_pixels() {
        for rgb565 in [false, true] {
            let mut d = Display::headless([64, 64], rgb565).unwrap();
            let mut vertices = Vec::new();
            for [x, y] in [
                [-1., -1.],
                [1., -1.],
                [-1., 1.],
                [-1., 1.],
                [1., -1.],
                [1., 1.],
            ] {
                vertices.extend([x, y, 1., 0., 0., 1.]);
            }
            d.upload(0, &vertices).unwrap();
            d.draw([1., 0., 0., 0., 1., 0., 0., 0., 1.], false).unwrap();
            let p = d.pixels().unwrap();
            assert!(
                p.chunks_exact(4)
                    .all(|v| v[0] > 245 && v[1] < 5 && v[2] < 5)
            );
            for v in vertices.chunks_exact_mut(6) {
                v[2] = 0.;
                v[4] = 1.;
                v[5] = 0.5;
            }
            d.upload(1, &vertices).unwrap();
            d.draw([1., 0., 0., 0., 1., 0., 0., 0., 1.], true).unwrap();
            let p = d.pixels().unwrap();
            let range = if d.linear_scene_enabled() {
                175..=200
            } else {
                115..=140
            };
            assert!(
                p.chunks_exact(4)
                    .all(|v| range.contains(&v[0]) && v[1] < 5 && range.contains(&v[2]))
            );
            d.present().unwrap();
        }
    }
}
