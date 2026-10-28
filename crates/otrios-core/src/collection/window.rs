// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::merkle::{Coverage, Edit, Hash, Snapshot};
use sha2::{Digest, Sha256};

const VIEW_DOMAIN: &[u8] = b"OTriOS\0collection-window\0v1\0";
pub(super) const MAX_VIEW_BYTES: usize = 1024 * 1024;
fn membership_schema() -> Hash {
    Sha256::digest(VIEW_DOMAIN).into()
}
pub(super) fn key_path(key: u64) -> Vec<u64> {
    (0..13)
        .rev()
        .map(|shift| (key >> (shift * 5)) & 31)
        .collect()
}

/// One qualified selected publication. Memory accounting here is canonical
/// selected bytes, not allocator residency; layout and GPU budgets are separate.
#[derive(Clone, Debug)]
pub struct Window {
    pub revision: Hash,
    pub keys: Vec<u64>,
    pub selected_bytes: usize,
    pub more_before: bool,
    pub more_after: bool,
    pub work: IndexWork,
    pub(super) snapshot: Snapshot,
}
impl Window {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn record(&self, key: u64) -> Result<Snapshot> {
        if self.keys.binary_search(&key).is_err() {
            return Err(Error::Invalid);
        }
        Ok(self.snapshot.subtree(&key_path(key))?)
    }
}

pub(super) fn check_plan(collection: u64, schema: &Schema, plan: &Selection) -> Result<()> {
    if plan.collection != collection || plan.schema != schema.id {
        return Err(Error::Schema);
    }
    if plan.record_limit == 0
        || usize::from(plan.record_limit) > MAX_WINDOW_RECORDS
        || plan.byte_limit == 0
        || plan.byte_limit as usize > MAX_WINDOW_BYTES
        || plan.fields.is_empty()
        || plan.fields.len() > MAX_FIELDS
    {
        return Err(Error::Limit);
    }
    if plan.fields.windows(2).any(|p| p[0] >= p[1]) {
        return Err(Error::Invalid);
    }
    if plan.fields.iter().any(|f| !schema.fields.contains_key(f)) {
        return Err(Error::Schema);
    }
    Ok(())
}

impl Collection {
    pub fn select(&self, epoch: u64, grant: &Grant, plan: &Selection) -> Result<Window> {
        // Deny before seeking (including anchor-existence disclosure).
        if grant.collection != self.id
            || plan.collection != self.id
            || plan.fields.iter().any(|f| !grant.fields.contains(f))
        {
            return Err(Error::Forbidden);
        }
        check_plan(self.id, &self.schema, plan)?;
        let mut work = IndexWork::default();
        let limit = usize::from(plan.record_limit);
        let (start, end, around) = match plan.anchor {
            Anchor::Tail => (self.len().saturating_sub(limit), self.len(), None),
            Anchor::Before(cursor) => {
                if cursor.epoch != epoch || cursor.collection != self.id {
                    return Err(Error::Stale);
                }
                let end = self.rank(cursor.key, &mut work)?;
                (end.saturating_sub(limit), end, None)
            }
            Anchor::Around(key) => {
                let rank = self.rank(key, &mut work)?;
                let start = rank
                    .saturating_sub(limit / 2)
                    .min(self.len().saturating_sub(limit));
                (start, (start + limit).min(self.len()), Some(rank - start))
            }
        };
        let records = self.range(start, end, &mut work);
        let coverage = Coverage::new(
            &plan.fields.iter().map(|f| vec![*f]).collect::<Vec<_>>(),
            &[],
        )?;
        // Tail/before prioritize the most recent rows. Around prioritizes the
        // anchor, then neighboring rows. Stop explicitly at the byte boundary.
        let order = if let Some(center) = around {
            let mut order = vec![center];
            for distance in 1..records.len() {
                if center >= distance {
                    order.push(center - distance);
                }
                if center + distance < records.len() {
                    order.push(center + distance);
                }
            }
            order
        } else {
            (0..records.len()).rev().collect()
        };
        let mut chosen = Vec::new();
        let mut bytes = 0;
        for i in order {
            let row = records[i].data.project(&coverage)?.0;
            let size = row.canonical_bytes();
            if bytes + size > plan.byte_limit as usize {
                if chosen.is_empty() {
                    return Err(Error::Limit);
                }
                break;
            }
            bytes += size;
            chosen.push((i, records[i].key, row));
        }
        chosen.sort_by_key(|(i, _, _)| *i);
        // The around walk alternates left/right and therefore remains contiguous
        // even if the next neighbor would exceed the byte budget.
        let more_before = chosen.first().map_or(start > 0, |(i, _, _)| start + i > 0);
        let more_after = chosen
            .last()
            .map_or(end < self.len(), |(i, _, _)| start + i + 1 < self.len());
        let rows = chosen
            .into_iter()
            .map(|(_, key, row)| (key, row))
            .collect::<Vec<_>>();
        let snapshot = assemble(plan, &rows, more_before, more_after)?;
        Ok(Window {
            revision: self.revision(),
            keys: rows.iter().map(|(key, _)| *key).collect(),
            selected_bytes: bytes,
            more_before,
            more_after,
            work,
            snapshot,
        })
    }
}

fn assemble(
    plan: &Selection,
    rows: &[(u64, Snapshot)],
    more_before: bool,
    more_after: bool,
) -> Result<Snapshot> {
    let mut manifest = VIEW_DOMAIN.to_vec();
    manifest.extend(plan.collection.to_le_bytes());
    manifest.extend(plan.schema);
    manifest.push(plan.fields.len() as u8);
    for field in &plan.fields {
        manifest.extend(field.to_le_bytes());
    }
    manifest.push(u8::from(more_before));
    manifest.push(u8::from(more_after));
    manifest.push(rows.len() as u8);
    for (key, row) in rows {
        manifest.extend(key.to_le_bytes());
        manifest.extend(row.hash());
    }
    let mut snapshot = Snapshot::empty()
        .edit(
            [0; 16],
            &[Edit {
                path: vec![],
                schema: membership_schema(),
                value: Some(manifest),
            }],
        )?
        .0;
    for (key, row) in rows {
        snapshot = snapshot.merge_subtree(&key_path(*key), row)?.0;
    }
    if snapshot.canonical_bytes() > MAX_VIEW_BYTES {
        return Err(Error::Limit);
    }
    Ok(snapshot)
}

/// Qualify selected membership, all referenced causal records and exact scope
/// before exposing any new rows. Reassembly also rejects hidden extra paths.
pub(super) fn qualify(
    plan: &Selection,
    schema: &Schema,
    snapshot: Snapshot,
    revision: Hash,
    more_before: bool,
    more_after: bool,
) -> Result<Window> {
    check_plan(plan.collection, schema, plan)?;
    if snapshot.canonical_bytes() > MAX_VIEW_BYTES {
        return Err(Error::Limit);
    }
    let register = snapshot.register().ok_or(Error::Invalid)?;
    if register.schema() != membership_schema() {
        return Err(Error::Schema);
    }
    let values = register.values().collect::<Vec<_>>();
    if values.len() != 1 {
        return Err(Error::Invalid);
    }
    let bytes = values[0].2.ok_or(Error::Invalid)?;
    // Check the fixed prefix against the compiled plan before reading lengths.
    let mut prefix = VIEW_DOMAIN.to_vec();
    prefix.extend(plan.collection.to_le_bytes());
    prefix.extend(plan.schema);
    prefix.push(plan.fields.len() as u8);
    for field in &plan.fields {
        prefix.extend(field.to_le_bytes());
    }
    prefix.push(u8::from(more_before));
    prefix.push(u8::from(more_after));
    if !bytes.starts_with(&prefix) {
        return Err(Error::Schema);
    }
    let rest = &bytes[prefix.len()..];
    let count = usize::from(*rest.first().ok_or(Error::Invalid)?);
    if count > usize::from(plan.record_limit) || rest.len() != 1 + count * 40 {
        return Err(Error::Invalid);
    }
    let mut rows = Vec::with_capacity(count);
    let mut selected_bytes = 0;
    for record in rest[1..].chunks_exact(40) {
        let key = u64::from_le_bytes(record[..8].try_into().map_err(|_| Error::Invalid)?);
        if key == 0 || rows.last().is_some_and(|(last, _)| *last >= key) {
            return Err(Error::Invalid);
        }
        let row = snapshot.subtree(&key_path(key))?;
        if row.hash() != record[8..]
            || row.register().is_some()
            || row.child_keys().collect::<Vec<_>>() != plan.fields
        {
            return Err(Error::Invalid);
        }
        for field in &plan.fields {
            let leaf = row.subtree(&[*field])?;
            if leaf.child_keys().next().is_some()
                || leaf
                    .register()
                    .is_none_or(|r| r.schema() != schema.fields[field])
            {
                return Err(Error::Schema);
            }
        }
        selected_bytes += row.canonical_bytes();
        if selected_bytes > plan.byte_limit as usize {
            return Err(Error::Limit);
        }
        rows.push((key, row));
    }
    if assemble(plan, &rows, more_before, more_after)?.hash() != snapshot.hash() {
        return Err(Error::Invalid);
    }
    Ok(Window {
        revision,
        keys: rows.iter().map(|(key, _)| *key).collect(),
        selected_bytes,
        more_before,
        more_after,
        work: IndexWork::default(),
        snapshot,
    })
}
