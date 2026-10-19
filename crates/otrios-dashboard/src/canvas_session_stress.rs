// SPDX-License-Identifier: Apache-2.0
//! External load generator for the real Canvas client, app behavior and transport.
//! Use an isolated authority: active mode deliberately edits Component Playground.
#[path = "session_client.rs"]
mod session_client;
use otrios_core::canvas::{Client, Result};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

struct Peer {
    client: Client,
    ws: WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    controls: [u32; 3],
    sent: u64,
    received: u64,
    received_packets: u64,
    pending: Option<Instant>,
    next: Instant,
    cycles: u64,
    acks: Vec<u64>,
    issued_age: u64,
    blocked: bool,
    failed: bool,
}
fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e,tungstenite::Error::Io(e) if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut))
}
impl Peer {
    fn new(endpoint: &str, catalog: &[u8], mounted: bool) -> Result<Self> {
        let mut request = endpoint.into_client_request().map_err(|e| e.to_string())?;
        request
            .headers_mut()
            .insert("Origin", "https://otrios.otri.ca".parse().unwrap());
        let (mut ws, _) = tungstenite::connect(request).map_err(|e| e.to_string())?;
        let MaybeTlsStream::Plain(tcp) = ws.get_mut() else {
            return Err("Use loopback/SSH forwarding for controlled transport".into());
        };
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        tcp.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        tcp.set_nodelay(true).map_err(|e| e.to_string())?;
        session_client::negotiate(&mut ws)?;
        let MaybeTlsStream::Plain(tcp) = ws.get_mut() else {
            unreachable!()
        };
        tcp.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut client = Client::default();
        if mounted {
            client.admit_catalog(catalog)?;
            client.open(
                "app:component-playground/ui",
                "app:component-playground/state",
            );
        }
        Ok(Self {
            client,
            ws,
            controls: [0; 3],
            sent: 0,
            received: 0,
            received_packets: 0,
            pending: None,
            next: Instant::now(),
            cycles: 0,
            acks: vec![],
            issued_age: 0,
            blocked: false,
            failed: false,
        })
    }
    fn pump(&mut self, now: u64, mounted: bool) -> Result<()> {
        if mounted {
            self.client
                .tick(now, otrios_core::dashboard::clock::wall_time_seconds())?;
        }
        match self.ws.flush() {
            Ok(()) => self.blocked = false,
            Err(e) if would_block(&e) => self.blocked = true,
            Err(e) => return Err(e.to_string()),
        }
        if mounted && !self.blocked {
            for _ in 0..4 {
                let Some(bytes) = self.client.outgoing()? else {
                    break;
                };
                self.sent += (bytes.len() + 1) as u64;
                match self.ws.send(Message::Binary(
                    otrios_interfaces::session::frame(1, bytes).into(),
                )) {
                    Ok(()) => {}
                    // Tungstenite retains a frame whose socket flush would block.
                    Err(e) if would_block(&e) => {
                        self.blocked = true;
                        break;
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        for _ in 0..8 {
            match self.ws.read() {
                Ok(Message::Binary(bytes)) if mounted => {
                    self.received += bytes.len() as u64;
                    self.received_packets += 1;
                    self.client
                        .receive(bytes.strip_prefix(&[1]).ok_or("Expected Canvas channel")?)?;
                }
                Ok(Message::Close(_)) => return Err("Server closed the session".into()),
                Ok(_) => {}
                Err(e) if would_block(&e) => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        if mounted {
            self.issued_age = self
                .issued_age
                .max(self.client.dispatch_usage().issued_age_ms.unwrap_or(0));
            if self.client.confirmed()
                && let Some(at) = self.pending.take()
            {
                self.acks.push(at.elapsed().as_micros() as u64);
            }
        }
        Ok(())
    }
    fn values(&self) -> Result<[u16; 4]> {
        Ok([
            self.client.value(1)?,
            self.client.value(2)?,
            self.client.value(3)?,
            self.client.value(4)?,
        ])
    }
    fn assign(&mut self, index: usize) -> Result<()> {
        let n = self.cycles as usize + index * 137;
        let value = |mul: usize| ((n * mul + 19) % 997 + 1) as u16;
        self.pending = Some(Instant::now());
        for (node, a, b) in [
            (self.controls[0], value(37), 0),
            (self.controls[1], value(53), 0),
            (self.controls[2], value(71), value(97)),
        ] {
            self.client
                .event_for_node(self.client.epoch(), node, a, b)?;
        }
        // Flush the final batch through the same release path as real gestures.
        self.client.pointer(
            self.client.epoch(),
            otrios_interfaces::scene::PointerEvent {
                id: 999,
                phase: otrios_interfaces::scene::PointerPhase::Up,
                position: [-1., -1.],
            },
        )?;
        self.cycles += 1;
        Ok(())
    }
}
fn quantiles(mut samples: Vec<u64>) -> serde_json::Value {
    if samples.is_empty() {
        return serde_json::Value::Null;
    }
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() as f64 * q).ceil() as usize).saturating_sub(1)];
    serde_json::json!({"n":samples.len(),"p50":at(0.5),"p95":at(0.95),"p99":at(0.99),"max":samples.last()})
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 {
        return Err("Usage: canvas-session-stress LOOPBACK_OR_TAILNET_WS CATALOG CLIENTS ACTIVE HZ SECONDS socket|mounted".into());
    }
    let count = args[3].parse::<usize>().map_err(|e| e.to_string())?;
    let active = args[4].parse::<usize>().map_err(|e| e.to_string())?;
    let hz = args[5].parse::<u32>().map_err(|e| e.to_string())?;
    let seconds = args[6].parse::<u64>().map_err(|e| e.to_string())?;
    let mounted = args[7] == "mounted";
    let controlled_endpoint = args[1]
        .parse::<tungstenite::http::Uri>()
        .ok()
        .is_some_and(|u| {
            u.scheme_str() == Some("ws")
                && u.host()
                    .and_then(|h| h.parse::<std::net::Ipv4Addr>().ok())
                    .is_some_and(|ip| {
                        ip.is_loopback()
                            || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
                    })
        });
    if !controlled_endpoint
        || !(1..=4096).contains(&count)
        || active > count
        || active > 256
        || hz > 120
        || !(1..=120).contains(&seconds)
        || (active > 0 && (!mounted || hz == 0))
        || !["socket", "mounted"].contains(&args[7].as_str())
    {
        return Err(
            "Bounds: loopback/Tailnet IPv4, 1..4096 clients, <=256 active, <=120 Hz, 1..120 seconds".into(),
        );
    }
    let catalog = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    let origin = Instant::now();
    let mut peers = Vec::with_capacity(count);
    let mut errors = vec![];
    let next = AtomicUsize::new(0);
    let cancel = AtomicBool::new(false);
    let (tx, rx) = mpsc::sync_channel(32);
    std::thread::scope(|scope| {
        for _ in 0..count.min(16) {
            let tx = tx.clone();
            let (next, cancel, catalog, endpoint) = (&next, &cancel, &catalog, &args[1]);
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= count {
                        break;
                    }
                    let result = Peer::new(endpoint, catalog, mounted);
                    if result.is_err() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    if tx.send((index, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (index, result) in rx {
            match result {
                Ok(peer) => peers.push(peer),
                Err(e) => errors.push(format!("connect {index}: {e}")),
            }
            // Keep sessions draining throughout the bounded connection ramp.
            if mounted || peers.len() % 32 == 0 {
                for peer in &mut peers {
                    if !peer.failed
                        && let Err(e) = peer.pump(origin.elapsed().as_millis() as u64, mounted)
                    {
                        peer.failed = true;
                        if errors.len() < 20 {
                            errors.push(format!("setup: {e}"));
                        }
                    }
                }
            }
        }
    });
    if mounted {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let mut ready = true;
            for peer in &mut peers {
                peer.pump(origin.elapsed().as_millis() as u64, true)?;
                ready &= !peer.client.ui_ref().is_empty() && peer.client.confirmed();
            }
            if ready {
                break;
            }
            if Instant::now() > deadline {
                return Err("Mount deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        for peer in peers.iter_mut().take(active) {
            for (slot, label) in ["Linear slider", "Rotary dial", "Touch pad"]
                .iter()
                .enumerate()
            {
                peer.controls[slot] = peer
                    .client
                    .scene()?
                    .controls()
                    .iter()
                    .find(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == *label))
                    .map(|c| c.node.id)
                    .ok_or("Missing Component Playground control")?;
            }
        }
    }
    let setup_seconds = origin.elapsed().as_secs_f64();
    let before_sent: u64 = peers.iter().map(|p| p.sent).sum();
    let before_received: u64 = peers.iter().map(|p| p.received).sum();
    let before_packets: u64 = peers.iter().map(|p| p.received_packets).sum();
    let measure_start_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let start = Instant::now();
    let mut loops = vec![];
    let mut converged = !mounted;
    let mut convergence_us = None;
    loop {
        let turn = Instant::now();
        let issuing = start.elapsed() < Duration::from_secs(seconds);
        for (index, peer) in peers.iter_mut().enumerate() {
            if peer.failed {
                continue;
            }
            if let Err(e) = peer.pump(origin.elapsed().as_millis() as u64, mounted) {
                peer.failed = true;
                if errors.len() < 20 {
                    errors.push(format!("peer {index}: {e}"));
                }
                continue;
            }
            if issuing && index < active && peer.pending.is_none() && turn >= peer.next {
                peer.assign(index)?;
                peer.next = Instant::now() + Duration::from_secs_f64(1. / f64::from(hz));
            }
        }
        loops.push(turn.elapsed().as_micros() as u64);
        if !issuing {
            if mounted {
                let values = peers.first().and_then(|p| p.values().ok());
                converged = values.is_some()
                    && peers
                        .iter_mut()
                        .all(|p| !p.failed && p.client.confirmed() && p.values().ok() == values);
            }
            if converged || start.elapsed() > Duration::from_secs(seconds + 30) {
                if converged {
                    convergence_us = Some(
                        start
                            .elapsed()
                            .saturating_sub(Duration::from_secs(seconds))
                            .as_micros() as u64,
                    );
                }
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let sent: u64 = peers.iter().map(|p| p.sent).sum();
    let received: u64 = peers.iter().map(|p| p.received).sum();
    let samples = peers.iter().flat_map(|p| p.acks.iter().copied()).collect();
    println!(
        "{}",
        serde_json::json!({"requested_clients":count,"connected":peers.len(),"mounted":mounted,"active":active,"requested_hz_per_active":hz,"requested_seconds":seconds,"setup_seconds":setup_seconds,"measure_start_unix_ms":measure_start_unix_ms,"measure_end_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis(),"measured_seconds":start.elapsed().as_secs_f64(),"errors":errors,"failed_peers":peers.iter().filter(|p|p.failed).count(),"behavior_events":peers.iter().map(|p|p.cycles*3).sum::<u64>(),"confirmed_cycles":peers.iter().map(|p|p.acks.len()).sum::<usize>(),"ack_us":quantiles(samples),"generator_loop_us":quantiles(loops),"max_issued_age_ms":peers.iter().map(|p|p.issued_age).max(),"sent_canvas_payload_bytes":sent-before_sent,"received_canvas_payload_bytes":received-before_received,"receive_packets":peers.iter().map(|p|p.received_packets).sum::<u64>()-before_packets,"converged":converged,"settle_after_stop_us":convergence_us,"final_values":peers.first().and_then(|p|p.values().ok())})
    );
    Ok(())
}
