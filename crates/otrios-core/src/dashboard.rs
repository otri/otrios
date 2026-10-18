// SPDX-License-Identifier: LGPL-3.0-only
//! Dashboard experiment: CRDT edits -> complete immutable view -> shared compositor.
//! This is a bounded, trusted-peer demo; it does not implement Linux shared memory.

mod render;
mod snapshot;
pub use render::{Mesh, render};
pub use snapshot::ViewSnapshot;

use automerge::transaction::Transactable;
use automerge::{ActorId, AutoCommit, ObjType, ROOT, ReadDoc, ScalarValue, TextEncoding};
use otrios_interfaces::{DashboardView, ViewContent, ViewNode};

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EDITS: usize = 4096;
pub type Result<T> = std::result::Result<T, String>;

fn seed() -> Result<AutoCommit> {
    // Every peer must share the *same history*, not independently create equal maps.
    // This actor is reserved for this exact immutable seed, never for user edits.
    let mut doc = AutoCommit::new_with_encoding(TextEncoding::UnicodeCodePoint)
        .with_actor(ActorId::from(b"otrios-dashboard-seed-v1".to_vec()));
    doc.put(ROOT, "schema", "otrios-dashboard-v1")
        .map_err(err)?;
    let title = doc.put_object(ROOT, "title", ObjType::Text).map_err(err)?;
    doc.splice_text(&title, 0, 0, "Studio dashboard")
        .map_err(err)?;
    doc.put(ROOT, "enabled", true).map_err(err)?;
    doc.put(ROOT, "touches", ScalarValue::Counter(0.into()))
        .map_err(err)?;
    let samples = doc
        .put_object(ROOT, "samples", ObjType::List)
        .map_err(err)?;
    for (i, sample) in [28_i64, 42, 36, 57, 48, 65, 59, 74, 62, 81, 69, 78]
        .into_iter()
        .enumerate()
    {
        doc.insert(&samples, i, sample).map_err(err)?;
    }
    doc.commit();
    Ok(doc)
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone)]
pub struct Replica {
    doc: AutoCommit,
    snapshot: ViewSnapshot,
}
impl Replica {
    pub fn new() -> Result<Self> {
        let mut doc = seed()?.fork();
        let snapshot = project(&mut doc)?;
        Ok(Self { doc, snapshot })
    }
    /// A fresh actor on every load prevents two tabs/processes sharing an actor.
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let mut doc = load_document(bytes)?.fork();
        let snapshot = project(&mut doc)?;
        Ok(Self { doc, snapshot })
    }
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }
    pub fn snapshot(&self) -> &ViewSnapshot {
        &self.snapshot
    }
    /// Prepare, validate, then replace the root. Errors retain both old doc and view.
    pub fn merge(&mut self, bytes: &[u8]) -> Result<bool> {
        let mut incoming = load_document(bytes)?;
        let mut candidate = self.doc.clone();
        candidate.merge(&mut incoming).map_err(err)?;
        self.publish(candidate)
    }
    pub fn set_enabled(&mut self, enabled: bool) -> Result<bool> {
        let mut candidate = self.doc.clone();
        candidate.put(ROOT, "enabled", enabled).map_err(err)?;
        candidate.increment(ROOT, "touches", 1).map_err(err)?;
        let samples = candidate
            .get(ROOT, "samples")
            .map_err(err)?
            .ok_or("Missing samples")?
            .1;
        let n = candidate.length(&samples);
        if n >= MAX_EDITS {
            return Err("Demo history limit reached; start a new room".into());
        }
        candidate
            .insert(&samples, n, if enabled { 78_i64 } else { 24_i64 })
            .map_err(err)?;
        candidate.commit(); // switch, count and sample are one CRDT change
        self.publish(candidate)
    }
    /// Coordinates refer to the currently displayed snapshot, not a hidden DOM layout.
    pub fn touch(&mut self, x: u32, y: u32) -> Result<bool> {
        for node in &self.snapshot.view().nodes {
            let [nx, ny, w, h] = node.bounds;
            if x >= nx
                && x < nx + w
                && y >= ny
                && y < ny + h
                && let ViewContent::Switch { enabled, .. } = node.content
            {
                return self.set_enabled(!enabled);
            }
        }
        Ok(false)
    }
    fn publish(&mut self, mut candidate: AutoCommit) -> Result<bool> {
        if candidate.save().len() > MAX_DOCUMENT_BYTES {
            return Err("Demo document limit reached".into());
        }
        let snapshot = project(&mut candidate)?;
        let changed = snapshot.id() != self.snapshot.id();
        self.doc = candidate;
        self.snapshot = snapshot;
        Ok(changed)
    }
}
fn load_document(bytes: &[u8]) -> Result<AutoCommit> {
    if bytes.is_empty() || bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("Invalid document size".into());
    }
    let doc = AutoCommit::load_with_options(
        bytes,
        automerge::LoadOptions::new().text_encoding(TextEncoding::UnicodeCodePoint),
    )
    .map_err(err)?;
    let seed_head = seed()?.get_heads()[0];
    if doc.get_change_by_hash(&seed_head).is_none() {
        return Err("Wrong dashboard seed/history".into());
    }
    Ok(doc)
}
fn project(doc: &mut AutoCommit) -> Result<ViewSnapshot> {
    let (_, title) = doc
        .get(ROOT, "title")
        .map_err(err)?
        .ok_or("Missing title")?;
    let title = doc.text(&title).map_err(err)?;
    if title.len() > 96 {
        return Err("Title too long".into());
    }
    let enabled = doc
        .get(ROOT, "enabled")
        .map_err(err)?
        .and_then(|(v, _)| v.as_bool())
        .ok_or("Invalid enabled flag")?;
    let touches = doc
        .get(ROOT, "touches")
        .map_err(err)?
        .and_then(|(v, _)| v.as_i64())
        .ok_or("Invalid touch count")?;
    if !(0..=1_000_000).contains(&touches) {
        return Err("Invalid touch count".into());
    }
    let (_, list) = doc
        .get(ROOT, "samples")
        .map_err(err)?
        .ok_or("Missing samples")?;
    let n = doc.length(&list);
    if n > MAX_EDITS {
        return Err("Too many demo edits".into());
    }
    let mut samples = Vec::new();
    for i in n.saturating_sub(32)..n {
        let v = doc
            .get(&list, i)
            .map_err(err)?
            .and_then(|(v, _)| v.as_i64())
            .ok_or("Invalid sample")?;
        if !(0..=100).contains(&v) {
            return Err("Sample outside 0..100".into());
        }
        samples.push(v as u32);
    }
    let mut heads: Vec<_> = doc.get_heads().iter().map(ToString::to_string).collect();
    heads.sort();
    ViewSnapshot::build(DashboardView {
        heads,
        nodes: vec![
            ViewNode {
                id: 1,
                bounds: [64, 64, 1152, 96],
                content: ViewContent::Label(title),
            },
            ViewNode {
                id: 2,
                bounds: [64, 208, 440, 240],
                content: ViewContent::Switch {
                    label: "Focus mode".into(),
                    enabled,
                },
            },
            ViewNode {
                id: 3,
                bounds: [536, 208, 680, 400],
                content: ViewContent::Graph {
                    label: "Sample signal".into(),
                    samples,
                },
            },
            ViewNode {
                id: 4,
                bounds: [64, 500, 440, 108],
                content: ViewContent::Label(format!("{touches} touches")),
            },
            ViewNode {
                id: 5,
                bounds: [64, 676, 1152, 60],
                content: ViewContent::Label("Touch a tile. Your other screen follows.".into()),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_started_peers_have_identical_seed_and_snapshot() {
        assert_eq!(
            Replica::new().unwrap().snapshot.hex_id(),
            "d101fac0c9bc309dd8067bb52ceac20001c7c4dce513fe8e7c93820f623f4daf"
        );
        assert_eq!(
            Replica::new().unwrap().snapshot.id(),
            Replica::new().unwrap().snapshot.id()
        );
    }
    #[test]
    fn offline_concurrent_edits_converge_and_duplicate_merge_is_idempotent() {
        let mut a = Replica::new().unwrap();
        let mut b = Replica::new().unwrap();
        a.set_enabled(false).unwrap();
        b.set_enabled(true).unwrap();
        let a_change = a.save();
        let b_change = b.save();
        a.merge(&b_change).unwrap();
        b.merge(&a_change).unwrap();
        assert_eq!(a.snapshot.bytes(), b.snapshot.bytes());
        assert!(
            a.snapshot
                .view()
                .nodes
                .iter()
                .any(|n| n.content == ViewContent::Label("2 touches".into()))
        );
        assert!(!a.merge(&b_change).unwrap());
    }
    #[test]
    fn old_snapshot_is_pinned_and_stale_response_preserves_new_local_edits() {
        let mut a = Replica::new().unwrap();
        let old = a.snapshot.clone();
        let stale = a.save();
        a.set_enabled(false).unwrap();
        let id = a.snapshot.id();
        a.merge(&stale).unwrap();
        assert_eq!(id, a.snapshot.id());
        assert_ne!(old.id(), id);
        let mut reloaded = Replica::load(&a.save()).unwrap();
        reloaded.set_enabled(true).unwrap();
        a.set_enabled(false).unwrap();
        let data = reloaded.save();
        a.merge(&data).unwrap();
        assert!(
            a.snapshot
                .view()
                .nodes
                .iter()
                .any(|n| n.content == ViewContent::Label("3 touches".into()))
        );
    }
    #[test]
    fn invalid_update_and_unrelated_history_leave_root_intact() {
        let mut a = Replica::new().unwrap();
        let old = a.snapshot.id();
        assert!(a.merge(b"bad").is_err());
        assert!(a.merge(&vec![0; MAX_DOCUMENT_BYTES + 1]).is_err());
        assert!(a.merge(&AutoCommit::new().save()).is_err());
        assert_eq!(old, a.snapshot.id());
    }
    #[test]
    fn hit_test_uses_published_bounds() {
        let mut a = Replica::new().unwrap();
        assert!(!a.touch(0, 0).unwrap());
        assert!(a.touch(200, 300).unwrap());
    }
}
