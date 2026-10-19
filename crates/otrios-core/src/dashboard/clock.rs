// SPDX-License-Identifier: LGPL-3.0-only
//! Native wall clock helper; animation and input use host-supplied monotonic time.
/// Hosts supply wall time explicitly on the web. The native default is useful
/// for headless commands; animation and touch-repeat timers stay monotonic.
pub fn wall_time_seconds() -> i64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |v| v.as_secs().min(i64::MAX as u64) as i64)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}
