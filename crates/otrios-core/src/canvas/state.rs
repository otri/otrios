// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use automerge::{
    ActorId, AutoCommit, ROOT, ReadDoc, ScalarValue, sync::SyncDoc, transaction::Transactable,
};
#[derive(Clone, Default)]
pub struct Session(pub automerge::sync::State);
#[derive(Clone)]
pub struct Document {
    doc: AutoCommit,
    schema: StateSchema,
    seed: automerge::ChangeHash,
}
impl Document {
    pub fn new(schema: StateSchema, state_ref: &str) -> Result<Self> {
        validate_schema(&schema)?;
        if state_ref.is_empty() || state_ref.len() > 128 {
            return Err("Invalid State reference".into());
        }
        let mut doc =
            AutoCommit::new().with_actor(ActorId::from(b"otrios-canvas-seed-v1".to_vec()));
        doc.put(ROOT, "schema", encode_schema(&schema)?)
            .map_err(err)?;
        doc.put(ROOT, "scope", state_ref).map_err(err)?;
        for f in &schema.fields {
            doc.put(ROOT, format!("f{}", f.id), u64::from(f.default))
                .map_err(err)?;
        }
        let seed = doc.commit().ok_or("No seed")?;
        let doc = doc.fork();
        Ok(Self { doc, schema, seed })
    }
    pub fn load(schema: StateSchema, state_ref: &str, bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err("Canvas document limit".into());
        }
        let mut result = Self::new(schema, state_ref)?;
        let mut candidate = AutoCommit::load(bytes).map_err(err)?;
        result.validate(&mut candidate)?;
        result.doc = candidate.fork();
        Ok(result)
    }
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }
    pub fn value(&self, id: u16) -> Result<u16> {
        self.doc
            .get(ROOT, format!("f{id}"))
            .map_err(err)?
            .and_then(|(v, _)| v.as_u64())
            .filter(|v| *v <= 1000)
            .map(|v| v as u16)
            .ok_or_else(|| "Missing Canvas field".into())
    }
    pub fn set(&mut self, commands: &[(u16, u16)]) -> Result<bool> {
        for (id, v) in commands {
            if *v > 1000 || !self.schema.fields.iter().any(|f| f.id == *id) {
                return Err("State command outside schema".into());
            }
        }
        let mut changed = false;
        let mut next = self.doc.clone();
        for (id, v) in commands {
            if self.value(*id)? != *v {
                next.put(ROOT, format!("f{id}"), u64::from(*v))
                    .map_err(err)?;
                changed = true;
            }
        }
        if changed {
            next.commit();
            self.doc = next;
        }
        Ok(changed)
    }
    pub fn confirmed(&mut self, session: &Session) -> bool {
        let heads = self.doc.get_heads();
        heads.iter().all(|h| session.0.shared_heads.contains(h))
    }
    pub fn generate(&mut self, session: &mut Session) -> Result<Option<Vec<u8>>> {
        let b = self
            .doc
            .sync()
            .generate_sync_message(&mut session.0)
            .map(|m| m.encode());
        if b.as_ref().is_some_and(|b| b.len() > MAX_BYTES) {
            return Err("Sync history budget exceeded".into());
        }
        Ok(b)
    }
    pub fn receive(&mut self, session: &mut Session, bytes: &[u8]) -> Result<bool> {
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
        self.validate(&mut next)?;
        let changed = old != next.get_heads();
        self.doc = next;
        *session = sync;
        Ok(changed)
    }
    fn validate(&mut self, candidate: &mut AutoCommit) -> Result<()> {
        use automerge::legacy::{Key, ObjectId, OpType};
        if candidate.get_changes_meta(&[]).len() > 4096 {
            return Err("Canvas prototype history budget exhausted".into());
        }
        if candidate.get_change_meta_by_hash(&self.seed).is_none()
            || candidate
                .get_changes_meta(&[])
                .iter()
                .any(|c| c.deps.is_empty() && c.hash != self.seed)
        {
            return Err("Wrong Canvas history/scope".into());
        }
        for change in self.doc.get_changes_added(candidate) {
            if change.len() > 5 {
                return Err("Canvas change operation limit".into());
            }
            for op in change.decode().operations {
                let allowed = match (&op.obj, &op.key, &op.action, op.insert) {
                    (ObjectId::Root, Key::Map(key), OpType::Put(ScalarValue::Uint(v)), false) => {
                        *v <= 1000
                            && self
                                .schema
                                .fields
                                .iter()
                                .any(|f| key == &format!("f{}", f.id))
                    }
                    _ => false,
                };
                if !allowed {
                    return Err("Protected Canvas state operation".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> StateSchema {
        StateSchema {
            id: "test".into(),
            fields: vec![
                Field {
                    id: 1,
                    name: "level".into(),
                    default: 50,
                },
                Field {
                    id: 2,
                    name: "dial".into(),
                    default: 70,
                },
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
                b.receive(sb, &x)?;
            }
            if let Some(y) = y {
                a.receive(sa, &y)?;
            }
        }
        Err("sync stuck".into())
    }
    #[test]
    fn concurrent_registers_merge_and_reconnect_from_authority() {
        let mut a = Document::new(schema(), "a").unwrap();
        let mut b = Document::new(schema(), "a").unwrap();
        let snapshot = a.clone();
        a.set(&[(1, 900)]).unwrap();
        b.set(&[(2, 300)]).unwrap();
        sync(
            &mut a,
            &mut b,
            &mut Session::default(),
            &mut Session::default(),
        )
        .unwrap();
        assert_eq!((a.value(1).unwrap(), a.value(2).unwrap()), (900, 300));
        assert_eq!(snapshot.value(1).unwrap(), 50);
        let saved = a.save();
        let mut restored = Document::load(schema(), "a", &saved).unwrap();
        let mut client = Document::new(schema(), "a").unwrap();
        sync(
            &mut restored,
            &mut client,
            &mut Session::default(),
            &mut Session::default(),
        )
        .unwrap();
        assert_eq!(client.value(1).unwrap(), 900);
        assert!(Document::load(schema(), "wrong-scope", &saved).is_err());
    }
    #[test]
    fn authority_rejects_protected_operations_even_with_unchanged_visible_value() {
        let mut trusted = Document::new(schema(), "a").unwrap();
        let mut peer = trusted.clone();
        peer.doc.put(ROOT, "scope", "tampered").unwrap();
        peer.doc.commit();
        peer.doc.put(ROOT, "scope", "a").unwrap();
        peer.doc.commit();
        assert!(
            sync(
                &mut trusted,
                &mut peer,
                &mut Session::default(),
                &mut Session::default()
            )
            .is_err()
        );
        assert_eq!(trusted.value(1).unwrap(), 50);
    }
}
