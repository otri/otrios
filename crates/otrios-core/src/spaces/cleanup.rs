// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded maintenance and truthful deletion progress; no remote-wipe claim.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Maintenance {
    reclaimed: BTreeSet<String>,
    failures: BTreeSet<String>,
    cursor: usize,
    retained_at: Option<u64>,
    backups: BTreeMap<String, u64>,
    originals: BTreeMap<String, Option<u64>>,
    unknown: bool,
}

impl Store {
    /// Active producers and connected replicas awaiting disposal acknowledgement
    /// are supplied under the authority's admission lock. A disconnected replica
    /// is unverified; these summaries describe authority-managed storage only.
    pub fn maintain_deleted(
        &mut self,
        now_unix_ms: u64,
        active: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, CleanupSummary>> {
        let registry = self.registry()?.clone();
        if self
            .maintenance
            .retained_at
            .is_none_or(|last| now_unix_ms < last || now_unix_ms.saturating_sub(last) >= 60_000)
        {
            let backup_pruned = self.prune_backups(now_unix_ms).is_ok();
            let originals_pruned = self.prune_legacy_sources(now_unix_ms).is_ok();
            let backups = self.backup_deadlines();
            let originals = self.legacy_deadlines();
            self.maintenance.unknown =
                !backup_pruned || !originals_pruned || backups.is_err() || originals.is_err();
            self.maintenance.backups = backups.unwrap_or_default();
            self.maintenance.originals = originals.unwrap_or_default();
            self.maintenance.retained_at = Some(now_unix_ms);
        }
        self.registry()?;
        // At most eight filesystem reclamations per turn. Skip already completed
        // work in RAM, and rotate past failures or blocked clients for fairness.
        let mut attempted = 0;
        for _ in 0..registry.deletions.len() {
            if attempted == 8 {
                break;
            }
            let index = self.maintenance.cursor % registry.deletions.len();
            self.maintenance.cursor = (index + 1) % registry.deletions.len();
            let collection = &registry.deletions[index].collection;
            if self.maintenance.reclaimed.contains(collection) {
                continue;
            }
            attempted += 1;
            match self.reclaim_deleted(collection, !active.contains(collection)) {
                Ok(true) => {
                    self.maintenance.reclaimed.insert(collection.clone());
                    self.maintenance.failures.remove(collection);
                }
                Ok(false) => {
                    self.maintenance.failures.remove(collection);
                }
                Err(_) => {
                    self.maintenance.failures.insert(collection.clone());
                }
            }
        }
        let mut summaries = registry
            .spaces
            .iter()
            .map(|space| {
                (
                    space.id.clone(),
                    CleanupSummary {
                        storage_unknown: self.maintenance.unknown,
                        ..CleanupSummary::default()
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        for deletion in &registry.deletions {
            let Some(summary) = summaries.get_mut(&deletion.space) else {
                // Earlier experimental registries did not retain a Space scope.
                // Do not disclose another Space's IDs or invent attribution.
                for summary in summaries.values_mut() {
                    summary.storage_unknown = true;
                }
                continue;
            };
            summary.deleted_collections += 1;
            summary.local_pending +=
                u32::from(!self.maintenance.reclaimed.contains(&deletion.collection));
            summary.storage_unknown |= self.maintenance.failures.contains(&deletion.collection);
            if let Some(until) = self.maintenance.backups.get(&deletion.collection) {
                summary.backup_until_unix_ms =
                    Some(summary.backup_until_unix_ms.unwrap_or_default().max(*until));
            }
            match self.maintenance.originals.get(&deletion.collection) {
                Some(Some(until)) => {
                    summary.legacy_until_unix_ms =
                        Some(summary.legacy_until_unix_ms.unwrap_or_default().max(*until));
                }
                Some(None) => summary.legacy_qualification_required = true,
                None => (),
            }
        }
        Ok(summaries)
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
