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
fn sent_profiles_remain_immutable_across_collection_windows_and_journal_recovery() {
    use crate::collection as col;
    use std::sync::{Arc, Mutex};
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
    let prior = fs::read(&p).unwrap();
    room.handle(
        1,
        Command::Profile {
            name: "Admiral Newname".into(),
            avatar: 42,
        },
        now + 71_000,
    )
    .unwrap();
    assert!(fs::read(&p).unwrap().starts_with(&prior));
    drop(room);
    let registry = Arc::new(Mutex::new(col::Channel::new(42)));
    let mut room = Room::open_in(Some(p.clone()), now + 73_000, registry.clone()).unwrap();
    assert_eq!(
        room.identity(Some(&token), now + 73_000).unwrap().0.name,
        "Admiral Newname"
    );
    room.join(2, Some(&token), false, now + 73_000, true)
        .unwrap();
    for (anchor, count) in [
        (col::Anchor::Tail, 64),
        (
            col::Anchor::Before(col::Cursor {
                epoch: 42,
                collection: COLLECTION,
                key: 7,
            }),
            6,
        ),
    ] {
        let plan = message_selection(anchor);
        let mut session = col::ChannelSession::default();
        let event = registry
            .lock()
            .unwrap()
            .handle(
                &mut session,
                col::Access {
                    admitted: true,
                    authenticated: false,
                },
                col::Command::Select {
                    generation: 1,
                    plan: plan.clone(),
                },
            )
            .unwrap();
        let col::Event::View(offer) = event else {
            panic!("view");
        };
        let mut replica = col::Replica::new(42, COLLECTION, message_schema());
        replica.begin_request(1, plan).unwrap();
        replica.apply(&offer).unwrap();
        let window = replica.window().unwrap();
        assert_eq!(window.keys.len(), count);
        for key in &window.keys {
            let message = read_message(&window.record(*key).unwrap()).unwrap();
            assert_eq!(message.sender, original);
            assert_eq!(message.text, "Unchanged dispatch");
        }
    }
    drop(room);
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
    let channel = std::sync::Arc::new(std::sync::Mutex::new(crate::collection::Channel::new(42)));
    let mut room = Room::open_in(None, now, channel.clone()).unwrap();
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
    assert!(
        channel
            .lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .is_empty()
    );
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

#[test]
fn journal_replay_publishes_one_identical_collection_with_original_sent_profiles() {
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    let p = path();
    let now = 1_000_000;
    let channel = Arc::new(Mutex::new(crate::collection::Channel::new(17)));
    let mut room = Room::open_in(Some(p.clone()), now, channel.clone()).unwrap();
    let (original, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    let admission = Arc::new(AtomicBool::new(false));
    room.bind_admission(1, admission.clone()).unwrap();
    assert!(admission.load(Ordering::Acquire));
    room.handle(1, send("Immutable dispatch", 1, now), now)
        .unwrap();
    let snapshot = channel
        .lock()
        .unwrap()
        .collection(COLLECTION)
        .unwrap()
        .clone();
    assert_eq!(snapshot.len(), 1);
    let row = snapshot.records().next().unwrap().1;
    assert_eq!(read_message(row).unwrap().sender, original);
    room.handle(
        1,
        Command::Profile {
            name: "Admiral Newname".into(),
            avatar: 42,
        },
        now + 1,
    )
    .unwrap();
    assert_eq!(
        channel
            .lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .revision(),
        snapshot.revision()
    );
    // A second owner is rejected before replay or any append is possible.
    assert!(Room::open(Some(p.clone()), now + 1).is_err());
    drop(room);
    assert!(!admission.load(Ordering::Acquire));
    let canonical = fs::read(&p).unwrap();
    let recovered = Arc::new(Mutex::new(crate::collection::Channel::new(18)));
    let mut room = Room::open_in(Some(p.clone()), now + 2, recovered.clone()).unwrap();
    assert_eq!(
        fs::read(&p).unwrap(),
        canonical,
        "valid recovery preserves the canonical journal bytes"
    );
    assert_eq!(
        recovered
            .lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .revision(),
        snapshot.revision()
    );
    room.join(2, Some(&token), false, now + 2, true).unwrap();
    let retry = room
        .handle(2, send("Immutable dispatch", 1, now), now + 3)
        .unwrap();
    assert!(matches!(
        retry.as_slice(),
        [Delivery {
            event: Event::Receipt { sequence: 1, .. },
            ..
        }]
    ));
    assert_eq!(
        recovered
            .lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .len(),
        1
    );
    drop(room);
    let mut corrupt = canonical;
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&p, corrupt).unwrap();
    let unavailable = Arc::new(Mutex::new(crate::collection::Channel::new(19)));
    assert!(Room::open_in(Some(p.clone()), now + 4, unavailable.clone()).is_err());
    assert!(
        unavailable.lock().unwrap().collection(COLLECTION).is_err(),
        "no partial recovered root was installed"
    );
    fs::remove_file(p).unwrap();
}

#[test]
fn failed_durable_append_publishes_nothing_and_a_recovered_retry_commits_once() {
    use std::sync::{Arc, Mutex};
    let p = path();
    let now = 1_000_000;
    let channel = Arc::new(Mutex::new(crate::collection::Channel::new(17)));
    let mut room = Room::open_in(Some(p.clone()), now, channel.clone()).unwrap();
    let (_, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    let root = channel
        .lock()
        .unwrap()
        .collection(COLLECTION)
        .unwrap()
        .revision();
    let journal = fs::read(&p).unwrap();
    room.fail_journal_writes();
    assert!(
        room.handle(1, send("Unconfirmed dispatch", 1, now), now)
            .is_err()
    );
    assert_eq!(
        channel
            .lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .revision(),
        root
    );
    assert_eq!(fs::read(&p).unwrap(), journal);
    drop(room);
    let channel = Arc::new(Mutex::new(crate::collection::Channel::new(18)));
    let mut recovered = Room::open_in(Some(p.clone()), now + 1, channel.clone()).unwrap();
    recovered
        .join(2, Some(&token), false, now + 1, true)
        .unwrap();
    recovered
        .handle(2, send("Unconfirmed dispatch", 1, now), now + 2)
        .unwrap();
    recovered
        .handle(2, send("Unconfirmed dispatch", 1, now), now + 3)
        .unwrap();
    assert_eq!(
        channel
            .lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .len(),
        1
    );
    drop(recovered);
    fs::remove_file(p).unwrap();
}

#[test]
fn streaming_compaction_retains_receipts_profiles_and_exclusive_journal_ownership() {
    use std::sync::{Arc, Mutex};
    let p = path();
    let now = 1_000_000;
    let channel = Arc::new(Mutex::new(crate::collection::Channel::new(17)));
    let mut room = Room::open_in(Some(p.clone()), now, channel.clone()).unwrap();
    let (_, token) = room.identity(None, now).unwrap();
    room.join(1, Some(&token), false, now, true).unwrap();
    room.handle(1, send("Retained dispatch", 1, now), now)
        .unwrap();
    let before = channel
        .lock()
        .unwrap()
        .collection(COLLECTION)
        .unwrap()
        .clone();
    room.force_compaction();
    room.handle(
        1,
        Command::Profile {
            name: "Captain Revision".into(),
            avatar: 12,
        },
        now + 1,
    )
    .unwrap();
    assert!(
        Room::open(Some(p.clone()), now + 1).is_err(),
        "data-file rename must not release the owner lock"
    );
    drop(room);
    let next = Arc::new(Mutex::new(crate::collection::Channel::new(18)));
    let mut room = Room::open_in(Some(p.clone()), now + 2, next.clone()).unwrap();
    room.join(2, Some(&token), false, now + 2, true).unwrap();
    assert_eq!(room.members()[0].profile.name, "Captain Revision");
    assert_eq!(
        next.lock()
            .unwrap()
            .collection(COLLECTION)
            .unwrap()
            .revision(),
        before.revision()
    );
    let retry = room
        .handle(2, send("Retained dispatch", 1, now), now + 3)
        .unwrap();
    assert!(matches!(
        retry.as_slice(),
        [Delivery {
            event: Event::Receipt { sequence: 1, .. },
            ..
        }]
    ));
    drop(room);
    fs::remove_file(p).unwrap();
}

#[test]
fn exhausted_recovered_message_sequence_rejects_before_appending() {
    let p = path();
    let now = 1_000_000;
    let (mut journal, _) = super::journal::Journal::open(p.clone()).unwrap();
    journal
        .append(&super::journal::Entry::Sequence(u64::MAX))
        .unwrap();
    drop(journal);
    let mut room = Room::open(Some(p.clone()), now).unwrap();
    room.join(1, None, false, now, true).unwrap();
    let before = fs::read(&p).unwrap();
    assert!(room.handle(1, send("No wrapping", 1, now), now).is_err());
    assert_eq!(fs::read(&p).unwrap(), before);
    drop(room);
    fs::remove_file(p).unwrap();
}
