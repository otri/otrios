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
type Factory = fn(&[u8]) -> otrios_interfaces::canvas::Ui;
fn sources() -> Vec<(otrios_interfaces::canvas::App, Factory)> {
    vec![
        (home::manifest(), home::ui),
        (playground::manifest(), playground::ui),
        (layout::manifest(), layout::ui),
        (input::manifest(), input::ui),
        (animation::manifest(), animation::ui),
        (three_d::manifest(), three_d::ui),
        (monitoring::manifest(), monitoring::ui),
    ]
}
fn write_changed(path: &Path, bytes: &[u8]) -> Result<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    fs::create_dir_all(path.parent().ok_or("Missing artifact parent")?)
        .map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("pending-{}", std::process::id()));
    fs::write(&temporary, bytes)
        .and_then(|_| fs::rename(&temporary, path))
        .map_err(|e| e.to_string())
}
fn describe(
    root: &Path,
    app: &otrios_interfaces::canvas::App,
    factory: Factory,
) -> Result<(otrios_interfaces::canvas::Ui, Vec<u8>)> {
    let source = root.join(&app.id).join("behavior/main.wat");
    let code = if source.exists() {
        wat::parse_file(source).map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    let ui = factory(&code);
    if let Some(reference) = &ui.behavior {
        Behavior::load(&code, reference)?;
    } else if !code.is_empty() {
        return Err("Undeclared app behavior".into());
    }
    Ok((ui, code))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let root = std::path::PathBuf::from(args.first().map(String::as_str).unwrap_or("apps"));
    if args
        .get(1)
        .is_some_and(|v| v == "--app-ui" || v == "--app-behavior")
    {
        let behavior = args[1] == "--app-behavior";
        if args.len() != if behavior { 5 } else { 4 } {
            return Err("Use canvas-pack OUTPUT --app-ui APP BASELINE or --app-behavior APP BASELINE SOURCE".into());
        }
        return app_update(
            &root,
            &args[2],
            Path::new(&args[3]),
            if behavior {
                Some(Path::new(&args[4]))
            } else {
                None
            },
        );
    }
    if args.len() > 1 {
        return Err("Unknown pack arguments".into());
    }
    let mut entries = Vec::new();
    for (app, factory) in sources() {
        let (ui, code) = describe(&root, &app, factory)?;
        let folder = root.join(&app.id).join("package");
        write_changed(&folder.join("app.flex"), &encode_app(&app)?)?;
        if !code.is_empty() {
            write_changed(&folder.join("behavior.wasm"), &code)?;
        } else if folder.join("behavior.wasm").exists() {
            fs::remove_file(folder.join("behavior.wasm")).map_err(|e| e.to_string())?;
        }
        let bytes = encode_ui(&ui)?;
        write_changed(&folder.join("ui.flex"), &bytes)?;
        entries.push(otrios_interfaces::canvas::CatalogEntry {
            private: app.id == "monitoring",
            app: app.clone(),
            ui_hash: hash(&bytes),
        });
        println!("{}: canvas-app", app.id);
    }
    let catalog = encode_catalog(&otrios_interfaces::canvas::Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: entries.clone(),
    })?;
    for entry in &entries {
        let folder = root.join(&entry.app.id).join("package");
        let mut client = Client::default();
        client.admit_catalog(&catalog)?;
        client.mount_local(
            &fs::read(folder.join("ui.flex")).map_err(|e| e.to_string())?,
            &entry.app.state_ref,
            &fs::read(folder.join("behavior.wasm")).unwrap_or_default(),
            &[],
        )?;
    }
    write_changed(&root.join("catalog.flex"), &catalog)
}
fn app_update(root: &Path, id: &str, baseline: &Path, source: Option<&Path>) -> Result<()> {
    let mut catalog =
        decode_catalog(&fs::read(baseline.join("apps/catalog.flex")).map_err(|e| e.to_string())?)?;
    let (app, factory) = sources()
        .into_iter()
        .find(|(app, _)| app.id == id)
        .ok_or("Unknown app")?;
    let entry = catalog
        .apps
        .iter_mut()
        .find(|e| e.app.id == id)
        .ok_or("Baseline has no app")?;
    if entry.app != app {
        return Err("App manifest changed; full build required".into());
    }
    let old = decode_ui(
        &fs::read(baseline.join("authority/apps").join(id).join("ui.flex"))
            .map_err(|e| e.to_string())?,
    )?;
    let (ui, code) = if let Some(source) = source {
        let raw = fs::read(source).map_err(|e| e.to_string())?;
        if raw.len() > 512 * 1024 {
            return Err("Behavior source budget exceeded".into());
        }
        let code = wat::parse_bytes(&raw)
            .map_err(|e| e.to_string())?
            .into_owned();
        let mut ui = old.clone();
        let reference = ui
            .behavior
            .as_mut()
            .ok_or("App has no declared behavior; qualify an initial full build")?;
        reference.sha256 = hash(&code);
        Behavior::load(&code, reference)?;
        compatible_behavior(&old, &ui)?;
        (ui, code)
    } else {
        let code = fs::read(
            baseline
                .join("authority/apps")
                .join(id)
                .join("behavior.wasm"),
        )
        .unwrap_or_default();
        let ui = factory(&code);
        compatible_ui(&old, &ui)?;
        (ui, code)
    };
    let bytes = encode_ui(&ui)?;
    entry.ui_hash = hash(&bytes);
    let state_ref = entry.app.state_ref.clone();
    let encoded = encode_catalog(&catalog)?;
    let mut client = Client::default();
    client.admit_catalog(&encoded)?;
    client.mount_local(&bytes, &state_ref, &code, &[])?;
    write_changed(&root.join(id).join("package/ui.flex"), &bytes)?;
    write_changed(&root.join("catalog.flex"), &encoded)?;
    if source.is_some() {
        write_changed(&root.join(id).join("package/behavior.wasm"), &code)?;
        println!(
            "{id}: {} byte behavior module; UI layout and host runtimes reused",
            code.len()
        );
    } else {
        println!("{id}: UI data only; runtime and behavior reused");
    }
    Ok(())
}
