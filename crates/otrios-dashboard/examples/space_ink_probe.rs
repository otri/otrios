// SPDX-License-Identifier: Apache-2.0
//! Add a drawing fixture to a disposable qualification catalog.
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use authoring as a;
use otrios_core::canvas::*;
use otrios_interfaces::canvas::{App, CatalogEntry};
fn main() -> Result<()> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Use space_ink_probe EXISTING_FIXTURE_APPS")?,
    );
    add(&root, "space-ink-probe", "Space ink probe")?;
    if std::env::args().nth(2).as_deref() == Some("--populated") {
        for (id, title) in [
            ("notes", "Notes"),
            ("sketches", "Sketches"),
            ("component-playground", "Components"),
            ("layout-lab", "Layouts"),
            ("layout-text-scroll", "Text scrolling"),
            ("layout-document", "Documents"),
            ("input-lab", "Text input"),
            ("animation-lab", "2D motion"),
            ("three-d-lab", "3D"),
            ("monitoring", "Monitoring"),
        ] {
            add(&root, id, title)?;
        }
    }
    Ok(())
}
fn add(root: &std::path::Path, id: &str, title: &str) -> Result<()> {
    let mut catalog =
        decode_catalog(&std::fs::read(root.join("catalog.flex")).map_err(|e| e.to_string())?)?;
    if catalog.apps.iter().any(|entry| entry.app.id == id) {
        return Err("Probe already exists".into());
    }
    let x = a::StateField::normalized(1, "x", 500);
    let y = a::StateField::normalized(2, "y", 500);
    let ui = a::AppUi::new(id, title, a::schema("space-ink-probe-v1", &[x, y]))
        .with_touch_pad("pad")
        .build(
            title,
            vec![
                a::canvas(vec![a::surface(a::CARD, 12.).pin_edges(0.)])
                    .key("pad")
                    .frame([24., 160., 1232., 600.])
                    .on_drag("Touch pad", a::Event::pad(x, y)),
            ],
        );
    let bytes = encode_ui(&ui)?;
    let app = App {
        id: id.into(),
        title: title.into(),
        ui_ref: ui.ui_ref,
        state_ref: format!("app:{id}/state"),
        status: "canvas-app".into(),
        dependencies: vec![],
    };
    catalog.apps.push(CatalogEntry {
        app: app.clone(),
        path: id.into(),
        private: false,
        ui_hash: hash(&bytes),
    });
    let catalog = encode_catalog(&catalog)?;
    let mut client = Client::default();
    client.admit_catalog(&catalog)?;
    client.mount_local(&bytes, &app.state_ref, &[], &[])?;
    let package = root.join(id).join("package");
    std::fs::create_dir(root.join(id)).map_err(|e| e.to_string())?;
    std::fs::create_dir(&package).map_err(|e| e.to_string())?;
    std::fs::write(package.join("app.flex"), encode_app(&app)?).map_err(|e| e.to_string())?;
    std::fs::write(package.join("ui.flex"), bytes).map_err(|e| e.to_string())?;
    std::fs::write(root.join("catalog.flex"), catalog).map_err(|e| e.to_string())?;
    Ok(())
}
