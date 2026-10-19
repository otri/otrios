// SPDX-License-Identifier: Apache-2.0
//! Host-side resource lookup. The shared runtime verifies and decodes the bytes.
use otrios_core::dashboard::{font::Font, font_reference::RESOURCE_SHA256};
use std::{path::PathBuf, sync::Arc};
pub(super) fn load() -> Result<Arc<Font>, String> {
    let configured = std::env::var_os("OTRIOS_FONT_RESOURCE").map(PathBuf::from);
    let adjacent = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("No host folder")?
        .join("fonts/Roboto-Regular.otq.zst");
    let path = configured.unwrap_or_else(|| {
        if adjacent.exists() {
            adjacent
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../web/dashboard/fonts/Roboto-Regular.otq.zst")
        }
    });
    let bytes =
        std::fs::read(&path).map_err(|e| format!("Font resource {}: {e}", path.display()))?;
    Font::from_zstd(&bytes, RESOURCE_SHA256).map(Arc::new)
}
