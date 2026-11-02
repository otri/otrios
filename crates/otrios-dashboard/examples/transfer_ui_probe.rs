// SPDX-License-Identifier: Apache-2.0
//! Offline shared-UI coordinates from private authority metadata. Never sends a
//! command or proves a transfer: physical input and authority outcomes do that.
use otrios_core::{
    canvas::{self, Client},
    spaces,
};
use otrios_interfaces::{
    keyboard::Edit,
    spaces::{Command, Discovery, Event},
    transfers::TransferReply,
};
use serde_json::{Value, json};
use std::{io::Read, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = std::fs::read(std::env::args().nth(1).ok_or("Catalog path required")?)?;
    let mut input = String::new();
    std::io::stdin()
        .take(256 * 1024 + 1)
        .read_to_string(&mut input)?;
    if input.len() > 256 * 1024 {
        return Err("Private coordinate input budget".into());
    }
    let request: Value = serde_json::from_str(&input)?;
    let snapshot: Discovery = serde_json::from_value(request["snapshot"].clone())?;
    let selected = snapshot
        .selected
        .as_ref()
        .ok_or("Selected Space required")?;
    if snapshot.catalog != canvas::hash(&catalog) {
        return Err("Coordinate catalog changed".into());
    }
    let mut client = Client::default();
    client.set_font(Arc::new(otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )?));
    client.set_viewport([1280., 800.])?;
    client.admit_catalog(&catalog)?;
    client.configure_spaces(Some(&selected.space))?;
    client.set_session_controls(true);
    client.set_chat_native(true);
    client.set_transfer_host(
        request["origin"].as_str().ok_or("Public origin required")?,
        true,
        true,
        false,
    )?;
    let Some(command) = client.spaces_outgoing()? else {
        return Err("Expected offline Watch".into());
    };
    let Command::Watch { observation, .. } = spaces::decode_command(&command)? else {
        return Err("Expected offline Watch".into());
    };
    client.receive_spaces(&spaces::encode_event(&Event::Snapshot {
        observation,
        snapshot: Box::new(snapshot),
        resync: true,
    })?)?;
    let mut points = Vec::new();
    for step in request["steps"]
        .as_array()
        .ok_or("Coordinate steps required")?
    {
        match step["type"].as_str().ok_or("Step type required")? {
            "activate" | "point" => {
                let label = step["label"].as_str().ok_or("Exact label required")?;
                let scene = client.scene()?;
                let mut controls = scene
                    .controls()
                    .into_iter()
                    .filter(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == label));
                let control = controls
                    .next()
                    .ok_or_else(|| format!("Unavailable coordinate: {label}"))?;
                if controls.next().is_some() {
                    return Err("Ambiguous coordinate label".into());
                }
                let id = control.node.id;
                let point = [
                    control.origin[0] + control.node.frame.width * control.scale / 2.,
                    control.origin[1] + control.node.frame.height * control.scale / 2.,
                ];
                let visible =
                    point[0] >= 0. && point[0] < 1280. && point[1] >= 0. && point[1] < 800.;
                if !visible && (step["type"] == "activate" || step["allow_offscreen"] != true) {
                    return Err("Scroll the real panel before using offscreen coordinates".into());
                }
                points.push(
                    json!({"label":label,"point":point,"visible":visible,"epoch":client.epoch()}),
                );
                if step["type"] == "activate" {
                    client.activate(client.epoch(), id)?;
                }
            }
            "reply" => {
                let task = client
                    .take_transfer_task()
                    .ok_or("No offline host I/O task")?;
                let reply: TransferReply = serde_json::from_value(step["reply"].clone())?;
                client.receive_transfer(task.id, reply)?;
            }
            "text" => {
                client.hardware_edit(&Edit::Commit(
                    step["value"].as_str().ok_or("Text required")?.into(),
                ))?;
            }
            "scroll" => {
                let delta = step["delta"]
                    .as_f64()
                    .filter(|v| v.is_finite() && v.abs() <= 1_000_000.)
                    .ok_or("Bounded scroll delta required")?;
                client.scroll_map(client.epoch(), delta as f32)?;
            }
            _ => return Err("Unsupported coordinate step".into()),
        }
        // A recipe may reach a review, but must not simulate its management
        // decision. Final confirmation coordinates use `point`, not `activate`.
        if client.spaces_outgoing()?.is_some() || client.take_space_switch().is_some() {
            return Err("Coordinate recipe queued an authority command or scope switch".into());
        }
    }
    println!(
        "{}",
        json!({"coordinate_only":true,"no_authority_commands":true,"points":points})
    );
    Ok(())
}
