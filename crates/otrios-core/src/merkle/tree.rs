// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

/// Deterministic work counters, excluding comparisons, transport and durable I/O.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Work {
    pub hashed_nodes: usize,
    pub hashed_bytes: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub hash: Hash,
    pub register: Option<Register>,
    pub children: BTreeMap<u64, Arc<Node>>,
    pub height: usize,
    pub count: usize,
    pub bytes: usize,
}
impl Node {
    pub fn build(
        register: Option<Register>,
        children: BTreeMap<u64, Arc<Node>>,
        work: &mut Work,
    ) -> Result<Arc<Self>> {
        if children.len() > MAX_CHILDREN {
            return Err(Error::Limit);
        }
        if let Some(r) = &register {
            r.validate()?;
        }
        let node = Self {
            hash: [0; 32],
            register,
            children,
            height: 0,
            count: 1,
            bytes: 0,
        };
        let encoded = wire::encode_node(&node);
        if encoded.len() > MAX_NODE_BYTES {
            return Err(Error::Limit);
        }
        let hash = Sha256::digest(&encoded).into();
        work.hashed_nodes += 1;
        work.hashed_bytes += encoded.len();
        Self::verified(node.register, node.children, hash, encoded.len())
    }
    // Canonical parser already validated and hashed these bytes. Do not hash a
    // second time on receipt; still bound the complete materialized DAG by edges.
    pub(crate) fn verified(
        register: Option<Register>,
        children: BTreeMap<u64, Arc<Node>>,
        hash: Hash,
        size: usize,
    ) -> Result<Arc<Self>> {
        let mut node = Self {
            hash,
            register,
            children,
            height: 0,
            count: 1,
            bytes: size,
        };
        for child in node.children.values() {
            node.height = node.height.max(child.height + 1);
            node.count = node.count.checked_add(child.count).ok_or(Error::Limit)?;
            node.bytes = node.bytes.checked_add(child.bytes).ok_or(Error::Limit)?;
        }
        if node.height > MAX_DEPTH || node.count > MAX_NODES || node.bytes > MAX_TREE_BYTES {
            return Err(Error::Limit);
        }
        Ok(Arc::new(node))
    }
}

/// Pinned immutable root. Cloning pins, not copies, all unchanged descendants.
#[derive(Clone, Debug)]
pub struct Snapshot(pub(crate) Arc<Node>);
#[derive(Clone, Debug)]
pub struct Edit {
    pub path: Path,
    pub schema: Hash,
    pub value: Option<Vec<u8>>,
}
impl Snapshot {
    pub fn empty() -> Self {
        Self(Node::build(None, BTreeMap::new(), &mut Work::default()).expect("bounded empty node"))
    }
    /// Canonical full snapshot for a trusted store, separate from a live lease.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        wire::encode_transfer(
            Cursor {
                epoch: 0,
                lease: 0,
                sequence: 1,
            },
            &[],
            true,
            None,
            Some(self),
        )
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let transfer = wire::decode_transfer(bytes)?;
        if transfer.cursor
            != (Cursor {
                epoch: 0,
                lease: 0,
                sequence: 1,
            })
            || !transfer.path.is_empty()
            || !transfer.full
        {
            return Err(Error::Invalid);
        }
        transfer.materialize(None)?.ok_or(Error::Missing)
    }
    /// Root-chained store delta; fixed cursor distinguishes it from live leases.
    pub fn delta_bytes(&self, base: &Self) -> Result<Vec<u8>> {
        wire::encode_transfer(
            Cursor {
                epoch: 0,
                lease: 0,
                sequence: 1,
            },
            &[],
            false,
            Some(base),
            Some(self),
        )
    }
    pub fn delta_roots(data: &[u8]) -> Result<(Hash, Hash)> {
        let t = wire::decode_transfer(data)?;
        if t.full
            || !t.path.is_empty()
            || t.cursor
                != (Cursor {
                    epoch: 0,
                    lease: 0,
                    sequence: 1,
                })
        {
            return Err(Error::Invalid);
        }
        Ok((t.base.ok_or(Error::Missing)?, t.root.ok_or(Error::Missing)?))
    }
    pub fn apply_delta(&self, data: &[u8]) -> Result<Self> {
        let (base, _) = Self::delta_roots(data)?;
        if base != self.hash() {
            return Err(Error::Conflict);
        }
        wire::decode_transfer(data)?
            .materialize(Some(self))?
            .ok_or(Error::Missing)
    }
    pub fn hash(&self) -> Hash {
        self.0.hash
    }
    pub fn node_count(&self) -> usize {
        self.0.count
    }
    pub fn canonical_bytes(&self) -> usize {
        self.0.bytes
    }
    pub fn subtree(&self, path: &[u64]) -> Result<Self> {
        path_check(path)?;
        let mut node = &self.0;
        for key in path {
            node = node.children.get(key).ok_or(Error::Missing)?;
        }
        Ok(Self(node.clone()))
    }
    pub fn register(&self) -> Option<&Register> {
        self.0.register.as_ref()
    }
    pub fn child_keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.0.children.keys().copied()
    }
    /// Atomic validated batch: sort stable paths, then rebuild each shared
    /// ancestor once. The old root remains usable even if any edit is rejected.
    pub fn edit(&self, actor: Actor, edits: &[Edit]) -> Result<(Self, Work)> {
        if edits.len() > MAX_EDITS {
            return Err(Error::Limit);
        }
        let mut ordered = edits.iter().collect::<Vec<_>>();
        ordered.sort_by(|a, b| a.path.cmp(&b.path));
        for (i, edit) in ordered.iter().enumerate() {
            path_check(&edit.path)?;
            if edit
                .value
                .as_ref()
                .is_some_and(|v| v.len() > MAX_VALUE_BYTES)
            {
                return Err(Error::Limit);
            }
            if i > 0 && ordered[i - 1].path == edit.path {
                return Err(Error::Invalid);
            }
        }
        let mut work = Work::default();
        let root = edit_node(Some(&self.0), actor, &ordered, 0, &mut work)?;
        Ok((Self(root), work))
    }
    /// Join CRDT state, skipping identical subtrees by digest. A schema conflict,
    /// actor collision or exhausted bound rejects the whole join.
    pub fn merge(&self, other: &Self) -> Result<(Self, Work)> {
        let mut work = Work::default();
        let root = merge_node(&self.0, &other.0, &mut work)?;
        Ok((Self(root), work))
    }
    /// Join a received/locally edited subtree back at its stable logical path.
    /// The host must validate write policy/schema before this call. Other scopes
    /// remain untouched; this method never interprets a digest as authorization.
    pub fn merge_subtree(&self, path: &[u64], other: &Self) -> Result<(Self, Work)> {
        path_check(path)?;
        if path.len() + other.0.height > MAX_DEPTH {
            return Err(Error::Limit);
        }
        let mut work = Work::default();
        let root = merge_at(Some(&self.0), path, &other.0, &mut work)?;
        Ok((Self(root), work))
    }
    /// Useful to assert actual structural sharing without exposing mutable nodes.
    pub fn shares_root(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Default for Snapshot {
    fn default() -> Self {
        Self::empty()
    }
}
fn edit_node(
    old: Option<&Arc<Node>>,
    actor: Actor,
    edits: &[&Edit],
    depth: usize,
    work: &mut Work,
) -> Result<Arc<Node>> {
    if edits.is_empty() {
        return Ok(old.expect("root exists").clone());
    }
    let mut register = old.and_then(|n| n.register.clone());
    let mut children = old.map(|n| n.children.clone()).unwrap_or_default();
    let mut index = 0;
    if edits[0].path.len() == depth {
        let edit = edits[0];
        let current = register.unwrap_or_else(|| Register::empty(edit.schema));
        if current.schema() != edit.schema {
            return Err(Error::Schema);
        }
        register = Some(current.assign(actor, edit.value.as_deref())?);
        index += 1;
    }
    while index < edits.len() {
        let key = edits[index].path[depth];
        let end = index + edits[index..].partition_point(|e| e.path.get(depth) == Some(&key));
        let child = edit_node(
            children.get(&key),
            actor,
            &edits[index..end],
            depth + 1,
            work,
        )?;
        children.insert(key, child);
        if children.len() > MAX_CHILDREN {
            return Err(Error::Limit);
        }
        index = end;
    }
    Node::build(register, children, work)
}
fn merge_node(a: &Arc<Node>, b: &Arc<Node>, work: &mut Work) -> Result<Arc<Node>> {
    if a.hash == b.hash {
        return Ok(a.clone());
    }
    let register = match (&a.register, &b.register) {
        (Some(a), Some(b)) => Some(a.merge(b)?),
        (a, b) => a.clone().or_else(|| b.clone()),
    };
    let mut children = a.children.clone();
    for (&key, child) in &b.children {
        let merged = match children.get(&key) {
            Some(here) => merge_node(here, child, work)?,
            None => child.clone(),
        };
        children.insert(key, merged);
    }
    // Absorbing an older state must retain both identity and allocations.
    if register == a.register
        && children.len() == a.children.len()
        && children
            .iter()
            .all(|(k, v)| a.children.get(k).is_some_and(|c| c.hash == v.hash))
    {
        return Ok(a.clone());
    }
    Node::build(register, children, work)
}

fn merge_at(
    old: Option<&Arc<Node>>,
    path: &[u64],
    other: &Arc<Node>,
    work: &mut Work,
) -> Result<Arc<Node>> {
    let Some((&key, rest)) = path.split_first() else {
        return old.map_or_else(|| Ok(other.clone()), |old| merge_node(old, other, work));
    };
    let child = merge_at(old.and_then(|n| n.children.get(&key)), rest, other, work)?;
    if let Some(old) = old.filter(|n| n.children.get(&key).is_some_and(|c| c.hash == child.hash)) {
        return Ok(old.clone());
    }
    let mut children = old.map(|n| n.children.clone()).unwrap_or_default();
    children.insert(key, child);
    Node::build(old.and_then(|n| n.register.clone()), children, work)
}
