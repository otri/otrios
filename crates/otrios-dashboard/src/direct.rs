// SPDX-License-Identifier: Apache-2.0
//! Linux platform host; the shared state worker remains safe Rust.
#[cfg(target_os = "linux")]
mod direct_host;
#[cfg(target_os = "linux")]
mod font_resource;
#[cfg(target_os = "linux")]
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
