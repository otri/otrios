// SPDX-License-Identifier: Apache-2.0
//! Real-process admitted app/guest probe; this is not physical display acceptance.
#[path = "../src/session_client.rs"]
mod session_client;
use otrios_core::canvas::{self, Client, Result};
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(4..=5).contains(&args.len()) || !args[0].starts_with("ws://127.0.0.1:") {
        return Err(
            "Use bundle_app_probe LOOPBACK_SESSION CATALOG APP (increase|read|deny)".into(),
        );
    }
    let bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    let catalog = canvas::decode_catalog(&bytes)?;
    let entry = catalog
        .apps
        .iter()
        .find(|entry| entry.app.id == args[2])
        .ok_or("App is not installed")?;
    let mut request = args[0]
        .as_str()
        .into_client_request()
        .map_err(|e| e.to_string())?;
    if let Some(origin) = args.get(4) {
        request.headers_mut().insert(
            "Origin",
            origin.parse().map_err(|_| "Invalid probe Origin")?,
        );
    }
    let (mut socket, _) = tungstenite::connect(request).map_err(|e| e.to_string())?;
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| e.to_string())?;
    }
    session_client::negotiate(&mut socket)?;
    let mut client = Client::default();
    client.admit_catalog(&bytes)?;
    client.open(&entry.app.ui_ref, &entry.app.state_ref);
    let start = Instant::now();
    let mut before = None;
    loop {
        client.tick(start.elapsed().as_millis() as u64, 0)?;
        while let Some(bytes) = client.outgoing()? {
            socket
                .send(tungstenite::Message::Binary(
                    otrios_interfaces::session::frame(1, bytes).into(),
                ))
                .map_err(|e| e.to_string())?;
        }
        match socket.read() {
            Ok(tungstenite::Message::Binary(bytes)) => {
                client.receive(bytes.strip_prefix(&[1]).ok_or("Unexpected state channel")?)?;
            }
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error.to_string()),
            _ => {}
        }
        if args[3] == "deny" && client.auth_required() {
            if !client.ui_ref().is_empty() {
                return Err("Private UI leaked before admission".into());
            }
            println!("DENIED app={} payloads=false", entry.app.id);
            break;
        }
        if args[3] != "deny" && !client.ui_ref().is_empty() && client.confirmed() {
            if args[3] == "controls" {
                let scene = client.scene()?;
                let controls:Vec<_>=scene.controls().into_iter().map(|control|serde_json::json!({
                    "label":control.node.semantics.as_ref().map(|value|value.label.as_str()).unwrap_or(""),
                    "x":control.origin[0]+control.node.frame.width*control.scale/2.,
                    "y":control.origin[1]+control.node.frame.height*control.scale/2.
                })).collect();
                println!(
                    "{}",
                    serde_json::json!({"app":entry.app.id,"ui_sha256":canvas::hex(&entry.ui_hash),"controls":controls})
                );
                break;
            }
            if args[3] == "increase" && before.is_none() {
                before = Some(client.value(1)?);
                let scene = client.scene()?;
                let node = scene
                    .controls()
                    .into_iter()
                    .find(|control| {
                        control
                            .node
                            .semantics
                            .as_ref()
                            .is_some_and(|value| value.label == "Increase counter")
                    })
                    .ok_or("Missing app control")?
                    .node
                    .id;
                client.event_for_node(client.epoch(), node, 1000, 0)?;
            } else {
                let value = client.value(1)?;
                if before.is_some_and(|before| value == before) {
                    return Err("Guest edit was not applied".into());
                }
                println!(
                    "CONFIRMED app={} value={} before={:?} state={} guest=true",
                    entry.app.id,
                    value,
                    before,
                    client.state_ref()
                );
                break;
            }
        }
        if start.elapsed() > Duration::from_secs(15) {
            return Err("Bundle app probe timed out".into());
        }
    }
    let _ = socket.close(None);
    Ok(())
}
