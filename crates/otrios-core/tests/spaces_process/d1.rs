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
#[path = "../support/oib_library_fixture.rs"]
mod library_fixture;

const STATE: &str = "app:oib-designer/state";
const VALUES: [(u16, u16); 4] = [(1, 481), (2, 729), (3, 0), (4, 1000)];

const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);

fn loopback_stream(socket: &mut Socket) -> &mut TcpStream {
    let MaybeTlsStream::Plain(stream) = socket.get_mut() else {
        panic!("D1 fixtures require loopback TCP");
    };
    stream
}

fn read_before(
    socket: &mut Socket,
    deadline: Instant,
) -> Result<ServerMessage, tungstenite::Error> {
    let previous_timeout = loopback_stream(socket).read_timeout().map_err(|error| {
        std::io::Error::new(error.kind(), format!("reading D1 timeout: {error}"))
    })?;
    let response = (|| loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "D1 observation deadline",
            )
            .into());
        }
        // Keep the socket timeout practical at the observation boundary.
        loopback_stream(socket)
            .set_read_timeout(Some(remaining.max(Duration::from_millis(1))))
            .map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!("setting D1 observation timeout: {error}"),
                )
            })?;
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                return Ok(canvas::decode_server(bytes.strip_prefix(&[1]).unwrap()).unwrap());
            }
            Ok(Message::Ping(_)) => socket.flush()?,
            Ok(_) => (),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    })();
    loopback_stream(socket)
        .set_read_timeout(previous_timeout)
        .map_err(|error| {
            std::io::Error::new(error.kind(), format!("restoring D1 timeout: {error}"))
        })?;
    response
}

#[track_caller]
fn read_d1(socket: &mut Socket) -> ServerMessage {
    read_before(socket, Instant::now() + OBSERVATION_TIMEOUT)
        .expect("D1 response before observation deadline")
}

#[test]
fn d1_reader_uses_observation_deadline_and_restores_socket_timeout() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let configured_timeout = stream.read_timeout().unwrap();
    let mut socket = tungstenite::WebSocket::from_raw_socket(
        MaybeTlsStream::Plain(stream),
        tungstenite::protocol::Role::Client,
        None,
    );
    let (peer, _) = listener.accept().unwrap();
    let (start, begin) = std::sync::mpsc::sync_channel(1);
    let sender = std::thread::spawn(move || {
        let mut peer = tungstenite::WebSocket::from_raw_socket(
            peer,
            tungstenite::protocol::Role::Server,
            None,
        );
        begin.recv().unwrap();
        std::thread::sleep(Duration::from_millis(80));
        let mut bytes = vec![1];
        bytes.extend(canvas::encode_server(&ServerMessage::Activated { epoch: 17 }).unwrap());
        peer.send(Message::Binary(bytes.into())).unwrap();
    });
    start.send(()).unwrap();
    assert!(matches!(
        read_before(&mut socket, Instant::now() + OBSERVATION_TIMEOUT).unwrap(),
        ServerMessage::Activated { epoch: 17 }
    ));
    assert_eq!(
        loopback_stream(&mut socket).read_timeout().unwrap(),
        configured_timeout
    );
    sender.join().unwrap();
}

#[test]
fn d1_reader_still_rejects_a_missing_response_at_the_deadline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let configured_timeout = stream.read_timeout().unwrap();
    let mut socket = tungstenite::WebSocket::from_raw_socket(
        MaybeTlsStream::Plain(stream),
        tungstenite::protocol::Role::Client,
        None,
    );
    let (_peer, _) = listener.accept().unwrap();
    let deadline = Instant::now() + Duration::from_millis(40);
    assert!(
        matches!(read_before(&mut socket, deadline), Err(tungstenite::Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
    );
    assert!(Instant::now() >= deadline);
    assert_eq!(
        loopback_stream(&mut socket).read_timeout().unwrap(),
        configured_timeout
    );
}

struct Fixture {
    rig: Rig,
    space: String,
    ui: Ui,
    counter: String,
    counter_bytes: Vec<u8>,
}
impl Fixture {
    fn start(binary: &str) -> Self {
        Self::start_ui(binary, app::ui(&[]))
    }
    fn start_ui(binary: &str, input: Ui) -> Self {
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
        let ui = canvas::decode_ui(&canvas::encode_ui(&input).unwrap()).unwrap();
        let manifest = app::manifest();
        assert_eq!(ui.ui_ref, "app:oib-designer/ui");
        assert_eq!(ui.state_schema.id, "oib-designer-state-v2");
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
        let response = read_d1(&mut socket);
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
            read_d1(&mut socket),
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
        // Pin the already admitted source before replay. This verifies the
        // checkpoint without reseeding and keeps replay validation from
        // rebuilding the canonical compiler input on every journal record.
        let checkpoint_root = doc.root_hash();
        doc.prepare_oib_replay(&self.ui).unwrap();
        assert_eq!(
            doc.root_hash(),
            checkpoint_root,
            "transient replay admission leaves checkpoint root untouched"
        );
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
    #[track_caller]
    fn until(&mut self, predicate: impl Fn(&StateAdapter) -> bool) {
        let deadline = Instant::now() + OBSERVATION_TIMEOUT;
        for _ in 0..256 {
            if predicate(&self.state) {
                return;
            }
            assert!(Instant::now() < deadline, "D1 observation deadline");
            let message = read_before(&mut self.socket, deadline)
                .expect("D1 response before observation deadline");
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
    #[track_caller]
    fn settle(&mut self) {
        self.flush();
        self.until(StateAdapter::confirmed);
    }
    fn edit_addresses(&mut self, changes: &[(PropertyAddress, Option<Value>)]) {
        let changes: Vec<_> = changes
            .iter()
            .map(|(address, value)| {
                (
                    self.state.oib_property_register(address).unwrap().cell,
                    value.clone(),
                )
            })
            .collect();
        self.edit(&changes);
    }
    fn edit(&mut self, changes: &[(u64, Option<Value>)]) {
        assert!(self.state.edit_oib(changes).unwrap());
        self.settle();
    }
    #[track_caller]
    fn denied(&mut self) -> String {
        // Permission rejection can follow an already queued publication/receipt
        // frame. Drain ordinary updates before inspecting the denial itself.
        for _ in 0..16 {
            match read_d1(&mut self.socket) {
                ServerMessage::Sync { bytes, .. } => {
                    self.state.receive(&bytes).unwrap();
                    self.flush();
                }
                ServerMessage::Error { message, .. } => return message,
                other => panic!("expected permission denial: {other:?}"),
            }
        }
        panic!("permission denial missing");
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
    let dynamic: Vec<_> = allocations.iter().collect();
    assert!(
        dynamic
            .iter()
            .all(|a| usize::from(a.slot) < MAX_ALLOCATIONS)
    );
    assert_eq!(
        dynamic.len(),
        dynamic
            .iter()
            .map(|a| a.slot)
            .collect::<BTreeSet<_>>()
            .len()
    );
    let expanded = oib::instances::expanded_seed(seed(ui), &allocations).unwrap();
    let expected_cells = seed(ui)
        .cells
        .iter()
        .filter(|c| c.inherit.is_none())
        .count()
        + allocations
            .iter()
            .map(|a| {
                seed(ui)
                    .designer
                    .as_ref()
                    .unwrap()
                    .mappings
                    .roots
                    .iter()
                    .find(|r| r.prototype.0 == a.prototype)
                    .unwrap()
                    .cells
                    .len()
            })
            .sum::<usize>();
    assert_eq!(expanded.cells.len(), expected_cells);
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
    let panel_accent = a
        .state
        .oib_property_register(&PropertyAddress::Definition {
            definition: DefinitionId(10),
            node: NodeId(1),
            property: PropertyId(1),
        })
        .unwrap()
        .cell;
    let slider_label = a
        .state
        .oib_property_register(&PropertyAddress::Definition {
            definition: DefinitionId(12),
            node: NodeId(1),
            property: PropertyId(3),
        })
        .unwrap()
        .cell;
    let audio_accent = a
        .state
        .oib_property_register(&address(20, &[7], 1))
        .unwrap()
        .cell;
    let display_accent = a
        .state
        .oib_property_register(&address(30, &[7], 1))
        .unwrap()
        .cell;
    let audio_toggle = a
        .state
        .oib_property_register(&address(20, &[8], 1))
        .unwrap()
        .cell;
    let display_toggle = a
        .state
        .oib_property_register(&address(30, &[8], 1))
        .unwrap()
        .cell;
    let audio_label = a
        .state
        .oib_property_register(&address(20, &[7], 3))
        .unwrap()
        .cell;
    let display_label = a
        .state
        .oib_property_register(&address(30, &[7], 3))
        .unwrap()
        .cell;
    assert!(a.binding.ui_editable && !b.binding.ui_editable);
    assert_eq!(a.binding.collection, b.binding.collection);
    assert_eq!(a.state.oib_active().unwrap(), b.state.oib_active().unwrap());
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    b.until(|s| VALUES.iter().all(|(id, v)| s.value(*id).unwrap() == *v));
    let gold = Value::Color([218, 165, 32, 255]);
    let orange = Value::Color([255, 136, 85, 255]);
    assert!(
        a.state
            .edit_oib(&[(panel_accent, Some(gold.clone()))])
            .unwrap()
    );
    let denied = a.issue();
    b.send(denied.clone());
    assert!(b.denied().contains("Space Admin"));
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
    a.edit(&[(audio_accent, Some(orange.clone()))]);
    let doc = f.durable(&a.binding.collection);
    assert_eq!(effective(&f.ui, &doc, audio_accent), orange);
    assert_eq!(effective(&f.ui, &doc, display_accent), gold);
    let blue = Value::Color([65, 155, 230, 255]);
    a.edit(&[
        (panel_accent, Some(blue.clone())),
        (slider_label, Some(Value::Text("Shared label".into()))),
    ]);
    assert_eq!(
        effective(&f.ui, &f.durable(&a.binding.collection), audio_accent),
        orange,
        "nested override still masks the newly changed forwarded source"
    );
    a.edit(&[
        (audio_accent, None),
        (audio_label, Some(Value::Text("Temporary".into()))),
    ]);
    a.edit(&[(audio_label, None), (display_toggle, Some(orange.clone()))]);
    b.observe(&a);
    assert_values(&a.state);
    assert_values(&b.state);
    let before = f.durable(&a.binding.collection);
    for cell in [audio_accent, audio_toggle, display_accent] {
        assert_eq!(effective(&f.ui, &before, cell), blue);
    }
    assert_eq!(effective(&f.ui, &before, display_toggle), orange);
    assert_eq!(
        effective(&f.ui, &before, audio_label),
        Value::Text("Volume".into())
    );
    assert_eq!(
        effective(&f.ui, &before, display_label),
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
    assert_eq!(effective(&f.ui, &recovered, display_toggle), orange);
    assert_eq!(
        effective(&f.ui, &recovered, audio_label),
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
    let shared_width = a
        .state
        .oib_property_register(&PropertyAddress::Definition {
            definition: DefinitionId(12),
            node: NodeId(1),
            property: PropertyId(2),
        })
        .unwrap()
        .cell;
    let toggle_label = a
        .state
        .oib_property_register(&PropertyAddress::Definition {
            definition: DefinitionId(13),
            node: NodeId(1),
            property: PropertyId(3),
        })
        .unwrap()
        .cell;
    let mut c = f.open('a', 53, 0); // Same authenticated Admin, independent writer/receipt.
    let mut b = f.open('b', 52, 0);
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    c.observe(&a);
    b.observe(&a);
    // Both intents capture the same admitted causal baseline before either send.
    assert!(
        a.state
            .edit_oib(&[(shared_width, Some(Value::Number(400.)))])
            .unwrap()
    );
    assert!(
        c.state
            .edit_oib(&[(toggle_label, Some(Value::Text("Shared toggle".into())))])
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
        a.state.oib_active().unwrap()[&shared_width],
        Some(Value::Number(400.))
    );
    assert_eq!(
        a.state.oib_active().unwrap()[&toggle_label],
        Some(Value::Text("Shared toggle".into()))
    );
    assert!(
        a.state
            .edit_oib(&[(shared_width, Some(Value::Number(408.)))])
            .unwrap()
    );
    assert!(
        c.state
            .edit_oib(&[(shared_width, Some(Value::Number(416.)))])
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
    assert_eq!(c.state.oib_conflicts(), vec![shared_width]);
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
    let draft = conflicted.oib_property_leaf(1, shared_width).unwrap();
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
    assert_eq!(a.state.oib_conflicts(), vec![shared_width]);
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
    a.edit(&[(shared_width, Some(Value::Number(424.)))]);
    b.observe(&a);
    assert!(b.state.oib_conflicts().is_empty());
    assert_eq!(
        b.state.oib_active().unwrap()[&shared_width],
        Some(Value::Number(424.))
    );
    assert!(a.state.oib_generation().unwrap() > before.oib_status().unwrap().generation);
    assert_values(&a.state);
    assert_values(&b.state);
    // Structural authoring never publishes an invalid pending graph. A
    // property conflict pins the complete last coherent active presentation.
    for _ in 0..4 {
        assert!(a.state.duplicate_oib(20, None).unwrap());
        a.settle();
    }
    let six = f.durable(&collection);
    assert_eq!(six.oib_live(2).unwrap().len(), 6);
    let removed: Vec<_> = six
        .oib_allocations()
        .unwrap()
        .into_iter()
        .filter(|allocation| allocation.id != 20 && allocation.id != 30)
        .map(|allocation| allocation.id)
        .collect();
    let mut c = f.open('a', 53, 0);
    assert!(
        a.state
            .edit_oib(&[(shared_width, Some(Value::Number(432.)))])
            .unwrap()
    );
    assert!(
        c.state
            .edit_oib(&[(shared_width, Some(Value::Number(440.)))])
            .unwrap()
    );
    let left = a.issue();
    let right = c.issue();
    a.send(left);
    a.settle();
    c.send(right);
    c.settle();
    a.observe(&c);
    let pinned = f.durable(&collection);
    let source_bytes = |document: &Document| {
        Snapshot::from_bytes(&document.save().unwrap())
            .unwrap()
            .subtree(&[4, 4])
            .unwrap()
            .to_bytes()
            .unwrap()
    };
    assert!(a.state.remove_oib(removed[0]).unwrap());
    let rejected = a.issue();
    a.send(rejected.clone());
    a.settle();
    let rejected_state = f.durable(&collection);
    assert!(
        rejected_state
            .oib_status()
            .unwrap()
            .error
            .as_ref()
            .is_some_and(|error| error.contains("Resolve the UI draft"))
    );
    assert_eq!(
        source_bytes(&rejected_state),
        source_bytes(&pinned),
        "rejected topology consumes no identity or source edit"
    );
    assert_eq!(
        rejected_state.oib_status().unwrap().active_root,
        pinned.oib_status().unwrap().active_root
    );
    assert_eq!(
        rejected_state.oib_values(2).unwrap(),
        pinned.oib_values(2).unwrap()
    );
    a.send(rejected); // Exact rejection replay is also durably acknowledged.
    c.observe(&a);
    c.edit(&[(shared_width, Some(Value::Number(400.)))]);
    a.observe(&c);
    assert!(a.state.oib_conflicts().is_empty());
    assert_eq!(a.state.oib_instances().unwrap().len(), 6);
    assert!(
        !a.state.duplicate_oib(20, None).unwrap(),
        "coherent live-root capacity preflight"
    );
    // A mixed request bypasses the capacity-only convenience preflight, so the
    // real authority must reject its final seven-root candidate atomically.
    use otrios_interfaces::oib::authoring::{
        DefinitionEdit, Destination, DocumentEdit, EditBatch, HierarchyEdit,
    };
    let current = f.durable(&collection);
    assert!(
        a.state
            .edit_oib_structure(EditBatch {
                generation: a.state.oib_structural_generation().unwrap(),
                operations: vec![
                    DocumentEdit::Definition(DefinitionEdit::RenameDefinition {
                        definition: DefinitionId(10),
                        name: "SettingsPanel".into()
                    }),
                    DocumentEdit::Hierarchy(HierarchyEdit::Duplicate {
                        instance: InstanceId(20),
                        name: None,
                        destination: Destination::Canvas,
                        before: None,
                        frame: None
                    }),
                ]
            })
            .unwrap()
    );
    a.settle();
    let capacity_rejected = f.durable(&collection);
    assert!(capacity_rejected.oib_status().unwrap().error.is_some());
    assert_eq!(source_bytes(&capacity_rejected), source_bytes(&current));
    assert_eq!(
        capacity_rejected.oib_status().unwrap().active_root,
        current.oib_status().unwrap().active_root
    );
    for id in removed {
        assert!(a.state.remove_oib(id).unwrap());
        a.settle();
    }
    let reclaimed = f.durable(&collection);
    assert_eq!(reclaimed.oib_live(2).unwrap().len(), 2);
    assert_eq!(reclaimed.oib_allocations().unwrap().len(), 2);
    assert_values(&a.state);
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
    let checkpoint_path = f
        .rig
        .root
        .join(format!("state/collection-{collection}.merkle"));
    let initial_checkpoint = Snapshot::from_bytes(&fs::read(&checkpoint_path).unwrap())
        .unwrap()
        .hash();

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
        assert_eq!(
            oib::document::address_for_cell(
                oib::seed(&doc.oib_compiled(2).unwrap().0).unwrap(),
                cell
            )
            .unwrap(),
            address(new.id, &[7], 3)
        );
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

    let retained = fs::metadata(checkpoint_path.with_extension("merkle.log"))
        .unwrap()
        .len();
    assert!(
        retained < 512 * 1024,
        "bounded OIB fixture retains {retained} journal bytes after rotation"
    );
    assert_ne!(
        Snapshot::from_bytes(&fs::read(&checkpoint_path).unwrap())
            .unwrap()
            .hash(),
        initial_checkpoint,
        "OIB checkpoint advances during repeated structural churn"
    );
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

#[test]
fn d2_canvas_order_stale_receipts_rebase_and_restart_preserve_overrides_and_values() {
    use otrios_interfaces::oib::authoring::EditBatch as InstanceOperation;
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut f = Fixture::start(&binary);
    let mut a = f.open('a', 71, 0);
    let mut b = f.open('a', 72, 0);
    let mut observer = f.open('a', 73, 0);
    assert!(a.binding.ui_editable && b.binding.ui_editable);
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    a.edit_addresses(&[
        (
            address(20, &[7], 3),
            Some(Value::Text("Audio custom label".into())),
        ),
        (
            address(30, &[8], 1),
            Some(Value::Color([119, 51, 204, 255])),
        ),
    ]);
    b.observe(&a);
    observer.observe(&a);
    let collection = a.binding.collection.clone();
    let original = f.durable(&collection);

    let original_allocations = original.oib_allocations().unwrap();
    let generation = a.state.oib_structural_generation().unwrap();
    let ids = |state: &StateAdapter| -> Vec<u64> {
        state
            .oib_instances()
            .unwrap()
            .iter()
            .map(|i| i.id)
            .collect()
    };
    assert_eq!(ids(&a.state), vec![20, 30]);

    // Capture both requests before either authority edit. The first writer loses
    // its reply; the second deliberately submits the now-stale generation.
    assert!(a.state.reorder_oib(30, Some(20)).unwrap());
    let accepted = a.issue();
    assert!(
        b.state
            .edit_oib_structure(InstanceOperation::insert(generation, 30, Some(20), None))
            .unwrap()
    );
    let rejected = b.issue();
    a.send(accepted.clone());
    observer
        .until(|s| s.oib_structural_generation() == Some(generation + 1) && ids(s) == vec![30, 20]);
    b.send(rejected.clone());
    observer.until(|s| s.oib_status_text().contains("Structure rejected"));
    assert!(
        !a.state.confirmed() && !b.state.confirmed(),
        "both reply losses are intentional"
    );
    let before_kill = f.durable(&collection);
    assert_eq!(
        before_kill.oib_status().unwrap().structural_generation,
        generation + 1
    );
    assert_eq!(
        before_kill.oib_values(2).unwrap(),
        original.oib_values(2).unwrap()
    );
    assert_eq!(
        before_kill.oib_allocations().unwrap(),
        vec![
            original_allocations[1].clone(),
            original_allocations[0].clone()
        ]
    );
    let a_actor = a.binding.actor;
    let b_actor = b.binding.actor;
    let pending_a = a.state;
    let pending_b = b.state;
    drop(a.socket);
    drop(b.socket);
    drop(observer);
    f.rig.stop(); // Real authority process kill, including both durable receipts.
    assert_eq!(f.durable(&collection).root_hash(), before_kill.root_hash());
    f.rig.restart(&binary, true, true);
    let mut a = f.open('a', 71, pending_a.resume_sequence());
    let mut b = f.open('a', 72, pending_b.resume_sequence());
    assert_eq!(a.binding.actor, a_actor);
    assert_eq!(b.binding.actor, b_actor);
    a.state.restore_pending(&pending_a).unwrap();
    b.state.restore_pending(&pending_b).unwrap();
    a.settle();
    b.settle();
    assert_eq!(ids(&a.state), vec![30, 20]);
    assert_eq!(ids(&b.state), vec![30, 20]);
    assert_eq!(a.state.oib_structural_generation(), Some(generation + 1));
    assert_eq!(b.state.oib_structural_generation(), Some(generation + 1));
    assert!(b.state.oib_status_text().contains("Structure rejected"));
    // Exact explicit replays after recovery remain idempotent. A subsequent
    // ordinary edit also proves neither stale receipt stranded its writer.
    a.send(accepted);
    b.send(rejected);
    assert!(b.state.set(&[(1, VALUES[0].1 + 1)]).unwrap());
    b.settle();
    assert!(b.state.set(&VALUES).unwrap());
    b.settle();
    assert_eq!(ids(&b.state), vec![30, 20]);

    // Deliberate rebase uses the fresh confirmed order. Exact Display prototype
    // preserves its Brightness authored assignment, rather than selecting Audio
    // because both factories happen to share the SettingsPanel definition.
    assert!(b.state.insert_oib(30, Some(20)).unwrap());
    b.settle();
    let inserted = b
        .state
        .oib_instances()
        .unwrap()
        .iter()
        .find(|i| i.id != 20 && i.id != 30)
        .unwrap()
        .clone();
    let source = Snapshot::from_bytes(&original.save().unwrap())
        .unwrap()
        .subtree(&[4, 4])
        .unwrap();
    let source: otrios_interfaces::oib::authoring::DocumentSource = flexbuffers::from_slice(
        source
            .register()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .2
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        inserted.id, source.next_identity,
        "stale insert consumed no identity"
    );
    assert_eq!(inserted.prototype, 30);
    assert_eq!(ids(&b.state), vec![30, inserted.id, 20]);
    a.observe(&b);
    assert!(a.state.reorder_oib(20, Some(30)).unwrap());
    a.settle();
    b.observe(&a);
    assert_eq!(ids(&a.state), vec![20, 30, inserted.id]);
    assert!(
        !a.state
            .oib_instances()
            .unwrap()
            .iter()
            .any(|i| i.prototype == 50),
        "Power starts in the palette only"
    );
    assert!(a.state.insert_oib(50, Some(30)).unwrap());
    a.settle();
    b.observe(&a);
    let power = a
        .state
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|i| i.prototype == 50)
        .unwrap();
    assert_eq!(power.id, inserted.id + 1);
    let expected_order = vec![20, power.id, 30, inserted.id];
    assert_eq!(ids(&a.state), expected_order);
    let completed = f.durable(&collection);
    for branch in [1, 2] {
        for cell in original.oib_values(branch).unwrap().keys() {
            assert_eq!(
                completed
                    .oib_property_leaf(branch, *cell)
                    .unwrap()
                    .to_bytes()
                    .unwrap(),
                original
                    .oib_property_leaf(branch, *cell)
                    .unwrap()
                    .to_bytes()
                    .unwrap(),
                "surviving causal property register {branch}/{cell}"
            );
        }
    }
    let label = oib::document::cell_for_address(
        seed(&f.ui),
        &completed.oib_allocations().unwrap(),
        &address(inserted.id, &[7], 3),
    )
    .unwrap();
    assert_eq!(
        effective(&f.ui, &completed, label),
        Value::Text("Brightness".into())
    );
    let power_label = oib::document::cell_for_address(
        seed(&f.ui),
        &completed.oib_allocations().unwrap(),
        &address(power.id, &[], 3),
    )
    .unwrap();
    assert_eq!(
        effective(&f.ui, &completed, power_label),
        Value::Text("Power".into())
    );
    let (compiled, layout) = completed.oib_compiled(2).unwrap();
    let expanded = oib::seed(&compiled).unwrap();
    let power_node = expanded
        .designer
        .as_ref()
        .unwrap()
        .mappings
        .objects
        .iter()
        .find(|m| {
            m.address.instance.0 == power.id
                && m.address.references.is_empty()
                && m.address.node == NodeId(1)
        })
        .unwrap()
        .layout_node
        .unwrap();
    fn find_node(
        root: &otrios_interfaces::layout::TemplateNode,
        id: u16,
    ) -> Option<&otrios_interfaces::layout::TemplateNode> {
        if root.id == id {
            Some(root)
        } else {
            root.children.iter().find_map(|n| find_node(n, id))
        }
    }
    let node = layout
        .blueprint()
        .templates
        .iter()
        .find_map(|t| find_node(&t.root, power_node))
        .unwrap();
    let Some(otrios_interfaces::layout::Responder::AppEvent { control, .. }) = &node.responder
    else {
        panic!("Power has its signed toggle responder")
    };
    let handler = compiled
        .handlers
        .iter()
        .find(|h| h.control == *control)
        .unwrap();
    assert!(matches!(handler.operation, Operation::Toggle));
    assert_eq!(
        handler.fields,
        vec![3],
        "Power keeps its signed audio-enabled binding"
    );
    assert_values(&a.state);
    assert_values(&b.state);
    assert!(
        completed.oib_status().unwrap().error.is_none(),
        "successful rebase clears rejection"
    );
    drop(a);
    drop(b);
    f.rig.stop();
    f.rig.restart(&binary, true, true);
    let a = f.open('a', 71, 0);
    assert_eq!(ids(&a.state), expected_order);
    assert_values(&a.state);
    assert_eq!(
        a.state.oib_active().unwrap(),
        completed.oib_values(2).unwrap()
    );
    f.counter_unchanged();
}

#[test]
fn d2_definition_race_lost_reply_restart_and_readonly_projection_preserve_values() {
    use otrios_interfaces::oib::authoring::{
        DefinitionBatch, DefinitionEdit, EditBatch as InstanceOperation,
    };
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    let mut f = Fixture::start(&binary);
    let mut a = f.open('a', 84, 0);
    let mut b = f.open('a', 85, 0);
    let mut viewer = f.open('b', 86, 0);
    assert!(!viewer.binding.ui_editable);
    assert!(a.state.set(&VALUES).unwrap());
    a.settle();
    a.edit_addresses(&[(
        address(20, &[7], 3),
        Some(Value::Text("Retained source".into())),
    )]);
    b.observe(&a);
    viewer.observe(&a);
    let collection = a.binding.collection.clone();
    let original = f.durable(&collection);
    let original_tree = Snapshot::from_bytes(&original.save().unwrap()).unwrap();
    let generation = a.state.oib_structural_generation().unwrap();
    let edit = |before| {
        InstanceOperation::definitions(DefinitionBatch {
            generation,
            operations: vec![DefinitionEdit::MoveNode {
                definition: DefinitionId(10),
                node: NodeId(8),
                owner: NodeId(1),
                visual_parent: Some(NodeId(1)),
                before: Some(NodeId(before)),
            }],
        })
    };
    assert!(a.state.edit_oib_structure(edit(7)).unwrap());
    assert!(b.state.edit_oib_structure(edit(9)).unwrap());
    let accepted = a.issue();
    let rejected = b.issue();
    a.send(accepted.clone());
    viewer.until(|s| s.oib_structural_generation() == Some(generation + 1));
    b.send(rejected.clone());
    viewer.until(|s| s.oib_status_text().contains("Structure rejected"));
    let persisted = f.durable(&collection);
    let compiled = persisted.oib_compiled(2).unwrap().0;
    let definition = oib::seed(&compiled)
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    assert_eq!(
        definition.nodes[0].visual_children,
        [2, 3, 8, 7, 9, 12].map(NodeId)
    );
    let tree = Snapshot::from_bytes(&persisted.save().unwrap()).unwrap();
    for branch in [1, 2, 5, 6] {
        assert_eq!(
            original_tree
                .subtree(&[4, branch])
                .unwrap()
                .to_bytes()
                .unwrap(),
            tree.subtree(&[4, branch]).unwrap().to_bytes().unwrap()
        );
    }
    assert_eq!(
        original_tree.subtree(&[1]).unwrap().hash(),
        tree.subtree(&[1]).unwrap().hash()
    );
    let pending_a = a.state;
    let pending_b = b.state;
    drop(a.socket);
    drop(b.socket);
    drop(viewer);
    f.rig.stop();
    assert_eq!(f.durable(&collection).root_hash(), persisted.root_hash());
    f.rig.restart(&binary, true, true);
    let mut a = f.open('a', 84, pending_a.resume_sequence());
    let mut b = f.open('a', 85, pending_b.resume_sequence());
    let mut viewer = f.open('b', 86, 0);
    a.state.restore_pending(&pending_a).unwrap();
    b.state.restore_pending(&pending_b).unwrap();
    a.settle();
    b.settle();
    a.send(accepted);
    b.send(rejected);
    assert_eq!(a.state.oib_structural_generation(), Some(generation + 1));
    assert_eq!(b.state.oib_structural_generation(), Some(generation + 1));
    assert!(b.state.oib_status_text().contains("Structure rejected"));
    assert_values(&viewer.state);
    let retry = InstanceOperation::definitions(DefinitionBatch {
        generation: generation + 1,
        operations: vec![DefinitionEdit::MoveNode {
            definition: DefinitionId(10),
            node: NodeId(8),
            owner: NodeId(1),
            visual_parent: Some(NodeId(1)),
            before: Some(NodeId(9)),
        }],
    });
    assert!(b.state.edit_oib_structure(retry).unwrap());
    b.settle();
    viewer.observe(&b);
    assert_eq!(b.state.oib_structural_generation(), Some(generation + 2));
    assert!(!b.state.oib_status_text().contains("Structure rejected"));
    assert_values(&viewer.state);
    f.counter_unchanged();
}

/// Canonical replacements for the former root tests' parallel-renderer seeds.
pub(super) fn canonical_admin_permissions_recover(binary: &str) {
    let mut fixture = Fixture::start(binary);
    let mut admin = fixture.open('a', 91, 0);
    let mut member = fixture.open('b', 92, 0);
    assert!(admin.binding.ui_editable && !member.binding.ui_editable);
    assert!(admin.state.set(&VALUES).unwrap());
    admin.settle();
    member.observe(&admin);
    let property = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(3),
    };
    assert!(
        admin
            .state
            .edit_oib_property(&property, Some(Value::Text("Confirmed live UI".into())))
            .unwrap()
    );
    let request = admin.issue();
    member.send(request.clone());
    assert!(member.denied().contains("Space Admin"));
    drop(member);
    let mut member = fixture.open('b', 92, 0);
    admin.send(request);
    admin.settle();
    member.observe(&admin);
    assert_eq!(
        admin
            .state
            .oib_property_register(&property)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Confirmed live UI".into()))]
    );
    assert_values(&admin.state);
    assert_values(&member.state);
    let active = admin.state.oib_active().unwrap();
    drop(admin);
    drop(member);
    fixture.rig.stop();
    fixture.rig.restart(binary, true, true);
    let recovered = fixture.open('a', 91, 0);
    assert_eq!(recovered.state.oib_active().unwrap(), active);
    assert_eq!(
        recovered
            .state
            .oib_property_register(&property)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Confirmed live UI".into()))]
    );
    assert_values(&recovered.state);
    fixture.counter_unchanged();
}

pub(super) fn canonical_instances_unknown_outcomes_recover(binary: &str) {
    let mut fixture = Fixture::start(binary);
    let mut admin = fixture.open('a', 93, 0);
    let mut member = fixture.open('b', 94, 0);
    assert!(admin.binding.ui_editable && !member.binding.ui_editable);
    assert!(admin.state.set(&VALUES).unwrap());
    admin.settle();
    member.observe(&admin);
    assert!(admin.state.duplicate_oib(30, Some("gain".into())).unwrap());
    let request = admin.issue();
    // A real read-only session cannot submit even an otherwise admitted intent.
    member.send(request.clone());
    assert!(member.denied().contains("Space Admin"));
    drop(member);
    let mut member = fixture.open('b', 94, 0);
    admin.send(request.clone());
    member.until(|state| state.oib_instances().unwrap().len() == 3);
    let created = member
        .state
        .oib_instances()
        .unwrap()
        .into_iter()
        .find(|instance| instance.id != 20 && instance.id != 30)
        .unwrap()
        .id;
    let pending = admin.state;
    drop(admin.socket);
    drop(member);
    fixture.rig.stop();
    fixture.rig.restart(binary, true, true);
    let mut admin = fixture.open('a', 93, pending.resume_sequence());
    admin.state.restore_pending(&pending).unwrap();
    admin.settle();
    assert_eq!(
        admin.state.oib_instances().unwrap().len(),
        3,
        "lost creation reply cannot allocate twice"
    );
    admin.send(request);
    assert!(admin.state.set(&[(1, 632)]).unwrap());
    admin.settle();
    assert_eq!(admin.state.oib_instances().unwrap().len(), 3);
    assert!(admin.state.remove_oib(created).unwrap());
    admin.settle();
    let width = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    let label = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(3),
    };
    let accent = address(20, &[7], 1);
    admin.edit_addresses(&[
        (width.clone(), Some(Value::Number(360.))),
        (label.clone(), Some(Value::Text("Shared label".into()))),
        (accent.clone(), Some(Value::Color([119, 51, 204, 255]))),
    ]);
    drop(admin);
    fixture.rig.stop();
    fixture.rig.restart(binary, true, true);
    let mut admin = fixture.open('a', 93, 0);
    assert_eq!(admin.state.value(1).unwrap(), 632);
    assert_eq!(
        admin
            .state
            .oib_property_register(&width)
            .unwrap()
            .alternatives,
        vec![Some(Value::Number(360.))]
    );
    assert_eq!(
        admin
            .state
            .oib_property_register(&label)
            .unwrap()
            .alternatives,
        vec![Some(Value::Text("Shared label".into()))]
    );
    assert_eq!(
        admin
            .state
            .oib_property_register(&accent)
            .unwrap()
            .alternatives,
        vec![Some(Value::Color([119, 51, 204, 255]))]
    );
    assert!(
        !admin
            .state
            .oib_instances()
            .unwrap()
            .iter()
            .any(|instance| instance.id == created)
    );
    assert!(admin.state.duplicate_oib(30, None).unwrap());
    admin.settle();
    let replacement = admin
        .state
        .oib_instances()
        .unwrap()
        .into_iter()
        .map(|instance| instance.id)
        .max()
        .unwrap();
    assert!(replacement > created);
    assert!(admin.state.restore_oib(false).unwrap());
    admin.settle();
    assert_eq!(
        admin
            .state
            .oib_instances()
            .unwrap()
            .iter()
            .map(|instance| instance.id)
            .collect::<Vec<_>>(),
        vec![20, 30]
    );
    assert_eq!(
        admin
            .state
            .oib_property_register(&width)
            .unwrap()
            .alternatives,
        vec![Some(Value::Number(360.))]
    );
    let generation = admin.state.oib_structural_generation().unwrap();
    assert!(admin.state.restore_oib(true).unwrap());
    let request = admin.issue();
    admin.send(request.clone());
    let mut member = fixture.open('b', 94, 0);
    member.until(|state| state.oib_structural_generation() == Some(generation + 1));
    let pending = admin.state;
    drop(admin.socket);
    drop(member);
    fixture.rig.stop();
    fixture.rig.restart(binary, true, true);
    let mut recovered = fixture.open('a', 93, pending.resume_sequence());
    recovered.state.restore_pending(&pending).unwrap();
    recovered.settle();
    recovered.send(request);
    assert_eq!(recovered.state.value(1).unwrap(), 632);
    assert_eq!(recovered.state.oib_instances().unwrap().len(), 2);
    for property in [width, label, accent] {
        assert_eq!(
            recovered
                .state
                .oib_property_register(&property)
                .unwrap()
                .alternatives,
            vec![None],
            "reset clears overrides to current authored defaults"
        );
    }
    fixture.counter_unchanged();
}

#[test]
fn l1_compatible_library_baseline_upgrade_restart_and_rollback_preserve_collection_and_causal_data()
{
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    // This exercises the real authority/persistence boundary with synthetic
    // already-verified bodies. Signed package/publication acceptance is separate.
    let original = library_fixture::admit(&library_fixture::imported());
    assert_eq!(library_fixture::seed(&original).resolved_libraries.len(), 1);
    let styled = library_fixture::admit(&library_fixture::update(&original, |graph| {
        graph
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(13))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|d| d.property == PropertyId(3))
            .unwrap()
            .value = Value::Text("Library revision two".into());
    }));
    let mut fixture = Fixture::start_ui(&binary, original.clone());
    let mut admin = fixture.open('a', 201, 0);
    assert!(admin.state.set(&VALUES).unwrap());
    admin.settle();
    let width = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    admin.edit_addresses(&[
        (width.clone(), Some(Value::Number(360.))),
        (
            address(20, &[8], 3),
            Some(Value::Text("Retained instance label".into())),
        ),
    ]);
    assert!(
        admin
            .state
            .edit_oib_structure(otrios_interfaces::oib::authoring::EditBatch::definitions(
                otrios_interfaces::oib::authoring::DefinitionBatch {
                    generation: admin.state.oib_structural_generation().unwrap(),
                    operations: vec![
                        otrios_interfaces::oib::authoring::DefinitionEdit::RenameDefinition {
                            definition: DefinitionId(10),
                            name: "Retained local wrapper".into()
                        }
                    ],
                }
            ))
            .unwrap()
    );
    admin.settle();
    let collection = admin.binding.collection.clone();
    let actor = admin.binding.actor;
    let resume = admin.state.resume_sequence();
    let before = fixture.durable(&collection);
    let epoch = before.oib_epoch().unwrap();
    let (generation, lock) = before.oib_baseline().unwrap();
    let properties = Snapshot::from_bytes(&before.save().unwrap()).unwrap();
    drop(admin);
    fn replace(fixture: &mut Fixture, binary: &str, ui: &Ui) {
        fixture.rig.stop();
        let bytes = canvas::encode_ui(ui).unwrap();
        fs::write(
            fixture.rig.root.join("apps/oib-designer/package/ui.flex"),
            &bytes,
        )
        .unwrap();
        let mut catalog = canvas::decode_catalog(&fixture.rig.catalog).unwrap();
        catalog
            .apps
            .iter_mut()
            .find(|entry| entry.app.id == "oib-designer")
            .unwrap()
            .ui_hash = canvas::hash(&bytes);
        fixture.rig.catalog = canvas::encode_catalog(&catalog).unwrap();
        fs::write(
            fixture.rig.root.join("apps/catalog.flex"),
            &fixture.rig.catalog,
        )
        .unwrap();
        fixture.ui = ui.clone();
        fixture.rig.restart(binary, true, true);
    }
    replace(&mut fixture, &binary, &styled);
    let admin = fixture.open('a', 201, resume);
    assert_eq!(admin.binding.collection, collection);
    assert_eq!(admin.binding.actor, actor);
    assert_eq!(admin.state.resume_sequence(), resume);
    assert_values(&admin.state);
    let upgraded = fixture.durable(&collection);
    assert_eq!(upgraded.oib_epoch().unwrap(), epoch);
    assert_eq!(upgraded.oib_baseline().unwrap().0, generation + 1);
    assert_ne!(upgraded.oib_baseline().unwrap().1, lock);
    for branch in [1, 2, 4] {
        assert_eq!(
            Snapshot::from_bytes(&upgraded.save().unwrap())
                .unwrap()
                .subtree(&[4, branch])
                .unwrap()
                .to_bytes()
                .unwrap(),
            properties
                .subtree(&[4, branch])
                .unwrap()
                .to_bytes()
                .unwrap()
        );
    }
    let read = |document: &Document, property: &PropertyAddress| {
        let (ui, _) = document.oib_compiled(2).unwrap();
        let source = oib::seed(&ui).unwrap();
        let cell =
            oib::document::cell_for_address(source, &document.oib_allocations().unwrap(), property)
                .unwrap();
        oib::effective(source, &document.oib_values(2).unwrap(), cell).unwrap()
    };
    assert_eq!(
        read(&upgraded, &address(30, &[8], 3)),
        Value::Text("Library revision two".into())
    );
    assert_eq!(
        read(&upgraded, &address(20, &[8], 3)),
        Value::Text("Retained instance label".into())
    );
    assert_eq!(read(&upgraded, &address(30, &[7], 2)), Value::Number(360.));
    drop(admin);
    fixture.rig.stop();
    fixture.rig.restart(&binary, true, true);
    let admin = fixture.open('a', 201, resume);
    assert_values(&admin.state);
    assert_eq!(
        fixture.durable(&collection).oib_baseline().unwrap(),
        upgraded.oib_baseline().unwrap()
    );
    drop(admin);
    replace(&mut fixture, &binary, &original);
    let admin = fixture.open('a', 201, resume);
    assert_eq!(admin.binding.collection, collection);
    assert_values(&admin.state);
    let rollback = fixture.durable(&collection);
    assert_eq!(rollback.oib_epoch().unwrap(), epoch);
    assert_eq!(rollback.oib_baseline().unwrap().0, generation + 2);
    assert_eq!(
        read(&rollback, &address(30, &[8], 3)),
        Value::Text("Enabled".into())
    );
    assert_eq!(
        read(&rollback, &address(20, &[8], 3)),
        Value::Text("Retained instance label".into())
    );
    fixture.counter_unchanged();
}

#[test]
fn l1_slow_state_adapter_receives_latest_coalesced_library_after_held_a_state_offer() {
    let Ok(binary) = std::env::var("OTRIOS_SPACES_SERVER") else {
        return;
    };
    // Real sockets/authority with synthetic already-verified dependency bodies;
    // signed publication and host deployment are independently qualified.
    let original = library_fixture::admit(&library_fixture::imported());
    let second = library_fixture::admit(&library_fixture::update(&original, |graph| {
        graph
            .definitions
            .iter_mut()
            .find(|definition| definition.id == DefinitionId(13))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|default| default.property == PropertyId(3))
            .unwrap()
            .value = Value::Text("Intermediate library".into());
    }));
    let mut third = library_fixture::update(&second, |graph| {
        graph
            .definitions
            .iter_mut()
            .find(|definition| definition.id == DefinitionId(13))
            .unwrap()
            .nodes[0]
            .defaults
            .iter_mut()
            .find(|default| default.property == PropertyId(3))
            .unwrap()
            .value = Value::Text("Coalesced latest library".into());
    });
    let source = library_fixture::seed_mut(&mut third);
    source.resolved_libraries[0].revision = [3; 32];
    source
        .graph
        .as_mut()
        .unwrap()
        .imports
        .as_mut()
        .unwrap()
        .locks[0]
        .revision = [3; 32];
    let third = library_fixture::admit(&third);
    let mut fixture = Fixture::start_ui(&binary, original);
    let mut admin = fixture.open('a', 221, 0);
    let mut slow = fixture.open('a', 222, 0);
    let generation = slow.state.oib_structural_generation().unwrap();
    let collection = slow.binding.collection.clone();
    let epoch = fixture.durable(&collection).oib_epoch().unwrap();
    assert!(admin.state.set(&[(1, 521)]).unwrap());
    admin.settle();
    let ServerMessage::Sync { bytes: held, .. } = read_d1(&mut slow.socket) else {
        panic!("ordinary A-state offer")
    };
    // Do not deliver or ACK the already offered A snapshot until both revisions
    // and their ordinary writes have reached the actual authority.
    fn hot_ui(fixture: &mut Fixture, ui: &Ui) {
        let bytes = canvas::encode_ui(ui).unwrap();
        fs::write(
            fixture.rig.root.join("apps/oib-designer/package/ui.flex"),
            &bytes,
        )
        .unwrap();
        let mut catalog = canvas::decode_catalog(&fixture.rig.catalog).unwrap();
        catalog
            .apps
            .iter_mut()
            .find(|entry| entry.app.id == "oib-designer")
            .unwrap()
            .ui_hash = canvas::hash(&bytes);
        fixture.rig.catalog = canvas::encode_catalog(&catalog).unwrap();
        fs::write(
            fixture.rig.root.join("apps/catalog.flex"),
            &fixture.rig.catalog,
        )
        .unwrap();
        fixture.ui = ui.clone();
    }
    hot_ui(&mut fixture, &second);
    let mut intermediate = fixture.open('a', 223, 0);
    assert!(intermediate.state.set(&[(2, 729)]).unwrap());
    intermediate.settle();
    let width = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    intermediate.edit_addresses(&[(width.clone(), Some(Value::Number(360.)))]);
    drop(intermediate);
    hot_ui(&mut fixture, &third);
    let mut latest = fixture.open('a', 224, 0);
    assert!(latest.state.set(&[(3, 0)]).unwrap());
    latest.settle();
    assert_eq!(
        latest.state.oib_structural_generation(),
        Some(generation + 2)
    );
    slow.state.receive(&held).unwrap();
    assert_eq!(
        slow.state.oib_structural_generation(),
        Some(generation),
        "held offer pins the complete old baseline"
    );
    slow.flush();
    slow.until(|state| state.oib_structural_generation() == Some(generation + 2));
    assert_eq!(slow.state.value(1).unwrap(), 521);
    assert_eq!(slow.state.value(2).unwrap(), 729);
    assert_eq!(slow.state.value(3).unwrap(), 0);
    assert_eq!(
        slow.state
            .oib_property_register(&width)
            .unwrap()
            .alternatives,
        vec![Some(Value::Number(360.))]
    );
    let label = slow
        .state
        .oib_property_register(&address(30, &[8], 3))
        .unwrap()
        .cell;
    assert!(
        slow.state
            .oib_property_text(label)
            .contains("Coalesced latest library")
    );
    assert_eq!(fixture.durable(&collection).oib_epoch().unwrap(), epoch);
    fixture.counter_unchanged();
}
