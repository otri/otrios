// SPDX-License-Identifier: Apache-2.0
//! Bounded shared-room contracts. Guest resume secrets never confer moderation.
use serde::{Deserialize, Serialize};

pub const CHANNEL: u8 = 3;
pub const MAX_COMMAND_BYTES: usize = 8192;
pub const MAX_EVENT_BYTES: usize = 192 * 1024;
pub const AVATARS: u8 = 64;
pub const PAGE_MESSAGES: usize = 64;

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
    History {
        before: u64,
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
    Left,
    Welcome {
        you: Member,
        temporary: bool,
        credential: Option<String>,
        epoch: u64,
        members: Vec<Member>,
        messages: Vec<Message>,
        oldest: u64,
        newest: u64,
    },
    Members {
        members: Vec<Member>,
    },
    Message {
        message: Message,
    },
    Receipt {
        nonce: String,
        sequence: u64,
    },
    History {
        messages: Vec<Message>,
        oldest: u64,
        before: u64,
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
