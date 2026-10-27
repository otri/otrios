// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "merkle")]
use otrios_core::collection::*;

fn plan(anchor: Anchor, fields: Vec<u64>) -> Selection {
    Selection {
        collection: 24,
        schema: [24; 32],
        anchor,
        record_limit: 16,
        byte_limit: 8192,
        fields,
    }
}
#[test]
fn channel_rechecks_policy_during_backpressure_and_revokes_both_leases() {
    let mut channel = Channel::new(19);
    Synthetic::default().update(&mut channel, 100, 1).unwrap();
    let mut session = ChannelSession::default();
    let access = Access {
        authenticated: true,
        admitted: false,
    };
    let first = channel
        .handle(
            &mut session,
            access,
            Command::Select {
                generation: 1,
                plan: plan(Anchor::Tail, vec![1, 2]),
            },
        )
        .unwrap();
    let Event::View(offer) = first else {
        panic!("view");
    };
    let parsed = Event::from_bytes(&Event::View(offer.clone()).to_bytes().unwrap()).unwrap();
    let Event::View(parsed) = parsed else {
        panic!("view");
    };
    assert_eq!(parsed.transfer.bytes(), offer.transfer.bytes());
    assert!(channel.poll(&mut session, access).is_none());
    let event = channel.poll(&mut session, Access::default()).unwrap();
    assert!(matches!(
        event,
        Event::Revoked {
            epoch: 19,
            generation: 1,
            collection: 24
        }
    ));
    assert!(channel.poll(&mut session, Access::default()).is_none());
    assert!(matches!(
        channel.handle(
            &mut session,
            Access::default(),
            Command::Acknowledge {
                generation: 1,
                cursor: offer.transfer.cursor,
                root: offer.transfer.root
            }
        ),
        Some(Event::Rejected {
            status: Status::Stale,
            ..
        })
    ));
    let fresh = channel
        .handle(
            &mut session,
            Access::default(),
            Command::Select {
                generation: 2,
                plan: plan(Anchor::Tail, vec![1]),
            },
        )
        .unwrap();
    assert!(matches!(fresh, Event::View(_)));
    channel.set_policy(24, Policy::default()).unwrap();
    assert!(matches!(
        channel.poll(&mut session, Access::default()),
        Some(Event::Revoked { generation: 2, .. })
    ));
}

#[test]
fn channel_status_codecs_are_bounded_and_invalid_requests_keep_old_view() {
    let mut channel = Channel::new(19);
    Synthetic::default().update(&mut channel, 100, 1).unwrap();
    let mut session = ChannelSession::default();
    let Event::View(initial) = channel
        .handle(
            &mut session,
            Access::default(),
            Command::Select {
                generation: 1,
                plan: plan(Anchor::Tail, vec![1]),
            },
        )
        .unwrap()
    else {
        panic!("view");
    };
    channel.handle(
        &mut session,
        Access::default(),
        Command::Acknowledge {
            generation: 1,
            cursor: initial.transfer.cursor,
            root: initial.transfer.root,
        },
    );
    let denied = channel
        .handle(
            &mut session,
            Access::default(),
            Command::Select {
                generation: 2,
                plan: plan(Anchor::Tail, vec![2]),
            },
        )
        .unwrap();
    assert!(matches!(
        denied,
        Event::Rejected {
            status: Status::Forbidden,
            generation: 2
        }
    ));
    assert_eq!(session.generation(), 1);
    for event in [
        denied,
        Event::Revoked {
            epoch: 19,
            generation: 1,
            collection: 24,
        },
    ] {
        let bytes = event.to_bytes().unwrap();
        assert_eq!(
            Event::from_bytes(&bytes).unwrap().to_bytes().unwrap(),
            bytes
        );
        for len in 0..bytes.len() {
            assert!(Event::from_bytes(&bytes[..len]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(Event::from_bytes(&extra).is_err());
    }
    assert!(Event::from_bytes(&vec![0; MAX_EVENT_BYTES + 1]).is_err());
}
