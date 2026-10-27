// SPDX-License-Identifier: LGPL-3.0-only
//! Deterministic bounded source for qualification on the real synchronization
//! transport. Hosts opt in explicitly; no synthetic source is installed by default.
use super::*;
use crate::merkle::{Edit, Snapshot};
use std::collections::BTreeMap;

pub fn synthetic_schema() -> Schema {
    Schema {
        id: [24; 32],
        fields: BTreeMap::from([(1, [1; 32]), (2, [2; 32])]),
    }
}
#[derive(Default)]
pub struct Synthetic {
    records: u32,
    retain_from: u64,
    installed: bool,
}
impl Synthetic {
    /// Collection 24 uses sparse commit keys (3 * ordinal), distinct public
    /// bodies and a private canary field. Trusted fixture controls are monotonic.
    pub fn update(&mut self, channel: &mut Channel, records: u32, retain_from: u64) -> Result<()> {
        if records > 100_000 || records < self.records || retain_from < self.retain_from {
            return Err(Error::Limit);
        }
        if self.installed && records == self.records && retain_from == self.retain_from {
            return Ok(());
        }
        let mut next = if self.installed {
            channel.collection(24)?.clone()
        } else {
            Collection::new(
                24,
                synthetic_schema(),
                Budget {
                    records: 100_000,
                    canonical_bytes: 64 * 1024 * 1024,
                },
            )?
        };
        for ordinal in self.records + 1..=records {
            let row = Snapshot::empty()
                .edit(
                    [1; 16],
                    &[
                        Edit {
                            path: vec![1],
                            schema: [1; 32],
                            value: Some(
                                format!("record-{ordinal:08} {}", "x".repeat(112)).into_bytes(),
                            ),
                        },
                        Edit {
                            path: vec![2],
                            schema: [2; 32],
                            value: Some(b"private-canary".to_vec()),
                        },
                    ],
                )?
                .0;
            next = next.append(u64::from(ordinal) * 3, row)?;
        }
        next = next.retain_from(retain_from);
        if self.installed {
            channel.publish(24, channel.collection(24)?.revision(), next)?;
        } else {
            channel.install(
                next,
                Policy {
                    public_fields: vec![1],
                    authenticated_fields: vec![2],
                    admitted_fields: vec![],
                },
            )?;
        }
        self.records = records;
        self.retain_from = retain_from;
        self.installed = true;
        Ok(())
    }
}
