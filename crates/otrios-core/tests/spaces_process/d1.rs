// SPDX-License-Identifier: LGPL-3.0-only
//! Real authority process, loopback WebSocket sessions and in-process adapters.
//! These tests do not establish Linux shared-memory IPC or spawned-client evidence.
use super::*;
use canvas::{Document, StateAdapter, oib};
use otrios_core::merkle::Snapshot;
use otrios_interfaces::oib::{MAX_ALLOCATIONS, MAX_BYTES, MAX_CELLS, designer::*};
use std::collections::BTreeSet;

#[path = "../../../../apps/oib-designer/app.rs"]
mod app;

const STATE: &str = "app:oib-designer/state";
const VALUES: [(u16, u16); 4] = [(1, 481), (2, 729), (3, 0), (4, 1000)];

struct Fixture {
    rig: Rig,
    space: String,
    ui: Ui,
    counter: String,
    counter_bytes: Vec<u8>,
}
impl Fixture {
    fn start(binary: &str) -> Self {
        let mut rig = Rig::start(binary);
        let space = rig.space("D1 authority process acceptance");
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "counter".into(),
            },
        );
        let (value, counter) = document_session(&rig, &space, "counter", schema(), Some(431));
        assert_eq!(value, 431);
        let counter = counter.unwrap();
        let counter_bytes =
            fs::read(rig.root.join(format!("state/collection-{counter}.merkle"))).unwrap();
        rig.stop();
        // Add the real companion alongside the existing app. Do not rewrite its
        // ui_ref, fields, signed mappings, seed, selectors or factory identities.
        let ui = app::ui(&[]);
        let manifest = app::manifest();
        assert_eq!(ui.ui_ref, "app:oib-designer/ui");
        assert_eq!(ui.state_schema.id, "oib-designer-state-v1");
        assert_eq!(
            ui.state_schema
                .fields
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        let bytes = canvas::encode_ui(&ui).unwrap();
        let package = rig.root.join("apps/oib-designer/package");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("ui.flex"), &bytes).unwrap();
        fs::write(
            package.join("app.flex"),
            canvas::encode_app(&manifest).unwrap(),
        )
        .unwrap();
        let mut catalog = canvas::decode_catalog(&rig.catalog).unwrap();
        catalog.apps.push(CatalogEntry {
            app: manifest,
            path: "oib-designer".into(),
            private: false,
            ui_hash: canvas::hash(&bytes),
        });
        rig.catalog = canvas::encode_catalog(&catalog).unwrap();
        fs::write(rig.root.join("apps/catalog.flex"), &rig.catalog).unwrap();
        rig.restart(binary, true, true);
        rig.change(
            'a',
            Change::Attach {
                space: space.clone(),
                app: "oib-designer".into(),
            },
        );
        rig.change(
            'a',
            Change::Membership {
                space: space.clone(),
                identity: "bob".into(),
                role: Some(Role::Member),
            },
        );
        Self {
            rig,
            space,
            ui,
            counter,
            counter_bytes,
        }
    }
    fn open(&self, actor: char, writer: u8, resume: u64) -> Peer {
        let mut socket = socket(&self.rig.public, &self.space, Some(actor));
        send(
            &mut socket,
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: self.ui.ui_ref.clone(),
                state_ref: STATE.into(),
                catalog_hash: canvas::hash(&self.rig.catalog),
                known_ui: None,
                known_code: vec![],
                writer: vec![writer; 16],
                resume,
                collection: None,
            },
        );
        let response = read(&mut socket);
        let ServerMessage::Mounted {
            state,
            ui,
            ui_hash,
            binding: Some(binding),
            ..
        } = response
        else {
            panic!("D1 mount: {response:?}");
        };
        assert_eq!(ui, canvas::encode_ui(&self.ui).unwrap());
        assert_eq!(ui_hash, canvas::hash(&ui));
        assert_ne!(binding.collection, self.counter, "separate app collection");
        let state = StateAdapter::for_ui(&self.ui, STATE, &state, [writer; 16]).unwrap();
        send(&mut socket, ClientMessage::AcceptMount { epoch: 1 });
        assert!(matches!(
            read(&mut socket),
            ServerMessage::Activated { epoch: 1 }
        ));
        let mut peer = Peer {
            socket,
            state,
            binding,
        };
        peer.settle();
        peer
    }
    fn counter_unchanged(&self) {
        assert_eq!(
            document_session(&self.rig, &self.space, "counter", schema(), None).0,
            431
        );
        assert_eq!(
            fs::read(
                self.rig
                    .root
                    .join(format!("state/collection-{}.merkle", self.counter))
            )
            .unwrap(),
            self.counter_bytes
        );
    }
    /// Read only test-owned storage while sessions are idle or the child is dead.
    /// Check each persisted record's framing, digest and root chain, then use the
    /// runtime replay API. The server's own restart below independently recovers it.
    fn durable(&self, collection: &str) -> Document {
        let path = self
            .rig
            .root
            .join(format!("state/collection-{collection}.merkle"));
        let mut doc = Document::load(
            self.ui.state_schema.clone(),
            STATE,
            &fs::read(&path).unwrap(),
        )
        .unwrap();
        let log = fs::read(path.with_extension("merkle.log")).unwrap();
        assert!(
            !log.is_empty(),
            "acceptance must exercise checkpoint plus journal"
        );
        let mut records = vec![];
        let mut at = 0;
        while at < log.len() {
            assert!(log.len() - at >= 36, "complete acknowledged journal header");
            let len = u32::from_le_bytes(log[at..at + 4].try_into().unwrap()) as usize;
            assert!(len <= canvas::MAX_BYTES);
            let end = at.checked_add(36).unwrap().checked_add(len).unwrap();
            assert!(end <= log.len(), "complete acknowledged journal body");
            let record = &log[at + 36..end];
            assert_eq!(canvas::hash(record), log[at + 4..at + 36]);
            let (base, root) = Document::journal_roots(record).unwrap();
            records.push((base, root, record));
            at = end;
        }
        let start = records
            .iter()
            .rposition(|(_, root, _)| *root == doc.root_hash())
            .map_or(0, |i| i + 1);
        for (base, _, record) in &records[start..] {
            assert_eq!(*base, doc.root_hash());
            doc.replay(record).unwrap();
        }
        assert!(
            !doc.configure_oib(&self.ui).unwrap(),
            "recovery does not reseed"
        );
        assert_bounds(&self.ui, &doc);
        doc
    }
}

struct Peer {
    socket: Socket,
    state: StateAdapter,
    binding: MountBinding,
}
impl Peer {
    fn send(&mut self, bytes: Vec<u8>) {
        send(&mut self.socket, ClientMessage::Sync { epoch: 1, bytes });
    }
    fn flush(&mut self) {
        for _ in 0..64 {
            let Some(bytes) = self.state.generate().unwrap() else {
                return;
            };
            self.send(bytes);
        }
        panic!("D1 outgoing sync did not quiesce");
    }
    fn until(&mut self, predicate: impl Fn(&StateAdapter) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..256 {
            if predicate(&self.state) {
                return;
            }
            assert!(Instant::now() < deadline, "D1 observation deadline");
            let message = read(&mut self.socket);
            match message {
                ServerMessage::Sync { bytes, .. } => {
                    self.state.receive(&bytes).unwrap();
                }
                other => panic!("D1 sync: {other:?}"),
            }
            self.flush();
        }
        panic!("D1 observation message bound");
    }
    fn settle(&mut self) {
        self.flush();
        self.until(StateAdapter::confirmed);
    }
    fn edit(&mut self, changes: &[(u64, Option<Value>)]) {
        assert!(self.state.edit_oib(changes).unwrap());
        self.settle();
    }
    fn issue(&mut self) -> Vec<u8> {
        for _ in 0..64 {
            let bytes = self.state.generate().unwrap().expect("pending D1 intent");
            if matches!(
                canvas::decode_state_request(&bytes).unwrap(),
                StateRequest::OibWrite { .. } | StateRequest::OibInstance { .. }
            ) {
                return bytes;
            }
            self.send(bytes);
        }
        panic!("D1 intent missing");
    }
    fn observe(&mut self, other: &Peer) {
        let active = other.state.oib_active().unwrap();
        let generation = other.state.oib_generation();
        let conflicts = other.state.oib_conflicts();
        let instances: Vec<_> = other
            .state
            .oib_instances()
            .unwrap()
            .iter()
            .map(|i| (i.id, i.active))
            .collect();
        self.until(|s| {
            s.oib_active().unwrap() == active
                && s.oib_generation() == generation
                && s.oib_conflicts() == conflicts
                && s.oib_instances()
                    .unwrap()
                    .iter()
                    .map(|i| (i.id, i.active))
                    .collect::<Vec<_>>()
                    == instances
        });
    }
}

fn assert_values(state: &StateAdapter) {
    for (field, value) in VALUES {
        assert_eq!(state.value(field).unwrap(), value, "field {field}");
    }
}
fn seed(ui: &Ui) -> &otrios_interfaces::oib::Seed {
    oib::seed(ui).unwrap()
}
fn effective(ui: &Ui, doc: &Document, cell: u64) -> Value {
    let expanded =
        oib::instances::expanded_seed(seed(ui), &doc.oib_allocations().unwrap()).unwrap();
    oib::effective(&expanded, &doc.oib_values(2).unwrap(), cell).unwrap()
}
fn address(instance: u64, references: &[u64], property: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: references.iter().copied().map(NodeId).collect(),
            node: NodeId(1),
            property: PropertyId(property),
        },
    }
}
fn assert_bounds(ui: &Ui, doc: &Document) {
    let allocations = doc.oib_allocations().unwrap();
    let draft = doc.oib_live(1).unwrap();
    let active = doc.oib_live(2).unwrap();
    assert!(allocations.len() <= MAX_ALLOCATIONS);
    assert!(draft.len() <= 6 && active.len() <= 6);
    assert_eq!(
        allocations.iter().map(|a| a.id).collect::<BTreeSet<_>>(),
        draft.union(&active).copied().collect()
    );
    let dynamic: Vec<_> = allocations.iter().filter(|a| a.id != a.prototype).collect();
    assert!(dynamic.iter().all(|a| a.slot < 8));
    assert_eq!(
        dynamic.len(),
        dynamic
            .iter()
            .map(|a| a.slot)
            .collect::<BTreeSet<_>>()
            .len()
    );
    let expanded = oib::instances::expanded_seed(seed(ui), &allocations).unwrap();
    assert_eq!(expanded.cells.len(), 6 + 6 * allocations.len());
    assert!(expanded.cells.len() <= MAX_CELLS);
    let snapshot = Snapshot::from_bytes(&doc.save().unwrap()).unwrap();
    assert!(snapshot.subtree(&[4, 1]).unwrap().canonical_bytes() <= MAX_BYTES);
    let (compiled, layout) = doc.oib_compiled(2).unwrap();
    fn nodes(n: &otrios_interfaces::layout::TemplateNode) -> usize {
        1 + n.children.iter().map(nodes).sum::<usize>()
    }
    assert!(nodes(&layout.blueprint().templates[0].root) <= 512);
    assert!(compiled.layout.len() <= 128 * 1024);
    assert!(compiled.handlers.len() <= 128 && compiled.selectors.len() <= 128);
}

#[test]
fn d1_shared_sources_permissions_and_kill_restart_preserve_four_fields_and_other_app() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut f = Fixture::start(&binary);
    let mut a = f.open('a', 41, 0);
    let mut b = f.open('b', 42, 0);
    assert!(a.binding.ui_editable && !b.binding.ui_editable);
    assert_eq!(a.binding.collection, b.binding.collection);
    assert_eq!(a.state.oib_active().unwrap(), b.state.oib_active().unwrap());
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    b.until(|s| VALUES.iter().all(|(id, v)| s.value(*id).unwrap() == *v));
    let gold = Value::Color([218, 165, 32, 255]);
    let orange = Value::Color([255, 136, 85, 255]);
    assert!(a.state.edit_oib(&[(100, Some(gold.clone()))]).unwrap());
    let denied = a.issue();
    b.send(denied.clone());
    let denial = read(&mut b.socket);
    assert!(
        matches!(denial, ServerMessage::Error { ref message, .. } if message.contains("Space Admin")),
        "{denial:?}"
    );
    drop(b);
    let mut b = f.open('b', 42, 0);
    assert_eq!(
        a.state.oib_generation(),
        b.state.oib_generation(),
        "denial did not publish"
    );
    a.send(denied);
    a.settle();
    b.observe(&a);
    a.edit(&[(201, Some(orange.clone()))]);
    let doc = f.durable(&a.binding.collection);
    assert_eq!(effective(&f.ui, &doc, 201), orange);
    assert_eq!(effective(&f.ui, &doc, 301), gold);
    let blue = Value::Color([65, 155, 230, 255]);
    a.edit(&[
        (100, Some(blue.clone())),
        (103, Some(Value::Text("Shared label".into()))),
    ]);
    assert_eq!(
        effective(&f.ui, &f.durable(&a.binding.collection), 201),
        orange,
        "nested override still masks the newly changed forwarded source"
    );
    a.edit(&[(201, None), (203, Some(Value::Text("Temporary".into())))]);
    a.edit(&[(203, None), (304, Some(orange.clone()))]);
    b.observe(&a);
    assert_values(&a.state);
    assert_values(&b.state);
    let before = f.durable(&a.binding.collection);
    for cell in [201, 204, 301] {
        assert_eq!(effective(&f.ui, &before, cell), blue);
    }
    assert_eq!(effective(&f.ui, &before, 304), orange);
    assert_eq!(effective(&f.ui, &before, 203), Value::Text("Volume".into()));
    assert_eq!(
        effective(&f.ui, &before, 303),
        Value::Text("Brightness".into())
    );
    assert_eq!(before.oib_values(1).unwrap(), before.oib_values(2).unwrap());
    let collection = a.binding.collection.clone();
    drop(a);
    drop(b);
    f.rig.stop(); // Child::kill + wait, not graceful shutdown.
    let dead = f.durable(&collection);
    assert_eq!(dead.root_hash(), before.root_hash());
    f.rig.restart(&binary, true, true);
    let a = f.open('a', 41, 0);
    let b = f.open('b', 42, 0);
    assert_eq!(a.state.oib_active().unwrap(), before.oib_values(2).unwrap());
    assert_eq!(a.state.oib_active().unwrap(), b.state.oib_active().unwrap());
    assert_values(&a.state);
    assert_values(&b.state);
    let recovered = f.durable(&collection);
    assert_eq!(
        recovered.oib_values(1).unwrap(),
        before.oib_values(1).unwrap()
    );
    assert_eq!(effective(&f.ui, &recovered, 304), orange);
    assert_eq!(
        effective(&f.ui, &recovered, 203),
        Value::Text("Volume".into())
    );
    f.counter_unchanged();
}

#[test]
fn d1_concurrent_edits_keep_exact_active_graph_and_recover_conflict_before_observed_resolution() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut f = Fixture::start(&binary);
    let mut a = f.open('a', 51, 0);
    let mut c = f.open('a', 53, 0); // Same authenticated Admin, independent writer/receipt.
    let mut b = f.open('b', 52, 0);
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    c.observe(&a);
    b.observe(&a);
    // Both intents capture the same admitted causal baseline before either send.
    assert!(
        a.state
            .edit_oib(&[(102, Some(Value::Number(400.)))])
            .unwrap()
    );
    assert!(
        c.state
            .edit_oib(&[(105, Some(Value::Text("Shared toggle".into())))])
            .unwrap()
    );
    let left = a.issue();
    let right = c.issue();
    a.send(left);
    a.settle();
    c.send(right);
    c.settle();
    a.observe(&c);
    b.observe(&c);
    assert!(a.state.oib_conflicts().is_empty());
    assert_eq!(
        a.state.oib_active().unwrap()[&102],
        Some(Value::Number(400.))
    );
    assert_eq!(
        a.state.oib_active().unwrap()[&105],
        Some(Value::Text("Shared toggle".into()))
    );
    assert!(
        a.state
            .edit_oib(&[(102, Some(Value::Number(408.)))])
            .unwrap()
    );
    assert!(
        c.state
            .edit_oib(&[(102, Some(Value::Number(416.)))])
            .unwrap()
    );
    let left = a.issue();
    let right = c.issue();
    a.send(left);
    a.settle();
    let before = f.durable(&a.binding.collection);
    let active = before.oib_status().unwrap().active_root;
    let (_, layout) = before.oib_compiled(2).unwrap();
    let exact_layout = Layout::encode(layout.blueprint()).unwrap();
    c.send(right);
    c.settle();
    a.observe(&c);
    b.observe(&c);
    assert_eq!(c.state.oib_conflicts(), vec![102]);
    let collection = a.binding.collection.clone();
    let conflicted = f.durable(&collection);
    assert_eq!(conflicted.oib_status().unwrap().active_root, active);
    assert_eq!(
        conflicted.oib_values(2).unwrap(),
        before.oib_values(2).unwrap()
    );
    assert_eq!(
        Layout::encode(conflicted.oib_compiled(2).unwrap().1.blueprint()).unwrap(),
        exact_layout
    );
    assert!(conflicted.oib_values(1).is_err());
    let snapshot = Snapshot::from_bytes(&conflicted.save().unwrap()).unwrap();
    // Persisted OIB property trie: branch, then thirteen base-32 ID digits.
    let mut path = vec![4, 1];
    path.extend((0..13).rev().map(|i| (102_u64 >> (i * 5)) & 31));
    let draft = snapshot.subtree(&path).unwrap();
    let heads: BTreeSet<_> = draft
        .register()
        .unwrap()
        .values()
        .map(|(_, _, v)| v.unwrap().to_vec())
        .collect();
    assert_eq!(
        heads,
        [408., 416.]
            .map(|n| flexbuffers::to_vec(Value::Number(n)).unwrap())
            .into_iter()
            .collect(),
        "exactly two unresolved causal alternatives"
    );
    assert_values(&a.state);
    assert_values(&b.state);
    drop(a);
    drop(b);
    drop(c);
    f.rig.stop();
    assert_eq!(f.durable(&collection).root_hash(), conflicted.root_hash());
    f.rig.restart(&binary, true, true);
    let mut a = f.open('a', 51, 0);
    let mut b = f.open('b', 52, 0);
    assert_eq!(a.state.oib_conflicts(), vec![102]);
    assert_eq!(a.state.oib_active().unwrap(), before.oib_values(2).unwrap());
    let recovered = f.durable(&collection);
    let recovered_snapshot = Snapshot::from_bytes(&recovered.save().unwrap()).unwrap();
    assert_eq!(
        recovered_snapshot.subtree(&[4]).unwrap().hash(),
        snapshot.subtree(&[4]).unwrap().hash(),
        "complete active/draft/causal/seed/roster subtree survives kill/restart"
    );
    assert_eq!(recovered.oib_status().unwrap().active_root, active);
    assert_eq!(
        recovered.oib_status().unwrap().source_draft,
        conflicted.oib_status().unwrap().source_draft
    );
    // A fresh intent observes both heads, explicitly resolving to this literal.
    a.edit(&[(102, Some(Value::Number(424.)))]);
    b.observe(&a);
    assert!(b.state.oib_conflicts().is_empty());
    assert_eq!(
        b.state.oib_active().unwrap()[&102],
        Some(Value::Number(424.))
    );
    assert!(a.state.oib_generation().unwrap() > before.oib_status().unwrap().generation);
    assert_values(&a.state);
    assert_values(&b.state);
    // Pin six active roots with a conflict, then replace four roots in draft.
    // The union, rather than either roster alone, must obey the 64-cell cap.
    for _ in 0..4 {
        assert!(a.state.duplicate_oib(20, None).unwrap());
        a.settle();
    }
    let six = f.durable(&collection);
    assert_eq!(six.oib_live(2).unwrap().len(), 6);
    assert_eq!(
        oib::instances::expanded_seed(seed(&f.ui), &six.oib_allocations().unwrap())
            .unwrap()
            .cells
            .len(),
        42
    );
    let removed: Vec<_> = six
        .oib_allocations()
        .unwrap()
        .iter()
        .filter(|a| a.id != a.prototype)
        .map(|a| a.id)
        .collect();
    let mut c = f.open('a', 53, 0);
    assert!(
        a.state
            .edit_oib(&[(102, Some(Value::Number(432.)))])
            .unwrap()
    );
    assert!(
        c.state
            .edit_oib(&[(102, Some(Value::Number(440.)))])
            .unwrap()
    );
    let left = a.issue();
    let right = c.issue();
    a.send(left);
    a.settle();
    c.send(right);
    c.settle();
    a.observe(&c);
    let pinned = f.durable(&collection).oib_status().unwrap().active_root;
    for id in &removed {
        assert!(a.state.remove_oib(*id).unwrap());
        a.settle();
    }
    for _ in 0..3 {
        assert!(a.state.duplicate_oib(20, None).unwrap());
        a.settle();
    }
    let nine = f.durable(&collection);
    assert_eq!(nine.oib_live(2).unwrap().len(), 6);
    assert_eq!(nine.oib_live(1).unwrap().len(), 5);
    assert_eq!(nine.oib_allocations().unwrap().len(), 9);
    assert_eq!(
        oib::instances::expanded_seed(seed(&f.ui), &nine.oib_allocations().unwrap())
            .unwrap()
            .cells
            .len(),
        60
    );
    assert_eq!(nine.oib_status().unwrap().active_root, pinned);
    assert!(
        !a.state.duplicate_oib(20, None).unwrap(),
        "client cell-union preflight"
    );
    // Exercise the authority preflight too, bypassing only the adapter's local
    // availability UI. The requested root is still a signed admitted factory.
    assert!(
        a.state
            .edit_oib(&[(102, Some(Value::Number(400.)))])
            .unwrap()
    );
    let StateRequest::OibWrite { sequence, .. } = canvas::decode_state_request(&a.issue()).unwrap()
    else {
        panic!("expected next writer sequence");
    };
    a.send(
        canvas::encode_state_request(&StateRequest::OibInstance {
            sequence,
            operation: otrios_interfaces::oib::InstanceOperation::Duplicate {
                instance: 20,
                name: None,
            },
        })
        .unwrap(),
    );
    let response = read(&mut a.socket);
    let ServerMessage::Error { message, .. } = response else {
        panic!("tenth resident: {response:?}");
    };
    assert_eq!(message, "OIB resident property capacity exhausted");
    assert_eq!(
        f.durable(&collection).root_hash(),
        nine.root_hash(),
        "rejection cannot consume identity or mutate draft"
    );
    c.observe(&a);
    c.edit(&[(102, Some(Value::Number(400.)))]);
    let reclaimed = f.durable(&collection);
    assert_eq!(reclaimed.oib_live(2).unwrap().len(), 5);
    assert_eq!(reclaimed.oib_allocations().unwrap().len(), 5);
    assert!(
        removed
            .iter()
            .all(|id| !reclaimed.oib_live(2).unwrap().contains(id))
    );
    assert_values(&c.state);
    f.counter_unchanged();
}

#[test]
fn d1_lost_reply_receipt_reclamation_and_retired_path_survive_real_authority_restart() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut f = Fixture::start(&binary);
    let mut a = f.open('a', 61, 0);
    let mut b = f.open('b', 62, 0);
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    b.observe(&a);
    assert!(
        a.state
            .duplicate_oib(30, Some("pending-display".into()))
            .unwrap()
    );
    let request = a.issue();
    a.send(request.clone());
    b.until(|s| s.oib_instances().unwrap().len() == 3);
    let created = b
        .state
        .oib_instances()
        .unwrap()
        .iter()
        .map(|i| i.id)
        .max()
        .unwrap();
    let collection = b.binding.collection.clone();
    let actor = a.binding.actor;
    assert!(
        !a.state.confirmed(),
        "writer deliberately never consumes reply"
    );
    let pending = a.state;
    drop(a.socket);
    drop(b);
    f.rig.stop();
    let dead = f.durable(&collection);
    assert!(
        dead.oib_allocations()
            .unwrap()
            .iter()
            .any(|a| a.id == created)
    );
    f.rig.restart(&binary, true, true);
    let mut a = f.open('a', 61, pending.resume_sequence());
    assert_eq!(a.binding.actor, actor, "same admitted receipt identity");
    a.state.restore_pending(&pending).unwrap();
    a.settle();
    assert_eq!(
        a.state.oib_instances().unwrap().len(),
        3,
        "same writer/receipt never allocates twice"
    );
    a.send(request); // Exact replay after a lost reply and kill/restart.
    assert!(a.state.set(&[(1, 482)]).unwrap());
    a.settle();
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    assert_eq!(a.state.oib_instances().unwrap().len(), 3);
    let roots = f.durable(&collection).oib_allocations().unwrap();
    let retired_slot = roots.iter().find(|a| a.id == created).unwrap().slot;
    let mut delayed = f.open('a', 63, 0);
    assert!(
        delayed
            .state
            .edit_oib_property(
                &address(created, &[7], 3),
                Some(Value::Text("Retired edit".into()))
            )
            .unwrap()
    );
    let stale = delayed.issue();
    assert!(a.state.remove_oib(created).unwrap());
    a.settle();
    assert!(a.state.duplicate_oib(30, None).unwrap());
    a.settle();
    let fresh = f.durable(&collection);
    let replacement = fresh
        .oib_allocations()
        .unwrap()
        .into_iter()
        .max_by_key(|a| a.id)
        .unwrap();
    assert!(replacement.id > created);
    assert_eq!(replacement.slot, retired_slot);
    let active = fresh.oib_status().unwrap().active_root;
    delayed.send(stale);
    delayed.settle();
    a.observe(&delayed);
    let after = f.durable(&collection);
    assert_eq!(after.oib_status().unwrap().active_root, active);
    assert_eq!(after.oib_values(2).unwrap(), fresh.oib_values(2).unwrap());
    assert!(
        oib::document::cell_for_address(
            seed(&f.ui),
            &after.oib_allocations().unwrap(),
            &address(created, &[7], 3)
        )
        .is_err()
    );
    drop(delayed);
    // More than the former ten-descriptor lifetime bound; the same renderer
    // slot is reused while authority IDs and sparse property keys stay fresh.
    let mut id = replacement.id;
    let baseline = a.state.state_usage();
    let authority_baseline = after.save().unwrap().len();
    for cycle in 0..24 {
        assert!(a.state.remove_oib(id).unwrap());
        a.settle();
        assert_eq!(a.state.oib_instances().unwrap().len(), 2);
        assert!(a.state.duplicate_oib(30, None).unwrap());
        a.settle();
        let doc = f.durable(&collection);
        let allocations = doc.oib_allocations().unwrap();
        assert_eq!(allocations.len(), 3);
        let new = allocations.iter().max_by_key(|a| a.id).unwrap();
        assert!(new.id > id, "cycle {cycle}");
        assert_eq!(new.slot, retired_slot);
        let cell =
            oib::document::cell_for_address(seed(&f.ui), &allocations, &address(new.id, &[7], 3))
                .unwrap();
        assert_eq!(cell, 0x100_0000 + (new.id - 0x1_0000) * 64 + 28);
        assert_eq!(
            effective(&f.ui, &doc, cell),
            Value::Text("Brightness".into())
        );
        assert!(a.state.state_usage().selected_nodes <= baseline.selected_nodes + 64);
        assert!(
            a.state.state_usage().selected_canonical_bytes
                <= baseline.selected_canonical_bytes + 4096
        );
        assert!(
            doc.save().unwrap().len() <= authority_baseline + 4096,
            "authority checkpoint retention stays bounded after reclamation"
        );
        assert_values(&a.state);
        id = new.id;
    }
    let before = f.durable(&collection);
    drop(a);
    f.rig.stop();
    f.rig.restart(&binary, true, true);
    let a = f.open('a', 61, 0);
    assert_values(&a.state);
    assert_eq!(a.state.oib_active().unwrap(), before.oib_values(2).unwrap());
    assert_eq!(
        f.durable(&collection).oib_allocations().unwrap(),
        before.oib_allocations().unwrap()
    );
    f.counter_unchanged();
}
