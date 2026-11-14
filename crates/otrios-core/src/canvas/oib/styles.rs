// SPDX-License-Identifier: LGPL-3.0-only
//! RFC 0037 immutable color-style admission, signed imports and resolution.
use super::super::{Result, err};
use otrios_interfaces::oib::{designer::ClassId, styles::*};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod mapping;
pub use mapping::{Context, Mapped, Projection, PropertyPaint};
mod frame;
pub use frame::{Contribution, Frame, Occurrence};
mod local;
pub(crate) use frame::automatic;
pub(crate) use local::edit_local;
pub use local::{binding, catalog, color_inputs, local_admitted, resolved_payload};

/// Cheap presence query for mode invalidation and retained-paint eligibility.
/// Admission and resolution still validate the complete catalog separately.
pub(crate) fn has_styles(seed: &otrios_interfaces::oib::Seed) -> bool {
    seed.graph
        .as_ref()
        .is_some_and(|graph| graph.styles.is_some())
        || seed
            .resolved_libraries
            .iter()
            .any(|body| body.graph.styles.is_some())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableUse {
    pub style: StyleRef,
    pub field: FieldId,
    pub state: Option<State>,
}
/// Constructed only after complete validation. Its revision is a content digest,
/// not a signature, grant or compatible-library-update proof.
pub struct Admitted {
    catalog: Catalog,
    bytes: Vec<u8>,
    revision: [u8; 32],
    variables: BTreeMap<VariableRef, Variable>,
    styles: BTreeMap<StyleRef, (usize, usize)>,
    dependencies: BTreeMap<VariableRef, Vec<VariableUse>>,
}

fn name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
    {
        return Err("Invalid style identifier name".into());
    }
    Ok(())
}
fn ordered<T: Ord>(keys: impl IntoIterator<Item = T>) -> Result<()> {
    let mut previous = None;
    for key in keys {
        if previous.as_ref().is_some_and(|p| p >= &key) {
            return Err("Style records must be unique and canonically ordered".into());
        }
        previous = Some(key);
    }
    Ok(())
}
fn names<'a>(keys: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for key in keys {
        name(key)?;
        if !seen.insert(key) {
            return Err("Duplicate style record name".into());
        }
    }
    Ok(())
}
fn add(total: &mut usize, count: usize, limit: usize) -> Result<()> {
    *total = total.checked_add(count).ok_or("Style count overflow")?;
    if *total > limit {
        return Err("Style resource limit exceeded".into());
    }
    Ok(())
}
fn field<'a>(contract: &'a Contract, id: &FieldId) -> Result<&'a Field> {
    contract
        .fields
        .iter()
        .find(|f| f.id == *id)
        .ok_or_else(|| "Unknown style field".into())
}
fn check_domain(domain: Domain, scope: Scope) -> Result<()> {
    if matches!(
        (domain, scope),
        (Domain::Focus, Scope::State(State::Focus))
            | (Domain::Body, Scope::All)
            | (
                Domain::Body,
                Scope::State(State::Selected | State::Hover | State::Pressed | State::Disabled)
            )
    ) {
        Ok(())
    } else {
        Err("Style body and focus fields cannot cross domains".into())
    }
}

impl Admitted {
    /// `owners` must come from the caller's document/signed closure admission.
    /// Supplying an owner here does not verify its signature or authorize edits.
    pub fn new(catalog: Catalog, owners: &BTreeSet<Owner>) -> Result<Self> {
        if catalog.format != FORMAT || catalog.systems.len() > MAX_SYSTEMS {
            return Err("Unsupported or oversized color-style catalog".into());
        }
        ordered(catalog.systems.iter().map(|s| &s.reference))?;
        let mut variable_count = 0;
        let mut style_count = 0;
        let mut contract_count = 0;
        let mut entries = 0;
        let mut references = 0;
        let mut variables = BTreeMap::new();
        let mut styles = BTreeMap::new();
        for (system_index, system) in catalog.systems.iter().enumerate() {
            name(&system.name)?;
            if system.reference.system.0 == 0
                || !owners.contains(&system.reference.owner)
                || matches!(system.reference.owner, Owner::Document(id) if id.0 == 0)
            {
                return Err("Unadmitted style system owner or identity".into());
            }
            add(&mut variable_count, system.variables.len(), MAX_VARIABLES)?;
            add(&mut style_count, system.styles.len(), MAX_STYLES)?;
            add(&mut contract_count, system.contracts.len(), MAX_CONTRACTS)?;
            ordered(system.variables.iter().map(|v| v.id))?;
            ordered(system.contracts.iter().map(|v| v.id))?;
            ordered(system.styles.iter().map(|v| v.id))?;
            names(system.variables.iter().map(|v| v.name.as_str()))?;
            names(system.contracts.iter().map(|v| v.name.as_str()))?;
            names(system.styles.iter().map(|v| v.name.as_str()))?;
            for variable in &system.variables {
                if variable.id.0 == 0 {
                    return Err("Zero style variable identity".into());
                }
                variables.insert(
                    VariableRef {
                        system: system.reference.clone(),
                        variable: variable.id,
                    },
                    variable.clone(),
                );
            }
            for contract in &system.contracts {
                if contract.id.0 == 0
                    || contract.fields.is_empty()
                    || contract.fields.len() > MAX_FIELDS
                    || contract.classes.is_empty()
                    || contract.classes.len() > super::designer::MAX_CLASS_DESCRIPTORS
                {
                    return Err("Invalid style contract bounds".into());
                }
                ordered(contract.classes.iter())?;
                ordered(contract.fields.iter().map(|f| f.id))?;
                names(contract.fields.iter().map(|f| f.name.as_str()))?;
                if contract.classes.iter().any(|c| c.0 == 0)
                    || contract.fields.iter().any(|f| f.id.0 == 0)
                {
                    return Err("Zero style contract identity".into());
                }
            }
            for (style_index, style) in system.styles.iter().enumerate() {
                if style.id.0 == 0 || style.patches.len() > 5 {
                    return Err("Invalid style identity or patches".into());
                }
                let contract = system
                    .contracts
                    .iter()
                    .find(|c| c.id == style.contract)
                    .ok_or("Unknown style contract")?;
                ordered(style.defaults.iter().map(|f| f.field))?;
                if style.defaults.len() != contract.fields.len()
                    || style
                        .defaults
                        .iter()
                        .zip(&contract.fields)
                        .any(|(v, f)| v.field != f.id)
                {
                    return Err(
                        "Style defaults must supply every contract field exactly once".into(),
                    );
                }
                add(&mut entries, style.defaults.len(), MAX_ENTRIES)?;
                ordered(style.patches.iter().map(|p| p.state))?;
                for patch in &style.patches {
                    if patch.fields.is_empty() || patch.fields.len() > MAX_FIELDS {
                        return Err("Invalid style patch bounds".into());
                    }
                    ordered(patch.fields.iter().map(|f| f.field))?;
                    add(&mut entries, patch.fields.len(), MAX_ENTRIES)?;
                    for entry in &patch.fields {
                        check_domain(
                            field(contract, &entry.field)?.domain,
                            Scope::State(patch.state),
                        )?;
                    }
                }
                styles.insert(
                    StyleRef {
                        system: system.reference.clone(),
                        style: style.id,
                    },
                    (system_index, style_index),
                );
            }
        }
        let mut dependencies: BTreeMap<VariableRef, Vec<VariableUse>> = BTreeMap::new();
        for system in &catalog.systems {
            for style in &system.styles {
                let reference = StyleRef {
                    system: system.reference.clone(),
                    style: style.id,
                };
                for (state, entry) in style.defaults.iter().map(|e| (None, e)).chain(
                    style
                        .patches
                        .iter()
                        .flat_map(|p| p.fields.iter().map(move |e| (Some(p.state), e))),
                ) {
                    if let FieldValue::Variable(variable) = &entry.value {
                        if !variables.contains_key(variable) {
                            return Err("Unknown exact style variable reference".into());
                        }
                        add(&mut references, 1, MAX_ENTRIES)?;
                        dependencies
                            .entry(variable.clone())
                            .or_default()
                            .push(VariableUse {
                                style: reference.clone(),
                                field: entry.field,
                                state,
                            });
                    }
                }
            }
        }
        let mut bytes = HEADER.to_vec();
        bytes.extend(flexbuffers::to_vec(&catalog).map_err(err)?);
        if bytes.len() > MAX_BYTES {
            return Err("Color-style byte budget exceeded".into());
        }
        let revision = Sha256::digest(&bytes).into();
        Ok(Self {
            catalog,
            bytes,
            revision,
            variables,
            styles,
            dependencies,
        })
    }
    pub fn decode(bytes: &[u8], owners: &BTreeSet<Owner>) -> Result<Self> {
        if bytes.len() > MAX_BYTES || !bytes.starts_with(HEADER) {
            return Err("Unsupported color-style encoding".into());
        }
        let catalog = flexbuffers::from_slice(&bytes[HEADER.len()..]).map_err(err)?;
        let result = Self::new(catalog, owners)?;
        if result.bytes != bytes {
            return Err("Noncanonical color-style encoding".into());
        }
        Ok(result)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn revision(&self) -> [u8; 32] {
        self.revision
    }
    pub fn dependencies(&self) -> &BTreeMap<VariableRef, Vec<VariableUse>> {
        &self.dependencies
    }
    /// Existing uses of retired records continue resolving. Picker exclusion is
    /// explicit so retirement never silently removes a currently applied style.
    pub fn selectable(&self, reference: &StyleRef) -> bool {
        self.style(reference)
            .is_ok_and(|(_, style)| style.availability == Availability::Active)
    }
    pub fn variable_selectable(&self, reference: &VariableRef) -> bool {
        self.variables
            .get(reference)
            .is_some_and(|variable| variable.availability == Availability::Active)
    }
    fn style(&self, reference: &StyleRef) -> Result<(&Contract, &Style)> {
        let (system, style) = *self
            .styles
            .get(reference)
            .ok_or("Unknown exact style reference")?;
        let system = &self.catalog.systems[system];
        let style = &system.styles[style];
        let contract = system
            .contracts
            .iter()
            .find(|c| c.id == style.contract)
            .ok_or("Unknown style contract")?;
        Ok((contract, style))
    }
    fn value(&self, value: &FieldValue, mode: Mode) -> Result<[u8; 4]> {
        match value {
            FieldValue::Literal(color) => Ok(*color),
            FieldValue::Variable(reference) => {
                let variable = self
                    .variables
                    .get(reference)
                    .ok_or("Unknown exact style variable reference")?;
                Ok(match mode {
                    Mode::Light => variable.values.light,
                    Mode::Dark => variable.values.dark,
                })
            }
        }
    }
    pub fn resolve(
        &self,
        reference: &StyleRef,
        class: ClassId,
        mode: Mode,
        mask: StateMask,
        document: &[Override],
        instance: &[Override],
    ) -> Result<Resolved> {
        let (contract, style) = self.style(reference)?;
        if !contract.classes.contains(&class) {
            return Err("Style does not apply to this class".into());
        }
        let mut count = 0;
        for layer in [document, instance] {
            add(&mut count, layer.len(), MAX_OVERRIDES)?;
            ordered(layer.iter().map(|o| (o.field, o.scope)))?;
            for entry in layer {
                check_domain(field(contract, &entry.field)?.domain, entry.scope)?;
                // Validate dormant overrides too, in both modes, before output.
                self.value(&entry.value, Mode::Light)?;
                self.value(&entry.value, Mode::Dark)?;
            }
        }
        let mut chosen: BTreeMap<_, _> = style
            .defaults
            .iter()
            .map(|entry| (entry.field, (entry.value.clone(), Layer::Style, Scope::All)))
            .collect();
        for patch in &style.patches {
            if active(patch.state, mask) {
                for entry in &patch.fields {
                    chosen.insert(
                        entry.field,
                        (entry.value.clone(), Layer::Style, Scope::State(patch.state)),
                    );
                }
            }
        }
        for (layer, entries) in [(Layer::Document, document), (Layer::Instance, instance)] {
            for entry in entries {
                if matches!(entry.scope, Scope::All)
                    || matches!(entry.scope, Scope::State(state) if active(state, mask))
                {
                    chosen.insert(entry.field, (entry.value.clone(), layer, entry.scope));
                }
            }
        }
        let mut resolved = Resolved {
            body: BTreeMap::new(),
            focus: BTreeMap::new(),
        };
        for descriptor in &contract.fields {
            if descriptor.domain == Domain::Focus && !mask.focus_visible {
                continue;
            }
            let (expression, layer, scope) = chosen
                .remove(&descriptor.id)
                .ok_or("Missing admitted style field")?;
            let color = self.value(&expression, mode)?;
            let entry = ResolvedField {
                color,
                expression,
                layer,
                scope,
                mode,
            };
            match descriptor.domain {
                Domain::Body => &mut resolved.body,
                Domain::Focus => &mut resolved.focus,
            }
            .insert(descriptor.id, entry);
        }
        Ok(resolved)
    }
}
fn active(state: State, mask: StateMask) -> bool {
    match state {
        State::Selected => mask.selected,
        State::Hover => !mask.disabled && mask.hovered,
        State::Pressed => !mask.disabled && mask.pressed,
        State::Disabled => mask.disabled,
        State::Focus => mask.focus_visible,
    }
}
pub fn root_mode(selection: RootMode, host: Option<Mode>) -> Mode {
    match selection {
        RootMode::System => host.unwrap_or(Mode::Dark),
        RootMode::Light => Mode::Light,
        RootMode::Dark => Mode::Dark,
    }
}
pub fn child_mode(selection: ModeSelection, parent: Mode) -> Mode {
    match selection {
        ModeSelection::Inherit => parent,
        ModeSelection::Light => Mode::Light,
        ModeSelection::Dark => Mode::Dark,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Style,
    Document,
    Instance,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedField {
    pub color: [u8; 4],
    pub expression: FieldValue,
    pub layer: Layer,
    pub scope: Scope,
    pub mode: Mode,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub body: BTreeMap<FieldId, ResolvedField>,
    pub focus: BTreeMap<FieldId, ResolvedField>,
}

#[cfg(test)]
#[path = "styles_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "styles_mapping_tests.rs"]
mod mapping_tests;
