// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded removal evidence for allocated occurrences, never write authority.
use super::impact::{Kind, Use};
use super::*;
use otrios_interfaces::oib::{
    authoring::{Destination, DocumentSource},
    designer::*,
    graph,
};

const MAX_ITEMS: usize = MAX_ALLOCATIONS * 4 + MAX_CELLS;

#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub target: InstanceId,
    pub name: String,
    pub scope: &'static str,
    pub complete: bool,
    pub readonly: bool,
    pub items: Vec<Use>,
    pub blockers: Vec<String>,
    /// Owned descendants precede their owners; sibling order follows source order.
    pub removal_order: Vec<InstanceId>,
}
impl Report {
    pub(crate) fn can_remove(&self) -> bool {
        self.complete && !self.readonly && !self.items.iter().any(|item| item.blocking)
    }
    fn incomplete(&mut self, reason: impl Into<String>) {
        self.complete = false;
        let reason = reason.into();
        if !self.blockers.contains(&reason) && self.blockers.len() < MAX_ITEMS {
            self.blockers.push(reason);
        }
    }
    fn row(
        &mut self,
        kind: Kind,
        label: String,
        detail: String,
        direct: bool,
        selection: Option<Selection>,
    ) {
        if self.items.len() >= MAX_ITEMS {
            self.incomplete("Instance removal report exceeds admitted display capacity");
            return;
        }
        self.items.push(Use {
            kind,
            label,
            detail,
            direct,
            blocking: false,
            selection,
        });
    }
}
fn definition(graph: &graph::Document, id: DefinitionId) -> Result<&ComponentDefinition> {
    graph
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "Occurrence definition is unavailable".into())
}
/// Resolve the full occurrence path through the exact admitted definition closure.
fn object<'a>(
    graph: &'a graph::Document,
    source: &DocumentSource,
    address: &ObjectAddress,
) -> Result<(&'a ComponentDefinition, &'a DefinitionNode)> {
    let root = source
        .instances
        .iter()
        .find(|i| i.id == address.instance.0)
        .ok_or("Occurrence path has an unavailable allocated root")?;
    if address.references.len() > MAX_REFERENCE_DEPTH {
        return Err("Occurrence path exceeds admission".into());
    }
    let mut current = definition(graph, DefinitionId(root.component))?;
    let mut seen = BTreeSet::new();
    seen.insert(current.id);
    for reference in &address.references {
        let next = current
            .nodes
            .iter()
            .find(|n| n.id == *reference)
            .and_then(|n| n.reference.as_ref())
            .ok_or("Occurrence path has an unavailable reference")?;
        if !seen.insert(next.definition) {
            return Err("Occurrence path has a cyclic definition reference".into());
        }
        current = definition(graph, next.definition)?;
    }
    let node = current
        .nodes
        .iter()
        .find(|n| n.id == address.node)
        .ok_or("Occurrence path has an unavailable terminal node")?;
    Ok((current, node))
}
fn root_address(
    graph: &graph::Document,
    component: u64,
    instance: InstanceId,
) -> Option<Selection> {
    definition(graph, DefinitionId(component)).ok().map(|d| {
        Selection::Object(ObjectAddress {
            instance,
            references: vec![],
            node: d.root,
        })
    })
}
fn visit(
    source: &DocumentSource,
    id: InstanceId,
    active: &mut BTreeSet<InstanceId>,
    done: &mut BTreeSet<InstanceId>,
    order: &mut Vec<InstanceId>,
) -> Result<()> {
    if active.contains(&id) {
        return Err("Owned occurrence hierarchy is cyclic".into());
    }
    if done.contains(&id) {
        return Ok(());
    }
    if !source.instances.iter().any(|i| i.id == id.0) {
        return Err("Owned occurrence root is unavailable".into());
    }
    if active.len() >= MAX_ALLOCATIONS || done.len() >= MAX_ALLOCATIONS {
        return Err("Owned occurrence closure exceeds allocation limits".into());
    }
    active.insert(id);
    for child in &source.instances {
        if source.placements.iter().any(|p| p.instance.0 == child.id && matches!(&p.destination, Destination::Slot {component, ..} if component.instance == id)) {
            visit(source, InstanceId(child.id), active, done, order)?;
        }
    }
    active.remove(&id);
    done.insert(id);
    order.push(id);
    Ok(())
}
/// Capture the exact current owned closure. Definition bodies, styles and behavior
/// remain reusable. Minimum slot cardinality, grants and final budgets are checked
/// by candidate preparation, rather than inferred from this evidence report.
pub(crate) fn instance(seed: &Seed, source: &DocumentSource, target: InstanceId) -> Report {
    let mut report = Report {
        target,
        name: format!("Instance {}", target.0),
        scope: "This document only; other documents are not checked.",
        complete: true,
        readonly: false,
        items: vec![],
        blockers: vec![],
        removal_order: vec![],
    };
    let Some(authored) = &seed.graph else {
        report.incomplete("Canonical component source is unavailable");
        return report;
    };
    if authored.definitions != source.definitions || authored.presets != source.presets {
        report.incomplete("Removal source does not match the current component candidate");
    }
    let graph = match imports::resolve(seed) {
        Ok(graph) => graph,
        Err(reason) => {
            report.incomplete(reason);
            authored.clone()
        }
    };
    if source.instances.len() > MAX_ALLOCATIONS
        || source.placements.len() > MAX_ALLOCATIONS
        || source.connections.len() > MAX_CELLS
    {
        report.incomplete("Removal source exceeds existing allocation or connection limits");
        return report;
    }
    if source
        .presets
        .iter()
        .map(|p| p.id)
        .collect::<BTreeSet<_>>()
        .len()
        != source.presets.len()
        || graph
            .definitions
            .iter()
            .map(|d| d.id)
            .collect::<BTreeSet<_>>()
            .len()
            != graph.definitions.len()
    {
        report.incomplete("Removal source has ambiguous preset or definition identities");
    }
    let mut ids = BTreeSet::new();
    for root in &source.instances {
        if !ids.insert(root.id) {
            report.incomplete("Allocated occurrence identities are not unique");
        }
        match source.presets.iter().find(|p| p.id == root.prototype) {
            Some(p) if p.definition.0 == root.component => {}
            _ => report.incomplete("Occurrence insertion preset is missing or mismatched"),
        }
        match definition(&graph, DefinitionId(root.component)) {
            Ok(d)
                if d.nodes.iter().filter(|n| n.id == d.root).count() == 1
                    && d.nodes.iter().map(|n| n.id).collect::<BTreeSet<_>>().len()
                        == d.nodes.len() => {}
            _ => report.incomplete("Occurrence definition or its unique root is unavailable"),
        }
    }
    let mut placements = BTreeSet::new();
    for placement in &source.placements {
        if !ids.contains(&placement.instance.0) || !placements.insert(placement.instance) {
            report.incomplete("Occurrence placement is missing its root or is duplicated");
        }
        if let Destination::Slot { component, slot } = &placement.destination {
            match object(&graph, source, component) {
                Ok((d, node)) if node.id == d.root => {
                    match d.slots.iter().find(|s| s.id == *slot) {
                        Some(s) => {
                            let host = ObjectAddress {
                                instance: component.instance,
                                references: component
                                    .references
                                    .iter()
                                    .chain(s.references.iter())
                                    .copied()
                                    .collect(),
                                node: s.node,
                            };
                            if let Err(reason) = object(&graph, source, &host) {
                                report.incomplete(reason);
                            }
                        }
                        None => {
                            report.incomplete("Owned occurrence slot declaration is unavailable")
                        }
                    }
                }
                Ok(_) => report.incomplete("Owned occurrence slot address is not a component root"),
                Err(reason) => report.incomplete(reason),
            }
        }
    }
    // Validate all ownership paths: an invalid external ancestor must not make a
    // selected subtree appear safely detached or hide an owned descendant.
    for root in &source.instances {
        if let Err(reason) = visit(
            source,
            InstanceId(root.id),
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut vec![],
        ) {
            report.incomplete(reason);
        }
    }
    if let Err(reason) = visit(
        source,
        target,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut report.removal_order,
    ) {
        report.incomplete(reason);
    }
    let closure: BTreeSet<_> = report.removal_order.iter().copied().collect();
    for id in report.removal_order.clone() {
        let Some(root) = source.instances.iter().find(|i| i.id == id.0) else {
            continue;
        };
        if id == target {
            report.name = root.name.clone();
        }
        let fixed = id == graph.presented_root
            || source
                .presets
                .iter()
                .find(|p| p.id == root.prototype)
                .is_some_and(|p| !p.palette)
            || graph
                .instances
                .iter()
                .find(|i| i.id == id)
                .is_some_and(|i| matches!(i.owner, Owner::Instance(_)));
        if fixed {
            report.readonly = true;
            report
                .blockers
                .push(format!("{} is a fixed host instance", root.name));
        }
        report.row(
            Kind::Instance,
            root.name.clone(),
            if id == target {
                "Selected instance will be removed"
            } else {
                "Owned child will be removed with its owner"
            }
            .into(),
            id == target,
            root_address(&graph, root.component, id),
        );
        if let Ok(d) = definition(&graph, DefinitionId(root.component)) {
            report.row(
                Kind::Definition,
                d.name.clone(),
                "Reusable component definition remains available".into(),
                id == target,
                Some(Selection::Definition(d.id)),
            );
        }
        if let Some(placement) = source.placements.iter().find(|p| p.instance == id)
            && let Destination::Slot { component, slot } = &placement.destination
        {
            let parent = source
                .instances
                .iter()
                .find(|i| i.id == component.instance.0)
                .map_or_else(
                    || format!("Instance {}", component.instance.0),
                    |i| i.name.clone(),
                );
            report.row(
                Kind::Ownership,
                format!("{} / slot {}", parent, slot.0),
                if closure.contains(&component.instance) {
                    "Owned attachment removed with this subtree"
                } else {
                    "Attachment removed; parent instance remains"
                }
                .into(),
                id == target,
                Some(Selection::Object(component.clone())),
            );
        }
    }
    for connection in &source.connections {
        if let Err(reason) = object(&graph, source, &connection.object) {
            report.incomplete(reason);
        }
        if closure.contains(&connection.object.instance) {
            report.row(
                Kind::Api,
                format!("Property {} connection", connection.property.0),
                format!(
                    "Explicit app field {} connection will be removed",
                    connection.source.field
                ),
                connection.object.instance == target,
                Some(Selection::Object(connection.object.clone())),
            );
        }
    }
    if !report.complete {
        for reason in report.blockers.clone() {
            if report.items.len() >= MAX_ITEMS {
                break;
            }
            report.items.push(Use {
                kind: Kind::Unknown,
                label: "Incomplete removal evidence".into(),
                detail: reason,
                direct: false,
                blocking: true,
                selection: None,
            });
        }
    }
    report
}

#[cfg(test)]
#[path = "instance_impact_tests.rs"]
mod tests;
