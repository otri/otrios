// SPDX-License-Identifier: Apache-2.0
//! Linux platform host; the shared state worker remains safe Rust.
#[cfg(target_os = "linux")]
mod direct_host;
#[cfg(any(target_os = "linux", test))]
mod font_resource;
#[cfg(any(target_os = "linux", test))]
mod native_input;
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "macOS runs safe worker tests without a DRM presentation host"
    )
)]
mod native_worker;
fn main() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        direct_host::run()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("The direct display host requires Linux DRM/KMS".into())
    }
}
