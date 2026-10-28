// SPDX-License-Identifier: Apache-2.0
//! Thin on-device loopback wire probe for disposable journal recovery trials.
//! Uses core codecs/qualification; contains no collection/runtime implementation.
use otrios_core::{canvas::chat, collection as col};
use otrios_interfaces::session::frame;
use std::{
    net::TcpStream,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};
type Socket = WebSocket<MaybeTlsStream<TcpStream>>;
const MARKER: &str = "RFC0024_ROLLBACK_CANARY_20261028";
fn next(socket: &mut Socket, channel: u8) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        match socket.read().map_err(|e| e.to_string())? {
            Message::Binary(b) if b.first() == Some(&channel) => return Ok(b[1..].to_vec()),
            Message::Ping(_) => socket.flush().map_err(|e| e.to_string())?,
            Message::Close(_) => return Err("Probe socket closed".into()),
            _ => (),
        }
    }
    Err("Probe event deadline".into())
}
fn send(socket: &mut Socket, command: chat::Command) -> Result<(), String> {
    socket
        .send(Message::Binary(
            frame(chat::CHANNEL, chat::encode_command(&command)?).into(),
        ))
        .map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let address = args.next().ok_or("Supply 127.0.0.1:PORT v6|v8 [send]")?;
    let address = address
        .parse::<std::net::SocketAddr>()
        .map_err(|e| e.to_string())?;
    if address.ip() != std::net::Ipv4Addr::LOCALHOST {
        return Err("Loopback only".into());
    }
    let protocol = args.next().ok_or("Supply v6 or v8")?;
    if !["v6", "v8"].contains(&protocol.as_str()) {
        return Err("Probe protocol".into());
    }
    let write = match args.next().as_deref() {
        None => false,
        Some("send") => true,
        _ => return Err("Probe arguments".into()),
    };
    if args.next().is_some() || write && protocol != "v8" {
        return Err("Probe arguments".into());
    }
    let (mut socket, _) =
        tungstenite::connect(format!("ws://{address}/session")).map_err(|e| e.to_string())?;
    if let MaybeTlsStream::Plain(t) = socket.get_mut() {
        t.set_read_timeout(Some(Duration::from_secs(15)))
            .map_err(|e| e.to_string())?;
    }
    let hello = serde_json::json!({"type":"hello","version":1,"release":null,"state_protocol":format!("otrios-canvas-sync-{protocol}"),"state":true});
    socket
        .send(Message::Text(hello.to_string().into()))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut greeted = false;
    let release = loop {
        if Instant::now() > deadline {
            return Err("Probe hello deadline".into());
        }
        if let Message::Text(t) = socket.read().map_err(|e| e.to_string())? {
            let control: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
            if control["type"] == "hello" {
                if control["version"] != 1 || control["compatible"] != true {
                    return Err("Probe incompatible hello".into());
                }
                greeted = true;
                if control["state_available"] == true {
                    break control["release"].clone();
                }
            } else if greeted && control["type"] == "state_status" && control["available"] == true {
                break serde_json::Value::Null;
            }
        }
    };
    send(&mut socket, chat::Command::Join { credential: None })?;
    if protocol == "v6" {
        // The current strict codec correctly rejects the retired Welcome shape.
        // Keep that body opaque and require the exact ASCII canary in its bytes.
        let bytes = next(&mut socket, chat::CHANNEL)?;
        if !bytes.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()) {
            return Err("Prior authority did not recover candidate's durable canary".into());
        }
        println!(
            "{}",
            serde_json::json!({"passed":true,"protocol":protocol,"release":release,"legacy_welcome_contains_marker":true,"sent":false})
        );
        return Ok(());
    }
    let epoch = loop {
        let bytes = next(&mut socket, chat::CHANNEL)?;
        if let chat::Event::Welcome { epoch, .. } = chat::decode_event(&bytes)? {
            break epoch;
        }
    };
    if write {
        let nonce = "00240000000000000000000000000001".to_owned();
        send(
            &mut socket,
            chat::Command::Send {
                epoch,
                nonce: nonce.clone(),
                text: MARKER.into(),
            },
        )?;
        loop {
            if let chat::Event::Receipt { nonce: n, .. } =
                chat::decode_event(&next(&mut socket, chat::CHANNEL)?)?
                && n == nonce
            {
                break;
            }
        }
    }
    let plan = chat::message_selection(col::Anchor::Tail);
    let command = col::Command::Select {
        generation: 1,
        plan: plan.clone(),
    };
    socket
        .send(Message::Binary(
            frame(
                col::CHANNEL,
                col::encode_command(&command).map_err(|e| format!("{e:?}"))?,
            )
            .into(),
        ))
        .map_err(|e| e.to_string())?;
    let view = loop {
        if let col::Event::View(v) = col::Event::from_bytes(&next(&mut socket, col::CHANNEL)?)
            .map_err(|e| format!("{e:?}"))?
        {
            break v;
        }
    };
    let mut replica = col::Replica::new(view.epoch, chat::COLLECTION, chat::message_schema());
    replica
        .begin_request(1, plan)
        .and_then(|_| replica.apply(&view))
        .map_err(|e| format!("{e:?}"))?;
    let window = replica.window().ok_or("No qualified window")?;
    let mut found = false;
    for key in &window.keys {
        found |=
            chat::read_message(&window.record(*key).map_err(|e| format!("{e:?}"))?)?.text == MARKER;
    }
    if !found {
        return Err("Recovered collection omitted durable canary".into());
    }
    println!(
        "{}",
        serde_json::json!({"passed":true,"protocol":protocol,"release":release,"sent":write,"messages":window.keys.len(),"first":window.keys.first(),"last":window.keys.last(),"collection_root":view.transfer.root.as_ref().map(|r|otrios_core::canvas::hex(r)),"marker_found":found})
    );
    Ok(())
}
