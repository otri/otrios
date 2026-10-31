// SPDX-License-Identifier: Apache-2.0
//! Add the real Brass Relay UI to a disposable Space qualification catalog.
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../apps/brass-relay/app.rs"]
mod brass;
#[path = "../../../tools/canvas/components.rs"]
pub mod components;
use otrios_core::canvas::*;
use otrios_interfaces::canvas::CatalogEntry;
fn main() -> Result<()> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Use space_chat_probe EXISTING_FIXTURE_APPS [LEGACY_JOURNAL]")?,
    );
    let mut catalog =
        decode_catalog(&std::fs::read(root.join("catalog.flex")).map_err(|e| e.to_string())?)?;
    let (app, factory) = brass::screens().remove(0);
    if catalog.apps.iter().any(|entry| entry.app.id == app.id) {
        return Err("Probe already exists".into());
    }
    let bytes = encode_ui(&factory(&[]))?;
    catalog.apps.push(CatalogEntry {
        app: app.clone(),
        path: app.id.clone(),
        private: false,
        ui_hash: hash(&bytes),
    });
    let catalog = encode_catalog(&catalog)?;
    let mut client = Client::default();
    client.admit_catalog(&catalog)?;
    client.mount_local(&bytes, &app.state_ref, &[], &[])?;
    let package = root.join(&app.id).join("package");
    std::fs::create_dir_all(&package).map_err(|e| e.to_string())?;
    std::fs::write(package.join("app.flex"), encode_app(&app)?).map_err(|e| e.to_string())?;
    std::fs::write(package.join("ui.flex"), bytes).map_err(|e| e.to_string())?;
    std::fs::write(root.join("catalog.flex"), catalog).map_err(|e| e.to_string())?;
    if let Some(path) = std::env::args().nth(2) {
        use otrios_interfaces::chat::Command;
        use std::time::{SystemTime, UNIX_EPOCH};
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return Err("Legacy fixture journal already exists".into());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;
        let mut room = otrios_core::canvas::chat::Room::open(Some(path), now)?;
        room.join(1, None, false, now, true)?;
        room.handle(
            1,
            Command::Profile {
                name: "Legacy Engineer".into(),
                avatar: 3,
            },
            now + 1,
        )?;
        room.handle(
            1,
            Command::Send {
                epoch: (now + 2) / 600_000,
                nonce: "a".repeat(32),
                text: "Legacy household dispatch".into(),
            },
            now + 2,
        )?;
    }
    Ok(())
}
