// SPDX-License-Identifier: Apache-2.0
#[path = "../../../apps/animation-lab/app.rs"]
mod animation;
#[path = "../../../tools/canvas/authoring.rs"]
mod authoring;
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
    for (app, ui) in [
        (home::manifest(), Some(home::ui())),
        (playground::manifest(), Some(playground::ui(&code))),
        (layout::manifest(), Some(layout::ui())),
        (input::manifest(), None),
        (animation::manifest(), None),
        (three_d::manifest(), None),
        (monitoring::manifest(), None),
    ] {
        let folder = root.join(&app.id).join("package");
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        fs::write(folder.join("app.flex"), encode_app(&app)?).map_err(|e| e.to_string())?;
        if let Some(ui) = ui {
            if let Some(reference) = &ui.behavior {
                Behavior::load(&code, reference)?;
                fs::write(folder.join("behavior.wasm"), &code).map_err(|e| e.to_string())?;
            }
            fs::write(folder.join("ui.flex"), encode_ui(&ui)?).map_err(|e| e.to_string())?;
        }
        println!("{}: {}", app.id, app.status);
    }
    Ok(())
}
