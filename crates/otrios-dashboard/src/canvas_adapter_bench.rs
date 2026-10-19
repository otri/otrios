// SPDX-License-Identifier: Apache-2.0
//! Bounded, headless benchmark of the real Canvas app protocol; no durable client files.
#[path = "session_client.rs"]
mod session_client;
use otrios_core::canvas::{Client, Result};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};
struct Peer {
    client: Client,
    ws: WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    sent: u64,
    received: u64,
    sent_ws: u64,
    received_ws: u64,
}
fn wire(n: usize, masked: bool) -> u64 {
    n as u64
        + if n < 126 {
            2
        } else if n <= 65535 {
            4
        } else {
            10
        }
        + if masked { 4 } else { 0 }
}
impl Peer {
    fn new(endpoint: &str, catalog: &[u8]) -> Result<Self> {
        let (mut ws, _) = tungstenite::connect(endpoint).map_err(|e| e.to_string())?;
        if let MaybeTlsStream::Plain(tcp) = ws.get_mut() {
            tcp.set_nonblocking(true).map_err(|e| e.to_string())?;
            tcp.set_nodelay(true).map_err(|e| e.to_string())?;
        } else {
            return Err("Use a loopback endpoint/SSH forward for controlled transport".into());
        }
        session_client::negotiate(&mut ws)?;
        let mut client = Client::default();
        client.set_legacy_transport(std::env::var_os("OTRIOS_BENCH_LEGACY_TRANSPORT").is_some());
        client.admit_catalog(catalog)?;
        client.open("app:three-d-lab/ui", "app:three-d-lab/state");
        Ok(Self {
            client,
            ws,
            sent: 0,
            received: 0,
            sent_ws: 0,
            received_ws: 0,
        })
    }
    fn pump(&mut self, now: u64) -> Result<()> {
        self.client
            .tick(now, otrios_core::dashboard::signal::wall_time_seconds())?;
        while let Some(b) = self.client.outgoing()? {
            self.sent += (b.len() + 1) as u64;
            self.sent_ws += wire(b.len() + 1, true);
            self.ws
                .send(Message::Binary(
                    otrios_interfaces::session::frame(1, b).into(),
                ))
                .map_err(|e| e.to_string())?;
        }
        match self.ws.read() {
            Ok(Message::Binary(b)) => {
                self.received += b.len() as u64;
                self.received_ws += wire(b.len(), false);
                self.client
                    .receive(b.strip_prefix(&[1]).ok_or("State channel required")?)?;
            }
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
    fn counters(&self) -> [u64; 4] {
        [self.sent, self.received, self.sent_ws, self.received_ws]
    }
}
fn quantiles(values: &mut [u64]) -> serde_json::Value {
    values.sort_unstable();
    let at = |q: f64| {
        values[((values.len() as f64 * q).ceil() as usize)
            .saturating_sub(1)
            .min(values.len() - 1)]
    };
    serde_json::json!({"p50": at(0.5), "p95": at(0.95), "p99": at(0.99), "max": values[values.len()-1]})
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 7 {
        return Err(
            "Usage: canvas-adapter-bench LOOPBACK_WS CATALOG UPDATES HZ WARMUP paired|writer"
                .into(),
        );
    }
    let endpoint = &args[1];
    if !endpoint.starts_with("ws://127.0.0.1:") {
        return Err("Benchmark requires loopback (SSH-forward remote authorities)".into());
    }
    let updates = args[3].parse::<usize>().map_err(|e| e.to_string())?;
    let hz = args[4].parse::<u32>().map_err(|e| e.to_string())?;
    let warmup = args[5].parse::<usize>().map_err(|e| e.to_string())?;
    if updates == 0
        || updates + warmup > 10000
        || hz == 0
        || hz > 60
        || !["paired", "writer"].contains(&args[6].as_str())
    {
        return Err("Benchmark bounds: 1..10000 updates, 1..60 Hz".into());
    }
    let catalog = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    let mut writer = Peer::new(endpoint, &catalog)?;
    let mut observer = if args[6] == "paired" {
        Some(Peer::new(endpoint, &catalog)?)
    } else {
        None
    };
    let start = Instant::now();
    loop {
        writer.pump(start.elapsed().as_millis() as u64)?;
        if let Some(p) = &mut observer {
            p.pump(start.elapsed().as_millis() as u64)?;
        }
        if !writer.client.ui_ref().is_empty()
            && writer.client.confirmed()
            && observer
                .as_mut()
                .is_none_or(|p| !p.client.ui_ref().is_empty() && p.client.confirmed())
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(20) {
            return Err("Mount timed out".into());
        }
    }
    let node = writer
        .client
        .scene()?
        .controls()
        .iter()
        .find(|c| {
            c.node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Tetrahedron count")
        })
        .map(|c| c.node.id)
        .ok_or("Missing count control")?;
    let initial_bytes = writer.counters();
    let mut before_writer = [0; 4];
    let mut before_observer = [0; 4];
    let mut ack_us = Vec::new();
    let mut observe_us = Vec::new();
    let mut measured_start = Instant::now();
    let interval = Duration::from_secs_f64(1. / f64::from(hz));
    let mut next = Instant::now();
    for index in 0..updates + warmup {
        while Instant::now() < next {
            writer.pump(start.elapsed().as_millis() as u64)?;
            if let Some(p) = &mut observer {
                p.pump(start.elapsed().as_millis() as u64)?;
            }
        }
        if index == warmup {
            before_writer = writer.counters();
            before_observer = observer.as_ref().map_or([0; 4], Peer::counters);
            measured_start = Instant::now();
        }
        let value = ((index * 37 + 19) % 997 + 1) as u16;
        let issue = Instant::now();
        next = issue + interval;
        writer
            .client
            .event_for_node(writer.client.epoch(), node, value, 0)?;
        let mut ack = None;
        let mut observed = None;
        loop {
            writer.pump(start.elapsed().as_millis() as u64)?;
            if let Some(p) = &mut observer {
                p.pump(start.elapsed().as_millis() as u64)?;
                if observed.is_none() && p.client.value(3)? == value {
                    observed = Some(issue.elapsed().as_micros() as u64);
                }
            }
            if ack.is_none() && writer.client.confirmed() && writer.client.value(3)? == value {
                ack = Some(issue.elapsed().as_micros() as u64);
            }
            if ack.is_some() && (observer.is_none() || observed.is_some()) {
                break;
            }
            if issue.elapsed() > Duration::from_secs(10) {
                return Err(format!("Update {index} timed out"));
            }
        }
        if index >= warmup {
            ack_us.push(ack.unwrap());
            if let Some(us) = observed {
                observe_us.push(us);
            }
        }
    }
    let elapsed = measured_start.elapsed().as_secs_f64();
    let difference = |after: [u64; 4], before: [u64; 4]| {
        (0..4).map(|i| after[i] - before[i]).collect::<Vec<_>>()
    };
    let mut report = serde_json::json!({"mode":args[6],"updates":updates,"requested_hz":hz,"warmup":warmup,"seconds":elapsed,"observed_hz":updates as f64/elapsed,"ack_us":quantiles(&mut ack_us),"writer_bytes":difference(writer.counters(),before_writer),"initial_writer_bytes":initial_bytes,"byte_order":["sent_payload","received_payload","sent_ws","received_ws"]});
    if let Some(p) = observer {
        report["observer_bytes"] = serde_json::json!(difference(p.counters(), before_observer));
        report["observe_us"] = quantiles(&mut observe_us);
    }
    println!("{report}");
    Ok(())
}
