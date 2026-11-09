// SPDX-License-Identifier: LGPL-3.0-only
//! One generation-guarded owned hierarchy edit; never a general transaction API.
use super::*;
use otrios_interfaces::oib::{MAX_LIVE_INSTANCES, authoring::*, designer::InstanceId};

impl Document {
    pub(super) fn oib_connections(&self) -> Result<Vec<AppConnection>> {
        Ok(self.oib_source_shared()?.connections.clone())
    }
    pub(super) fn oib_placements(&self) -> Result<Vec<Placement>> {
        Ok(self.oib_source_shared()?.placements.clone())
    }

    pub(super) fn oib_hierarchy_stage(&mut self, operations: &[HierarchyEdit]) -> Result<()> {
        let signed = self.oib_binding()?.seed.clone();
        if signed.designer.is_none() || signed.instance_surface.is_none() {
            return Err("Hierarchy authoring requires signed designer factories".into());
        }
        let previous = self.oib_placements()?;
        let mut placements = previous.clone();
        let previous_connections = self.oib_connections()?;
        let mut connections = previous_connections.clone();
        for operation in operations {
            match operation {
                HierarchyEdit::Connect {
                    object,
                    property,
                    source,
                } => {
                    // Explicitly removing a connection remains valid after the
                    // definition phase deleted its endpoint. It never guesses a
                    // replacement endpoint or silently redirects the source.
                    if source.is_none() {
                        connections.retain(|connection| {
                            connection.object != *object || connection.property != *property
                        });
                        continue;
                    }
                    self.require_owned_root(object.instance)?;
                    let seed = self.oib_authored_seed()?;
                    let metadata = super::super::super::oib::document::expanded_metadata(
                        &seed,
                        &self.oib_allocations()?,
                    )?;
                    let admission = self.oib_authored_ui()?;
                    super::super::super::oib::document::connections::choices(
                        &admission, &metadata, object, *property,
                    )?;
                    connections.retain(|c| c.object != *object || c.property != *property);
                    if let Some(source) = source {
                        connections.push(
                            super::super::super::oib::document::connections::admitted(
                                &admission,
                                &metadata,
                                object,
                                *property,
                                source.clone(),
                            )?,
                        );
                    }
                }
                HierarchyEdit::Insert {
                    prototype,
                    name,
                    destination,
                    before,
                    frame,
                } => {
                    let id = InstanceId(self.oib_next_id()?);
                    self.oib_instance_stage(&InstanceStage::Insert {
                        prototype: prototype.0,
                        name: name.clone(),
                    })?;
                    put(&mut placements, id, destination.clone(), *frame)?;
                    self.order_owned(id, destination, *before, &placements)?;
                }
                HierarchyEdit::Move {
                    instance,
                    destination,
                    before,
                    frame,
                } => {
                    self.require_owned_root(*instance)?;
                    put(&mut placements, *instance, destination.clone(), *frame)?;
                    self.order_owned(*instance, destination, *before, &placements)?;
                }
                HierarchyEdit::Place { instance, frame } => {
                    self.require_owned_root(*instance)?;
                    let destination = super::super::super::oib::document::placement::destination(
                        &placements,
                        *instance,
                    );
                    put(&mut placements, *instance, destination, Some(*frame))?;
                }
                HierarchyEdit::Reorder { instance, before } => {
                    self.require_owned_root(*instance)?;
                    let destination = super::super::super::oib::document::placement::destination(
                        &placements,
                        *instance,
                    );
                    self.order_owned(*instance, &destination, *before, &placements)?;
                }
                HierarchyEdit::Rename { instance, name } => {
                    self.require_owned_root(*instance)?;
                    if !admitted_instance_name(&signed, name) {
                        return Err("Invalid instance name".into());
                    }
                    if self
                        .oib_source()?
                        .instances
                        .iter()
                        .any(|current| current.id == instance.0 && current.name == *name)
                    {
                        continue;
                    }
                    let edit = self.oib_source_edit(|source| {
                        if let Some(current) = source
                            .instances
                            .iter_mut()
                            .find(|current| current.id == instance.0)
                        {
                            current.name = name.clone();
                        }
                    })?;
                    self.change(&[], vec![edit])?;
                }
                HierarchyEdit::Remove { instance } => {
                    if self.oib_retired_instance(instance.0)? {
                        continue;
                    }
                    self.require_owned_root(*instance)?;
                    self.oib_instance_stage(&InstanceStage::Remove {
                        instance: instance.0,
                    })?;
                    placements.retain(|p| p.instance != *instance);
                    connections.retain(|c| c.object.instance != *instance);
                }
                HierarchyEdit::Duplicate {
                    instance,
                    name,
                    destination,
                    before,
                    frame,
                } => {
                    self.require_owned_root(*instance)?;
                    let live = self.oib_live(1)?;
                    let allocations = self.oib_allocations()?;
                    let old = placements.clone();
                    let old_connections = connections.clone();
                    let mut sources = vec![*instance];
                    let mut at = 0;
                    while at < sources.len() {
                        let owner = sources[at];
                        for a in &allocations {
                            let id = InstanceId(a.id);
                            if live.contains(&a.id)
                                && matches!(super::super::super::oib::document::placement::destination(&old, id),
                                Destination::Slot { component, .. } if component.instance == owner)
                            {
                                if sources.contains(&id) || sources.len() >= MAX_LIVE_INSTANCES {
                                    return Err("Cyclic or oversized owned duplicate".into());
                                }
                                sources.push(id);
                            }
                        }
                        at += 1;
                    }
                    let mut remap = BTreeMap::new();
                    for source in sources {
                        let fresh = InstanceId(self.oib_next_id()?);
                        self.oib_instance_stage(&InstanceStage::Duplicate {
                            instance: source.0,
                            name: if source == *instance {
                                name.clone()
                            } else {
                                None
                            },
                        })?;
                        remap.insert(source, fresh);
                        for old in old_connections
                            .iter()
                            .filter(|c| c.object.instance == source)
                        {
                            let mut copied = old.clone();
                            copied.object.instance = fresh;
                            connections.push(copied);
                        }
                        if source == *instance {
                            put(&mut placements, fresh, destination.clone(), *frame)?;
                            self.order_owned(fresh, destination, *before, &placements)?;
                        } else {
                            let mut copy = old
                                .iter()
                                .find(|p| p.instance == source)
                                .ok_or("Missing owned duplicate placement")?
                                .clone();
                            copy.instance = fresh;
                            if let Destination::Slot { component, .. } = &mut copy.destination {
                                component.instance = *remap
                                    .get(&component.instance)
                                    .ok_or("Missing copied owner identity")?;
                            }
                            put(&mut placements, fresh, copy.destination, copy.frame)?;
                        }
                    }
                }
            }
        }
        connections.sort_by(|a, b| (&a.object, a.property).cmp(&(&b.object, b.property)));
        if connections != previous_connections || placements != previous {
            let edit = self.oib_source_edit(|source| {
                source.connections = connections;
                source.placements = placements;
            })?;
            self.change(&[], vec![edit])?;
        }
        Ok(())
    }

    fn require_owned_root(&self, instance: InstanceId) -> Result<()> {
        if self.oib_live(1)?.contains(&instance.0)
            && self.oib_allocations()?.iter().any(|a| a.id == instance.0)
        {
            Ok(())
        } else {
            Err("Hierarchy edits require a live owned component root".into())
        }
    }

    fn order_owned(
        &mut self,
        instance: InstanceId,
        destination: &Destination,
        before: Option<InstanceId>,
        placements: &[Placement],
    ) -> Result<()> {
        if before == Some(instance) {
            return Err("A component cannot precede itself".into());
        }
        if let Some(before) = before {
            self.require_owned_root(before)?;
            if super::super::super::oib::document::placement::destination(placements, before)
                != *destination
            {
                return Err("Insertion anchor must be a sibling in the destination".into());
            }
        }
        let mut allocations = self.oib_allocations()?;
        let index = allocations
            .iter()
            .position(|a| a.id == instance.0)
            .ok_or("Unknown moved root")?;
        let moved = allocations.remove(index);
        let index = before
            .map(|id| {
                allocations
                    .iter()
                    .position(|a| a.id == id.0)
                    .ok_or("Missing destination anchor")
            })
            .transpose()?
            .unwrap_or(allocations.len());
        allocations.insert(index, moved);
        if allocations != self.oib_allocations()? {
            self.change(
                &[],
                vec![self.oib_source_edit(|source| {
                    source.instances = allocations.iter().map(Into::into).collect()
                })?],
            )?;
        }
        Ok(())
    }
}

fn put(
    placements: &mut Vec<Placement>,
    instance: InstanceId,
    destination: Destination,
    frame: Option<[f32; 4]>,
) -> Result<()> {
    super::super::super::oib::document::placement::validate_frame(frame)?;
    placements.retain(|p| p.instance != instance);
    if destination != Destination::Canvas || frame.is_some() {
        placements.push(Placement {
            instance,
            destination,
            frame,
        });
    }
    placements.sort_by_key(|p| p.instance);
    Ok(())
}

impl StateAdapter {
    pub(in crate::canvas) fn oib_connection_choices(
        &self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
    ) -> Result<Vec<(String, InstanceOperation)>> {
        use otrios_interfaces::oib::designer::PropertyAddress;
        let PropertyAddress::Occurrence { instance, path } = address else {
            return Ok(vec![]);
        };
        let object = otrios_interfaces::oib::designer::ObjectAddress {
            instance: *instance,
            references: path.references.clone(),
            node: path.node,
        };
        let seed = self.doc.oib_authored_seed()?;
        let metadata = super::super::super::oib::document::expanded_metadata(
            &seed,
            &self.doc.oib_allocations()?,
        )?;
        let admission = self.doc.oib_authored_ui()?;
        let Ok(choices) = super::super::super::oib::document::connections::choices(
            &admission,
            &metadata,
            &object,
            path.property,
        ) else {
            return Ok(vec![]);
        };
        let current = metadata
            .instances
            .iter()
            .find(|i| i.id == *instance)
            .and_then(|i| i.assignments.iter().find(|a| a.target == *path))
            .and_then(|a| {
                if let otrios_interfaces::oib::designer::AssignmentSource::AppBinding(b) = a.source
                {
                    admission
                        .selectors
                        .iter()
                        .find(|selector| {
                            otrios_interfaces::layout::Binding::Local(selector.local) == b
                        })
                        .map(|selector| ConnectionSource {
                            field: selector.field,
                            projection: selector.projection.clone(),
                        })
                } else {
                    None
                }
            });
        let generation = self.doc.oib_status()?.structural_generation;
        let operation = |source| {
            InstanceOperation::hierarchy(HierarchyBatch {
                generation,
                operations: vec![HierarchyEdit::Connect {
                    object: object.clone(),
                    property: path.property,
                    source,
                }],
            })
        };
        let mut result: Vec<_> = choices
            .into_iter()
            .filter(|(_, source)| current.as_ref() != Some(source))
            .map(|(name, source)| (format!("Bind to {name}"), operation(Some(source))))
            .collect();
        if self
            .doc
            .oib_connections()?
            .iter()
            .any(|c| c.object == object && c.property == path.property)
        {
            result.push(("Restore signed binding".into(), operation(None)));
        }
        Ok(result)
    }
    pub(in crate::canvas) fn oib_destination(&self, instance: u64) -> Result<Destination> {
        Ok(super::super::super::oib::document::placement::destination(
            &self.doc.oib_placements()?,
            InstanceId(instance),
        ))
    }
    pub(in crate::canvas) fn oib_hierarchy_allowed(&self, batch: &HierarchyBatch) -> bool {
        let mut preview = self.doc.clone();
        preview
            .oib_structure_write([0; 16], &InstanceOperation::hierarchy(batch.clone()))
            .is_ok()
            && preview
                .oib_status()
                .is_ok_and(|status| status.error.is_none())
    }
    pub(in crate::canvas) fn oib_placement_properties(
        &self,
        object: &otrios_interfaces::oib::designer::ObjectAddress,
    ) -> Result<Vec<super::super::super::oib::document::PropertyInfo>> {
        use otrios_interfaces::layout::{Expr, TemplateNode, ValueType};
        use otrios_interfaces::oib::designer::*;
        if !object.references.is_empty() || !self.doc.oib_live(2)?.contains(&object.instance.0) {
            return Err("Placement requires a live owned component root".into());
        }
        let (ui, layout) = self.doc.oib_compiled(2)?;
        let doc = super::super::super::oib::seed(&ui)
            .ok_or("Missing OIB seed")?
            .designer
            .as_ref()
            .ok_or("Missing OIB document")?;
        let root = doc
            .instances
            .iter()
            .find(|i| i.id == object.instance)
            .ok_or("Unknown placed component")?;
        let definition = doc
            .definitions
            .iter()
            .find(|d| d.id == root.definition)
            .ok_or("Unknown component definition")?;
        if object.node != definition.root {
            return Err("Inherited nodes do not own instance placement".into());
        }
        let id = doc
            .mappings
            .objects
            .iter()
            .find(|m| &m.address == object)
            .and_then(|m| m.layout_node)
            .ok_or("Missing placed root mapping")?;
        fn find(n: &TemplateNode, id: u16) -> Option<&TemplateNode> {
            if n.id == id {
                Some(n)
            } else {
                n.children.iter().find_map(|n| find(n, id))
            }
        }
        let node = layout
            .blueprint()
            .templates
            .iter()
            .find_map(|t| find(&t.root, id))
            .ok_or("Missing placed render root")?;
        let frame = node.frame.each_ref().map(|e| {
            if let Expr::Literal(Value::Number(n)) = e {
                Ok(*n)
            } else {
                Err("Placement requires an admitted literal frame")
            }
        });
        let [x, y, w, h] = frame;
        let frame = [x?, y?, w?, h?];
        let class = definition
            .nodes
            .iter()
            .find(|n| n.id == definition.root)
            .ok_or("Missing root class")?
            .class;
        let kind = doc
            .classes
            .iter()
            .find(|c| c.id == class)
            .ok_or("Missing root class descriptor")?
            .implementation;
        let interactive = matches!(
            kind,
            ClassKind::O2DButton | ClassKind::O2DToggle | ClassKind::O2DSlider
        );
        Ok([
            PlacementProperty::X,
            PlacementProperty::Y,
            PlacementProperty::Width,
            PlacementProperty::Height,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, property)| {
            let label = [
                "Placement X",
                "Placement Y",
                "Placement width",
                "Placement height",
            ][index];
            let range = match property {
                PlacementProperty::X => [0., 16000. - frame[2]],
                PlacementProperty::Y => [0., 16000. - frame[3]],
                PlacementProperty::Width => [if interactive { 48. } else { 1. }, 1000.],
                PlacementProperty::Height => [if interactive { 32. } else { 1. }, 1000.],
            };
            super::super::super::oib::document::PropertyInfo {
                address: Some(PropertyAddress::Placement {
                    object: object.clone(),
                    property,
                }),
                cell: None,
                descriptor: PropertyDescriptor {
                    id: PropertyId(index as u64 + 1),
                    name: label.into(),
                    label: label.into(),
                    value_type: ValueType::Number,
                    default: Value::Number(frame[index]),
                    number_range: Some(range),
                    number_step: Some(1.),
                    units: Some("px".into()),
                    group: InspectorGroup::Placement,
                    editor: PropertyEditor::NumberSuggestions,
                    policy: PropertyPolicy {
                        live_literal: true,
                        app_binding: false,
                    },
                },
                value: Some(Value::Number(frame[index])),
                inherited: None,
                origin: "placement".into(),
                source: "literal".into(),
                overridden: false,
            }
        })
        .collect())
    }

    pub(in crate::canvas) fn edit_oib_placement_property(
        &mut self,
        object: &otrios_interfaces::oib::designer::ObjectAddress,
        property: otrios_interfaces::oib::designer::PlacementProperty,
        value: Option<Value>,
    ) -> Result<bool> {
        self.edit_oib_placement_property_at(
            object,
            property,
            value,
            self.doc.oib_status()?.structural_generation,
        )
    }

    pub(in crate::canvas) fn edit_oib_placement_property_at(
        &mut self,
        object: &otrios_interfaces::oib::designer::ObjectAddress,
        property: otrios_interfaces::oib::designer::PlacementProperty,
        value: Option<Value>,
        generation: u64,
    ) -> Result<bool> {
        use otrios_interfaces::oib::designer::PlacementProperty;
        let properties = self.oib_placement_properties(object)?;
        let mut frame = [0.; 4];
        for (index, info) in properties.iter().enumerate() {
            let Some(Value::Number(n)) = info.value else {
                return Err("Missing placement value".into());
            };
            frame[index] = n;
        }
        let Some(Value::Number(value)) = value else {
            return Err("Placement requires an explicit numeric value".into());
        };
        let index = match property {
            PlacementProperty::X => 0,
            PlacementProperty::Y => 1,
            PlacementProperty::Width => 2,
            PlacementProperty::Height => 3,
        };
        frame[index] = value;
        self.edit_oib_structure(InstanceOperation::hierarchy(HierarchyBatch {
            generation,
            operations: vec![HierarchyEdit::Place {
                instance: object.instance,
                frame,
            }],
        }))
    }
}
