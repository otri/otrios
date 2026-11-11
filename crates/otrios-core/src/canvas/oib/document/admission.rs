// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn ordered<T: Ord>(items: impl IntoIterator<Item = T>) -> bool {
    let mut previous = None;
    for item in items {
        if previous.as_ref().is_some_and(|p| *p >= item) {
            return false;
        }
        previous = Some(item);
    }
    true
}
fn visual(doc: &Document, id: ClassId) -> Result<bool> {
    Ok(!matches!(
        class(doc, id)?.implementation,
        ClassKind::OIBObject | ClassKind::OIBDocument
    ))
}
fn subclass(doc: &Document, child: ClassId, parent: ClassId) -> Result<bool> {
    let mut id = Some(child);
    let mut seen = BTreeSet::new();
    while let Some(c) = id {
        if !seen.insert(c) || seen.len() > 16 {
            return Err("Cyclic OIB class ancestry".into());
        }
        if c == parent {
            return Ok(true);
        }
        id = class(doc, c)?.base;
    }
    Ok(false)
}
pub(super) fn accepts(doc: &Document, classes: &[ClassId], child: ClassId) -> Result<bool> {
    for accepted in classes {
        if subclass(doc, child, *accepted)? {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(super) fn valid_descriptor(p: &PropertyDescriptor) -> Result<()> {
    require(
        p.id.0 != 0
            && valid_text(&p.name, 64)
            && valid_text(&p.label, 96)
            && p.units.as_ref().is_none_or(|u| valid_text(u, 16)),
        "Invalid OIB property descriptor identity",
    )?;
    validate_property_value(p, &p.default)?;
    if let Some([lo, hi]) = p.number_range {
        require(
            p.value_type == ValueType::Number
                && lo.is_finite()
                && hi.is_finite()
                && !(lo == 0. && lo.is_sign_negative())
                && !(hi == 0. && hi.is_sign_negative())
                && lo >= 0.
                && hi <= 1000.
                && lo <= hi,
            "Invalid OIB numeric range",
        )?;
    }
    if let Some(step) = p.number_step {
        let [lo, hi] = p.number_range.unwrap_or([0., 1000.]);
        require(
            p.value_type == ValueType::Number && step.is_finite() && step > 0. && step <= hi - lo,
            "Invalid OIB numeric step",
        )?;
    }
    require(
        match p.editor {
            PropertyEditor::ReadOnly => !p.policy.live_literal,
            PropertyEditor::TextSuggestions => p.value_type == ValueType::Text,
            PropertyEditor::NumberSuggestions => {
                p.value_type == ValueType::Number && p.number_step.is_some()
            }
            PropertyEditor::Palette => p.value_type == ValueType::Color,
            PropertyEditor::Toggle => p.value_type == ValueType::Boolean,
        },
        "Unavailable OIB property editor",
    )
}
pub(super) fn declarations(
    classes: &[ClassDescriptor],
    definitions: &[ComponentDefinition],
) -> Result<()> {
    require(
        classes.iter().map(|c| c.properties.len()).sum::<usize>()
            + definitions
                .iter()
                .map(|d| d.properties.len())
                .sum::<usize>()
            <= 64,
        "Definition and class property budget exceeded",
    )?;
    for def in definitions {
        require(
            def.properties.len() <= 64 && ordered(def.properties.iter().map(|p| p.id)),
            "Invalid definition endpoint ordering/budget",
        )?;
        if def.properties.is_empty() {
            require(def.next_property == 0, "Invalid definition endpoint cursor")?;
            continue;
        }
        let root_class = def
            .nodes
            .iter()
            .find(|n| n.id == def.root)
            .ok_or("Missing endpoint root")?
            .class;
        let inherited = class_descriptors(classes, root_class)?;
        let mut names: BTreeSet<_> = inherited.iter().map(|p| p.name.as_str()).collect();
        let first = inherited
            .iter()
            .map(|p| p.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Property identity space exhausted")?;
        for (index, p) in def.properties.iter().enumerate() {
            valid_descriptor(p)?;
            require(
                p.id.0
                    == first
                        .checked_add(index as u64)
                        .ok_or("Property identity space exhausted")?
                    && p.id.0 < def.next_property
                    && names.insert(&p.name)
                    && p.policy.live_literal
                    && !p.policy.app_binding,
                "Invalid definition endpoint identity, name or capability policy",
            )?;
        }
        require(
            if def.properties.is_empty() {
                def.next_property == 0
            } else {
                def.next_property
                    == first
                        .checked_add(def.properties.len() as u64)
                        .ok_or("Property identity space exhausted")?
            },
            "Invalid definition endpoint cursor",
        )?;
    }
    Ok(())
}

fn check_assignments(
    doc: &Document,
    def: DefinitionId,
    containing: Option<DefinitionId>,
    assignments: &[Assignment],
) -> Result<()> {
    require(
        assignments.len() <= 64 && ordered(assignments.iter().map(|a| &a.target)),
        "Invalid OIB assignment ordering/budget",
    )?;
    for a in assignments {
        let descriptor = property(doc, def, &a.target)?;
        match &a.source {
            AssignmentSource::Literal(v) => {
                require(
                    descriptor.policy.live_literal,
                    "Literal assigned to read-only OIB source",
                )?;
                validate_property_value(&descriptor, v)?;
            }
            AssignmentSource::Forward(from) => {
                let source = property(
                    doc,
                    containing.ok_or("Top-level forwarding is unsupported")?,
                    from,
                )?;
                require(
                    source.value_type == descriptor.value_type,
                    "OIB forwarding type mismatch",
                )?;
            }
            AssignmentSource::AppBinding(_) => require(
                descriptor.policy.app_binding,
                "OIB app binding forbidden by property policy",
            )?,
        }
    }
    Ok(())
}
fn all_paths(
    doc: &Document,
    id: DefinitionId,
    prefix: &[NodeId],
    ancestry: &mut Vec<DefinitionId>,
    out: &mut Vec<PropertyPath>,
    objects: &mut BTreeSet<(Vec<NodeId>, NodeId)>,
    depth: usize,
) -> Result<()> {
    require(
        !ancestry.contains(&id) && ancestry.len() < 8 && depth <= 12,
        "Cyclic or too deep OIB definition references",
    )?;
    ancestry.push(id);
    let def = definition(doc, id)?;
    for n in &def.nodes {
        require(
            objects.insert((prefix.to_vec(), n.id)) && objects.len() <= 512,
            "OIB expansion budget",
        )?;
        for p in node_descriptors(doc, def, n)? {
            out.push(PropertyPath {
                references: prefix.to_vec(),
                node: n.id,
                property: p.id,
            });
        }
        if let Some(r) = &n.reference {
            let mut nested = prefix.to_vec();
            nested.push(n.id);
            all_paths(
                doc,
                r.definition,
                &nested,
                ancestry,
                out,
                objects,
                depth + 2,
            )?;
        }
    }
    ancestry.pop();
    Ok(())
}
pub fn validate(seed: &Seed) -> Result<()> {
    let doc = metadata(seed)?;
    require(
        doc.id.0 == seed.main_oib
            && doc.placements.is_empty()
            && doc.connections.is_empty()
            && doc.id.0 != 0
            && valid_text(&doc.name, 64)
            && doc.classes.len() <= 16
            && !doc.classes.is_empty()
            && doc.definitions.len() <= 16
            && !doc.definitions.is_empty()
            && 1 + doc.definitions.len()
                + doc.definitions.iter().map(|d| d.nodes.len()).sum::<usize>()
                + doc.instances.len()
                <= 64
            && doc
                .classes
                .iter()
                .map(|c| c.properties.len())
                .sum::<usize>()
                + doc
                    .definitions
                    .iter()
                    .map(|d| d.properties.len())
                    .sum::<usize>()
                <= 64
            && doc.mappings.objects.len() <= 512
            && doc.mappings.definitions.len()
                + doc
                    .mappings
                    .roots
                    .iter()
                    .map(|r| r.cells.len())
                    .sum::<usize>()
                <= if seed.graph.is_some() && !seed.expanded {
                    MAX_CELLS * (MAX_DEFINITIONS + 1)
                } else {
                    MAX_CELLS
                },
        "Unsupported or oversized OIB designer metadata",
    )?;
    require(
        ordered(doc.classes.iter().map(|c| c.id))
            && ordered(doc.definitions.iter().map(|d| d.id))
            && ordered(doc.instances.iter().map(|i| i.id))
            && ordered(doc.mappings.objects.iter().map(|m| &m.address))
            && ordered(
                doc.mappings
                    .definitions
                    .iter()
                    .map(|m| (m.definition, m.node, m.property)),
            )
            && ordered(doc.mappings.roots.iter().map(|r| r.prototype)),
        "Noncanonical OIB metadata ordering",
    )?;
    let mut raw_ids = BTreeSet::from([doc.id.0]);
    for id in doc
        .definitions
        .iter()
        .map(|d| d.id.0)
        .chain(doc.instances.iter().map(|i| i.id.0))
    {
        require(
            id != 0 && raw_ids.insert(id),
            "Colliding OIB object-owner namespaces",
        )?;
    }
    require(
        doc.id.0 != 0
            && class(doc, doc.class)?.implementation == ClassKind::OIBDocument
            && seed
                .components
                .iter()
                .map(|c| c.id)
                .collect::<BTreeSet<_>>()
                == doc.definitions.iter().map(|d| d.id.0).collect(),
        "OIB document/component closure mismatch",
    )?;
    for c in &doc.classes {
        require(
            c.id.0 != 0
                && valid_text(&c.name, 64)
                && c.properties.len() <= 64
                && ordered(c.properties.iter().map(|p| p.id))
                && c.owned_child_classes.len() <= 16
                && c.visual_child_classes.len() <= 16
                && ordered(c.owned_child_classes.iter())
                && ordered(c.visual_child_classes.iter()),
            "Invalid OIB class descriptor",
        )?;
        let base = c.base.map(|id| class(doc, id)).transpose()?;
        require(
            match c.implementation {
                ClassKind::OIBObject => base.is_none(),
                ClassKind::OIBDocument | ClassKind::O2DView => {
                    base.is_some_and(|b| b.implementation == ClassKind::OIBObject)
                }
                ClassKind::O2DToggle => base.is_some_and(|b| {
                    matches!(b.implementation, ClassKind::O2DView | ClassKind::O2DButton)
                }),
                _ => base.is_some_and(|b| b.implementation == ClassKind::O2DView),
            },
            "Unsupported OIB class base/implementation",
        )?;
        descriptors(doc, c.id)?;
        for p in &c.properties {
            valid_descriptor(p)?;
        }
        for id in c.owned_child_classes.iter().chain(&c.visual_child_classes) {
            class(doc, *id)?;
        }
        for id in &c.visual_child_classes {
            require(visual(doc, *id)?, "Nonvisual class in visual child rules")?;
        }
    }
    declarations(&doc.classes, &doc.definitions)?;
    let mut expected_definitions = BTreeSet::new();
    for def in &doc.definitions {
        require(
            valid_text(&def.name, 64)
                && !def.nodes.is_empty()
                && def.nodes.len() <= 64
                && def.exposed.len() <= 64
                && def.slots.len() <= 64
                && ordered(def.nodes.iter().map(|n| n.id))
                && ordered(def.exposed.iter().map(|e| e.property))
                && ordered(def.slots.iter().map(|s| s.id)),
            "Invalid OIB definition ordering/budget",
        )?;
        let root = path_node(doc, def.id, &[], def.root)?.1;
        require(root.owner.is_none(), "OIB definition root has an owner")?;
        let mut visual_parents = BTreeSet::new();
        for n in &def.nodes {
            require(
                n.id.0 != 0
                    && valid_text(&n.name, 64)
                    && n.visual_children.len() <= 64
                    && n.defaults.len() <= 64
                    && ordered(n.defaults.iter().map(|p| p.property))
                    && (n.id == def.root || n.owner.is_some()),
                "Invalid OIB definition node",
            )?;
            let c = class(doc, n.class)?;
            let mut owner = n.owner;
            let mut seen = BTreeSet::from([n.id]);
            while let Some(id) = owner {
                require(
                    seen.insert(id) && seen.len() <= 12,
                    "Cyclic/deep OIB ownership",
                )?;
                owner = path_node(doc, def.id, &[], id)?.1.owner;
            }
            if let Some(owner) = n.owner {
                let parent = path_node(doc, def.id, &[], owner)?.1;
                require(
                    parent.reference.is_none()
                        && accepts(doc, &class(doc, parent.class)?.owned_child_classes, n.class)?,
                    "OIB ownership class rule violated",
                )?;
            }
            require(
                visual(doc, n.class)? || n.visual_children.is_empty(),
                "Nonvisual OIB node has visual children",
            )?;
            for child in &n.visual_children {
                let child = path_node(doc, def.id, &[], *child)?.1;
                require(
                    child.id != def.root
                        && visual_parents.insert(child.id)
                        && visual(doc, child.class)?
                        && accepts(doc, &c.visual_child_classes, child.class)?,
                    "OIB visual class/parent rule violated",
                )?;
            }
            for p in node_descriptors(doc, def, n)? {
                if p.policy.live_literal && p.editor != PropertyEditor::ReadOnly {
                    expected_definitions.insert((def.id, n.id, p.id));
                }
            }
            for default in &n.defaults {
                validate_property_value(
                    &property(
                        doc,
                        def.id,
                        &PropertyPath {
                            references: vec![],
                            node: n.id,
                            property: default.property,
                        },
                    )?,
                    &default.value,
                )?;
            }
            if let Some(reference) = &n.reference {
                require(
                    n.visual_children.is_empty()
                        && !def.nodes.iter().any(|child| child.owner == Some(n.id)),
                    "OIB reference wrapper cannot own local children",
                )?;
                let target = definition(doc, reference.definition)?;
                require(
                    visual(doc, n.class)?
                        == visual(doc, path_node(doc, target.id, &[], target.root)?.1.class)?,
                    "Visual/nonvisual OIB reference mismatch",
                )?;
                check_assignments(doc, target.id, Some(def.id), &reference.assignments)?;
            }
        }
        // Visual containment can differ from ownership, but must independently terminate.
        for n in &def.nodes {
            let mut id = n.id;
            let mut seen = BTreeSet::new();
            while let Some(parent) = def.nodes.iter().find(|p| p.visual_children.contains(&id)) {
                require(
                    seen.insert(id) && seen.len() <= 12,
                    "Cyclic/deep OIB visual containment",
                )?;
                id = parent.id;
            }
        }
        for exposed in &def.exposed {
            let source = property(
                doc,
                def.id,
                &PropertyPath {
                    references: vec![],
                    node: def.root,
                    property: exposed.property,
                },
            )?;
            require(
                exposed.targets.len() <= 64 && ordered(exposed.targets.iter()),
                "Invalid exposed OIB target list",
            )?;
            for target in &exposed.targets {
                require(
                    property(doc, def.id, target)?.value_type == source.value_type,
                    "OIB exposed property type mismatch",
                )?;
            }
        }
        for slot in &def.slots {
            require(
                slot.id.0 != 0
                    && valid_text(&slot.name, 64)
                    && slot.references.len() <= 8
                    && slot.admitted_classes.len() <= 16
                    && ordered(slot.admitted_classes.iter())
                    && slot.cardinality[0] <= slot.cardinality[1]
                    && slot.cardinality[1] <= 64,
                "Invalid OIB slot descriptor",
            )?;
            let (host_def, host) = path_node(doc, def.id, &slot.references, slot.node)?;
            for accepted in &slot.admitted_classes {
                class(doc, *accepted)?;
            }
            let children: Vec<_> = host_def
                .nodes
                .iter()
                .filter(|n| n.owner == Some(host.id))
                .collect();
            require(
                children.len() >= usize::from(slot.cardinality[0])
                    && children.len() <= usize::from(slot.cardinality[1]),
                "OIB slot cardinality mismatch",
            )?;
            for child in children {
                require(
                    accepts(doc, &slot.admitted_classes, child.class)?,
                    "OIB slot class mismatch",
                )?;
            }
        }
        all_paths(
            doc,
            def.id,
            &[],
            &mut Vec::new(),
            &mut Vec::new(),
            &mut BTreeSet::new(),
            0,
        )?;
    }
    require(
        expected_definitions
            == doc
                .mappings
                .definitions
                .iter()
                .map(|m| (m.definition, m.node, m.property))
                .collect(),
        "Missing/extra editable OIB definition mapping",
    )?;
    for cell in &seed.cells {
        require(
            valid_text(&cell.name, 64)
                && cell.targets.iter().all(|t| {
                    !(t.scale == 0. && t.scale.is_sign_negative()
                        || t.offset == 0. && t.offset.is_sign_negative())
                }),
            "Noncanonical OIB cell name/float",
        )?;
    }
    let mut aliases = BTreeSet::new();
    for m in &doc.mappings.definitions {
        require(
            m.cell != 0 && aliases.insert(m.cell),
            "Colliding OIB definition aliases",
        )?;
        let cell = seed
            .cells
            .iter()
            .find(|c| c.id == m.cell)
            .ok_or("Missing OIB definition cell")?;
        let path = PropertyPath {
            references: vec![],
            node: m.node,
            property: m.property,
        };
        require(
            cell.owner == m.definition.0
                && cell.inherit.is_none()
                && cell.authored_override.is_none()
                && cell.default == fallback(doc, m.definition, &path)?,
            "OIB definition cell mapping mismatch",
        )?;
        let descriptor = property(doc, m.definition, &path)?;
        require(
            descriptor.editor == PropertyEditor::Toggle || !cell.choices.is_empty(),
            "OIB suggestion editor has no choices",
        )?;
        for choice in &cell.choices {
            validate_property_value(&descriptor, choice)?;
        }
    }
    let factories: BTreeSet<_> = seed.instances.iter().map(|i| InstanceId(i.id)).collect();
    require(
        factories == doc.mappings.roots.iter().map(|m| m.prototype).collect(),
        "OIB factory root mapping mismatch",
    )?;
    for factory in &seed.instances {
        require(
            doc.instances.iter().any(|root| {
                root.id.0 == factory.id && factory.component == Some(root.definition.0)
            }),
            "OIB factory/document instance mismatch",
        )?;
    }
    let mut expected_objects = BTreeSet::new();
    let mut sibling_order = BTreeSet::new();
    let presented = instance(doc, doc.presented_root)?;
    require(
        presented.visual_parent.is_none()
            && presented.owner == Owner::Document
            && !factories.contains(&presented.id)
            && visual(
                doc,
                path_node(
                    doc,
                    presented.definition,
                    &[],
                    definition(doc, presented.definition)?.root,
                )?
                .1
                .class,
            )?,
        "Invalid fixed OIB presentation shell",
    )?;
    for root in &doc.instances {
        require(valid_text(&root.name, 64), "Invalid OIB instance label")?;
        let def = definition(doc, root.definition)?;
        let root_class = path_node(doc, def.id, &[], def.root)?.1.class;
        let owner_class = match root.owner {
            Owner::Document => doc.class,
            Owner::Instance(id) => {
                require(
                    !factories.contains(&id),
                    "D1 document ownership cannot depend on a removable factory root",
                )?;
                let owner = instance(doc, id)?;
                let def = definition(doc, owner.definition)?;
                path_node(doc, def.id, &[], def.root)?.1.class
            }
        };
        require(
            accepts(
                doc,
                &class(doc, owner_class)?.owned_child_classes,
                root_class,
            )?,
            "OIB document ownership class rule violated",
        )?;
        let mut owner = root.owner;
        let mut seen = BTreeSet::from([root.id]);
        while let Owner::Instance(id) = owner {
            require(
                seen.insert(id) && seen.len() <= 12,
                "Cyclic/deep document ownership",
            )?;
            owner = instance(doc, id)?.owner;
        }
        if let Some(parent) = &root.visual_parent {
            require(
                !factories.contains(&parent.instance),
                "D1 document attachment cannot depend on a removable factory root",
            )?;
            let parent_instance = instance(doc, parent.instance)?;
            let parent_node = path_node(
                doc,
                parent_instance.definition,
                &parent.references,
                parent.node,
            )?
            .1;
            require(
                visual(doc, root_class)?
                    && visual(doc, parent_node.class)?
                    && accepts(
                        doc,
                        &class(doc, parent_node.class)?.visual_child_classes,
                        root_class,
                    )?
                    && sibling_order.insert((parent.clone(), root.order)),
                "Invalid OIB document visual attachment",
            )?;
            // Follow full visual instance attachments independently of ownership.
            let mut next = Some(parent.instance);
            let mut seen = BTreeSet::from([root.id]);
            while let Some(id) = next {
                require(
                    seen.insert(id) && seen.len() <= 12,
                    "Cyclic/deep document visual attachment",
                )?;
                next = instance(doc, id)?
                    .visual_parent
                    .as_ref()
                    .map(|p| p.instance);
            }
        } else {
            require(
                root.id == doc.presented_root || !visual(doc, root_class)?,
                "Detached visual OIB instance",
            )?;
        }
        check_assignments(doc, root.definition, None, &root.assignments)?;
        let sources = sources(doc, root)?;
        let mut paths = Vec::new();
        let mut objects = BTreeSet::new();
        all_paths(
            doc,
            def.id,
            &[],
            &mut Vec::new(),
            &mut paths,
            &mut objects,
            0,
        )?;
        for (references, node) in objects {
            expected_objects.insert(ObjectAddress {
                instance: root.id,
                references,
                node,
            });
        }
        let mut editable = BTreeSet::new();
        for path in &paths {
            let p = property(doc, def.id, path)?;
            let bound = bound_source(&sources, path, &mut BTreeSet::new())?;
            if bound.is_some() {
                require(
                    p.policy.app_binding,
                    "Forwarded binding forbidden by property policy",
                )?;
            }
            if p.policy.live_literal && p.editor != PropertyEditor::ReadOnly && bound.is_none() {
                editable.insert(path.clone());
            }
            resolve(
                seed,
                root,
                &sources,
                path,
                &BTreeMap::new(),
                None,
                &mut BTreeSet::new(),
            )?;
        }
        for (target, source) in &sources {
            let p = property(doc, def.id, target)?;
            match source {
                Source::Literal(v) => validate_property_value(&p, v)?,
                Source::Forward(from) => require(
                    property(doc, def.id, from)?.value_type == p.value_type,
                    "Forwarding type mismatch",
                )?,
                Source::Bound(_) => require(p.policy.app_binding, "App binding policy mismatch")?,
            }
        }
        if factories.contains(&root.id) {
            require(
                seed.instances
                    .iter()
                    .any(|i| i.id == root.id.0 && i.component == Some(def.id.0))
                    && root.visual_parent.is_some(),
                "Factory component/attachment mismatch",
            )?;
            let table = doc
                .mappings
                .roots
                .iter()
                .find(|t| t.prototype == root.id)
                .ok_or("Missing OIB root mapping")?;
            require(
                table.cells.len() <= 64
                    && ordered(table.cells.iter().map(|r| r.path.clone()))
                    && editable == table.cells.iter().map(|r| r.path.clone()).collect(),
                "Missing/extra OIB occurrence mappings",
            )?;
            let mut mapped_paths = BTreeSet::new();
            for row in &table.cells {
                require(
                    mapped_paths.insert(&row.path)
                        && row.initial_cell != 0
                        && aliases.insert(row.initial_cell),
                    "Colliding OIB path/key/alias",
                )?;
                let cell = seed
                    .cells
                    .iter()
                    .find(|c| c.id == row.initial_cell)
                    .ok_or("Missing OIB occurrence cell")?;
                let authored = match sources.get(&row.path) {
                    Some(Source::Literal(v)) => Some(v),
                    _ => None,
                };
                require(
                    cell.owner == root.id.0
                        && cell.inherit == terminal_cell(doc, def.id, &row.path)?
                        && cell.inherit.is_some()
                        && cell.default == fallback(doc, def.id, &row.path)?
                        && cell.authored_override.as_ref() == authored,
                    "OIB occurrence cell source mapping mismatch",
                )?;
                let p = property(doc, def.id, &row.path)?;
                require(
                    p.editor == PropertyEditor::Toggle || !cell.choices.is_empty(),
                    "OIB suggestion editor has no choices",
                )?;
                for choice in &cell.choices {
                    validate_property_value(&p, choice)?;
                }
            }
        } else {
            require(
                editable.is_empty(),
                "Fixed OIB shell cannot own mutable occurrence cells",
            )?;
        }
    }
    require(
        aliases == seed.cells.iter().map(|c| c.id).collect()
            && expected_objects
                == doc
                    .mappings
                    .objects
                    .iter()
                    .map(|m| m.address.clone())
                    .collect(),
        "OIB cell/object closure mismatch",
    )?;
    let mut visual_nodes = BTreeSet::new();
    for mapping in &doc.mappings.objects {
        let root = instance(doc, mapping.address.instance)?;
        let n = path_node(
            doc,
            root.definition,
            &mapping.address.references,
            mapping.address.node,
        )?
        .1;
        require(
            visual(doc, n.class)? == mapping.layout_node.is_some()
                && mapping.layout_node.is_none_or(|id| visual_nodes.insert(id)),
            "OIB object visual mapping mismatch",
        )?;
    }
    object_rows(doc)?;
    output_cells(seed)?;
    Ok(())
}

fn find(root: &TemplateNode, id: u16) -> Option<&TemplateNode> {
    if root.id == id {
        Some(root)
    } else {
        root.children.iter().find_map(|c| find(c, id))
    }
}
fn layout_node(blueprint: &Blueprint, id: u16) -> Result<&TemplateNode> {
    blueprint
        .templates
        .iter()
        .find_map(|t| find(&t.root, id))
        .ok_or_else(|| "Missing signed OIB layout mapping".into())
}
fn binding_type(ui: &Ui, binding: Binding) -> Result<ValueType> {
    let Binding::Local(local) = binding;
    let selector = ui
        .selectors
        .iter()
        .find(|s| s.local == local)
        .ok_or("OIB binding has no signed selector")?;
    let field = ui
        .state_schema
        .fields
        .iter()
        .find(|f| f.id == selector.field)
        .ok_or("OIB binding has no signed app field")?;
    let numeric = matches!(field.default, Value::Number(_));
    match &selector.projection {
        Projection::Identity => match field.default {
            Value::Number(_) => Ok(ValueType::Number),
            Value::Boolean(_) => Ok(ValueType::Boolean),
            Value::Text(_) => Ok(ValueType::Text),
            Value::Color(_) => Ok(ValueType::Color),
            _ => Err("Unsupported OIB binding field type".into()),
        },
        Projection::Nonzero if numeric => Ok(ValueType::Boolean),
        Projection::Number { scale, offset }
            if numeric && scale.is_finite() && offset.is_finite() =>
        {
            Ok(ValueType::Number)
        }
        Projection::ChooseText { .. } | Projection::NumberText { .. } | Projection::Percent
            if numeric =>
        {
            Ok(ValueType::Text)
        }
        Projection::ChooseColor { .. } if numeric => Ok(ValueType::Color),
        _ => Err("Unsupported OIB app binding projection".into()),
    }
}
fn validate_bound_control(
    ui: &Ui,
    layout: &TemplateNode,
    kind: ClassKind,
    binding: Binding,
) -> Result<()> {
    let Binding::Local(local) = binding;
    let selector = ui
        .selectors
        .iter()
        .find(|s| s.local == local)
        .ok_or("OIB binding has no signed selector")?;
    let Some(otrios_interfaces::layout::Responder::AppEvent { control, drag }) = layout.responder
    else {
        return Err("OIB bound control has no admitted responder".into());
    };
    let handler = ui
        .handlers
        .iter()
        .find(|h| h.control == control)
        .ok_or("OIB bound control has no admitted handler")?;
    require(
        handler.fields.as_slice() == [selector.field],
        "OIB control and property binding use different app fields",
    )?;
    match kind {
        ClassKind::O2DToggle => {
            require(
                layout
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.checked == Expr::State(binding)),
                "OIB checked expression does not match its declared binding",
            )?;
            require(
                !drag
                    && handler.kind == GestureKind::Activate
                    && handler.operation == Operation::Toggle
                    && selector.projection == Projection::Nonzero,
                "Unsupported OIB toggle binding implementation",
            )
        }
        ClassKind::O2DSlider => require(
            drag && handler.kind == GestureKind::Slider
                && handler.operation == Operation::Set
                && selector.projection == Projection::Identity,
            "Unsupported OIB slider value binding implementation",
        ),
        _ => Err("Unsupported OIB app binding endpoint".into()),
    }
}
pub fn validate_layout(ui: &Ui) -> Result<()> {
    let seed = super::super::seed(ui).ok_or("Missing OIB seed")?;
    let doc = metadata(seed)?;
    let blueprint = Layout::from_bytes(&ui.layout)?.blueprint().clone();
    if let Some(id) = doc.hierarchy_viewport {
        require(
            matches!(
                layout_node(&blueprint, id)?.shape,
                Shape::Canvas { clip: true }
            ),
            "OIB hierarchy must be a clipped viewport",
        )?;
    }
    for mapping in &doc.mappings.objects {
        let Some(id) = mapping.layout_node else {
            continue;
        };
        let root = instance(doc, mapping.address.instance)?;
        let (def, n) = path_node(
            doc,
            root.definition,
            &mapping.address.references,
            mapping.address.node,
        )?;
        let layout = layout_node(&blueprint, id)?;
        require(
            match class(doc, n.class)?.implementation {
                ClassKind::OIBObject | ClassKind::OIBDocument => false,
                ClassKind::O2DText => matches!(layout.shape, Shape::Text { .. }),
                ClassKind::O2DShape => matches!(
                    layout.shape,
                    Shape::Superellipse { .. } | Shape::Circle { .. }
                ),
                _ => matches!(
                    layout.shape,
                    Shape::Canvas { .. } | Shape::Superellipse { .. }
                ),
            },
            "OIB class/layout implementation mismatch",
        )?;
        let mut expected_children = Vec::new();
        for child in &n.visual_children {
            expected_children.push(ObjectAddress {
                instance: root.id,
                references: mapping.address.references.clone(),
                node: *child,
            });
        }
        if let Some(r) = &n.reference {
            let mut references = mapping.address.references.clone();
            references.push(n.id);
            expected_children.push(ObjectAddress {
                instance: root.id,
                references,
                node: definition(doc, r.definition)?.root,
            });
        }
        let mut previous = None;
        for child in expected_children {
            let child_id = doc
                .mappings
                .objects
                .iter()
                .find(|m| m.address == child)
                .and_then(|m| m.layout_node)
                .ok_or("Missing visual OIB child mapping")?;
            let rank = layout
                .children
                .iter()
                .position(|n| n.id == child_id)
                .ok_or("OIB visual mapping does not match signed containment")?;
            require(
                previous.is_none_or(|p| p < rank),
                "OIB signed visual order mismatch",
            )?;
            previous = Some(rank);
        }
        if mapping.address.references.is_empty()
            && mapping.address.node == def.root
            && let Some(parent) = &root.visual_parent
        {
            let parent_id = doc
                .mappings
                .objects
                .iter()
                .find(|m| m.address == *parent)
                .and_then(|m| m.layout_node)
                .ok_or("Missing document visual parent mapping")?;
            require(
                layout_node(&blueprint, parent_id)?
                    .children
                    .iter()
                    .any(|n| n.id == id),
                "OIB root does not retain signed parent attachment",
            )?;
            if doc.mappings.roots.iter().any(|t| t.prototype == root.id) {
                require(
                    seed.instance_surface.as_ref().is_some_and(|s| {
                        s.preview_viewport == parent_id
                            && s.prototypes
                                .iter()
                                .any(|p| p.instance == root.id.0 && p.preview_root == id)
                    }),
                    "OIB factory root/surface attachment mismatch",
                )?;
            }
        }
    }
    fn flow_nodes(flow: &otrios_interfaces::layout::Flow, out: &mut BTreeSet<u16>) {
        use otrios_interfaces::layout::FlowKind;
        match &flow.kind {
            FlowKind::Item { node, .. } => {
                out.insert(*node);
            }
            FlowKind::Wrap { nodes, .. } => out.extend(nodes),
            FlowKind::Group { children, .. } => {
                for child in children {
                    flow_nodes(child, out);
                }
            }
        }
    }
    let mut arranged = BTreeSet::new();
    if let Some(composition) = &blueprint.composition {
        flow_nodes(&composition.wide.content, &mut arranged);
        flow_nodes(&composition.tall.content, &mut arranged);
    }
    for def in &doc.definitions {
        for slot in &def.slots {
            for occurrence in &doc.mappings.objects {
                let root = instance(doc, occurrence.address.instance)?;
                let (scope, object) = path_node(
                    doc,
                    root.definition,
                    &occurrence.address.references,
                    occurrence.address.node,
                )?;
                if scope.id != def.id || object.id != def.root {
                    continue;
                }
                let mut references = occurrence.address.references.clone();
                references.extend(&slot.references);
                let address = ObjectAddress {
                    instance: root.id,
                    references,
                    node: slot.node,
                };
                let host = doc
                    .mappings
                    .objects
                    .iter()
                    .find(|m| m.address == address)
                    .and_then(|m| m.layout_node)
                    .ok_or("OIB layout slot must have a visual host")?;
                let children = &layout_node(&blueprint, host)?.children;
                let any_arranged = children.iter().any(|n| arranged.contains(&n.id));
                require(
                    match slot.layout {
                        SlotLayout::Absolute => !any_arranged,
                        SlotLayout::ExistingFlow => {
                            !children.is_empty()
                                && children.iter().all(|n| arranged.contains(&n.id))
                        }
                    },
                    "OIB slot layout assertion does not match signed layout",
                )?;
            }
        }
    }
    for root in &doc.instances {
        let graph = sources(doc, root)?;
        for path in graph.keys() {
            let Some(binding) = bound_source(&graph, path, &mut BTreeSet::new())? else {
                continue;
            };
            require(
                binding_type(ui, binding)? == property(doc, root.definition, path)?.value_type,
                "OIB app binding type mismatch",
            )?;
            let (_, object) = path_node(doc, root.definition, &path.references, path.node)?;
            let kind = class(doc, object.class)?.implementation;
            if matches!(kind, ClassKind::O2DToggle | ClassKind::O2DSlider) {
                let address = ObjectAddress {
                    instance: root.id,
                    references: path.references.clone(),
                    node: path.node,
                };
                let id = doc
                    .mappings
                    .objects
                    .iter()
                    .find(|m| m.address == address)
                    .and_then(|m| m.layout_node)
                    .ok_or("Missing bound OIB control mapping")?;
                validate_bound_control(ui, layout_node(&blueprint, id)?, kind, binding)?;
            } else {
                // Exposed sources may forward through containers, but every
                // terminal binding must reach a supported, verified control.
                require(
                    graph
                        .values()
                        .any(|s| matches!(s, Source::Forward(from) if from == path)),
                    "Unsupported OIB app binding endpoint",
                )?;
            }
        }
    }
    // Canonical visual parameters may read any declared property in the same
    // component occurrence. Legacy derived cells use their object's subtree.
    // Shared definition outputs may cover several occurrences; occurrence outputs
    // must resolve to the exact full path, including every reference boundary.
    let mut outputs: BTreeMap<(u16, u8), u64> = BTreeMap::new();
    for cell in &seed.cells {
        let mut containers = Vec::new();
        if let Some(m) = doc.mappings.definitions.iter().find(|m| m.cell == cell.id) {
            for mapping in &doc.mappings.objects {
                let root = instance(doc, mapping.address.instance)?;
                let (def, n) = path_node(
                    doc,
                    root.definition,
                    &mapping.address.references,
                    mapping.address.node,
                )?;
                if def.id == m.definition
                    && n.id == m.node
                    && let Some(id) = mapping.layout_node
                {
                    containers.push(id);
                }
            }
        } else {
            let (table, row) = doc
                .mappings
                .roots
                .iter()
                .find_map(|t| {
                    t.cells
                        .iter()
                        .find(|r| r.initial_cell == cell.id)
                        .map(|r| (t, r))
                })
                .ok_or("Unmapped OIB output cell")?;
            let address = if seed.graph.is_some() {
                let root = instance(doc, table.prototype)?;
                ObjectAddress {
                    instance: table.prototype,
                    references: vec![],
                    node: definition(doc, root.definition)?.root,
                }
            } else {
                ObjectAddress {
                    instance: table.prototype,
                    references: row.path.references.clone(),
                    node: row.path.node,
                }
            };
            if let Some(id) = doc
                .mappings
                .objects
                .iter()
                .find(|m| m.address == address)
                .and_then(|m| m.layout_node)
            {
                containers.push(id);
            }
        }
        for target in &cell.targets {
            require(
                containers.iter().any(|id| {
                    layout_node(&blueprint, *id).is_ok_and(|n| find(n, target.node).is_some())
                }),
                "OIB output escapes signed object closure",
            )?;
            let expected = match target.property {
                Property::Text | Property::SemanticLabel => ValueType::Text,
                Property::Color => ValueType::Color,
                Property::Visible => ValueType::Boolean,
                _ => ValueType::Number,
            };
            let actual = match cell.default {
                Value::Text(_) => ValueType::Text,
                Value::Number(_) => ValueType::Number,
                Value::Boolean(_) => ValueType::Boolean,
                Value::Color(_) => ValueType::Color,
                _ => return Err("Unsupported OIB output value".into()),
            };
            require(actual == expected, "OIB output type mismatch")?;
            let key = (
                target.node,
                if target.property == Property::FrameWidth {
                    Property::Width as u8
                } else {
                    target.property as u8
                },
            );
            if let Some(previous) = outputs.insert(key, cell.id) {
                let previous = seed
                    .cells
                    .iter()
                    .find(|c| c.id == previous)
                    .ok_or("Missing OIB output source")?;
                require(
                    previous.inherit == Some(cell.id) || cell.inherit == Some(previous.id),
                    "Conflicting OIB terminal outputs",
                )?;
            }
        }
    }
    Ok(())
}
