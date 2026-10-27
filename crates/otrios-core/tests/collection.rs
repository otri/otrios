// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "merkle")]
use otrios_core::{
    collection::*,
    merkle::{Edit, Snapshot},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Barrier, Mutex},
};

fn schema() -> Schema {
    Schema {
        id: [24; 32],
        fields: BTreeMap::from([(1, [1; 32]), (2, [2; 32])]),
    }
}
fn record(bytes: &[u8]) -> Snapshot {
    Snapshot::empty()
        .edit(
            [3; 16],
            &[
                Edit {
                    path: vec![1],
                    schema: [1; 32],
                    value: Some(bytes.to_vec()),
                },
                Edit {
                    path: vec![2],
                    schema: [2; 32],
                    value: Some(b"private-canary".to_vec()),
                },
            ],
        )
        .unwrap()
        .0
}
fn collection(count: u64) -> Collection {
    let mut c = Collection::new(24, schema(), Budget::default()).unwrap();
    let row = record(b"visible");
    // Deliberately sparse keys: positions are never identities or cursors.
    for key in 1..=count {
        c = c.append(key * 3, row.clone()).unwrap();
    }
    c
}
fn grant() -> Grant {
    Grant {
        collection: 24,
        fields: vec![1],
    }
}
fn plan(anchor: Anchor) -> Selection {
    Selection {
        collection: 24,
        schema: [24; 32],
        anchor,
        record_limit: 16,
        byte_limit: 32 * 1024,
        fields: vec![1],
    }
}
fn ack(session: &mut Session, offer: &Offer) {
    session
        .acknowledge(offer.generation, offer.transfer.cursor, offer.transfer.root)
        .unwrap();
}
fn value(window: &Window, key: u64) -> Vec<u8> {
    window
        .record(key)
        .unwrap()
        .subtree(&[1])
        .unwrap()
        .register()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .2
        .unwrap()
        .to_vec()
}

#[test]
fn sparse_seek_is_bounded_and_never_exposes_unselected_fields() {
    let c = collection(5000);
    let w = c.select(71, &grant(), &plan(Anchor::Around(7500))).unwrap();
    assert_eq!(w.keys, (2492..2508).map(|n| n * 3).collect::<Vec<_>>());
    assert_eq!(w.work.records_considered, 16);
    assert!(w.work.pages_visited < 20, "{:?}", w.work);
    assert_eq!(value(&w, 7500), b"visible");
    assert!(w.record(7500).unwrap().subtree(&[2]).is_err());
    assert!(
        !w.snapshot()
            .to_bytes()
            .unwrap()
            .windows(14)
            .any(|b| b == b"private-canary")
    );
    let before = c
        .select(
            71,
            &grant(),
            &plan(Anchor::Before(Cursor {
                epoch: 71,
                collection: 24,
                key: 7500,
            })),
        )
        .unwrap();
    assert_eq!(before.keys.last(), Some(&7497));
    assert!(before.more_after && before.more_before);
}

#[test]
fn byte_caps_select_a_contiguous_window_and_keep_the_anchor() {
    let c = collection(100);
    let one = c
        .select(71, &grant(), &plan(Anchor::Tail))
        .unwrap()
        .record(300)
        .unwrap()
        .canonical_bytes();
    for anchor in [Anchor::Tail, Anchor::Around(150)] {
        let mut p = plan(anchor);
        p.byte_limit = (one * 3) as u32;
        let w = c.select(71, &grant(), &p).unwrap();
        assert_eq!(w.keys.len(), 3);
        assert!(w.keys.windows(2).all(|k| k[1] - k[0] == 3));
        if matches!(anchor, Anchor::Around(_)) {
            assert!(w.keys.contains(&150));
        } else {
            assert_eq!(w.keys.last(), Some(&300));
        }
        assert_eq!(w.selected_bytes, one * 3);
        p.byte_limit = (one - 1) as u32;
        assert!(matches!(c.select(71, &grant(), &p), Err(Error::Limit)));
    }
}

#[test]
fn invalid_schema_limits_and_policy_fail_before_selection() {
    let c = collection(3);
    let mut p = plan(Anchor::Around(999));
    p.fields = vec![2];
    assert!(matches!(c.select(71, &grant(), &p), Err(Error::Forbidden)));
    p.fields = vec![1, 1];
    assert!(matches!(c.select(71, &grant(), &p), Err(Error::Invalid)));
    p.fields = vec![1];
    p.record_limit = 65;
    assert!(matches!(c.select(71, &grant(), &p), Err(Error::Limit)));
    p.record_limit = 1;
    p.schema = [0; 32];
    assert!(matches!(c.select(71, &grant(), &p), Err(Error::Schema)));
    assert!(matches!(
        c.append(12, Snapshot::empty()),
        Err(Error::Schema)
    ));
    assert!(matches!(
        c.append(9, record(b"reused")),
        Err(Error::Conflict)
    ));
}

#[test]
fn pruning_expires_cursors_without_mutating_pins_or_reusing_ids() {
    let old = collection(3000);
    let current = old.retain_from(6000);
    assert_eq!(current.first_key(), Some(6000));
    let cursor = Cursor {
        epoch: 71,
        collection: 24,
        key: 300,
    };
    assert!(matches!(
        current.select(71, &grant(), &plan(Anchor::Before(cursor))),
        Err(Error::AnchorExpired)
    ));
    assert!(
        old.select(71, &grant(), &plan(Anchor::Before(cursor)))
            .is_ok()
    );
    let all_pruned = current.retain_from(u64::MAX);
    assert!(all_pruned.is_empty());
    assert!(matches!(
        all_pruned.append(1, record(b"reuse")),
        Err(Error::Conflict)
    ));
    assert_eq!(all_pruned.append(9003, record(b"next")).unwrap().len(), 1);
    for key in [6000, 6099, 8997, 9000] {
        assert!(
            current
                .select(71, &grant(), &plan(Anchor::Around(key)))
                .unwrap()
                .keys
                .contains(&key)
        );
    }
}

#[test]
fn slow_reader_gets_one_pinned_offer_then_latest_coherent_window() {
    let mut authority = Authority::new(71, collection(100));
    let mut session = Session::default();
    let p = plan(Anchor::Tail);
    let first = authority
        .request(&mut session, grant(), 1, p.clone())
        .unwrap();
    let mut replica = Replica::new(71, 24, schema());
    replica.begin_request(1, p).unwrap();
    for key in 301..=320 {
        let next = authority
            .collection()
            .append(key, record(format!("row-{key}").as_bytes()))
            .unwrap();
        authority
            .publish(authority.collection().revision(), next)
            .unwrap();
        assert!(matches!(
            authority.poll(&mut session),
            Err(Error::Backpressure)
        ));
        assert!(session.waiting_for_ack());
        assert_eq!(session.lease_count(), 1);
    }
    replica.apply(&first).unwrap();
    ack(&mut session, &first);
    let pinned = replica.window().unwrap().clone();
    assert_eq!(pinned.keys.last(), Some(&300));
    let last = authority.poll(&mut session).unwrap().unwrap();
    replica.apply(&last).unwrap();
    ack(&mut session, &last);
    let window = replica.window().unwrap();
    assert_eq!(window.keys, (305..=320).collect::<Vec<_>>());
    for key in &window.keys {
        assert_eq!(value(window, *key), format!("row-{key}").as_bytes());
    }
    assert_eq!(pinned.keys.last(), Some(&300));
    assert_eq!(replica.apply(&last), Ok(false));
    assert!(authority.poll(&mut session).unwrap().is_none());
}

#[test]
fn unrelated_append_does_not_transfer_to_an_anchored_window() {
    let mut authority = Authority::new(71, collection(100));
    let p = plan(Anchor::Around(150));
    let mut lease = authority.subscribe(grant(), 1, p).unwrap();
    let offer = authority.offer(&mut lease).unwrap().unwrap();
    lease
        .acknowledge(offer.transfer.cursor, offer.transfer.root)
        .unwrap();
    let next = authority
        .collection()
        .append(303, record(b"outside-selection"))
        .unwrap();
    authority
        .publish(authority.collection().revision(), next)
        .unwrap();
    assert!(authority.offer(&mut lease).unwrap().is_none());
}

#[test]
fn superseded_window_cannot_replace_newer_request_and_overlap_is_bounded() {
    let mut authority = Authority::new(71, collection(100));
    let mut session = Session::default();
    let mut replica = Replica::new(71, 24, schema());
    let tail = plan(Anchor::Tail);
    replica.begin_request(1, tail.clone()).unwrap();
    let first = authority.request(&mut session, grant(), 1, tail).unwrap();
    replica.apply(&first).unwrap();
    ack(&mut session, &first);
    let pinned = replica.window().unwrap().clone();
    let mut late = first;
    for generation in 2..=12 {
        let p = plan(Anchor::Around(generation * 3));
        replica.begin_request(generation, p.clone()).unwrap();
        let next = authority
            .request(&mut session, grant(), generation, p)
            .unwrap();
        assert_eq!(session.lease_count(), 2);
        assert_eq!(replica.apply(&late), Err(Error::Stale));
        assert_eq!(
            replica.window().unwrap().snapshot().hash(),
            pinned.snapshot().hash()
        );
        late = next;
    }
    replica.apply(&late).unwrap();
    ack(&mut session, &late);
    assert_eq!(session.lease_count(), 1);
    assert!(replica.window().unwrap().keys.contains(&36));
}

#[test]
fn revocation_clears_view_and_server_coverage_even_during_replacement() {
    let mut authority = Authority::new(71, collection(10));
    let mut session = Session::default();
    let mut replica = Replica::new(71, 24, schema());
    let p = plan(Anchor::Tail);
    replica.begin_request(1, p.clone()).unwrap();
    let offer = authority.request(&mut session, grant(), 1, p).unwrap();
    replica.apply(&offer).unwrap();
    ack(&mut session, &offer);
    assert_eq!(
        authority.authorize_session(
            &mut session,
            Grant {
                collection: 24,
                fields: vec![]
            }
        ),
        Err(Error::Forbidden)
    );
    assert_eq!(session.lease_count(), 0);
    replica.revoke();
    assert!(replica.window().is_none());
    assert_eq!(replica.apply(&offer), Err(Error::Forbidden));
}

#[test]
fn competing_publications_have_one_winner_and_leave_old_window_immutable() {
    let collection = collection(10);
    let expected = collection.revision();
    let authority = Arc::new(Mutex::new(Authority::new(71, collection.clone())));
    let barrier = Arc::new(Barrier::new(3));
    let handles = [31, 32].map(|key| {
        let authority = authority.clone();
        let barrier = barrier.clone();
        let next = collection.append(key, record(&[key as u8])).unwrap();
        std::thread::spawn(move || {
            barrier.wait();
            authority.lock().unwrap().publish(expected, next)
        })
    });
    barrier.wait();
    let results = handles.map(|h| h.join().unwrap());
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::Conflict))
            .count(),
        1
    );
    assert_eq!(collection.last_key(), Some(30));
}

#[test]
fn reconnect_requires_fresh_epoch_and_explicit_resync_recovers_lost_ram() {
    let mut authority = Authority::new(71, collection(10));
    let p = plan(Anchor::Tail);
    let mut lease = authority.subscribe(grant(), 1, p.clone()).unwrap();
    let first = authority.offer(&mut lease).unwrap().unwrap();
    lease
        .acknowledge(first.transfer.cursor, first.transfer.root)
        .unwrap();
    lease.resync();
    let reset = authority.offer(&mut lease).unwrap().unwrap();
    let mut replica = Replica::new(71, 24, schema());
    replica.begin_request(1, p.clone()).unwrap();
    replica.apply(&reset).unwrap();
    let mut restarted = Replica::new(72, 24, schema());
    restarted.begin_request(1, p).unwrap();
    assert_eq!(restarted.apply(&reset), Err(Error::Stale));
    let stale = plan(Anchor::Before(Cursor {
        epoch: 70,
        collection: 24,
        key: 30,
    }));
    assert!(matches!(
        authority.collection().select(71, &grant(), &stale),
        Err(Error::Stale)
    ));
}

#[test]
fn fixed_window_work_bytes_and_replica_size_stay_bounded_as_history_grows() {
    let mut c = collection(0);
    let row = record(b"constant");
    let mut sizes = Vec::new();
    for count in 1..=65_536 {
        c = c.append(count * 3, row.clone()).unwrap();
        if [128, 4096, 65_536].contains(&count) {
            let w = c.select(71, &grant(), &plan(Anchor::Around(192))).unwrap();
            assert_eq!(w.work.records_considered, 16);
            assert!(w.work.pages_visited < 24, "count={count} {:?}", w.work);
            sizes.push((
                w.selected_bytes,
                w.snapshot().canonical_bytes(),
                w.snapshot().to_bytes().unwrap().len(),
            ));
        }
    }
    assert!(sizes.windows(2).all(|pair| pair[0] == pair[1]), "{sizes:?}");
    // Independent construction produces identical canonical index/record roots.
    let rebuilt = collection(100);
    let independently = collection(100);
    assert_eq!(rebuilt.revision(), independently.revision());
}

#[test]
fn wire_roundtrips_and_truncated_or_corrupt_views_never_publish() {
    let mut authority = Authority::new(71, collection(100));
    let mut session = Session::default();
    let p = plan(Anchor::Around(150));
    let request = Command::Select {
        generation: 1,
        plan: p.clone(),
    };
    let bytes = encode_command(&request).unwrap();
    assert_eq!(decode_command(&bytes).unwrap(), request);
    for end in 0..bytes.len() {
        assert!(decode_command(&bytes[..end]).is_err());
    }
    let offer = authority
        .handle(&mut session, grant(), decode_command(&bytes).unwrap())
        .unwrap()
        .unwrap();
    let bytes = offer.to_bytes().unwrap();
    let parsed = Offer::from_bytes(&bytes).unwrap();
    assert_eq!(bytes, parsed.to_bytes().unwrap());
    let mut replica = Replica::new(71, 24, schema());
    replica.begin_request(1, p.clone()).unwrap();
    replica.apply(&parsed).unwrap();
    let pinned = replica.window().unwrap().clone();
    for end in 0..bytes.len() {
        assert!(Offer::from_bytes(&bytes[..end]).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(Offer::from_bytes(&extra).is_err());
    let mut corrupt = bytes;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    match Offer::from_bytes(&corrupt) {
        Err(_) => (),
        Ok(offer) => {
            assert!(replica.apply(&offer).is_err());
        }
    }
    assert_eq!(
        replica.window().unwrap().snapshot().hash(),
        pinned.snapshot().hash()
    );
    let ack = Command::Acknowledge {
        generation: 1,
        cursor: parsed.transfer.cursor,
        root: parsed.transfer.root,
    };
    assert_eq!(decode_command(&encode_command(&ack).unwrap()).unwrap(), ack);
    assert!(
        authority
            .handle(&mut session, grant(), ack)
            .unwrap()
            .is_none()
    );
    for command in [
        Command::Cancel { generation: 1 },
        Command::Resync { generation: 1 },
    ] {
        assert_eq!(
            decode_command(&encode_command(&command).unwrap()).unwrap(),
            command
        );
    }
}

#[test]
fn canonical_empty_collection_has_an_independent_sha256_vector() {
    // Python hashlib over the documented domain, LE collection ID, schema ID,
    // sorted field/schema pairs, high-water key and absent-page flag.
    assert_eq!(
        collection(0).revision(),
        [
            0x5a, 0xaa, 0x86, 0xd0, 0xcf, 0x82, 0xb2, 0xc0, 0x1f, 0x55, 0x4a, 0x46, 0xdf, 0x18,
            0xb3, 0x00, 0xe5, 0xfb, 0x8d, 0xa1, 0x81, 0x5f, 0xa0, 0x78, 0x7e, 0xa9, 0x2c, 0x6c,
            0x4b, 0x0d, 0x63, 0xc5,
        ]
    );
}

#[test]
fn empty_tail_and_before_first_have_coherent_explicit_boundary_metadata() {
    let empty = collection(0)
        .select(71, &grant(), &plan(Anchor::Tail))
        .unwrap();
    assert!(empty.keys.is_empty());
    assert!(!empty.more_before && !empty.more_after);
    let before = collection(10)
        .select(
            71,
            &grant(),
            &plan(Anchor::Before(Cursor {
                epoch: 71,
                collection: 24,
                key: 3,
            })),
        )
        .unwrap();
    assert!(before.keys.is_empty());
    assert!(!before.more_before && before.more_after);
}

#[test]
fn retention_updates_boundary_metadata_without_changing_selected_records() {
    let mut authority = Authority::new(71, collection(100));
    let p = plan(Anchor::Before(Cursor {
        epoch: 71,
        collection: 24,
        key: 300,
    }));
    let mut session = Session::default();
    let initial = authority
        .request(&mut session, grant(), 1, p.clone())
        .unwrap();
    let mut replica = Replica::new(71, 24, schema());
    replica.begin_request(1, p).unwrap();
    replica.apply(&initial).unwrap();
    ack(&mut session, &initial);
    let pinned = replica.window().unwrap().clone();
    assert!(pinned.more_before);
    let next = authority.collection().retain_from(pinned.keys[0]);
    authority
        .publish(authority.collection().revision(), next)
        .unwrap();
    let update = authority.poll(&mut session).unwrap().unwrap();
    replica.apply(&update).unwrap();
    let window = replica.window().unwrap();
    assert!(!window.more_before);
    assert_eq!(window.keys, pinned.keys);
    for key in &window.keys {
        assert_eq!(
            window.record(*key).unwrap().hash(),
            pinned.record(*key).unwrap().hash()
        );
    }
}

#[test]
fn record_and_collection_budgets_fail_atomically_at_the_boundary() {
    let c = Collection::new(
        24,
        schema(),
        Budget {
            records: 2,
            canonical_bytes: 64 * 1024,
        },
    )
    .unwrap();
    let c = c
        .append(1, record(b"one"))
        .unwrap()
        .append(2, record(b"two"))
        .unwrap();
    let pinned = c.revision();
    assert!(matches!(c.append(3, record(b"three")), Err(Error::Limit)));
    assert_eq!(c.revision(), pinned);
    let c = Collection::new(
        24,
        schema(),
        Budget {
            records: 10,
            canonical_bytes: 600,
        },
    )
    .unwrap();
    assert!(matches!(c.append(1, record(&[0; 1024])), Err(Error::Limit)));
    assert_eq!(c.len(), 0);
    let c = collection(0);
    assert!(matches!(
        c.append(1, record(&[0; 64 * 1024])),
        Err(Error::Limit)
    ));
}

#[test]
fn valid_merkle_bytes_with_mixed_membership_and_record_versions_are_rejected_atomically() {
    let mut authority = Authority::new(71, collection(10));
    let mut session = Session::default();
    let p = plan(Anchor::Tail);
    let initial = authority
        .request(&mut session, grant(), 1, p.clone())
        .unwrap();
    let mut replica = Replica::new(71, 24, schema());
    replica.begin_request(1, p.clone()).unwrap();
    replica.apply(&initial).unwrap();
    let pinned = replica.window().unwrap().clone();
    let key = pinned.keys[0];
    let mut path = (0..13)
        .rev()
        .map(|shift| (key >> (shift * 5)) & 31)
        .collect::<Vec<_>>();
    path.push(1);
    let changed = pinned
        .snapshot()
        .edit(
            [9; 16],
            &[Edit {
                path,
                schema: [1; 32],
                value: Some(b"independently advanced record".to_vec()),
            }],
        )
        .unwrap()
        .0;
    let mut wrong = otrios_core::merkle::Authority::new(71, changed);
    // A replacement has a new lease ID, so baseline replay protection does not
    // mask the stronger membership/row coherence check under test.
    wrong.subscribe(&[vec![]], vec![]).unwrap();
    let mut scope = wrong.subscribe(&[vec![]], vec![]).unwrap();
    let mixed = Offer {
        generation: 2,
        lease: scope.cursor().lease,
        transfer: wrong.offer(&mut scope).unwrap().unwrap(),
        ..initial
    };
    replica.begin_request(2, p).unwrap();
    assert_eq!(replica.apply(&mixed), Err(Error::Invalid));
    assert_eq!(
        replica.window().unwrap().snapshot().hash(),
        pinned.snapshot().hash()
    );
}
