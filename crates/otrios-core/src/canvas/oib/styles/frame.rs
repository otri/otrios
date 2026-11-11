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
