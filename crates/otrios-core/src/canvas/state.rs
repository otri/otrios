// SPDX-License-Identifier: LGPL-3.0-only
//! Server-owned Merkle state, durable receipts and RAM-only optimistic intents.
use super::*;
use crate::merkle::{self, Actor, Authority, Cursor, Edit, Snapshot, Subscription};
use otrios_interfaces::layout::Value;
use std::collections::BTreeMap;
const ROOT_SCHEMA: &[u8] = b"otrios-canvas-merkle-v1";
const RECEIPTS: usize = 1024;
const RESUME_SECONDS: i64 = 86400;
const SIGNAL_SECONDS: i64 = 300;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Root {
    format: String,
    scope: String,
    schema: StateSchema,
    actor: Vec<u8>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    writer: Vec<u8>,
    sequence: u64,
    digest: Vec<u8>,
    time: i64,
}
#[derive(Clone)]
pub struct Document {
    schema: std::sync::Arc<StateSchema>,
    scope: String,
    actor: Actor,
    tree: Snapshot,
    field_hashes: std::sync::Arc<BTreeMap<u16, merkle::Hash>>,
    wall: i64,
    /// Authority policy from admitted UI dependencies; not replicated state.
    signal_fields: u64,
    workspace: std::sync::Arc<std::sync::Mutex<merkle::Workspace>>,
}
#[derive(Clone, Default)]
pub struct Session {
    writer: Option<[u8; 16]>,
    slot: Option<usize>,
    authority: Option<Authority>,
    subscription: Option<Subscription>,
    initial: bool,
    last_sent: Option<Cursor>,
    binary: bool,
}
impl Session {
    /// Opt in only when the admitted mount explicitly advertises Blob support.
    pub fn set_binary_transport(&mut self, enabled: bool) {
        self.binary = enabled;
    }
}
fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(err)?;
    Ok(bytes)
}
pub(crate) fn writer_nonce() -> Result<[u8; 16]> {
    random()
}
fn digest(bytes: &[u8]) -> merkle::Hash {
    Sha256::digest(bytes).into()
}
fn path(kind: u64, id: usize) -> Vec<u64> {
    vec![kind, (id / 32) as u64, (id % 32) as u64]
}
fn bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    flexbuffers::to_vec(value).map_err(err)
}
fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(data: &[u8]) -> Result<T> {
    check(data)?;
    let v: T = flexbuffers::from_slice(data).map_err(err)?;
    if bytes(&v)? != data {
        return Err("Noncanonical Canvas state payload".into());
    }
    Ok(v)
}
fn payload(tree: &Snapshot, p: &[u64]) -> Result<Vec<u8>> {
    let node = tree.subtree(p).map_err(err)?;
    let register = node.register().ok_or("Missing Canvas register")?;
    let mut values = register.values();
    let (_, _, value) = values.next().ok_or("Empty register")?;
    if values.next().is_some() {
        return Err("Canvas authority must resolve register conflicts".into());
    }
    value
        .map(<[u8]>::to_vec)
        .ok_or_else(|| "Deleted required Canvas register".into())
}
impl Document {
    pub fn new(schema: StateSchema, scope: &str) -> Result<Self> {
        validate_schema(&schema)?;
        if scope.is_empty() || scope.len() > 128 {
            return Err("Invalid State reference".into());
        }
        let actor = random()?;
        let root = Root {
            format: "otrios-canvas-merkle-v1".into(),
            scope: scope.into(),
            schema: schema.clone(),
            actor: actor.to_vec(),
        };
        let field_hashes = std::sync::Arc::new(Self::field_hashes(&schema, scope)?);
        let mut edits = vec![Edit {
            path: vec![],
            schema: digest(ROOT_SCHEMA),
            value: Some(bytes(&root)?),
        }];
        for field in &schema.fields {
            edits.push(Edit {
                path: path(1, usize::from(field.id) - 1),
                schema: field_hashes[&field.id],
                value: Some(bytes(&field.default)?),
            });
        }
        let tree = Snapshot::empty().edit([0; 16], &edits).map_err(err)?.0;
        Ok(Self {
            schema: schema.into(),
            scope: scope.into(),
            actor,
            tree,
            field_hashes,
            wall: 0,
            signal_fields: 0,
            workspace: Default::default(),
        })
    }
    fn field_hashes(schema: &StateSchema, scope: &str) -> Result<BTreeMap<u16, merkle::Hash>> {
        schema
            .fields
            .iter()
            .map(|f| {
                Ok((
                    f.id,
                    digest(
                        &[
                            b"OTriOS\0canvas-field-schema\0v1\0".as_slice(),
                            &bytes(&(scope, &schema.id, f))?,
                        ]
                        .concat(),
                    ),
                ))
            })
            .collect()
    }
    pub fn load(schema: StateSchema, scope: &str, data: &[u8]) -> Result<Self> {
        if data.len() > MAX_BYTES {
            return Err("Canvas state size limit".into());
        }
        let tree = Snapshot::from_bytes(data).map_err(err)?;
        Self::from_tree(schema, scope, tree)
    }
    fn from_tree(schema: StateSchema, scope: &str, tree: Snapshot) -> Result<Self> {
        validate_schema(&schema)?;
        let root: Root = decode(&payload(&tree, &[])?)?;
        if root.format != "otrios-canvas-merkle-v1"
            || root.scope != scope
            || root.schema != schema
            || root.actor.len() != 16
        {
            return Err("Wrong Canvas schema/scope".into());
        }
        let actor = root.actor.as_slice().try_into().map_err(err)?;
        let result = Self {
            field_hashes: std::sync::Arc::new(Self::field_hashes(&schema, scope)?),
            schema: schema.into(),
            scope: scope.into(),
            actor,
            tree,
            wall: 0,
            signal_fields: 0,
            workspace: Default::default(),
        };
        result.validate_tree()?;
        Ok(result)
    }
    fn validate_tree(&self) -> Result<()> {
        self.validate_nodes(None, &self.tree)?;
        for f in &self.schema.fields {
            self.typed_value(f.id)?;
        }
        Ok(())
    }
    fn validate_changed(&self, next: &Snapshot) -> Result<()> {
        if self.tree.register() != next.register() {
            return Err("Authority changed immutable schema/scope metadata".into());
        }
        self.validate_nodes(Some(&self.tree), next)
    }
    fn validate_nodes(&self, old: Option<&Snapshot>, tree: &Snapshot) -> Result<()> {
        if tree
            .register()
            .is_none_or(|r| r.schema() != digest(ROOT_SCHEMA))
        {
            return Err("Wrong Canvas root kind".into());
        }
        fn retained(old: Option<&Snapshot>, new: &Snapshot) -> Result<()> {
            if let Some(old) = old
                && old.child_keys().any(|k| new.subtree(&[k]).is_err())
            {
                return Err("Canvas structure was removed".into());
            }
            Ok(())
        }
        retained(old, tree)?;
        for kind in tree.child_keys() {
            if !(1..=3).contains(&kind) {
                return Err("Unknown Canvas subtree".into());
            }
            let branch = tree.subtree(&[kind]).map_err(err)?;
            let previous = old.and_then(|s| s.subtree(&[kind]).ok());
            if previous.as_ref().is_some_and(|p| p.hash() == branch.hash()) {
                continue;
            }
            if branch.register().is_some() {
                return Err("Invalid Canvas branch".into());
            }
            retained(previous.as_ref(), &branch)?;
            for bucket in branch.child_keys() {
                if bucket >= 32 {
                    return Err("Canvas bucket bound".into());
                }
                let group = branch.subtree(&[bucket]).map_err(err)?;
                let before = previous.as_ref().and_then(|p| p.subtree(&[bucket]).ok());
                if before.as_ref().is_some_and(|p| p.hash() == group.hash()) {
                    continue;
                }
                if group.register().is_some() {
                    return Err("Invalid Canvas bucket".into());
                }
                retained(before.as_ref(), &group)?;
                for key in group.child_keys() {
                    if key >= 32 {
                        return Err("Canvas key bound".into());
                    }
                    let leaf = group.subtree(&[key]).map_err(err)?;
                    if before
                        .as_ref()
                        .and_then(|p| p.subtree(&[key]).ok())
                        .is_some_and(|p| p.hash() == leaf.hash())
                    {
                        continue;
                    }
                    if leaf.child_keys().next().is_some() {
                        return Err("Unexpected Canvas descendants".into());
                    }
                    let slot = (bucket * 32 + key) as usize;
                    let schema = if kind == 1 {
                        *self
                            .field_hashes
                            .get(&u16::try_from(slot + 1).map_err(err)?)
                            .ok_or("Unknown field")?
                    } else {
                        self.extra_schema(kind, slot)
                    };
                    if leaf.register().is_none_or(|r| r.schema() != schema) {
                        return Err("Wrong Canvas leaf schema".into());
                    }
                    let data = payload(tree, &path(kind, slot))?;
                    match kind {
                        1 => {
                            let f = self.field(u16::try_from(slot + 1).map_err(err)?)?;
                            let value: Value = decode(&data)?;
                            validate_value(f, &value)?;
                        }
                        2 => {
                            let r: Receipt = decode(&data)?;
                            if r.writer.len() != 32 || r.digest.len() != 32 || r.time < 0 {
                                return Err("Invalid receipt".into());
                            }
                        }
                        3 => {
                            self.field(u16::try_from(slot + 1).map_err(err)?)?;
                            let signal: Vec<(i64, f32)> = decode(&data)?;
                            if signal.len() > 301
                                || signal.windows(2).any(|p| p[0].0 >= p[1].0)
                                || signal.iter().any(|(t, v)| {
                                    *t < 0 || !v.is_finite() || !(0. ..=100.).contains(v)
                                })
                            {
                                return Err("Invalid signal window".into());
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        Ok(())
    }
    pub fn save(&self) -> Result<Vec<u8>> {
        let data = self.tree.to_bytes().map_err(err)?;
        if data.len() > MAX_BYTES {
            return Err("Canvas state byte budget exhausted".into());
        }
        Ok(data)
    }
    pub fn journal_delta(&self, base: &Self) -> Result<Vec<u8>> {
        self.tree.delta_bytes(&base.tree).map_err(err)
    }
    pub fn journal_roots(data: &[u8]) -> Result<(merkle::Hash, merkle::Hash)> {
        Snapshot::delta_roots(data).map_err(err)
    }
    pub fn root_hash(&self) -> merkle::Hash {
        self.tree.hash()
    }
    pub fn replay(&mut self, data: &[u8]) -> Result<()> {
        let tree = self.tree.apply_delta(data).map_err(err)?;
        self.validate_changed(&tree)?;
        self.tree = tree;
        Ok(())
    }
    pub fn schema(&self) -> &StateSchema {
        &self.schema
    }
    pub fn set_wall_time(&mut self, time: i64) {
        self.wall = time.max(0);
    }
    /// Only admitted UI signal selectors request retained history. Configuration
    /// is RAM-only authority policy; loading a checkpoint does not enable it.
    pub fn set_signal_fields(&mut self, fields: &[u16]) -> Result<()> {
        let mut mask = 0;
        for id in fields {
            let field = self.field(*id)?;
            if !matches!(field.default, Value::Number(_) | Value::Boolean(_)) {
                return Err("Signal requires a numeric or Boolean field".into());
            }
            mask |= 1_u64 << (id - 1);
        }
        self.signal_fields = mask;
        Ok(())
    }
    fn field(&self, id: u16) -> Result<&Field> {
        self.schema
            .fields
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| "Field outside Canvas schema".into())
    }
    pub fn typed_value(&self, id: u16) -> Result<Value> {
        let f = self.field(id)?;
        let v = decode(&payload(&self.tree, &path(1, usize::from(id) - 1))?)?;
        validate_value(f, &v)?;
        Ok(v)
    }
    pub fn value(&self, id: u16) -> Result<u16> {
        match self.typed_value(id)? {
            Value::Number(n) if n.fract() == 0. => Ok(n as u16),
            _ => Err("Canvas field is not a normalized number".into()),
        }
    }
    fn validate_commands(&self, commands: &[(u16, Value)], authority: bool) -> Result<()> {
        if commands.len() > MAX_FIELDS {
            return Err("Canvas command batch limit".into());
        }
        let mut ids = 0_u64;
        for (id, v) in commands {
            let f = self.field(*id)?;
            let bit = 1_u64 << (*id - 1); // Admitted fields are 1..=64.
            if ids & bit != 0 || (!authority && !f.writable) {
                return Err("Read-only or duplicated Canvas field".into());
            }
            ids |= bit;
            validate_value(f, v)?;
        }
        Ok(())
    }
    pub fn validate_snapshot(&self, values: &[(u16, Value)]) -> Result<()> {
        self.validate_commands(values, true)?;
        if values
            .iter()
            .any(|(id, _)| self.field(*id).is_ok_and(|f| f.writable))
        {
            return Err("Snapshot may only replace read-only fields".into());
        }
        Ok(())
    }
    fn extra_schema(&self, kind: u64, slot: usize) -> merkle::Hash {
        digest(
            &[
                b"OTriOS\0canvas-metadata-schema\0v1\0".as_slice(),
                &(self.scope.len() as u32).to_le_bytes(),
                self.scope.as_bytes(),
                &kind.to_le_bytes(),
                &(slot as u64).to_le_bytes(),
            ]
            .concat(),
        )
    }
    fn change(&mut self, commands: &[(u16, Value)], extra: Vec<Edit>) -> Result<bool> {
        let mut edits = extra;
        for (id, value) in commands {
            if self.typed_value(*id)? == *value {
                continue;
            }
            let slot = usize::from(*id) - 1;
            edits.push(Edit {
                path: path(1, slot),
                schema: self.field_hashes[id],
                value: Some(bytes(value)?),
            });
            let n = match value {
                Value::Number(n) => Some(*n / 10.),
                Value::Boolean(b) => Some(if *b { 100. } else { 0. }),
                _ => None,
            };
            if let Some(n) =
                n.filter(|_| self.wall > 0 && self.signal_fields & (1_u64 << slot) != 0)
            {
                let p = path(3, slot);
                let mut samples: Vec<(i64, f32)> = payload(&self.tree, &p)
                    .ok()
                    .map(|b| decode(&b))
                    .transpose()?
                    .unwrap_or_default();
                samples.retain(|(t, _)| {
                    *t > self.wall.saturating_sub(SIGNAL_SECONDS) && *t < self.wall
                });
                samples.push((self.wall, n));
                edits.push(Edit {
                    path: p,
                    schema: self.extra_schema(3, slot),
                    value: Some(bytes(&samples)?),
                });
            }
        }
        if edits.is_empty() {
            return Ok(false);
        }
        let next = self
            .tree
            .edit_with_workspace(
                self.actor,
                &edits,
                &mut *self.workspace.lock().map_err(err)?,
            )
            .map_err(err)?
            .0;
        if next.canonical_bytes() > MAX_BYTES - 8192 {
            return Err("Canvas state byte budget exhausted".into());
        }
        self.tree = next;
        Ok(true)
    }
    pub fn set_typed(&mut self, commands: &[(u16, Value)]) -> Result<bool> {
        self.validate_commands(commands, false)?;
        self.change(commands, vec![])
    }
    pub fn set(&mut self, commands: &[(u16, u16)]) -> Result<bool> {
        self.set_typed(
            &commands
                .iter()
                .map(|(id, v)| (*id, Value::Number(f32::from(*v))))
                .collect::<Vec<_>>(),
        )
    }
    pub fn set_typed_authority(&mut self, commands: &[(u16, Value)]) -> Result<bool> {
        self.validate_commands(commands, true)?;
        self.change(commands, vec![])
    }
    pub fn signal(&self, id: u16, now: i64, seconds: u32) -> Vec<[f32; 2]> {
        if id == 0 || seconds == 0 || seconds > 86400 {
            return vec![];
        }
        let samples: Vec<(i64, f32)> = payload(&self.tree, &path(3, usize::from(id) - 1))
            .ok()
            .and_then(|b| decode(&b).ok())
            .unwrap_or_default();
        let start = now.saturating_sub(i64::from(seconds));
        samples
            .into_iter()
            .filter(|(t, _)| *t > start && *t <= now)
            .map(|(t, v)| [(t - start) as f32 / seconds as f32, v])
            .collect()
    }
    fn receipts(&self) -> Result<Vec<(usize, Receipt)>> {
        let Ok(branch) = self.tree.subtree(&[2]) else {
            return Ok(vec![]);
        };
        let mut out = Vec::new();
        for bucket in branch.child_keys() {
            let group = branch.subtree(&[bucket]).map_err(err)?;
            for key in group.child_keys() {
                let slot = (bucket * 32 + key) as usize;
                out.push((slot, decode(&payload(&self.tree, &path(2, slot))?)?));
            }
        }
        Ok(out)
    }
    fn receipt(&self, writer: &[u8; 16]) -> Result<Option<(usize, Receipt)>> {
        let hash = hash(writer);
        Ok(self.receipts()?.into_iter().find(|(_, r)| r.writer == hash))
    }
    fn admit(&mut self, writer: &[u8; 16], resume: u64) -> Result<bool> {
        let records = self.receipts()?;
        if let Some((_, record)) = records.iter().find(|(_, r)| r.writer == hash(writer)) {
            if resume > record.sequence.saturating_add(1) {
                return Err("Writer resume sequence is ahead".into());
            }
            if self.wall.saturating_sub(record.time) <= RESUME_SECONDS {
                return Ok(false);
            }
        }
        if resume > 0 {
            return Err("Pending writer receipt expired; no uncertain intent replayed".into());
        }
        let existing = records
            .iter()
            .find(|(_, r)| r.writer == hash(writer))
            .map(|(slot, _)| *slot);
        let slot = existing
            .or_else(|| (0..RECEIPTS).find(|i| !records.iter().any(|(slot, _)| slot == i)))
            .or_else(|| {
                records
                    .iter()
                    .filter(|(_, r)| self.wall.saturating_sub(r.time) > RESUME_SECONDS)
                    .min_by_key(|(_, r)| r.time)
                    .map(|(s, _)| *s)
            })
            .ok_or("Writer receipt capacity exhausted")?;
        let receipt = Receipt {
            writer: hash(writer),
            sequence: 0,
            digest: vec![0; 32],
            time: self.wall,
        };
        self.change(
            &[],
            vec![Edit {
                path: path(2, slot),
                schema: self.extra_schema(2, slot),
                value: Some(bytes(&receipt)?),
            }],
        )
    }
    pub fn open_session(&mut self, writer: &[u8], resume: u64) -> Result<(Session, bool)> {
        let writer: [u8; 16] = writer.try_into().map_err(|_| "Invalid writer nonce")?;
        let changed = self.admit(&writer, resume)?;
        let authority = Authority::new(u64::from_le_bytes(random()?), self.tree.clone());
        Ok((
            Session {
                writer: Some(writer),
                slot: self.receipt(&writer)?.map(|(slot, _)| slot),
                authority: Some(authority),
                subscription: None,
                initial: true,
                last_sent: None,
                binary: false,
            },
            changed,
        ))
    }
    pub fn receive(&mut self, session: &mut Session, data: &[u8]) -> Result<bool> {
        let request = decode_state_request(data)?;
        if let StateRequest::Hello { writer, resume } = request {
            if session.writer.is_some() {
                return Err("Writer already bound to this lease".into());
            }
            let (next, changed) = self.open_session(&writer, resume)?;
            *session = next;
            return Ok(changed);
        }
        let writer = session
            .writer
            .ok_or("State write without admitted writer")?;
        match request {
            StateRequest::Write { sequence, values } => {
                self.validate_commands(&values, false)?;
                if values.is_empty() {
                    return Err("Empty state write".into());
                }
                let slot = session.slot.ok_or("Missing writer slot")?;
                let mut receipt: Receipt = decode(&payload(&self.tree, &path(2, slot))?)?;
                if receipt.writer != hash(&writer) {
                    return Err("Writer receipt no longer retained".into());
                }
                let d = hash(&bytes(&values)?);
                if sequence <= receipt.sequence {
                    if sequence == receipt.sequence && receipt.digest != d {
                        return Err("Changed replay at same intent sequence".into());
                    }
                    return Ok(false);
                }
                if sequence
                    != receipt
                        .sequence
                        .checked_add(1)
                        .ok_or("Writer sequence exhausted")?
                {
                    return Err("State intent sequence gap".into());
                }
                receipt.sequence = sequence;
                receipt.digest = d;
                receipt.time = self.wall;
                self.change(
                    &values,
                    vec![Edit {
                        path: path(2, slot),
                        schema: self.extra_schema(2, slot),
                        value: Some(bytes(&receipt)?),
                    }],
                )
            }
            StateRequest::Ack {
                authority,
                lease,
                sequence,
                root,
            } => {
                let hash: merkle::Hash = root.try_into().map_err(|_| "Invalid state ACK digest")?;
                session
                    .subscription
                    .as_mut()
                    .ok_or("ACK before mount")?
                    .acknowledge(
                        Cursor {
                            epoch: authority,
                            lease,
                            sequence,
                        },
                        Some(hash),
                    )
                    .map_err(err)?;
                Ok(false)
            }
            StateRequest::Resync => {
                session
                    .subscription
                    .as_mut()
                    .ok_or("Resync before mount")?
                    .resync();
                Ok(false)
            }
            StateRequest::Hello { .. } => unreachable!(),
        }
    }
    pub fn generate(&self, session: &mut Session) -> Result<Option<Vec<u8>>> {
        let Some(authority) = &mut session.authority else {
            return Ok(None);
        };
        if authority.snapshot().hash() != self.tree.hash() {
            authority
                .publish(authority.snapshot().hash(), self.tree.clone())
                .map_err(err)?;
        }
        if session.subscription.is_none() {
            session.subscription = Some(authority.subscribe(&[vec![]], vec![]).map_err(err)?);
        }
        let sub = session
            .subscription
            .as_mut()
            .ok_or("Missing state subscription")?;
        let Some(offer) = authority.offer(sub).map_err(err)? else {
            return Ok(None);
        };
        if !session.initial && session.last_sent == Some(offer.cursor) {
            return Ok(None);
        }
        let response = if session.initial {
            session.initial = false;
            StateResponse::Mount {
                authority: offer.cursor.epoch,
                lease: offer.cursor.lease,
                frame: offer.bytes().to_vec(),
            }
        } else {
            session.last_sent = Some(offer.cursor);
            StateResponse::Update {
                frame: offer.bytes().to_vec(),
            }
        };
        if session.binary {
            encode_state_response_binary(&response)
        } else {
            encode_state_response(&response)
        }
        .map(Some)
    }
    /// One-time trusted-file import. The network protocol never accepts Automerge.
    pub fn import_legacy(schema: StateSchema, scope: &str, data: &[u8]) -> Result<Self> {
        use automerge::{AutoCommit, ROOT, ReadDoc};
        if data.len() > MAX_BYTES {
            return Err("Legacy state size limit".into());
        }
        let old = AutoCommit::load(data).map_err(err)?;
        if old
            .get(ROOT, "scope")
            .map_err(err)?
            .and_then(|v| v.0.as_str().map(str::to_owned))
            .as_deref()
            != Some(scope)
            || old
                .get(ROOT, "schema")
                .map_err(err)?
                .and_then(|v| v.0.as_bytes().map(<[u8]>::to_vec))
                != Some(encode_schema(&schema)?)
        {
            return Err("Legacy schema/scope mismatch".into());
        }
        let mut doc = Self::new(schema, scope)?;
        let mut values = Vec::new();
        for f in &doc.schema.fields {
            let raw = old
                .get(ROOT, format!("f{}", f.id))
                .map_err(err)?
                .ok_or("Missing legacy field")?;
            let value: Value = decode(raw.0.as_bytes().ok_or("Legacy value is not typed binary")?)?;
            validate_value(f, &value)?;
            values.push((f.id, value));
        }
        doc.set_typed_authority(&values)?;
        Ok(doc)
    }
}

#[derive(Clone)]
struct Flight {
    sequence: u64,
    values: Vec<(u16, Value)>,
    sent: bool,
}
/// Bounded scheduling of unissued continuous numeric assignments. Transactions
/// and issued causal writes are never split or replaced by this policy.
#[derive(Clone, Copy, Debug)]
pub struct DispatchPolicy {
    interval_ms: u64,
    max_fields: usize,
}
impl Default for DispatchPolicy {
    fn default() -> Self {
        Self {
            interval_ms: 33,
            max_fields: MAX_FIELDS,
        }
    }
}
impl DispatchPolicy {
    pub fn new(interval_ms: u64, max_fields: usize) -> Result<Self> {
        if interval_ms > 1000 || !(1..=MAX_FIELDS).contains(&max_fields) {
            return Err("Dispatch policy outside bounded limits".into());
        }
        Ok(Self {
            interval_ms,
            max_fields,
        })
    }
}
/// RAM-only pending intents, separate from authoritative causal metadata.
pub struct StateAdapter {
    doc: Document,
    writer: [u8; 16],
    replica: Option<merkle::Replica>,
    pending: BTreeMap<u16, Value>,
    dispatch: DispatchPolicy,
    clock: u64,
    last_issue: Option<u64>,
    deadline: Option<u64>,
    flight: Option<Flight>,
    acknowledged: u64,
    slot: Option<usize>,
    ack: Option<(Cursor, merkle::Hash)>,
    hello_sent: bool,
    resync: bool,
}
impl StateAdapter {
    pub fn new(schema: StateSchema, scope: &str, data: &[u8], writer: [u8; 16]) -> Result<Self> {
        let transport = !data.is_empty() && decode_state_response(data).is_ok();
        let doc = if data.is_empty() || transport {
            Document::new(schema, scope)?
        } else {
            Document::load(schema, scope, data)?
        };
        let mut result = Self {
            doc,
            writer,
            replica: None,
            pending: BTreeMap::new(),
            dispatch: DispatchPolicy::default(),
            clock: 0,
            last_issue: None,
            deadline: None,
            flight: None,
            acknowledged: 0,
            slot: None,
            ack: None,
            hello_sent: false,
            resync: false,
        };
        if transport {
            result.receive(data)?;
        }
        Ok(result)
    }
    pub fn restore_pending(&mut self, old: &Self) -> Result<()> {
        if self.writer != old.writer || self.doc.scope != old.doc.scope {
            return Err("Pending intent scope mismatch".into());
        }
        self.pending = old.pending.clone();
        self.dispatch = old.dispatch;
        self.clock = old.clock;
        self.last_issue = old.last_issue;
        self.deadline = old.deadline;
        self.flight = old.flight.clone();
        if let Some(f) = &mut self.flight {
            f.sent = false;
        }
        self.reconcile()?;
        Ok(())
    }
    fn reconcile(&mut self) -> Result<()> {
        let sequence = if let Some(slot) = self.slot {
            let record: Receipt = decode(&payload(&self.doc.tree, &path(2, slot))?)?;
            if record.writer != hash(&self.writer) {
                return Err("Writer receipt retired; uncertain intent not replayed".into());
            }
            record.sequence
        } else {
            0
        };
        self.acknowledged = self.acknowledged.max(sequence);
        if self.flight.as_ref().is_some_and(|f| f.sequence <= sequence) {
            self.flight = None;
        }
        Ok(())
    }
    pub fn resume_sequence(&self) -> u64 {
        self.flight.as_ref().map_or(0, |f| f.sequence)
    }
    pub fn reconnect(&mut self) {
        if let Some(f) = &mut self.flight {
            f.sent = false;
        }
        self.hello_sent = false;
    }
    pub fn confirmed(&self) -> bool {
        self.pending.is_empty() && self.flight.is_none() && self.replica.is_some()
    }
    pub fn schema(&self) -> &StateSchema {
        self.doc.schema()
    }
    pub fn value(&self, id: u16) -> Result<u16> {
        match self.typed_value(id)? {
            Value::Number(n) => Ok(n as u16),
            _ => Err("Not a normalized number".into()),
        }
    }
    pub fn typed_value(&self, id: u16) -> Result<Value> {
        self.pending
            .get(&id)
            .or_else(|| {
                self.flight
                    .as_ref()
                    .and_then(|f| f.values.iter().find(|(k, _)| *k == id).map(|(_, v)| v))
            })
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| self.doc.typed_value(id))
    }
    pub fn set_wall_time(&mut self, time: i64) {
        self.doc.set_wall_time(time);
    }
    pub fn signal(&self, id: u16, time: i64, seconds: u32) -> Vec<[f32; 2]> {
        self.doc.signal(id, time, seconds)
    }
    pub fn validate_snapshot(&self, values: &[(u16, Value)]) -> Result<()> {
        self.doc.validate_snapshot(values)
    }
    pub fn set(&mut self, values: &[(u16, u16)]) -> Result<bool> {
        self.set_typed(
            &values
                .iter()
                .map(|(id, v)| (*id, Value::Number(f32::from(*v))))
                .collect::<Vec<_>>(),
        )
    }
    pub fn set_dispatch_policy(&mut self, policy: DispatchPolicy) {
        self.dispatch = policy;
        if let Some(deadline) = &mut self.deadline {
            *deadline = (*deadline).min(self.last_issue.map_or(self.clock, |at| {
                at.saturating_add(policy.interval_ms).max(self.clock)
            }));
        }
    }
    /// Host monotonic milliseconds; backwards samples cannot postpone a batch.
    pub fn advance(&mut self, now: u64) {
        self.clock = self.clock.max(now);
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len() + self.flight.as_ref().map_or(0, |f| f.values.len())
    }
    /// Finish a gesture immediately; acknowledgements and replay bypass this timer.
    pub fn flush_pending(&mut self) {
        self.deadline = (!self.pending.is_empty()).then_some(self.clock);
    }
    pub fn defer(&mut self, values: &[(u16, u16)]) -> Result<bool> {
        if values.len() > MAX_FIELDS {
            return Err("Canvas command batch limit".into());
        }
        // Bounded numeric staging uses stack storage, not a Vec per touch sample.
        let mut numeric: [(u16, Value); MAX_FIELDS] =
            std::array::from_fn(|_| (0, Value::Number(0.)));
        for ((id, number), slot) in values.iter().zip(&mut numeric) {
            *slot = (*id, Value::Number(f32::from(*number)));
        }
        let changed = self.queue(&numeric[..values.len()])?;
        if changed && self.deadline.is_none() {
            self.deadline = Some(self.last_issue.map_or(self.clock, |at| {
                at.saturating_add(self.dispatch.interval_ms).max(self.clock)
            }));
        }
        if self.pending.len() >= self.dispatch.max_fields {
            self.flush_pending();
        }
        Ok(changed)
    }
    pub fn set_typed(&mut self, values: &[(u16, Value)]) -> Result<bool> {
        let changed = self.queue(values)?;
        if changed {
            self.flush_pending();
        }
        Ok(changed)
    }
    fn queue(&mut self, values: &[(u16, Value)]) -> Result<bool> {
        self.doc.validate_commands(values, false)?;
        let mut changed = false;
        for (id, value) in values {
            if self.typed_value(*id)? != *value {
                self.pending.insert(*id, value.clone());
                changed = true;
            }
        }
        Ok(changed)
    }
    pub fn generate(&mut self) -> Result<Option<Vec<u8>>> {
        if std::mem::take(&mut self.resync) {
            return encode_state_request(&StateRequest::Resync).map(Some);
        }
        if let Some((c, root)) = self.ack.take() {
            return encode_state_request(&StateRequest::Ack {
                authority: c.epoch,
                lease: c.lease,
                sequence: c.sequence,
                root: root.to_vec(),
            })
            .map(Some);
        }
        if self.replica.is_none() {
            if self.hello_sent {
                return Ok(None);
            }
            self.hello_sent = true;
            return encode_state_request(&StateRequest::Hello {
                writer: self.writer.to_vec(),
                resume: self.resume_sequence(),
            })
            .map(Some);
        }
        if self.flight.is_none() && !self.pending.is_empty() {
            if self.deadline.is_some_and(|at| self.clock < at) {
                return Ok(None);
            }
            self.deadline = None;
            self.last_issue = Some(self.clock);
            let values = std::mem::take(&mut self.pending).into_iter().collect();
            self.flight = Some(Flight {
                sequence: self
                    .acknowledged
                    .checked_add(1)
                    .ok_or("Intent sequence exhausted")?,
                values,
                sent: false,
            });
        }
        if let Some(f) = &mut self.flight
            && !f.sent
        {
            f.sent = true;
            return encode_state_request(&StateRequest::Write {
                sequence: f.sequence,
                values: f.values.clone(),
            })
            .map(Some);
        }
        Ok(None)
    }
    pub fn receive(&mut self, data: &[u8]) -> Result<bool> {
        let response = decode_state_response(data)?;
        let mounting = matches!(response, StateResponse::Mount { .. });
        let (mut replica, frame) = match response {
            StateResponse::Mount {
                authority,
                lease,
                frame,
            } => (
                merkle::Replica::for_lease(authority, lease, vec![]).map_err(err)?,
                frame,
            ),
            StateResponse::Update { frame } => (
                self.replica.clone().ok_or("State update before mount")?,
                frame,
            ),
        };
        match replica.apply(&frame) {
            Ok(_) => {}
            Err(merkle::Error::ResyncRequired) => {
                self.resync = true;
                if let Some(f) = &mut self.flight {
                    f.sent = false;
                }
                return Ok(false);
            }
            Err(e) => return Err(err(e)),
        }
        let tree = replica.snapshot().ok_or("Missing Canvas subtree")?.clone();
        let old = self.doc.tree.hash();
        let mut next = if mounting {
            Document::from_tree(self.doc.schema.as_ref().clone(), &self.doc.scope, tree)?
        } else {
            self.doc.validate_changed(&tree)?;
            let mut next = self.doc.clone();
            next.tree = tree;
            next
        };
        if mounting {
            self.slot = next.receipt(&self.writer)?.map(|(slot, _)| slot);
        }
        next.wall = self.doc.wall;
        let cursor = replica.cursor().ok_or("Missing state cursor")?;
        self.ack = Some((cursor, next.tree.hash()));
        self.doc = next;
        self.replica = Some(replica);
        self.hello_sent = true;
        self.reconcile()?;
        Ok(old != self.doc.tree.hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> StateSchema {
        StateSchema {
            id: "merkle-adapter-test-v1".into(),
            fields: vec![
                Field {
                    id: 1,
                    name: "one".into(),
                    default: Value::Number(0.),
                    writable: true,
                },
                Field {
                    id: 2,
                    name: "two".into(),
                    default: Value::Text("seed".into()),
                    writable: true,
                },
                Field {
                    id: 3,
                    name: "authority".into(),
                    default: Value::Boolean(false),
                    writable: false,
                },
            ],
        }
    }
    fn client(id: u8) -> StateAdapter {
        StateAdapter::new(schema(), "test", &[], [id; 16]).unwrap()
    }
    fn pump(doc: &mut Document, s: &mut Session, c: &mut StateAdapter) {
        for _ in 0..20 {
            let mut progress = false;
            while let Some(b) = c.generate().unwrap() {
                doc.receive(s, &b).unwrap();
                progress = true;
            }
            if let Some(b) = doc.generate(s).unwrap() {
                c.receive(&b).unwrap();
                progress = true;
            }
            if !progress {
                assert!(c.confirmed());
                return;
            }
        }
        panic!("adapter did not settle");
    }
    #[test]
    fn dispatch_deadline_coalesces_without_postponing_and_control_bypasses_it() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let mut c = client(1);
        let mut session = Session::default();
        pump(&mut doc, &mut session, &mut c);
        c.advance(100);
        c.defer(&[(1, 100)]).unwrap();
        let first = c.generate().unwrap().unwrap();
        assert!(matches!(
            decode_state_request(&first).unwrap(),
            StateRequest::Write { sequence: 1, .. }
        ));
        doc.receive(&mut session, &first).unwrap();
        c.receive(&doc.generate(&mut session).unwrap().unwrap())
            .unwrap();
        c.advance(101);
        c.defer(&[(1, 200)]).unwrap();
        let ack = c.generate().unwrap().unwrap();
        assert!(
            matches!(
                decode_state_request(&ack).unwrap(),
                StateRequest::Ack { .. }
            ),
            "ACK bypasses a deferred write"
        );
        doc.receive(&mut session, &ack).unwrap();
        assert!(c.generate().unwrap().is_none());
        c.advance(120);
        c.defer(&[(1, 700)]).unwrap();
        c.advance(132);
        assert!(c.generate().unwrap().is_none());
        c.advance(133);
        let next = c.generate().unwrap().unwrap();
        assert!(
            matches!(decode_state_request(&next).unwrap(), StateRequest::Write { sequence: 2, values } if values == vec![(1, Value::Number(700.))])
        );
        doc.receive(&mut session, &next).unwrap();
        c.receive(&doc.generate(&mut session).unwrap().unwrap())
            .unwrap();
        // Explicit final release must not wait for another 33 ms interval.
        c.advance(134);
        c.defer(&[(1, 900)]).unwrap();
        c.flush_pending();
        pump(&mut doc, &mut session, &mut c);
        assert_eq!(doc.value(1).unwrap(), 900);
        // A new idle gesture does not inherit an old force-flush flag.
        c.flush_pending();
        c.advance(135);
        c.defer(&[(1, 800)]).unwrap();
        c.advance(1);
        assert!(c.generate().unwrap().is_none());
        c.advance(167);
        pump(&mut doc, &mut session, &mut c);
        assert_eq!(doc.value(1).unwrap(), 800);
    }
    #[test]
    fn dispatch_keeps_issued_replay_atomic_and_rebases_the_latest_unsent_value() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let mut c = client(1);
        let mut session = Session::default();
        pump(&mut doc, &mut session, &mut c);
        c.set_dispatch_policy(DispatchPolicy::new(100, 1).unwrap());
        c.defer(&[(1, 100)]).unwrap();
        let issued = c.generate().unwrap().unwrap();
        c.advance(1);
        c.defer(&[(1, 900)]).unwrap();
        assert_eq!(c.pending_count(), 2);
        assert!(c.generate().unwrap().is_none(), "one uncertain write only");
        c.reconnect();
        let replay = c.generate().unwrap().unwrap();
        assert_eq!(replay, issued, "issued values/sequence are immutable");
        doc.receive(&mut session, &replay).unwrap();
        c.receive(&doc.generate(&mut session).unwrap().unwrap())
            .unwrap();
        let ack = c.generate().unwrap().unwrap();
        assert!(matches!(
            decode_state_request(&ack).unwrap(),
            StateRequest::Ack { .. }
        ));
        // The configured field threshold flushes the whole queued assignment.
        let latest = c.generate().unwrap().unwrap();
        assert!(
            matches!(decode_state_request(&latest).unwrap(), StateRequest::Write { sequence: 2, values } if values == vec![(1,Value::Number(900.))])
        );
        assert!(DispatchPolicy::new(1001, 1).is_err());
        assert!(DispatchPolicy::new(1, 0).is_err());
        assert!(DispatchPolicy::new(1, MAX_FIELDS + 1).is_err());
    }
    #[test]
    fn ram_pending_writes_coalesce_and_receipts_survive_durable_reload() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let mut c = client(1);
        let mut s = Session::default();
        pump(&mut doc, &mut s, &mut c);
        c.set(&[(1, 100)]).unwrap();
        c.set(&[(1, 700)]).unwrap();
        assert_eq!(doc.value(1).unwrap(), 0);
        assert_eq!(c.value(1).unwrap(), 700);
        assert!(!c.confirmed());
        pump(&mut doc, &mut s, &mut c);
        assert_eq!(doc.value(1).unwrap(), 700);
        assert_eq!(doc.receipt(&[1; 16]).unwrap().unwrap().1.sequence, 1);
        let restored = Document::load(schema(), "test", &doc.save().unwrap()).unwrap();
        assert_eq!(restored.value(1).unwrap(), 700);
        assert_eq!(restored.receipt(&[1; 16]).unwrap().unwrap().1.sequence, 1);
    }
    #[test]
    fn lost_ack_reconnect_does_not_replay_over_a_newer_peer_write() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let mut a = client(1);
        let mut b = client(2);
        let mut sa = Session::default();
        let mut sb = Session::default();
        pump(&mut doc, &mut sa, &mut a);
        pump(&mut doc, &mut sb, &mut b);
        a.set(&[(1, 300)]).unwrap();
        let intent = a.generate().unwrap().unwrap();
        doc.receive(&mut sa, &intent).unwrap();
        assert!(!a.confirmed());
        b.set(&[(1, 900)]).unwrap();
        pump(&mut doc, &mut sb, &mut b);
        assert_eq!(doc.value(1).unwrap(), 900);
        let persisted = doc.save().unwrap();
        let mut restarted = Document::load(schema(), "test", &persisted).unwrap();
        let (mut new_session, _) = restarted
            .open_session(&[1; 16], a.resume_sequence())
            .unwrap();
        let mount = restarted.generate(&mut new_session).unwrap().unwrap();
        let mut resumed = StateAdapter::new(schema(), "test", &mount, [1; 16]).unwrap();
        resumed.restore_pending(&a).unwrap();
        assert_eq!(resumed.value(1).unwrap(), 900);
        assert!(resumed.confirmed());
        pump(&mut restarted, &mut new_session, &mut resumed);
        assert_eq!(restarted.value(1).unwrap(), 900);
        assert!(!doc.receive(&mut sa, &intent).unwrap());
        assert_eq!(doc.value(1).unwrap(), 900);
    }
    #[test]
    fn unaccepted_flight_and_a_newer_ram_edit_rebase_without_dropping_either() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let mut c = client(1);
        let mut s = Session::default();
        pump(&mut doc, &mut s, &mut c);
        c.set(&[(1, 200)]).unwrap();
        let _lost = c.generate().unwrap().unwrap();
        c.set(&[(1, 800)]).unwrap();
        let (mut s, _) = doc.open_session(&[1; 16], c.resume_sequence()).unwrap();
        let mount = doc.generate(&mut s).unwrap().unwrap();
        let mut next = StateAdapter::new(schema(), "test", &mount, [1; 16]).unwrap();
        next.restore_pending(&c).unwrap();
        assert_eq!(next.value(1).unwrap(), 800);
        pump(&mut doc, &mut s, &mut next);
        assert_eq!(doc.value(1).unwrap(), 800);
        assert_eq!(doc.receipt(&[1; 16]).unwrap().unwrap().1.sequence, 2);
    }
    #[test]
    fn all_policy_checks_precede_mutation_including_replayed_or_malformed_batches() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let mut c = client(1);
        let mut s = Session::default();
        pump(&mut doc, &mut s, &mut c);
        let old = doc.save().unwrap();
        for values in [
            vec![(1, Value::Number(200.)), (3, Value::Boolean(true))],
            vec![(1, Value::Text("wrong".into()))],
            vec![(1, Value::Number(1001.))],
            vec![(2, Value::Text("x".repeat(MAX_TEXT_BYTES + 1)))],
            vec![(1, Value::Number(1.)), (1, Value::Number(2.))],
            vec![],
        ] {
            let data = encode_state_request(&StateRequest::Write {
                sequence: 1,
                values,
            })
            .unwrap();
            assert!(doc.receive(&mut s, &data).is_err());
            assert_eq!(doc.save().unwrap(), old);
        }
        assert!(
            doc.receive(
                &mut s,
                &encode_state_request(&StateRequest::Hello {
                    writer: vec![2; 16],
                    resume: 0
                })
                .unwrap()
            )
            .is_err()
        );
        c.set(&[(1, 500)]).unwrap();
        let good = c.generate().unwrap().unwrap();
        doc.receive(&mut s, &good).unwrap();
        let committed = doc.save().unwrap();
        let bad = encode_state_request(&StateRequest::Write {
            sequence: 1,
            values: vec![(1, Value::Number(1.))],
        })
        .unwrap();
        assert!(doc.receive(&mut s, &bad).is_err());
        assert_eq!(doc.save().unwrap(), committed);
    }
    #[test]
    fn journal_replay_is_root_chained_and_only_changed_nodes_are_transferred() {
        let mut doc = Document::new(schema(), "test").unwrap();
        let checkpoint = doc.clone();
        let mut c = client(1);
        let mut s = Session::default();
        pump(&mut doc, &mut s, &mut c);
        let admission = doc.journal_delta(&checkpoint).unwrap();
        let before = doc.clone();
        c.set(&[(1, 600)]).unwrap();
        pump(&mut doc, &mut s, &mut c);
        let record = doc.journal_delta(&before).unwrap();
        assert!(record.len() < doc.save().unwrap().len());
        let mut restored = checkpoint;
        restored.replay(&admission).unwrap();
        restored.replay(&record).unwrap();
        assert_eq!(restored.root_hash(), doc.root_hash());
        assert!(restored.replay(&record).is_err());
        let old = restored.root_hash();
        let mut corrupt = record;
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(restored.replay(&corrupt).is_err());
        assert_eq!(restored.root_hash(), old);
    }
    #[test]
    fn signals_are_bounded_by_server_time_and_expire_without_writing_state() {
        let mut doc = Document::new(schema(), "test").unwrap();
        doc.set_signal_fields(&[1]).unwrap();
        for t in 1..=400 {
            doc.set_wall_time(t);
            doc.set(&[(1, if t % 2 == 0 { 1000 } else { 0 })]).unwrap();
        }
        let signal = doc.signal(1, 400, 300);
        assert!(signal.len() <= 300);
        assert!(signal.len() > 290);
        let before = doc.root_hash();
        assert!(doc.signal(1, 701, 300).is_empty());
        assert_eq!(doc.root_hash(), before);
    }
    #[test]
    fn unrequested_history_is_absent_and_retention_changes_do_not_mutate_old_samples() {
        let mut doc = Document::new(schema(), "test").unwrap();
        doc.set_wall_time(1);
        doc.set(&[(1, 100)]).unwrap();
        assert!(doc.tree.subtree(&[3]).is_err());
        doc.set_signal_fields(&[1, 3]).unwrap();
        doc.set_wall_time(2);
        doc.set(&[(1, 200)]).unwrap();
        doc.set_typed_authority(&[(3, Value::Boolean(true))])
            .unwrap();
        assert_eq!(doc.signal(1, 2, 300).len(), 1);
        assert_eq!(doc.signal(3, 2, 300).len(), 1);
        // Failed policy updates are atomic; a text field cannot request a signal.
        assert!(doc.set_signal_fields(&[2]).is_err());
        doc.set_wall_time(3);
        doc.set(&[(1, 300)]).unwrap();
        assert_eq!(doc.signal(1, 3, 300).len(), 2);
        let old_history = doc.tree.subtree(&[3]).unwrap();
        let checkpoint = doc.save().unwrap();
        let mut restored = Document::load(schema(), "test", &checkpoint).unwrap();
        restored.set_wall_time(4);
        restored.set(&[(1, 400)]).unwrap();
        assert!(!old_history.shares_root(&restored.tree.subtree(&[3]).unwrap()));
        assert_eq!(
            old_history.hash(),
            restored.tree.subtree(&[3]).unwrap().hash()
        );
        doc.set_signal_fields(&[]).unwrap();
        doc.set_wall_time(4);
        doc.set(&[(1, 400)]).unwrap();
        assert!(old_history.shares_root(&doc.tree.subtree(&[3]).unwrap()));
        assert_eq!(doc.signal(1, 4, 300).len(), 2);
    }
    #[test]
    fn writer_capacity_and_expired_uncertain_replays_fail_explicitly() {
        let mut doc = Document::new(schema(), "test").unwrap();
        doc.wall = 100;
        let mut edits = Vec::new();
        for slot in 0..RECEIPTS {
            let token = (slot as u128).to_le_bytes();
            let receipt = Receipt {
                writer: hash(&token),
                sequence: 1,
                digest: vec![0; 32],
                time: 100,
            };
            edits.push(Edit {
                path: path(2, slot),
                schema: doc.extra_schema(2, slot),
                value: Some(bytes(&receipt).unwrap()),
            });
        }
        for chunk in edits.chunks(256) {
            doc.tree = doc.tree.edit(doc.actor, chunk).unwrap().0;
        }
        assert!(doc.open_session(&[255; 16], 0).is_err());
        doc.wall = RESUME_SECONDS + 101;
        assert!(doc.open_session(&0_u128.to_le_bytes(), 1).is_err());
        assert!(doc.open_session(&[255; 16], 0).is_ok());
    }
    #[test]
    fn trusted_legacy_import_preserves_values_but_network_rejects_legacy_bytes() {
        use automerge::{AutoCommit, ROOT, transaction::Transactable};
        let mut old = AutoCommit::new();
        old.put(ROOT, "scope", "test").unwrap();
        old.put(ROOT, "schema", encode_schema(&schema()).unwrap())
            .unwrap();
        for f in schema().fields {
            let value = if f.id == 1 {
                Value::Number(730.)
            } else {
                f.default
            };
            old.put(ROOT, format!("f{}", f.id), bytes(&value).unwrap())
                .unwrap();
        }
        let data = old.save();
        let doc = Document::import_legacy(schema(), "test", &data).unwrap();
        assert_eq!(doc.value(1).unwrap(), 730);
        assert!(Document::load(schema(), "test", &data).is_err());
        assert!(Document::import_legacy(schema(), "wrong", &data).is_err());
    }
}
