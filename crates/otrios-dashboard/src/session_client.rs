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
/// Native remote sessions use the same explicit Origin contract as browser
/// sessions. Authentication remains a separate header supplied by the caller.
pub fn request(url: &str) -> Result<tungstenite::http::Request<()>, String> {
    use tungstenite::client::IntoClientRequest;
    let mut request = url
        .into_client_request()
        .map_err(|_| "Invalid session URL")?;
    let uri = request.uri();
    let scheme = match uri.scheme_str() {
        Some("wss") => "https",
        Some("ws") => "http",
        _ => return Err("Expected WebSocket URL".into()),
    };
    let authority = uri.authority().ok_or("Missing session host")?.as_str();
    if authority.contains('@') {
        return Err("Session URL must not contain credentials".into());
    }
    let authority = match scheme {
        "https" => authority.strip_suffix(":443").unwrap_or(authority),
        _ => authority.strip_suffix(":80").unwrap_or(authority),
    };
    let origin = format!("{scheme}://{authority}");
    request.headers_mut().insert(
        tungstenite::http::header::ORIGIN,
        origin.parse().map_err(|_| "Invalid session origin")?,
    );
    Ok(request)
}
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

#[cfg(test)]
mod tests {
    #[test]
    fn native_session_origin_matches_remote_and_loopback_endpoints() {
        for (url, origin) in [
            (
                "wss://otrios.otri.ca/session?space=abc",
                "https://otrios.otri.ca",
            ),
            ("wss://otrios.otri.ca:443/session", "https://otrios.otri.ca"),
            ("ws://127.0.0.1:8876/session", "http://127.0.0.1:8876"),
            ("ws://[::1]:80/session", "http://[::1]"),
        ] {
            let request = super::request(url).unwrap();
            assert_eq!(request.uri().to_string(), url);
            assert_eq!(request.headers()["Origin"], origin);
            assert!(!request.headers().contains_key("Authorization"));
        }
        assert!(super::request("wss://user:password@otrios.otri.ca/session").is_err());
    }
}

pub fn authenticated_request(
    url: &str,
    identity: Option<&std::path::Path>,
) -> Result<tungstenite::http::Request<()>, String> {
    let mut request = request(url)?;
    if let Some(path) = identity {
        use std::io::Read;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| "Cannot open Frame identity")?;
        let metadata = file
            .metadata()
            .map_err(|_| "Cannot inspect Frame identity")?;
        if !metadata.is_file() || !(64..=65).contains(&metadata.len()) {
            return Err("Invalid Frame identity file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 {
                return Err("Frame identity must be private and unlinked".into());
            }
        }
        let mut token = String::new();
        file.take(66)
            .read_to_string(&mut token)
            .map_err(|_| "Cannot read Frame identity")?;
        let token = token.trim();
        if token.len() != 64
            || !token
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("Invalid Frame identity".into());
        }
        request.headers_mut().insert(
            tungstenite::http::header::AUTHORIZATION,
            format!("Bearer {token}")
                .parse()
                .map_err(|_| "Invalid Frame identity")?,
        );
    }
    Ok(request)
}
