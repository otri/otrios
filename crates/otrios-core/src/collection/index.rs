// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::merkle::{Hash, Snapshot};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub(super) const PAGE_SIZE: usize = 32;
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_RECORDS: usize = 1_000_000;
const MAX_COLLECTION_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schema {
    pub id: Hash,
    /// Flat field IDs and their causal-register schemas. Nested query plans are
    /// outside this first ordered-window contract.
    pub fields: BTreeMap<u64, Hash>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    pub records: usize,
    pub canonical_bytes: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            records: MAX_RECORDS,
            canonical_bytes: MAX_COLLECTION_BYTES,
        }
    }
}
/// Deterministic traversal counters. They exclude allocator/RSS and wire costs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexWork {
    pub pages_visited: usize,
    pub records_considered: usize,
}
#[derive(Clone, Debug)]
pub(super) struct Record {
    pub key: u64,
    pub data: Snapshot,
}
#[derive(Clone, Debug)]
enum Items {
    Leaf(Vec<Record>),
    Branch(Vec<Arc<Page>>),
}
#[derive(Debug)]
struct Page {
    hash: Hash,
    first: u64,
    last: u64,
    count: usize,
    /// Total canonical record and index bytes reachable from this page.
    bytes: usize,
    items: Items,
}

/// Authority-side streaming traversal for recovery/compaction. This deliberately
/// visits the whole retained collection; viewport selection never calls it.
pub struct Records<'a> {
    stack: Vec<(&'a Page, usize)>,
    remaining: usize,
}
impl<'a> Iterator for Records<'a> {
    type Item = (u64, &'a Snapshot);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (page, position) = self.stack.last_mut()?;
            let index = *position;
            *position += 1;
            let page = *page;
            match &page.items {
                Items::Leaf(records) => {
                    if let Some(record) = records.get(index) {
                        self.remaining -= 1;
                        return Some((record.key, &record.data));
                    }
                    self.stack.pop();
                }
                Items::Branch(pages) => {
                    if let Some(page) = pages.get(index) {
                        self.stack.push((page, 0));
                    } else {
                        self.stack.pop();
                    }
                }
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl ExactSizeIterator for Records<'_> {}
impl Page {
    fn at_or_after(&self, key: u64) -> Option<u64> {
        if key > self.last {
            return None;
        }
        match &self.items {
            Items::Leaf(records) => records
                .get(records.partition_point(|r| r.key < key))
                .map(|r| r.key),
            Items::Branch(pages) => pages.iter().find(|p| p.last >= key)?.at_or_after(key),
        }
    }
    fn new(items: Items) -> Arc<Self> {
        // Each page is its own bounded canonical object; no record bodies here.
        // Child bounds/counts/byte totals participate in the hash, as do every
        // record's causal Merkle digest. Integers are unsigned little endian.
        let mut out = b"OTriOS\0collection-index\0v1\0".to_vec();
        let (first, last, count, children_bytes) = match &items {
            Items::Leaf(records) => {
                debug_assert!(!records.is_empty() && records.len() <= PAGE_SIZE);
                out.push(0);
                out.push(records.len() as u8);
                let mut bytes = 0;
                for r in records {
                    out.extend(r.key.to_le_bytes());
                    out.extend(r.data.hash());
                    out.extend((r.data.canonical_bytes() as u64).to_le_bytes());
                    bytes += r.data.canonical_bytes();
                }
                (
                    records[0].key,
                    records.last().unwrap().key,
                    records.len(),
                    bytes,
                )
            }
            Items::Branch(pages) => {
                debug_assert!(!pages.is_empty() && pages.len() <= PAGE_SIZE);
                out.push(1);
                out.push(pages.len() as u8);
                let (mut count, mut bytes) = (0, 0);
                for p in pages {
                    out.extend(p.first.to_le_bytes());
                    out.extend(p.last.to_le_bytes());
                    out.extend((p.count as u64).to_le_bytes());
                    out.extend((p.bytes as u64).to_le_bytes());
                    out.extend(p.hash);
                    count += p.count;
                    bytes += p.bytes;
                }
                (pages[0].first, pages.last().unwrap().last, count, bytes)
            }
        };
        Arc::new(Self {
            hash: Sha256::digest(&out).into(),
            first,
            last,
            count,
            bytes: children_bytes + out.len(),
            items,
        })
    }
    /// Copy only the right spine. Splits preserve a deterministic append shape.
    fn append(&self, record: Record) -> (Arc<Self>, Option<Arc<Self>>) {
        match &self.items {
            Items::Leaf(records) => {
                if records.len() == PAGE_SIZE {
                    return (
                        Self::new(self.items.clone()),
                        Some(Self::new(Items::Leaf(vec![record]))),
                    );
                }
                let mut next = records.clone();
                next.push(record);
                (Self::new(Items::Leaf(next)), None)
            }
            Items::Branch(pages) => {
                let mut next = pages.clone();
                let (last, extra) = next.last().unwrap().append(record);
                *next.last_mut().unwrap() = last;
                if let Some(extra) = extra {
                    if next.len() == PAGE_SIZE {
                        return (
                            Self::new(Items::Branch(next)),
                            Some(Self::new(Items::Branch(vec![extra]))),
                        );
                    }
                    next.push(extra);
                }
                (Self::new(Items::Branch(next)), None)
            }
        }
    }
    fn prune(&self, key: u64) -> Option<Arc<Self>> {
        if self.last < key {
            return None;
        }
        match &self.items {
            Items::Leaf(records) => {
                let from = records.partition_point(|r| r.key < key);
                Some(Self::new(Items::Leaf(records[from..].to_vec())))
            }
            Items::Branch(pages) => {
                let mut next = Vec::new();
                for page in pages {
                    if page.first >= key {
                        next.push(page.clone());
                    } else if let Some(page) = page.prune(key) {
                        next.push(page);
                    }
                }
                Some(Self::new(Items::Branch(next)))
            }
        }
    }
    fn rank(&self, key: u64, work: &mut IndexWork) -> Option<usize> {
        work.pages_visited += 1;
        if key < self.first || key > self.last {
            return None;
        }
        match &self.items {
            Items::Leaf(records) => records.binary_search_by_key(&key, |r| r.key).ok(),
            Items::Branch(pages) => {
                let mut before = 0;
                for page in pages {
                    if key <= page.last {
                        return page.rank(key, work).map(|n| n + before);
                    }
                    before += page.count;
                }
                None
            }
        }
    }
    fn range(&self, start: usize, end: usize, out: &mut Vec<Record>, work: &mut IndexWork) {
        work.pages_visited += 1;
        match &self.items {
            Items::Leaf(records) => {
                let selected = &records[start..end];
                work.records_considered += selected.len();
                out.extend_from_slice(selected);
            }
            Items::Branch(pages) => {
                let mut at = 0;
                for page in pages {
                    if at >= end {
                        break;
                    }
                    if at + page.count > start {
                        page.range(
                            start.saturating_sub(at),
                            (end - at).min(page.count),
                            out,
                            work,
                        );
                    }
                    at += page.count;
                }
            }
        }
    }
}

/// A pinned immutable collection publication. Clones share index pages and
/// record DAGs. Retention removes coverage, never reuses committed identities.
#[derive(Clone, Debug)]
pub struct Collection {
    pub(super) id: u64,
    pub(super) schema: Arc<Schema>,
    budget: Budget,
    root: Option<Arc<Page>>,
    high_water: u64,
    revision: Hash,
}
impl Collection {
    /// First retained key >= the missing anchor, or the last retained key if
    /// the anchor is beyond the tail. Bounded page descent, never a log scan.
    pub fn neighbor(&self, key: u64) -> Option<u64> {
        self.root
            .as_ref()
            .and_then(|page| page.at_or_after(key).or(Some(page.last)))
    }
    pub fn records(&self) -> Records<'_> {
        Records {
            stack: self
                .root
                .as_ref()
                .map(|page| vec![(page.as_ref(), 0)])
                .unwrap_or_default(),
            remaining: self.len(),
        }
    }
    pub fn new(id: u64, schema: Schema, budget: Budget) -> Result<Self> {
        if id == 0 || schema.fields.is_empty() || schema.fields.len() > MAX_FIELDS {
            return Err(Error::Schema);
        }
        if budget.records == 0
            || budget.records > MAX_RECORDS
            || budget.canonical_bytes == 0
            || budget.canonical_bytes > MAX_COLLECTION_BYTES
        {
            return Err(Error::Limit);
        }
        let mut this = Self {
            id,
            schema: Arc::new(schema),
            budget,
            root: None,
            high_water: 0,
            revision: [0; 32],
        };
        this.rehash();
        Ok(this)
    }
    fn rehash(&mut self) {
        let mut out = b"OTriOS\0collection-root\0v1\0".to_vec();
        out.extend(self.id.to_le_bytes());
        out.extend(self.schema.id);
        out.push(self.schema.fields.len() as u8);
        for (field, schema) in &self.schema.fields {
            out.extend(field.to_le_bytes());
            out.extend(schema);
        }
        out.extend(self.high_water.to_le_bytes());
        out.push(u8::from(self.root.is_some()));
        if let Some(page) = &self.root {
            out.extend(page.hash);
        }
        self.revision = Sha256::digest(out).into();
    }
    pub fn revision(&self) -> Hash {
        self.revision
    }
    pub fn len(&self) -> usize {
        self.root.as_ref().map_or(0, |p| p.count)
    }
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }
    pub fn canonical_bytes(&self) -> usize {
        self.root.as_ref().map_or(0, |p| p.bytes)
    }
    pub fn first_key(&self) -> Option<u64> {
        self.root.as_ref().map(|p| p.first)
    }
    pub fn last_key(&self) -> Option<u64> {
        self.root.as_ref().map(|p| p.last)
    }

    pub fn append(&self, key: u64, record: Snapshot) -> Result<Self> {
        if key <= self.high_water {
            return Err(Error::Conflict);
        }
        if record.canonical_bytes() > MAX_RECORD_BYTES || self.len() >= self.budget.records {
            return Err(Error::Limit);
        }
        if record.register().is_some() || record.child_keys().count() != self.schema.fields.len() {
            return Err(Error::Schema);
        }
        for (field, schema) in &self.schema.fields {
            let node = record.subtree(&[*field]).map_err(|_| Error::Schema)?;
            if node.child_keys().next().is_some()
                || node.register().is_none_or(|r| r.schema() != *schema)
            {
                return Err(Error::Schema);
            }
        }
        let record = Record { key, data: record };
        let root = if let Some(root) = &self.root {
            let (left, right) = root.append(record);
            match right {
                Some(right) => Page::new(Items::Branch(vec![left, right])),
                None => left,
            }
        } else {
            Page::new(Items::Leaf(vec![record]))
        };
        if root.bytes > self.budget.canonical_bytes {
            return Err(Error::Limit);
        }
        let mut next = self.clone();
        next.root = Some(root);
        next.high_water = key;
        next.rehash();
        Ok(next)
    }
    pub fn retain_from(&self, key: u64) -> Self {
        if self.first_key().is_none_or(|first| first >= key) {
            return self.clone();
        }
        let mut next = self.clone();
        next.root = self.root.as_ref().and_then(|p| p.prune(key));
        // Collapse single-child roots, while retaining all other page levels.
        while let Some(root) = &next.root {
            if let Items::Branch(pages) = &root.items
                && pages.len() == 1
            {
                next.root = Some(pages[0].clone());
            } else {
                break;
            }
        }
        next.rehash();
        next
    }
    pub(super) fn rank(&self, key: u64, work: &mut IndexWork) -> Result<usize> {
        self.root
            .as_ref()
            .and_then(|p| p.rank(key, work))
            .ok_or(Error::AnchorExpired)
    }
    pub(super) fn range(&self, start: usize, end: usize, work: &mut IndexWork) -> Vec<Record> {
        let mut records = Vec::with_capacity(end - start);
        if start < end
            && let Some(root) = &self.root
        {
            root.range(start, end, &mut records, work);
        }
        records
    }
}
