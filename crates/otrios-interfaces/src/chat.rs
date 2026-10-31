// SPDX-License-Identifier: Apache-2.0
//! Bounded shared-room contracts. Guest resume secrets never confer moderation.
use serde::{Deserialize, Serialize};

pub const CHANNEL: u8 = 3;
pub const MAX_COMMAND_BYTES: usize = 384 * 1024;
pub const MAX_EVENT_BYTES: usize = 384 * 1024;
pub const AVATARS: u8 = 64;
/// Admitted Brass Relay message collection; identity is not permission.
pub const COLLECTION: u64 = 22;
pub const MESSAGE_FIELD: u64 = 1;
pub const MESSAGE_SCHEMA: &[u8] = b"otrios-brass-relay-message-v2";
/// Immutable owned attachment metadata. A digest is neither a URL nor a grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
    pub bytes: u32,
}
pub const MAX_ASSET_BYTES: usize = 256 * 1024;
pub const MAX_MESSAGE_ASSETS: usize = 4;
pub const MAX_MESSAGE_BYTES: usize = 8192;

pub const MAX_INITIAL_BYTES: usize = 256 * 1024;

/// Qualified first transcript window. No authority identity, guest secret,
/// presence admission or writable collection session is conveyed by this view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initial {
    pub version: u32,
    pub members: Vec<Member>,
    pub messages: Vec<InitialMessage>,
    pub older: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialMessage {
    pub message: Message,
    pub revision: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub avatar: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub profile: Profile,
    pub generation: u64,
    pub admin: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Message,
    Action,
    Notice,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
    pub sequence: u64,
    pub at: u64,
    pub sender: Profile,
    pub kind: Kind,
    pub text: String,
    pub nonce: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    Join {
        credential: Option<String>,
    },
    Leave,
    Profile {
        name: String,
        avatar: u8,
    },
    Send {
        epoch: u64,
        nonce: String,
        text: String,
    },
    SendAsset {
        epoch: u64,
        nonce: String,
        text: String,
        name: String,
        #[serde(with = "attachment_bytes")]
        bytes: Vec<u8>,
    },
    /// Explicit copy of a reviewed confirmed dispatch into another Space.
    Publish {
        epoch: u64,
        nonce: String,
        sequence: u64,
        revision: [u8; 32],
        destination_space: String,
        expected_generation: u64,
    },
    ReadAsset {
        sequence: u64,
        revision: [u8; 32],
        index: u8,
    },
    Redact {
        sequence: u64,
        revision: [u8; 32],
    },
    ClaimDraft,
    Draft {
        generation: u64,
        revision: u64,
        text: String,
    },
    ClearDraft {
        generation: u64,
        revision: u64,
    },
    Kick {
        participant: String,
        generation: u64,
        reason: String,
    },
    Authenticate {
        username: String,
        password: String,
    },
    SignOut,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    Published {
        nonce: String,
        sequence: u64,
    },
    Redacted {
        sequence: u64,
    },
    Asset {
        sequence: u64,
        revision: [u8; 32],
        asset: Asset,
        #[serde(with = "attachment_bytes")]
        bytes: Vec<u8>,
    },

    Left,
    Welcome {
        you: Member,
        temporary: bool,
        credential: Option<String>,
        epoch: u64,
        members: Vec<Member>,
    },
    Members {
        members: Vec<Member>,
    },
    Receipt {
        nonce: String,
        sequence: u64,
    },
    DraftLease {
        generation: u64,
    },
    Draft {
        participant: String,
        generation: u64,
        revision: u64,
        text: String,
        expires: u64,
    },
    Effect {
        participant: String,
        kind: String,
        target: Option<String>,
    },
    Permissions {
        admin: bool,
    },
    Kicked {
        reason: String,
        rejoin_at: u64,
    },
    Error {
        message: String,
        nonce: Option<String>,
    },
    Resync,
}

// Binary attachments are one bounded blob rather than a vector of scalar wire
// values. JSON SDK clients may still supply an ordinary byte array.
mod attachment_bytes {
    use super::MAX_ASSET_BYTES;
    use serde::{
        Deserializer, Serializer,
        de::{self, SeqAccess, Visitor},
    };
    use std::fmt;
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        if bytes.len() > MAX_ASSET_BYTES {
            return Err(serde::ser::Error::custom("Attachment byte limit"));
        }
        serializer.serialize_bytes(bytes)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        struct Bytes;
        impl<'de> Visitor<'de> for Bytes {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded attachment bytes")
            }
            fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Vec<u8>, E> {
                if value.len() > MAX_ASSET_BYTES {
                    return Err(E::custom("Attachment byte limit"));
                }
                Ok(value.to_vec())
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Vec<u8>, A::Error> {
                if sequence.size_hint().is_some_and(|n| n > MAX_ASSET_BYTES) {
                    return Err(de::Error::custom("Attachment byte limit"));
                }
                let mut bytes =
                    Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_ASSET_BYTES));
                while let Some(byte) = sequence.next_element::<u8>()? {
                    if bytes.len() == MAX_ASSET_BYTES {
                        return Err(de::Error::custom("Attachment byte limit"));
                    }
                    bytes.push(byte);
                }
                Ok(bytes)
            }
        }
        deserializer.deserialize_bytes(Bytes)
    }
}
