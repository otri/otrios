// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded endpoint-removal evidence. Final candidate admission remains authority.
use super::impact::{Kind, Use};
use super::*;
use otrios_interfaces::oib::{
    authoring::{Destination, DocumentSource},
    designer::*,
    graph::{self, ParameterSource},
};

const MAX_ITEMS: usize = MAX_AUTHORED_OBJECTS * 8;
/// Exact local endpoint ownership for pending editor buffers and literal writes.
/// Immutable imports cannot point back into the document's local namespace.
pub(in crate::canvas) fn addresses_endpoint(
    source: &DocumentSource,
    address: &PropertyAddress,
    definition: DefinitionId,
    property: PropertyId,
) -> bool {
    let Some(target) = source.definitions.iter().find(|d| d.id == definition) else {
        return false;
    };
    match address {
        PropertyAddress::Structural {
            target:
                StructuralTarget::Property {
                    definition: owner,
                    property: id,
                },
            ..
        } => *owner == definition && *id == property,
        PropertyAddress::Definition {
            definition: owner,
            node,
            property: id,
        } => *owner == definition && *node == target.root && *id == property,
        PropertyAddress::Occurrence { instance, path } => {
            if path.property != property || path.references.len() > MAX_REFERENCE_DEPTH {
                return false;
            }
            let Some(instance) = source.instances.iter().find(|i| i.id == instance.0) else {
                return false;
            };
            let mut current = DefinitionId(instance.component);
            for boundary in &path.references {
                let Some(next) = source
                    .definitions
                    .iter()
                    .find(|d| d.id == current)
                    .and_then(|d| d.nodes.iter().find(|n| n.id == *boundary))
                    .and_then(|n| n.reference.as_ref())
                else {
                    return false;
                };
                current = next.definition;
            }
            current == definition && path.node == target.root
        }
        _ => false,
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub target: DefinitionId,
    pub property: PropertyId,
    pub name: String,
    pub scope: &'static str,
    pub complete: bool,
    pub readonly: bool,
    pub items: Vec<Use>,
    pub blockers: Vec<String>,
}
impl Report {
    pub(crate) fn can_remove(&self) -> bool {
        self.complete && !self.readonly && !self.items.iter().any(|i| i.blocking)
    }
    fn incomplete(&mut self, reason: impl Into<String>) {
        self.complete = false;
        let reason = reason.into();
        if !self.blockers.contains(&reason) && self.blockers.len() < MAX_DEFINITIONS {
            self.blockers.push(reason);
        }
    }
    fn row(
        &mut self,
        kind: Kind,
        label: String,
        detail: &str,
        blocking: bool,
        selection: Option<Selection>,
    ) {
        if self.items.len() >= MAX_ITEMS {
            self.incomplete("Endpoint impact exceeds admitted display capacity");
            return;
        }
        self.items.push(Use {
            kind,
            label,
            detail: detail.into(),
            direct: true,
            blocking,
            selection,
        });
    }
    fn finish(mut self) -> Self {
        if !self.complete {
            for reason in self.blockers.clone() {
                if self.items.len() >= MAX_ITEMS {
                    break;
                }
                self.items.push(Use {
                    kind: Kind::Unknown,
                    label: "Incomplete endpoint evidence".into(),
                    detail: reason,
                    direct: false,
                    blocking: true,
                    selection: None,
                });
            }
        }
        self
    }
}
fn definition(graph: &graph::Document, id: DefinitionId) -> Result<&ComponentDefinition> {
    let mut found = graph.definitions.iter().filter(|d| d.id == id);
    let value = found
        .next()
        .ok_or("Endpoint path has an unavailable definition")?;
    if found.next().is_some() {
        return Err("Endpoint path has an ambiguous definition".into());
    }
    Ok(value)
}
fn node(definition: &ComponentDefinition, id: NodeId) -> Result<&DefinitionNode> {
    let mut found = definition.nodes.iter().filter(|n| n.id == id);
    let value = found
        .next()
        .ok_or("Endpoint path has an unavailable node")?;
    if found.next().is_some() {
        return Err("Endpoint path has an ambiguous node".into());
    }
    Ok(value)
}
fn terminal<'a>(
    graph: &'a graph::Document,
    origin: DefinitionId,
    references: &[NodeId],
    end: NodeId,
) -> Result<(&'a ComponentDefinition, &'a DefinitionNode)> {
    if references.len() > MAX_REFERENCE_DEPTH {
        return Err("Endpoint reference path exceeds admission".into());
    }
    let mut current = definition(graph, origin)?;
    let mut seen = BTreeSet::from([origin]);
    for boundary in references {
        let reference = node(current, *boundary)?
            .reference
            .as_ref()
            .ok_or("Endpoint path has an unavailable reference")?;
        if !seen.insert(reference.definition) {
            return Err("Endpoint reference path is cyclic".into());
        }
        current = definition(graph, reference.definition)?;
    }
    Ok((current, node(current, end)?))
}
fn path_uses(
    graph: &graph::Document,
    origin: DefinitionId,
    path: &PropertyPath,
    target: DefinitionId,
    property: PropertyId,
) -> Result<bool> {
    let (def, end) = terminal(graph, origin, &path.references, path.node)?;
    let mut descriptors = document::class_descriptors(&graph.classes, end.class)?;
    if end.id == def.root {
        descriptors.extend(def.properties.iter().cloned());
    }
    if descriptors.iter().filter(|p| p.id == path.property).count() != 1 {
        return Err("Endpoint path has an unavailable or ambiguous property".into());
    }
    Ok(def.id == target && end.id == def.root && path.property == property)
}
fn use_path(
    report: &mut Report,
    graph: &graph::Document,
    origin: DefinitionId,
    path: &PropertyPath,
    label: String,
    detail: &str,
    selection: Selection,
) {
    match path_uses(graph, origin, path, report.target, report.property) {
        Ok(true) => report.row(Kind::Api, label, detail, true, Some(selection)),
        Ok(false) => {}
        Err(reason) => report.incomplete(reason),
    }
}
fn validate_graph(graph: &graph::Document) -> Result<()> {
    if graph.definitions.len() > MAX_DEFINITIONS
        || graph.classes.len() > MAX_CLASS_DESCRIPTORS
        || graph
            .definitions
            .iter()
            .map(|d| d.nodes.len())
            .sum::<usize>()
            > MAX_AUTHORED_OBJECTS
        || graph
            .definitions
            .iter()
            .map(|d| d.id)
            .collect::<BTreeSet<_>>()
            .len()
            != graph.definitions.len()
        || graph
            .classes
            .iter()
            .map(|c| c.id)
            .collect::<BTreeSet<_>>()
            .len()
            != graph.classes.len()
    {
        return Err("Endpoint dependency source exceeds definition/object admission or has duplicate identities".into());
    }
    document::validate_declarations(&graph.classes, &graph.definitions)?;
    fn visit(
        graph: &graph::Document,
        id: DefinitionId,
        active: &mut BTreeSet<DefinitionId>,
        done: &mut BTreeMap<DefinitionId, usize>,
    ) -> Result<usize> {
        if active.contains(&id) {
            return Err("Endpoint dependencies contain a component cycle".into());
        }
        if let Some(depth) = done.get(&id) {
            return Ok(*depth);
        }
        if active.len() >= MAX_REFERENCE_DEPTH {
            return Err("Endpoint component dependency depth exceeds admission".into());
        }
        active.insert(id);
        let def = definition(graph, id)?;
        node(def, def.root)?;
        if def
            .nodes
            .iter()
            .map(|n| n.id)
            .collect::<BTreeSet<_>>()
            .len()
            != def.nodes.len()
            || def.properties.len() > 64
            || def
                .properties
                .iter()
                .map(|p| p.id)
                .collect::<BTreeSet<_>>()
                .len()
                != def.properties.len()
            || def.exposed.len() > 64
            || def
                .exposed
                .iter()
                .map(|e| e.property)
                .collect::<BTreeSet<_>>()
                .len()
                != def.exposed.len()
            || def.slots.len() > 64
            || def
                .slots
                .iter()
                .map(|s| s.id)
                .collect::<BTreeSet<_>>()
                .len()
                != def.slots.len()
        {
            return Err(
                "Endpoint dependency source has ambiguous or oversized declarations".into(),
            );
        }
        let mut depth = 1;
        for n in &def.nodes {
            document::class_descriptors(&graph.classes, n.class)?;
            if n.defaults.len() > 64 || n.visual.as_ref().is_some_and(|v| v.parameters.len() > 16) {
                return Err("Endpoint node source exceeds admission".into());
            }
            if let Some(reference) = &n.reference {
                if reference.assignments.len() > 64 {
                    return Err("Endpoint reference assignment budget".into());
                }
                depth = depth.max(1 + visit(graph, reference.definition, active, done)?);
            }
        }
        active.remove(&id);
        if depth > MAX_REFERENCE_DEPTH {
            return Err("Endpoint component dependency depth exceeds admission".into());
        }
        done.insert(id, depth);
        Ok(depth)
    }
    let mut done = BTreeMap::new();
    for def in &graph.definitions {
        visit(graph, def.id, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}
/// Remove only an explicitly declared root endpoint. Its own fallback and
/// outgoing Exposed row are owned removals; other routes require explicit repair.
pub(crate) fn endpoint(
    seed: &Seed,
    source: &DocumentSource,
    target: DefinitionId,
    property: PropertyId,
) -> Report {
    let mut report = Report {
        target,
        property,
        name: format!("Property {}", property.0),
        scope: "This document only; other documents are not checked.",
        complete: true,
        readonly: false,
        items: vec![],
        blockers: vec![],
    };
    let Some(authored) = &seed.graph else {
        report.incomplete("Canonical endpoint source is unavailable");
        return report.finish();
    };
    if authored.definitions != source.definitions || authored.presets != source.presets {
        report.incomplete("Endpoint source does not match the current component candidate");
    }
    let graph = match imports::resolve(seed) {
        Ok(graph) => graph,
        Err(reason) => {
            report.incomplete(reason);
            authored.clone()
        }
    };
    if let Err(reason) = validate_graph(&graph) {
        report.incomplete(reason);
        return report.finish();
    }
    if source.instances.len() > MAX_ALLOCATIONS
        || source.connections.len() > MAX_CELLS
        || source.placements.len() > MAX_ALLOCATIONS
        || source.presets.len() > MAX_DEFINITIONS
        || source
            .instances
            .iter()
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            .len()
            != source.instances.len()
        || source
            .presets
            .iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>()
            .len()
            != source.presets.len()
    {
        report.incomplete(
            "Endpoint instance or preset source exceeds admission or has duplicate identities",
        );
        return report.finish();
    }
    let selected = match definition(&graph, target) {
        Ok(d) => d,
        Err(reason) => {
            report.incomplete(reason);
            return report.finish();
        }
    };
    let declared = selected.properties.iter().find(|p| p.id == property);
    if let Some(descriptor) = declared {
        report.name = format!("{} / {}", selected.name, descriptor.name);
    } else {
        let inherited = node(selected, selected.root)
            .and_then(|root| document::class_descriptors(&graph.classes, root.class));
        match inherited {
            Ok(fields) if fields.iter().any(|p| p.id == property) => {
                report.readonly = true;
                report.row(
                    Kind::Ownership,
                    selected.name.clone(),
                    "Inherited class properties cannot be removed from a component",
                    true,
                    Some(Selection::Definition(target)),
                );
            }
            Ok(_) => report.incomplete("Declared endpoint is unavailable"),
            Err(reason) => report.incomplete(reason),
        }
        return report.finish();
    }
    if imports::imported_definitions(seed).contains(&target) {
        report.readonly = true;
        report.row(
            Kind::Export,
            selected.name.clone(),
            "This endpoint belongs to an imported component",
            true,
            Some(Selection::Definition(target)),
        );
    }
    report.row(
        Kind::Api,
        report.name.clone(),
        "Declared endpoint will be removed; its component remains available",
        false,
        Some(Selection::Definition(target)),
    );
    if let Ok(root) = node(selected, selected.root)
        && root.defaults.iter().any(|d| d.property == property)
    {
        report.row(
            Kind::Api,
            report.name.clone(),
            "This endpoint's authored fallback will be removed",
            false,
            Some(Selection::Definition(target)),
        );
    }
    for def in &graph.definitions {
        for n in &def.nodes {
            let selection = Selection::DefinitionNode {
                definition: def.id,
                node: n.id,
            };
            if let Some(reference) = &n.reference {
                for assignment in &reference.assignments {
                    use_path(
                        &mut report,
                        &graph,
                        reference.definition,
                        &assignment.target,
                        format!("{} / {} assignment", def.name, n.name),
                        "Reference assignment targets this endpoint",
                        selection.clone(),
                    );
                    if let AssignmentSource::Forward(path) = &assignment.source {
                        use_path(
                            &mut report,
                            &graph,
                            def.id,
                            path,
                            format!("{} / {} source", def.name, n.name),
                            "Reference assignment forwards from this endpoint",
                            selection.clone(),
                        );
                    }
                }
            }
            if let Some(visual) = &n.visual {
                for parameter in &visual.parameters {
                    if let ParameterSource::Property(path) = &parameter.source {
                        use_path(
                            &mut report,
                            &graph,
                            def.id,
                            path,
                            format!("{} / {} visual", def.name, n.name),
                            "Visual parameter reads this endpoint",
                            selection.clone(),
                        );
                    }
                }
                if let Some(action) = &visual.action {
                    use_path(
                        &mut report,
                        &graph,
                        def.id,
                        &action.source,
                        format!("{} / {} action", def.name, n.name),
                        "Input action reads this endpoint",
                        selection.clone(),
                    );
                }
            }
        }
        for exposed in &def.exposed {
            let own = def.id == target && exposed.property == property;
            let source = PropertyPath {
                references: vec![],
                node: def.root,
                property: exposed.property,
            };
            if let Err(reason) = path_uses(&graph, def.id, &source, target, property) {
                report.incomplete(reason);
            }
            if exposed.targets.len() > 64 {
                report.incomplete("Endpoint forwarding target budget");
            }
            for path in &exposed.targets {
                match path_uses(&graph, def.id, path, target, property) {
                    Ok(true) if !own => report.row(
                        Kind::Api,
                        format!("{} / property {}", def.name, exposed.property.0),
                        "Another endpoint forwards to this endpoint",
                        true,
                        Some(Selection::Definition(def.id)),
                    ),
                    Ok(_) => {}
                    Err(reason) => report.incomplete(reason),
                }
            }
            if own {
                report.row(
                    Kind::Api,
                    report.name.clone(),
                    "This endpoint's outgoing forwarding routes will be removed",
                    false,
                    Some(Selection::Definition(target)),
                );
            }
        }
        for slot in &def.slots {
            if let Err(reason) = terminal(&graph, def.id, &slot.references, slot.node) {
                report.incomplete(reason);
            }
        }
    }
    for preset in &source.presets {
        if let Err(reason) = definition(&graph, preset.definition) {
            report.incomplete(reason);
        }
        if preset.assignments.len() > 64 {
            report.incomplete("Endpoint preset assignment budget");
        }
        for assignment in &preset.assignments {
            use_path(
                &mut report,
                &graph,
                preset.definition,
                &assignment.target,
                format!("{} preset", preset.name),
                "Preset assigns this endpoint",
                Selection::Definition(preset.definition),
            );
            if let graph::Source::Forward(path) = &assignment.source {
                use_path(
                    &mut report,
                    &graph,
                    preset.definition,
                    path,
                    format!("{} preset source", preset.name),
                    "Preset forwards from this endpoint",
                    Selection::Definition(preset.definition),
                );
            }
        }
    }
    for instance in &source.instances {
        if let Err(reason) = definition(&graph, DefinitionId(instance.component)) {
            report.incomplete(reason);
        }
        if !source
            .presets
            .iter()
            .any(|p| p.id == instance.prototype && p.definition.0 == instance.component)
        {
            report.incomplete("Endpoint occurrence has an unavailable insertion preset");
        }
    }
    let mut placed = BTreeSet::new();
    for placement in &source.placements {
        if !source
            .instances
            .iter()
            .any(|i| i.id == placement.instance.0)
            || !placed.insert(placement.instance)
        {
            report.incomplete("Endpoint placement has an unavailable or duplicate occurrence");
        }
        if let Destination::Slot { component, slot } = &placement.destination {
            match source
                .instances
                .iter()
                .find(|i| i.id == component.instance.0)
            {
                Some(instance) => match terminal(
                    &graph,
                    DefinitionId(instance.component),
                    &component.references,
                    component.node,
                ) {
                    Ok((def, root)) if root.id == def.root => {
                        match def.slots.iter().find(|s| s.id == *slot) {
                            Some(declaration) => {
                                let references: Vec<_> = component
                                    .references
                                    .iter()
                                    .chain(&declaration.references)
                                    .copied()
                                    .collect();
                                if let Err(reason) = terminal(
                                    &graph,
                                    DefinitionId(instance.component),
                                    &references,
                                    declaration.node,
                                ) {
                                    report.incomplete(reason);
                                }
                            }
                            None => report
                                .incomplete("Endpoint placement slot declaration is unavailable"),
                        }
                    }
                    Ok(_) => {
                        report.incomplete("Endpoint placement slot owner is not a component root")
                    }
                    Err(reason) => report.incomplete(reason),
                },
                None => report.incomplete("Endpoint placement has an unavailable owner"),
            }
        }
    }
    for connection in &source.connections {
        match source
            .instances
            .iter()
            .find(|i| i.id == connection.object.instance.0)
        {
            Some(instance) => use_path(
                &mut report,
                &graph,
                DefinitionId(instance.component),
                &PropertyPath {
                    references: connection.object.references.clone(),
                    node: connection.object.node,
                    property: connection.property,
                },
                format!("{} connection", instance.name),
                "App connection targets this endpoint",
                Selection::Object(connection.object.clone()),
            ),
            None => report.incomplete("Endpoint connection has an unavailable occurrence"),
        }
    }
    report.finish()
}

#[cfg(test)]
#[path = "endpoint_impact_tests.rs"]
mod tests;
