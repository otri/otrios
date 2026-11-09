// SPDX-License-Identifier: LGPL-3.0-only
//! Resolve verified immutable library bodies without changing authored identities.
//!
//! Signature/trust and package-root verification belong to library/bundle
//! admission. This layer validates the exact transported closure and resolves it
//! purely, so native and browser clients require no filesystem or network.
use super::super::{Result, err, hash};
use otrios_interfaces::oib::{
    Seed,
    designer::{ComponentDefinition, DefinitionId, MAX_AUTHORED_OBJECTS, MAX_DEFINITIONS},
    graph::{self, ImportBinding, LibraryLock},
    library::{ExportId, Graph, LibraryId, Revision},
};
use std::collections::{BTreeMap, BTreeSet};

fn ordered<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut previous = None;
    for value in values {
        if previous.as_ref().is_some_and(|old| *old >= value) {
            return false;
        }
        previous = Some(value);
    }
    true
}

fn closure(library: &Graph, export: ExportId) -> Result<BTreeSet<DefinitionId>> {
    fn visit(
        graph: &Graph,
        id: DefinitionId,
        active: &mut BTreeSet<DefinitionId>,
        seen: &mut BTreeSet<DefinitionId>,
    ) -> Result<()> {
        if !active.insert(id) {
            return Err("Cyclic library definition reference".into());
        }
        if active.len() > 8 {
            return Err("Library reference depth budget".into());
        }
        if seen.insert(id) {
            let definition = graph
                .definitions
                .iter()
                .find(|definition| definition.id == id)
                .ok_or("Missing library definition")?;
            for node in &definition.nodes {
                if let Some(reference) = &node.reference {
                    visit(graph, reference.definition, active, seen)?;
                }
            }
        }
        active.remove(&id);
        Ok(())
    }
    let export = library
        .exports
        .iter()
        .find(|item| item.id == export)
        .ok_or("Missing locked library export")?;
    let mut seen = BTreeSet::new();
    visit(library, export.definition, &mut BTreeSet::new(), &mut seen)?;
    Ok(seen)
}

/// Document-local immutable definition IDs supplied by explicit import maps.
/// This query performs no resolution and never infers IDs from display order.
pub fn imported_definitions(seed: &Seed) -> BTreeSet<DefinitionId> {
    seed.graph
        .as_ref()
        .and_then(|graph| graph.imports.as_ref())
        .into_iter()
        .flat_map(|imports| &imports.bindings)
        .flat_map(|binding| &binding.definitions)
        .map(|item| item.target)
        .collect()
}

/// Structural source contains only local definitions. Imported defaults and
/// structure are selected by a verified pin, never copied into DocumentSource.
pub fn validate_local_definitions(seed: &Seed, definitions: &[ComponentDefinition]) -> Result<()> {
    let imported = imported_definitions(seed);
    if definitions
        .iter()
        .any(|definition| imported.contains(&definition.id))
    {
        return Err(
            "Imported library definitions are read-only; create a new local component".into(),
        );
    }
    Ok(())
}

fn lock<'a>(locks: &'a [LibraryLock], library: &LibraryId) -> Result<&'a LibraryLock> {
    locks
        .iter()
        .find(|lock| &lock.library == library)
        .ok_or_else(|| "Missing exact library lock".into())
}

fn validate_bindings(imports: &graph::Imports) -> Result<()> {
    if imports.format != graph::IMPORTS_FORMAT
        || imports.locks.is_empty()
        || imports.locks.len() > graph::MAX_LIBRARIES
        || imports.bindings.is_empty()
        || imports.bindings.len() > MAX_DEFINITIONS
        || !ordered(imports.locks.iter().map(|lock| &lock.library))
        || !ordered(imports.bindings.iter().map(|binding| binding.id))
    {
        return Err("Invalid or oversized OIB imports".into());
    }
    let mut required = BTreeSet::new();
    for pin in &imports.locks {
        if !crate::library::valid_id(&pin.library)
            || pin.exports.is_empty()
            || pin.exports.len() > MAX_DEFINITIONS
            || !ordered(pin.exports.iter().map(|export| export.export))
        {
            return Err("Noncanonical library export lock".into());
        }
        for export in &pin.exports {
            required.insert((pin.library.clone(), export.export));
        }
    }
    let mut bound = BTreeSet::new();
    for binding in &imports.bindings {
        if binding.id == 0
            || binding.definitions.is_empty()
            || binding.definitions.len() > MAX_DEFINITIONS
            || binding
                .definitions
                .iter()
                .any(|item| item.source.0 == 0 || item.target.0 == 0)
            || !ordered(binding.definitions.iter().map(|item| item.source))
            || !bound.insert((binding.library.clone(), binding.export))
        {
            return Err("Noncanonical library definition binding".into());
        }
        lock(&imports.locks, &binding.library)?;
    }
    if required != bound {
        return Err("Library locks and bound exports differ".into());
    }
    Ok(())
}

/// Resolve the canonical app source and its separately transported verified
/// imports. The returned graph is transient compiler input; never serialize it
/// as app-authored definitions or replace the source Seed with this projection.
pub fn resolve(seed: &Seed) -> Result<graph::Document> {
    let _profile = crate::canvas::profiling::span("oib.imports.resolve");
    let original = seed
        .graph
        .as_ref()
        .ok_or("Canonical OIB graph is required")?;
    let mut result = original.clone();
    let Some(imports) = &original.imports else {
        if !seed.resolved_libraries.is_empty() {
            return Err("Resolved libraries require an exact import lock".into());
        }
        return Ok(result);
    };
    validate_bindings(imports)?;
    validate_local_definitions(seed, &original.definitions)?;
    if seed.resolved_libraries.len() != imports.locks.len()
        || !ordered(seed.resolved_libraries.iter().map(|body| &body.library))
    {
        return Err("Missing, duplicate or noncanonical resolved library bodies".into());
    }
    let mut imported_bytes = 0usize;
    for body in &seed.resolved_libraries {
        let pin = lock(&imports.locks, &body.library)?;
        if pin.revision != body.revision {
            return Err("Resolved library revision differs from its exact lock".into());
        }
        crate::library::validate_graph(&body.graph)?;
        crate::library::required_classes_compatible(
            &body.graph.required_classes,
            &original.classes,
        )?;
        imported_bytes = imported_bytes
            .checked_add(flexbuffers::to_vec(&body.graph).map_err(err)?.len())
            .ok_or("Imported library byte count overflow")?;
        if imported_bytes > graph::MAX_IMPORTED_BYTES {
            return Err("Imported library source budget".into());
        }
        for expected in &pin.exports {
            let export = body
                .graph
                .exports
                .iter()
                .find(|export| export.id == expected.export)
                .ok_or("Missing locked library export")?;
            if expected.contract != export.contract
                || crate::library::export_contract(&body.graph, expected.export)?
                    != expected.contract
            {
                return Err("Library export contract differs from the app lock".into());
            }
        }
    }
    let mut source_targets = BTreeMap::new();
    let mut targets: BTreeMap<DefinitionId, (LibraryId, DefinitionId)> = original
        .definitions
        .iter()
        .map(|definition| (definition.id, (LibraryId(String::new()), definition.id)))
        .collect();
    for binding in &imports.bindings {
        let body = seed
            .resolved_libraries
            .iter()
            .find(|body| body.library == binding.library)
            .ok_or("Missing bound library body")?;
        let reachable = closure(&body.graph, binding.export)?;
        if reachable != binding.definitions.iter().map(|item| item.source).collect() {
            return Err("Import definition map must name the complete exact export closure".into());
        }
        for item in &binding.definitions {
            let source = (binding.library.clone(), item.source);
            if source_targets
                .insert(source.clone(), item.target)
                .is_some_and(|old| old != item.target)
            {
                return Err(
                    "One library definition cannot acquire another document identity".into(),
                );
            }
            if targets
                .insert(item.target, source.clone())
                .is_some_and(|old| old != source)
            {
                return Err("Imported definition identity collides with another source".into());
            }
        }
    }
    for ((library, source), target) in source_targets {
        let body = &seed
            .resolved_libraries
            .iter()
            .find(|body| body.library == library)
            .ok_or("Missing imported library")?
            .graph;
        let mut definition = body
            .definitions
            .iter()
            .find(|definition| definition.id == source)
            .ok_or("Missing imported definition")?
            .clone();
        definition.id = target;
        for node in &mut definition.nodes {
            if let Some(reference) = &mut node.reference {
                reference.definition = *targets
                    .iter()
                    .find_map(|(target, identity)| {
                        (identity == &(library.clone(), reference.definition)).then_some(target)
                    })
                    .ok_or("Incomplete imported reference map")?;
            }
        }
        result.definitions.push(definition);
    }
    result.definitions.sort_by_key(|definition| definition.id);
    if result.definitions.len() > MAX_DEFINITIONS
        || result
            .definitions
            .iter()
            .map(|definition| definition.nodes.len())
            .sum::<usize>()
            > MAX_AUTHORED_OBJECTS
    {
        return Err("Resolved definitions exceed the aggregate app graph budget".into());
    }
    Ok(result)
}

/// Exact dependency lock digest for durable expected-baseline adoption. This is
/// a content selector, never a signature or permission to load package bytes.
pub fn lock_digest(seed: &Seed) -> Result<Revision> {
    let graph = seed
        .graph
        .as_ref()
        .ok_or("Canonical OIB graph is required")?;
    let mut canonical = b"OTriOS\0oib-library-lock-v1\0".to_vec();
    canonical.extend(flexbuffers::to_vec(&graph.imports).map_err(err)?);
    hash(&canonical)
        .try_into()
        .map_err(|_| "Invalid SHA-256 length".into())
}

fn stable_bindings(old: &[ImportBinding], new: &[ImportBinding]) -> bool {
    old == new
}

/// Admit a library-only baseline update under exactly the same app source,
/// namespace bindings and exposed semantic contracts. No library structure is
/// silently adopted as authored local edits and no app grant is expanded.
pub fn compatible(previous: &Seed, candidate: &Seed) -> Result<()> {
    resolve(previous)?;
    resolve(candidate)?;
    let old = previous.graph.as_ref().ok_or("Missing previous graph")?;
    let new = candidate.graph.as_ref().ok_or("Missing candidate graph")?;
    let mut old_local = old.clone();
    old_local.imports = None;
    let mut new_local = new.clone();
    new_local.imports = None;
    if old_local != new_local {
        return Err("Library adoption cannot change app-authored source or grants".into());
    }
    let (Some(old), Some(new)) = (&old.imports, &new.imports) else {
        return Err("Library adoption requires existing stable import bindings".into());
    };
    if !stable_bindings(&old.bindings, &new.bindings) || old.locks.len() != new.locks.len() {
        return Err("Library adoption cannot replace an import binding or export identity".into());
    }
    for (old, new) in old.locks.iter().zip(&new.locks) {
        if old.library != new.library || old.exports != new.exports {
            return Err("Incompatible library export contract".into());
        }
        if old.revision == new.revision {
            let previous_body = previous
                .resolved_libraries
                .iter()
                .find(|body| body.library == old.library)
                .ok_or("Missing previous library body")?;
            let candidate_body = candidate
                .resolved_libraries
                .iter()
                .find(|body| body.library == new.library)
                .ok_or("Missing candidate library body")?;
            if previous_body.graph != candidate_body.graph {
                return Err(
                    "One exact library revision cannot name different canonical bodies".into(),
                );
            }
        }
    }
    Ok(())
}
