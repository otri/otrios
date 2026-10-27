// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
fn path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "otrios-chat-{}-{}.journal",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn send(text: &str, nonce: u64, now: u64) -> Command {
    Command::Send {
        epoch: now / 600_000,
        nonce: format!("{nonce:032x}"),
        text: text.into(),
    }
}
#[test]
fn renamed_authors_resolve_in_old_pages_welcome_and_recovered_history() {
    let p = path();
    let now = 1_000_000;
    let mut room = Room::open(Some(p.clone()), now).unwrap();
    let (original, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    for i in 1..=70 {
        let at = now + i * 1000;
        room.handle(1, send("Unchanged dispatch", i, at), at)
            .unwrap();
    }
    let prior_log = fs::read(&p).unwrap();
    room.handle(
        1,
        Command::Profile {
            name: "Admiral Newname".into(),
            avatar: 42,
        },
        now + 71_000,
    )
    .unwrap();
    assert!(
        fs::read(&p).unwrap().starts_with(&prior_log),
        "rename appends a profile update without rewriting message records"
    );
    room.leave(1, now + 72_000).unwrap();
    drop(room);
    let mut room = Room::open(Some(p.clone()), now + 73_000).unwrap();
    let joined = room.join(2, None, false, now + 73_000, true).unwrap();
    let welcome = joined
        .iter()
        .find_map(|d| match &d.event {
            Event::Welcome { messages, .. } => Some(messages),
            _ => None,
        })
        .unwrap();
    assert_eq!(welcome.len(), PAGE_MESSAGES);
    let history = room
        .handle(2, Command::History { before: 7 }, now + 74_000)
        .unwrap();
    let Event::History { messages, .. } = &history[0].event else {
        panic!("history")
    };
    assert_eq!(messages.len(), 6);
    for message in welcome.iter().chain(messages) {
        assert_eq!(message.sender.id, original.id);
        assert_eq!(message.sender.name, "Admiral Newname");
        assert_eq!(message.sender.avatar, 42);
        assert_eq!(message.text, "Unchanged dispatch");
    }
    fs::remove_file(p).unwrap();
}
#[test]
fn quiet_presence_and_departure_persist_utc_last_seen_and_profiles_expire_after_absence() {
    let p = path();
    let now = 1_700_000_000_000;
    let mut room = Room::open(Some(p.clone()), now).unwrap();
    let (_, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    let initial_bytes = fs::metadata(&p).unwrap().len();
    room.tick(now + 59_999).unwrap();
    assert_eq!(fs::metadata(&p).unwrap().len(), initial_bytes);
    room.tick(now + 60_000).unwrap();
    let checkpoint_bytes = fs::metadata(&p).unwrap().len();
    assert!(checkpoint_bytes > initial_bytes);
    room.tick(now + 60_001).unwrap();
    assert_eq!(fs::metadata(&p).unwrap().len(), checkpoint_bytes);
    let departure = now + 120_000;
    room.leave(1, departure).unwrap();
    assert!(room.members().is_empty());
    assert!(
        room.valid_identity(&token),
        "leaving removes presence, not the profile"
    );
    // A backwards wall-clock adjustment must not move last-seen backwards.
    room.identity(Some(&token), now).unwrap();
    drop(room);
    let (_, entries) = super::journal::Journal::open(p.clone()).unwrap();
    let seen = entries
        .iter()
        .rev()
        .find_map(|e| match e {
            super::journal::Entry::Guest(g) => Some(g.last_seen_unix_ms),
            _ => None,
        })
        .unwrap();
    assert_eq!(seen, departure);
    let day = 86_400_000;
    let retained = Room::open(Some(p.clone()), departure + 29 * day).unwrap();
    assert!(retained.valid_identity(&token));
    drop(retained);
    let expired = Room::open(Some(p.clone()), departure + 30 * day).unwrap();
    assert!(!expired.valid_identity(&token));
    drop(expired);
    fs::remove_file(p).unwrap();
}
#[test]
fn durable_receipts_survive_lost_ack_restart_and_conflicting_retries() {
    let p = path();
    let now = 1_000_000;
    let mut room = Room::open(Some(p.clone()), now).unwrap();
    let (profile, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    let frozen = room.members();
    let first = room.handle(1, send("First dispatch", 1, now), now).unwrap();
    let sequence = first
        .iter()
        .find_map(|d| {
            if let Event::Receipt { sequence, .. } = d.event {
                Some(sequence)
            } else {
                None
            }
        })
        .unwrap();
    drop(room);
    let mut room = Room::open(Some(p.clone()), now + 1).unwrap();
    room.join(2, Some(&token), false, now + 1, true).unwrap();
    let retry = room
        .handle(2, send("First dispatch", 1, now), now + 2)
        .unwrap();
    assert_eq!(retry.len(), 1);
    assert!(matches!(retry[0].event,Event::Receipt{sequence:s,..}if s==sequence));
    assert!(
        room.handle(2, send("Other payload", 1, now), now + 3)
            .is_err()
    );
    assert_eq!(room.members()[0].profile.id, profile.id);
    assert_eq!(frozen[0].profile, profile);
    assert!(
        room.handle(2, send("Expired", 2, now), now + 1_200_000)
            .is_err()
    );
    fs::remove_file(p).unwrap();
}
#[test]
fn kick_requires_live_connection_permission_removes_all_tabs_and_persists_cooldown() {
    let p = path();
    let now = 1_000_000;
    let mut room = Room::open(Some(p.clone()), now).unwrap();
    let (_, a) = room.identity(None, now).unwrap();
    let (target, b) = room.identity(None, now).unwrap();
    room.join(1, Some(&a), false, now, true).unwrap();
    room.join(2, Some(&b), false, now, true).unwrap();
    room.join(3, Some(&b), false, now, true).unwrap();
    assert_eq!(room.members().len(), 2);
    let generation = room
        .members()
        .iter()
        .find(|m| m.profile.id == target.id)
        .unwrap()
        .generation;
    let kick = Command::Kick {
        participant: target.id.clone(),
        generation,
        reason: "Give the boiler a minute".into(),
    };
    assert!(room.handle(1, kick.clone(), now).is_err());
    room.set_admin(1, true);
    room.set_admin(1, false);
    assert!(room.handle(1, kick.clone(), now).is_err());
    room.set_admin(1, true);
    let events = room.handle(1, kick, now).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|d| matches!(d.event, Event::Kicked { .. }))
            .count(),
        2
    );
    assert!(!room.active(2) && !room.active(3));
    assert!(room.active(1));
    drop(room);
    let mut room = Room::open(Some(p.clone()), now + 1).unwrap();
    assert!(matches!(
        room.join(4, Some(&b), false, now + 1, true).unwrap()[0].event,
        Event::Kicked { .. }
    ));
    room.join(4, Some(&b), false, now + 60_001, true).unwrap();
    assert!(room.active(4));
    fs::remove_file(p).unwrap();
}
#[test]
fn draft_generations_reject_stale_tabs_revisions_and_expire_without_history() {
    let now = 1_000_000;
    let mut room = Room::open(None, now).unwrap();
    let (_, token) = room.identity(None, now).unwrap();
    let joined = room.join(1, Some(&token), false, now, true).unwrap();
    let generation = joined
        .iter()
        .find_map(|d| {
            if let Event::DraftLease { generation } = d.event {
                Some(generation)
            } else {
                None
            }
        })
        .unwrap();
    let update = |revision, text: &str| Command::Draft {
        generation,
        revision,
        text: text.into(),
    };
    assert_eq!(
        room.handle(1, update(2, "Hello"), now + 100).unwrap().len(),
        1
    );
    assert!(
        room.handle(1, update(1, "Old"), now + 200)
            .unwrap()
            .is_empty()
    );
    let events = room.tick(now + 3101).unwrap();
    assert!(
        events
            .iter()
            .any(|d| matches!(&d.event,Event::Draft{text,..}if text.is_empty()))
    );
    assert!(
        room.handle(1, update(2, "Resurrection"), now + 3200)
            .unwrap()
            .is_empty()
    );
    room.join(2, Some(&token), false, now + 3300, true).unwrap();
    assert!(
        room.handle(1, update(10, "Wrong tab"), now + 3400)
            .unwrap()
            .is_empty()
    );
    let history = room
        .handle(2, Command::History { before: u64::MAX }, now + 3400)
        .unwrap();
    assert!(matches!(&history[0].event,Event::History{messages,..}if messages.is_empty()));
}
#[test]
fn unknown_ids_do_not_resume_identity_and_copied_ids_never_grant_admin() {
    let now = 1_000_000;
    let mut room = Room::open(None, now).unwrap();
    let (a, token) = room.identity(None, now).unwrap();
    assert!(room.identity(Some(&a.id), now).is_err());
    assert!(room.identity(Some(&"f".repeat(64)), now).is_err());
    room.join(1, Some(&token), true, now, true).unwrap();
    room.join(2, Some(&token), false, now, true).unwrap();
    let target = room.identity(None, now).unwrap();
    room.join(3, Some(&target.1), false, now, true).unwrap();
    let generation = room
        .members()
        .iter()
        .find(|m| m.profile.id == target.0.id)
        .unwrap()
        .generation;
    assert!(
        room.handle(
            2,
            Command::Kick {
                participant: target.0.id,
                generation,
                reason: String::new()
            },
            now
        )
        .is_err()
    );
}
#[test]
fn incomplete_tail_is_recovered_but_complete_corruption_fails_closed() {
    use std::io::Write;
    let p = path();
    let mut room = Room::open(Some(p.clone()), 1000).unwrap();
    let (_, token) = room.identity(None, 1000).unwrap();
    drop(room);
    let original = fs::metadata(&p).unwrap().len();
    fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(&[10, 0, 0])
        .unwrap();
    room = Room::open(Some(p.clone()), 1001).unwrap();
    assert!(room.valid_identity(&token));
    assert_eq!(fs::metadata(&p).unwrap().len(), original);
    drop(room);
    let mut bytes = fs::read(&p).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&p, bytes).unwrap();
    assert!(Room::open(Some(p.clone()), 1002).is_err());
    fs::remove_file(p).unwrap();
}
#[test]
fn framing_and_admission_reject_oversized_or_spoofed_data() {
    let c = Command::Join { credential: None };
    let bytes = encode_command(&c).unwrap();
    assert_eq!(decode_command(&bytes).unwrap(), c);
    assert!(decode_command(&vec![0; MAX_COMMAND_BYTES + 1]).is_err());
    assert!(decode_event(&[0; 4]).is_err());
    let now = 1000;
    let mut room = Room::open(None, now).unwrap();
    let (_, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    assert!(
        room.handle(1, send(&"x".repeat(2049), 1, now), now)
            .is_err()
    );
    assert!(
        room.handle(
            1,
            Command::Profile {
                name: "@Admin".into(),
                avatar: 0
            },
            now
        )
        .is_err()
    );
}
