// SPDX-License-Identifier: Apache-2.0
//! Exercise the real native app owner in Linux CI without a DRM device.
#![cfg(all(feature = "canvas", feature = "host-sync"))]
#[path = "../src/font_resource.rs"]
mod font_resource;
#[allow(dead_code, reason = "Headless tests omit the platform display adapter")]
#[path = "../src/native_worker.rs"]
mod native_worker;
