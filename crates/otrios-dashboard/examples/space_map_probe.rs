// SPDX-License-Identifier: Apache-2.0
//! Read-only map coordinates/replay from private HTTP metadata, using the real runtime.
//! Run on the host holding the metadata; its output stays in the private probe pipe.
use otrios_core::{
    canvas::{Client, decode_catalog},
    spaces,
};
use otrios_interfaces::{scene::*, spaces::*};
use serde_json::{Value, json};
use std::{io::Read, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Catalog path required")?;
    let bytes = std::fs::read(path)?;
    let catalog = decode_catalog(&bytes)?;
    let mut input = String::new();
    std::io::stdin()
        .take(128 * 1024)
        .read_to_string(&mut input)?;
    let metadata: Value = serde_json::from_str(&input)?;
    let mut client = Client::default();
    client.set_font(Arc::new(otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )?));
    client.set_viewport([1280., 800.])?;
    client.admit_catalog(&bytes)?;
    client.configure_spaces(None)?;
    client.spaces_outgoing()?;
    let mut summaries = vec![];
    for space in metadata["spaces"].as_array().ok_or("Missing Spaces")? {
        let apps = space["apps"]
            .as_array()
            .ok_or("Missing apps")?
            .iter()
            .filter(|a| a["attached"] == true)
            .map(|a| {
                let id = a["policy"]["app"].as_str().ok_or("Missing app ID")?;
                let title = a["label"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .or_else(|| {
                        catalog
                            .apps
                            .iter()
                            .find(|e| e.app.id == id)
                            .map(|e| e.app.title.clone())
                    })
                    .ok_or("Missing title")?;
                Ok::<_, Box<dyn std::error::Error>>(MapApp {
                    app: id.into(),
                    title,
                    availability: AppAvailability::Available,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        summaries.push(SpaceSummary {
            id: space["id"].as_str().ok_or("Missing ID")?.into(),
            name: space["name"].as_str().ok_or("Missing name")?.into(),
            audience: serde_json::from_value(space["audience"].clone())?,
            role: serde_json::from_value(space["role"].clone())?,
            layout: serde_json::from_value(space["layout"].clone())?,
            attached_count: apps.len().try_into()?,
            apps,
        });
    }
    client.receive_spaces(&spaces::encode_event(&Event::Snapshot {
        observation: 1,
        resync: true,
        snapshot: Discovery {
            version: VERSION,
            cursor: DiscoveryCursor {
                authority: "a".repeat(32),
                revision: 1,
            },
            generation: metadata["generation"]
                .as_u64()
                .ok_or("Missing generation")?,
            identity: metadata["identity"].as_str().map(str::to_owned),
            spaces: summaries,
            selected: None,
        },
    })?)?;
    let scene = client.scene()?;
    let edit = scene
        .controls()
        .into_iter()
        .find(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == "Edit"))
        .ok_or("Edit unavailable")?
        .node
        .id;
    client.activate(client.epoch(), edit)?;
    let scene = client.scene()?;
    let point = |label: &str| -> Result<[f32; 2], Box<dyn std::error::Error>> {
        let c = scene
            .controls()
            .into_iter()
            .find(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == label))
            .ok_or("Requested title unavailable")?;
        Ok([c.origin[0] + 4., c.origin[1] + 4.])
    };
    let start = point("Space: Brass Relay")?;
    let end = point("Space: Household")?;
    let epoch = client.epoch();
    for (phase, position) in [
        (PointerPhase::Down, start),
        (PointerPhase::Move, end),
        (PointerPhase::Up, end),
    ] {
        client.pointer_at(
            epoch,
            PointerEvent {
                id: 1,
                phase,
                position,
            },
            100,
        )?;
    }
    let command = client.spaces_outgoing()?.ok_or("No reorder request")?;
    let Command::Manage { request, .. } = spaces::decode_command(&command)? else {
        return Err("Expected management".into());
    };
    if !matches!(
        request.change,
        Change::PlaceSpace {
            before: Some(_),
            ..
        }
    ) {
        return Err("Unexpected reorder".into());
    }
    println!(
        "{}",
        json!({"start":start,"end":end,"offline_reorder_passed":true})
    );
    Ok(())
}
