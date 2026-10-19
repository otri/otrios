// SPDX-License-Identifier: LGPL-3.0-only
//! Read admission, serialization and replica costs; excludes network and fsync.
use otrios_core::canvas::*;
use otrios_interfaces::{
    canvas::{Field, StateSchema},
    layout::Value,
};
use std::time::Instant;
fn q(values: &mut [f64], percent: usize) -> f64 {
    values.sort_by(f64::total_cmp);
    values[(values.len() * percent / 100).min(values.len() - 1)]
}
fn main() -> Result<()> {
    for repetition in 0..3 {
        let schema = StateSchema {
            id: "reader-cost-v1".into(),
            fields: vec![Field {
                id: 1,
                name: "status".into(),
                default: Value::Number(0.),
                writable: false,
            }],
        };
        let mut doc = Document::new(schema.clone(), "reader-cost")?;
        let mut times = Vec::new();
        let mut journal_bytes = 0;
        let mut wire_bytes = 0;
        let mut changed_reads = 0;
        let original = doc.root_hash();
        for id in 0..170_u64 {
            let mut writer = [0; 16];
            writer[..8].copy_from_slice(&id.to_le_bytes());
            let before = doc.clone();
            let started = Instant::now();
            let (mut session, changed) = doc.open_session(&writer, 0)?;
            session.set_binary_transport(true);
            let frame = doc.generate(&mut session)?.ok_or("No reader mount")?;
            let mut client = StateAdapter::new(schema.clone(), "reader-cost", &frame, writer)?;
            assert!(client.confirmed());
            assert_eq!(client.value(1)?, 0);
            while let Some(ack) = client.generate()? {
                doc.receive(&mut session, &ack)?;
            }
            let us = started.elapsed().as_secs_f64() * 1e6;
            if id >= 20 {
                times.push(us);
                changed_reads += usize::from(changed);
                wire_bytes += frame.len();
                if changed {
                    journal_bytes += doc.journal_delta(&before)?.len();
                }
            }
        }
        let final_bytes = doc.save()?.len();
        println!(
            "{{\"repetition\":{repetition},\"warmup\":20,\"mounts\":150,\"durable_changes\":{changed_reads},\"journal_payload_bytes\":{journal_bytes},\"wire_bytes_per_mount\":{},\"admit_offer_receive_ack_p50_us\":{:.3},\"admit_offer_receive_ack_p95_us\":{:.3},\"final_checkpoint_bytes\":{final_bytes},\"root_unchanged\":{}}}",
            wire_bytes as f64 / 150.,
            q(&mut times, 50),
            q(&mut times, 95),
            doc.root_hash() == original
        );
    }
    Ok(())
}
