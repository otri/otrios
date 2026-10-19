// SPDX-License-Identifier: Apache-2.0
//! Small bootstrap companion: stable release hello/events, no app-state protocol.
use otrios_interfaces::release::{ClientMessage, MAX_BYTES, ServerMessage, VERSION};
use std::{
    io::{self, Write},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Error, Message, stream::MaybeTlsStream};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: release-watch WSS_URL CURRENT_HASH_OR_EMPTY STATE_PROTOCOL".into());
    }
    if !args[1].starts_with("wss://") && !args[1].starts_with("ws://127.0.0.1:") {
        return Err("Use secure release control (or loopback for tests)".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let hello = serde_json::to_string(&ClientMessage::Hello {
        version: VERSION,
        release: (!args[2].is_empty()).then(|| args[2].clone()),
        state_protocol: args[3].clone(),
    })?;
    let mut delay = 250;
    loop {
        let connection = tungstenite::client::connect_with_config(
            &args[1],
            Some(
                tungstenite::protocol::WebSocketConfig::default()
                    .max_message_size(Some(MAX_BYTES))
                    .max_frame_size(Some(MAX_BYTES)),
            ),
            0,
        );
        if let Ok((mut socket, _)) = connection {
            let tcp = match socket.get_mut() {
                MaybeTlsStream::Plain(t) => Some(t),
                MaybeTlsStream::Rustls(t) => Some(&mut t.sock),
                _ => None,
            };
            if let Some(tcp) = tcp {
                tcp.set_read_timeout(Some(Duration::from_secs(5)))?;
                tcp.set_write_timeout(Some(Duration::from_secs(5)))?;
                tcp.set_nodelay(true)?;
            }
            if socket.send(Message::Text(hello.clone().into())).is_ok() {
                let mut heard = Instant::now();
                let mut greeted = false;
                loop {
                    match socket.read() {
                        Ok(Message::Text(text)) => {
                            let Ok(event) = serde_json::from_str::<ServerMessage>(&text) else {
                                break;
                            };
                            if event.version() != VERSION
                                || !event.release().valid()
                                || (!greeted && !matches!(event, ServerMessage::Hello { .. }))
                            {
                                break;
                            }
                            greeted = true;
                            delay = 250;
                            heard = Instant::now();
                            let mut stdout = io::stdout().lock();
                            writeln!(stdout, "{}", serde_json::to_string(&event)?)?;
                            stdout.flush()?;
                        }
                        Ok(Message::Ping(_) | Message::Pong(_)) => {
                            heard = Instant::now();
                            if socket.flush().is_err() {
                                break;
                            }
                        }
                        Err(Error::Io(e))
                            if matches!(
                                e.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                            ) =>
                        {
                            if heard.elapsed() > Duration::from_secs(if greeted { 60 } else { 5 }) {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            }
        }
        eprintln!("Release control disconnected; reconnecting");
        thread::sleep(Duration::from_millis(delay));
        delay = (delay * 2).min(10000);
    }
}
