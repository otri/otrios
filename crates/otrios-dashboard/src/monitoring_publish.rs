// SPDX-License-Identifier: Apache-2.0
//! Outbound-only producer transport. Collection and local rendering never wait on it.
use super::session_client;
use otrios_core::canvas::{
    self,
    monitoring::{self as data, Command, Event, Snapshot},
};
use std::{
    net::TcpStream,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};
type Socket = WebSocket<MaybeTlsStream<TcpStream>>;
fn send(socket: &mut Socket, command: Command) -> Result<(), String> {
    socket
        .send(Message::Binary(
            otrios_interfaces::session::frame(data::CHANNEL, data::encode_command(&command)?)
                .into(),
        ))
        .map_err(|_| "Monitoring publication disconnected".into())
}
fn connection(
    path: &Path,
    url: &str,
    identity: &Path,
    base: &mut Option<Snapshot>,
) -> Result<(), String> {
    if !(url.starts_with("wss://")
        || url.starts_with("ws://127.0.0.1:")
        || url.starts_with("ws://[::1]:"))
    {
        return Err("Remote Monitoring publication requires WSS".into());
    }
    let request = session_client::authenticated_request(url, Some(identity))?;
    let (mut socket, _) = tungstenite::client::connect_with_config(
        request,
        Some(
            tungstenite::protocol::WebSocketConfig::default()
                .max_message_size(Some(data::MAX_PACKET_BYTES + 1))
                .max_frame_size(Some(data::MAX_PACKET_BYTES + 1)),
        ),
        0,
    )
    .map_err(|_| "Monitoring server unavailable")?;
    let tcp = match socket.get_mut() {
        MaybeTlsStream::Plain(s) => s,
        MaybeTlsStream::Rustls(s) => &mut s.sock,
        _ => return Err("Unsupported Monitoring transport".into()),
    };
    tcp.set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|_| "Monitoring timeout configuration")?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "Monitoring timeout configuration")?;
    socket
        .send(session_client::hello(None, true))
        .map_err(|_| "Monitoring hello failed")?;
    let mut announced = false;
    let mut greeted = false;
    let mut available = false;
    let mut offered: Option<Snapshot> = None;
    let mut last_offer = Instant::now() - Duration::from_secs(2);
    let mut heard = Instant::now();
    loop {
        if release_changed() {
            return Ok(());
        }
        if heard.elapsed() > Duration::from_secs(65) {
            return Err("Monitoring heartbeat expired".into());
        }
        if offered.is_some() && last_offer.elapsed() > Duration::from_secs(15) {
            return Err("Monitoring publication acknowledgement timed out".into());
        }
        if greeted
            && available
            && offered.is_none()
            && last_offer.elapsed() >= Duration::from_secs(1)
        {
            let latest = data::store::load(path)?;
            if !announced || base.as_ref().is_none_or(|p| p.root() != latest.root()) {
                send(
                    &mut socket,
                    Command::Offer {
                        feed: latest.head().feed.clone(),
                        root: latest.root().to_vec(),
                    },
                )?;
                offered = Some(latest);
            }
            last_offer = Instant::now();
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                use otrios_interfaces::session::ServerControl;
                let control = session_client::decode(&text)?;
                heard = Instant::now();
                match control {
                    ServerControl::Hello {
                        compatible,
                        state_available,
                        ..
                    }
                    | ServerControl::Upgrade {
                        compatible,
                        state_available,
                        ..
                    } => {
                        if !compatible {
                            return Err("Monitoring server protocol incompatible".into());
                        }
                        greeted = true;
                        available = state_available;
                    }
                    ServerControl::StateStatus { available: a, .. } => {
                        available = a;
                        if !a {
                            offered = None;
                            announced = false;
                        }
                    }
                }
            }
            Ok(Message::Binary(bytes)) if bytes.first() == Some(&data::CHANNEL) => {
                heard = Instant::now();
                match data::decode_event(&bytes[1..])? {
                    Event::Need { base: server_base } => {
                        let latest = offered.as_ref().ok_or("Unsolicited Monitoring request")?;
                        let prior = base
                            .as_ref()
                            .filter(|b| server_base.as_deref() == Some(b.root().as_slice()));
                        send(
                            &mut socket,
                            Command::Transfer {
                                feed: latest.head().feed.clone(),
                                root: latest.root().to_vec(),
                                base: prior.map(|p| p.root().to_vec()),
                                bytes: latest.transfer(prior)?,
                            },
                        )?;
                    }
                    Event::Ack { root, revision } => {
                        announced = true;
                        let latest = offered
                            .take()
                            .ok_or("Unsolicited Monitoring acknowledgement")?;
                        if latest.root().as_slice() != root || latest.head().revision != revision {
                            return Err("Wrong Monitoring acknowledgement".into());
                        }
                        let status = serde_json::json!({"root":canvas::hex(&root),"revision":revision,"generated":latest.head().window.generated});
                        data::store::atomic(
                            &path.with_extension("published.json"),
                            status.to_string().as_bytes(),
                        )?;
                        println!(
                            "PUBLISHED monitoring revision={revision} root={}",
                            canvas::hex(&root)
                        );
                        *base = Some(latest);
                    }
                    Event::Rejected => {
                        return Err("Monitoring producer grant or revision rejected".into());
                    }
                }
            }
            Ok(Message::Ping(bytes)) => {
                heard = Instant::now();
                socket
                    .send(Message::Pong(bytes))
                    .map_err(|_| "Monitoring ping failed")?;
            }
            Ok(Message::Pong(_)) => heard = Instant::now(),
            Ok(Message::Close(_)) => return Err("Monitoring publication closed".into()),
            Ok(_) => return Err("Unexpected Monitoring channel".into()),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err("Monitoring publication disconnected".into()),
        }
    }
}
pub fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err("Use --publish SNAPSHOT SESSION IDENTITY".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut base = None;
    loop {
        if release_changed() {
            return Ok(());
        }
        let result = connection(
            Path::new(&args[0]),
            &args[1],
            Path::new(&args[2]),
            &mut base,
        );
        if let Err(error) = result {
            eprintln!("MONITORING local collection retained: {error}");
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn release_changed() -> bool {
    let Ok(expected) = std::env::var("OTRIOS_MONITORING_RELEASE_ID") else {
        return false;
    };
    let Ok(path) = std::env::var("OTRIOS_MONITORING_RELEASE_POINTER") else {
        return false;
    };
    std::fs::read(path)
        .ok()
        .filter(|b| b.len() <= 8192)
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["id"].as_str().is_some_and(|id| id != expected))
}
