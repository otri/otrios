// SPDX-License-Identifier: LGPL-3.0-only
//! Same-tree comparison of full, writer-local and eight-field coherent views.
//! No network, durable Canvas policy, renderer or physical latency claim.
use otrios_core::merkle::*;
use std::time::Instant;
fn path(kind: u64, slot: u64) -> Path {
    vec![kind, slot / 32, slot % 32]
}
fn edit(path: Path, number: u64, size: usize) -> Edit {
    let mut bytes = vec![0; size];
    bytes[..8].copy_from_slice(&number.to_le_bytes());
    Edit {
        path,
        schema: [7; 32],
        value: Some(bytes),
    }
}
fn quantile(samples: &mut [f64], percent: usize) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[(samples.len() * percent / 100).min(samples.len() - 1)]
}
fn trial(seed: &Snapshot, slots: u64, scenario: &str, view: &str, repetition: usize) -> Result<()> {
    let mut authority = Authority::new(1, seed.clone());
    let mut sub = match view {
        "full" => authority.subscribe(&[vec![]], vec![])?,
        "own_receipt" => authority
            .subscribe_coverage(&[vec![]], Coverage::new(&[vec![], path(2, 0)], &[vec![1]])?)?,
        _ => {
            let mut registers = vec![vec![], path(2, 0)];
            registers.extend((0..8).map(|slot| path(1, slot)));
            authority.subscribe_coverage(&[vec![]], Coverage::new(&registers, &[])?)?
        }
    };
    let mut replica = Replica::new(&sub);
    let first = authority.offer(&mut sub)?.ok_or(Error::Missing)?;
    let mount_bytes = first.bytes().len();
    replica.apply(first.bytes())?;
    sub.acknowledge(first.cursor, first.root)?;
    let mut edit_times = Vec::new();
    let mut offer_times = Vec::new();
    let mut receive_times = Vec::new();
    let mut bytes = 0;
    let mut offers = 0;
    let mut workspace = Workspace::default();
    for sequence in 0..600_u64 {
        let commands = match scenario {
            "selected" => vec![
                edit(path(1, 0), sequence, 8),
                edit(path(2, 0), sequence, 128),
            ],
            "other_receipt" => vec![edit(path(2, slots - 1), sequence, 128)],
            _ => vec![edit(path(1, 63), sequence, 8)],
        };
        let started = Instant::now();
        let (next, _) =
            authority
                .snapshot()
                .edit_with_workspace([1; 16], &commands, &mut workspace)?;
        let edit_us = started.elapsed().as_secs_f64() * 1e6;
        let started = Instant::now();
        authority.publish(authority.snapshot().hash(), next)?;
        let offer = authority.offer(&mut sub)?;
        let offer_us = started.elapsed().as_secs_f64() * 1e6;
        let started = Instant::now();
        if let Some(offer) = offer {
            replica.apply(offer.bytes())?;
            sub.acknowledge(offer.cursor, offer.root)?;
            if sequence >= 100 {
                bytes += offer.bytes().len();
                offers += 1;
            }
        }
        let receive_us = started.elapsed().as_secs_f64() * 1e6;
        if sequence >= 100 {
            edit_times.push(edit_us);
            offer_times.push(offer_us);
            receive_times.push(receive_us);
        }
    }
    println!(
        "{{\"repetition\":{repetition},\"initial_receipts\":{slots},\"scenario\":\"{scenario}\",\"view\":\"{view}\",\"warmup\":100,\"updates\":500,\"offers\":{offers},\"mount_bytes\":{mount_bytes},\"delta_bytes_per_update\":{},\"edit_p50_us\":{:.3},\"offer_p50_us\":{:.3},\"offer_p95_us\":{:.3},\"receive_p50_us\":{:.3},\"receive_p95_us\":{:.3},\"retained_view_nodes\":{},\"retained_view_canonical_bytes\":{}}}",
        bytes as f64 / 500.,
        quantile(&mut edit_times, 50),
        quantile(&mut offer_times, 50),
        quantile(&mut offer_times, 95),
        quantile(&mut receive_times, 50),
        quantile(&mut receive_times, 95),
        replica.snapshot().unwrap().node_count(),
        replica.snapshot().unwrap().canonical_bytes()
    );
    Ok(())
}
fn main() -> Result<()> {
    for slots in [2, 128, 1024] {
        let mut commands = vec![edit(vec![], 0, 64)];
        commands.extend((0..64).map(|slot| edit(path(1, slot), 0, 8)));
        commands.extend((0..slots).map(|slot| edit(path(2, slot), 0, 128)));
        let mut seed = Snapshot::empty();
        for chunk in commands.chunks(MAX_EDITS) {
            seed = seed.edit([1; 16], chunk)?.0;
        }
        for scenario in ["selected", "other_receipt", "unselected_field"] {
            for repetition in 0..3 {
                let views = ["full", "own_receipt", "eight_fields"];
                for offset in 0..3 {
                    trial(
                        &seed,
                        slots,
                        scenario,
                        views[(repetition + offset) % 3],
                        repetition,
                    )?;
                }
            }
        }
    }
    Ok(())
}
