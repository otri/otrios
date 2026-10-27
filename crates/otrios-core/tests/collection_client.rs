// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "merkle")]
use otrios_core::collection::*;
fn plan(anchor: Anchor) -> Selection {
    Selection {
        collection: 24,
        schema: [24; 32],
        anchor,
        record_limit: 16,
        byte_limit: 8192,
        fields: vec![1],
    }
}
fn setup() -> (Channel, Synthetic, ChannelSession, Client, u64) {
    let mut channel = Channel::new(19);
    let mut source = Synthetic::default();
    source.update(&mut channel, 1000, 1).unwrap();
    (
        channel,
        source,
        ChannelSession::default(),
        Client::new(24, synthetic_schema()),
        0,
    )
}
fn wire(event: Event) -> Event {
    Event::from_bytes(&event.to_bytes().unwrap()).unwrap()
}
fn request(
    channel: &mut Channel,
    session: &mut ChannelSession,
    client: &mut Client,
    counter: &mut u64,
) -> Event {
    let command = client.outgoing(counter).unwrap().unwrap();
    let command = decode_command(&encode_command(&command).unwrap()).unwrap();
    wire(channel.handle(session, Access::default(), command).unwrap())
}
fn ack(
    channel: &mut Channel,
    session: &mut ChannelSession,
    client: &mut Client,
    counter: &mut u64,
) {
    let command = client.outgoing(counter).unwrap().unwrap();
    assert!(matches!(command, Command::Acknowledge { .. }));
    assert!(
        channel
            .handle(session, Access::default(), command)
            .is_none()
    );
}
#[test]
fn pending_and_superseded_requests_cannot_publish_late_views_across_activations() {
    let (mut channel, _, mut session, mut client, mut counter) = setup();
    client.select(plan(Anchor::Tail)).unwrap();
    let old = request(&mut channel, &mut session, &mut client, &mut counter);
    client.select(plan(Anchor::Around(192))).unwrap();
    assert_eq!(client.receive(wire(old.clone())), Ok(Change::Unchanged));
    assert_eq!(
        client.receive(Event::Rejected {
            generation: 1,
            status: Status::Stale
        }),
        Ok(Change::Unchanged)
    );
    assert!(client.waiting());
    let current = request(&mut channel, &mut session, &mut client, &mut counter);
    assert_eq!(counter, 2);
    assert_eq!(client.receive(current), Ok(Change::View));
    assert!(client.window().unwrap().keys.contains(&192));
    ack(&mut channel, &mut session, &mut client, &mut counter);
    client.stop();
    assert_eq!(
        client.receive(Event::AnchorExpired {
            epoch: 19,
            generation: counter,
            collection: 24,
            neighbor: Some(201)
        }),
        Ok(Change::Unchanged)
    );
    let cancel = client.outgoing(&mut counter).unwrap().unwrap();
    channel.handle(&mut session, Access::default(), cancel);
    let mut reopened = Client::new(24, synthetic_schema());
    reopened.select(plan(Anchor::Tail)).unwrap();
    let current = request(&mut channel, &mut session, &mut reopened, &mut counter);
    assert_eq!(counter, 3);
    assert_eq!(reopened.receive(old), Ok(Change::Unchanged));
    assert!(reopened.window().is_none());
    assert_eq!(reopened.receive(current), Ok(Change::View));
}
#[test]
fn rejected_replacement_retains_the_previous_qualified_public_view() {
    let (mut channel, mut source, mut session, mut client, mut counter) = setup();
    client.select(plan(Anchor::Tail)).unwrap();
    let initial = request(&mut channel, &mut session, &mut client, &mut counter);
    client.receive(initial).unwrap();
    ack(&mut channel, &mut session, &mut client, &mut counter);
    let pinned = client.window().unwrap().clone();
    let mut denied = plan(Anchor::Tail);
    denied.fields = vec![2];
    client.select(denied).unwrap();
    let rejected = request(&mut channel, &mut session, &mut client, &mut counter);
    assert_eq!(
        client.receive(rejected),
        Ok(Change::Rejected(Status::Forbidden))
    );
    assert!(
        client.waiting(),
        "restore live coverage of the prior valid selection"
    );
    assert_eq!(
        client.window().unwrap().snapshot().hash(),
        pinned.snapshot().hash()
    );
    let restored = request(&mut channel, &mut session, &mut client, &mut counter);
    client.receive(restored).unwrap();
    ack(&mut channel, &mut session, &mut client, &mut counter);
    source.update(&mut channel, 1001, 1).unwrap();
    assert_eq!(
        client.receive(channel.poll(&mut session, Access::default()).unwrap()),
        Ok(Change::View)
    );
    assert_eq!(client.window().unwrap().keys.last(), Some(&3003));
}
#[test]
fn revocation_clears_ram_before_a_first_view_and_is_idempotent() {
    let (mut channel, _, mut session, mut client, mut counter) = setup();
    client.select(plan(Anchor::Tail)).unwrap();
    let pending = request(&mut channel, &mut session, &mut client, &mut counter);
    let revoked = Event::Revoked {
        epoch: 19,
        generation: counter,
        collection: 24,
    };
    assert_eq!(client.receive(wire(revoked.clone())), Ok(Change::Revoked));
    assert_eq!(client.receive(wire(revoked)), Ok(Change::Unchanged));
    assert_eq!(client.receive(pending), Ok(Change::Unchanged));
    assert!(client.window().is_none());
}
#[test]
fn expired_anchors_return_the_nearest_retained_key_without_scanning_history() {
    let (mut channel, mut source, mut session, mut client, mut counter) = setup();
    client.select(plan(Anchor::Around(9))).unwrap();
    let initial = request(&mut channel, &mut session, &mut client, &mut counter);
    client.receive(initial).unwrap();
    ack(&mut channel, &mut session, &mut client, &mut counter);
    source.update(&mut channel, 1000, 200).unwrap();
    let expired = wire(channel.poll(&mut session, Access::default()).unwrap());
    assert_eq!(client.receive(expired), Ok(Change::Expired(Some(201))));
    client.select(plan(Anchor::Around(201))).unwrap();
    let recovered = request(&mut channel, &mut session, &mut client, &mut counter);
    client.receive(recovered).unwrap();
    assert_eq!(client.window().unwrap().keys.first(), Some(&201));
    assert_eq!(
        channel.collection(24).unwrap().neighbor(u64::MAX),
        Some(3000)
    );
}
#[test]
fn a_delta_gap_requests_explicit_resync_without_publishing_torn_state() {
    let (mut channel, mut source, mut session, mut client, mut counter) = setup();
    client.select(plan(Anchor::Tail)).unwrap();
    let initial = request(&mut channel, &mut session, &mut client, &mut counter);
    client.receive(initial).unwrap();
    ack(&mut channel, &mut session, &mut client, &mut counter);
    let pinned = client.window().unwrap().clone();
    source.update(&mut channel, 1001, 1).unwrap();
    let Event::View(lost) = channel.poll(&mut session, Access::default()).unwrap() else {
        panic!("view")
    };
    channel.handle(
        &mut session,
        Access::default(),
        Command::Acknowledge {
            generation: counter,
            cursor: lost.transfer.cursor,
            root: lost.transfer.root,
        },
    );
    source.update(&mut channel, 1002, 1).unwrap();
    let gap = channel.poll(&mut session, Access::default()).unwrap();
    assert_eq!(client.receive(wire(gap)), Ok(Change::Unchanged));
    assert_eq!(
        client.window().unwrap().snapshot().hash(),
        pinned.snapshot().hash()
    );
    let command = client.outgoing(&mut counter).unwrap().unwrap();
    assert!(matches!(command, Command::Resync { .. }));
    let full = channel
        .handle(&mut session, Access::default(), command)
        .unwrap();
    client.receive(wire(full)).unwrap();
    assert_eq!(client.window().unwrap().keys.last(), Some(&3006));
}
