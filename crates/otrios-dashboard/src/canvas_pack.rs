// SPDX-License-Identifier: Apache-2.0
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../tools/canvas/components.rs"]
pub mod components;
#[path = "../../../apps/home/app.rs"]
mod home;
#[path = "../../../apps/monitoring/app.rs"]
mod monitoring;
#[path = "../../../apps/test-playground/app.rs"]
mod test_playground;
use otrios_core::canvas::*;
use std::{fs, path::Path};
type Factory = fn(&[u8]) -> otrios_interfaces::canvas::Ui;
fn sources() -> Vec<authoring::AppScreen> {
    [home::screens, test_playground::screens, monitoring::screens]
        .into_iter()
        .flat_map(|screens| screens())
        .collect()
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
        println!("{}: canvas-app", app.id);
        entries.push(otrios_interfaces::canvas::CatalogEntry {
            path: String::new(),
            private: app.id == "monitoring",
            app,
            ui_hash: hash(&bytes),
        });
    }
    let mut catalog = otrios_interfaces::canvas::Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: entries,
    };
    resolve_paths(&root, &mut catalog)?;
    let bytes = encode_catalog(&catalog)?;
    for entry in &catalog.apps {
        let folder = root.join(&entry.app.id).join("package");
        let mut client = Client::default();
        client.admit_catalog(&bytes)?;
        client.mount_local(
            &fs::read(folder.join("ui.flex")).map_err(|e| e.to_string())?,
            &entry.app.state_ref,
            &fs::read(folder.join("behavior.wasm")).unwrap_or_default(),
            &[],
        )?;
    }
    write_changed(&root.join("catalog.flex"), &bytes)
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
    write_changed(&root.join(id).join("package/ui.flex"), &bytes)?;
    resolve_paths(root, &mut catalog)?;
    let encoded = encode_catalog(&catalog)?;
    let mut client = Client::default();
    client.admit_catalog(&encoded)?;
    client.mount_local(&bytes, &state_ref, &code, &[])?;
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

/// Derive the navigation tree from the UI documents, never from package IDs.
fn resolve_paths(root: &Path, catalog: &mut otrios_interfaces::canvas::Catalog) -> Result<()> {
    let mut resolved = std::collections::BTreeMap::new();
    let entry = catalog
        .apps
        .first()
        .ok_or("No entry screen")?
        .app
        .ui_ref
        .clone();
    resolved.insert(entry.clone(), String::new());
    let mut queue = std::collections::VecDeque::from([entry]);
    while let Some(ui_ref) = queue.pop_front() {
        let entry = catalog
            .apps
            .iter()
            .find(|e| e.app.ui_ref == ui_ref)
            .ok_or("Unknown route owner")?;
        let ui = decode_ui(
            &fs::read(root.join(&entry.app.id).join("package/ui.flex"))
                .map_err(|e| e.to_string())?,
        )?;
        let mut names = std::collections::BTreeSet::new();
        for navigation in ui.navigation {
            let Some(segment) = navigation.route else {
                continue;
            };
            if !otrios_interfaces::canvas::valid_destination(&segment)
                || segment.contains('/')
                || !names.insert(segment.clone())
            {
                return Err("Invalid or duplicate child route name".into());
            }
            if !catalog
                .apps
                .iter()
                .any(|e| e.app.ui_ref == navigation.ui_ref)
            {
                return Err("Named route target is not admitted".into());
            }
            let prefix = &resolved[&ui_ref];
            let path = if prefix.is_empty() {
                segment
            } else {
                format!("{prefix}/{segment}")
            };
            if !otrios_interfaces::canvas::valid_destination(&path) {
                return Err("Resolved route exceeds path budget".into());
            }
            if resolved.insert(navigation.ui_ref.clone(), path).is_some() {
                return Err("Each screen requires one route owner; cycles are not allowed".into());
            }
            queue.push_back(navigation.ui_ref);
        }
    }
    for entry in &mut catalog.apps {
        entry.path = resolved
            .remove(&entry.app.ui_ref)
            .ok_or("Screen has no app-owned route")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_names_belong_to_owners_and_parent_renames_rebase_children() {
        use authoring::*;
        let root = std::env::temp_dir().join(format!("otrios-route-owners-{}", std::process::id()));
        let ui = |id, links| AppUi::new(id, id, schema("test", &[])).build(id, links);
        let parent = ui(
            "package-b",
            vec![button("Child").navigate_to(Destination::app("package-c").route("detail"))],
        );
        let child = ui("package-c", vec![]);
        let owner = |name| {
            ui(
                "package-a",
                vec![button("Lab").navigate_to(Destination::app("package-b").route(name))],
            )
        };
        let mut catalog = otrios_interfaces::canvas::Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: vec![],
        };
        for document in [owner("fixture"), parent, child] {
            let id = document
                .ui_ref
                .strip_prefix("app:")
                .unwrap()
                .strip_suffix("/ui")
                .unwrap();
            let bytes = encode_ui(&document).unwrap();
            write_changed(&root.join(id).join("package/ui.flex"), &bytes).unwrap();
            catalog.apps.push(otrios_interfaces::canvas::CatalogEntry {
                app: app(id, id, "test", &[]),
                path: String::new(),
                ui_hash: hash(&bytes),
                private: false,
            });
        }
        resolve_paths(&root, &mut catalog).unwrap();
        assert_eq!(
            catalog
                .apps
                .iter()
                .map(|e| e.path.as_str())
                .collect::<Vec<_>>(),
            ["", "fixture", "fixture/detail"]
        );
        let child_bytes = fs::read(root.join("package-c/package/ui.flex")).unwrap();
        compatible_ui(&owner("fixture"), &owner("renamed")).unwrap();
        write_changed(
            &root.join("package-a/package/ui.flex"),
            &encode_ui(&owner("renamed")).unwrap(),
        )
        .unwrap();
        resolve_paths(&root, &mut catalog).unwrap();
        assert_eq!(catalog.apps[2].path, "renamed/detail");
        assert_eq!(
            fs::read(root.join("package-c/package/ui.flex")).unwrap(),
            child_bytes
        );
        let invalid = ui(
            "package-a",
            vec![
                button("Lab").navigate_to(Destination::app("package-b").route("same")),
                button("Other").navigate_to(Destination::app("package-c").route("same")),
            ],
        );
        write_changed(
            &root.join("package-a/package/ui.flex"),
            &encode_ui(&invalid).unwrap(),
        )
        .unwrap();
        assert!(resolve_paths(&root, &mut catalog).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
