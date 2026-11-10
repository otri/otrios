// SPDX-License-Identifier: LGPL-3.0-only
//! Occurrence-local execution of admitted signed component behaviors.
//!
//! Semantic attachment and declared outlets constrain the guest to the existing
//! signed app effect surface. Guest code never names app fields or scene nodes.
use super::{document, seed};
use crate::{
    canvas::{
        Result, StateAdapter,
        component_behavior::{Guest, Runtime},
    },
    dashboard::layout::Layout,
};
use otrios_interfaces::{
    canvas::{GestureKind, Handler, Operation, Projection, Selector, Ui},
    layout::{Binding, Responder, TemplateNode, Value, ValueType},
    oib::{
        Seed,
        component_behavior::{self as abi, Command, Event, Field, Input, Phase},
        designer::{self, ObjectAddress, PropertyDescriptor, PropertyPath},
        library::{BehaviorDeclaration, LibraryId},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
struct Read {
    id: u64,
    path: PropertyPath,
    descriptor: PropertyDescriptor,
    selector: Option<Selector>,
}
#[derive(Clone, Debug, PartialEq)]
struct Outlet {
    id: u64,
    read: Read,
}
#[derive(Clone, Debug, PartialEq)]
struct Spec {
    address: ObjectAddress,
    library: LibraryId,
    revision: [u8; 32],
    code_hash: [u8; 32],
    declaration: BehaviorDeclaration,
    handler: Handler,
    properties: Vec<Read>,
    outlets: Vec<Outlet>,
}
struct Instance {
    spec: Spec,
    guest: Guest,
    local: BTreeMap<u64, Value>,
}
#[derive(Default)]
pub(crate) struct Instances {
    controls: BTreeMap<u8, Instance>,
}
pub(crate) struct Outcome {
    pub changed: bool,
    /// Already validated against declaration, phase and parent-owned capture.
    pub capture: Option<bool>,
}
fn value_type(value: &Value) -> ValueType {
    match value {
        Value::Number(_) => ValueType::Number,
        Value::Boolean(_) => ValueType::Boolean,
        Value::Text(_) => ValueType::Text,
        Value::Color(_) => ValueType::Color,
        Value::Series(_) => ValueType::Series,
        Value::TimedSeries(_) => ValueType::TimedSeries,
    }
}
fn full_path(address: &ObjectAddress, relative: &PropertyPath) -> PropertyPath {
    let mut references = address.references.clone();
    references.extend(&relative.references);
    PropertyPath {
        references,
        node: relative.node,
        property: relative.property,
    }
}
fn terminal_definition<'a>(
    metadata: &'a designer::Document,
    address: &ObjectAddress,
) -> Result<&'a designer::ComponentDefinition> {
    let mut id = metadata
        .instances
        .iter()
        .find(|i| i.id == address.instance)
        .ok_or("Missing component behavior instance")?
        .definition;
    for reference in &address.references {
        let definition = metadata
            .definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or("Missing component behavior definition")?;
        id = definition
            .nodes
            .iter()
            .find(|n| n.id == *reference)
            .and_then(|n| n.reference.as_ref())
            .ok_or("Missing component behavior reference boundary")?
            .definition;
    }
    metadata
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "Missing component behavior terminal definition".into())
}
fn layout_node(node: &TemplateNode, id: u16) -> Option<&TemplateNode> {
    if node.id == id {
        Some(node)
    } else {
        node.children.iter().find_map(|n| layout_node(n, id))
    }
}
fn read_spec(
    ui: &Ui,
    seed: &Seed,
    address: &ObjectAddress,
    id: u64,
    relative: &PropertyPath,
) -> Result<Read> {
    let path = full_path(address, relative);
    let descriptor = document::graph_descriptor(seed, address.instance, &path)?;
    let (_, _, binding) =
        document::graph_property(seed, address.instance, &path, &BTreeMap::new())?;
    let selector = binding
        .map(|Binding::Local(local)| {
            ui.selectors
                .iter()
                .find(|s| s.local == local)
                .cloned()
                .ok_or_else(|| "Missing component behavior property selector".to_string())
        })
        .transpose()?;
    if selector.as_ref().is_some_and(|s| {
        !matches!(
            s.projection,
            Projection::Identity | Projection::Nonzero | Projection::Number { .. }
        )
    }) {
        return Err("Unsupported component behavior property projection".into());
    }
    Ok(Read {
        id,
        path,
        descriptor,
        selector,
    })
}
fn specs(ui: &Ui, layout: &Layout) -> Result<Vec<Spec>> {
    let Some(seed) = seed(ui) else {
        return Ok(vec![]);
    };
    if seed
        .resolved_libraries
        .iter()
        .all(|library| library.graph.behaviors.is_empty())
    {
        return Ok(vec![]);
    }
    let metadata = seed
        .designer
        .as_ref()
        .ok_or("Missing component behavior metadata")?;
    let graph = seed
        .graph
        .as_ref()
        .ok_or("Missing component behavior graph")?;
    let imports = graph
        .imports
        .as_ref()
        .ok_or("Component behaviors require exact library imports")?;
    let mut live: BTreeSet<_> = if seed.expanded {
        seed.instances.iter().map(|i| i.id).collect()
    } else {
        graph.instances.iter().map(|i| i.id.0).collect()
    };
    // Fixed document roots are separate from allocation-owned palette instances.
    live.extend(
        graph
            .instances
            .iter()
            .filter(|i| {
                graph
                    .presets
                    .iter()
                    .find(|p| p.id == i.preset)
                    .is_some_and(|p| !p.palette)
            })
            .map(|i| i.id.0),
    );
    let mut result = Vec::new();
    let mut controls = BTreeSet::new();
    let mut addresses = BTreeSet::new();
    for mapping in &metadata.mappings.objects {
        if !live.contains(&mapping.address.instance.0) {
            continue;
        }
        let definition = terminal_definition(metadata, &mapping.address)?;
        let mut attached: Option<(
            &otrios_interfaces::oib::graph::ResolvedLibrary,
            &BehaviorDeclaration,
        )> = None;
        for library in &seed.resolved_libraries {
            for binding in imports
                .bindings
                .iter()
                .filter(|binding| binding.library == library.library)
            {
                for mapped in binding
                    .definitions
                    .iter()
                    .filter(|mapped| mapped.target == definition.id)
                {
                    for declaration in
                        library.graph.behaviors.iter().filter(|d| {
                            d.definition == mapped.source && d.node == mapping.address.node
                        })
                    {
                        if let Some((previous_library, previous_declaration)) = attached {
                            // Several exported closures can contain the same
                            // exact mapped definition. This is one attachment.
                            if !std::ptr::eq(previous_library, library)
                                || !std::ptr::eq(previous_declaration, declaration)
                            {
                                return Err("Ambiguous component behavior attachment".into());
                            }
                        } else {
                            attached = Some((library, declaration));
                        }
                    }
                }
            }
        }
        let Some((library, declaration)) = attached else {
            continue;
        };
        if !addresses.insert(mapping.address.clone()) {
            return Err("Duplicate component behavior occurrence".into());
        }
        let code = library
            .component
            .as_ref()
            .ok_or("Missing component behavior code")?;
        if code.abi != abi::ABI {
            return Err("Unsupported component behavior ABI".into());
        }
        let id = mapping
            .layout_node
            .ok_or("Component behavior requires an admitted input node")?;
        let nodes: Vec<_> = layout
            .blueprint()
            .templates
            .iter()
            .filter_map(|t| layout_node(&t.root, id))
            .collect();
        let [node] = nodes.as_slice() else {
            return Err("Missing or ambiguous component behavior layout node".into());
        };
        let action = definition
            .nodes
            .iter()
            .find(|n| n.id == mapping.address.node)
            .and_then(|n| n.visual.as_ref())
            .and_then(|v| v.action.as_ref())
            .ok_or("Missing component behavior admitted action")?;
        if node.responder.is_none() {
            let path = full_path(&mapping.address, &action.source);
            if document::graph_property(seed, mapping.address.instance, &path, &BTreeMap::new())?
                .2
                .is_none()
            {
                // D2 allows an authored literal instead of an app connection.
                // The compiler deliberately strips input and semantics from
                // this display-only occurrence; it needs no guest lease.
                continue;
            }
        }
        let Some(Responder::AppEvent { control, drag }) = node.responder else {
            return Err("Component behavior node has no admitted app responder".into());
        };
        let handler = ui
            .handlers
            .iter()
            .find(|h| h.control == control)
            .ok_or("Missing component behavior handler")?
            .clone();
        if !controls.insert(control)
            || handler.fields.len() != 1
            || !matches!(
                (&handler.operation, handler.kind),
                (Operation::Set, GestureKind::Slider) | (Operation::Toggle, GestureKind::Activate)
            )
            || drag != (handler.kind == GestureKind::Slider)
        {
            return Err("Unsupported or duplicated component behavior effect surface".into());
        }

        let action_operation = match handler.operation {
            Operation::Set => otrios_interfaces::oib::graph::ActionOperation::Set,
            Operation::Toggle => otrios_interfaces::oib::graph::ActionOperation::Toggle,
            _ => unreachable!(),
        };
        if action.kind != handler.kind
            || action.operation != action_operation
            || !graph.grants.iter().any(|g| {
                g.field == handler.fields[0]
                    && g.kind == handler.kind
                    && g.operation == action_operation
            })
        {
            return Err("Component behavior exceeds signed app action grant".into());
        }
        let properties = declaration
            .properties
            .iter()
            .map(|p| read_spec(ui, seed, &mapping.address, p.id, &p.source))
            .collect::<Result<Vec<_>>>()?;
        let mut outlets = Vec::new();
        for outlet in &declaration.outlets {
            let read = read_spec(ui, seed, &mapping.address, outlet.id, &outlet.source)?;
            let selector = read
                .selector
                .as_ref()
                .ok_or("Component behavior outlet requires an admitted app binding")?;
            if !read.descriptor.policy.app_binding
                || read.descriptor.value_type != outlet.value_type
                || selector.field != handler.fields[0]
                || !matches!(
                    (&handler.operation, &selector.projection, outlet.value_type),
                    (Operation::Set, Projection::Identity, ValueType::Number)
                        | (Operation::Toggle, Projection::Nonzero, ValueType::Boolean)
                )
            {
                return Err(
                    "Component behavior outlet exceeds its admitted typed field/effect".into(),
                );
            }
            outlets.push(Outlet {
                id: outlet.id,
                read,
            });
        }
        if result.len() >= crate::canvas::component_behavior::MAX_INSTANCES {
            return Err("Component behavior static instance budget".into());
        }
        result.push(Spec {
            address: mapping.address.clone(),
            library: library.library.clone(),
            revision: library.revision,
            code_hash: code.sha256,
            declaration: declaration.clone(),
            handler,
            properties,
            outlets,
        });
    }
    result.sort_by_key(|s| s.handler.control);
    Ok(result)
}
fn default_field(ui: &Ui, id: u16) -> Result<Value> {
    ui.state_schema
        .fields
        .iter()
        .find(|field| field.id == id)
        .map(|field| field.default.clone())
        .ok_or_else(|| "Missing component context app field".into())
}
fn context(spec: &Spec, event: Event, properties: Vec<Field>, local: Vec<Field>) -> Input {
    Input {
        behavior: spec.declaration.id,
        address: spec.address.clone(),
        event,
        properties,
        local,
    }
}
fn validation_event() -> Event {
    Event {
        phase: Phase::Cancel,
        pointer: 0,
        a: 0,
        b: 0,
    }
}
fn resolve_value(
    ui: &Ui,
    seed: &Seed,
    address: &ObjectAddress,
    read: &Read,
    values: &BTreeMap<u64, Option<Value>>,
    read_field: &impl Fn(u16) -> Result<Value>,
) -> Result<Value> {
    let (_, literal, binding) =
        document::graph_property(seed, address.instance, &read.path, values)?;
    let value = if let Some(Binding::Local(local)) = binding {
        let selector = ui
            .selectors
            .iter()
            .find(|s| s.local == local)
            .ok_or("Missing live component behavior selector")?;
        if Some(selector) != read.selector.as_ref() {
            return Err("Component behavior source changed during dispatch".into());
        }
        match (&selector.projection, read_field(selector.field)?) {
            (Projection::Identity, value) => value,
            (Projection::Nonzero, Value::Number(n)) => Value::Boolean(n != 0.),
            (Projection::Number { scale, offset }, Value::Number(n)) => {
                Value::Number(n * scale + offset)
            }
            _ => return Err("Unsupported live component behavior source projection/type".into()),
        }
    } else {
        if read.selector.is_some() {
            return Err("Component behavior source was disconnected during dispatch".into());
        }
        literal.ok_or("Missing component behavior property value")?
    };
    document::validate_property_value(&read.descriptor, &value)?;
    Ok(value)
}
fn validate_default_contexts(
    ui: &Ui,
    specs: &[Spec],
    values: &BTreeMap<u64, Option<Value>>,
    read_field: &impl Fn(u16) -> Result<Value>,
) -> Result<()> {
    if specs.is_empty() {
        return Ok(());
    }
    let seed = seed(ui).ok_or("Missing component behavior source")?;
    for spec in specs {
        let properties = spec
            .properties
            .iter()
            .map(|read| {
                Ok(Field {
                    id: read.id,
                    value: resolve_value(ui, seed, &spec.address, read, values, read_field)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let local = spec
            .declaration
            .local
            .iter()
            .map(|property| Field {
                id: property.id,
                value: property.default.clone(),
            })
            .collect();
        abi::encode_input(&context(spec, validation_event(), properties, local))?;
    }
    Ok(())
}
/// Pure candidate validation before publication. No guest/store is allocated;
/// the Client separately accounts for active plus staged execution leases.
pub(crate) fn validate(ui: &Ui, layout: &Layout) -> Result<()> {
    validate_context(ui, layout, &BTreeMap::new(), |id| default_field(ui, id))
}
/// Validate the complete current input envelope with declared local defaults.
/// The authority passes its candidate causal values and app-field reader before
/// publication, independently of any Client guest allocation.
pub(crate) fn validate_context(
    ui: &Ui,
    layout: &Layout,
    oib_values: &BTreeMap<u64, Option<Value>>,
    read_field: impl Fn(u16) -> Result<Value>,
) -> Result<()> {
    validate_default_contexts(ui, &specs(ui, layout)?, oib_values, &read_field)
}
fn read_value(
    ui: &Ui,
    seed: &Seed,
    doc: &StateAdapter,
    address: &ObjectAddress,
    read: &Read,
    values: &mut Option<BTreeMap<u64, Option<Value>>>,
) -> Result<Value> {
    // Ordinary bound-only slider samples read just their app fields. Literal
    // contexts share one lazily fetched causal projection per complete input.
    if read.selector.is_none() && values.is_none() {
        *values = Some(doc.oib_active()?);
    }
    resolve_value(
        ui,
        seed,
        address,
        read,
        values.as_ref().unwrap_or(&BTreeMap::new()),
        &|id| doc.typed_value(id),
    )
}

fn resumed_locals(spec: &Spec, previous: Option<&Instances>) -> BTreeMap<u64, Value> {
    let old = previous.and_then(|previous| {
        previous.controls.values().find(|old| {
            old.spec.address == spec.address
                && old.spec.library == spec.library
                && old.spec.declaration.id == spec.declaration.id
        })
    });
    spec.declaration
        .local
        .iter()
        .map(|property| {
            let value = old
                .and_then(|old| old.local.get(&property.id))
                .filter(|value| value_type(value) == value_type(&property.default))
                .cloned()
                .unwrap_or_else(|| property.default.clone());
            (property.id, value)
        })
        .collect()
}
fn validate_local_context(
    spec: &Spec,
    local: &BTreeMap<u64, Value>,
    ui: &Ui,
    seed: &Seed,
    doc: &StateAdapter,
    values: &mut Option<BTreeMap<u64, Option<Value>>>,
) -> Result<()> {
    if ui
        .handlers
        .iter()
        .find(|handler| handler.control == spec.handler.control)
        != Some(&spec.handler)
    {
        return Err("Component behavior handler changed during context validation".into());
    }
    let properties = spec
        .properties
        .iter()
        .map(|read| {
            Ok(Field {
                id: read.id,
                value: read_value(ui, seed, doc, &spec.address, read, values)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let local = local
        .iter()
        .map(|(id, value)| Field {
            id: *id,
            value: value.clone(),
        })
        .collect();
    abi::encode_input(&context(spec, validation_event(), properties, local))?;
    Ok(())
}

impl Instances {
    pub fn len(&self) -> usize {
        self.controls.len()
    }
    pub fn code_hashes(&self) -> Vec<[u8; 32]> {
        self.controls
            .values()
            .map(|instance| instance.guest.code_hash())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn contains(&self, control: u8) -> bool {
        self.controls.contains_key(&control)
    }
    pub fn matches(&self, ui: &Ui, layout: &Layout) -> Result<bool> {
        let candidate = specs(ui, layout)?;
        Ok(candidate.len() == self.controls.len()
            && candidate
                .iter()
                .zip(self.controls.values())
                .all(|(a, b)| a == &b.spec))
    }
    pub fn prepare(
        ui: &Ui,
        layout: &Layout,
        runtime: &Arc<Runtime>,
        previous: Option<&Self>,
    ) -> Result<Self> {
        let specs = specs(ui, layout)?;
        // Reject an oversized initial context before compiling or reserving a
        // single guest; individual field bounds do not imply an aggregate fit.
        validate_default_contexts(ui, &specs, &BTreeMap::new(), &|id| default_field(ui, id))?;
        runtime.check_capacity(specs.len())?;
        let mut result = Self::default();
        for spec in specs {
            let code = seed(ui)
                .and_then(|seed| {
                    seed.resolved_libraries.iter().find(|library| {
                        library.library == spec.library && library.revision == spec.revision
                    })
                })
                .and_then(|library| library.component.as_ref())
                .ok_or("Missing exact component behavior code")?;
            let compiled = runtime.compile(&code.bytes, spec.code_hash)?;
            let guest = runtime.instantiate(compiled)?;
            let local = resumed_locals(&spec, previous);
            let instance = Instance { spec, guest, local };
            // Even default local state must fit the canonical record envelope.
            abi::encode_input(&Input {
                behavior: instance.spec.declaration.id,
                address: instance.spec.address.clone(),
                event: Event {
                    phase: Phase::Cancel,
                    pointer: 0,
                    a: 0,
                    b: 0,
                },
                properties: vec![],
                local: instance
                    .local
                    .iter()
                    .map(|(id, value)| Field {
                        id: *id,
                        value: value.clone(),
                    })
                    .collect(),
            })?;
            result
                .controls
                .insert(instance.spec.handler.control, instance);
        }
        Ok(result)
    }
    /// Check current app/literal properties together with preserved host locals
    /// before a prepared candidate can replace the active UI.
    pub fn validate_current(&self, ui: &Ui, doc: &StateAdapter) -> Result<()> {
        if self.controls.is_empty() {
            return Ok(());
        }
        let seed = seed(ui).ok_or("Missing component behavior source")?;
        let mut values = None;
        for instance in self.controls.values() {
            validate_local_context(&instance.spec, &instance.local, ui, seed, doc, &mut values)?;
        }
        Ok(())
    }
    /// Refresh a prepared candidate from the latest compatible host local state
    /// at activation. Validate every complete context before committing any map;
    /// the prepared guests and shared execution budget are unchanged.
    pub fn resume_from(&mut self, previous: &Self, ui: &Ui, doc: &StateAdapter) -> Result<()> {
        if self.controls.is_empty() {
            return Ok(());
        }
        let seed = seed(ui).ok_or("Missing component behavior source")?;
        let mut values = None;
        let mut resumed = Vec::with_capacity(self.controls.len());
        for instance in self.controls.values() {
            let local = resumed_locals(&instance.spec, Some(previous));
            validate_local_context(&instance.spec, &local, ui, seed, doc, &mut values)?;
            resumed.push(local);
        }
        for (instance, local) in self.controls.values_mut().zip(resumed) {
            instance.local = local;
        }
        Ok(())
    }
    pub fn dispatch(
        &mut self,
        ui: &Ui,
        doc: &mut StateAdapter,
        control: u8,
        event: Event,
        capture_allowed: bool,
    ) -> Result<Outcome> {
        let instance = self
            .controls
            .get_mut(&control)
            .ok_or("Unknown component behavior control")?;
        if ui.handlers.iter().find(|h| h.control == control) != Some(&instance.spec.handler) {
            return Err("Component behavior handler changed during dispatch".into());
        }
        let seed = seed(ui).ok_or("Missing component behavior source")?;
        let mut values = None;
        let properties = instance
            .spec
            .properties
            .iter()
            .map(|read| {
                Ok(Field {
                    id: read.id,
                    value: read_value(ui, seed, doc, &instance.spec.address, read, &mut values)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut input = Input {
            behavior: instance.spec.declaration.id,
            address: instance.spec.address.clone(),
            event,
            properties,
            local: instance
                .local
                .iter()
                .map(|(id, value)| Field {
                    id: *id,
                    value: value.clone(),
                })
                .collect(),
        };
        let output = instance.guest.call(&input)?;
        let mut local = instance.local.clone();
        let mut commands = Vec::new();
        let mut fields = BTreeSet::new();
        let mut capture = None;
        for command in output.commands {
            match command {
                Command::SetLocal { property, value } => {
                    let declared = instance
                        .spec
                        .declaration
                        .local
                        .iter()
                        .find(|p| p.id == property)
                        .ok_or("Undeclared component local property")?;
                    if value_type(&declared.default) != value_type(&value) {
                        return Err("Component local property type mismatch".into());
                    }
                    local.insert(property, value);
                }
                Command::Emit { outlet, value } => {
                    if event.phase == Phase::Cancel {
                        return Err("Cancelled component input cannot emit app state".into());
                    }
                    let outlet = instance
                        .spec
                        .outlets
                        .iter()
                        .find(|o| o.id == outlet)
                        .ok_or("Undeclared component outlet")?;
                    document::validate_property_value(&outlet.read.descriptor, &value)?;
                    // Re-read the source to reject a disconnected/rebound outlet;
                    // all resulting app effects remain inside the original handler.
                    let current = read_value(
                        ui,
                        seed,
                        doc,
                        &instance.spec.address,
                        &outlet.read,
                        &mut values,
                    )?;
                    let selector = outlet
                        .read
                        .selector
                        .as_ref()
                        .ok_or("Missing component outlet selector")?;
                    if !fields.insert(selector.field) {
                        return Err("Duplicate component result app field".into());
                    }
                    let value = match (
                        &instance.spec.handler.operation,
                        &selector.projection,
                        value,
                        current,
                    ) {
                        (
                            Operation::Set,
                            Projection::Identity,
                            Value::Number(n),
                            Value::Number(_),
                        ) => Value::Number(n),
                        (
                            Operation::Toggle,
                            Projection::Nonzero,
                            Value::Boolean(next),
                            Value::Boolean(current),
                        ) if next != current => Value::Number(if next { 1000. } else { 0. }),
                        _ => {
                            return Err(
                                "Component outlet exceeded its admitted action effect".into()
                            );
                        }
                    };
                    commands.push((selector.field, value));
                }
                Command::Capture | Command::ReleaseCapture => {
                    let requested = matches!(command, Command::Capture);
                    if !instance.spec.declaration.can_capture
                        || !capture_allowed
                        || event.phase == Phase::Activate
                        || (requested && !matches!(event.phase, Phase::Down | Phase::Move))
                    {
                        return Err(
                            "Component capture request exceeds admitted input ownership".into()
                        );
                    }
                    capture = Some(requested);
                }
            }
        }
        // Canonical combined local state must fit the next input. Validate every
        // output and capacity before changing any app/local/capture state.
        input.local = local
            .iter()
            .map(|(id, value)| Field {
                id: *id,
                value: value.clone(),
            })
            .collect();
        abi::encode_input(&input)?;
        let changed = if commands.is_empty() {
            false
        } else if event.phase == Phase::Activate {
            doc.set_typed(&commands)?
        } else {
            doc.defer_typed(&commands)?
        };
        let local_changed = local != instance.local;
        instance.local = local;
        Ok(Outcome {
            changed: changed || local_changed,
            capture,
        })
    }
}

#[cfg(test)]
mod tests;
