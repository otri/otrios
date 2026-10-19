// SPDX-License-Identifier: Apache-2.0
//! Small synchronous bootstrap shared by CLI clients/tests. No CRDT decoding.
use otrios_interfaces::session::{
    ClientControl, MAX_CONTROL_BYTES, STATE_PROTOCOL, ServerControl, VERSION,
};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};
pub fn hello(release: Option<String>, state: bool) -> Message {
    Message::Text(
        serde_json::to_string(&ClientControl::Hello {
            version: VERSION,
            release,
            state_protocol: STATE_PROTOCOL.into(),
            state,
        })
        .expect("hello serialization")
        .into(),
    )
}
pub fn decode(text: &str) -> Result<ServerControl, String> {
    if text.len() > MAX_CONTROL_BYTES {
        return Err("Control size exceeded".into());
    }
    let event: ServerControl = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let version = match &event {
        ServerControl::Hello { version, .. }
        | ServerControl::Upgrade { version, .. }
        | ServerControl::StateStatus { version, .. } => *version,
    };
    if version != VERSION || event.release().is_some_and(|r| !r.valid()) {
        return Err("Invalid control envelope".into());
    }
    Ok(event)
}
#[allow(dead_code)] // Streaming hosts use hello/decode directly.
pub fn negotiate<S: Read + Write>(socket: &mut WebSocket<S>) -> Result<(), String> {
    socket.send(hello(None, true)).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut greeted = false;
    loop {
        if Instant::now() > deadline {
            return Err("Session hello timeout".into());
        }
        match socket.read() {
            Ok(Message::Text(text)) => match decode(&text)? {
                ServerControl::Hello {
                    compatible: true,
                    state_available,
                    ..
                } if !greeted => {
                    greeted = true;
                    if state_available {
                        return Ok(());
                    }
                }
                ServerControl::StateStatus {
                    available: true, ..
                } if greeted => return Ok(()),
                ServerControl::Hello { .. } => return Err("Incompatible state protocol".into()),
                _ => {
                    std::thread::sleep(Duration::from_millis(1));
                }
            },
            Ok(Message::Ping(_)) => socket.flush().map_err(|e| e.to_string())?,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            other => return Err(format!("Unexpected session negotiation {other:?}")),
        }
    }
}
