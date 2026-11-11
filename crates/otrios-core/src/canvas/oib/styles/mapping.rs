// SPDX-License-Identifier: LGPL-3.0-only
//! Typed component color inputs over the canonical property resolver. Mapping
//! queries are not edits or compiler activation.
use super::*;
use crate::canvas::oib::{Allocation, Seed, document};
use document::{PropertySources, SourceRole};
use otrios_interfaces::{
    layout::{Value, ValueType},
    oib::designer::{ObjectAddress, PropertyAddress, PropertyEditor},
};

pub struct Mapped<'a> {
    catalog: &'a Admitted,
    seed: &'a Seed,
    binding: ComponentStyle,
    class: ClassId,
}
/// Mode and state are receiving-component presentation context; no input or
/// authored app preference is changed by this query.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub mode: Mode,
    pub state: StateMask,
    pub document: &'a [Override],
    pub instance: &'a [Override],
}
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyPaint {
    Style {
        field: FieldId,
        source: ResolvedField,
        /// Native canonical trace, including the winning style and masked fallbacks.
        trace: PropertySources,
    },
    /// Includes unresolved app bindings. Do not turn these into literal colors.
    Explicit(PropertySources),
}
/// One component occurrence's property sources from the native styled resolver.
/// This query does not publish edits or activate a package capability.
pub struct Projection {
    properties: BTreeMap<PropertyAddress, PropertyPaint>,
    focus: BTreeMap<FieldId, ResolvedField>,
}
impl Projection {
    pub fn properties(&self) -> &BTreeMap<PropertyAddress, PropertyPaint> {
        &self.properties
    }
    pub fn focus(&self) -> &BTreeMap<FieldId, ResolvedField> {
        &self.focus
    }
}
fn admitted_owner(seed: &Seed, owner: &Owner) -> bool {
    match owner {
        Owner::Document(id) => seed.graph.as_ref().is_some_and(|graph| graph.id == *id),
        Owner::Library { library, revision } => seed
            .resolved_libraries
            .iter()
            .any(|resolved| resolved.library == *library && resolved.revision == *revision),
    }
}
impl<'a> Mapped<'a> {
    /// `seed` must already have passed canonical graph/package admission. Owner
    /// membership here does not authenticate its transported library bodies.
    pub fn new(catalog: &'a Admitted, seed: &'a Seed, binding: ComponentStyle) -> Result<Self> {
        // Catalog admission is not a cross-document or ambient-library bridge.
        if catalog
            .catalog
            .systems
            .iter()
            .any(|system| !admitted_owner(seed, &system.reference.owner))
        {
            return Err("Style owner is outside this exact component closure".into());
        }
        let doc = document::metadata(seed)?;
        let definition = doc
            .definitions
            .iter()
            .find(|d| d.id == binding.definition)
            .ok_or("Unknown styled definition")?;
        let root = definition
            .nodes
            .iter()
            .find(|n| n.id == definition.root)
            .ok_or("Missing styled root")?;
        let (contract, _) = catalog.style(&binding.style)?;
        if !contract.classes.contains(&root.class) {
            return Err("Style contract does not apply to component class".into());
        }
        ordered(binding.fields.iter().map(|mapping| mapping.field))?;
        let body: Vec<_> = contract
            .fields
            .iter()
            .filter(|field| field.domain == Domain::Body)
            .collect();
        if binding.fields.len() != body.len()
            || binding
                .fields
                .iter()
                .zip(&body)
                .any(|(mapping, field)| mapping.field != field.id)
        {
            return Err("Component style must map each body field exactly once".into());
        }
        let mut paths = BTreeSet::new();
        let mut count = 0;
        for mapping in &binding.fields {
            if mapping.targets.is_empty() {
                return Err("Empty style property mapping".into());
            }
            add(&mut count, mapping.targets.len(), MAX_ENTRIES)?;
            ordered(mapping.targets.iter())?;
            for path in &mapping.targets {
                if !paths.insert(path) {
                    return Err("Two style fields target the same canonical property".into());
                }
                let property = document::property(doc, binding.definition, path)?;
                if property.value_type != ValueType::Color
                    || !property.policy.live_literal
                    || property.editor == PropertyEditor::ReadOnly
                {
                    return Err(
                        "Style target must be an admitted literal-capable Color property".into(),
                    );
                }
            }
        }
        Ok(Self {
            catalog,
            seed,
            class: root.class,
            binding,
        })
    }
    pub(super) fn contributions(
        &self,
        scope: &ObjectAddress,
        allocations: &[Allocation],
        context: Context<'_>,
    ) -> Result<(
        BTreeMap<PropertyAddress, Contribution>,
        BTreeMap<FieldId, ResolvedField>,
    )> {
        let expanded = document::expanded_metadata(self.seed, allocations)?;
        let root = expanded
            .instances
            .iter()
            .find(|instance| instance.id == scope.instance)
            .ok_or("Unknown styled occurrence")?;
        let (definition, node) =
            document::path_node(&expanded, root.definition, &scope.references, scope.node)?;
        if definition.id != self.binding.definition || node.id != definition.root {
            return Err("Style occurrence does not match its admitted component root".into());
        }
        let resolved = self.catalog.resolve(
            &self.binding.style,
            self.class,
            context.mode,
            context.state,
            context.document,
            context.instance,
        )?;
        let mut colors = BTreeMap::new();
        let imported = crate::canvas::oib::imports::imported_definitions(self.seed);
        for mapping in &self.binding.fields {
            let source = resolved
                .body
                .get(&mapping.field)
                .ok_or("Missing resolved body field")?;
            for target in &mapping.targets {
                let mut path = target.clone();
                path.references
                    .splice(0..0, scope.references.iter().copied());
                let address = PropertyAddress::Occurrence {
                    instance: scope.instance,
                    path,
                };
                let PropertyAddress::Occurrence { path, .. } = &address else {
                    unreachable!()
                };
                let (target, _) =
                    document::path_node(&expanded, root.definition, &path.references, path.node)?;
                colors.insert(
                    address,
                    Contribution {
                        style: self.binding.style.clone(),
                        field: mapping.field,
                        source: source.clone(),
                        library_default: imported.contains(&target.id),
                    },
                );
            }
        }
        Ok((colors, resolved.focus))
    }
    pub fn resolve(
        &self,
        scope: &ObjectAddress,
        allocations: &[Allocation],
        values: &BTreeMap<u64, Option<Value>>,
        context: Context<'_>,
    ) -> Result<Projection> {
        let frame = Frame::new(
            self.seed,
            allocations,
            self.catalog,
            &[Occurrence {
                component: &self.binding,
                scope,
                context,
            }],
        )?;
        let mut result = Projection {
            properties: BTreeMap::new(),
            focus: frame.focus().get(scope).cloned().unwrap_or_default(),
        };
        for address in frame.colors().keys() {
            let trace =
                document::property_sources_styled(self.seed, allocations, address, values, &frame)?;
            let winning = trace.hops.iter().find_map(|hop| match &hop.role {
                SourceRole::Style(contribution) if !hop.masked => {
                    Some(contribution.as_ref().clone())
                }
                _ => None,
            });
            let paint = if let Some(winning) = winning {
                PropertyPaint::Style {
                    field: winning.field,
                    source: winning.source,
                    trace,
                }
            } else {
                PropertyPaint::Explicit(trace)
            };
            result.properties.insert(address.clone(), paint);
        }
        Ok(result)
    }
}
