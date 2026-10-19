// SPDX-License-Identifier: LGPL-3.0-only
//! Hash-work experiment only: cached ancestor path, no CRDT store/merge/IPC.
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut leaf = [0_u8; 1024];
    let mut ancestors = [[[1_u8; 32]; 32]; 4];
    let mut samples = Vec::with_capacity(1000);
    for revision in 0_u32..1000 {
        leaf[..4].copy_from_slice(&revision.to_le_bytes());
        let start = Instant::now();
        let mut hash = Sha256::new();
        hash.update(b"OTriOS\0experiment-leaf\0v1\0");
        hash.update((leaf.len() as u32).to_le_bytes());
        hash.update(black_box(&leaf));
        let mut child: [u8; 32] = hash.finalize().into();
        for parent in &mut ancestors {
            parent[0] = child;
            let mut hash = Sha256::new();
            hash.update(b"OTriOS\0experiment-branch\0v1\0");
            hash.update((parent.len() as u32).to_le_bytes());
            for (key, digest) in parent.iter().enumerate() {
                hash.update((key as u32).to_le_bytes());
                hash.update(digest);
            }
            child = hash.finalize().into();
        }
        black_box(child);
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.);
    }
    samples.sort_by(f64::total_cmp);
    println!("Hashing only; 1 KiB leaf + four cached 32-way ancestors; 1000 edits");
    println!(
        "SHA-256 calls/edit: 5; p50: {:.3} us; p95: {:.3} us; max: {:.3} us",
        samples[500], samples[949], samples[999]
    );
    println!(
        "Tree shape fits 32^4 leaves; full tree not allocated; excludes CRDT merge, canonicalization, allocation, I/O and rendering"
    );
}
