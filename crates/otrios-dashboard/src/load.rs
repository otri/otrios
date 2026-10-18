// SPDX-License-Identifier: Apache-2.0
//! Bounded development load producer. Use a disposable server room.
use otrios_core::dashboard::{Replica, SyncSession, motion::Intent};
use std::time::{Duration, Instant};
use tungstenite::{Error, Message, stream::MaybeTlsStream};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: dashboard-load ws://DISPOSABLE_ROOM/stream HZ SECONDS".into());
    }
    let hz = args[2].parse::<u64>().map_err(|e| e.to_string())?;
    let seconds = args[3].parse::<u64>().map_err(|e| e.to_string())?;
    if !(1..=30).contains(&hz) || !(1..=60).contains(&seconds) {
        return Err("Use 1..30 Hz, 1..60 seconds; disposable room only".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (mut socket, _) = tungstenite::connect(args[1].as_str()).map_err(|e| e.to_string())?;
    if let MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_millis(2)))
            .map_err(|e| e.to_string())?;
    } else {
        return Err("This load tool requires a plain ws URL on loopback or a private VPN".into());
    }
    let mut r = Replica::new()?;
    let mut session = SyncSession::default();
    let start = Instant::now();
    let mut last = Instant::now();
    let mut updates = 0;
    let mut bytes_sent = 0;
    while start.elapsed() < Duration::from_secs(seconds) {
        if last.elapsed() >= Duration::from_secs_f64(1. / hz as f64) {
            r.set_motion(Intent {
                speed: if updates % 2 == 0 { 2. } else { 1. },
                ..Intent::default()
            })?;
            updates += 1;
            last = Instant::now();
        }
        if let Some(b) = r.generate_sync(&mut session)? {
            bytes_sent += b.len();
            socket
                .send(Message::Binary(b.into()))
                .map_err(|e| e.to_string())?;
        }
        match socket.read() {
            Ok(Message::Binary(b)) if !b.starts_with(b"OTML1") => {
                r.receive_sync(&mut session, &b)?;
            }
            Ok(Message::Ping(_)) => {
                socket.flush().map_err(|e| e.to_string())?;
            }
            Ok(Message::Close(_)) => return Err("Room closed".into()),
            Ok(_) => {}
            Err(Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    println!(
        "{{\"updates\":{updates},\"seconds\":{:.3},\"sync_bytes_sent\":{bytes_sent}}}",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
