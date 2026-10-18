// SPDX-License-Identifier: LGPL-3.0-only
//! Dashboard experiment: CRDT edits -> complete immutable view -> shared compositor.
//! This is a bounded, trusted-peer demo; it does not implement Linux shared memory.

pub mod components;
pub mod controls;
pub mod display;
pub mod ink;
pub mod input;
pub mod keyboard;
pub mod layout;
pub mod motion;
pub mod presentation;
mod render;
pub mod scene;
pub mod shell;
mod snapshot;
pub use render::{Mesh, render, render_scene};
pub use snapshot::ViewSnapshot;

use automerge::sync::SyncDoc;
use automerge::transaction::Transactable;
use automerge::{ActorId, AutoCommit, ObjType, ROOT, ReadDoc, ScalarValue, TextEncoding};
use otrios_interfaces::{DashboardView, ViewContent, ViewNode};

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EDITS: usize = 4096;
pub type Result<T> = std::result::Result<T, String>;

/// Ephemeral protocol state for one reliable, ordered connection. Reset on reconnect.
#[derive(Clone, Default)]
pub struct SyncSession(automerge::sync::State);

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
    base_snapshot: ViewSnapshot,
    saved: std::sync::Arc<[u8]>,
    presentation: Option<layout::Layout>,
}
impl Replica {
    pub fn new() -> Result<Self> {
        let mut doc = seed()?.fork();
        let (base_snapshot, snapshot) = project_cached(&mut doc, None, None, None)?;
        let saved = doc.save().into();
        Ok(Self {
            doc,
            snapshot,
            base_snapshot,
            saved,
            presentation: None,
        })
    }
    /// A fresh actor on every load prevents two tabs/processes sharing an actor.
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let mut doc = load_document(bytes)?.fork();
        let (base_snapshot, snapshot) = project_cached(&mut doc, None, None, None)?;
        let saved = doc.save().into();
        Ok(Self {
            doc,
            snapshot,
            base_snapshot,
            saved,
            presentation: None,
        })
    }
    pub fn controls(&self) -> Result<controls::State> {
        controls::State::read(&self.doc)
    }
    /// Each scalar has its own merge register. The pad pair is one atomic value.
    pub fn set_control(&mut self, control: u8, value: [u16; 2]) -> Result<bool> {
        if value.iter().any(|v| *v > 1000) || (control < 2 && value[1] != 0) {
            return Err("Invalid control value".into());
        }
        if self.controls()?.value(control)? == value {
            return Ok(false);
        }
        let mut candidate = self.doc.clone();
        match control {
            0 | 1 => {
                candidate
                    .put(
                        ROOT,
                        if control == 0 {
                            "control_level_v1"
                        } else {
                            "control_dial_v1"
                        },
                        u64::from(value[0]),
                    )
                    .map_err(err)?;
            }
            2 => {
                let bytes = value
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>();
                candidate
                    .put(ROOT, "control_pad_v1", ScalarValue::Bytes(bytes))
                    .map_err(err)?;
            }
            _ => return Err("Unknown control".into()),
        }
        candidate.commit();
        self.publish(candidate)
    }
    pub fn motion(&self) -> Result<motion::Intent> {
        match self.doc.get(ROOT, "motion").map_err(err)? {
            Some((value, _)) => {
                motion::Intent::decode(value.as_bytes().ok_or("Motion must be binary")?)
            }
            None => Ok(motion::Intent::default()),
        }
    }
    pub fn set_motion(&mut self, intent: motion::Intent) -> Result<bool> {
        intent.validate()?;
        if intent == self.motion()? {
            return Ok(false);
        }
        let mut candidate = self.doc.clone();
        candidate
            .put(ROOT, "motion", ScalarValue::Bytes(intent.encode()))
            .map_err(err)?;
        candidate.commit();
        self.publish(candidate)
    }
    pub fn save(&mut self) -> Vec<u8> {
        self.saved.to_vec()
    }
    pub fn confirmed_heads(session: &SyncSession) -> Vec<String> {
        session
            .0
            .shared_heads
            .iter()
            .map(ToString::to_string)
            .collect()
    }
    pub fn is_confirmed(&self, session: &SyncSession) -> bool {
        let common = Self::confirmed_heads(session);
        self.snapshot
            .view()
            .heads
            .iter()
            .all(|h| common.contains(h))
    }
    pub fn snapshot(&self) -> &ViewSnapshot {
        &self.snapshot
    }
    /// Latest complete read-only feed view; never inserted into the CRDT history.
    pub fn presentation(&self) -> Option<&layout::Layout> {
        self.presentation.as_ref()
    }
    pub fn base_layout(&self) -> Result<layout::Layout> {
        match self.doc.get(ROOT, "layout").map_err(err)? {
            Some((value, _)) => {
                layout::Layout::from_bytes(value.as_bytes().ok_or("Invalid layout")?)
            }
            None => layout::Layout::bootstrap(),
        }
    }
    pub fn set_presentation(&mut self, bytes: Option<&[u8]>) -> Result<bool> {
        if self.presentation.as_ref().map(|p| p.bytes()) == bytes {
            return Ok(false);
        }
        let next = bytes.map(layout::Layout::from_bytes).transpose()?;
        let (base, snapshot) = project_cached(
            &mut self.doc,
            Some(&self.base_snapshot),
            next.as_ref(),
            Some(&self.snapshot),
        )?;
        self.presentation = next;
        self.base_snapshot = base;
        self.snapshot = snapshot;
        Ok(true)
    }
    pub fn set_layout(&mut self, bytes: &[u8]) -> Result<bool> {
        let layout = layout::Layout::from_bytes(bytes)?;
        layout.qualify(self.snapshot.view())?;
        if layout.bytes() == self.base_layout()?.bytes() {
            return Ok(false);
        }
        let mut candidate = self.doc.clone();
        candidate
            .put(ROOT, "layout", ScalarValue::Bytes(bytes.to_vec()))
            .map_err(err)?;
        candidate.commit();
        self.publish(candidate)
    }
    pub fn generate_sync(&mut self, session: &mut SyncSession) -> Result<Option<Vec<u8>>> {
        let data = self
            .doc
            .sync()
            .generate_sync_message(&mut session.0)
            .map(|m| m.encode());
        if data.as_ref().is_some_and(|b| b.len() > MAX_DOCUMENT_BYTES) {
            return Err("Sync message exceeds demo limit".into());
        }
        Ok(data)
    }
    /// Invalid messages retain both the previous view and protocol state.
    pub fn receive_sync(&mut self, session: &mut SyncSession, bytes: &[u8]) -> Result<bool> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err("Sync message too large".into());
        }
        let message = automerge::sync::Message::decode(bytes).map_err(err)?;
        let mut doc = self.doc.clone();
        let mut next_session = session.clone();
        doc.sync()
            .receive_sync_message(&mut next_session.0, message)
            .map_err(err)?;
        let changed = self.publish(doc)?;
        *session = next_session;
        Ok(changed)
    }
    /// Public demo peers may mutate only the existing switch/count/sample and
    /// motion fields. Check every new operation, including losing conflicts, so
    /// a protected layout/schema write cannot hide behind an unchanged view.
    pub fn receive_demo_sync(&mut self, session: &mut SyncSession, bytes: &[u8]) -> Result<bool> {
        use automerge::legacy::{Key, ObjectId, OpId, OpType};
        let mut candidate = self.clone();
        let mut next = session.clone();
        let changed = candidate.receive_sync(&mut next, bytes)?;
        let samples = match self
            .doc
            .get(ROOT, "samples")
            .map_err(err)?
            .ok_or("Missing samples")?
            .1
        {
            automerge::ObjId::Id(counter, actor, _) => ObjectId::Id(OpId::new(counter, &actor)),
            _ => return Err("Invalid demo sample object".into()),
        };
        for change in self.doc.get_changes_added(&mut candidate.doc) {
            if change.len() > 16 {
                return Err("Demo change exceeds operation limit".into());
            }
            for op in change.decode().operations {
                let allowed = match (&op.obj, &op.key, &op.action, op.insert) {
                    (
                        ObjectId::Root,
                        Key::Map(key),
                        OpType::Put(ScalarValue::Boolean(_)),
                        false,
                    ) if key == "enabled" => true,
                    (ObjectId::Root, Key::Map(key), OpType::Increment(1), false)
                        if key == "touches" =>
                    {
                        true
                    }
                    (
                        ObjectId::Root,
                        Key::Map(key),
                        OpType::Put(ScalarValue::Bytes(bytes)),
                        false,
                    ) if key == "motion" => motion::Intent::decode(bytes).is_ok(),
                    (ObjectId::Root, Key::Map(key), OpType::Put(ScalarValue::Uint(v)), false)
                        if matches!(key.as_str(), "control_level_v1" | "control_dial_v1") =>
                    {
                        *v <= 1000
                    }
                    (
                        ObjectId::Root,
                        Key::Map(key),
                        OpType::Put(ScalarValue::Bytes(bytes)),
                        false,
                    ) if key == "control_pad_v1" => {
                        bytes.len() == 4
                            && bytes
                                .chunks_exact(2)
                                .all(|v| u16::from_le_bytes([v[0], v[1]]) <= 1000)
                    }
                    (obj, Key::Seq(_), OpType::Put(ScalarValue::Int(24 | 78)), true)
                        if *obj == samples =>
                    {
                        true
                    }
                    _ => false,
                };
                if !allowed {
                    return Err("Public peer attempted a protected change".into());
                }
            }
        }
        *self = candidate;
        *session = next;
        Ok(changed)
    }
    /// Prepare, validate, then replace the root. Errors retain both old doc and view.
    pub fn merge(&mut self, bytes: &[u8]) -> Result<bool> {
        // Publication checks newly added roots against our validated history.
        let mut incoming = decode_document(bytes)?;
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
    /// Compatibility tap adapter; real hosts deliver the complete pointer sequence.
    pub fn touch(&mut self, x: u32, y: u32) -> Result<bool> {
        use otrios_interfaces::scene::{PointerEvent, PointerPhase};
        let mut router = input::InputRouter::default();
        let event = PointerEvent {
            id: 0,
            phase: PointerPhase::Down,
            position: [x as f32, y as f32],
        };
        router.dispatch(self.snapshot.scene().clone(), event);
        let result = router.dispatch(
            self.snapshot.scene().clone(),
            PointerEvent {
                phase: PointerPhase::Up,
                ..event
            },
        );
        self.apply_action(result.action)
    }
    pub fn apply_action(
        &mut self,
        action: Option<otrios_interfaces::scene::Action>,
    ) -> Result<bool> {
        match action {
            Some(otrios_interfaces::scene::Action::ToggleFocus) => {
                let enabled = self
                    .snapshot
                    .view()
                    .nodes
                    .iter()
                    .find_map(|n| match n.content {
                        ViewContent::Switch { enabled, .. } => Some(enabled),
                        _ => None,
                    })
                    .ok_or("Missing focus switch")?;
                self.set_enabled(!enabled)
            }
            _ => Ok(false),
        }
    }
    fn publish(&mut self, mut candidate: AutoCommit) -> Result<bool> {
        let known = self.doc.get_heads();
        if candidate
            .get_changes_meta(&known)
            .iter()
            .any(|change| change.deps.is_empty())
        {
            return Err("Unrelated document history".into());
        }
        let saved = candidate.save();
        if saved.len() > MAX_DOCUMENT_BYTES {
            return Err("Demo document limit reached".into());
        }
        let (base, snapshot) = project_cached(
            &mut candidate,
            Some(&self.base_snapshot),
            self.presentation.as_ref(),
            Some(&self.snapshot),
        )?;
        let changed = snapshot.id() != self.snapshot.id();
        self.doc = candidate;
        self.saved = saved.into();
        self.base_snapshot = base;
        self.snapshot = snapshot;
        Ok(changed)
    }
}
fn load_document(bytes: &[u8]) -> Result<AutoCommit> {
    let mut doc = decode_document(bytes)?;
    validate_lineage(&mut doc)?;
    Ok(doc)
}
fn decode_document(bytes: &[u8]) -> Result<AutoCommit> {
    if bytes.is_empty() || bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("Invalid document size".into());
    }
    let mut doc = AutoCommit::load_with_options(
        bytes,
        automerge::LoadOptions::new().text_encoding(TextEncoding::UnicodeCodePoint),
    )
    .map_err(err)?;
    let seed_head = seed()?.get_heads()[0];
    if doc.get_change_meta_by_hash(&seed_head).is_none() {
        return Err("Wrong dashboard seed/history".into());
    }
    Ok(doc)
}
fn validate_lineage(doc: &mut AutoCommit) -> Result<()> {
    let seed_head = seed()?.get_heads()[0];
    if doc
        // Metadata carries exactly the roots/dependencies needed here. Asking
        // for full Changes re-encodes every historical operation on each edit.
        .get_changes_meta(&[])
        .iter()
        .any(|change| change.deps.is_empty() && change.hash != seed_head)
    {
        return Err("Unrelated document history".into());
    }
    Ok(())
}
fn project_cached(
    doc: &mut AutoCommit,
    cached: Option<&ViewSnapshot>,
    presentation: Option<&layout::Layout>,
    visible: Option<&ViewSnapshot>,
) -> Result<(ViewSnapshot, ViewSnapshot)> {
    controls::State::read(doc)?;
    if let Some((v, _)) = doc.get(ROOT, "motion").map_err(err)? {
        motion::Intent::decode(v.as_bytes().ok_or("Motion must be binary")?)?;
    }
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
    let layout = doc
        .get(ROOT, "layout")
        .map_err(err)?
        .map(|(value, _)| {
            value
                .as_bytes()
                .map(|v| v.to_vec())
                .ok_or("Layout must be a binary scalar")
        })
        .transpose()?;
    let view = DashboardView {
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
    };
    // A read-only feed cannot conceal an invalid collaborative layout that would fail on clear.
    let base = ViewSnapshot::build_cached(view.clone(), layout.as_deref(), cached)?;
    let snapshot = if let Some(presentation) = presentation {
        ViewSnapshot::build_cached(view, Some(presentation.bytes()), visible)?
    } else {
        base.clone()
    };
    Ok((base, snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn drain(
        a: &mut Replica,
        sa: &mut SyncSession,
        b: &mut Replica,
        sb: &mut SyncSession,
    ) -> usize {
        let mut sent = 0;
        for _ in 0..100 {
            let one = a.generate_sync(sa).unwrap();
            let two = b.generate_sync(sb).unwrap();
            if one.is_none() && two.is_none() {
                return sent;
            }
            if let Some(bytes) = one {
                sent += bytes.len();
                b.receive_sync(sb, &bytes).unwrap();
            }
            if let Some(bytes) = two {
                sent += bytes.len();
                a.receive_sync(sa, &bytes).unwrap();
            }
        }
        panic!("Sync did not quiesce");
    }
    #[test]
    fn acknowledgement_covers_current_heads_and_resets_for_new_session() {
        let mut client = Replica::new().unwrap();
        let mut server = Replica::new().unwrap();
        let mut c = SyncSession::default();
        let mut s = SyncSession::default();
        assert!(!client.is_confirmed(&c));
        drain(&mut client, &mut c, &mut server, &mut s);
        assert!(client.is_confirmed(&c));
        client.set_control(0, [735, 0]).unwrap();
        assert!(!client.is_confirmed(&c));
        drain(&mut client, &mut c, &mut server, &mut s);
        assert!(client.is_confirmed(&c));
        assert!(!client.is_confirmed(&SyncSession::default()));
        let mut fresh = Replica::new().unwrap();
        let mut f = SyncSession::default();
        let mut server_session = SyncSession::default();
        drain(&mut fresh, &mut f, &mut server, &mut server_session);
        assert!(fresh.is_confirmed(&f));
        assert_eq!(fresh.controls().unwrap().level, 735);
    }
    fn drain_public(
        a: &mut Replica,
        sa: &mut SyncSession,
        b: &mut Replica,
        sb: &mut SyncSession,
    ) -> Result<()> {
        for _ in 0..100 {
            let one = a.generate_sync(sa)?;
            let two = b.generate_sync(sb)?;
            if one.is_none() && two.is_none() {
                return Ok(());
            }
            if let Some(bytes) = one {
                b.receive_demo_sync(sb, &bytes)?;
            }
            if let Some(bytes) = two {
                a.receive_sync(sa, &bytes)?;
            }
        }
        Err("Public sync did not quiesce".into())
    }
    #[test]
    fn public_demo_sync_accepts_controls_and_motion_without_replacing_presentation() {
        let mut browser = Replica::new().unwrap();
        let mut frame = Replica::new().unwrap();
        frame
            .set_presentation(Some(layout::DEFAULT_LAYOUT))
            .unwrap();
        let presentation = frame.presentation().unwrap().hex_id();
        let (mut a, mut b) = (SyncSession::default(), SyncSession::default());
        drain_public(&mut browser, &mut a, &mut frame, &mut b).unwrap();
        browser.set_enabled(false).unwrap();
        browser
            .set_motion(motion::Intent {
                count: 48,
                speed: 2.,
                ..Default::default()
            })
            .unwrap();
        drain_public(&mut browser, &mut a, &mut frame, &mut b).unwrap();
        assert_eq!(frame.motion().unwrap(), browser.motion().unwrap());
        assert_eq!(frame.presentation().unwrap().hex_id(), presentation);
        let mut public_copy = Replica::load(&frame.save()).unwrap();
        assert!(public_copy.presentation().is_none());
        assert_eq!(public_copy.save(), frame.save());
    }
    #[test]
    fn public_sync_rejects_protected_operations_even_when_the_visible_layout_is_unchanged() {
        for field in ["layout", "schema", "admin"] {
            let mut browser = Replica::new().unwrap();
            let mut authority = Replica::new().unwrap();
            let (mut a, mut b) = (SyncSession::default(), SyncSession::default());
            drain_public(&mut browser, &mut a, &mut authority, &mut b).unwrap();
            let before = authority.save();
            if field == "layout" {
                let bytes = browser.base_layout().unwrap().bytes().to_vec();
                browser
                    .doc
                    .put(ROOT, field, ScalarValue::Bytes(bytes))
                    .unwrap();
            } else {
                browser.doc.put(ROOT, field, "unauthorized").unwrap();
            }
            browser.doc.commit();
            assert!(drain_public(&mut browser, &mut a, &mut authority, &mut b).is_err());
            assert_eq!(authority.save(), before);
        }
    }
    #[test]
    fn independent_component_registers_merge_and_pad_coordinates_stay_atomic() {
        let mut a = Replica::new().unwrap();
        let mut b = Replica::new().unwrap();
        a.set_control(0, [920, 0]).unwrap();
        b.set_control(1, [180, 0]).unwrap();
        let (mut sa, mut sb) = (SyncSession::default(), SyncSession::default());
        drain_public(&mut a, &mut sa, &mut b, &mut sb).unwrap();
        assert_eq!(
            a.controls().unwrap(),
            controls::State {
                level: 920,
                dial: 180,
                pad: [500, 500]
            }
        );
        a.set_control(2, [120, 800]).unwrap();
        b.set_control(2, [700, 300]).unwrap();
        drain_public(&mut a, &mut sa, &mut b, &mut sb).unwrap();
        assert_eq!(a.controls().unwrap(), b.controls().unwrap());
        assert!([[120, 800], [700, 300]].contains(&a.controls().unwrap().pad));
        let before = a.snapshot().id();
        assert!(a.set_control(0, [1001, 0]).is_err());
        assert!(a.set_control(2, [0, 1001]).is_err());
        assert!(a.set_control(3, [0, 0]).is_err());
        assert_eq!(a.snapshot().id(), before);
    }
    #[test]
    fn unchanged_bindings_reuse_scenes_but_changed_views_and_layouts_are_revalidated() {
        use std::sync::Arc;
        let mut r = Replica::new().unwrap();
        let mut b = layout::Layout::bootstrap().unwrap().blueprint().clone();
        b.root.properties.push(otrios_interfaces::layout::Property {
            key: 2,
            value: otrios_interfaces::layout::Expr::Literal(
                otrios_interfaces::layout::Value::Color([7, 8, 9, 255]),
            ),
        });
        let private = layout::Layout::encode(&b).unwrap();
        r.set_presentation(Some(&private)).unwrap();
        let pinned = r.snapshot().clone();
        let base = r.base_snapshot.clone();
        r.set_control(0, [731, 0]).unwrap();
        assert_ne!(r.snapshot().id(), pinned.id());
        assert!(Arc::ptr_eq(r.snapshot().scene(), pinned.scene()));
        assert!(Arc::ptr_eq(r.base_snapshot.scene(), base.scene()));
        let copy = ViewSnapshot::from_bytes(r.snapshot().bytes()).unwrap();
        assert_eq!(
            render_scene(copy.scene()).vertices,
            render_scene(r.snapshot().scene()).vertices
        );
        r.set_enabled(false).unwrap();
        assert!(!Arc::ptr_eq(r.snapshot().scene(), pinned.scene()));
        assert!(!Arc::ptr_eq(r.base_snapshot.scene(), base.scene()));
        // A hidden collaborative layout is still validated before publication.
        let before = r.snapshot().id();
        let mut invalid = r.doc.clone();
        invalid
            .put(ROOT, "layout", ScalarValue::Bytes(vec![0]))
            .unwrap();
        invalid.commit();
        assert!(r.publish(invalid).is_err());
        assert_eq!(r.snapshot().id(), before);
        r.set_presentation(None).unwrap();
        assert!(Arc::ptr_eq(r.snapshot().scene(), r.base_snapshot.scene()));
        assert!(!Arc::ptr_eq(r.snapshot().scene(), pinned.scene()));
    }
    #[test]
    fn incremental_sync_pushes_edits_and_recovers_offline_concurrency() {
        let mut a = Replica::new().unwrap();
        let mut b = Replica::new().unwrap();
        let mut sa = SyncSession::default();
        let mut sb = SyncSession::default();
        drain(&mut a, &mut sa, &mut b, &mut sb);
        a.set_enabled(false).unwrap();
        drain(&mut a, &mut sa, &mut b, &mut sb);
        assert_eq!(a.snapshot.id(), b.snapshot.id());
        a.set_enabled(true).unwrap();
        b.set_enabled(false).unwrap();
        sa = SyncSession::default();
        sb = SyncSession::default();
        drain(&mut a, &mut sa, &mut b, &mut sb);
        assert_eq!(a.snapshot.id(), b.snapshot.id());
        assert!(
            a.snapshot
                .view()
                .nodes
                .iter()
                .any(|n| n.content == ViewContent::Label("3 touches".into()))
        );
        assert!(a.generate_sync(&mut sa).unwrap().is_none());
    }
    #[test]
    fn malformed_sync_preserves_view_and_connection_can_continue() {
        let mut a = Replica::new().unwrap();
        let mut b = Replica::new().unwrap();
        let mut sa = SyncSession::default();
        let mut sb = SyncSession::default();
        let old = a.snapshot.id();
        assert!(a.receive_sync(&mut sa, b"invalid").is_err());
        assert_eq!(a.snapshot.id(), old);
        b.set_enabled(false).unwrap();
        drain(&mut a, &mut sa, &mut b, &mut sb);
        assert_eq!(a.snapshot.id(), b.snapshot.id());
    }
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
        // Even a valid seed cannot disguise a second unrelated history root.
        let mut graft = seed().unwrap();
        let mut foreign = AutoCommit::new();
        foreign.put(ROOT, "extra", true).unwrap();
        foreign.commit();
        graft.merge(&mut foreign).unwrap();
        let graft = graft.save();
        assert!(Replica::load(&graft).is_err());
        assert!(a.merge(&graft).is_err());
        assert_eq!(old, a.snapshot.id());
        assert!(Replica::load(&a.save()).is_ok());
    }
    #[test]
    fn hit_test_uses_published_bounds() {
        let mut a = Replica::new().unwrap();
        assert!(!a.touch(0, 0).unwrap());
        assert!(a.touch(200, 300).unwrap());
    }
}
