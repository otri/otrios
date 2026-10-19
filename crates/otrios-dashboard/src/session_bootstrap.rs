// SPDX-License-Identifier: Apache-2.0
//! One-shot cold-start hello. Exits before the renderer opens its host session.
#[path = "session_client.rs"]
mod session_client;
use otrios_interfaces::session::{MAX_CONTROL_BYTES, ServerControl};
use std::time::Duration;
use tungstenite::{Message, stream::MaybeTlsStream};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: session-bootstrap WSS_URL CURRENT_HASH_OR_EMPTY".into());
    }
    if !args[1].starts_with("wss://")
        && !args[1].starts_with("ws://127.0.0.1:")
        && !args[1].starts_with("ws://[::1]:")
    {
        return Err("TLS or loopback required".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (mut socket, _) = tungstenite::client::connect_with_config(
        args[1].as_str(),
        Some(
            tungstenite::protocol::WebSocketConfig::default()
                .max_message_size(Some(MAX_CONTROL_BYTES))
                .max_frame_size(Some(MAX_CONTROL_BYTES)),
        ),
        0,
    )?;
    let tcp = match socket.get_mut() {
        MaybeTlsStream::Plain(s) => Some(s),
        MaybeTlsStream::Rustls(s) => Some(&mut s.sock),
        _ => None,
    };
    if let Some(tcp) = tcp {
        tcp.set_read_timeout(Some(Duration::from_secs(5)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(5)))?;
    }
    socket.send(session_client::hello(
        (!args[2].is_empty()).then(|| args[2].clone()),
        false,
    ))?;
    let Message::Text(text) = socket.read()? else {
        return Err("Missing stable hello".into());
    };
    let event = session_client::decode(&text)?;
    if !matches!(event, ServerControl::Hello { .. }) || event.release().is_none() {
        return Err("Missing release descriptor".into());
    }
    println!("{}", serde_json::to_string(&event)?);
    let _ = socket.close(None);
    Ok(())
}
