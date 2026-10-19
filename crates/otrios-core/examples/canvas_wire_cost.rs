// SPDX-License-Identifier: LGPL-3.0-only
//! Diagnose opaque binary vector amplification; this does not change the deployed protocol.
use otrios_core::canvas::{encode_server, encode_state_response};
use otrios_interfaces::canvas::{ServerMessage, StateResponse};
use serde::{Serialize, Serializer};
fn encode_blob<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(value)
}
#[derive(Serialize)]
enum BlobState {
    Update {
        #[serde(serialize_with = "encode_blob")]
        frame: Vec<u8>,
    },
}
#[derive(Serialize)]
enum BlobServer {
    Sync {
        epoch: u32,
        #[serde(serialize_with = "encode_blob")]
        bytes: Vec<u8>,
    },
}
fn main() {
    for length in [128, 256, 1024, 2048, 4096, 16384] {
        let frame: Vec<u8> = (0..length).map(|i| (i % 256) as u8).collect();
        let current_inner = encode_state_response(&StateResponse::Update {
            frame: frame.clone(),
        })
        .unwrap();
        let current = encode_server(&ServerMessage::Sync {
            epoch: 1,
            bytes: current_inner.clone(),
        })
        .unwrap();
        let blob_inner = flexbuffers::to_vec(BlobState::Update { frame }).unwrap();
        let blob = flexbuffers::to_vec(BlobServer::Sync {
            epoch: 1,
            bytes: blob_inner.clone(),
        })
        .unwrap();
        println!(
            "{{\"raw_frame_bytes\":{length},\"current_inner_bytes\":{},\"current_outer_bytes\":{},\"blob_inner_bytes\":{},\"blob_outer_bytes\":{}}}",
            current_inner.len(),
            current.len(),
            blob_inner.len(),
            blob.len()
        );
    }
}
