// SPDX-License-Identifier: Apache-2.0
// Allocation-free no_std kernels. Raw-pointer inputs are validated by the harness.
#![no_std]
#![no_main]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    #[cfg(target_arch = "wasm32")]
    core::arch::wasm32::unreachable();
    #[cfg(not(target_arch = "wasm32"))]
    {
        unsafe extern "C" { fn abort() -> !; }
        unsafe { abort() }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel(seed: u32, count: u32) -> u32 {
    let mut x = seed;
    for _ in 0..count {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        x ^= x >> 13;
    }
    x
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn scan(data: *const u32, count: u32, rounds: u32) -> u32 {
    let mut value = 2166136261u32;
    for round in 0..rounds {
        for index in 0..count {
            value = (value ^ unsafe { *data.add(index as usize) } ^ round).wrapping_mul(16777619);
        }
    }
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn noop(value: u32) -> u32 { value }
