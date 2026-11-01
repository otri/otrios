// SPDX-License-Identifier: LGPL-3.0-only
//! Collection compatibility covers every owned document, independently of UI.
use super::*;
use std::collections::BTreeMap;

pub fn collection_schema(roots: &[RootSchema]) -> Result<String> {
    if roots.is_empty() || roots.len() > MAX_ROOTS {
        return Err(Error::Capacity);
    }
    let mut canonical = BTreeMap::new();
    let mut total = 0_usize;
    for root in roots {
        if root.state_ref.is_empty()
            || root.state_ref.len() > 128
            || root.state_ref.chars().any(char::is_control)
        {
            return Err(Error::Invalid);
        }
        crate::canvas::validate_schema(&root.schema).map_err(|_| Error::Invalid)?;
        let bytes = crate::canvas::encode_schema(&root.schema).map_err(|_| Error::Invalid)?;
        if let Some(previous) = canonical.get(root.state_ref.as_str()) {
            if previous != &bytes {
                return Err(Error::Incompatible);
            }
            continue;
        }
        total = total.checked_add(bytes.len()).ok_or(Error::Capacity)?;
        if total > MAX_COLLECTION_SCHEMA_BYTES {
            return Err(Error::Capacity);
        }
        canonical.insert(root.state_ref.as_str(), bytes);
    }
    let payload = flexbuffers::to_vec(("otrios-space-app-schema-v1", canonical))
        .map_err(|_| Error::Invalid)?;
    if payload.len() > MAX_COLLECTION_SCHEMA_BYTES {
        return Err(Error::Capacity);
    }
    Ok(crate::canvas::hex(&crate::canvas::hash(&payload)))
}
