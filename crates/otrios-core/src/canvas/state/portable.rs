// SPDX-License-Identifier: LGPL-3.0-only
//! Portable app-owned causal fields. Authority roots and receipts never cross hosts.
use super::*;
use otrios_interfaces::transfers::{
    CANVAS_CHECKPOINT_HEADER, CANVAS_CODEC, CanvasCheckpoint, MAX_CANVAS_CHECKPOINT_BYTES,
};

impl Document {
    /// Caller pins confirmed authority state and checks source admission. This
    /// carries neither permission nor the source authority's writer/root state.
    pub fn portable_checkpoint(&self) -> Result<Vec<u8>> {
        self.validate_tree()?;
        let checkpoint = CanvasCheckpoint {
            format: CANVAS_CODEC.into(),
            scope: self.scope.clone(),
            schema: self.schema.as_ref().clone(),
            fields: match self.tree.subtree(&[1]) {
                Ok(fields) => fields,
                Err(_) if self.schema.fields.is_empty() => Snapshot::empty(),
                Err(error) => return Err(err(error)),
            }
            .to_bytes()
            .map_err(err)?,
            signals: self
                .tree
                .subtree(&[3])
                .ok()
                .map(|tree| tree.to_bytes().map_err(err))
                .transpose()?,
        };
        let mut bytes = CANVAS_CHECKPOINT_HEADER.to_vec();
        bytes.extend(flexbuffers::to_vec(checkpoint).map_err(err)?);
        if bytes.len() > MAX_CANVAS_CHECKPOINT_BYTES {
            return Err("Portable Canvas checkpoint budget".into());
        }
        Ok(bytes)
    }

    /// Qualify against trusted installed schema/scope, never metadata chosen by
    /// the payload. The eventual Space decision supplies a fresh collection ID.
    pub fn from_portable_checkpoint(
        schema: StateSchema,
        scope: &str,
        bytes: &[u8],
    ) -> Result<Self> {
        if bytes.len() > MAX_CANVAS_CHECKPOINT_BYTES || !bytes.starts_with(CANVAS_CHECKPOINT_HEADER)
        {
            return Err("Portable Canvas checkpoint framing".into());
        }
        let bytes = &bytes[CANVAS_CHECKPOINT_HEADER.len()..];
        crate::flex_guard::validate(
            bytes,
            20,
            MAX_CANVAS_CHECKPOINT_BYTES * 2,
            MAX_CANVAS_CHECKPOINT_BYTES,
        )
        .map_err(err)?;
        let checkpoint: CanvasCheckpoint = flexbuffers::from_slice(bytes).map_err(err)?;
        if flexbuffers::to_vec(&checkpoint).map_err(err)? != bytes
            || checkpoint.format != CANVAS_CODEC
            || checkpoint.scope != scope
            || checkpoint.schema != schema
        {
            return Err("Portable Canvas checkpoint schema/canonical encoding".into());
        }
        let fields = Snapshot::from_bytes(&checkpoint.fields).map_err(err)?;
        // Do not let new-document defaults fill gaps in an incomplete export.
        for field in &schema.fields {
            fields
                .subtree(&path(1, usize::from(field.id) - 1)[1..])
                .map_err(err)?;
        }
        let mut result = Self::new(schema, scope)?;
        result.tree = result.tree.merge_subtree(&[1], &fields).map_err(err)?.0;
        if let Some(bytes) = checkpoint.signals {
            let signals = Snapshot::from_bytes(&bytes).map_err(err)?;
            result.tree = result.tree.merge_subtree(&[3], &signals).map_err(err)?.0;
        }
        result.validate_tree()?;
        // Imported payloads cannot create writer receipts or enable signal policy.
        if !result.receipts()?.is_empty() {
            return Err("Portable checkpoint imported admission state".into());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> StateSchema {
        StateSchema {
            id: "portable-test-v1".into(),
            fields: vec![
                Field {
                    id: 1,
                    name: "number".into(),
                    default: Value::Number(0.),
                    writable: true,
                },
                Field {
                    id: 2,
                    name: "text".into(),
                    default: Value::Text("".into()),
                    writable: true,
                },
            ],
        }
    }
    fn mutate(bytes: &[u8], change: impl FnOnce(&mut CanvasCheckpoint)) -> Vec<u8> {
        let mut value: CanvasCheckpoint = flexbuffers::from_slice(&bytes[8..]).unwrap();
        change(&mut value);
        let mut result = CANVAS_CHECKPOINT_HEADER.to_vec();
        result.extend(flexbuffers::to_vec(value).unwrap());
        result
    }
    #[test]
    fn portable_fields_and_signals_preserve_causality_but_renew_authority() {
        let mut source = Document::new(schema(), "app:test/state").unwrap();
        source.set_signal_fields(&[1]).unwrap();
        source.set_wall_time(100);
        source
            .set_typed(&[
                (1, Value::Number(42.)),
                (2, Value::Text("confirmed".into())),
            ])
            .unwrap();
        source.open_session(&[7; 16], 0).unwrap();
        assert!(!source.receipts().unwrap().is_empty());
        let bytes = source.portable_checkpoint().unwrap();
        let imported =
            Document::from_portable_checkpoint(schema(), source.scope(), &bytes).unwrap();
        assert_ne!(source.actor, imported.actor);
        assert_ne!(source.root_hash(), imported.root_hash());
        assert!(imported.receipts().unwrap().is_empty());
        assert_eq!(imported.signal_fields, 0);
        for kind in [1, 3] {
            assert_eq!(
                source.tree.subtree(&[kind]).unwrap().hash(),
                imported.tree.subtree(&[kind]).unwrap().hash()
            );
        }
        assert_eq!(
            imported.typed_value(2).unwrap(),
            Value::Text("confirmed".into())
        );
        assert_eq!(bytes, imported.portable_checkpoint().unwrap());
        assert!(Document::load(schema(), source.scope(), &imported.save().unwrap()).is_ok());
    }
    #[test]
    fn portable_rejects_incomplete_schema_wrong_scope_and_authority_injection() {
        let source = Document::new(schema(), "app:test/state").unwrap();
        let bytes = source.portable_checkpoint().unwrap();
        assert!(Document::from_portable_checkpoint(schema(), "other", &bytes).is_err());
        let mut other = schema();
        other.fields[0].writable = false;
        assert!(Document::from_portable_checkpoint(other, source.scope(), &bytes).is_err());
        for bad in [
            mutate(&bytes, |c| c.fields = Snapshot::empty().to_bytes().unwrap()),
            mutate(&bytes, |c| c.fields = source.tree.to_bytes().unwrap()),
            mutate(&bytes, |c| {
                c.signals = Some(source.tree.to_bytes().unwrap())
            }),
        ] {
            assert!(Document::from_portable_checkpoint(schema(), source.scope(), &bad).is_err());
        }
        let mut bad = bytes;
        bad[0] ^= 1;
        assert!(Document::from_portable_checkpoint(schema(), source.scope(), &bad).is_err());
    }
}
