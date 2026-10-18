// SPDX-License-Identifier: Apache-2.0
// Native-only no_std + alloc ownership experiment using the platform allocator.
#![no_std]
extern crate alloc;
use alloc::{rc::Rc, sync::Arc};
use core::alloc::{GlobalAlloc, Layout};

#[cfg_attr(target_os = "macos", link(name = "System"))]
#[cfg_attr(target_os = "linux", link(name = "c"))]
unsafe extern "C" {
    fn posix_memalign(out: *mut *mut u8, alignment: usize, size: usize) -> i32;
    fn free(pointer: *mut u8);
    fn abort() -> !;
}
struct PlatformAllocator;
unsafe impl GlobalAlloc for PlatformAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut pointer = core::ptr::null_mut();
        let result = unsafe { posix_memalign(&mut pointer, layout.align().max(8), layout.size().max(1)) };
        if result == 0 { pointer } else { core::ptr::null_mut() }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, _: Layout) { unsafe { free(pointer) } }
}
#[global_allocator]
static ALLOCATOR: PlatformAllocator = PlatformAllocator;
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! { unsafe { abort() } }

#[unsafe(no_mangle)]
pub extern "C" fn rc_create(value: u32) -> *const u32 { Rc::into_raw(Rc::new(value)) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rc_retain(pointer: *const u32) -> *const u32 {
    unsafe { Rc::increment_strong_count(pointer) }; pointer
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rc_release(pointer: *const u32) { unsafe { Rc::decrement_strong_count(pointer) } }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rc_value(pointer: *const u32) -> u32 { unsafe { *pointer } }
#[unsafe(no_mangle)]
pub extern "C" fn arc_create(value: u32) -> *const u32 { Arc::into_raw(Arc::new(value)) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_retain(pointer: *const u32) -> *const u32 {
    unsafe { Arc::increment_strong_count(pointer) }; pointer
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_release(pointer: *const u32) { unsafe { Arc::decrement_strong_count(pointer) } }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_value(pointer: *const u32) -> u32 { unsafe { *pointer } }
