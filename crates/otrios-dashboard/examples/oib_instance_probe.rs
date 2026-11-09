// SPDX-License-Identifier: Apache-2.0
//! Bounded loopback client exercising the same OIB APIs as the shared editor.
#[path = "../src/session_client.rs"]
mod session_client;
use otrios_core::canvas::Client;
use otrios_interfaces::oib::designer::{
    DefinitionId, DesignerMode, DocumentId, ObjectAddress, Selection,
};
use serde_json::{Value, json};
use std::net::TcpStream;
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

fn settle(
    client: &mut Client,
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    start: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        client.tick(start.elapsed().as_millis() as u64, 0)?;
        while let Some(bytes) = client.outgoing()? {
            socket.send(Message::Binary(
                otrios_interfaces::session::frame(1, bytes).into(),
            ))?;
        }
        if client.confirmed()
            && client
                .resource_usage()
                .oib
                .as_ref()
                .is_some_and(|v| v.active_generation == v.presented_generation)
        {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("OIB qualification deadline".into());
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) if bytes.first() == Some(&1) => {
                client.receive(&bytes[1..])?;
            }
            Ok(Message::Ping(_)) => socket.flush()?,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Ok(Message::Text(text)) => {
                session_client::decode(&text)?;
            }
            Ok(Message::Close(_)) => return Err("Authority closed the probe session".into()),
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
}
fn connect(
    endpoint: &str,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, Box<dyn std::error::Error>> {
    let mut upgrade = endpoint.into_client_request()?;
    if let Ok(token) = std::env::var("OTRIOS_PROBE_TOKEN") {
        upgrade
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse()?);
    }
    let (mut socket, _) = tungstenite::connect(upgrade)?;
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    }
    session_client::negotiate(&mut socket)?;
    Ok(socket)
}
fn probe_client(catalog: &[u8], request: &Value) -> Result<Client, Box<dyn std::error::Error>> {
    let mut client = Client::default();
    client.set_font(Arc::new(otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )?));
    client.admit_catalog(catalog)?;
    client.open(
        request["ui_ref"].as_str().unwrap_or("app:live-ui/ui"),
        request["state_ref"].as_str().unwrap_or("app:live-ui/state"),
    );
    Ok(client)
}
fn state_controls(client: &Client) -> Result<Value, Box<dyn std::error::Error>> {
    fn find<'a>(
        node: &'a otrios_interfaces::scene::SceneNode,
        label: &str,
    ) -> Option<&'a otrios_interfaces::scene::SceneNode> {
        if node.semantics.as_ref().is_some_and(|s| s.label == label) {
            Some(node)
        } else {
            node.children.iter().find_map(|n| find(n, label))
        }
    }
    let scene = client.scene()?;
    let mut values = serde_json::Map::new();
    for (label, field) in [
        ("Volume", 1),
        ("Brightness", 2),
        ("Audio enabled", 3),
        ("Display enabled", 4),
    ] {
        let control = find(scene.root(), label);
        values.insert(
            label.into(),
            json!({
                "normalized": client.value(field).ok(),
                "node": control.map(|c| c.id),
                "checked": control.and_then(|c| c.semantics.as_ref()).map(|s| s.checked),
                "control_percent": control.and_then(|c| client.control_value(c.id).ok().flatten()),
            }),
        );
    }
    Ok(Value::Object(values))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let endpoint = args.get(1).ok_or("Loopback session endpoint required")?;
    if !endpoint.starts_with("ws://127.0.0.1:") {
        return Err("Use the existing loopback authority".into());
    }
    let catalog = std::fs::read(args.get(2).ok_or("Verified active catalog path required")?)?;
    let mut text = String::new();
    std::io::stdin().take(16385).read_to_string(&mut text)?;
    if text.len() > 16384 {
        return Err("OIB probe request budget".into());
    }
    let request: Value = serde_json::from_str(&text)?;
    let steps = request["steps"]
        .as_array()
        .ok_or("Bounded steps required")?;
    if steps.len() > 16 {
        return Err("OIB probe step limit".into());
    }
    let mut socket = connect(endpoint)?;
    let mut client = probe_client(&catalog, &request)?;
    let start = Instant::now();
    settle(&mut client, &mut socket, start)?;
    println!(
        "{}",
        json!({"phase":"mounted","epoch":client.epoch(),"oib":client.resource_usage().oib,"state_controls":state_controls(&client)?,"scene":client.scene()?.root()})
    );
    for step in steps {
        let id = step["id"].as_u64().unwrap_or(10);
        let name = step["name"].as_str().map(str::to_owned);
        let changed = match step["op"].as_str().ok_or("Operation required")? {
            "restore" => client.restore_oib(step["defaults"].as_bool().unwrap_or(false))?,
            "add" => client.instantiate_oib(id, name)?,
            "studio" => {
                client.set_oib_designer_studio(step["enabled"].as_bool().unwrap_or(true))?
            }
            "insert" => client.insert_oib(
                step["prototype"].as_u64().unwrap_or(id),
                step["before"].as_u64(),
            )?,
            "reorder" => client.reorder_oib(id, step["before"].as_u64())?,
            "definitions" => {
                client.edit_oib_definitions(serde_json::from_value(step["batch"].clone())?)?
            }
            "hierarchy" => {
                client.edit_oib_hierarchy(serde_json::from_value(step["batch"].clone())?)?
            }
            "race-reorder" => {
                let other_id = step["other_id"]
                    .as_u64()
                    .ok_or("Rival component required")?;
                let mut rival_socket = connect(endpoint)?;
                let mut rival = probe_client(&catalog, &request)?;
                settle(&mut rival, &mut rival_socket, start)?;
                let generation = client
                    .resource_usage()
                    .oib
                    .ok_or("Main OIB state required")?
                    .active_generation;
                if rival
                    .resource_usage()
                    .oib
                    .as_ref()
                    .is_none_or(|o| o.active_generation != generation)
                {
                    return Err("Race clients must start at the same confirmed generation".into());
                }
                // Queue the captured-generation main intent without sending it.
                // The rival then commits first through an independent session.
                if !client.reorder_oib(id, step["before"].as_u64())? {
                    return Err("Main reorder was not queued".into());
                }
                if !rival.reorder_oib(other_id, step["other_before"].as_u64())? {
                    return Err("Rival reorder was not queued".into());
                }
                settle(&mut rival, &mut rival_socket, start)?;
                if rival
                    .resource_usage()
                    .oib
                    .as_ref()
                    .is_none_or(|o| o.active_generation <= generation)
                {
                    return Err("Rival reorder must change the confirmed order".into());
                }
                let _ = rival_socket.close(None);
                // The ordinary end-step settle sends the older main request and
                // records its durable rejection; a later reorder deliberately rebases.
                true
            }
            "duplicate" => client.duplicate_oib(id, name)?,
            "remove" => client.remove_oib(id)?,
            "inspect" => client.inspect_oib(id)?,
            "selection" => {
                let selection = match step["scope"].as_str().ok_or("Selection scope required")? {
                    "document" => Selection::Document(DocumentId(id)),
                    "definition" => Selection::Definition(DefinitionId(id)),
                    "definition-node" => Selection::DefinitionNode {
                        definition: DefinitionId(id),
                        node: otrios_interfaces::oib::designer::NodeId(
                            step["node"].as_u64().ok_or("Definition node required")?,
                        ),
                    },
                    "object" => Selection::Object(serde_json::from_value::<ObjectAddress>(
                        step["address"].clone(),
                    )?),
                    _ => return Err("Unknown selection scope".into()),
                };
                client.inspect_oib_selection(selection)?
            }
            "mode" => client.set_oib_designer_mode(match step["mode"].as_str() {
                Some("edit") => DesignerMode::Edit,
                Some("test") => DesignerMode::Test,
                _ => return Err("Explicit edit/test mode required".into()),
            })?,
            "viewport" => client.set_viewport(serde_json::from_value(step["size"].clone())?)?,
            "activate" => client.activate(client.epoch(), u32::try_from(id)?)?,
            "pointer" => client.pointer_at(
                client.epoch(),
                serde_json::from_value(step["event"].clone())?,
                start.elapsed().as_millis() as u64,
            )?,
            "instance-property" => {
                let property = step["property"].as_str().ok_or("Named property required")?;
                let value = if step["value"].is_null() {
                    None
                } else {
                    Some(serde_json::from_value(step["value"].clone())?)
                };
                client.edit_oib_instance_property(id, property, value)?
            }
            "property" => {
                let value = if step["value"].is_null() {
                    None
                } else {
                    Some(serde_json::from_value(step["value"].clone())?)
                };
                client.edit_oib(&[(id, value)])?
            }
            "properties" => {
                let changes = step["changes"]
                    .as_array()
                    .ok_or("Property batch required")?;
                if changes.len() > 16 {
                    return Err("Property batch limit".into());
                }
                let changes = changes
                    .iter()
                    .map(|change| {
                        let cell = change["cell"].as_u64().ok_or("Cell required")?;
                        let value = if change["value"].is_null() {
                            None
                        } else {
                            Some(serde_json::from_value(change["value"].clone())?)
                        };
                        Ok((cell, value))
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
                client.edit_oib(&changes)?
            }
            "observe" => false,
            _ => return Err("Unknown OIB probe operation".into()),
        };
        settle(&mut client, &mut socket, start)?;
        let usage = client.resource_usage();
        println!(
            "{}",
            json!({"phase":"step","step":step,"changed":changed,"epoch":client.epoch(),"oib":usage.oib,"confirmed":client.confirmed(),"input_busy":client.input_busy(),"elapsed_ms":start.elapsed().as_millis(),"state_controls":state_controls(&client)?,"scene":client.scene()?.root()})
        );
    }
    let _ = socket.close(None);
    Ok(())
}
