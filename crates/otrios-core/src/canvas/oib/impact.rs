// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded document-scoped dependency evidence. This report never authorizes a write.
use super::*;
use otrios_interfaces::oib::{
    authoring::{Destination, DocumentSource},
    designer::*,
    graph::{self, ParameterSource},
};

/// These are data rows, not additional scene/control capacity. An incomplete
/// projection cannot establish absence of dependencies.
const MAX_ITEMS: usize = MAX_AUTHORED_OBJECTS * 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Definition,
    Instance,
    Preset,
    Api,
    Export,
    Ownership,
    Unknown,
}
#[derive(Clone, Debug)]
pub(crate) struct Use {
    pub kind: Kind,
    pub label: String,
    pub detail: String,
    pub direct: bool,
    pub blocking: bool,
    pub selection: Option<Selection>,
}
#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub target: DefinitionId,
    pub name: String,
    pub scope: &'static str,
    pub complete: bool,
    pub readonly: bool,
    pub items: Vec<Use>,
    pub removable_presets: Vec<u64>,
    pub blockers: Vec<String>,
}
impl Report {
    pub(crate) fn can_delete(&self) -> bool {
        self.complete && !self.readonly && !self.items.iter().any(|item| item.blocking)
    }
    fn push(&mut self, item: Use) {
        if self.items.len() < MAX_ITEMS {
            self.items.push(item);
        } else {
            self.incomplete("Dependency report exceeds admitted display capacity");
        }
    }
    fn incomplete(&mut self, reason: impl Into<String>) {
        self.complete = false;
        let reason = reason.into();
        if !self.blockers.contains(&reason) && self.blockers.len() < MAX_DEFINITIONS {
            self.blockers.push(reason);
        }
    }
    fn use_at(
        &mut self,
        kind: Kind,
        label: String,
        detail: String,
        direct: bool,
        selection: Option<Selection>,
    ) {
        self.push(Use {
            kind,
            label,
            detail,
            direct,
            blocking: true,
            selection,
        });
    }
}
fn definition(graph: &graph::Document, id: DefinitionId) -> Result<&ComponentDefinition> {
    graph
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "Referenced definition is unavailable in this document".into())
}
fn validate_graph(graph: &graph::Document) -> Result<()> {
    if graph.definitions.len() > MAX_DEFINITIONS
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
    {
        return Err("Dependency source exceeds the admitted definition/object budget".into());
    }
    fn visit(
        graph: &graph::Document,
        id: DefinitionId,
        active: &mut BTreeSet<DefinitionId>,
        done: &mut BTreeSet<DefinitionId>,
    ) -> Result<()> {
        if active.contains(&id) {
            return Err("Cyclic component dependencies cannot establish a complete scan".into());
        }
        if done.contains(&id) {
            return Ok(());
        }
        if active.len() >= MAX_REFERENCE_DEPTH {
            return Err("Dependency reference depth exceeds admission".into());
        }
        active.insert(id);
        for reference in definition(graph, id)?
            .nodes
            .iter()
            .filter_map(|n| n.reference.as_ref())
        {
            visit(graph, reference.definition, active, done)?;
        }
        active.remove(&id);
        done.insert(id);
        Ok(())
    }
    let mut done = BTreeSet::new();
    for d in &graph.definitions {
        visit(graph, d.id, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}
fn reaches(
    graph: &graph::Document,
    start: DefinitionId,
    target: DefinitionId,
    seen: &mut BTreeSet<DefinitionId>,
) -> bool {
    start == target
        || seen.insert(start)
            && graph
                .definitions
                .iter()
                .find(|d| d.id == start)
                .is_some_and(|d| {
                    d.nodes
                        .iter()
                        .filter_map(|n| n.reference.as_ref())
                        .any(|r| reaches(graph, r.definition, target, seen))
                })
}
/// Validate the complete semantic path while recording every crossed definition.
fn path_uses(
    graph: &graph::Document,
    origin: DefinitionId,
    path: &PropertyPath,
    target: DefinitionId,
) -> Result<bool> {
    let mut current = origin;
    let mut found = current == target;
    if path.references.len() > MAX_REFERENCE_DEPTH {
        return Err("Dependency path exceeds admission".into());
    }
    for id in &path.references {
        current = definition(graph, current)?
            .nodes
            .iter()
            .find(|n| n.id == *id)
            .and_then(|n| n.reference.as_ref())
            .ok_or("Dependency path has an unavailable reference")?
            .definition;
        found |= current == target;
    }
    if !definition(graph, current)?
        .nodes
        .iter()
        .any(|n| n.id == path.node)
    {
        return Err("Dependency path has an unavailable terminal node".into());
    }
    Ok(found)
}
fn object_uses(
    graph: &graph::Document,
    source: &DocumentSource,
    object: &ObjectAddress,
    target: DefinitionId,
) -> Result<bool> {
    let instance = source
        .instances
        .iter()
        .find(|i| i.id == object.instance.0)
        .ok_or("Dependency occurrence is unavailable")?;
    path_uses(
        graph,
        DefinitionId(instance.component),
        &PropertyPath {
            references: object.references.clone(),
            node: object.node,
            property: PropertyId(0),
        },
        target,
    )
}
fn api_use(
    report: &mut Report,
    graph: &graph::Document,
    origin: DefinitionId,
    path: &PropertyPath,
    label: String,
    detail: &str,
    selection: Selection,
) {
    match path_uses(graph, origin, path, report.target) {
        Ok(true) => report.use_at(
            Kind::Api,
            label,
            detail.into(),
            origin == report.target,
            Some(selection),
        ),
        Ok(false) => {}
        Err(reason) => report.incomplete(reason),
    }
}
fn definition_uses(report: &mut Report, graph: &graph::Document) {
    for d in &graph.definitions {
        if d.id == report.target {
            continue;
        }
        for n in &d.nodes {
            let selection = Selection::DefinitionNode {
                definition: d.id,
                node: n.id,
            };
            if let Some(r) = &n.reference {
                if reaches(graph, r.definition, report.target, &mut BTreeSet::new()) {
                    report.use_at(
                        Kind::Definition,
                        format!("{} / {}", d.name, n.name),
                        if r.definition == report.target {
                            "Direct component reference"
                        } else {
                            "Transitive component reference"
                        }
                        .into(),
                        r.definition == report.target,
                        Some(selection.clone()),
                    );
                }
                for a in &r.assignments {
                    api_use(
                        report,
                        graph,
                        r.definition,
                        &a.target,
                        format!("{} / {} assignment", d.name, n.name),
                        "Reference property assignment",
                        selection.clone(),
                    );
                    if let AssignmentSource::Forward(p) = &a.source {
                        api_use(
                            report,
                            graph,
                            d.id,
                            p,
                            format!("{} / {} forwarding", d.name, n.name),
                            "Forwarded property source",
                            selection.clone(),
                        );
                    }
                }
            }
            if let Some(v) = &n.visual {
                if let Some(a) = &v.action {
                    api_use(
                        report,
                        graph,
                        d.id,
                        &a.source,
                        format!("{} / {} action", d.name, n.name),
                        "Input action route",
                        selection.clone(),
                    );
                }
                for p in &v.parameters {
                    if let ParameterSource::Property(path) = &p.source {
                        api_use(
                            report,
                            graph,
                            d.id,
                            path,
                            format!("{} / {} parameter", d.name, n.name),
                            "Visual property route",
                            selection.clone(),
                        );
                    }
                }
            }
        }
        for exposed in &d.exposed {
            for p in &exposed.targets {
                api_use(
                    report,
                    graph,
                    d.id,
                    p,
                    format!("{} / property {}", d.name, exposed.property.0),
                    "Exposed property forwarding",
                    Selection::Definition(d.id),
                );
            }
        }
        for slot in &d.slots {
            api_use(
                report,
                graph,
                d.id,
                &PropertyPath {
                    references: slot.references.clone(),
                    node: slot.node,
                    property: PropertyId(0),
                },
                format!("{} / {}", d.name, slot.name),
                "Slot host",
                Selection::Definition(d.id),
            );
        }
    }
}
fn instance_uses(report: &mut Report, graph: &graph::Document, source: &DocumentSource) {
    for instance in &source.instances {
        let origin = DefinitionId(instance.component);
        match definition(graph, origin) {
            Ok(d) if reaches(graph, origin, report.target, &mut BTreeSet::new()) => report.use_at(
                Kind::Instance,
                instance.name.clone(),
                if origin == report.target {
                    "Direct document instance"
                } else {
                    "Instance contains a transitive reference"
                }
                .into(),
                origin == report.target,
                Some(Selection::Object(ObjectAddress {
                    instance: InstanceId(instance.id),
                    references: vec![],
                    node: d.root,
                })),
            ),
            Ok(_) => {}
            Err(reason) => report.incomplete(reason),
        }
    }
    for connection in &source.connections {
        match object_uses(graph, source, &connection.object, report.target) {
            Ok(true) => report.use_at(
                Kind::Api,
                format!(
                    "{} / property {}",
                    instance_name(source, connection.object.instance),
                    connection.property.0
                ),
                "App property connection".into(),
                true,
                Some(Selection::Object(connection.object.clone())),
            ),
            Ok(false) => {}
            Err(reason) => report.incomplete(reason),
        }
    }
    for placement in &source.placements {
        if let Destination::Slot { component, slot } = &placement.destination {
            match object_uses(graph, source, component, report.target) {
                Ok(true) => report.use_at(
                    Kind::Api,
                    format!(
                        "{} / slot {}",
                        instance_name(source, placement.instance),
                        slot.0
                    ),
                    "Owned content attached to this component".into(),
                    true,
                    Some(Selection::Object(component.clone())),
                ),
                Ok(false) => {}
                Err(reason) => report.incomplete(reason),
            }
        }
    }
}
fn instance_name(source: &DocumentSource, id: InstanceId) -> String {
    source
        .instances
        .iter()
        .find(|i| i.id == id.0)
        .map_or_else(|| format!("Instance {}", id.0), |i| i.name.clone())
}
fn preset_uses(report: &mut Report, graph: &graph::Document, source: &DocumentSource) {
    for preset in &source.presets {
        let direct = preset.definition == report.target;
        if reaches(
            graph,
            preset.definition,
            report.target,
            &mut BTreeSet::new(),
        ) {
            let removable = direct && preset.palette && !report.readonly;
            if removable {
                report.removable_presets.push(preset.id);
            }
            report.push(Use {
                kind: Kind::Preset,
                label: preset.name.clone(),
                detail: if removable {
                    "Insertion preset removed with this component"
                } else if !preset.palette {
                    "Required signed preset"
                } else {
                    "Preset contains a transitive reference"
                }
                .into(),
                direct,
                blocking: !removable,
                selection: Some(Selection::Definition(preset.definition)),
            });
        }
        if preset.definition != report.target {
            for a in &preset.assignments {
                api_use(
                    report,
                    graph,
                    preset.definition,
                    &a.target,
                    format!("{} assignment", preset.name),
                    "Preset property assignment",
                    Selection::Definition(preset.definition),
                );
                if let graph::Source::Forward(path) = &a.source {
                    api_use(
                        report,
                        graph,
                        preset.definition,
                        path,
                        format!("{} forwarding", preset.name),
                        "Preset forwarding source",
                        Selection::Definition(preset.definition),
                    );
                }
            }
        }
    }
    if let Some(shell) = graph
        .instances
        .iter()
        .find(|i| i.id == graph.presented_root)
        && let Some(preset) = graph.presets.iter().find(|p| p.id == shell.preset)
        && preset.definition == report.target
    {
        report.use_at(
            Kind::Ownership,
            "Presented shell".into(),
            "The host's presented shell has fixed ownership".into(),
            true,
            Some(Selection::Definition(report.target)),
        );
    }
}
fn export_uses(report: &mut Report, seed: &Seed) {
    let Some(imports) = seed.graph.as_ref().and_then(|g| g.imports.as_ref()) else {
        return;
    };
    for binding in &imports.bindings {
        if !binding
            .definitions
            .iter()
            .any(|m| m.target == report.target)
        {
            continue;
        }
        let library = seed
            .resolved_libraries
            .iter()
            .find(|l| l.library == binding.library);
        let export = library.and_then(|l| l.graph.exports.iter().find(|e| e.id == binding.export));
        match (library, export) {
            (Some(library), Some(export)) => {
                let revision = library
                    .revision
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                let direct = binding
                    .definitions
                    .iter()
                    .any(|m| m.source == export.definition && m.target == report.target);
                report.use_at(
                    Kind::Export,
                    export.name.clone(),
                    format!(
                        "{} @ {} · exact admitted export",
                        binding.library.0, revision
                    ),
                    direct,
                    Some(Selection::Definition(report.target)),
                );
            }
            _ => report.incomplete("An exact imported export is unavailable"),
        }
    }
}
/// Inspect exactly the current document and its admitted immutable import closure.
/// Historical packages and consumers outside this document are not enumerated.
/// On incomplete or malformed input, retain known evidence and block deletion.
pub(crate) fn component(seed: &Seed, source: &DocumentSource, target: DefinitionId) -> Report {
    let imported = imports::imported_definitions(seed);
    let mut report = Report {
        target,
        name: format!("Component {}", target.0),
        scope: "This document and its exact admitted imports; external consumers are not scanned",
        complete: true,
        readonly: imported.contains(&target),
        items: vec![],
        removable_presets: vec![],
        blockers: vec![],
    };
    let Some(authored) = &seed.graph else {
        report.incomplete("Canonical component source is unavailable");
        return report;
    };
    if authored.definitions != source.definitions || authored.presets != source.presets {
        report.incomplete("Dependency source does not match the current component candidate");
    }
    let resolved = match imports::resolve(seed) {
        Ok(graph) => graph,
        Err(reason) => {
            report.incomplete(reason);
            authored.clone()
        }
    };
    match definition(&resolved, target) {
        Ok(d) => report.name = d.name.clone(),
        Err(reason) => report.incomplete(reason),
    }
    if let Err(reason) = validate_graph(&resolved) {
        report.incomplete(reason);
    }
    if source.instances.len() > MAX_AUTHORED_OBJECTS
        || source.presets.len() > MAX_AUTHORED_OBJECTS
        || source.connections.len() > MAX_AUTHORED_OBJECTS
        || source.placements.len() > MAX_AUTHORED_OBJECTS
    {
        report.incomplete("Dependency source exceeds existing object limits");
    }
    definition_uses(&mut report, &resolved);
    instance_uses(&mut report, &resolved, source);
    preset_uses(&mut report, &resolved, source);
    export_uses(&mut report, seed);
    if report.readonly {
        report.use_at(
            Kind::Ownership,
            "Imported component".into(),
            "Owned by an exact signed library; local deletion is unavailable".into(),
            true,
            Some(Selection::Definition(target)),
        );
    }
    for reason in report.blockers.clone() {
        report.push(Use {
            kind: Kind::Unknown,
            label: "Scan incomplete".into(),
            detail: reason,
            direct: false,
            blocking: true,
            selection: None,
        });
    }
    report.items.sort_by(|a, b| {
        (a.kind, !a.direct, &a.label, &a.detail).cmp(&(b.kind, !b.direct, &b.label, &b.detail))
    });
    report.removable_presets.sort_unstable();
    report
}

#[cfg(test)]
#[path = "impact_tests.rs"]
mod tests;
