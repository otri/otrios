// SPDX-License-Identifier: LGPL-3.0-only
//! Recreate only removed graph fragments, allocating a fresh authority interval.
use super::*;
use otrios_interfaces::oib::graph::{ParameterSource, Preset};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Graph {
    /// Only definitions containing removed identities, used to resolve old paths.
    definitions: Vec<ComponentDefinition>,
    deleted_definitions: BTreeSet<DefinitionId>,
    deleted_nodes: BTreeSet<(DefinitionId, NodeId)>,
    deleted_slots: BTreeSet<(DefinitionId, SlotId)>,
    deleted_properties: BTreeSet<(DefinitionId, PropertyId)>,
    presets: Vec<Preset>,
    instances: Vec<InstanceRecord>,
    placements: Vec<Placement>,
    connections: Vec<AppConnection>,
    siblings: Vec<(Destination, Vec<InstanceId>)>,
    values: Vec<(PropertyAddress, Option<Value>)>,
    origins: Vec<(InstanceId, DefinitionId)>,
}
#[derive(Default)]
struct Mapping {
    definitions: BTreeMap<DefinitionId, DefinitionId>,
    nodes: BTreeMap<(DefinitionId, NodeId), NodeId>,
    slots: BTreeMap<(DefinitionId, SlotId), SlotId>,
    properties: BTreeMap<(DefinitionId, PropertyId), PropertyId>,
    presets: BTreeMap<u64, u64>,
    instances: BTreeMap<InstanceId, InstanceId>,
}
impl Graph {
    pub(super) fn capture(
        before: &DocumentSource,
        after: &DocumentSource,
        properties: &[(PropertyAddress, Vec<Option<Value>>)],
    ) -> Result<Self> {
        let mut result = Self::default();
        for d in &before.definitions {
            let next = after.definitions.iter().find(|a| a.id == d.id);
            if next.is_none() {
                result.deleted_definitions.insert(d.id);
            }
            for n in &d.nodes {
                if next.is_none_or(|a| !a.nodes.iter().any(|v| v.id == n.id)) {
                    result.deleted_nodes.insert((d.id, n.id));
                }
            }
            for s in &d.slots {
                if next.is_none_or(|a| !a.slots.iter().any(|v| v.id == s.id)) {
                    result.deleted_slots.insert((d.id, s.id));
                }
            }
            for p in &d.properties {
                if next.is_none_or(|a| !a.properties.iter().any(|v| v.id == p.id)) {
                    result.deleted_properties.insert((d.id, p.id));
                }
            }
            if result.deleted_definitions.contains(&d.id)
                || result.deleted_nodes.iter().any(|(id, _)| *id == d.id)
                || result.deleted_slots.iter().any(|(id, _)| *id == d.id)
                || result.deleted_properties.iter().any(|(id, _)| *id == d.id)
            {
                result.definitions.push(d.clone());
            }
        }
        result.presets = before
            .presets
            .iter()
            .filter(|p| !after.presets.iter().any(|a| a.id == p.id))
            .cloned()
            .collect();
        result.instances = before
            .instances
            .iter()
            .filter(|instance| !after.instances.iter().any(|next| next.id == instance.id))
            .cloned()
            .collect();
        for instance in &result.instances {
            let preset = before
                .presets
                .iter()
                .find(|preset| preset.id == instance.prototype)
                .ok_or("Removed occurrence has no captured insertion preset")?;
            if !preset.palette || preset.definition.0 != instance.component {
                return Err(
                    "Removed occurrence cannot be recreated through its admitted insertion preset"
                        .into(),
                );
            }
        }
        result.placements = before
            .placements
            .iter()
            .filter(|placement| result.recreates_instance(placement.instance))
            .cloned()
            .collect();
        result.connections = before
            .connections
            .iter()
            .filter(|connection| result.recreates_instance(connection.object.instance))
            .cloned()
            .collect();
        for instance in &result.instances {
            let destination = destination(before, InstanceId(instance.id));
            if !result
                .siblings
                .iter()
                .any(|(existing, _)| *existing == destination)
            {
                result
                    .siblings
                    .push((destination.clone(), sibling_ids(before, &destination)));
            }
        }
        for instance in &before.instances {
            let origin = DefinitionId(instance.component);
            if result.recreates_instance(InstanceId(instance.id))
                || result.reaches_fragment(before, origin, &mut BTreeSet::new())
            {
                result.origins.push((InstanceId(instance.id), origin));
            }
        }
        for (address, alternatives) in properties {
            if !result.removed_address(before, address)? {
                continue;
            }
            if alternatives.len() != 1 {
                return Err("Undo cannot recreate a deleted property's causal conflict; review its alternatives".into());
            }
            if alternatives[0].is_some() {
                result
                    .values
                    .push((address.clone(), alternatives[0].clone()));
            }
        }
        Ok(result)
    }
    fn reaches_fragment(
        &self,
        source: &DocumentSource,
        d: DefinitionId,
        seen: &mut BTreeSet<DefinitionId>,
    ) -> bool {
        if !seen.insert(d) {
            return false;
        }
        self.definitions.iter().any(|v| v.id == d)
            || source
                .definitions
                .iter()
                .find(|v| v.id == d)
                .is_some_and(|v| {
                    v.nodes
                        .iter()
                        .filter_map(|n| n.reference.as_ref())
                        .any(|r| self.reaches_fragment(source, r.definition, seen))
                })
    }
    fn origin(&self, current: &DocumentSource, instance: InstanceId) -> Result<DefinitionId> {
        self.origins
            .iter()
            .find_map(|(id, definition)| (*id == instance).then_some(*definition))
            .or_else(|| {
                current
                    .instances
                    .iter()
                    .find(|i| i.id == instance.0)
                    .map(|i| DefinitionId(i.component))
            })
            .ok_or_else(|| "History occurrence was removed".into())
    }
    pub(super) fn recreates_definition(&self, d: DefinitionId) -> bool {
        self.deleted_definitions.contains(&d)
    }
    pub(super) fn recreates_node(&self, d: DefinitionId, n: NodeId) -> bool {
        self.deleted_nodes.contains(&(d, n))
    }
    pub(super) fn recreates_property(&self, d: DefinitionId, p: PropertyId) -> bool {
        self.deleted_properties.contains(&(d, p))
    }
    pub(super) fn recreates_preset(&self, id: u64) -> bool {
        self.presets.iter().any(|p| p.id == id)
    }
    pub(super) fn recreates_instance(&self, id: InstanceId) -> bool {
        self.instances.iter().any(|instance| instance.id == id.0)
    }
    pub(super) fn guard_keys(&self) -> Vec<Key> {
        self.definitions
            .iter()
            .map(|d| Key::Definition(d.id))
            .chain(self.presets.iter().map(|p| Key::Preset(p.id)))
            .chain(self.origins.iter().map(|(id, _)| Key::Instance(*id)))
            .chain(self.instances.iter().flat_map(|instance| {
                [
                    Key::Placement(InstanceId(instance.id)),
                    Key::Preset(instance.prototype),
                ]
            }))
            .chain(
                self.connections.iter().map(|connection| {
                    Key::Connection(connection.object.clone(), connection.property)
                }),
            )
            .chain(
                self.siblings
                    .iter()
                    .map(|(destination, _)| Key::Siblings(destination.clone())),
            )
            .collect()
    }
    fn removed_address(&self, source: &DocumentSource, address: &PropertyAddress) -> Result<bool> {
        match address {
            PropertyAddress::Definition {
                definition,
                node,
                property,
            } => Ok(self.deleted_nodes.contains(&(*definition, *node))
                || self
                    .old_definition(source, *definition)
                    .is_some_and(|d| d.root == *node)
                    && self.deleted_properties.contains(&(*definition, *property))),
            PropertyAddress::Occurrence { instance, path } => {
                if self.recreates_instance(*instance) {
                    return Ok(true);
                }
                let Some(i) = source.instances.iter().find(|i| i.id == instance.0) else {
                    return Ok(false);
                };
                let mut d = DefinitionId(i.component);
                for n in &path.references {
                    if self.deleted_nodes.contains(&(d, *n)) {
                        return Ok(true);
                    }
                    let Some(next) = source
                        .definitions
                        .iter()
                        .find(|v| v.id == d)
                        .and_then(|v| v.nodes.iter().find(|v| v.id == *n))
                        .and_then(|v| v.reference.as_ref())
                    else {
                        return Ok(false);
                    };
                    d = next.definition;
                }
                Ok(self.deleted_nodes.contains(&(d, path.node))
                    || self
                        .old_definition(source, d)
                        .is_some_and(|def| def.root == path.node)
                        && self.deleted_properties.contains(&(d, path.property)))
            }
            _ => Ok(false),
        }
    }
    fn mapping(&self, current: &DocumentSource) -> Result<Mapping> {
        let mut next = current.next_identity;
        let mut fresh = || -> Result<u64> {
            if next == 0 || next == u64::MAX {
                return Err("OIB identity space exhausted".into());
            }
            let id = next;
            next += 1;
            Ok(id)
        };
        let mut map = Mapping::default();
        for d in &self.deleted_definitions {
            map.definitions.insert(*d, DefinitionId(fresh()?));
        }
        for key in &self.deleted_nodes {
            map.nodes.insert(*key, NodeId(fresh()?));
        }
        for key in &self.deleted_slots {
            map.slots.insert(*key, SlotId(fresh()?));
        }
        let mut cursors = BTreeMap::new();
        for (d, p) in &self.deleted_properties {
            if self.deleted_definitions.contains(d) {
                continue;
            }
            let cursor = cursors
                .entry(*d)
                .or_insert(definition(current, *d)?.next_property);
            if *cursor == 0 {
                return Err("Removed endpoint has no retained property cursor".into());
            }
            let id = *cursor;
            *cursor = cursor
                .checked_add(1)
                .ok_or("Property identity space exhausted")?;
            map.properties.insert((*d, *p), PropertyId(id));
        }
        for p in &self.presets {
            map.presets.insert(p.id, fresh()?);
        }
        for instance in self.occurrence_order()? {
            map.instances
                .insert(InstanceId(instance.id), InstanceId(fresh()?));
        }
        Ok(map)
    }
    fn occurrence_destination(&self, id: InstanceId) -> Destination {
        self.placements
            .iter()
            .find(|placement| placement.instance == id)
            .map_or(Destination::Canvas, |placement| {
                placement.destination.clone()
            })
    }
    fn occurrence_order(&self) -> Result<Vec<&InstanceRecord>> {
        let mut remaining: Vec<_> = self.instances.iter().collect();
        let mut ordered = vec![];
        let mut created = BTreeSet::new();
        while !remaining.is_empty() {
            let Some(index) = remaining.iter().position(|instance| {
                match self.occurrence_destination(InstanceId(instance.id)) {
                    Destination::Canvas => true,
                    Destination::Slot { component, .. } => {
                        !self.recreates_instance(component.instance)
                            || created.contains(&component.instance)
                    }
                }
            }) else {
                return Err(
                    "Removed owned content has a cyclic or unavailable parent closure".into(),
                );
            };
            let instance = remaining.remove(index);
            created.insert(InstanceId(instance.id));
            ordered.push(instance);
        }
        Ok(ordered)
    }
    fn old_definition<'a>(
        &'a self,
        current: &'a DocumentSource,
        id: DefinitionId,
    ) -> Option<&'a ComponentDefinition> {
        self.definitions
            .iter()
            .find(|d| d.id == id)
            .or_else(|| current.definitions.iter().find(|d| d.id == id))
    }
    fn path(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        origin: DefinitionId,
        path: &PropertyPath,
    ) -> Result<PropertyPath> {
        let mut d = origin;
        let mut references = vec![];
        for (index, n) in path.references.iter().enumerate() {
            references.push(map.node(d, *n));
            let Some(def) = self.old_definition(current, d) else {
                // Immutable imported closures cannot contain document-local identities.
                references.extend_from_slice(&path.references[index + 1..]);
                return Ok(PropertyPath {
                    references,
                    node: path.node,
                    property: path.property,
                });
            };
            d = def
                .nodes
                .iter()
                .find(|v| v.id == *n)
                .and_then(|v| v.reference.as_ref())
                .ok_or("History reference path changed")?
                .definition;
        }
        Ok(PropertyPath {
            references,
            node: map.node(d, path.node),
            property: self.property(current, map, d, path.node, path.property),
        })
    }
    fn property(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        d: DefinitionId,
        node: NodeId,
        property: PropertyId,
    ) -> PropertyId {
        if self
            .old_definition(current, d)
            .is_some_and(|def| def.root == node)
        {
            map.properties
                .get(&(d, property))
                .copied()
                .unwrap_or(property)
        } else {
            property
        }
    }
    fn visual(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        d: DefinitionId,
        visual: &mut Option<otrios_interfaces::oib::graph::Visual>,
    ) -> Result<()> {
        if let Some(v) = visual {
            if let Some(a) = &mut v.action {
                a.source = self.path(current, map, d, &a.source)?;
            }
            for parameter in &mut v.parameters {
                if let ParameterSource::Property(path) = &mut parameter.source {
                    *path = self.path(current, map, d, path)?;
                }
            }
        }
        Ok(())
    }
    fn node(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        d: DefinitionId,
        node: &DefinitionNode,
    ) -> Result<DefinitionNode> {
        let mut result = node.clone();
        result.id = map.node(d, node.id);
        result.owner = node.owner.map(|n| map.node(d, n));
        result.visual_children.clear();
        for value in &mut result.defaults {
            value.property = self.property(current, map, d, node.id, value.property);
        }
        self.visual(current, map, d, &mut result.visual)?;
        if let Some(reference) = &mut result.reference {
            let target = reference.definition;
            reference.definition = map.definition(target);
            for a in &mut reference.assignments {
                a.target = self.path(current, map, target, &a.target)?;
                match &mut a.source {
                    AssignmentSource::Forward(path) => *path = self.path(current, map, d, path)?,
                    AssignmentSource::AppBinding(_) => {
                        return Err(
                            "Undo cannot recreate an app-bound reference through authoring grants"
                                .into(),
                        );
                    }
                    AssignmentSource::Literal(_) => {}
                }
            }
        }
        Ok(result)
    }
    fn preset(&self, current: &DocumentSource, map: &Mapping, preset: &Preset) -> Result<Preset> {
        let mut p = preset.clone();
        p.id = map.presets.get(&p.id).copied().unwrap_or(p.id);
        let origin = p.definition;
        p.definition = map.definition(origin);
        for a in &mut p.assignments {
            a.target = self.path(current, map, origin, &a.target)?;
            if let otrios_interfaces::oib::graph::Source::Forward(path) = &mut a.source {
                *path = self.path(current, map, origin, path)?;
            }
        }
        Ok(p)
    }
    fn slots(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        d: DefinitionId,
        slots: &[SlotDescriptor],
    ) -> Result<Vec<SlotDescriptor>> {
        slots
            .iter()
            .map(|s| {
                let mut s = s.clone();
                s.id = map.slots.get(&(d, s.id)).copied().unwrap_or(s.id);
                let path = self.path(
                    current,
                    map,
                    d,
                    &PropertyPath {
                        references: s.references,
                        node: s.node,
                        property: PropertyId(0),
                    },
                )?;
                s.references = path.references;
                s.node = path.node;
                Ok(s)
            })
            .collect()
    }
    pub(super) fn operations(
        &self,
        current: &DocumentSource,
        inverse: &[DocumentEdit],
    ) -> Result<Vec<DocumentEdit>> {
        let map = self.mapping(current)?;
        let mut result = vec![];
        for old in &self.definitions {
            if self.deleted_definitions.contains(&old.id) {
                let root = old
                    .nodes
                    .iter()
                    .find(|n| n.id == old.root)
                    .ok_or("Missing history root")?;
                result.push(DocumentEdit::Definition(DefinitionEdit::CreateDefinition {
                    definition: map.definition(old.id),
                    name: old.name.clone(),
                    root: self.node(current, &map, old.id, root)?,
                    properties: old.properties.clone(),
                    next_property: old.next_property,
                }));
            }
        }
        for old in &self.definitions {
            if !self.deleted_definitions.contains(&old.id) {
                for descriptor in &old.properties {
                    if !self.recreates_property(old.id, descriptor.id) {
                        continue;
                    }
                    let mut descriptor = descriptor.clone();
                    descriptor.id = map.properties[&(old.id, descriptor.id)];
                    result.push(DocumentEdit::Definition(DefinitionEdit::SetProperty {
                        definition: map.definition(old.id),
                        descriptor,
                    }));
                }
                let root = old
                    .nodes
                    .iter()
                    .find(|node| node.id == old.root)
                    .ok_or("Missing history root")?;
                for value in &root.defaults {
                    if self.recreates_property(old.id, value.property) {
                        result.push(DocumentEdit::Definition(DefinitionEdit::SetDefault {
                            definition: old.id,
                            node: old.root,
                            property: self.property(
                                current,
                                &map,
                                old.id,
                                old.root,
                                value.property,
                            ),
                            value: value.value.clone(),
                        }));
                    }
                }
            }
        }
        // Creation follows ownership, independent of visual order. Visual links
        // are restored after every owner exists, so the two trees need not agree.
        let mut remaining: Vec<_> = self
            .definitions
            .iter()
            .flat_map(|d| {
                d.nodes
                    .iter()
                    .filter(|n| n.id != d.root && self.deleted_nodes.contains(&(d.id, n.id)))
                    .map(move |n| (d.id, n))
            })
            .collect();
        let mut created = BTreeSet::new();
        while !remaining.is_empty() {
            let Some(index) = remaining.iter().position(|(d, n)| {
                n.owner.is_some_and(|owner| {
                    !self.deleted_nodes.contains(&(*d, owner))
                        || created.contains(&(*d, owner))
                        || self
                            .definitions
                            .iter()
                            .any(|def| def.id == *d && def.root == owner)
                })
            }) else {
                return Err("Deleted graph has no restorable ownership order".into());
            };
            let (d, n) = remaining.remove(index);
            result.push(DocumentEdit::Definition(DefinitionEdit::CreateNode {
                definition: map.definition(d),
                node: self.node(current, &map, d, n)?,
                visual_parent: None,
                before: None,
            }));
            created.insert((d, n.id));
        }
        // Keep existing inverse source repairs, then combine their moves with
        // restored nodes and sort all sibling repairs right-to-left below.
        for op in inverse {
            if !matches!(op, DocumentEdit::Hierarchy(_)) {
                result.push(self.operation(current, &map, op)?);
            }
        }
        for d in &self.definitions {
            for n in &d.nodes {
                if !self.deleted_nodes.contains(&(d.id, n.id)) || n.id == d.root {
                    continue;
                }
                let parent = d.nodes.iter().find(|p| p.visual_children.contains(&n.id));
                if let Some(parent) = parent {
                    let index = parent
                        .visual_children
                        .iter()
                        .position(|id| *id == n.id)
                        .unwrap();
                    result.push(DocumentEdit::Definition(DefinitionEdit::MoveNode {
                        definition: map.definition(d.id),
                        node: map.node(d.id, n.id),
                        owner: map.node(d.id, n.owner.ok_or("Missing deleted node owner")?),
                        visual_parent: Some(map.node(d.id, parent.id)),
                        before: parent
                            .visual_children
                            .get(index + 1)
                            .map(|id| map.node(d.id, *id)),
                    }));
                }
            }
            if self.deleted_definitions.contains(&d.id) && !d.slots.is_empty() {
                result.push(DocumentEdit::Definition(DefinitionEdit::SetSlots {
                    definition: map.definition(d.id),
                    slots: self.slots(current, &map, d.id, &d.slots)?,
                }));
            }
            for exposed in &d.exposed {
                if self.deleted_definitions.contains(&d.id)
                    || self.recreates_property(d.id, exposed.property)
                {
                    result.push(DocumentEdit::Definition(DefinitionEdit::Expose {
                        definition: map.definition(d.id),
                        property: self.property(current, &map, d.id, d.root, exposed.property),
                        targets: exposed
                            .targets
                            .iter()
                            .map(|p| self.path(current, &map, d.id, p))
                            .collect::<Result<_>>()?,
                    }));
                }
            }
        }
        for p in &self.presets {
            result.push(DocumentEdit::Preset(PresetEdit::Create {
                preset: self.preset(current, &map, p)?,
            }));
        }
        for instance in self.occurrence_order()? {
            let old = InstanceId(instance.id);
            result.push(DocumentEdit::Hierarchy(HierarchyEdit::Insert {
                prototype: InstanceId(
                    map.presets
                        .get(&instance.prototype)
                        .copied()
                        .unwrap_or(instance.prototype),
                ),
                name: Some(instance.name.clone()),
                destination: self.destination(current, &map, &self.occurrence_destination(old))?,
                before: None,
                frame: self
                    .placements
                    .iter()
                    .find(|p| p.instance == old)
                    .and_then(|p| p.frame),
            }));
        }
        for op in inverse {
            if matches!(op, DocumentEdit::Hierarchy(_)) {
                result.push(self.operation(current, &map, op)?);
            }
        }
        for (destination, siblings) in &self.siblings {
            for (index, old) in siblings.iter().enumerate().rev() {
                if !self.recreates_instance(*old) {
                    continue;
                }
                result.push(DocumentEdit::Hierarchy(HierarchyEdit::Move {
                    instance: map.instance(*old),
                    destination: self.destination(current, &map, destination)?,
                    before: siblings.get(index + 1).map(|id| map.instance(*id)),
                    frame: self
                        .placements
                        .iter()
                        .find(|p| p.instance == *old)
                        .and_then(|p| p.frame),
                }));
            }
        }
        for connection in &self.connections {
            result.push(DocumentEdit::Hierarchy(HierarchyEdit::Connect {
                object: self.object(current, &map, &connection.object)?,
                property: connection.property,
                source: Some(connection.source.clone()),
            }));
        }
        // Preserve non-move definition order (Create roots, then owned nodes),
        // restore sibling moves in before order, then presets and hierarchy.
        result.sort_by_key(|op| match op {
            DocumentEdit::Definition(DefinitionEdit::MoveNode {
                definition: d,
                node: n,
                ..
            }) => {
                let old_d = map
                    .definitions
                    .iter()
                    .find_map(|(old, new)| (*new == *d).then_some(*old))
                    .unwrap_or(*d);
                let old_n = map
                    .nodes
                    .iter()
                    .find_map(|((def, old), new)| (*def == old_d && *new == *n).then_some(*old))
                    .unwrap_or(*n);
                let old = self.old_definition(current, old_d);
                let parent =
                    old.and_then(|d| d.nodes.iter().find(|p| p.visual_children.contains(&old_n)));
                let index = parent
                    .and_then(|p| p.visual_children.iter().position(|n| *n == old_n))
                    .unwrap_or(0);
                (
                    0,
                    1,
                    old_d.0,
                    parent.map_or(0, |p| p.id.0),
                    usize::MAX - index,
                )
            }
            DocumentEdit::Definition(_) => (0, 0, 0, 0, 0),
            DocumentEdit::Preset(_) => (1, 0, 0, 0, 0),
            DocumentEdit::Hierarchy(HierarchyEdit::Insert { .. }) => (2, 0, 0, 0, 0),
            DocumentEdit::Hierarchy(HierarchyEdit::Move { instance, .. }) => {
                let old = map
                    .instances
                    .iter()
                    .find_map(|(old, new)| (*new == *instance).then_some(*old))
                    .unwrap_or(*instance);
                let location =
                    self.siblings
                        .iter()
                        .enumerate()
                        .find_map(|(group, (_, siblings))| {
                            siblings
                                .iter()
                                .position(|id| *id == old)
                                .map(|index| (group, index))
                        });
                match location {
                    Some((group, index)) => (2, 1, 0, group as u64, usize::MAX - index),
                    None => (2, 1, 1, 0, 0),
                }
            }
            DocumentEdit::Hierarchy(HierarchyEdit::Connect { .. }) => (2, 2, 0, 0, 0),
            _ => (2, 3, 0, 0, 0),
        });
        Ok(result)
    }
    fn operation(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        op: &DocumentEdit,
    ) -> Result<DocumentEdit> {
        let mut op = op.clone();
        match &mut op {
            DocumentEdit::Definition(edit) => match edit {
                DefinitionEdit::SetProperty {
                    definition,
                    descriptor,
                } => {
                    descriptor.id = map
                        .properties
                        .get(&(*definition, descriptor.id))
                        .copied()
                        .unwrap_or(descriptor.id);
                    *definition = map.definition(*definition);
                }
                DefinitionEdit::DeleteProperty {
                    definition,
                    property,
                } => {
                    *property = map
                        .properties
                        .get(&(*definition, *property))
                        .copied()
                        .unwrap_or(*property);
                    *definition = map.definition(*definition);
                }
                DefinitionEdit::SetSlots {
                    definition: d,
                    slots,
                } => {
                    *slots = self.slots(current, map, *d, slots)?;
                    *d = map.definition(*d);
                }
                DefinitionEdit::Expose {
                    definition: d,
                    property,
                    targets,
                } => {
                    for p in targets {
                        *p = self.path(current, map, *d, p)?;
                    }
                    *property = map
                        .properties
                        .get(&(*d, *property))
                        .copied()
                        .unwrap_or(*property);
                    *d = map.definition(*d);
                }
                DefinitionEdit::ConnectReference {
                    definition: d,
                    reference,
                    target,
                    source,
                } => {
                    let referenced = self
                        .old_definition(current, *d)
                        .and_then(|d| d.nodes.iter().find(|n| n.id == *reference))
                        .and_then(|n| n.reference.as_ref())
                        .ok_or("Missing history reference")?
                        .definition;
                    *target = self.path(current, map, referenced, target)?;
                    if let Some(LiteralSource::Forward(p)) = source {
                        *p = self.path(current, map, *d, p)?;
                    }
                    *reference = map.node(*d, *reference);
                    *d = map.definition(*d);
                }
                DefinitionEdit::SetVisual {
                    definition: d,
                    node,
                    visual,
                } => {
                    self.visual(current, map, *d, visual)?;
                    *node = map.node(*d, *node);
                    *d = map.definition(*d);
                }
                DefinitionEdit::SetDefault {
                    definition: d,
                    node,
                    property,
                    ..
                }
                | DefinitionEdit::ClearDefault {
                    definition: d,
                    node,
                    property,
                } => {
                    *property = self.property(current, map, *d, *node, *property);
                    *node = map.node(*d, *node);
                    *d = map.definition(*d);
                }
                DefinitionEdit::RenameNode {
                    definition: d,
                    node,
                    ..
                }
                | DefinitionEdit::DeleteNode {
                    definition: d,
                    node,
                } => {
                    *node = map.node(*d, *node);
                    *d = map.definition(*d);
                }
                DefinitionEdit::RenameDefinition { definition, .. }
                | DefinitionEdit::DeleteDefinition { definition } => {
                    *definition = map.definition(*definition)
                }
                DefinitionEdit::MoveNode {
                    definition: d,
                    node,
                    owner,
                    visual_parent,
                    before,
                } => {
                    *node = map.node(*d, *node);
                    *owner = map.node(*d, *owner);
                    *visual_parent = visual_parent.map(|n| map.node(*d, n));
                    *before = before.map(|n| map.node(*d, n));
                    *d = map.definition(*d);
                }
                DefinitionEdit::CreateDefinition { .. } | DefinitionEdit::CreateNode { .. } => {
                    return Err("Unexpected stored history creation".into());
                }
            },
            DocumentEdit::Preset(PresetEdit::Update { preset } | PresetEdit::Create { preset }) => {
                *preset = self.preset(current, map, preset)?
            }
            DocumentEdit::Preset(PresetEdit::Delete { preset }) => {
                *preset = map.presets.get(preset).copied().unwrap_or(*preset)
            }
            DocumentEdit::Hierarchy(edit) => match edit {
                HierarchyEdit::Connect {
                    object, property, ..
                } => {
                    let origin = self.origin(current, object.instance)?;
                    *property = self
                        .path(
                            current,
                            map,
                            origin,
                            &PropertyPath {
                                references: object.references.clone(),
                                node: object.node,
                                property: *property,
                            },
                        )?
                        .property;
                    *object = self.object(current, map, object)?
                }
                HierarchyEdit::Insert {
                    prototype,
                    destination,
                    before,
                    ..
                } => {
                    prototype.0 = map
                        .presets
                        .get(&prototype.0)
                        .copied()
                        .unwrap_or(prototype.0);
                    *destination = self.destination(current, map, destination)?;
                    *before = before.map(|id| map.instance(id));
                }
                HierarchyEdit::Move {
                    instance,
                    destination,
                    before,
                    ..
                } => {
                    *instance = map.instance(*instance);
                    *destination = self.destination(current, map, destination)?;
                    *before = before.map(|id| map.instance(id));
                }
                HierarchyEdit::Duplicate {
                    instance,
                    destination,
                    before,
                    ..
                } => {
                    *instance = map.instance(*instance);
                    *destination = self.destination(current, map, destination)?;
                    *before = before.map(|id| map.instance(id));
                }
                HierarchyEdit::Reorder { instance, before } => {
                    *instance = map.instance(*instance);
                    *before = before.map(|id| map.instance(id));
                }
                HierarchyEdit::Rename { instance, .. }
                | HierarchyEdit::Place { instance, .. }
                | HierarchyEdit::Remove { instance } => *instance = map.instance(*instance),
            },
            DocumentEdit::Restore { .. } => {}
        }
        Ok(op)
    }
    fn object(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        object: &ObjectAddress,
    ) -> Result<ObjectAddress> {
        let d = self.origin(current, object.instance)?;
        let p = self.path(
            current,
            map,
            d,
            &PropertyPath {
                references: object.references.clone(),
                node: object.node,
                property: PropertyId(0),
            },
        )?;
        Ok(ObjectAddress {
            instance: map.instance(object.instance),
            references: p.references,
            node: p.node,
        })
    }
    fn destination(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        destination: &Destination,
    ) -> Result<Destination> {
        match destination {
            Destination::Canvas => Ok(Destination::Canvas),
            Destination::Slot { component, slot } => {
                let mut d = self.origin(current, component.instance)?;
                for n in &component.references {
                    let Some(definition) = self.old_definition(current, d) else {
                        // Imported closure identities are immutable under the
                        // captured baseline and cannot contain local slots.
                        return Ok(Destination::Slot {
                            component: self.object(current, map, component)?,
                            slot: *slot,
                        });
                    };
                    d = definition
                        .nodes
                        .iter()
                        .find(|v| v.id == *n)
                        .and_then(|n| n.reference.as_ref())
                        .ok_or("History slot owner path changed")?
                        .definition;
                }
                Ok(Destination::Slot {
                    component: self.object(current, map, component)?,
                    slot: map.slots.get(&(d, *slot)).copied().unwrap_or(*slot),
                })
            }
        }
    }
    fn address(
        &self,
        current: &DocumentSource,
        map: &Mapping,
        address: &PropertyAddress,
    ) -> Result<PropertyAddress> {
        match address {
            PropertyAddress::Definition {
                definition: d,
                node: n,
                property,
            } => Ok(PropertyAddress::Definition {
                definition: map.definition(*d),
                node: map.node(*d, *n),
                property: self.property(current, map, *d, *n, *property),
            }),
            PropertyAddress::Occurrence { instance, path } => {
                let d = self.origin(current, *instance)?;
                Ok(PropertyAddress::Occurrence {
                    instance: map.instance(*instance),
                    path: self.path(current, map, d, path)?,
                })
            }
            _ => Ok(address.clone()),
        }
    }
    pub(super) fn values(
        &self,
        current: &DocumentSource,
    ) -> Result<Vec<(PropertyAddress, Option<Value>)>> {
        let map = self.mapping(current)?;
        self.values
            .iter()
            .map(|(a, v)| Ok((self.address(current, &map, a)?, v.clone())))
            .collect()
    }
    pub(super) fn selection(
        &self,
        current: &DocumentSource,
        selection: &Selection,
    ) -> Result<Selection> {
        let map = self.mapping(current)?;
        Ok(match selection {
            Selection::Definition(d) => Selection::Definition(map.definition(*d)),
            Selection::DefinitionNode { definition, node } => Selection::DefinitionNode {
                definition: map.definition(*definition),
                node: map.node(*definition, *node),
            },
            Selection::Object(object) => {
                // Selection is advisory local UI state. An unrelated retired
                // selection must not veto an otherwise guarded data inverse;
                // Designer refresh will choose its normal valid fallback.
                let Ok(origin) = self.origin(current, object.instance) else {
                    return Ok(selection.clone());
                };
                let Ok(path) = self.path(
                    current,
                    &map,
                    origin,
                    &PropertyPath {
                        references: object.references.clone(),
                        node: object.node,
                        property: PropertyId(0),
                    },
                ) else {
                    return Ok(selection.clone());
                };
                Selection::Object(ObjectAddress {
                    instance: map.instance(object.instance),
                    references: path.references,
                    node: path.node,
                })
            }
            _ => selection.clone(),
        })
    }
}
impl Mapping {
    fn definition(&self, d: DefinitionId) -> DefinitionId {
        self.definitions.get(&d).copied().unwrap_or(d)
    }
    fn node(&self, d: DefinitionId, n: NodeId) -> NodeId {
        self.nodes.get(&(d, n)).copied().unwrap_or(n)
    }
    fn instance(&self, instance: InstanceId) -> InstanceId {
        self.instances.get(&instance).copied().unwrap_or(instance)
    }
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "occurrence_tests.rs"]
mod occurrence_tests;
