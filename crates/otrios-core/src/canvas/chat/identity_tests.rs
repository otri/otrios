// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

fn profile(events: &[Delivery]) -> Profile {
    events
        .iter()
        .find_map(|d| match &d.event {
            Event::Welcome {
                you,
                temporary: false,
                credential: None,
                ..
            } => Some(you.profile.clone()),
            _ => None,
        })
        .expect("durable browser/account profile")
}
fn rename(room: &mut Room, connection: u64, name: &str, now: u64) {
    room.handle(
        connection,
        Command::Profile {
            name: name.into(),
            avatar: 12,
        },
        now,
    )
    .unwrap();
}
#[test]
fn browser_identity_restores_adopts_and_recovers_without_alias_or_account_hijacking() {
    let path = std::env::temp_dir().join(format!("otrios-browser-{}.journal", random(8).unwrap()));
    let browser_a = format!("browser:{}", "a".repeat(64));
    let browser_b = format!("browser:{}", "b".repeat(64));
    let browser_c = format!("browser:{}", "c".repeat(64));
    let now = 1_000_000;
    let mut room = Room::open(Some(path.clone()), now).unwrap();
    let first = profile(&room.join_browser(1, &browser_a, None, false, now).unwrap());
    rename(&mut room, 1, "OTriMax Safari", now + 1);
    room.abandon(1, now + 2);
    let restored = profile(
        &room
            .join_browser(2, &browser_a, None, false, now + 3)
            .unwrap(),
    );
    assert_eq!(
        (restored.id.clone(), restored.name.as_str(), restored.avatar),
        (first.id.clone(), "OTriMax Safari", 12)
    );
    let signed_in = profile(
        &room
            .join_browser(3, &browser_a, Some("account-alice"), true, now + 4)
            .unwrap(),
    );
    assert_eq!(
        signed_in, restored,
        "first login keeps this verified browser's profile"
    );
    assert!(
        !room.active(2),
        "anonymous sockets must re-admit after adoption"
    );
    let other_browser = profile(
        &room
            .join_browser(4, &browser_b, Some("account-alice"), true, now + 5)
            .unwrap(),
    );
    assert_eq!(
        other_browser, restored,
        "existing account profile wins on another browser"
    );
    let signed_out = profile(
        &room
            .join_browser(5, &browser_b, None, false, now + 6)
            .unwrap(),
    );
    assert_ne!(
        signed_out.id, restored.id,
        "browser bearer cannot impersonate a signed-out account"
    );
    rename(&mut room, 5, "OTriMax Safari", now + 7);
    assert_ne!(
        room.members()
            .iter()
            .find(|m| m.profile.id == signed_out.id)
            .unwrap()
            .profile
            .name,
        "OTriMax Safari"
    );
    let other_owner = profile(
        &room
            .join_browser(6, &browser_c, Some("account-bob"), false, now + 8)
            .unwrap(),
    );
    rename(&mut room, 6, "Other owner", now + 9);
    room.abandon(6, now + 10);
    rename(&mut room, 3, "Other owner", now + 11);
    assert_ne!(
        room.members()
            .iter()
            .find(|m| m.profile.id == restored.id)
            .unwrap()
            .profile
            .name,
        "Other owner"
    );
    assert_ne!(other_owner.id, restored.id);

    // Old managed ephemeral guests never received a usable resume credential.
    room.join(99, None, false, now + 12, false).unwrap();
    rename(&mut room, 99, "Former guest alias", now + 13);
    room.abandon(99, now + 14);
    rename(&mut room, 3, "Former guest alias", now + 15);
    assert_eq!(
        room.members()
            .iter()
            .find(|m| m.profile.id == restored.id)
            .unwrap()
            .profile
            .name,
        "Former guest alias"
    );
    let before = std::fs::read(&path).unwrap();
    assert!(
        room.join_browser(7, "browser:bad", Some("account-alice"), true, now + 16)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    drop(room);
    let mut recovered = Room::open(Some(path.clone()), now + 31 * 86_400_000).unwrap();
    let restored = profile(
        &recovered
            .join_browser(
                1,
                &browser_c,
                Some("account-alice"),
                true,
                now + 31 * 86_400_000,
            )
            .unwrap(),
    );
    assert_eq!(
        (restored.id.clone(), restored.name.as_str(), restored.avatar),
        (first.id, "Former guest alias", 12)
    );
    let browser_restored = profile(
        &recovered
            .join_browser(2, &browser_b, None, false, now + 31 * 86_400_000 + 1)
            .unwrap(),
    );
    assert_eq!(
        browser_restored.id, signed_out.id,
        "browser profiles survive the old 30-day guest expiry"
    );
    let signed_out_original = profile(
        &recovered
            .join_browser(3, &browser_a, None, false, now + 31 * 86_400_000 + 2)
            .unwrap(),
    );
    assert_ne!(
        signed_out_original.id, restored.id,
        "adoption does not leave a browser-only path into the account profile"
    );
    drop(recovered);
    std::fs::remove_file(&path).unwrap();
    let _ = std::fs::remove_file(path.with_extension("journal.owner.lock"));
}
