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
            band(i, 8, 16);
            band(8 + i, 8, 24);
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
                7.,
                7.,
                0.,
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
