// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use automerge::{
    ActorId, AutoCommit, ROOT, ReadDoc, ScalarValue,
    sync::SyncDoc,
    transaction::{CommitOptions, Transactable},
};
use otrios_interfaces::layout::Value;
const MAX_HISTORY_BYTES: usize = MAX_BYTES - 4096;
#[derive(Clone, Default)]
pub struct Session(pub automerge::sync::State);
type SignalCache =
    std::cell::RefCell<std::collections::BTreeMap<u16, std::sync::Arc<[(i64, f32)]>>>;
#[derive(Clone)]
pub struct Document {
    doc: AutoCommit,
    schema: StateSchema,
    seed: automerge::ChangeHash,
    history_bytes: usize,
    wall_time: i64,
    state_ref: String,
    signal_cache: SignalCache,
}
impl Document {
    pub fn new(schema: StateSchema, state_ref: &str) -> Result<Self> {
        validate_schema(&schema)?;
        if state_ref.is_empty() || state_ref.len() > 128 {
            return Err("Invalid State reference".into());
        }
        let mut doc =
            AutoCommit::new().with_actor(ActorId::from(b"otrios-canvas-seed-v2".to_vec()));
        doc.put(ROOT, "schema", encode_schema(&schema)?)
            .map_err(err)?;
        doc.put(ROOT, "scope", state_ref).map_err(err)?;
        for field in &schema.fields {
            doc.put(
                ROOT,
                format!("f{}", field.id),
                ScalarValue::Bytes(encode_value(&field.default)?),
            )
            .map_err(err)?;
        }
        let seed = doc.commit().ok_or("No seed")?;
        let history_bytes = doc
            .get_last_local_change()
            .ok_or("No seed change")?
            .raw_bytes()
            .len();
        if history_bytes > MAX_HISTORY_BYTES {
            return Err("Canvas seed exceeds history byte budget".into());
        }
        Ok(Self {
            doc: doc.fork(),
            schema,
            seed,
            history_bytes,
            wall_time: 0,
            state_ref: state_ref.into(),
            signal_cache: Default::default(),
        })
    }
    pub fn load(schema: StateSchema, state_ref: &str, bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err("Canvas document limit".into());
        }
        let mut result = Self::new(schema, state_ref)?;
        let mut candidate = AutoCommit::load(bytes).map_err(err)?;
        let history_bytes = result.validate(&mut candidate, false)?;
        result.doc = candidate.fork();
        result.history_bytes = history_bytes;
        Ok(result)
    }
    /// Merge an authenticated authority snapshot while retaining valid local RAM edits.
    pub fn merge_authority(&mut self, bytes: &[u8]) -> Result<bool> {
        let mut remote = Self::load(self.schema.clone(), &self.state_ref, bytes)?;
        let old = self.doc.get_heads();
        let mut candidate = self.doc.clone();
        candidate.merge(&mut remote.doc).map_err(err)?;
        let history = self.validate(&mut candidate, false)?;
        let changed = old != candidate.get_heads();
        self.doc = candidate;
        self.history_bytes = history;
        if changed {
            self.signal_cache.get_mut().clear();
        }
        Ok(changed)
    }
    /// Lazily index authored field transitions. Wall time only changes this bounded view;
    /// no history traversal occurs unless a caller asks for a signal after a state edit.
    pub fn signal(&self, id: u16, now: i64, seconds: u32) -> Vec<[f32; 2]> {
        if seconds == 0 || seconds > 86400 {
            return vec![];
        }
        if !self.signal_cache.borrow().contains_key(&id) {
            use automerge::legacy::{Key, ObjectId, OpType};
            let Some(field) = self.schema.fields.iter().find(|f| f.id == id) else {
                return vec![];
            };
            let mut doc = self.doc.clone();
            let key = format!("f{id}");
            let mut samples = Vec::new();
            for change in doc.get_changes(&[]) {
                if change.timestamp() <= 0 || change.hash() == self.seed {
                    continue;
                }
                let value = change
                    .decode()
                    .operations
                    .into_iter()
                    .filter_map(|op| match (op.obj, op.key, op.action, op.insert) {
                        (
                            ObjectId::Root,
                            Key::Map(k),
                            OpType::Put(ScalarValue::Bytes(bytes)),
                            false,
                        ) if k == key => decode_value(field, &bytes).ok(),
                        _ => None,
                    })
                    .next_back();
                let value = match value {
                    Some(Value::Number(n)) => n / 10.,
                    Some(Value::Boolean(b)) => {
                        if b {
                            100.
                        } else {
                            0.
                        }
                    }
                    _ => continue,
                };
                samples.push((change.timestamp(), change.hash().to_string(), value));
            }
            samples.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            self.signal_cache.borrow_mut().insert(
                id,
                samples
                    .into_iter()
                    .map(|(t, _, v)| (t, v))
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        let cache = self.signal_cache.borrow();
        let samples = &cache[&id];
        let start = now.saturating_sub(i64::from(seconds));
        let first = samples.partition_point(|s| s.0 <= start);
        let last = samples.partition_point(|s| s.0 <= now);
        let bucket = ((i64::from(seconds) + (MAX_SERIES_POINTS - 2) as i64)
            / (MAX_SERIES_POINTS - 1) as i64)
            .max(1);
        let mut points = std::collections::BTreeMap::new();
        for &(time, value) in &samples[first..last] {
            let offset = time.saturating_sub(start);
            points.insert(offset / bucket, [offset as f32 / seconds as f32, value]);
        }
        points.into_values().collect()
    }
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }
    pub fn schema(&self) -> &StateSchema {
        &self.schema
    }
    /// Qualify a replaceable authority snapshot without inserting it into CRDT history.
    pub fn validate_snapshot(&self, values: &[(u16, Value)]) -> Result<()> {
        if values.len() > MAX_FIELDS {
            return Err("Canvas snapshot field limit".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for (id, value) in values {
            let field = self
                .schema
                .fields
                .iter()
                .find(|f| f.id == *id)
                .ok_or("Snapshot field outside schema")?;
            if field.writable || !ids.insert(*id) {
                return Err("Snapshot may only replace distinct read-only fields".into());
            }
            validate_value(field, value)?;
        }
        Ok(())
    }
    pub fn set_wall_time(&mut self, seconds: i64) {
        self.wall_time = seconds.max(0);
    }
    pub fn typed_value(&self, id: u16) -> Result<Value> {
        let field = self
            .schema
            .fields
            .iter()
            .find(|f| f.id == id)
            .ok_or("Field outside Canvas schema")?;
        let value = self
            .doc
            .get(ROOT, format!("f{id}"))
            .map_err(err)?
            .ok_or("Missing Canvas field")?
            .0;
        decode_value(
            field,
            value
                .as_bytes()
                .ok_or("Canvas field must be typed binary")?,
        )
    }
    /// Numeric event ABI convenience; other declared typed fields require typed_value.
    pub fn value(&self, id: u16) -> Result<u16> {
        match self.typed_value(id)? {
            Value::Number(value) if value.fract() == 0. => Ok(value as u16),
            _ => Err("Canvas field is not an integral normalized number".into()),
        }
    }
    pub fn set(&mut self, commands: &[(u16, u16)]) -> Result<bool> {
        self.set_at(commands, self.wall_time)
    }
    pub fn set_at(&mut self, commands: &[(u16, u16)], seconds: i64) -> Result<bool> {
        let typed = commands
            .iter()
            .map(|(id, v)| (*id, Value::Number(f32::from(*v))))
            .collect::<Vec<_>>();
        self.apply(&typed, false, seconds)
    }
    pub fn set_typed(&mut self, commands: &[(u16, Value)]) -> Result<bool> {
        self.apply(commands, false, self.wall_time)
    }
    pub fn set_typed_at(&mut self, commands: &[(u16, Value)], seconds: i64) -> Result<bool> {
        self.apply(commands, false, seconds)
    }
    /// Trusted authority API; never select this path using a client-supplied flag.
    pub fn set_typed_authority(&mut self, commands: &[(u16, Value)]) -> Result<bool> {
        self.apply(commands, true, self.wall_time)
    }
    fn apply(&mut self, commands: &[(u16, Value)], authority: bool, seconds: i64) -> Result<bool> {
        if commands.len() > MAX_FIELDS {
            return Err("Canvas command batch limit".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for (id, value) in commands {
            let field = self
                .schema
                .fields
                .iter()
                .find(|f| f.id == *id)
                .ok_or("State command outside schema")?;
            if !ids.insert(*id) || (!authority && !field.writable) {
                return Err("Canvas field is read-only or duplicated".into());
            }
            validate_value(field, value)?;
        }
        let mut next = self.doc.clone();
        let mut changed = false;
        for (id, value) in commands {
            if self.typed_value(*id)? != *value {
                next.put(
                    ROOT,
                    format!("f{id}"),
                    ScalarValue::Bytes(encode_value(value)?),
                )
                .map_err(err)?;
                changed = true;
            }
        }
        if changed {
            next.commit_with(CommitOptions::default().with_time(seconds.max(0)));
            let bytes = next
                .get_last_local_change()
                .ok_or("Missing typed state change")?
                .raw_bytes()
                .len();
            let history = self
                .history_bytes
                .checked_add(bytes)
                .ok_or("Canvas history byte overflow")?;
            if history > MAX_HISTORY_BYTES {
                return Err("Canvas history byte budget exhausted".into());
            }
            self.doc = next;
            self.history_bytes = history;
            self.signal_cache.get_mut().clear();
        }
        Ok(changed)
    }
    pub fn confirmed(&mut self, session: &Session) -> bool {
        self.doc
            .get_heads()
            .iter()
            .all(|h| session.0.shared_heads.contains(h))
    }
    pub fn generate(&mut self, session: &mut Session) -> Result<Option<Vec<u8>>> {
        let bytes = self
            .doc
            .sync()
            .generate_sync_message(&mut session.0)
            .map(|m| m.encode());
        if bytes.as_ref().is_some_and(|b| b.len() > MAX_BYTES) {
            return Err("Sync history budget exceeded".into());
        }
        Ok(bytes)
    }
    /// Authority ingress for untrusted client changes. Every historical operation is checked.
    pub fn receive(&mut self, session: &mut Session, bytes: &[u8]) -> Result<bool> {
        self.receive_inner(session, bytes, false)
    }
    /// Client ingress from its authenticated authority; read-only fields may be updated.
    pub fn receive_authority(&mut self, session: &mut Session, bytes: &[u8]) -> Result<bool> {
        self.receive_inner(session, bytes, true)
    }
    fn receive_inner(
        &mut self,
        session: &mut Session,
        bytes: &[u8],
        authority: bool,
    ) -> Result<bool> {
        if bytes.len() > MAX_BYTES {
            return Err("Sync size limit".into());
        }
        let message = automerge::sync::Message::decode(bytes).map_err(err)?;
        let mut next = self.doc.clone();
        let old = self.doc.get_heads();
        let mut sync = session.clone();
        next.sync()
            .receive_sync_message(&mut sync.0, message)
            .map_err(err)?;
        let history = self.validate(&mut next, !authority)?;
        let changed = old != next.get_heads();
        self.doc = next;
        self.history_bytes = history;
        if changed {
            self.signal_cache.get_mut().clear();
        }
        *session = sync;
        Ok(changed)
    }
    fn validate(&mut self, candidate: &mut AutoCommit, clients_only: bool) -> Result<usize> {
        use automerge::legacy::{Key, ObjectId, OpType};
        if candidate.get_change_meta_by_hash(&self.seed).is_none() {
            return Err("Wrong Canvas history/scope".into());
        }
        let mut history = self.history_bytes;
        // Walk only newly added changes, not the full document history on each update.
        for change in self.doc.get_changes_added(candidate) {
            if change.deps().is_empty() || change.len() > MAX_FIELDS {
                return Err("Canvas change root/operation limit".into());
            }
            history = history
                .checked_add(change.raw_bytes().len())
                .ok_or("Canvas history byte overflow")?;
            if history > MAX_HISTORY_BYTES {
                return Err("Canvas history byte budget exhausted".into());
            }
            for op in change.decode().operations {
                let (key, bytes) = match (&op.obj, &op.key, &op.action, op.insert) {
                    (
                        ObjectId::Root,
                        Key::Map(key),
                        OpType::Put(ScalarValue::Bytes(bytes)),
                        false,
                    ) => (key, bytes),
                    _ => return Err("Protected Canvas state operation".into()),
                };
                let field = self
                    .schema
                    .fields
                    .iter()
                    .find(|f| key == &format!("f{}", f.id))
                    .ok_or("Protected Canvas state field")?;
                if clients_only && !field.writable {
                    return Err("Client modified read-only Canvas field".into());
                }
                decode_value(field, bytes)?;
            }
        }
        Ok(history)
    }
}
fn encode_value(value: &Value) -> Result<Vec<u8>> {
    flexbuffers::to_vec(value).map_err(err)
}
fn decode_value(field: &Field, bytes: &[u8]) -> Result<Value> {
    check(bytes)?;
    let value: Value = flexbuffers::from_slice(bytes).map_err(err)?;
    validate_value(field, &value)?;
    if encode_value(&value)? != bytes {
        return Err("Noncanonical typed Canvas value".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(id: u16, default: Value, writable: bool) -> Field {
        Field {
            id,
            name: format!("field{id}"),
            default,
            writable,
        }
    }
    fn schema() -> StateSchema {
        StateSchema {
            id: "typed-state-v2".into(),
            fields: vec![
                field(1, Value::Number(50.), true),
                field(2, Value::Number(70.), true),
                field(3, Value::Text("hello".into()), true),
                field(4, Value::Boolean(false), true),
                field(5, Value::Series(vec![]), false),
                field(6, Value::TimedSeries(vec![]), false),
            ],
        }
    }
    fn sync(a: &mut Document, b: &mut Document, sa: &mut Session, sb: &mut Session) -> Result<()> {
        for _ in 0..20 {
            let x = a.generate(sa)?;
            let y = b.generate(sb)?;
            if x.is_none() && y.is_none() {
                return Ok(());
            }
            if let Some(x) = x {
                b.receive_authority(sb, &x)?;
            }
            if let Some(y) = y {
                a.receive(sa, &y)?;
            }
        }
        Err("sync stuck".into())
    }
    #[test]
    fn typed_values_roundtrip_atomic_batches_and_numeric_abi_stays_strict() {
        let mut d = Document::new(schema(), "a").unwrap();
        assert!(
            d.set_typed(&[
                (1, Value::Number(500.)),
                (3, Value::Text("typed ✓".into())),
                (4, Value::Boolean(true))
            ])
            .unwrap()
        );
        assert_eq!(d.value(1).unwrap(), 500);
        assert_eq!(d.typed_value(1).unwrap(), Value::Number(500.));
        let before = d.save();
        for bad in [
            Value::Text("wrong kind".into()),
            Value::Number(f32::NAN),
            Value::Number(0.5),
            Value::Number(1001.),
        ] {
            assert!(d.set_typed(&[(2, Value::Number(200.)), (1, bad)]).is_err());
            assert_eq!(d.save(), before);
        }
        assert!(
            d.set_typed(&[(1, Value::Number(1.)), (1, Value::Number(2.))])
                .is_err()
        );
        assert_eq!(d.save(), before);
        let copy = Document::load(schema(), "a", &before).unwrap();
        assert_eq!(copy.typed_value(3).unwrap(), Value::Text("typed ✓".into()));
        assert_eq!(copy.typed_value(4).unwrap(), Value::Boolean(true));
    }
    #[test]
    fn concurrent_fields_merge_and_persisted_scope_schema_are_checked() {
        let mut a = Document::new(schema(), "a").unwrap();
        let mut b = Document::new(schema(), "a").unwrap();
        let old = a.clone();
        a.set(&[(1, 900)]).unwrap();
        b.set_typed(&[(3, Value::Text("other".into()))]).unwrap();
        sync(
            &mut a,
            &mut b,
            &mut Session::default(),
            &mut Session::default(),
        )
        .unwrap();
        assert_eq!(a.value(1).unwrap(), 900);
        assert_eq!(b.value(1).unwrap(), 900);
        assert_eq!(a.typed_value(3).unwrap(), b.typed_value(3).unwrap());
        assert_eq!(old.value(1).unwrap(), 50);
        let saved = a.save();
        assert!(Document::load(schema(), "wrong-scope", &saved).is_err());
        let mut wrong = schema();
        wrong.fields[0].writable = false;
        assert!(Document::load(wrong, "a", &saved).is_err());
        let mut restored = Document::load(schema(), "a", &saved).unwrap();
        let mut fresh = Document::new(schema(), "a").unwrap();
        sync(
            &mut restored,
            &mut fresh,
            &mut Session::default(),
            &mut Session::default(),
        )
        .unwrap();
        assert_eq!(fresh.value(1).unwrap(), 900);
    }
    #[test]
    fn readonly_authority_updates_reach_clients_but_client_history_is_denied() {
        let mut authority = Document::new(schema(), "a").unwrap();
        let mut client = Document::new(schema(), "a").unwrap();
        let values = vec![
            (5, Value::Series(vec![1., 2., 3.])),
            (6, Value::TimedSeries(vec![[0., 20.], [1., 80.]])),
        ];
        assert!(client.set_typed(&values).is_err());
        authority.set_typed_authority(&values).unwrap();
        sync(
            &mut authority,
            &mut client,
            &mut Session::default(),
            &mut Session::default(),
        )
        .unwrap();
        assert_eq!(client.typed_value(5).unwrap(), values[0].1);
        let restored = Document::load(schema(), "a", &authority.save()).unwrap();
        assert_eq!(restored.typed_value(5).unwrap(), values[0].1);
        let mut rogue = authority.clone();
        // Restore the visible value after a forbidden write: both historical operations
        // still have to be rejected, even though the final projection is unchanged.
        rogue
            .doc
            .put(
                ROOT,
                "f5",
                ScalarValue::Bytes(encode_value(&Value::Series(vec![99.])).unwrap()),
            )
            .unwrap();
        rogue.doc.commit();
        rogue
            .doc
            .put(
                ROOT,
                "f5",
                ScalarValue::Bytes(encode_value(&values[0].1).unwrap()),
            )
            .unwrap();
        rogue.doc.commit();
        let before = authority.save();
        assert!(
            sync(
                &mut authority,
                &mut rogue,
                &mut Session::default(),
                &mut Session::default()
            )
            .is_err()
        );
        assert_eq!(authority.save(), before);
    }
    #[test]
    fn protected_scope_and_malformed_typed_history_leave_authority_unchanged() {
        for attack in 0..5 {
            let mut authority = Document::new(schema(), "a").unwrap();
            let mut rogue = authority.clone();
            match attack {
                0 => {
                    rogue.doc.put(ROOT, "scope", "tampered").unwrap();
                    rogue.doc.commit();
                    rogue.doc.put(ROOT, "scope", "a").unwrap();
                }
                1 => {
                    rogue
                        .doc
                        .put(
                            ROOT,
                            "f1",
                            ScalarValue::Bytes(
                                encode_value(&Value::Text("not a number".into())).unwrap(),
                            ),
                        )
                        .unwrap();
                }
                2 => {
                    rogue
                        .doc
                        .put(ROOT, "f1", ScalarValue::Bytes(vec![0, 1, 2, 3]))
                        .unwrap();
                }
                3 => {
                    rogue.doc.put(ROOT, "f1", ScalarValue::Uint(100)).unwrap();
                }
                _ => {
                    rogue
                        .doc
                        .put(
                            ROOT,
                            "f1",
                            ScalarValue::Bytes(encode_value(&Value::Number(0.5)).unwrap()),
                        )
                        .unwrap();
                }
            }
            rogue.doc.commit();
            let before = authority.save();
            assert!(
                sync(
                    &mut authority,
                    &mut rogue,
                    &mut Session::default(),
                    &mut Session::default()
                )
                .is_err()
            );
            assert_eq!(authority.save(), before);
        }
    }
    #[test]
    fn typed_size_bounds_and_replaceable_snapshots_do_not_write_history() {
        let d = Document::new(schema(), "a").unwrap();
        let text = schema().fields[2].clone();
        assert!(validate_value(&text, &Value::Text("a".repeat(MAX_TEXT_BYTES))).is_ok());
        assert!(validate_value(&text, &Value::Text("a".repeat(MAX_TEXT_BYTES + 1))).is_err());
        let series = schema().fields[4].clone();
        assert!(validate_value(&series, &Value::Series(vec![1.; MAX_SERIES_POINTS])).is_ok());
        assert!(validate_value(&series, &Value::Series(vec![1.; MAX_SERIES_POINTS + 1])).is_err());
        assert!(validate_value(&series, &Value::Series(vec![f32::INFINITY])).is_err());
        assert!(
            d.validate_snapshot(&[(5, Value::Series(vec![1.; MAX_SERIES_POINTS]))])
                .is_ok()
        );
        assert!(d.validate_snapshot(&[(1, Value::Number(1.))]).is_err());
        assert!(
            d.validate_snapshot(&[(5, Value::Series(vec![])), (5, Value::Series(vec![]))])
                .is_err()
        );
        assert_eq!(d.typed_value(5).unwrap(), Value::Series(vec![]));
        assert!(validate_value(&schema().fields[5], &Value::TimedSeries(vec![[2., 50.]])).is_err());
    }
    #[test]
    fn sixty_four_fields_are_bounded_without_a_small_register_lane_limit() {
        let mut schema = schema();
        schema.fields = (1..=64)
            .map(|id| field(id, Value::Number(0.), true))
            .collect();
        let mut d = Document::new(schema.clone(), "a").unwrap();
        let commands = (1..=64).map(|id| (id, id)).collect::<Vec<_>>();
        assert!(d.set(&commands).unwrap());
        assert_eq!(d.value(64).unwrap(), 64);
        let mut invalid = schema.clone();
        invalid.fields.push(field(65, Value::Number(0.), true));
        assert!(Document::new(invalid, "a").is_err());
        let copy = Document::load(schema, "a", &d.save()).unwrap();
        assert_eq!(copy.value(64).unwrap(), 64);
    }
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    fn schema() -> StateSchema {
        StateSchema {
            id: "lifecycle-v2".into(),
            fields: (1..=2)
                .map(|id| Field {
                    id,
                    name: format!("field{id}"),
                    default: Value::Number(0.),
                    writable: true,
                })
                .collect(),
        }
    }
    #[test]
    fn authoritative_mount_keeps_pending_ram_edits_and_signal_time_is_not_receipt_time() {
        let mut authority = Document::new(schema(), "state").unwrap();
        let mut client = Document::new(schema(), "state").unwrap();
        authority.set_at(&[(2, 500)], 100).unwrap();
        client.set_at(&[(1, 1000)], 200).unwrap();
        client.merge_authority(&authority.save()).unwrap();
        assert_eq!(client.value(1).unwrap(), 1000);
        assert_eq!(client.value(2).unwrap(), 500);
        let points = client.signal(1, 300, 300);
        assert_eq!(points.len(), 1);
        assert!((points[0][0] - 2. / 3.).abs() < 0.00001);
        assert_eq!(points[0][1], 100.);
        assert!(client.signal(1, 501, 300).is_empty());
        assert_eq!(
            Document::load(schema(), "state", &client.save())
                .unwrap()
                .signal(1, 300, 300),
            points
        );
    }
}
