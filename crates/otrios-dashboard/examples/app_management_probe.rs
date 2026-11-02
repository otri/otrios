// SPDX-License-Identifier: Apache-2.0
//! Thin loopback shared-session client for installer integration qualification.
#[path = "../src/session_client.rs"]
mod session_client;
use otrios_core::spaces;
use std::{io::Read, time::Duration};
use tungstenite::client::IntoClientRequest;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args().nth(1).ok_or("Loopback session required")?;
    if !endpoint.starts_with("ws://127.0.0.1:") {
        return Err("Use a loopback qualification authority".into());
    }
    let mut text = String::new();
    std::io::stdin().take(8193).read_to_string(&mut text)?;
    if text.len() > 8192 {
        return Err("Management command budget".into());
    }
    let command: otrios_interfaces::spaces::Command = serde_json::from_str(&text)?;
    let bytes = spaces::encode_command(&command)?;
    let mut request = endpoint.into_client_request()?;
    if let Ok(token) = std::env::var("OTRIOS_PROBE_TOKEN") {
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse()?);
    }
    let (mut socket, _) = tungstenite::connect(request)?;
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    }
    session_client::negotiate(&mut socket)?;
    socket.send(tungstenite::Message::Binary(
        otrios_interfaces::session::frame(5, bytes).into(),
    ))?;
    loop {
        match socket.read()? {
            tungstenite::Message::Binary(bytes) if bytes.first() == Some(&5) => {
                println!(
                    "{}",
                    serde_json::to_string(&spaces::decode_event(&bytes[1..])?)?
                );
                break;
            }
            tungstenite::Message::Ping(_) => socket.flush()?,
            _ => (),
        }
    }
    let _ = socket.close(None);
    Ok(())
}
