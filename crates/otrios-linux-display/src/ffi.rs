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
            assert!(
                p.chunks_exact(4).all(|v| (115..=140).contains(&v[0])
                    && v[1] < 5
                    && (115..=140).contains(&v[2]))
            );
            d.present().unwrap();
        }
    }
}
