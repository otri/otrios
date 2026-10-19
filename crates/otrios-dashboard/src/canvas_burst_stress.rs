// SPDX-License-Identifier: Apache-2.0
//! External complete-protocol clients: real pointer gestures, Canvas state and ink.
//! Use only against a disposable authority; gestures deliberately change app state.
#[path = "session_client.rs"]
mod session_client;
use otrios_core::canvas::{Client, Result};
use otrios_interfaces::{
    ink,
    scene::{PointerEvent, PointerPhase, Rect},
    session::{self, ClientControl},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};

type Key = (u32, u32, u8, [u16; 2]);
struct Delivery {
    sent: Instant,
    seen: Vec<bool>,
}
struct Ledger {
    points: BTreeMap<Key, Delivery>,
    latencies: Vec<u64>,
    duplicates: u64,
    unknown: u64,
    recipients: usize,
    delivered: u64,
}
impl Ledger {
    fn sent(&mut self, peer: u32, bytes: &[u8]) -> Result<()> {
        for bytes in bytes.chunks_exact(ink::CLIENT_BYTES) {
            let p = ink::Point::decode(bytes)?;
            let key = (peer, p.stroke, p.phase, p.position);
            if self
                .points
                .insert(
                    key,
                    Delivery {
                        sent: Instant::now(),
                        seen: vec![false; self.recipients],
                    },
                )
                .is_some()
            {
                return Err("Duplicate outgoing ink point key".into());
            }
        }
        Ok(())
    }
    fn received(&mut self, receiver: usize, own: u32, bytes: &[u8]) -> Result<Option<u32>> {
        let mut welcome = None;
        for bytes in bytes.chunks_exact(ink::SERVER_BYTES) {
            let (sender, _, point) = ink::decode_server(bytes)?;
            let Some(p) = point else {
                welcome = Some(sender);
                continue;
            };
            if sender == own {
                continue;
            }
            if let Some(delivery) = self
                .points
                .get_mut(&(sender, p.stroke, p.phase, p.position))
            {
                if delivery.seen[receiver] {
                    self.duplicates += 1;
                } else {
                    delivery.seen[receiver] = true;
                    self.delivered += 1;
                    self.latencies
                        .push(delivery.sent.elapsed().as_micros() as u64);
                }
            } else {
                self.unknown += 1;
            }
        }
        Ok(welcome)
    }
    fn expected(&self) -> u64 {
        self.points.len() as u64 * self.recipients.saturating_sub(1) as u64
    }
}
struct Peer {
    client: Client,
    ws: WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    id: u32,
    controls: [Rect; 3],
    kind: usize,
    phase_ms: u64,
    begun: bool,
    released: bool,
    moves: u64,
    skipped: u64,
    next_sample: u64,
    release: Option<Instant>,
    release_us: Option<u64>,
    blocked: bool,
    failed: bool,
    sent: u64,
    received: u64,
}
fn blocked(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}
impl Peer {
    fn new(endpoint: &str, catalog: &[u8], kind: usize, phase_ms: u64) -> Result<Self> {
        let mut request = endpoint.into_client_request().map_err(|e| e.to_string())?;
        request
            .headers_mut()
            .insert("Origin", "https://otrios.otri.ca".parse().unwrap());
        let (mut ws, _) = tungstenite::connect(request).map_err(|e| e.to_string())?;
        let MaybeTlsStream::Plain(tcp) = ws.get_mut() else {
            return Err("Controlled plain Tailnet/loopback transport required".into());
        };
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        tcp.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        tcp.set_nodelay(true).map_err(|e| e.to_string())?;
        session_client::negotiate(&mut ws)?;
        ws.send(Message::Text(
            serde_json::to_string(&ClientControl::Ink { enabled: true })
                .unwrap()
                .into(),
        ))
        .map_err(|e| e.to_string())?;
        let MaybeTlsStream::Plain(tcp) = ws.get_mut() else {
            unreachable!()
        };
        tcp.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut client = Client::default();
        client.admit_catalog(catalog)?;
        client.open(
            "app:component-playground/ui",
            "app:component-playground/state",
        );
        Ok(Self {
            client,
            ws,
            id: 0,
            controls: [Rect::new(0., 0., 0., 0.); 3],
            kind,
            phase_ms,
            begun: false,
            released: false,
            moves: 0,
            skipped: 0,
            next_sample: 0,
            release: None,
            release_us: None,
            blocked: false,
            failed: false,
            sent: 0,
            received: 0,
        })
    }
    fn send(&mut self, message: Message) -> Result<()> {
        match self.ws.send(message) {
            Ok(()) => Ok(()),
            Err(e) if blocked(&e) => {
                self.blocked = true;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }
    fn pump(&mut self, now: u64, index: usize, ledger: &mut Ledger) -> Result<()> {
        self.client
            .tick(now, otrios_core::dashboard::clock::wall_time_seconds())?;
        match self.ws.flush() {
            Ok(()) => self.blocked = false,
            Err(e) if blocked(&e) => self.blocked = true,
            Err(e) => return Err(e.to_string()),
        }
        if !self.blocked {
            for _ in 0..4 {
                let Some(bytes) = self.client.outgoing()? else {
                    break;
                };
                self.sent += bytes.len() as u64 + 1;
                self.send(Message::Binary(
                    session::frame(session::STATE, bytes).into(),
                ))?;
                if self.blocked {
                    break;
                }
            }
        }
        if self.id != 0 && !self.blocked {
            let bytes = self.client.ink_outgoing(now);
            if !bytes.is_empty() {
                ledger.sent(self.id, &bytes)?;
                self.sent += bytes.len() as u64 + 1;
                self.send(Message::Binary(session::frame(session::INK, bytes).into()))?;
            }
        }
        for _ in 0..64 {
            match self.ws.read() {
                Ok(Message::Binary(bytes)) => {
                    self.received += bytes.len() as u64;
                    match bytes.split_first() {
                        Some((&session::STATE, bytes)) => {
                            self.client.receive(bytes)?;
                        }
                        Some((&session::INK, bytes)) => {
                            if bytes.len() % ink::SERVER_BYTES != 0 {
                                return Err("Invalid ink batch length".into());
                            }
                            if let Some(id) = ledger.received(index, self.id, bytes)? {
                                self.id = id;
                            }
                            self.client.receive_ink(bytes, now)?;
                        }
                        _ => return Err("Unexpected session channel".into()),
                    }
                }
                Ok(Message::Close(_)) => {
                    return Err("Server closed the complete-client session".into());
                }
                Ok(Message::Text(text)) => {
                    session_client::decode(&text)?;
                }
                Ok(_) => {}
                Err(e) if blocked(&e) => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        if let Some(release) = self.release
            && self.release_us.is_none()
            && self.client.confirmed()
        {
            self.release_us = Some(release.elapsed().as_micros() as u64);
        }
        Ok(())
    }
    fn locate(&mut self) -> Result<()> {
        let scene = self.client.scene()?;
        for (slot, label) in ["Linear slider", "Rotary dial", "Touch pad"]
            .iter()
            .enumerate()
        {
            let c = scene
                .controls()
                .into_iter()
                .find(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == *label))
                .ok_or("Missing Playground control")?;
            self.controls[slot] = Rect::new(
                c.origin[0],
                c.origin[1],
                c.node.frame.width * c.scale,
                c.node.frame.height * c.scale,
            );
        }
        Ok(())
    }
    fn pointer(&mut self, phase: PointerPhase, fraction: f32) -> Result<()> {
        let rect = self.controls[self.kind];
        let [x, y] = if self.kind == 1 {
            let a = fraction * std::f32::consts::TAU;
            [0.5 + 0.35 * a.sin(), 0.5 - 0.35 * a.cos()]
        } else {
            [
                0.15 + 0.7 * fraction,
                if self.kind == 0 {
                    0.5
                } else {
                    0.2 + 0.6 * fraction
                },
            ]
        };
        self.client.pointer(
            self.client.epoch(),
            PointerEvent {
                id: 71,
                phase,
                position: [rect.x + x * rect.width, rect.y + y * rect.height],
            },
        )?;
        if phase == PointerPhase::Down && !self.client.input_busy() {
            return Err("Pointer did not capture the expected control".into());
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
}
fn quantiles(mut samples: Vec<u64>) -> serde_json::Value {
    if samples.is_empty() {
        return serde_json::Value::Null;
    }
    samples.sort_unstable();
    let at = |p: f64| samples[((samples.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    serde_json::json!({"n":samples.len(),"p50":at(0.5),"p95":at(0.95),"p99":at(0.99),"max":samples.last()})
}
fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 {
        return Err("Usage: canvas-burst-stress WS CATALOG CLIENTS ACTIVE HZ SECONDS idle|sparse-control|sparse-ink|sparse-mixed|aligned-mixed|aligned-ink".into());
    }
    let count: usize = args[3]
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let active: usize = args[4]
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let hz: u64 = args[5]
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let seconds: u64 = args[6]
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let mode = args[7].as_str();
    let sparse = mode.starts_with("sparse-");
    let endpoint_ok = args[1]
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
    if !endpoint_ok
        || !(2..=512).contains(&count)
        || active > count
        || !(1..=30).contains(&hz)
        || !(3..=120).contains(&seconds)
        || (sparse && seconds != 100)
        || ![
            "idle",
            "sparse-control",
            "sparse-ink",
            "sparse-mixed",
            "aligned-mixed",
            "aligned-ink",
        ]
        .contains(&mode)
    {
        return Err("Use controlled IPv4 loopback/Tailnet, 2..512 clients, <=clients active, 1..30 Hz, 3..120 seconds; sparse trials require 100 seconds".into());
    }
    let catalog = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    let origin = Instant::now();
    let mut ledger = Ledger {
        points: BTreeMap::new(),
        latencies: vec![],
        duplicates: 0,
        unknown: 0,
        recipients: count,
        delivered: 0,
    };
    let mut peers = Vec::<Peer>::new();
    let mut seed = 0x4f54_7269_4f53_2026u64;
    for i in 0..count {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let kind = if mode.ends_with("ink") {
            2
        } else if mode.ends_with("control") {
            i % 2
        } else {
            i % 3
        };
        let phase = if mode == "idle" || (!sparse && i >= active) {
            u64::MAX
        } else if sparse {
            seed % 99_001
        } else {
            1000
        };
        peers.push(Peer::new(&args[1], &catalog, kind, phase)?);
        for (j, peer) in peers.iter_mut().enumerate() {
            peer.pump(origin.elapsed().as_millis() as u64, j, &mut ledger)?;
        }
    }
    let until = Instant::now() + Duration::from_secs(90);
    while peers
        .iter_mut()
        .any(|p| p.id == 0 || p.client.ui_ref().is_empty() || !p.client.confirmed())
    {
        for (j, p) in peers.iter_mut().enumerate() {
            p.pump(origin.elapsed().as_millis() as u64, j, &mut ledger)?;
        }
        if Instant::now() > until {
            return Err("Complete-client mount/ink welcome deadline".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for p in &mut peers {
        p.locate()?;
    }
    let setup_seconds = origin.elapsed().as_secs_f64();
    let before_sent: u64 = peers.iter().map(|p| p.sent).sum();
    let before_received: u64 = peers.iter().map(|p| p.received).sum();
    let start = Instant::now();
    let measure_start_unix_ms = unix_ms();
    let mut errors = vec![];
    let mut loops = vec![];
    let mut concurrency = vec![];
    let mut next_concurrency_ms = 0;
    let mut last_release: Option<Instant> = None;
    let mut state_settle_us = None;
    let mut converged = false;
    loop {
        let turn = Instant::now();
        let elapsed_ms = start.elapsed().as_millis() as u64;
        for (i, p) in peers.iter_mut().enumerate() {
            if p.failed {
                continue;
            }
            let action = (|| -> Result<()> {
                p.pump(origin.elapsed().as_millis() as u64, i, &mut ledger)?;
                if p.phase_ms == u64::MAX {
                    return Ok(());
                }
                if !p.begun && elapsed_ms >= p.phase_ms {
                    p.pointer(PointerPhase::Down, 0.)?;
                    p.begun = true;
                }
                if p.begun && !p.released {
                    let progress = elapsed_ms.saturating_sub(p.phase_ms);
                    if progress < 1000 {
                        let due = progress * hz / 1000;
                        if due >= p.next_sample {
                            p.skipped += due - p.next_sample;
                            p.pointer(PointerPhase::Move, (due + 1) as f32 / hz as f32)?;
                            p.moves += 1;
                            p.next_sample = due + 1;
                        }
                    } else {
                        p.skipped += hz.saturating_sub(p.next_sample);
                        p.pointer(PointerPhase::Up, 1.)?;
                        p.released = true;
                        p.release = Some(Instant::now());
                        last_release = p.release;
                        state_settle_us = None;
                    }
                }
                Ok(())
            })();
            if let Err(e) = action {
                p.failed = true;
                if errors.len() < 20 {
                    errors.push(format!("client {i}: {e}"));
                }
            }
        }
        if elapsed_ms >= next_concurrency_ms && elapsed_ms < seconds * 1000 {
            concurrency.push(
                peers
                    .iter()
                    .filter(|p| p.begun && !p.released && !p.failed)
                    .count() as u64,
            );
            next_concurrency_ms += 100;
        }
        let all_finished = peers.iter().all(|p| p.phase_ms == u64::MAX || p.released);
        if all_finished {
            let values = peers.first().and_then(|p| p.values().ok());
            converged = values.is_some()
                && peers
                    .iter_mut()
                    .all(|p| !p.failed && p.client.confirmed() && p.values().ok() == values);
            if converged && state_settle_us.is_none() {
                state_settle_us =
                    Some(last_release.map_or(0, |at| at.elapsed().as_micros() as u64));
            }
        }
        loops.push(turn.elapsed().as_micros() as u64);
        if elapsed_ms >= seconds * 1000
            && ((converged && ledger.delivered == ledger.expected())
                || elapsed_ms > (seconds + 30) * 1000)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let releases: Vec<_> = peers.iter().filter_map(|p| p.release_us).collect();
    let release_us = quantiles(releases.clone());
    let ink_us = quantiles(ledger.latencies.clone());
    let moves: u64 = peers.iter().map(|p| p.moves).sum();
    let bursts = peers.iter().filter(|p| p.begun).count();
    let expected_moves = bursts as u64 * hz;
    let missing = ledger.expected().saturating_sub(ledger.delivered);
    let ink_bursts = peers.iter().filter(|p| p.kind == 2 && p.begun).count();
    let ink_downs = ledger.points.keys().filter(|k| k.2 == 0).count();
    let ink_ups = ledger.points.keys().filter(|k| k.2 == 2).count();
    let ink_moves = ledger.points.keys().filter(|k| k.2 == 1).count();
    let ink_schedule_fulfilled = ink_downs == ink_bursts
        && ink_ups == ink_bursts
        && ink_moves as u64 * 100 >= ink_bursts as u64 * hz * 95;
    let latency_ok = |q: &serde_json::Value, p: &str, bound: u64| {
        q.is_null() || q[p].as_u64().is_some_and(|v| v <= bound)
    };
    let passed = ink_schedule_fulfilled
        && errors.is_empty()
        && missing == 0
        && ledger.duplicates == 0
        && ledger.unknown == 0
        && releases.len() == bursts
        && converged
        && moves * 100 >= expected_moves * 95
        && latency_ok(&ink_us, "p95", 250_000)
        && latency_ok(&ink_us, "p99", 500_000)
        && latency_ok(&release_us, "p95", 1_000_000)
        && state_settle_us.is_some_and(|n| n <= 2_000_000);
    println!(
        "{}",
        serde_json::json!({"mode":mode,"requested_clients":count,"connected":peers.len(),"ink_subscribed":peers.iter().filter(|p|p.id!=0).count(),"requested_hz":hz,"requested_seconds":seconds,"measure_start_unix_ms":measure_start_unix_ms,"measure_end_unix_ms":unix_ms(),"setup_seconds":setup_seconds,"measured_seconds":start.elapsed().as_secs_f64(),"seed":"4f5472694f532026","input_duty_per_client":if sparse {Some(0.01)}else{None},"bursts":bursts,"offered_moves":moves,"expected_moves":expected_moves,"skipped_moves":peers.iter().map(|p|p.skipped).sum::<u64>(),"concurrency":quantiles(concurrency),"ink_bursts":ink_bursts,"ink_moves_sent":ink_moves,"ink_downs_sent":ink_downs,"ink_ups_sent":ink_ups,"ink_schedule_fulfilled":ink_schedule_fulfilled,"ink_points_sent":ledger.points.len(),"ink_expected_deliveries":ledger.expected(),"ink_deliveries":ledger.delivered,"ink_missing_deliveries":missing,"ink_duplicate_deliveries":ledger.duplicates,"ink_unknown_deliveries":ledger.unknown,"ink_delivery_us":ink_us,"release_confirmation_us":release_us,"confirmed_bursts":releases.len(),"converged":converged,"state_settle_after_last_release_us":state_settle_us,"generator_loop_us":quantiles(loops),"sent_session_payload_bytes":peers.iter().map(|p|p.sent).sum::<u64>()-before_sent,"received_session_payload_bytes":peers.iter().map(|p|p.received).sum::<u64>()-before_received,"failed_peers":peers.iter().filter(|p|p.failed).count(),"errors":errors,"client_gates_passed":passed})
    );
    Ok(())
}
