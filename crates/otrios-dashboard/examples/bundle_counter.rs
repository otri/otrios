// SPDX-License-Identifier: Apache-2.0
//! Build a standalone app without changing canvas-pack or the native/browser hosts.
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/bundle-counter/app.rs"]
mod counter;
use otrios_core::canvas::*;
use otrios_interfaces::canvas::{Catalog, CatalogEntry};
fn main() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or("Use bundle_counter OUTPUT_APPS_DIRECTORY")?;
    let root = std::path::Path::new(&root);
    if root.exists() {
        return Err("Standalone output already exists".into());
    }
    let step = std::env::args()
        .nth(2)
        .map(|value| value.parse::<u16>().map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or(1);
    if !(1..=10).contains(&step) {
        return Err("Counter step must be 1..=10".into());
    }
    let source = include_str!("../../../apps/bundle-counter/behavior/main.wat").replacen(
        "(global $step i32 (i32.const 1))",
        &format!("(global $step i32 (i32.const {step}))"),
        1,
    );
    let code = wat::parse_bytes(source.as_bytes())
        .map_err(|e| e.to_string())?
        .into_owned();
    let app = counter::manifest();
    let ui = counter::ui(&code, step);
    let bytes = encode_ui(&ui)?;
    let catalog = encode_catalog(&Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![CatalogEntry {
            app: app.clone(),
            path: "bundle-counter".into(),
            private: false,
            ui_hash: hash(&bytes),
        }],
    })?;
    let mut client = Client::default();
    client.admit_catalog(&catalog)?;
    client.mount_local(&bytes, &app.state_ref, &code, &[])?;
    let folder = root.join("bundle-counter/package");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for (name, bytes) in [
        ("app.flex", encode_app(&app)?),
        ("ui.flex", bytes),
        ("behavior.wasm", code),
    ] {
        std::fs::write(folder.join(name), bytes).map_err(|e| e.to_string())?;
    }
    std::fs::write(root.join("catalog.flex"), catalog).map_err(|e| e.to_string())?;
    println!("Built standalone bundle-counter; no host registry change");
    Ok(())
}
