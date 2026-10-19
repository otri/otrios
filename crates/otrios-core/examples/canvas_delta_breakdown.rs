// SPDX-License-Identifier: LGPL-3.0-only
//! Inspect changed-node bytes in a trusted checkpoint/journal fixture; no payload values are printed.
use otrios_core::{canvas::hash, merkle::Snapshot};
fn own(tree: &Snapshot) -> usize {
    tree.canonical_bytes()
        - tree
            .child_keys()
            .map(|k| tree.subtree(&[k]).unwrap().canonical_bytes())
            .sum::<usize>()
}
fn changed(old: Option<&Snapshot>, new: &Snapshot) -> usize {
    if old.is_some_and(|o| o.hash() == new.hash()) {
        return 0;
    }
    own(new)
        + new
            .child_keys()
            .map(|k| {
                let o = old.and_then(|t| t.subtree(&[k]).ok());
                changed(o.as_ref(), &new.subtree(&[k]).unwrap())
            })
            .sum::<usize>()
}
fn main() {
    let a: Vec<_> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let mut tree = Snapshot::from_bytes(&bytes).unwrap();
    let log = std::fs::read(format!("{}.log", a[1])).unwrap();
    let mut at = 0;
    let mut previous = tree.clone();
    let mut count = 0;
    let mut last_delta = 0;
    while at < log.len() {
        let n = u32::from_le_bytes(log[at..at + 4].try_into().unwrap()) as usize;
        let delta = &log[at + 36..at + 36 + n];
        assert_eq!(hash(delta).as_slice(), &log[at + 4..at + 36]);
        previous = tree.clone();
        tree = tree.apply_delta(delta).unwrap();
        last_delta = n;
        at += 36 + n;
        count += 1;
    }
    println!(
        "{{\"journal_records\":{count},\"last_raw_delta_bytes\":{last_delta},\"changed_node_bytes\":{},\"root_record_bytes\":{}}}",
        changed(Some(&previous), &tree),
        own(&tree)
    );
    for (kind, name) in [(1, "fields"), (2, "receipts"), (3, "signals")] {
        if let Ok(branch) = tree.subtree(&[kind]) {
            let old = previous.subtree(&[kind]).ok();
            let mut payload = 0;
            for bucket in branch.child_keys() {
                let b = branch.subtree(&[bucket]).unwrap();
                for key in b.child_keys() {
                    let n = b.subtree(&[key]).unwrap();
                    if let Some(r) = n.register() {
                        for (_, _, v) in r.values() {
                            payload += v.map_or(0, |v| v.len());
                        }
                    }
                }
            }
            println!(
                "{{\"branch\":\"{name}\",\"nodes\":{},\"canonical_bytes\":{},\"register_payload_bytes\":{payload},\"changed_node_bytes\":{}}}",
                branch.node_count(),
                branch.canonical_bytes(),
                changed(old.as_ref(), &branch)
            );
        }
    }
}
