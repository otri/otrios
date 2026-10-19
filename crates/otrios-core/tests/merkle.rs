// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "merkle")]
use otrios_core::merkle::*;
const SCHEMA: Hash = [7; 32];
fn edit(path: &[u64], value: Option<&[u8]>) -> Edit {
    Edit {
        path: path.to_vec(),
        schema: SCHEMA,
        value: value.map(<[u8]>::to_vec),
    }
}
fn set(tree: &Snapshot, actor: u8, path: &[u64], value: &[u8]) -> Snapshot {
    tree.edit([actor; 16], &[edit(path, Some(value))])
        .unwrap()
        .0
}
fn value(tree: &Snapshot, path: &[u64]) -> Vec<Option<Vec<u8>>> {
    tree.subtree(path)
        .unwrap()
        .register()
        .unwrap()
        .values()
        .map(|(_, _, v)| v.map(<[u8]>::to_vec))
        .collect()
}
fn receive(
    authority: &Authority,
    subscription: &mut Subscription,
    replica: &mut Replica,
) -> Option<Offer> {
    let offer = authority.offer(subscription).unwrap()?;
    replica.apply(offer.bytes()).unwrap();
    subscription.acknowledge(offer.cursor, offer.root).unwrap();
    Some(offer)
}
#[test]
fn edit_work_is_path_bounded_batch_ancestors_are_shared_and_old_roots_stay_pinned() {
    let mut tree = Snapshot::empty();
    for app in 0..32 {
        let edits = (0..32)
            .map(|id| edit(&[app, id], Some(&[id as u8; 1024])))
            .collect::<Vec<_>>();
        tree = tree.edit([1; 16], &edits).unwrap().0;
    }
    let old = tree.clone();
    let (next, work) = tree
        .edit(
            [1; 16],
            &[edit(&[0, 1], Some(b"new")), edit(&[0, 2], Some(b"other"))],
        )
        .unwrap();
    assert_eq!(work.hashed_nodes, 4); // two leaves, shared parent, root
    assert!(
        tree.subtree(&[31])
            .unwrap()
            .shares_root(&next.subtree(&[31]).unwrap())
    );
    assert_eq!(value(&old, &[0, 1]), vec![Some(vec![1; 1024])]);
    assert_eq!(value(&next, &[0, 1]), vec![Some(b"new".to_vec())]);
    assert_eq!(next.edit([1; 16], &[]).unwrap().1.hashed_nodes, 0);
    // Same materialized state, rebuilt independently with different batch sizes.
    let mut rebuilt = Snapshot::empty();
    let mut commands = Vec::new();
    for app in (0..32).rev() {
        for id in (0..32).rev() {
            commands.push(edit(&[app, id], Some(&[id as u8; 1024])));
        }
    }
    for chunk in commands.chunks(MAX_EDITS) {
        rebuilt = rebuilt.edit([1; 16], chunk).unwrap().0;
    }
    assert_eq!(tree.hash(), rebuilt.hash());
    assert_eq!(tree.merge(&tree).unwrap().1.hashed_nodes, 0);
}
#[test]
fn join_converges_for_siblings_conflicts_deletes_and_metadata_only_edits() {
    let seed = set(&Snapshot::empty(), 1, &[1], b"same");
    let a = set(&seed, 2, &[1], b"same");
    let b = set(&seed, 3, &[1], b"same");
    let c = set(&seed, 4, &[2], b"sibling");
    let ab = a.merge(&b).unwrap().0;
    assert_ne!(seed.hash(), ab.hash());
    assert_eq!(value(&ab, &[1]), vec![Some(b"same".to_vec()); 2]);
    assert_eq!(ab.hash(), b.merge(&a).unwrap().0.hash());
    assert_eq!(
        ab.merge(&c).unwrap().0.hash(),
        a.merge(&b.merge(&c).unwrap().0).unwrap().0.hash()
    );
    let tombstone = ab.edit([2; 16], &[edit(&[1], None)]).unwrap().0;
    assert_eq!(value(&tombstone, &[1]), vec![None]);
    assert_eq!(tombstone.hash(), tombstone.merge(&seed).unwrap().0.hash());
    let concurrent = set(&seed, 5, &[1], b"concurrent");
    let joined = tombstone.merge(&concurrent).unwrap().0;
    assert_eq!(
        value(&joined, &[1]),
        vec![None, Some(b"concurrent".to_vec())]
    );
    let resolved = set(&joined, 2, &[1], b"resolved");
    assert_eq!(value(&resolved, &[1]), vec![Some(b"resolved".to_vec())]);
    assert_eq!(resolved.merge(&joined).unwrap().0.hash(), resolved.hash());
    assert!(resolved.shares_root(&resolved.merge(&joined).unwrap().0));
}
#[test]
fn seeded_join_schedules_agree_associatively_and_commutatively() {
    let mut replicas = vec![Snapshot::empty(); 5];
    let mut rng = 0xfeed_cafe_u64;
    for step in 0..300_u32 {
        let bytes = step.to_le_bytes();
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let a = (rng % 5) as usize;
        let b = ((rng >> 8) % 5) as usize;
        if step % 3 == 0 {
            replicas[a] = replicas[a].merge(&replicas[b]).unwrap().0;
        } else {
            replicas[a] = replicas[a]
                .edit(
                    [a as u8; 16],
                    &[edit(
                        &[(rng >> 16) % 3, (rng >> 24) % 4],
                        if step % 7 == 0 { None } else { Some(&bytes) },
                    )],
                )
                .unwrap()
                .0;
        }
    }
    let forward = replicas
        .iter()
        .try_fold(Snapshot::empty(), |s, r| s.merge(r).map(|p| p.0))
        .unwrap();
    let reverse = replicas
        .iter()
        .rev()
        .try_fold(Snapshot::empty(), |s, r| s.merge(r).map(|p| p.0))
        .unwrap();
    assert_eq!(forward.hash(), reverse.hash(), "seed=0xfeedcafe");
    for r in &replicas {
        assert_eq!(forward.merge(r).unwrap().0.hash(), forward.hash());
    }
}
#[test]
fn scope_isolation_missing_slots_and_metadata_only_revisions_replicate() {
    let tree = set(
        &set(&Snapshot::empty(), 1, &[1, 1], b"public"),
        1,
        &[2, 1],
        b"private-canary",
    );
    let mut authority = Authority::new(100, tree);
    assert!(matches!(
        authority.subscribe(&[vec![1]], vec![2]),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        authority.subscribe(&[vec![1]], vec![]),
        Err(Error::Forbidden)
    ));
    let mut sub = authority.subscribe(&[vec![1]], vec![1]).unwrap();
    let mut replica = Replica::new(&sub);
    let initial = receive(&authority, &mut sub, &mut replica).unwrap();
    assert!(!initial.bytes().windows(14).any(|b| b == b"private-canary"));
    assert_eq!(Replica::transferred_nodes(initial.bytes()).unwrap(), 2);
    let before = authority.snapshot().hash();
    let unrelated = set(authority.snapshot(), 1, &[2, 1], b"elsewhere");
    authority.publish(before, unrelated).unwrap();
    assert!(authority.offer(&mut sub).unwrap().is_none());
    let current = authority.snapshot();
    let metadata = set(current, 2, &[1, 1], b"public");
    authority.publish(current.hash(), metadata).unwrap();
    let delta = receive(&authority, &mut sub, &mut replica).unwrap();
    assert_eq!(Replica::transferred_nodes(delta.bytes()).unwrap(), 2);
    assert_eq!(
        replica.snapshot().unwrap().hash(),
        authority.snapshot().subtree(&[1]).unwrap().hash()
    );
    let mut absent = authority.subscribe(&[vec![1]], vec![1, 99]).unwrap();
    let mut empty = Replica::new(&absent);
    receive(&authority, &mut absent, &mut empty).unwrap();
    assert!(empty.snapshot().is_none());
    let next = set(authority.snapshot(), 1, &[1, 99], b"created");
    authority
        .publish(authority.snapshot().hash(), next)
        .unwrap();
    receive(&authority, &mut absent, &mut empty).unwrap();
    assert_eq!(
        value(empty.snapshot().unwrap(), &[]),
        vec![Some(b"created".to_vec())]
    );
}
#[test]
fn slow_reader_retransmits_one_offer_then_coalesces_and_handles_resync_and_revocation() {
    let mut authority = Authority::new(100, set(&Snapshot::empty(), 1, &[1], b"0"));
    let mut sub = authority.subscribe(&[vec![]], vec![]).unwrap();
    let mut replica = Replica::new(&sub);
    let first = authority.offer(&mut sub).unwrap().unwrap();
    for i in 1..100_u32 {
        let next = set(authority.snapshot(), 1, &[1], &i.to_le_bytes());
        authority
            .publish(authority.snapshot().hash(), next)
            .unwrap();
        let repeated = authority.offer(&mut sub).unwrap().unwrap();
        assert_eq!(repeated.bytes(), first.bytes());
    }
    assert!(sub.acknowledge(first.cursor, Some([0; 32])).is_err());
    replica.apply(first.bytes()).unwrap();
    assert!(!replica.apply(first.bytes()).unwrap());
    sub.acknowledge(first.cursor, first.root).unwrap();
    let latest = receive(&authority, &mut sub, &mut replica).unwrap();
    assert_eq!(latest.cursor.sequence, 2);
    assert_eq!(
        replica.snapshot().unwrap().hash(),
        authority.snapshot().hash()
    );
    assert_eq!(replica.apply(first.bytes()), Err(Error::Stale));
    let mut lost_ram = Replica::new(&sub);
    let next = set(authority.snapshot(), 1, &[1], b"100");
    authority
        .publish(authority.snapshot().hash(), next)
        .unwrap();
    let delta = authority.offer(&mut sub).unwrap().unwrap();
    assert_eq!(lost_ram.apply(delta.bytes()), Err(Error::ResyncRequired));
    sub.resync();
    receive(&authority, &mut sub, &mut lost_ram).unwrap();
    assert_eq!(
        lost_ram.snapshot().unwrap().hash(),
        authority.snapshot().hash()
    );
    replica.cancel();
    assert!(replica.snapshot().is_none());
    assert_eq!(replica.apply(latest.bytes()), Err(Error::Forbidden));
    sub.cancel();
    assert!(matches!(authority.offer(&mut sub), Err(Error::Forbidden)));
    assert_eq!(
        sub.acknowledge(latest.cursor, latest.root),
        Err(Error::Forbidden)
    );
}
#[test]
fn corrupt_partial_oversized_wrong_scope_and_old_epoch_frames_publish_nothing() {
    let mut authority = Authority::new(100, set(&Snapshot::empty(), 1, &[1], b"payload"));
    let mut sub = authority.subscribe(&[vec![]], vec![]).unwrap();
    let mut replica = Replica::new(&sub);
    let first = receive(&authority, &mut sub, &mut replica).unwrap();
    let pinned = replica.snapshot().unwrap().clone();
    let next = set(authority.snapshot(), 1, &[1], b"new-payload");
    authority
        .publish(authority.snapshot().hash(), next)
        .unwrap();
    let delta = authority.offer(&mut sub).unwrap().unwrap();
    assert_eq!(
        replica.apply(&vec![0; MAX_TREE_BYTES + MAX_NODES * 4 + 1025]),
        Err(Error::Limit)
    );
    for n in 0..delta.bytes().len() {
        assert!(replica.apply(&delta.bytes()[..n]).is_err());
        assert_eq!(replica.snapshot().unwrap().hash(), pinned.hash());
    }
    let mut corrupt = delta.bytes().to_vec();
    let p = corrupt
        .windows(11)
        .position(|b| b == b"new-payload")
        .unwrap();
    corrupt[p] ^= 1;
    assert!(replica.apply(&corrupt).is_err());
    assert_eq!(replica.snapshot().unwrap().hash(), pinned.hash());
    let mut extra = delta.bytes().to_vec();
    extra.push(0);
    assert!(replica.apply(&extra).is_err());
    let mut wrong = Replica::for_lease(100, sub.cursor().lease, vec![99]).unwrap();
    assert_eq!(wrong.apply(first.bytes()), Err(Error::Stale));
    let mut restarted = Replica::for_lease(101, sub.cursor().lease, vec![]).unwrap();
    assert_eq!(restarted.apply(delta.bytes()), Err(Error::Stale));
    receive(&authority, &mut sub, &mut replica).unwrap();
    assert_eq!(value(&pinned, &[1]), vec![Some(b"payload".to_vec())]);
}
#[test]
fn limits_schema_collisions_and_stale_publication_are_explicit_and_atomic() {
    let tree = set(&Snapshot::empty(), 1, &[1], b"old");
    let original = tree.hash();
    let mut bad = edit(&[1], Some(b"new"));
    bad.schema = [9; 32];
    assert!(matches!(
        tree.edit([1; 16], &[edit(&[2], Some(b"ok")), bad]),
        Err(Error::Schema)
    ));
    assert!(matches!(
        tree.edit([1; 16], &[edit(&[3; MAX_DEPTH + 1], None)]),
        Err(Error::Limit)
    ));
    assert!(matches!(
        tree.edit([1; 16], &[edit(&[1], Some(&vec![0; MAX_VALUE_BYTES + 1]))]),
        Err(Error::Limit)
    ));
    assert!(matches!(
        tree.edit(
            [1; 16],
            &(0..=MAX_CHILDREN as u64)
                .map(|k| edit(&[k], None))
                .collect::<Vec<_>>()
        ),
        Err(Error::Limit)
    ));
    let collision = set(&Snapshot::empty(), 1, &[1], b"different same dot");
    assert!(matches!(tree.merge(&collision), Err(Error::ActorCollision)));
    let mut full = tree.clone();
    for actor in 2..=MAX_ACTORS as u8 {
        full = set(&full, actor, &[1], b"x");
    }
    assert!(matches!(
        full.edit([99; 16], &[edit(&[1], None)]),
        Err(Error::Limit)
    ));
    assert_eq!(tree.hash(), original);
    let mut authority = Authority::new(1, tree);
    let next = set(authority.snapshot(), 1, &[1], b"new");
    assert_eq!(authority.publish([0; 32], next), Err(Error::Conflict));
    assert_eq!(authority.snapshot().hash(), original);
}

#[test]
fn edited_partial_replicas_join_back_without_replacing_unseen_siblings() {
    let global = set(
        &set(&Snapshot::empty(), 1, &[1, 9], b"initial"),
        1,
        &[2, 9],
        b"unseen-private",
    );
    let a = set(&global.subtree(&[1]).unwrap(), 2, &[9], b"left");
    let b = set(&global.subtree(&[1]).unwrap(), 3, &[9], b"right");
    let joined = global
        .merge_subtree(&[1], &a)
        .unwrap()
        .0
        .merge_subtree(&[1], &b)
        .unwrap()
        .0;
    let reverse = global
        .merge_subtree(&[1], &b)
        .unwrap()
        .0
        .merge_subtree(&[1], &a)
        .unwrap()
        .0;
    assert_eq!(joined.hash(), reverse.hash());
    assert_eq!(
        value(&joined, &[1, 9]),
        vec![Some(b"left".to_vec()), Some(b"right".to_vec())]
    );
    assert!(
        joined
            .subtree(&[2])
            .unwrap()
            .shares_root(&global.subtree(&[2]).unwrap())
    );
    assert_eq!(
        global
            .merge_subtree(&[1], &global.subtree(&[1]).unwrap())
            .unwrap()
            .1
            .hashed_nodes,
        0
    );
    assert!(matches!(
        global.merge_subtree(&[9; MAX_DEPTH], &a),
        Err(Error::Limit)
    ));
}

#[test]
fn publishers_racing_the_same_root_have_exactly_one_commit() {
    use std::sync::{Arc, Barrier, Mutex};
    let seed = set(&Snapshot::empty(), 1, &[1], b"seed");
    let expected = seed.hash();
    let authority = Arc::new(Mutex::new(Authority::new(1, seed.clone())));
    let barrier = Arc::new(Barrier::new(2));
    let handles = [2, 3].map(|actor| {
        let authority = authority.clone();
        let barrier = barrier.clone();
        let next = set(&seed, actor, &[1], &[actor]);
        std::thread::spawn(move || {
            barrier.wait();
            authority.lock().unwrap().publish(expected, next)
        })
    });
    let results = handles.map(|h| h.join().unwrap());
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::Conflict))
            .count(),
        1
    );
    assert_ne!(authority.lock().unwrap().snapshot().hash(), expected);
}
