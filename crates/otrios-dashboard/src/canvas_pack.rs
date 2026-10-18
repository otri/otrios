// SPDX-License-Identifier: Apache-2.0
#[path = "../../../apps/animation-lab/app.rs"]
mod animation;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../tools/canvas/components.rs"]
pub mod components;
#[path = "../../../apps/home/app.rs"]
mod home;
#[path = "../../../apps/input-lab/app.rs"]
mod input;
#[path = "../../../apps/layout-lab/app.rs"]
mod layout;
#[path = "../../../apps/monitoring/app.rs"]
mod monitoring;
#[path = "../../../apps/component-playground/app.rs"]
mod playground;
#[path = "../../../apps/three-d-lab/app.rs"]
mod three_d;
use otrios_core::canvas::*;
use std::{fs, path::Path};
fn main() -> Result<()> {
    let root = std::env::args().nth(1).unwrap_or_else(|| "apps".into());
    let root = Path::new(&root);
    let code = wat::parse_file(root.join("component-playground/behavior/main.wat"))
        .map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for (app, ui) in [
        (home::manifest(), home::ui()),
        (playground::manifest(), playground::ui(&code)),
        (layout::manifest(), layout::ui()),
        (input::manifest(), input::ui()),
        (animation::manifest(), animation::ui()),
        (three_d::manifest(), three_d::ui()),
        (monitoring::manifest(), monitoring::ui()),
    ] {
        let folder = root.join(&app.id).join("package");
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        fs::write(folder.join("app.flex"), encode_app(&app)?).map_err(|e| e.to_string())?;
        {
            if let Some(reference) = &ui.behavior {
                Behavior::load(&code, reference)?;
                fs::write(folder.join("behavior.wasm"), &code).map_err(|e| e.to_string())?;
            }
            let bytes = encode_ui(&ui)?;
            entries.push(otrios_interfaces::canvas::CatalogEntry {
                private: app.id == "monitoring",
                app: app.clone(),
                ui_hash: hash(&bytes),
            });
            fs::write(folder.join("ui.flex"), bytes).map_err(|e| e.to_string())?;
        }
        println!("{}: {}", app.id, app.status);
    }
    let catalog = encode_catalog(&otrios_interfaces::canvas::Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: entries.clone(),
    })?;
    for entry in &entries {
        let folder = root.join(&entry.app.id).join("package");
        let mut client = Client::default();
        client.admit_catalog(&catalog)?;
        let code = fs::read(folder.join("behavior.wasm")).unwrap_or_default();
        client.mount_local(
            &fs::read(folder.join("ui.flex")).map_err(|e| e.to_string())?,
            &entry.app.state_ref,
            &code,
            &[],
        )?;
    }
    fs::write(
        root.join("catalog.flex"),
        encode_catalog(&otrios_interfaces::canvas::Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: entries,
        })?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
