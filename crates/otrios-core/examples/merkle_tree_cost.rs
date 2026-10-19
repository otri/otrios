// SPDX-License-Identifier: LGPL-3.0-only
//! Materialized-tree edit/transfer/receive timings, NOT durable or display latency.
use otrios_core::merkle::*;
use std::time::Instant;
fn path(mut index: u64) -> Path {
    let mut path = vec![0; 4];
    for key in path.iter_mut().rev() {
        *key = index % 8;
        index /= 8;
    }
    path
}
fn edit(index: u64, value: Vec<u8>) -> Edit {
    Edit {
        path: path(index),
        schema: [7; 32],
        value: Some(value),
    }
}
fn percentile(samples: &mut [f64]) -> (f64, f64, f64) {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    (
        samples[n / 2],
        samples[(n * 95 / 100).min(n - 1)],
        samples[(n * 99 / 100).min(n - 1)],
    )
}
fn main() -> Result<()> {
    for count in [64, 512, 4096] {
        let mut tree = Snapshot::empty();
        let mut workspace = Workspace::default();
        for first in (0..count).step_by(MAX_EDITS) {
            let edits = (first..(first + MAX_EDITS as u64).min(count))
                .map(|i| {
                    let mut value = vec![0; 1024];
                    value[..8].copy_from_slice(&i.to_le_bytes());
                    edit(i, value)
                })
                .collect::<Vec<_>>();
            tree = tree.edit_with_workspace([1; 16], &edits, &mut workspace)?.0;
        }
        let nodes = tree.node_count();
        let bytes = tree.canonical_bytes();
        let mut authority = Authority::new(1, tree);
        let mut sub = authority.subscribe(&[vec![]], vec![])?;
        let mut replica = Replica::new(&sub);
        let initial = authority.offer(&mut sub)?.ok_or(Error::Missing)?;
        let full_bytes = initial.bytes().len();
        replica.apply(initial.bytes())?;
        sub.acknowledge(initial.cursor, initial.root)?;
        let mut roundtrips = Vec::new();
        let mut edits = Vec::new();
        let mut transfers = Vec::new();
        let mut receives = Vec::new();
        let mut merges = Vec::new();
        let mut last = (0, 0, 0);
        let mut encoding_buffer_grows = 0;
        for i in 0..1100_u32 {
            let mut payload = vec![0; 1024];
            payload[..4].copy_from_slice(&i.to_le_bytes());
            let command = edit(0, payload);
            let roundtrip_start = Instant::now();
            let start = Instant::now();
            let (next, work) =
                authority
                    .snapshot()
                    .edit_with_workspace([1; 16], &[command], &mut workspace)?;
            let edit_us = start.elapsed().as_secs_f64() * 1e6;
            let start = Instant::now();
            authority.publish(authority.snapshot().hash(), next)?;
            let offer = authority.offer(&mut sub)?.ok_or(Error::Missing)?;
            let transfer_us = start.elapsed().as_secs_f64() * 1e6;
            let start = Instant::now();
            replica.apply(offer.bytes())?;
            let receive_us = start.elapsed().as_secs_f64() * 1e6;
            assert_eq!(replica.snapshot().map(Snapshot::hash), offer.root);
            sub.acknowledge(offer.cursor, offer.root)?;
            let roundtrip_us = roundtrip_start.elapsed().as_secs_f64() * 1e6;
            let concurrent = authority
                .snapshot()
                .edit([2; 16], &[edit(0, vec![42; 1024])])?
                .0;
            let branch = authority
                .snapshot()
                .edit([3; 16], &[edit(0, vec![43; 1024])])?
                .0;
            let start = Instant::now();
            let merged = concurrent.merge(&branch)?;
            std::hint::black_box(merged);
            let merge_us = start.elapsed().as_secs_f64() * 1e6;
            if i >= 100 {
                encoding_buffer_grows += work.encoding_buffer_grows;
                roundtrips.push(roundtrip_us);
                edits.push(edit_us);
                transfers.push(transfer_us);
                receives.push(receive_us);
                merges.push(merge_us);
            }
            last = (work.hashed_nodes, work.hashed_bytes, offer.bytes().len());
        }
        println!(
            "leaves={count} nodes={nodes} canonical_bytes={bytes} full_wire_bytes={full_bytes} hashed_nodes={} hashed_bytes={} delta_bytes={}",
            last.0, last.1, last.2
        );
        println!(
            "encoding_buffer_growths_in_1000_edits={encoding_buffer_grows} retained_encoding_capacity={}",
            workspace.capacity()
        );
        for (name, mut samples) in [
            ("edit_offer_receive_ack", roundtrips),
            ("edit", edits),
            ("offer", transfers),
            ("receive", receives),
            ("concurrent_merge", merges),
        ] {
            let (p50, p95, p99) = percentile(&mut samples);
            println!("{name}_us p50={p50:.3} p95={p95:.3} p99={p99:.3}");
        }
    }
    println!(
        "100 warmup + 1000 measured edits; real 1 KiB registers, four ancestors, 8-way keys; allocations included; counts cover only hash-encoding buffer growth; no network, durable I/O, selector or display latency"
    );
    Ok(())
}
