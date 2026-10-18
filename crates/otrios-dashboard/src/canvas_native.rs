// SPDX-License-Identifier: Apache-2.0
//! Headless process proof using exactly the Canvas byte stream/browser runtime.
use otrios_core::{
    canvas::{Client, Result},
    dashboard::render_scene,
};
use std::time::{Duration, Instant};
// Stable persisted schema field used by this Playground-specific acceptance probe.
const LEVEL_FIELD_ID: u16 = 1;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let endpoint = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("ws://127.0.0.1:8894/stream");
    if !endpoint.starts_with("ws://127.0.0.1:") {
        return Err("Prototype native probe requires a loopback endpoint (SSH forward for remote authority)".into());
    }
    let (mut ws, _) = tungstenite::connect(endpoint).map_err(|e| e.to_string())?;
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = ws.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| e.to_string())?;
    }
    let mut client = Client::default();
    client.open(
        "app:component-playground/ui",
        "app:component-playground/state",
    );
    let start = Instant::now();
    let mut edited = false;
    loop {
        while let Some(b) = client.outgoing()? {
            ws.send(tungstenite::Message::Binary(b.into()))
                .map_err(|e| e.to_string())?;
        }
        match ws.read() {
            Ok(tungstenite::Message::Binary(b)) => {
                client.receive(&b)?;
            }
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
        if !client.ui_ref().is_empty() && client.confirmed() {
            if !edited {
                let scene = client.scene()?;
                let slider = scene
                    .controls()
                    .iter()
                    .find(|c| {
                        c.node
                            .semantics
                            .as_ref()
                            .is_some_and(|s| s.label == "Linear slider")
                    })
                    .map(|c| c.node.id)
                    .ok_or("Missing slider control")?;
                client.event_for_node(client.epoch(), slider, 730, 0)?;
                edited = true;
            } else {
                let scene = client.scene()?;
                println!(
                    "Loaded app={} state={} WASM reducer level={} primitive_vertices={} acknowledged=true",
                    client.ui_ref(),
                    client.state_ref(),
                    client.value(LEVEL_FIELD_ID)?,
                    render_scene(&scene).vertices.len() / 6
                );
                break;
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err("Canvas native probe timed out".into());
        }
    }
    let _ = ws.close(None);
    Ok(())
}
