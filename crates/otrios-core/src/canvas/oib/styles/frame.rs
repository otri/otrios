// SPDX-License-Identifier: LGPL-3.0-only
//! One immutable, bounded set of receiving-component style contributions.
use super::*;
use crate::canvas::oib::{Allocation, Seed};
use otrios_interfaces::oib::{
    designer::{ObjectAddress, PropertyAddress},
    graph,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contribution {
    pub style: StyleRef,
    pub field: FieldId,
    pub source: ResolvedField,
}
pub struct Occurrence<'a> {
    pub component: &'a ComponentStyle,
    pub scope: &'a ObjectAddress,
    pub context: Context<'a>,
}
/// A presentation input, never a causal value map or a package admission grant.
/// Source/allocations are pinned so a frame cannot be applied to another graph.
pub struct Frame {
    graph: graph::Document,
    libraries: Vec<graph::ResolvedLibrary>,
    allocations: Vec<Allocation>,
    colors: BTreeMap<PropertyAddress, Contribution>,
    focus: BTreeMap<ObjectAddress, BTreeMap<FieldId, ResolvedField>>,
}
impl Frame {
    pub fn new(
        seed: &Seed,
        allocations: &[Allocation],
        catalog: &Admitted,
        uses: &[Occurrence<'_>],
    ) -> Result<Self> {
        if uses.len() > MAX_ENTRIES || allocations.len() > otrios_interfaces::oib::MAX_ALLOCATIONS {
            return Err("Style occurrence limit exceeded".into());
        }
        let mut overrides = 0;
        for usage in uses {
            add(&mut overrides, usage.context.document.len(), MAX_OVERRIDES)?;
            add(&mut overrides, usage.context.instance.len(), MAX_OVERRIDES)?;
        }
        let mut result = Self {
            graph: seed
                .graph
                .as_ref()
                .ok_or("Missing canonical styled source")?
                .clone(),
            libraries: seed.resolved_libraries.clone(),
            allocations: allocations.to_vec(),
            colors: BTreeMap::new(),
            focus: BTreeMap::new(),
        };
        let mut count = 0;
        for usage in uses {
            let mapped = Mapped::new(catalog, seed, usage.component.clone())?;
            let (colors, focus) = mapped.contributions(usage.scope, allocations, usage.context)?;
            add(&mut count, colors.len(), MAX_ENTRIES)?;
            add(&mut count, focus.len(), MAX_ENTRIES)?;
            if result.focus.insert(usage.scope.clone(), focus).is_some() {
                return Err("Duplicate styled component scope".into());
            }
            for (address, color) in colors {
                if result.colors.insert(address, color).is_some() {
                    return Err("Competing style contributions for one property".into());
                }
            }
        }
        Ok(result)
    }
    pub(crate) fn validate(&self, seed: &Seed, allocations: &[Allocation]) -> Result<()> {
        if seed.graph.as_ref() != Some(&self.graph)
            || seed.resolved_libraries != self.libraries
            || allocations != self.allocations
        {
            return Err("Style frame belongs to a different canonical source or allocation".into());
        }
        Ok(())
    }
    pub(crate) fn validate_source(&self, seed: &Seed) -> Result<()> {
        self.validate_compiler(seed, &self.allocations)
    }
    pub(crate) fn validate_compiler(&self, seed: &Seed, allocations: &[Allocation]) -> Result<()> {
        self.validate(seed, allocations)?;
        if self.focus.values().any(|fields| !fields.is_empty()) {
            return Err("Style focus-overlay compiler capability is not active".into());
        }
        Ok(())
    }
    pub(crate) fn color(&self, address: &PropertyAddress) -> Option<&Contribution> {
        self.colors.get(address)
    }
    pub(super) fn colors(&self) -> &BTreeMap<PropertyAddress, Contribution> {
        &self.colors
    }
    pub fn focus(&self) -> &BTreeMap<ObjectAddress, BTreeMap<FieldId, ResolvedField>> {
        &self.focus
    }
}

/// Build the same receiving-mode frame for rendering, inspection and guest reads.
/// Local authoring currently has a single Light/Dark mode dimension; each use
/// keeps its canonical source/occurrence identity across that local change.
pub(crate) fn automatic(
    seed: &Seed,
    allocations: &[Allocation],
    mode: Mode,
) -> Result<Option<Frame>> {
    use otrios_interfaces::oib::designer::{DefinitionId, NodeId};
    if seed
        .graph
        .as_ref()
        .is_none_or(|graph| graph.styles.is_none())
    {
        return Ok(None);
    }
    let Some(catalog) = super::local_admitted(seed)? else {
        return Ok(None);
    };
    let payload = seed
        .graph
        .as_ref()
        .and_then(|graph| graph.styles.as_ref())
        .ok_or("Missing local style source")?;
    if payload.components.is_empty() {
        return Ok(None);
    }
    let doc = crate::canvas::oib::document::expanded_metadata(seed, allocations)?;
    fn walk(
        doc: &otrios_interfaces::oib::designer::Document,
        payload: &DocumentStyles,
        definition: DefinitionId,
        instance: otrios_interfaces::oib::designer::InstanceId,
        prefix: &[NodeId],
        uses: &mut Vec<(ComponentStyle, ObjectAddress)>,
    ) -> Result<()> {
        if prefix.len() > 8 {
            return Err("Style component ancestry limit".into());
        }
        let definition = doc
            .definitions
            .iter()
            .find(|candidate| candidate.id == definition)
            .ok_or("Unknown styled definition")?;
        if let Some(binding) = payload
            .components
            .iter()
            .find(|binding| binding.definition == definition.id)
        {
            if uses.len() >= MAX_ENTRIES {
                return Err("Style occurrence limit".into());
            }
            uses.push((
                binding.clone(),
                ObjectAddress {
                    instance,
                    references: prefix.to_vec(),
                    node: definition.root,
                },
            ));
        }
        for node in &definition.nodes {
            if let Some(reference) = &node.reference {
                let mut child = prefix.to_vec();
                child.push(node.id);
                walk(doc, payload, reference.definition, instance, &child, uses)?;
            }
        }
        Ok(())
    }
    let mut bindings = vec![];
    for root in &doc.instances {
        walk(&doc, payload, root.definition, root.id, &[], &mut bindings)?;
    }
    let uses: Vec<_> = bindings
        .iter()
        .map(|(component, scope)| Occurrence {
            component,
            scope,
            context: Context {
                mode,
                state: StateMask::default(),
                document: &[],
                instance: &[],
            },
        })
        .collect();
    Frame::new(seed, allocations, &catalog, &uses).map(Some)
}
