// SPDX-License-Identifier: LGPL-3.0-only
use super::{tree::Node, *};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
const NODE_DOMAIN: &[u8] = b"OTriOS\0merkle-node\0v1\0";
const TRANSFER_DOMAIN: &[u8] = b"OTriOS\0merkle-transfer\0v1\0";
const MAX_TRANSFER_BYTES: usize = MAX_TREE_BYTES + MAX_NODES * 4 + 1024;

pub(crate) fn encode_node(node: &Node) -> Vec<u8> {
    let mut out = NODE_DOMAIN.to_vec();
    out.push(u8::from(node.register.is_some()));
    if let Some(reg) = &node.register {
        out.extend(reg.schema);
        out.push(reg.context.len() as u8);
        for (actor, counter) in &reg.context {
            out.extend(actor);
            out.extend(counter.to_le_bytes());
        }
        out.push(reg.entries.len() as u8);
        for ((actor, counter), value) in &reg.entries {
            out.extend(actor);
            out.extend(counter.to_le_bytes());
            out.push(u8::from(value.is_some()));
            if let Some(value) = value {
                out.extend((value.len() as u32).to_le_bytes());
                out.extend(value);
            }
        }
    }
    out.push(node.children.len() as u8);
    for (key, child) in &node.children {
        out.extend(key.to_le_bytes());
        out.extend(child.hash);
    }
    out
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.0.len() {
            return Err(Error::Invalid);
        }
        let (bytes, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().map_err(|_| Error::Invalid)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    fn flag(&mut self) -> Result<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Invalid),
        }
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<usize> {
        Ok(u32::from_le_bytes(self.array()?) as usize)
    }
    fn digest(&mut self) -> Result<Option<Hash>> {
        if self.flag()? {
            Ok(Some(self.array()?))
        } else {
            Ok(None)
        }
    }
    fn domain(&mut self, domain: &[u8]) -> Result<()> {
        if self.take(domain.len())? == domain {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
    fn finish(self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
}
fn digest(out: &mut Vec<u8>, hash: Option<Hash>) {
    out.push(u8::from(hash.is_some()));
    if let Some(hash) = hash {
        out.extend(hash);
    }
}
struct RawNode {
    register: Option<Register>,
    children: BTreeMap<u64, Hash>,
    size: usize,
}
fn decode_node(bytes: &[u8]) -> Result<(Hash, RawNode)> {
    if bytes.len() > MAX_NODE_BYTES {
        return Err(Error::Limit);
    }
    let mut r = Reader(bytes);
    r.domain(NODE_DOMAIN)?;
    let register = if r.flag()? {
        let schema = r.array()?;
        let count = usize::from(r.byte()?);
        if count == 0 || count > MAX_ACTORS {
            return Err(Error::Limit);
        }
        let mut context = BTreeMap::new();
        let mut previous = None;
        for _ in 0..count {
            let actor = r.array()?;
            let counter = r.u64()?;
            if previous.is_some_and(|a| a >= actor) {
                return Err(Error::Invalid);
            }
            previous = Some(actor);
            context.insert(actor, counter);
        }
        let count = usize::from(r.byte()?);
        if count == 0 || count > MAX_ACTORS {
            return Err(Error::Limit);
        }
        let mut entries = BTreeMap::new();
        let mut previous = None;
        for _ in 0..count {
            let actor = r.array()?;
            let counter = r.u64()?;
            if previous.is_some_and(|a| a >= actor) {
                return Err(Error::Invalid);
            }
            previous = Some(actor);
            let value = if r.flag()? {
                let size = r.u32()?;
                if size > MAX_VALUE_BYTES {
                    return Err(Error::Limit);
                }
                Some(r.take(size)?.to_vec())
            } else {
                None
            };
            entries.insert((actor, counter), value);
        }
        let register = Register {
            schema,
            context,
            entries,
        };
        register.validate()?;
        Some(register)
    } else {
        None
    };
    let count = usize::from(r.byte()?);
    if count > MAX_CHILDREN {
        return Err(Error::Limit);
    }
    let mut children = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let key = r.u64()?;
        let hash = r.array()?;
        if previous.is_some_and(|k| k >= key) {
            return Err(Error::Invalid);
        }
        previous = Some(key);
        children.insert(key, hash);
    }
    r.finish()?;
    Ok((
        Sha256::digest(bytes).into(),
        RawNode {
            register,
            children,
            size: bytes.len(),
        },
    ))
}

pub(crate) struct Transfer {
    pub cursor: Cursor,
    pub path: Path,
    pub full: bool,
    pub base: Option<Hash>,
    pub root: Option<Hash>,
    nodes: BTreeMap<Hash, RawNode>,
}
pub(crate) fn encode_transfer(
    cursor: Cursor,
    path: &[u64],
    full: bool,
    old: Option<&Snapshot>,
    new: Option<&Snapshot>,
) -> Result<Vec<u8>> {
    fn collect(old: Option<&Arc<Node>>, new: &Arc<Node>, nodes: &mut BTreeMap<Hash, Vec<u8>>) {
        if old.is_some_and(|n| n.hash == new.hash) {
            return;
        }
        for (key, child) in &new.children {
            collect(old.and_then(|n| n.children.get(key)), child, nodes);
        }
        nodes.entry(new.hash).or_insert_with(|| encode_node(new));
    }
    let mut nodes = BTreeMap::new();
    if let Some(new) = new {
        collect(old.map(|n| &n.0), &new.0, &mut nodes);
    }
    let mut out = TRANSFER_DOMAIN.to_vec();
    out.extend(cursor.epoch.to_le_bytes());
    out.extend(cursor.lease.to_le_bytes());
    out.extend(cursor.sequence.to_le_bytes());
    out.push(path.len() as u8);
    for key in path {
        out.extend(key.to_le_bytes());
    }
    out.push(u8::from(full));
    digest(&mut out, old.map(Snapshot::hash));
    digest(&mut out, new.map(Snapshot::hash));
    out.extend((nodes.len() as u32).to_le_bytes());
    for bytes in nodes.values() {
        out.extend((bytes.len() as u32).to_le_bytes());
        out.extend(bytes);
    }
    if out.len() > MAX_TRANSFER_BYTES {
        return Err(Error::Limit);
    }
    Ok(out)
}
pub(crate) fn decode_transfer(bytes: &[u8]) -> Result<Transfer> {
    if bytes.len() > MAX_TRANSFER_BYTES {
        return Err(Error::Limit);
    }
    let mut r = Reader(bytes);
    r.domain(TRANSFER_DOMAIN)?;
    let cursor = Cursor {
        epoch: r.u64()?,
        lease: r.u64()?,
        sequence: r.u64()?,
    };
    if cursor.sequence == 0 {
        return Err(Error::Invalid);
    }
    let count = usize::from(r.byte()?);
    if count > MAX_DEPTH {
        return Err(Error::Limit);
    }
    let mut path = Vec::with_capacity(count);
    for _ in 0..count {
        path.push(r.u64()?);
    }
    let full = r.flag()?;
    let base = r.digest()?;
    let root = r.digest()?;
    if full && base.is_some() {
        return Err(Error::Invalid);
    }
    let count = r.u32()?;
    if count > MAX_NODES {
        return Err(Error::Limit);
    }
    let mut nodes = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let size = r.u32()?;
        let bytes = r.take(size)?;
        let (hash, node) = decode_node(bytes)?;
        if previous.is_some_and(|h| h >= hash) {
            return Err(Error::Invalid);
        }
        previous = Some(hash);
        nodes.insert(hash, node);
    }
    r.finish()?;
    if root.is_none() && !nodes.is_empty() {
        return Err(Error::Invalid);
    }
    Ok(Transfer {
        cursor,
        path,
        full,
        base,
        root,
        nodes,
    })
}
impl Transfer {
    /// Fingerprint canonical envelope fields and already-verified record hashes.
    /// Exact retransmits are idempotent; a changed record set at the same cursor
    /// is rejected without retaining a second full transport frame in client RAM.
    pub fn fingerprint(&self) -> Hash {
        let mut hash = Sha256::new();
        hash.update(TRANSFER_DOMAIN);
        for value in [self.cursor.epoch, self.cursor.lease, self.cursor.sequence] {
            hash.update(value.to_le_bytes());
        }
        hash.update([self.path.len() as u8]);
        for key in &self.path {
            hash.update(key.to_le_bytes());
        }
        hash.update([u8::from(self.full)]);
        for digest in [self.base, self.root] {
            hash.update([u8::from(digest.is_some())]);
            if let Some(digest) = digest {
                hash.update(digest);
            }
        }
        hash.update((self.nodes.len() as u32).to_le_bytes());
        for digest in self.nodes.keys() {
            hash.update(digest);
        }
        hash.finalize().into()
    }
    pub fn materialize(&self, old: Option<&Snapshot>) -> Result<Option<Snapshot>> {
        fn build(
            hash: Hash,
            old: Option<&Arc<Node>>,
            nodes: &BTreeMap<Hash, RawNode>,
            used: &mut BTreeSet<Hash>,
            depth: usize,
            remaining: &mut (usize, usize),
        ) -> Result<Arc<Node>> {
            if depth > MAX_DEPTH {
                return Err(Error::Limit);
            }
            if let Some(old) = old.filter(|n| n.hash == hash) {
                if depth + old.height > MAX_DEPTH {
                    return Err(Error::Limit);
                }
                remaining.0 = remaining.0.checked_sub(old.count).ok_or(Error::Limit)?;
                remaining.1 = remaining.1.checked_sub(old.bytes).ok_or(Error::Limit)?;
                return Ok(old.clone());
            }
            let raw = nodes.get(&hash).ok_or(Error::ResyncRequired)?;
            remaining.0 = remaining.0.checked_sub(1).ok_or(Error::Limit)?;
            remaining.1 = remaining.1.checked_sub(raw.size).ok_or(Error::Limit)?;
            used.insert(hash);
            let mut children = BTreeMap::new();
            for (&key, &child) in &raw.children {
                children.insert(
                    key,
                    build(
                        child,
                        old.and_then(|n| n.children.get(&key)),
                        nodes,
                        used,
                        depth + 1,
                        remaining,
                    )?,
                );
            }
            Node::verified(raw.register.clone(), children, hash, raw.size)
        }
        let mut used = BTreeSet::new();
        let mut remaining = (MAX_NODES, MAX_TREE_BYTES);
        let root = self
            .root
            .map(|hash| {
                build(
                    hash,
                    old.map(|n| &n.0),
                    &self.nodes,
                    &mut used,
                    0,
                    &mut remaining,
                )
            })
            .transpose()?
            .map(Snapshot);
        if used.len() != self.nodes.len() {
            return Err(Error::Invalid);
        }
        Ok(root)
    }
    pub fn record_count(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cursor() -> Cursor {
        Cursor {
            epoch: 1,
            lease: 1,
            sequence: 1,
        }
    }
    #[test]
    fn canonical_empty_node_has_a_fixed_sha256_vector() {
        let empty = Snapshot::empty();
        assert_eq!(encode_node(&empty.0), b"OTriOS\0merkle-node\0v1\0\0\0");
        // Independently generated with Python hashlib.sha256 over the bytes above.
        assert_eq!(
            empty.hash(),
            [
                197, 19, 71, 105, 211, 212, 56, 60, 76, 209, 140, 193, 62, 192, 142, 30, 63, 18,
                24, 97, 161, 56, 102, 250, 60, 229, 86, 118, 244, 201, 105, 239
            ]
        );
    }
    #[test]
    fn unused_duplicate_and_noncanonical_records_are_rejected() {
        let tree = Snapshot::empty();
        let bytes = encode_transfer(cursor(), &[], true, None, Some(&tree)).unwrap();
        let mut parsed = decode_transfer(&bytes).unwrap();
        let other = Snapshot::empty()
            .edit(
                [1; 16],
                &[Edit {
                    path: vec![],
                    schema: [7; 32],
                    value: None,
                }],
            )
            .unwrap()
            .0;
        let (hash, raw) = decode_node(&encode_node(&other.0)).unwrap();
        parsed.nodes.insert(hash, raw);
        let mut replica = Replica::for_lease(1, 1, vec![]).unwrap();
        replica.apply(&bytes).unwrap();
        let offset = TRANSFER_DOMAIN.len() + 24 + 1 + 1 + 1 + 33;
        let mut extra = bytes[..offset].to_vec();
        extra.extend(2_u32.to_le_bytes());
        let records = BTreeMap::from([
            (tree.hash(), encode_node(&tree.0)),
            (other.hash(), encode_node(&other.0)),
        ]);
        for record in records.values() {
            extra.extend((record.len() as u32).to_le_bytes());
            extra.extend(record);
        }
        assert_eq!(replica.apply(&extra), Err(Error::Invalid));

        assert!(matches!(parsed.materialize(None), Err(Error::Invalid)));
        let mut trailing = encode_node(&tree.0);
        trailing.push(0);
        assert!(decode_node(&trailing).is_err());
        // Raw record order and keys are canonical, not map insertion order.
        let mut malformed = NODE_DOMAIN.to_vec();
        malformed.push(0);
        malformed.push(2);
        for key in [2_u64, 1] {
            malformed.extend(key.to_le_bytes());
            malformed.extend(tree.hash());
        }
        assert!(decode_node(&malformed).is_err());
        let mut nonboolean = encode_node(&tree.0);
        nonboolean[NODE_DOMAIN.len()] = 2;
        assert!(decode_node(&nonboolean).is_err());
        let mut duplicate = bytes.clone();
        let count_offset = TRANSFER_DOMAIN.len() + 24 + 1 + 1 + 1 + 33;
        duplicate[count_offset..count_offset + 4].copy_from_slice(&2_u32.to_le_bytes());
        duplicate.extend(&bytes[count_offset + 4..]);
        assert!(decode_transfer(&duplicate).is_err());
    }
    #[test]
    fn unknown_children_and_adversarial_dag_expansion_are_bounded() {
        let mut transfer = Transfer {
            cursor: cursor(),
            path: vec![],
            full: true,
            base: None,
            root: Some([9; 32]),
            nodes: BTreeMap::new(),
        };
        transfer.nodes.insert(
            [9; 32],
            RawNode {
                register: None,
                children: BTreeMap::from([(1, [8; 32])]),
                size: 64,
            },
        );
        assert!(matches!(
            transfer.materialize(None),
            Err(Error::ResyncRequired)
        ));
        // Eleven tiny wire records can denote 32^10 expanded references. Refuse
        // while descending, BEFORE constructing an exponential number of nodes.
        transfer.nodes.clear();
        for level in 0..=10_u8 {
            let children = if level == 10 {
                BTreeMap::new()
            } else {
                (0..32).map(|k| (k, [level + 1; 32])).collect()
            };
            transfer.nodes.insert(
                [level; 32],
                RawNode {
                    register: None,
                    children,
                    size: 64,
                },
            );
        }
        transfer.root = Some([0; 32]);
        assert!(matches!(transfer.materialize(None), Err(Error::Limit)));
        // Depth is bounded even for a deliberately cyclic synthetic parsed graph.
        transfer.nodes.clear();
        transfer.nodes.insert(
            [0; 32],
            RawNode {
                register: None,
                children: BTreeMap::from([(1, [0; 32])]),
                size: 64,
            },
        );
        assert!(matches!(transfer.materialize(None), Err(Error::Limit)));
    }
}
