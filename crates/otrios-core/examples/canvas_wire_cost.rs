// SPDX-License-Identifier: LGPL-3.0-only
//! Matched canonical-frame codec costs; excludes network, tree assembly and fsync.
use otrios_core::canvas::*;
use otrios_interfaces::{canvas::*, layout::Value};
use serde::{Serialize, Serializer};
use std::{hint::black_box, time::Instant};
fn blob<S: Serializer>(value: &&[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_bytes(value)
}
#[derive(Serialize)]
struct Opaque<'a>(#[serde(serialize_with = "blob")] &'a [u8]);
#[derive(Serialize)]
enum OldState<'a> {
    Update { frame: Opaque<'a> },
}
#[derive(Serialize)]
enum OldServer<'a> {
    Sync { epoch: u32, bytes: Opaque<'a> },
}
fn legacy(frame: &[u8]) -> Vec<u8> {
    let inner = flexbuffers::to_vec(OldState::Update {
        frame: Opaque(frame),
    })
    .unwrap();
    flexbuffers::to_vec(OldServer::Sync {
        epoch: 1,
        bytes: Opaque(&inner),
    })
    .unwrap()
}
fn legacy_read(bytes: &[u8]) -> Vec<u8> {
    // The old full validator also recursively walked its envelope. This accessor
    // baseline is deliberately cheaper; do not call it the full v3 decode cost.
    let root = flexbuffers::Reader::get_root(bytes).unwrap();
    let outer = root
        .get_map()
        .unwrap()
        .index("Sync")
        .unwrap()
        .get_map()
        .unwrap();
    let inner = outer.index("bytes").unwrap().get_blob().unwrap();
    let root = flexbuffers::Reader::get_root(inner.0).unwrap();
    root.get_map()
        .unwrap()
        .index("Update")
        .unwrap()
        .get_map()
        .unwrap()
        .index("frame")
        .unwrap()
        .get_blob()
        .unwrap()
        .0
        .to_vec()
}
fn cost(mut op: impl FnMut(), count: usize) -> f64 {
    for _ in 0..20 {
        op();
    }
    let start = Instant::now();
    for _ in 0..count {
        op();
    }
    start.elapsed().as_secs_f64() * 1e6 / count as f64
}
fn frame(series: bool, update: bool) -> Vec<u8> {
    let schema = StateSchema {
        id: "codec-probe-v1".into(),
        fields: vec![Field {
            id: 1,
            name: "value".into(),
            writable: true,
            default: if series {
                Value::Series((0..2016).map(|i| (i as f32 / 31.).sin()).collect())
            } else {
                Value::Number(500.)
            },
        }],
    };
    let mut doc = Document::new(schema.clone(), "codec-probe").unwrap();
    let writer = [7; 16];
    let (mut session, _) = doc.open_session(&writer, 0).unwrap();
    let mounted = doc.generate(&mut session).unwrap().unwrap();
    let mut replica = StateAdapter::new(schema, "codec-probe", &mounted, writer).unwrap();
    while let Some(ack) = replica.generate().unwrap() {
        doc.receive(&mut session, &ack).unwrap();
    }
    let response = if update {
        doc.set(&[(1, 501)]).unwrap();
        doc.generate(&mut session).unwrap().unwrap()
    } else {
        mounted
    };
    match decode_state_response(&response).unwrap() {
        StateResponse::Mount { frame, .. } | StateResponse::Update { frame } => frame,
    }
}
fn main() {
    for repetition in 0..3 {
        for (name, frame) in [
            ("scalar-delta", frame(false, true)),
            ("scalar-mount", frame(false, false)),
            ("series-mount", frame(true, false)),
        ] {
            let value = ServerMessage::Sync {
                epoch: 1,
                bytes: encode_state_response(&StateResponse::Update {
                    frame: frame.clone(),
                })
                .unwrap(),
            };
            let mut raw = Vec::new();
            encode_server_uncompressed_into(&value, &mut raw).unwrap();
            let mut packed = Vec::new();
            let compressed = compress_packet_into(&raw, &mut packed).unwrap();
            let old = legacy(&frame);
            assert_eq!(legacy_read(&old), frame);
            assert_eq!(decode_server(&packed).unwrap(), value);
            let mut builder = Vec::new();
            let flex_encode = cost(
                || {
                    black_box(legacy(black_box(&frame)));
                },
                300,
            );
            let flex_access = cost(
                || {
                    black_box(legacy_read(black_box(&old)));
                },
                300,
            );
            let raw_encode = cost(
                || {
                    encode_server_uncompressed_into(black_box(&value), &mut builder).unwrap();
                },
                300,
            );
            let raw_decode = cost(
                || {
                    black_box(decode_server(black_box(&raw)).unwrap());
                },
                300,
            );
            let lz4_encode = cost(
                || {
                    black_box(compress_packet_into(black_box(&raw), &mut builder).unwrap());
                },
                300,
            );
            let lz4_decode = cost(
                || {
                    black_box(decode_server(black_box(&packed)).unwrap());
                },
                300,
            );
            println!(
                "{{\"repetition\":{repetition},\"case\":\"{name}\",\"canonical_frame_bytes\":{},\"flex_bytes\":{},\"packed_bytes\":{},\"lz4_bytes\":{},\"compressed\":{compressed},\"flex_encode_us\":{flex_encode:.3},\"flex_access_us\":{flex_access:.3},\"packed_encode_us\":{raw_encode:.3},\"packed_decode_us\":{raw_decode:.3},\"lz4_encode_us\":{lz4_encode:.3},\"lz4_decode_us\":{lz4_decode:.3}}}",
                frame.len(),
                old.len(),
                raw.len(),
                packed.len()
            );
        }
    }
}
