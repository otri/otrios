// SPDX-License-Identifier: LGPL-3.0-only
//! Local receiving-mode source queries use the same immutable style payload as
//! compilation. No inspector/default color is written into causal state.
use super::*;
use crate::canvas::oib::{document, styles};
use otrios_interfaces::oib::designer::{PropertyAddress, Selection};

#[derive(Clone, Copy)]
enum SourceScope {
    Occurrence,
    Definition(otrios_interfaces::oib::designer::DefinitionId),
    Other,
}
struct SourceContext<'a> {
    seed: &'a otrios_interfaces::oib::Seed,
    allocations: &'a [otrios_interfaces::oib::Allocation],
    values: &'a BTreeMap<u64, Option<Value>>,
    frame: Option<styles::Frame>,
    resolved: Option<styles::Resolved>,
}
impl<'a> SourceContext<'a> {
    fn new(
        seed: &'a otrios_interfaces::oib::Seed,
        allocations: &'a [otrios_interfaces::oib::Allocation],
        values: &'a BTreeMap<u64, Option<Value>>,
        mode: otrios_interfaces::oib::styles::Mode,
        scope: SourceScope,
    ) -> Result<Self> {
        let has_styles = seed
            .graph
            .as_ref()
            .is_some_and(|graph| graph.styles.is_some());
        let frame = if has_styles && matches!(scope, SourceScope::Occurrence) {
            styles::automatic(seed, allocations, mode)?
        } else {
            None
        };
        let resolved = if let SourceScope::Definition(id) = scope {
            if has_styles {
                let catalog = styles::local_admitted(seed)?;
                let binding = seed
                    .graph
                    .as_ref()
                    .and_then(|graph| graph.styles.as_ref())
                    .and_then(|payload| {
                        payload
                            .components
                            .iter()
                            .find(|binding| binding.definition == id)
                    });
                if let (Some(catalog), Some(binding)) = (catalog, binding) {
                    let definition = seed
                        .designer
                        .as_ref()
                        .and_then(|doc| {
                            doc.definitions
                                .iter()
                                .find(|definition| definition.id == id)
                        })
                        .ok_or("Missing styled definition")?;
                    let class = definition
                        .nodes
                        .iter()
                        .find(|node| node.id == definition.root)
                        .ok_or("Missing styled root")?
                        .class;
                    Some(catalog.resolve(
                        &binding.style,
                        class,
                        mode,
                        otrios_interfaces::oib::styles::StateMask::default(),
                        &[],
                        &[],
                    )?)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        Ok(Self {
            seed,
            allocations,
            values,
            frame,
            resolved,
        })
    }
    fn at(&self, address: &PropertyAddress) -> Result<document::PropertySources> {
        if let Some(frame) = &self.frame {
            return document::property_sources_styled(
                self.seed,
                self.allocations,
                address,
                self.values,
                frame,
            );
        }
        let seed = self.seed;
        let mut trace = document::property_sources(seed, self.allocations, address, self.values)?;
        let PropertyAddress::Definition {
            definition,
            node,
            property,
        } = address
        else {
            return Ok(trace);
        };
        let Some(payload) = seed.graph.as_ref().and_then(|graph| graph.styles.as_ref()) else {
            return Ok(trace);
        };
        let Some(binding) = payload
            .components
            .iter()
            .find(|binding| binding.definition == *definition)
        else {
            return Ok(trace);
        };
        let Some(mapping) = binding.fields.iter().find(|mapping| {
            mapping.targets.iter().any(|target| {
                target.references.is_empty() && target.node == *node && target.property == *property
            })
        }) else {
            return Ok(trace);
        };
        let Some(resolved) = &self.resolved else {
            return Ok(trace);
        };
        let color = resolved
            .body
            .get(&mapping.field)
            .ok_or("Missing styled field")?;
        let value = Value::Color(color.color);
        let authored = trace
            .hops
            .iter()
            .any(|hop| matches!(hop.role, document::SourceRole::NodeDefault));
        let masked = authored || trace.overridden;
        for hop in &mut trace.hops {
            if matches!(
                hop.role,
                document::SourceRole::ClassDefault | document::SourceRole::DefinitionDefault
            ) {
                hop.masked = true;
            }
        }
        if !masked {
            trace.value = Some(value.clone());
        }
        if !authored {
            trace.inherited = Some(value.clone());
        }
        let at = trace
            .hops
            .iter()
            .position(|hop| {
                matches!(
                    hop.role,
                    document::SourceRole::ClassDefault | document::SourceRole::DefinitionDefault
                )
            })
            .unwrap_or(trace.hops.len());
        trace.hops.insert(
            at,
            document::SourceHop {
                address: address.clone(),
                role: document::SourceRole::Style(Box::new(styles::Contribution {
                    style: binding.style.clone(),
                    field: mapping.field,
                    source: color.clone(),
                })),
                cell: None,
                value: Some(value),
                masked,
            },
        );
        Ok(trace)
    }
}

impl StateAdapter {
    pub(in crate::canvas) fn oib_style_sources(
        &self,
        seed: &otrios_interfaces::oib::Seed,
        address: &PropertyAddress,
    ) -> Result<document::PropertySources> {
        let allocations = self.oib_allocations()?;
        let values = self.oib_active()?;
        let scope = match address {
            PropertyAddress::Occurrence { .. } => SourceScope::Occurrence,
            PropertyAddress::Definition { definition, .. } => SourceScope::Definition(*definition),
            _ => SourceScope::Other,
        };
        SourceContext::new(seed, &allocations, &values, self.oib_style_mode, scope)?.at(address)
    }
    pub(in crate::canvas) fn oib_presented_properties(
        &self,
        selection: &Selection,
    ) -> Result<Vec<document::PropertyInfo>> {
        let seed = self.oib_authoring_seed()?;
        let allocations = self.oib_allocations()?;
        let values = self.oib_active()?;
        let mut properties = document::properties(&seed, &allocations, selection, &values)?;
        if seed
            .graph
            .as_ref()
            .is_none_or(|graph| graph.styles.is_none())
        {
            return Ok(properties);
        }
        let scope = match selection {
            Selection::Object(_) => SourceScope::Occurrence,
            Selection::Definition(definition) | Selection::DefinitionNode { definition, .. } => {
                SourceScope::Definition(*definition)
            }
            _ => SourceScope::Other,
        };
        let context = SourceContext::new(&seed, &allocations, &values, self.oib_style_mode, scope)?;
        for property in &mut properties {
            let Some(address) = &property.address else {
                continue;
            };
            let trace = context.at(address)?;
            if !trace
                .hops
                .iter()
                .any(|hop| matches!(hop.role, document::SourceRole::Style(_)))
            {
                continue;
            }
            property.value = trace.value;
            property.inherited = trace.inherited;
            if trace
                .hops
                .iter()
                .any(|hop| !hop.masked && matches!(hop.role, document::SourceRole::Style(_)))
            {
                property.origin = "style".into();
                property.source = "style".into();
            }
        }
        Ok(properties)
    }
}
