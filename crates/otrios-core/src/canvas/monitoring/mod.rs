// SPDX-License-Identifier: LGPL-3.0-only
//! Monitoring-owned single-producer data, built on the shared canonical Merkle tree.
use super::{Result, err, hash};
use crate::merkle::{Edit, Hash, Snapshot as Tree};
pub use otrios_interfaces::monitoring::*;
use std::sync::Arc;
mod project;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;
pub use project::{build as project, empty as empty_projection};

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    flexbuffers::to_vec(value).map_err(err)
}
fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(bytes: &[u8]) -> Result<T> {
    crate::flex_guard::validate(bytes, 12, MAX_PACKET_BYTES, MAX_PACKET_BYTES)?;
    let value = flexbuffers::from_slice(bytes).map_err(err)?;
    if encode(&value)? != bytes {
        return Err("Noncanonical Monitoring payload".into());
    }
    Ok(value)
}
pub fn encode_command(value: &Command) -> Result<Vec<u8>> {
    encode(value)
}
pub fn decode_command(bytes: &[u8]) -> Result<Command> {
    if bytes.len() > MAX_PACKET_BYTES {
        return Err("Monitoring publication bound".into());
    }
    decode(bytes)
}
pub fn encode_event(value: &Event) -> Result<Vec<u8>> {
    encode(value)
}
pub fn decode_event(bytes: &[u8]) -> Result<Event> {
    if bytes.len() > 1024 {
        return Err("Monitoring reply bound".into());
    }
    decode(bytes)
}
pub fn validate_data(data: &Data) -> Result<()> {
    let w = &data.window;
    if w.start < 0
        || w.end.checked_sub(w.start) != Some(7 * 86400)
        || w.step != 3600
        || w.end % 3600 != 0
        || w.generated < w.end
        || w.generated - w.end >= 3600
        || [&w.collected_label, &w.start_label, &w.end_label]
            .iter()
            .any(|v| v.is_empty() || v.len() > 32 || !v.is_ascii() || v.bytes().any(|b| b < 32))
        || data.metrics.len() != METRICS.len()
    {
        return Err("Invalid Monitoring window".into());
    }
    for (i, m) in data.metrics.iter().enumerate() {
        let valid =
            |n: &f64| n.is_finite() && *n >= 0. && *n <= if i < 4 || i == 10 { 1. } else { 1e18 };
        if m.history.len() != 169
            || m.current.as_ref().is_some_and(|n| !valid(n))
            || m.history.iter().flatten().any(|n| !valid(n))
        {
            return Err("Invalid Monitoring samples".into());
        }
    }
    Ok(())
}
fn schema(key: &str) -> Hash {
    hash(format!("OTriOS\0monitoring\0v1\0{key}").as_bytes())
        .try_into()
        .expect("SHA-256")
}
fn actor(producer: &str) -> [u8; 16] {
    schema(producer)[..16].try_into().expect("actor")
}
fn payload<T: serde::de::DeserializeOwned + serde::Serialize>(
    tree: &Tree,
    key: &[u64],
    expected: Hash,
    writer: [u8; 16],
    max_revision: u64,
) -> Result<T> {
    let node = tree.subtree(key).map_err(err)?;
    let r = node.register().ok_or("Missing Monitoring value")?;
    let mut values = r.values();
    let (a, n, value) = values.next().ok_or("Empty Monitoring value")?;
    if r.schema() != expected
        || a != writer
        || n == 0
        || n > max_revision
        || values.next().is_some()
    {
        return Err("Wrong Monitoring writer/schema".into());
    }
    decode(value.ok_or("Deleted Monitoring value")?)
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    tree: Tree,
    head: Arc<Head>,
    data: Arc<Data>,
}
impl Snapshot {
    pub fn root(&self) -> Hash {
        self.tree.hash()
    }
    pub fn head(&self) -> &Head {
        &self.head
    }
    pub fn data(&self) -> &Data {
        &self.data
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        let bytes = self.tree.to_bytes().map_err(err)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("Monitoring snapshot bound".into());
        }
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("Monitoring snapshot bound".into());
        }
        let tree = Tree::from_bytes(bytes).map_err(err)?;
        if tree.to_bytes().map_err(err)? != bytes {
            return Err("Noncanonical Monitoring snapshot".into());
        }
        Self::from_tree(tree)
    }
    fn from_tree(tree: Tree) -> Result<Self> {
        if tree.node_count() != 12
            || tree.child_keys().collect::<Vec<_>>() != (1..=11).collect::<Vec<_>>()
        {
            return Err("Monitoring tree shape".into());
        }
        // Decode only bounded metadata before deriving the expected writer.
        let raw = tree
            .register()
            .and_then(|r| r.values().next())
            .and_then(|(_, _, v)| v)
            .ok_or("Missing Monitoring head")?;
        let head: Head = decode(raw)?;
        if head.format != FORMAT
            || !crate::spaces::valid_id(&head.feed)
            || !crate::spaces::valid_identity(&head.producer)
            || head.revision == 0
        {
            return Err("Invalid Monitoring head".into());
        }
        let writer = actor(&head.producer);
        let checked: Head = payload(&tree, &[], schema("head"), writer, head.revision)?;
        if tree
            .register()
            .and_then(|r| r.values().next())
            .map(|(_, n, _)| n)
            != Some(head.revision)
            || checked != head
        {
            return Err("Monitoring revision mismatch".into());
        }
        let mut metrics = Vec::new();
        for (i, key) in METRICS.iter().enumerate() {
            let node = tree.subtree(&[i as u64 + 1]).map_err(err)?;
            if node.child_keys().next().is_some() {
                return Err("Unexpected Monitoring children".into());
            }
            metrics.push(payload(
                &tree,
                &[i as u64 + 1],
                schema(key),
                writer,
                head.revision,
            )?);
        }
        let data = Data {
            window: head.window.clone(),
            metrics,
        };
        validate_data(&data)?;
        // Reject complete data that exceeds the shared chart projection's
        // existing scene/instance limits before either owner publishes it.
        project(&data)?;
        Ok(Self {
            tree,
            head: Arc::new(head),
            data: Arc::new(data),
        })
    }
    pub fn collect(
        previous: Option<&Self>,
        feed: &str,
        producer: &str,
        data: Data,
    ) -> Result<Self> {
        validate_data(&data)?;
        if let Some(old) = previous {
            if old.head.feed != feed
                || old.head.producer != producer
                || data.window.generated < old.head.window.generated
            {
                return Err("Monitoring source/clock changed".into());
            }
            if old.data() == &data {
                return Ok(old.clone());
            }
        }
        let revision = previous.map_or(Ok(1), |p| {
            p.head
                .revision
                .checked_add(1)
                .ok_or("Monitoring revision exhausted")
        })?;
        let head = Head {
            format: FORMAT.into(),
            feed: feed.into(),
            producer: producer.into(),
            revision,
            window: data.window.clone(),
        };
        let mut edits = vec![Edit {
            path: vec![],
            schema: schema("head"),
            value: Some(encode(&head)?),
        }];
        for (i, key) in METRICS.iter().enumerate() {
            if previous.is_some_and(|p| p.data.metrics[i] == data.metrics[i]) {
                continue;
            }
            edits.push(Edit {
                path: vec![i as u64 + 1],
                schema: schema(key),
                value: Some(encode(&data.metrics[i])?),
            });
        }
        let empty = Tree::empty();
        let tree = previous
            .map_or(&empty, |p| &p.tree)
            .edit(actor(producer), &edits)
            .map_err(err)?
            .0;
        let result = Self::from_tree(tree)?;
        result.bytes()?;
        Ok(result)
    }
    pub fn transfer(&self, base: Option<&Self>) -> Result<Vec<u8>> {
        match base {
            Some(base) => self.tree.delta_bytes(&base.tree).map_err(err),
            None => self.bytes(),
        }
    }
    pub fn receive(
        current: Option<&Self>,
        base: Option<&[u8]>,
        bytes: &[u8],
        root: &[u8],
        feed: &str,
        producer: &str,
    ) -> Result<Self> {
        if bytes.len() > MAX_SNAPSHOT_BYTES || root.len() != 32 {
            return Err("Monitoring transfer bound".into());
        }
        let next = if let Some(base) = base {
            let old = current
                .filter(|p| p.root().as_slice() == base)
                .ok_or("Monitoring base changed")?;
            Self::from_tree(old.tree.apply_delta(bytes).map_err(err)?)?
        } else {
            Self::from_bytes(bytes)?
        };
        if next.head.feed != feed
            || next.head.producer != producer
            || next.root().as_slice() != root
        {
            return Err("Wrong Monitoring feed/producer/root".into());
        }
        if let Some(old) = current {
            if next.head.revision < old.head.revision
                || next.head.revision == old.head.revision && next.root() != old.root()
                || next.head.window.generated < old.head.window.generated
            {
                return Err("Stale/equivocal Monitoring revision".into());
            }
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Data {
        Data {
            window: Window {
                generated: 604800,
                start: 0,
                end: 604800,
                step: 3600,
                collected_label: "Jan 08 00:00 UTC".into(),
                start_label: "Jan 01 00:00 UTC".into(),
                end_label: "Jan 08 00:00 UTC".into(),
            },
            metrics: METRICS
                .iter()
                .map(|_| Metric {
                    current: None,
                    history: vec![None; 169],
                })
                .collect(),
        }
    }
    #[test]
    fn monitoring_revisions_preserve_roots_gaps_and_reject_replays() {
        let feed = "11111111111111111111111111111111";
        let mut d = data();
        d.metrics[0].history[0] = Some(1.);
        d.metrics[0].history[3] = Some(0.);
        let first = Snapshot::collect(None, feed, "collector", d.clone()).unwrap();
        assert_eq!(
            Snapshot::from_bytes(&first.bytes().unwrap())
                .unwrap()
                .root(),
            first.root()
        );
        d.window.generated += 1;
        d.metrics[4].current = Some(2302.);
        let next = Snapshot::collect(Some(&first), feed, "collector", d.clone()).unwrap();
        let delta = next.transfer(Some(&first)).unwrap();
        assert!(delta.len() < next.bytes().unwrap().len());
        let received = Snapshot::receive(
            Some(&first),
            Some(&first.root()),
            &delta,
            &next.root(),
            feed,
            "collector",
        )
        .unwrap();
        assert_eq!(received.root(), next.root());
        assert_eq!(received.data().metrics[0].history[1], None);
        crate::dashboard::layout::Layout::from_bytes(&project(received.data()).unwrap()).unwrap();
        assert!(
            Snapshot::receive(
                Some(&received),
                None,
                &first.bytes().unwrap(),
                &first.root(),
                feed,
                "collector"
            )
            .is_err()
        );
        let mut corrupt = next.bytes().unwrap();
        let middle = corrupt.len() / 2;
        corrupt[middle] ^= 1;
        assert!(Snapshot::from_bytes(&corrupt).is_err());
        assert!(
            Snapshot::receive(
                None,
                None,
                &next.bytes().unwrap(),
                &next.root(),
                feed,
                "intruder"
            )
            .is_err()
        );
        d.window.generated = 604799;
        assert!(Snapshot::collect(Some(&next), feed, "collector", d).is_err());
        assert_eq!(
            Snapshot::receive(
                Some(&next),
                None,
                &next.bytes().unwrap(),
                &next.root(),
                feed,
                "collector"
            )
            .unwrap()
            .root(),
            next.root()
        );
        let mut conflict = data();
        conflict.metrics[4].current = Some(9.);
        let conflict = Snapshot::collect(None, feed, "collector", conflict).unwrap();
        assert!(
            Snapshot::receive(
                Some(&first),
                None,
                &conflict.bytes().unwrap(),
                &conflict.root(),
                feed,
                "collector"
            )
            .is_err()
        );
    }
}
