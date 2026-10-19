// SPDX-License-Identifier: LGPL-3.0-only
//! Negotiated opaque byte envelopes. Canonical state/UI encodings are unchanged.
use super::*;
use serde::Serialize;

#[derive(Serialize)]
struct Opaque<'a>(#[serde(serialize_with = "blob")] &'a [u8]);
fn blob<S: serde::Serializer>(data: &&[u8], serializer: S) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_bytes(data)
}
#[derive(Serialize)]
enum Response<'a> {
    Mount {
        authority: u64,
        lease: u64,
        frame: Opaque<'a>,
    },
    Update {
        frame: Opaque<'a>,
    },
}
#[derive(Serialize)]
enum Message<'a> {
    View {
        epoch: u32,
        bytes: Opaque<'a>,
    },
    Mounted {
        epoch: u32,
        ui: Opaque<'a>,
        ui_hash: Opaque<'a>,
        code: Opaque<'a>,
        state: Opaque<'a>,
    },
    Sync {
        epoch: u32,
        bytes: Opaque<'a>,
    },
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let bytes = flexbuffers::to_vec(value).map_err(err)?;
    check(&bytes)?;
    Ok(bytes)
}
pub fn encode_state_response_binary(value: &StateResponse) -> Result<Vec<u8>> {
    encode(&match value {
        StateResponse::Mount {
            authority,
            lease,
            frame,
        } => Response::Mount {
            authority: *authority,
            lease: *lease,
            frame: Opaque(frame),
        },
        StateResponse::Update { frame } => Response::Update {
            frame: Opaque(frame),
        },
    })
}
/// Call only after the client opts in on this message's mount epoch.
pub fn encode_server_binary(value: &ServerMessage) -> Result<Vec<u8>> {
    let message = match value {
        ServerMessage::View { epoch, bytes } => Message::View {
            epoch: *epoch,
            bytes: Opaque(bytes),
        },
        ServerMessage::Mounted {
            epoch,
            ui,
            ui_hash,
            code,
            state,
        } => Message::Mounted {
            epoch: *epoch,
            ui: Opaque(ui),
            ui_hash: Opaque(ui_hash),
            code: Opaque(code),
            state: Opaque(state),
        },
        ServerMessage::Sync { epoch, bytes } => Message::Sync {
            epoch: *epoch,
            bytes: Opaque(bytes),
        },
        _ => return encode_server(value),
    };
    encode(&message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_blobs_preserve_bytes_and_avoid_vector_width_growth() {
        for length in [0, 128, 255, 256, 2048, 16384] {
            let frame: Vec<u8> = (0..length).map(|i| (i % 256) as u8).collect();
            let response = StateResponse::Update {
                frame: frame.clone(),
            };
            for bytes in [
                encode_state_response(&response).unwrap(),
                encode_state_response_binary(&response).unwrap(),
            ] {
                for outer in [encode_server, encode_server_binary] {
                    let wire = outer(&ServerMessage::Sync {
                        epoch: 7,
                        bytes: bytes.clone(),
                    })
                    .unwrap();
                    let ServerMessage::Sync { epoch, bytes } = decode_server(&wire).unwrap() else {
                        panic!()
                    };
                    assert_eq!(epoch, 7);
                    let StateResponse::Update { frame: actual } =
                        decode_state_response(&bytes).unwrap()
                    else {
                        panic!()
                    };
                    assert_eq!(actual, frame);
                }
            }
            let wire = encode_server_binary(&ServerMessage::Sync {
                epoch: 7,
                bytes: encode_state_response_binary(&response).unwrap(),
            })
            .unwrap();
            assert!(wire.len() < length + 200);
        }
    }
    #[test]
    fn missing_capability_defaults_to_legacy_and_false_does_not_change_mount_bytes() {
        #[derive(Serialize, serde::Deserialize)]
        enum Legacy {
            Mount {
                epoch: u32,
                ui_ref: String,
                state_ref: String,
                catalog_hash: Vec<u8>,
                known_code: Vec<Vec<u8>>,
                writer: Vec<u8>,
                resume: u64,
            },
        }
        let old = flexbuffers::to_vec(Legacy::Mount {
            epoch: 1,
            ui_ref: "ui".into(),
            state_ref: "state".into(),
            catalog_hash: vec![1; 32],
            known_code: vec![],
            writer: vec![2; 16],
            resume: 0,
        })
        .unwrap();
        let value = decode_client(&old).unwrap();
        assert!(matches!(value, ClientMessage::Mount { binary: false, .. }));
        assert_eq!(old, encode_client(&value).unwrap());
        let mut advertised = value;
        if let ClientMessage::Mount { binary, .. } = &mut advertised {
            *binary = true;
        }
        // The old contract ignores the added opt-in and continues with vectors.
        let _: Legacy = flexbuffers::from_slice(&encode_client(&advertised).unwrap()).unwrap();
    }
    #[test]
    fn blobs_are_bounded_and_truncated_data_is_rejected() {
        assert!(
            encode_server_binary(&ServerMessage::View {
                epoch: 1,
                bytes: vec![0; MAX_BYTES]
            })
            .is_err()
        );
        let bytes = encode_server_binary(&ServerMessage::Mounted {
            epoch: 1,
            ui: vec![5; 256],
            ui_hash: vec![3; 32],
            code: vec![7; 1024],
            state: vec![9; 256],
        })
        .unwrap();
        let ServerMessage::Mounted {
            ui, code, state, ..
        } = decode_server(&bytes).unwrap()
        else {
            panic!()
        };
        assert_eq!(ui, vec![5; 256]);
        assert_eq!(code, vec![7; 1024]);
        assert_eq!(state, vec![9; 256]);
        assert!(decode_server(&bytes[..bytes.len() - 1]).is_err());
        for end in 0..bytes.len() {
            let _ = decode_server(&bytes[..end]);
        }
    }
}
