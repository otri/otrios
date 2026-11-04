// SPDX-License-Identifier: LGPL-3.0-only
//! One signed canvas container; authority-serialized, generation-guarded order.
use super::*;
use otrios_interfaces::oib::designer::{ObjectAddress, Owner};

impl Document {
    pub(super) fn oib_validate_structure(&self, operation: &InstanceOperation) -> Result<()> {
        let (generation, before, moving, prototype) = match operation {
            InstanceOperation::Insert {
                prototype,
                before,
                generation,
            } => (*generation, *before, None, Some(*prototype)),
            InstanceOperation::Reorder {
                instance,
                before,
                generation,
            } => (*generation, *before, Some(*instance), None),
            _ => return Err("Not a canvas structure operation".into()),
        };
        let status = self.oib_status()?;
        if generation != status.generation {
            return Err("Canvas changed; review order and retry".into());
        }
        if !status.conflicts.is_empty() || self.oib_draft_root()? != status.source_draft {
            return Err("Resolve the UI draft before changing structure".into());
        }
        let seed = self.oib_binding()?.seed;
        let doc = seed
            .designer
            .as_ref()
            .ok_or("Canvas authoring requires designer metadata")?;
        let root = doc
            .instances
            .iter()
            .find(|i| i.id == doc.presented_root)
            .ok_or("Missing presented canvas")?;
        let definition = doc
            .definitions
            .iter()
            .find(|d| d.id == root.definition)
            .ok_or("Missing canvas definition")?;
        let canvas = ObjectAddress {
            instance: root.id,
            references: vec![],
            node: definition.root,
        };
        let admits = |id: u64| {
            doc.instances.iter().any(|i| {
                i.id.0 == id
                    && i.owner == Owner::Document
                    && i.visual_parent.as_ref() == Some(&canvas)
            }) && seed
                .instance_surface
                .as_ref()
                .is_some_and(|s| s.prototypes.iter().any(|p| p.instance == id))
        };
        // A single roster cannot also order another container's children.
        if seed.instances.is_empty() || seed.instances.iter().any(|i| !admits(i.id)) {
            return Err("Signed factories do not share one admitted canvas".into());
        }
        if prototype.is_some_and(|p| !admits(p)) {
            return Err("Factory is not admitted in this canvas".into());
        }
        let allocations = self.oib_allocations()?;
        let live = self.oib_live(2)?;
        if self.oib_live(1)? != live || allocations.iter().any(|a| !live.contains(&a.id)) {
            return Err("Resolve pending canvas children first".into());
        }
        for id in moving.into_iter().chain(before) {
            if !live.contains(&id)
                || !allocations
                    .iter()
                    .any(|a| a.id == id && admits(a.prototype))
            {
                return Err("Destination and child must be live siblings in this canvas".into());
            }
        }
        if moving.is_some() && moving == before {
            return Err("A component cannot precede itself".into());
        }
        Ok(())
    }

    pub(super) fn oib_structure_write(
        &mut self,
        writer: Actor,
        operation: &InstanceOperation,
    ) -> Result<()> {
        let mut candidate = self.clone();
        let result = (|| {
            candidate.oib_validate_structure(operation)?;
            candidate.oib_instance_apply(writer, operation)?;
            // An already-satisfied ordering is still an accepted deliberate
            // retry. Clear a previous structural rejection without publishing
            // another graph generation or rewriting any property register.
            if candidate.tree.hash() == self.tree.hash() {
                let mut status = candidate.oib_status()?;
                status.error = None;
                candidate.change(
                    &[],
                    vec![Edit {
                        path: vec![4, 3],
                        schema: candidate.extra_schema(4, 3),
                        value: Some(bytes(&status)?),
                    }],
                )?;
            }
            let status = candidate.oib_status()?;
            if !status.conflicts.is_empty() || status.error.is_some() {
                return Err(status
                    .error
                    .unwrap_or_else(|| "Resolve UI conflicts first".into()));
            }
            candidate.oib_compiled(2)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.tree = candidate.tree,
            Err(error) => {
                // The outer authority transaction persists the rejection and its
                // ordinary writer receipt together. No failed identity/order or
                // property edits escape the disposable candidate.
                let mut status = self.oib_status()?;
                let mut diagnostic = format!("Structure rejected: {error}");
                while diagnostic.len() > 256 {
                    diagnostic.pop();
                }
                status.error = Some(diagnostic);
                self.change(
                    &[],
                    vec![Edit {
                        path: vec![4, 3],
                        schema: self.extra_schema(4, 3),
                        value: Some(bytes(&status)?),
                    }],
                )?;
            }
        }
        Ok(())
    }
}
