// SPDX-License-Identifier: LGPL-3.0-only
//! One canonical accepted structural source; no independent overlay roots.
use super::*;
use otrios_interfaces::oib::authoring::*;

impl Document {
    pub(super) fn oib_source(&self) -> Result<DocumentSource> {
        let data = payload(&self.tree, &[4, 4])?;
        if data.len() > MAX_DOCUMENT_BYTES {
            return Err("OIB document source budget".into());
        }
        let source: DocumentSource = decode(&data)?;
        if bytes(&source)? != data
            || source.instances.len() > MAX_ALLOCATIONS
            || source.definitions.len() > otrios_interfaces::oib::designer::MAX_DEFINITIONS
            || bytes(&source.definitions)?.len() > MAX_SOURCE_BYTES
            || bytes(&source.placements)?.len() > MAX_PLACEMENT_BYTES
            || bytes(&source.connections)?.len() > MAX_CONNECTION_BYTES
            || source.placements.len() > MAX_ALLOCATIONS
            || source.connections.len() > otrios_interfaces::oib::MAX_CELLS
            || source
                .placements
                .windows(2)
                .any(|w| w[0].instance >= w[1].instance)
            || source
                .connections
                .windows(2)
                .any(|w| (&w[0].object, w[0].property) >= (&w[1].object, w[1].property))
        {
            return Err("Noncanonical OIB document source".into());
        }
        for placement in &source.placements {
            super::super::super::oib::document::placement::validate_frame(placement.frame)?;
        }
        Ok(source)
    }
    pub(super) fn oib_source_edit(&self, change: impl FnOnce(&mut DocumentSource)) -> Result<Edit> {
        let mut source = self.oib_source()?;
        change(&mut source);
        let value = bytes(&source)?;
        if value.len() > MAX_DOCUMENT_BYTES {
            return Err("OIB document source budget".into());
        }
        Ok(Edit {
            path: vec![4, 4],
            schema: self.extra_schema(4, 4),
            value: Some(value),
        })
    }
}
