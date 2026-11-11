// SPDX-License-Identifier: LGPL-3.0-only
//! Local, bounded provenance over the same disposable sources used by resolve.
use super::*;

/// Local authoring query, not a signed authority proof. Alternatives are folded
/// by equal value in canonical register-dot order, including causal clear.
/// The admitted Merkle register bounds this list to MAX_ACTORS alternatives.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyRegisterView {
    pub address: otrios_interfaces::oib::designer::PropertyAddress,
    pub cell: u64,
    /// Exact Merkle leaf version includes all causal metadata, even equal writes.
    pub version: crate::merkle::Hash,
    pub alternatives: Vec<Option<Value>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoredOrigin {
    Instance(InstanceId),
    Reference {
        definition: DefinitionId,
        node: NodeId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceRole {
    LiveOverride,
    AuthoredLiteral(AuthoredOrigin),
    Forward {
        from: PropertyAddress,
    },
    AppBinding(Binding),
    SharedRegister,
    /// Boxed to keep ordinary non-style trace rows small.
    Style(Box<super::super::styles::Contribution>),
    NodeDefault,
    DefinitionDefault,
    ClassDefault,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceHop {
    pub address: PropertyAddress,
    pub role: SourceRole,
    pub cell: Option<u64>,
    pub value: Option<Value>,
    /// This layer cannot affect the requested value until a preceding layer clears.
    pub masked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertySources {
    pub address: PropertyAddress,
    pub value: Option<Value>,
    /// Current resolved value with only this address's live override ignored.
    pub inherited: Option<Value>,
    pub overridden: bool,
    pub read_only: bool,
    pub binding: Option<Binding>,
    /// First forwarded address beneath the local override, if any.
    pub immediate_source: Option<PropertyAddress>,
    /// Shared source reached by forwarding, never a masked nested default.
    /// May itself be masked by a live override (see hops).
    pub terminal_shared: Option<PropertyAddress>,
    pub hops: Vec<SourceHop>,
}

fn declaration(
    doc: &Document,
    root: &ComponentInstance,
    path: &PropertyPath,
) -> Result<AuthoredOrigin> {
    if root.assignments.iter().any(|a| a.target == *path) {
        return Ok(AuthoredOrigin::Instance(root.id));
    }
    for i in 0..path.references.len() {
        let (def, node) = path_node(
            doc,
            root.definition,
            &path.references[..i],
            path.references[i],
        )?;
        let relative = PropertyPath {
            references: path.references[i + 1..].to_vec(),
            node: path.node,
            property: path.property,
        };
        if node
            .reference
            .as_ref()
            .is_some_and(|r| r.assignments.iter().any(|a| a.target == relative))
        {
            return Ok(AuthoredOrigin::Reference {
                definition: def.id,
                node: node.id,
            });
        }
    }
    Err("Missing authored OIB source declaration".into())
}

fn defaults(
    doc: &Document,
    start: DefinitionId,
    path: &PropertyPath,
    masked: bool,
    style: Option<(&PropertyAddress, &super::super::styles::Contribution)>,
    hops: &mut Vec<SourceHop>,
) -> Result<()> {
    let (def, node) = path_node(doc, start, &path.references, path.node)?;
    let address = PropertyAddress::Definition {
        definition: def.id,
        node: node.id,
        property: path.property,
    };
    let authored = node.defaults.iter().find(|d| d.property == path.property);
    if let Some(default) = authored {
        hops.push(SourceHop {
            address: address.clone(),
            role: SourceRole::NodeDefault,
            cell: None,
            value: Some(default.value.clone()),
            masked,
        });
    }
    if let Some((at, style)) = style {
        hops.push(SourceHop {
            address: at.clone(),
            role: SourceRole::Style(Box::new(style.clone())),
            cell: None,
            value: Some(Value::Color(style.source.color)),
            masked: masked || authored.is_some(),
        });
    }
    hops.push(SourceHop {
        address,
        role: if node.id == def.root && def.properties.iter().any(|p| p.id == path.property) {
            SourceRole::DefinitionDefault
        } else {
            SourceRole::ClassDefault
        },
        cell: None,
        value: Some(property(doc, start, path)?.default),
        masked: masked || authored.is_some() || style.is_some(),
    });
    Ok(())
}

/// Trace admitted active values, not unresolved draft registers. At most 64
/// forwarding paths (the resolver's bound), with at most five layers per path.
/// Authored literals/defaults are immutable declarations, not editable registers.
pub fn property_sources(
    seed: &Seed,
    allocations: &[Allocation],
    address: &PropertyAddress,
    active_values: &BTreeMap<u64, Option<Value>>,
) -> Result<PropertySources> {
    property_sources_impl(seed, allocations, address, active_values, None)
}
/// Trace the same native style fallback used by graph compilation. The style
/// frame must belong to this exact canonical source and allocation set.
pub fn property_sources_styled(
    seed: &Seed,
    allocations: &[Allocation],
    address: &PropertyAddress,
    active_values: &BTreeMap<u64, Option<Value>>,
    styles: &super::super::styles::Frame,
) -> Result<PropertySources> {
    styles.validate(seed, allocations)?;
    property_sources_impl(seed, allocations, address, active_values, Some(styles))
}
fn property_sources_impl(
    seed: &Seed,
    allocations: &[Allocation],
    address: &PropertyAddress,
    active_values: &BTreeMap<u64, Option<Value>>,
    styles: Option<&super::super::styles::Frame>,
) -> Result<PropertySources> {
    let mut expanded = seed.clone();
    expanded.designer = Some(expanded_metadata(seed, allocations)?);
    let doc = metadata(&expanded)?;
    let mut result = PropertySources {
        address: address.clone(),
        value: None,
        inherited: None,
        overridden: false,
        read_only: false,
        binding: None,
        immediate_source: None,
        terminal_shared: None,
        hops: Vec::new(),
    };
    match address {
        PropertyAddress::Structural { .. } => {
            result.read_only = false;
        }
        PropertyAddress::Placement { object, .. } => {
            instance(doc, object.instance)?;
            // Automatic layout has no authored scalar default to pretend is a
            // class/definition register. The inspector supplies its resolved frame.
            result.read_only = false;
        }
        PropertyAddress::Definition {
            definition,
            node,
            property: id,
        } => {
            let path = PropertyPath {
                references: vec![],
                node: *node,
                property: *id,
            };
            let descriptor = property(doc, *definition, &path)?;
            let cell = terminal_cell(doc, *definition, &path)?;
            let value = cell.and_then(|c| active_values.get(&c)).cloned().flatten();
            result.overridden = value.is_some();
            result.inherited = Some(fallback(doc, *definition, &path)?);
            result.value = value.clone().or(result.inherited.clone());
            if let Some(value) = &result.value {
                validate_property_value(&descriptor, value)?;
            }
            result.read_only = cell.is_none()
                || !descriptor.policy.live_literal
                || descriptor.editor == PropertyEditor::ReadOnly;
            if let Some(cell) = cell {
                result.terminal_shared = Some(address.clone());
                result.hops.push(SourceHop {
                    address: address.clone(),
                    role: SourceRole::SharedRegister,
                    cell: Some(cell),
                    masked: false,
                    value,
                });
            }
            defaults(
                doc,
                *definition,
                &path,
                result.overridden,
                None,
                &mut result.hops,
            )?;
        }
        PropertyAddress::Occurrence { instance: id, path } => {
            let root = instance(doc, *id)?;
            let source = sources(doc, root)?;
            let descriptor = property(doc, root.definition, path)?;
            result.binding = bound_source(&source, path, &mut BTreeSet::new())?;
            result.read_only =
                result.binding.is_some() || cell_for_address(seed, allocations, address).is_err();
            let local = occurrence_cell(doc, root.id, path);
            result.overridden = local
                .and_then(|c| active_values.get(&c))
                .is_some_and(Option::is_some);
            let resolution = Resolution {
                seed: &expanded,
                root,
                source: &source,
                values: active_values,
                ignore: None,
                styles,
            };
            result.value = resolution.at(path, &mut BTreeSet::new())?;
            result.inherited = Resolution {
                ignore: local,
                ..resolution
            }
            .at(path, &mut BTreeSet::new())?;
            let mut current = path.clone();
            let mut seen = BTreeSet::new();
            let mut masked = false;
            loop {
                if !seen.insert(current.clone()) || seen.len() > 64 {
                    return Err("Cyclic OIB property forwarding".into());
                }
                let at = PropertyAddress::Occurrence {
                    instance: *id,
                    path: current.clone(),
                };
                if let Some(cell) = occurrence_cell(doc, *id, &current) {
                    let value = active_values.get(&cell).cloned().flatten();
                    result.hops.push(SourceHop {
                        address: at.clone(),
                        role: SourceRole::LiveOverride,
                        cell: Some(cell),
                        value: value.clone(),
                        masked,
                    });
                    masked |= value.is_some();
                }
                let next = match source.get(&current) {
                    Some(Source::Literal(value)) => {
                        result.hops.push(SourceHop {
                            address: at.clone(),
                            role: SourceRole::AuthoredLiteral(declaration(doc, root, &current)?),
                            cell: None,
                            value: Some(value.clone()),
                            masked,
                        });
                        None
                    }
                    Some(Source::Bound(binding)) => {
                        result.hops.push(SourceHop {
                            address: at.clone(),
                            role: SourceRole::AppBinding(*binding),
                            cell: None,
                            value: None,
                            masked,
                        });
                        None
                    }
                    Some(Source::Forward(from)) => {
                        let from_address = PropertyAddress::Occurrence {
                            instance: *id,
                            path: from.clone(),
                        };
                        if result.immediate_source.is_none() {
                            result.immediate_source = Some(from_address.clone());
                        }
                        result.hops.push(SourceHop {
                            address: at.clone(),
                            role: SourceRole::Forward { from: from_address },
                            cell: None,
                            value: None,
                            masked,
                        });
                        Some(from.clone())
                    }
                    None => {
                        if let Some(cell) = terminal_cell(doc, root.definition, &current)? {
                            let (def, node) =
                                path_node(doc, root.definition, &current.references, current.node)?;
                            let shared = PropertyAddress::Definition {
                                definition: def.id,
                                node: node.id,
                                property: current.property,
                            };
                            result.terminal_shared = Some(shared.clone());
                            let value = active_values.get(&cell).cloned().flatten();
                            result.hops.push(SourceHop {
                                address: shared,
                                role: SourceRole::SharedRegister,
                                cell: Some(cell),
                                value: value.clone(),
                                masked,
                            });
                            masked |= value.is_some();
                        }
                        defaults(
                            doc,
                            root.definition,
                            &current,
                            masked,
                            styles
                                .and_then(|styles| styles.color(&at))
                                .map(|style| (&at, style)),
                            &mut result.hops,
                        )?;
                        break;
                    }
                };
                // Forwarding, literals and bindings all mask the local definition.
                if let Some(cell) = terminal_cell(doc, root.definition, &current)? {
                    let (def, node) =
                        path_node(doc, root.definition, &current.references, current.node)?;
                    result.hops.push(SourceHop {
                        address: PropertyAddress::Definition {
                            definition: def.id,
                            node: node.id,
                            property: current.property,
                        },
                        role: SourceRole::SharedRegister,
                        cell: Some(cell),
                        value: active_values.get(&cell).cloned().flatten(),
                        masked: true,
                    });
                }
                defaults(
                    doc,
                    root.definition,
                    &current,
                    true,
                    styles
                        .and_then(|styles| styles.color(&at))
                        .map(|style| (&at, style)),
                    &mut result.hops,
                )?;
                match next {
                    Some(next) => current = next,
                    None => break,
                }
            }
            if let Some(value) = &result.value {
                validate_property_value(&descriptor, value)?;
            }
        }
    }
    Ok(result)
}
