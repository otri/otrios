// SPDX-License-Identifier: LGPL-3.0-only
//! Document-local style payload and pure typed source edits.
use super::*;
use crate::canvas::oib::{Seed, document as properties};
use otrios_interfaces::{
    layout::{Value, ValueType},
    oib::{
        authoring::DocumentSource,
        designer::{
            DefinitionId, NodeId, PropertyDescriptor, PropertyEditor, PropertyId, PropertyPath,
        },
    },
};

pub fn color_inputs(seed: &Seed, definition: DefinitionId) -> Result<Vec<PropertyDescriptor>> {
    let doc = properties::metadata(seed)?;
    let def = doc
        .definitions
        .iter()
        .find(|d| d.id == definition)
        .ok_or("Unknown styled component")?;
    let root = def
        .nodes
        .iter()
        .find(|n| n.id == def.root)
        .ok_or("Missing styled root")?;
    Ok(properties::node_descriptors(doc, def, root)?
        .into_iter()
        .filter(|property| {
            property.value_type == ValueType::Color
                && property.policy.live_literal
                && property.editor != PropertyEditor::ReadOnly
        })
        .collect())
}
pub fn local_admitted(seed: &Seed) -> Result<Option<Admitted>> {
    let graph = seed
        .graph
        .as_ref()
        .ok_or("Missing canonical style source")?;
    let Some(payload) = &graph.styles else {
        return Ok(None);
    };
    let owner = Owner::Document(graph.id);
    if payload.catalog.systems.len() != 1
        || payload.catalog.systems[0].reference
            != (SystemRef {
                owner: owner.clone(),
                system: SystemId(1),
            })
    {
        return Err("Only the local document style system is admitted".into());
    }
    ordered(
        payload
            .components
            .iter()
            .map(|component| component.definition),
    )?;
    let admitted = Admitted::new(payload.catalog.clone(), &BTreeSet::from([owner]))?;
    let imported = crate::canvas::oib::imports::imported_definitions(seed);
    for component in &payload.components {
        if imported.contains(&component.definition) {
            return Err("Imported style bindings are read-only".into());
        }
        Mapped::new(&admitted, seed, component.clone())?;
    }
    Ok(Some(admitted))
}
fn payload(
    source: &mut DocumentSource,
    owner: otrios_interfaces::oib::designer::DocumentId,
) -> Result<&mut DocumentStyles> {
    let payload = source.styles.get_or_insert_with(|| DocumentStyles {
        catalog: Catalog {
            format: FORMAT.into(),
            systems: vec![System {
                reference: SystemRef {
                    owner: Owner::Document(owner),
                    system: SystemId(1),
                },
                name: "document".into(),
                variables: vec![],
                contracts: vec![],
                styles: vec![],
            }],
        },
        components: vec![],
    });
    if payload.catalog.systems.len() != 1 {
        return Err("Only one local style system is admitted".into());
    }
    Ok(payload)
}
fn mapped(
    definition: DefinitionId,
    root: NodeId,
    style: StyleRef,
    contract: &Contract,
) -> ComponentStyle {
    ComponentStyle {
        definition,
        style,
        fields: contract
            .fields
            .iter()
            .filter(|field| field.domain == Domain::Body)
            .map(|field| PropertyMapping {
                field: field.id,
                targets: vec![PropertyPath {
                    references: vec![],
                    node: root,
                    property: PropertyId(field.id.0),
                }],
            })
            .collect(),
    }
}
pub(crate) fn edit_local(seed: &Seed, source: &mut DocumentSource, edit: &StyleEdit) -> Result<()> {
    let graph = seed.graph.as_ref().ok_or("Missing style graph")?;
    match edit {
        StyleEdit::CreateStyle {
            id,
            name: label,
            definition,
        } => {
            name(label)?;
            let def = source
                .definitions
                .iter()
                .find(|d| d.id == *definition)
                .ok_or("Unknown local style component")?;
            let root = def
                .nodes
                .iter()
                .find(|n| n.id == def.root)
                .ok_or("Missing local style root")?;
            let mut descriptors = properties::class_descriptors(&graph.classes, root.class)?;
            descriptors.extend(def.properties.clone());
            descriptors.retain(|property| {
                property.value_type == ValueType::Color
                    && property.policy.live_literal
                    && property.editor != PropertyEditor::ReadOnly
            });
            descriptors.sort_by_key(|property| property.id);
            if descriptors.is_empty() || descriptors.len() > MAX_FIELDS {
                return Err("Component has no admitted style color inputs".into());
            }
            let fields = descriptors
                .iter()
                .map(|property| {
                    let candidate = if property.name.eq_ignore_ascii_case("accent") {
                        "fill".into()
                    } else {
                        property.name.to_ascii_lowercase().replace(' ', "_")
                    };
                    let label = if candidate.is_empty()
                        || candidate.len() > 64
                        || !candidate
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
                    {
                        format!("color_{}", property.id.0)
                    } else {
                        candidate
                    };
                    Field {
                        id: FieldId(property.id.0),
                        name: label,
                        domain: Domain::Body,
                    }
                })
                .collect();
            let defaults = descriptors
                .iter()
                .map(|property| -> Result<Entry> {
                    let value = root
                        .defaults
                        .iter()
                        .find(|default| default.property == property.id)
                        .map(|default| default.value.clone())
                        .unwrap_or_else(|| property.default.clone());
                    let Value::Color(color) = value else {
                        return Err("Invalid Color source default".into());
                    };
                    Ok(Entry {
                        field: FieldId(property.id.0),
                        value: FieldValue::Literal(color),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let class = root.class;
            let system = &mut payload(source, graph.id)?.catalog.systems[0];
            if system.styles.iter().any(|style| style.id == StyleId(*id)) {
                return Err("Style identity already exists".into());
            }
            system.contracts.push(Contract {
                id: ContractId(*id),
                name: format!("contract_{id}"),
                classes: vec![class],
                fields,
            });
            system.styles.push(Style {
                id: StyleId(*id),
                name: label.clone(),
                availability: Availability::Active,
                contract: ContractId(*id),
                defaults,
                patches: vec![],
            });
            system.contracts.sort_by_key(|contract| contract.id);
            system.styles.sort_by_key(|style| style.id);
        }
        StyleEdit::CreateVariable {
            id,
            name: label,
            values,
        } => {
            name(label)?;
            let system = &mut payload(source, graph.id)?.catalog.systems[0];
            if system
                .variables
                .iter()
                .any(|variable| variable.id == VariableId(*id))
            {
                return Err("Variable identity already exists".into());
            }
            system.variables.push(Variable {
                id: VariableId(*id),
                name: label.clone(),
                availability: Availability::Active,
                values: values.clone(),
            });
            system.variables.sort_by_key(|variable| variable.id);
        }
        StyleEdit::Assign { definition, style } => {
            if source.styles.is_none() && style.is_none() {
                return Ok(());
            }
            let def = source
                .definitions
                .iter()
                .find(|d| d.id == *definition)
                .ok_or("Unknown local styled component")?;
            let root = def.root;
            let data = payload(source, graph.id)?;
            data.components
                .retain(|component| component.definition != *definition);
            if let Some(style) = style {
                let system = &data.catalog.systems[0];
                let selected = system
                    .styles
                    .iter()
                    .find(|record| {
                        record.id == *style && record.availability == Availability::Active
                    })
                    .ok_or("Style is unavailable")?;
                let contract = system
                    .contracts
                    .iter()
                    .find(|contract| contract.id == selected.contract)
                    .ok_or("Missing style contract")?;
                data.components.push(mapped(
                    *definition,
                    root,
                    StyleRef {
                        system: system.reference.clone(),
                        style: *style,
                    },
                    contract,
                ));
                data.components
                    .sort_by_key(|component| component.definition);
            }
        }
        StyleEdit::Replace { styles } => source.styles = styles.clone(),
        edit => {
            let data = source.styles.as_mut().ok_or("No local design system")?;
            let system = data
                .catalog
                .systems
                .first_mut()
                .ok_or("Missing local style system")?;
            match edit {
                StyleEdit::RenameStyle { style, name: label } => {
                    name(label)?;
                    system
                        .styles
                        .iter_mut()
                        .find(|record| record.id == *style)
                        .ok_or("Style was removed")?
                        .name = label.clone();
                }
                StyleEdit::RenameVariable {
                    variable,
                    name: label,
                } => {
                    name(label)?;
                    system
                        .variables
                        .iter_mut()
                        .find(|record| record.id == *variable)
                        .ok_or("Variable was removed")?
                        .name = label.clone();
                }
                StyleEdit::SetVariable {
                    variable,
                    mode,
                    color,
                } => {
                    let variable = system
                        .variables
                        .iter_mut()
                        .find(|record| record.id == *variable)
                        .ok_or("Variable was removed")?;
                    match mode {
                        Mode::Light => variable.values.light = *color,
                        Mode::Dark => variable.values.dark = *color,
                    }
                }
                StyleEdit::SetField {
                    style,
                    field,
                    value,
                } => {
                    let record = system
                        .styles
                        .iter_mut()
                        .find(|record| record.id == *style)
                        .ok_or("Style was removed")?;
                    record
                        .defaults
                        .iter_mut()
                        .find(|entry| entry.field == *field)
                        .ok_or("Style field was removed")?
                        .value = value.clone();
                }
                StyleEdit::DeleteStyle { style } => {
                    if data
                        .components
                        .iter()
                        .any(|component| component.style.style == *style)
                    {
                        return Err(
                            "Style is in use; remove or replace its component assignments first"
                                .into(),
                        );
                    }
                    let index = system
                        .styles
                        .iter()
                        .position(|record| record.id == *style)
                        .ok_or("Style was removed")?;
                    let removed = system.styles.remove(index);
                    if !system
                        .styles
                        .iter()
                        .any(|record| record.contract == removed.contract)
                    {
                        system
                            .contracts
                            .retain(|contract| contract.id != removed.contract);
                    }
                }
                StyleEdit::DeleteVariable { variable } => {
                    let reference = VariableRef {
                        system: system.reference.clone(),
                        variable: *variable,
                    };
                    if system.styles.iter().any(|style| {
                        style
                            .defaults
                            .iter()
                            .chain(style.patches.iter().flat_map(|patch| &patch.fields))
                            .any(|entry| entry.value == FieldValue::Variable(reference.clone()))
                    }) {
                        return Err("Variable is in use; replace its style references first".into());
                    }
                    let index = system
                        .variables
                        .iter()
                        .position(|record| record.id == *variable)
                        .ok_or("Variable was removed")?;
                    system.variables.remove(index);
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
