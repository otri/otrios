// SPDX-License-Identifier: Apache-2.0
//! Bounded loopback client exercising the same OIB APIs as the shared editor.
#[path = "../src/session_client.rs"]
mod session_client;
use otrios_core::canvas::Client;
use serde_json::{Value, json};
use std::net::TcpStream;
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

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
    let (mut socket, _) = tungstenite::connect(endpoint.as_str())?;
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    }
    session_client::negotiate(&mut socket)?;
    let mut client = Client::default();
    client.set_font(Arc::new(otrios_core::dashboard::font::Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        otrios_core::dashboard::font_reference::RESOURCE_SHA256,
    )?));
    client.admit_catalog(&catalog)?;
    client.open("app:live-ui/ui", "app:live-ui/state");
    let start = Instant::now();
    settle(&mut client, &mut socket, start)?;
    println!(
        "{}",
        json!({"phase":"mounted","epoch":client.epoch(),"oib":client.resource_usage().oib})
    );
    for step in steps {
        let id = step["id"].as_u64().unwrap_or(10);
        let name = step["name"].as_str().map(str::to_owned);
        let changed = match step["op"].as_str().ok_or("Operation required")? {
            "add" => client.instantiate_oib(id, name)?,
            "duplicate" => client.duplicate_oib(id, name)?,
            "remove" => client.remove_oib(id)?,
            "inspect" => client.inspect_oib(id)?,
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
            "observe" => false,
            _ => return Err("Unknown OIB probe operation".into()),
        };
        settle(&mut client, &mut socket, start)?;
        let usage = client.resource_usage();
        println!(
            "{}",
            json!({"phase":"step","step":step,"changed":changed,"epoch":client.epoch(),"oib":usage.oib,"confirmed":client.confirmed(),"input_busy":client.input_busy(),"elapsed_ms":start.elapsed().as_millis()})
        );
    }
    let _ = socket.close(None);
    Ok(())
}
