// SPDX-License-Identifier: LGPL-3.0-only
//! RFC 0036 bounded graph and component-behavior package authentication. App resolution separately
//! validates required classes, bindings, aggregate limits and the compiled scene.
use crate::{
    bundle::{self, BundleTree},
    canvas::{self, Result, err, hash, hex},
};
use otrios_interfaces::oib::library::FEATURE as LIBRARY_FEATURE;
use otrios_interfaces::{
    bundle::{BundleSignature, FILES_PATH, INFO_PATH, SIGNATURE_PATH},
    layout::{Expr, Shape, Value, ValueType},
    oib::{designer::*, graph::Action, library::*},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PACKAGE_BYTES: usize = 256 * 1024;
pub const MAX_GRAPH_BYTES: usize = 16 * 1024;
pub const MAX_IMPORTED_GRAPH_BYTES: usize = 32 * 1024;
pub const MAX_LIBRARIES: usize = 4;
pub const MAX_COMPONENT_BYTES: usize = 64 * 1024;
pub const MAX_IMPORTED_COMPONENT_BYTES: usize = MAX_LIBRARIES * MAX_COMPONENT_BYTES;
const DIGEST_DOMAIN: &[u8] = b"otrios-oib-library-v1\0";
const SIGNING_DOMAIN: &[u8] = b"otrios-oib-library-signature-v1\0";
const CONTRACT_DOMAIN: &[u8] = b"OTriOS\0oib-library-export-contract-v1\0";

#[derive(Clone, Debug)]
pub struct InspectedLibrary {
    pub info: LibraryInfo,
    pub graph: Graph,
    pub component: Option<ComponentCode>,
    pub revision: Revision,
    pub digest: String,
    pub files: usize,
    pub bytes: usize,
    pub signature: Option<BundleSignature>,
}

fn text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.contains('\0')
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn valid_id(id: &LibraryId) -> bool {
    id.0.split_once(':').is_some_and(|(publisher, name)| {
        digest(publisher)
            && !name.is_empty()
            && name.len() <= 63
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}
fn ordered<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut previous = None;
    for value in values {
        if previous.as_ref().is_some_and(|p| p >= &value) {
            return false;
        }
        previous = Some(value);
    }
    true
}
fn canonical<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>> {
    let bytes = flexbuffers::to_vec(value).map_err(err)?;
    if bytes.len() > limit {
        return Err("OIB library canonical byte budget".into());
    }
    Ok(bytes)
}
fn validate_info(info: &LibraryInfo) -> Result<()> {
    let executable = info.format == BEHAVIOR_INFO_FORMAT;
    if !matches!(info.format.as_str(), INFO_FORMAT | BEHAVIOR_INFO_FORMAT)
        || !valid_id(&info.library)
        || !text(&info.version, 64)
        || info.entry_graph != GRAPH_PATH
        || info.required_features.len() > 8
        || !ordered(info.required_features.iter())
        || !info.required_features.iter().any(|f| f == LIBRARY_FEATURE)
        || info.required_features.iter().any(|f| {
            !matches!(
                f.as_str(),
                LIBRARY_FEATURE
                    | "otrios-oib-v2"
                    | "otrios-layout-v5"
                    | bundle::AUTO_LAYOUT_FEATURE
                    | "otrios-component-behavior-v1"
            )
        })
        || info
            .required_features
            .iter()
            .any(|f| f == otrios_interfaces::oib::component_behavior::FEATURE)
            != executable
    {
        return Err("Unsupported OIB library metadata/feature".into());
    }
    Ok(())
}
pub fn encode_info(info: &LibraryInfo) -> Result<Vec<u8>> {
    validate_info(info)?;
    let mut bytes = if info.format == BEHAVIOR_INFO_FORMAT {
        BEHAVIOR_INFO_HEADER
    } else {
        INFO_HEADER
    }
    .to_vec();
    bytes.extend(canonical(info, 4096)?);
    Ok(bytes)
}
pub fn decode_info(bytes: &[u8]) -> Result<LibraryInfo> {
    let expected = if bytes.starts_with(INFO_HEADER) {
        INFO_FORMAT
    } else if bytes.starts_with(BEHAVIOR_INFO_HEADER) {
        BEHAVIOR_INFO_FORMAT
    } else {
        return Err("Unsupported OIB library package kind/version".into());
    };
    let info: LibraryInfo = bundle::metadata_limit(&bytes[INFO_HEADER.len()..], 4096, 16)?;
    validate_info(&info)?;
    if info.format != expected {
        return Err("Library metadata header/format mismatch".into());
    }
    Ok(info)
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
fn finite(value: &Value) -> bool {
    match value {
        Value::Number(n) => n.is_finite() && !(*n == 0. && n.is_sign_negative()),
        Value::Text(s) => s.len() <= 4096 && !s.contains('\0'),
        Value::Series(v) => v.len() <= 2016 && v.iter().all(|v| v.is_finite()),
        Value::TimedSeries(v) => v.len() <= 2016 && v.iter().flatten().all(|v| v.is_finite()),
        _ => true,
    }
}
fn expression(value: &Expr, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match value {
        Expr::State(_) => false,
        Expr::Literal(value) => finite(value),
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            expression(a, depth + 1) && expression(b, depth + 1)
        }
        Expr::Select { condition, yes, no } => {
            expression(condition, depth + 1)
                && expression(yes, depth + 1)
                && expression(no, depth + 1)
        }
        _ => true,
    }
}
fn definition(graph: &Graph, id: DefinitionId) -> Result<&ComponentDefinition> {
    graph
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or("Missing OIB library definition".into())
}
pub fn required_classes_compatible(
    required: &[ClassDescriptor],
    host: &[ClassDescriptor],
) -> Result<()> {
    for class in required {
        let admitted = host
            .iter()
            .find(|c| c.id == class.id)
            .ok_or("Library requires an unadmitted app class")?;
        if canvas::oib::contract::class_contract(class)?
            != canvas::oib::contract::class_contract(admitted)?
        {
            return Err("OIB library class contract mismatch".into());
        }
    }
    Ok(())
}

/// Package-local bounded source validation. Full property/slot/action and scene
/// admission still runs against the importing app's class table and grants.
pub fn validate_graph(graph: &Graph) -> Result<()> {
    let _profile = crate::canvas::profiling::span("library.graph.validate");
    if !matches!(graph.format.as_str(), FORMAT | BEHAVIOR_FORMAT)
        || (graph.format == BEHAVIOR_FORMAT) == graph.behaviors.is_empty()
        || graph.required_classes.is_empty()
        || graph.required_classes.len() > MAX_CLASS_DESCRIPTORS
        || graph.definitions.is_empty()
        || graph.definitions.len() > MAX_DEFINITIONS
        || graph.exports.is_empty()
        || graph.exports.len() > MAX_DEFINITIONS
        || !ordered(graph.required_classes.iter().map(|c| c.id))
        || !ordered(graph.definitions.iter().map(|d| d.id))
        || !ordered(graph.exports.iter().map(|e| e.id))
        || graph
            .definitions
            .iter()
            .map(|d| d.nodes.len())
            .sum::<usize>()
            > MAX_AUTHORED_OBJECTS
    {
        return Err("OIB library graph identity/count budget".into());
    }
    let classes: BTreeMap<_, _> = graph.required_classes.iter().map(|c| (c.id, c)).collect();
    let mut properties = 0;
    for class in &graph.required_classes {
        properties += class.properties.len();
        if class.id.0 == 0
            || !text(&class.name, 64)
            || !ordered(class.properties.iter().map(|p| p.id))
            || !ordered(class.owned_child_classes.iter())
            || !ordered(class.visual_child_classes.iter())
            || class.owned_child_classes.len() > 16
            || class.visual_child_classes.len() > 16
            || class
                .owned_child_classes
                .iter()
                .chain(&class.visual_child_classes)
                .any(|id| !classes.contains_key(id))
        {
            return Err("Invalid library class requirement".into());
        }
        let mut seen = BTreeSet::new();
        let mut current = Some(class.id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err("Cyclic library class requirement".into());
            }
            current = classes.get(&id).ok_or("Missing library base class")?.base;
        }
        for p in &class.properties {
            if p.id.0 == 0
                || p.value_type != value_type(&p.default)
                || !finite(&p.default)
                || !text(&p.name, 64)
                || !text(&p.label, 96)
                || p.number_range.is_some_and(|r| {
                    r.iter().any(|n| !n.is_finite() || *n < 0. || *n > 1000.) || r[0] > r[1]
                })
            {
                return Err("Invalid library property requirement".into());
            }
        }
    }
    if properties > MAX_PROPERTY_DESCRIPTORS {
        return Err("Library class property budget".into());
    }
    canvas::oib::document::validate_declarations(&graph.required_classes, &graph.definitions)?;
    for def in &graph.definitions {
        if def.id.0 == 0
            || !text(&def.name, 64)
            || def.nodes.is_empty()
            || !ordered(def.nodes.iter().map(|n| n.id))
            || def.exposed.len() > 64
            || def.slots.len() > 16
            || !ordered(def.exposed.iter().map(|e| e.property))
            || !ordered(def.slots.iter().map(|s| s.id))
        {
            return Err("Invalid library definition".into());
        }
        let nodes: BTreeMap<_, _> = def.nodes.iter().map(|n| (n.id, n)).collect();
        if nodes.get(&def.root).is_none_or(|n| n.owner.is_some()) {
            return Err("Library root must exist without owner".into());
        }
        let mut painted = BTreeSet::new();
        for node in &def.nodes {
            if node.id.0 == 0
                || !text(&node.name, 64)
                || !classes.contains_key(&node.class)
                || node.defaults.len() > 64
                || !ordered(node.defaults.iter().map(|p| p.property))
                || node
                    .defaults
                    .iter()
                    .any(|p| p.property.0 == 0 || !finite(&p.value))
                || (node.id != def.root && node.owner.is_none())
            {
                return Err("Invalid library definition node".into());
            }
            let mut current = Some(node.id);
            let mut seen = BTreeSet::new();
            while let Some(id) = current {
                if !seen.insert(id) {
                    return Err("Cyclic library node ownership".into());
                }
                current = nodes.get(&id).ok_or("Missing library owner")?.owner;
            }
            for child in &node.visual_children {
                if *child == def.root || !nodes.contains_key(child) || !painted.insert(*child) {
                    return Err("Invalid library paint parent".into());
                }
            }
            if let Some(reference) = &node.reference {
                definition(graph, reference.definition)?;
                if reference.assignments.len() > 64
                    || !ordered(reference.assignments.iter().map(|a| &a.target))
                    || reference
                        .assignments
                        .iter()
                        .any(|a| matches!(a.source, AssignmentSource::AppBinding(_)))
                {
                    return Err("Library references cannot bind app fields".into());
                }
            }
            if let Some(visual) = &node.visual {
                let shape_ok = match &visual.shape {
                    Shape::Canvas { .. } => true,
                    Shape::Text {
                        value,
                        scale,
                        color,
                    } => [value, scale, color].iter().all(|v| expression(v, 0)),
                    Shape::Superellipse {
                        radius,
                        exponent,
                        color,
                    } => exponent.is_finite() && expression(radius, 0) && expression(color, 0),
                    Shape::Circle { color } => expression(color, 0),
                    _ => false,
                };
                if !shape_ok
                    || visual.parameters.len() > 16
                    || !visual.frame.iter().all(|v| expression(v, 0))
                    || !expression(&visual.visible, 0)
                    || visual
                        .semantics
                        .as_ref()
                        .is_some_and(|s| !expression(&s.label, 0) || !expression(&s.checked, 0))
                {
                    return Err("Unsupported library visual primitive/local state".into());
                }
            }
        }
        // Paint cycles are distinct from lifetime ownership cycles.
        fn paint(
            id: NodeId,
            nodes: &BTreeMap<NodeId, &DefinitionNode>,
            stack: &mut BTreeSet<NodeId>,
        ) -> Result<()> {
            if !stack.insert(id) {
                return Err("Cyclic library paint tree".into());
            }
            for child in &nodes[&id].visual_children {
                paint(*child, nodes, stack)?;
            }
            stack.remove(&id);
            Ok(())
        }
        for node in &def.nodes {
            paint(node.id, &nodes, &mut BTreeSet::new())?;
        }
    }
    for def in &graph.definitions {
        reachable(graph, def.id)?;
    }
    for export in &graph.exports {
        if export.id.0 == 0 || !text(&export.name, 64) {
            return Err("Invalid library export".into());
        }
        reachable(graph, export.definition)?;
    }
    validate_behaviors(graph)?;
    if canonical(graph, MAX_GRAPH_BYTES)?.len() > MAX_GRAPH_BYTES {
        return Err("Library graph byte budget".into());
    }
    Ok(())
}
fn behavior_property<'a>(
    graph: &'a Graph,
    start: DefinitionId,
    path: &PropertyPath,
) -> Result<&'a PropertyDescriptor> {
    if path.references.len() > 8 || path.node.0 == 0 || path.property.0 == 0 {
        return Err("Invalid component behavior property path".into());
    }
    let mut def = definition(graph, start)?;
    for reference in &path.references {
        let id = def
            .nodes
            .iter()
            .find(|node| node.id == *reference)
            .and_then(|node| node.reference.as_ref())
            .ok_or("Missing behavior property reference")?
            .definition;
        def = definition(graph, id)?;
    }
    let node = def
        .nodes
        .iter()
        .find(|node| node.id == path.node)
        .ok_or("Missing behavior property node")?;
    if node.id == def.root
        && let Some(property) = def.properties.iter().find(|p| p.id == path.property)
    {
        return Ok(property);
    }
    let mut class = Some(node.class);
    while let Some(id) = class {
        let descriptor = graph
            .required_classes
            .iter()
            .find(|class| class.id == id)
            .ok_or("Missing behavior class")?;
        if let Some(property) = descriptor
            .properties
            .iter()
            .find(|property| property.id == path.property)
        {
            return Ok(property);
        }
        class = descriptor.base;
    }
    Err("Missing behavior property descriptor".into())
}
fn validate_behaviors(graph: &Graph) -> Result<()> {
    use otrios_interfaces::oib::component_behavior::{MAX_COMMANDS, MAX_FIELDS};
    if graph.behaviors.len() > 8 || !ordered(graph.behaviors.iter().map(|b| b.id)) {
        return Err("Component behavior declaration budget/order".into());
    }
    let mut attached = BTreeSet::new();
    for behavior in &graph.behaviors {
        let def = definition(graph, behavior.definition)?;
        if behavior.id == 0
            || !attached.insert((behavior.definition, behavior.node))
            || def
                .nodes
                .iter()
                .find(|node| node.id == behavior.node)
                .and_then(|node| node.visual.as_ref())
                .and_then(|visual| visual.action.as_ref())
                .is_none()
            || behavior.properties.len() > MAX_FIELDS
            || behavior.local.len() > MAX_FIELDS
            || behavior.outlets.is_empty()
            || behavior.outlets.len() > MAX_COMMANDS
            || !ordered(behavior.properties.iter().map(|p| p.id))
            || !ordered(behavior.local.iter().map(|p| p.id))
            || !ordered(behavior.outlets.iter().map(|p| p.id))
        {
            return Err("Invalid component behavior attachment/declaration".into());
        }
        for property in &behavior.properties {
            if property.id == 0 {
                return Err("Zero component property identity".into());
            }
            behavior_property(graph, behavior.definition, &property.source)?;
        }
        for property in &behavior.local {
            if property.id == 0 || !finite(&property.default) {
                return Err("Invalid component local-property declaration".into());
            }
            otrios_interfaces::oib::component_behavior::encode_output(
                &otrios_interfaces::oib::component_behavior::Output {
                    commands: vec![
                        otrios_interfaces::oib::component_behavior::Command::SetLocal {
                            property: property.id,
                            value: property.default.clone(),
                        },
                    ],
                },
            )?;
        }
        for outlet in &behavior.outlets {
            if outlet.id == 0
                || behavior_property(graph, behavior.definition, &outlet.source)?.value_type
                    != outlet.value_type
            {
                return Err("Invalid typed component outlet".into());
            }
        }
    }
    Ok(())
}
/// Validate exact transported code as part of whole-library candidate admission.
/// The small immutable cache avoids recompilation during repeated graph queries;
/// each Client still owns separate execution budgets and occurrence stores.
pub fn validate_component(graph: &Graph, component: Option<&ComponentCode>) -> Result<()> {
    use crate::canvas::component_behavior::{Compiled, Runtime};
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex, OnceLock},
    };
    type Entry = (Revision, Arc<Compiled>);
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    static CACHE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
    if graph.behaviors.is_empty() {
        return if component.is_none() {
            Ok(())
        } else {
            Err("Undeclared component code in data-only library".into())
        };
    }
    if graph.format != BEHAVIOR_FORMAT {
        return Err("Component behavior requires library graph v2".into());
    }
    let component = component.ok_or("Missing declared component code")?;
    if component.abi != otrios_interfaces::oib::component_behavior::ABI
        || component.bytes.is_empty()
        || component.bytes.len() > MAX_COMPONENT_BYTES
        || Revision::from(Sha256::digest(&component.bytes)) != component.sha256
    {
        return Err("Invalid component code ABI/digest/budget".into());
    }
    let mut cache = CACHE
        .lock()
        .map_err(|_| "Component admission cache poisoned")?;
    if let Some(index) = cache
        .iter()
        .position(|(digest, _)| *digest == component.sha256)
    {
        let entry = cache
            .remove(index)
            .ok_or("Component admission cache changed")?;
        cache.push_back(entry);
        return Ok(());
    }
    if cache.len() == 8 {
        cache.pop_front();
    }
    let compiled = RUNTIME
        .get_or_init(Runtime::default)
        .compile(&component.bytes, component.sha256)?;
    cache.push_back((component.sha256, compiled));
    Ok(())
}

fn reachable(graph: &Graph, root: DefinitionId) -> Result<BTreeSet<DefinitionId>> {
    fn visit(
        graph: &Graph,
        id: DefinitionId,
        stack: &mut BTreeSet<DefinitionId>,
        all: &mut BTreeSet<DefinitionId>,
        heights: &mut BTreeMap<DefinitionId, usize>,
    ) -> Result<usize> {
        if let Some(height) = heights.get(&id) {
            return Ok(*height);
        }
        if !stack.insert(id) {
            return Err("Cyclic library definition reference".into());
        }
        let mut height = 0;
        for node in &definition(graph, id)?.nodes {
            if let Some(r) = &node.reference {
                height = height.max(
                    visit(graph, r.definition, stack, all, heights)?
                        .checked_add(1)
                        .ok_or("Library reference depth overflow")?,
                );
            }
        }
        if height > 8 {
            return Err("Deep library definition reference".into());
        }
        stack.remove(&id);
        all.insert(id);
        heights.insert(id, height);
        Ok(height)
    }
    let mut all = BTreeSet::new();
    visit(
        graph,
        root,
        &mut BTreeSet::new(),
        &mut all,
        &mut BTreeMap::new(),
    )?;
    Ok(all)
}

#[derive(Serialize)]
struct SlotSignature<'a> {
    id: SlotId,
    references: &'a [NodeId],
    node: NodeId,
    admitted_classes: &'a [ClassId],
    cardinality: [u16; 2],
    layout: SlotLayout,
}
#[derive(Serialize)]
enum SourceSignature<'a> {
    Literal(ValueType),
    Forward(&'a PropertyPath),
}
#[derive(Serialize)]
struct NodeSignature<'a> {
    id: NodeId,
    class: ClassId,
    owner: Option<NodeId>,
    reference: Option<(DefinitionId, Vec<(&'a PropertyPath, SourceSignature<'a>)>)>,
    action: Option<&'a Action>,
}
#[derive(Serialize)]
struct DefinitionSignature<'a> {
    id: DefinitionId,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    properties: Vec<canvas::oib::contract::PropertySignature>,
    root: NodeId,
    nodes: Vec<NodeSignature<'a>>,
    exposed: &'a [ExposedProperty],
    slots: Vec<SlotSignature<'a>>,
}
pub fn export_contract(graph: &Graph, export: ExportId) -> Result<Revision> {
    let _profile = crate::canvas::profiling::span("library.export_contract");
    let export = graph
        .exports
        .iter()
        .find(|e| e.id == export)
        .ok_or("Missing library export")?;
    let closure = reachable(graph, export.definition)?;
    let classes = graph
        .required_classes
        .iter()
        .map(|c| Ok((c.id, canvas::oib::contract::class_contract(c)?)))
        .collect::<Result<Vec<_>>>()?;
    let definitions = closure
        .iter()
        .map(|id| {
            let def = definition(graph, *id)?;
            let nodes = def
                .nodes
                .iter()
                .map(|node| {
                    let reference = if let Some(r) = &node.reference {
                        let assignments = r
                            .assignments
                            .iter()
                            .map(|a| {
                                let source = match &a.source {
                                    AssignmentSource::Literal(v) => {
                                        SourceSignature::Literal(value_type(v))
                                    }
                                    AssignmentSource::Forward(path) => {
                                        SourceSignature::Forward(path)
                                    }
                                    AssignmentSource::AppBinding(_) => {
                                        return Err("Library app binding is prohibited".into());
                                    }
                                };
                                Ok((&a.target, source))
                            })
                            .collect::<Result<Vec<_>>>()?;
                        Some((r.definition, assignments))
                    } else {
                        None
                    };
                    Ok(NodeSignature {
                        id: node.id,
                        class: node.class,
                        owner: node.owner,
                        reference,
                        action: node.visual.as_ref().and_then(|v| v.action.as_ref()),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let slots = def
                .slots
                .iter()
                .map(|s| SlotSignature {
                    id: s.id,
                    references: &s.references,
                    node: s.node,
                    admitted_classes: &s.admitted_classes,
                    cardinality: s.cardinality,
                    layout: s.layout,
                })
                .collect();
            Ok(DefinitionSignature {
                id: def.id,
                properties: def
                    .properties
                    .iter()
                    .map(canvas::oib::contract::property_signature)
                    .collect(),
                root: def.root,
                nodes,
                exposed: &def.exposed,
                slots,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut bytes = CONTRACT_DOMAIN.to_vec();
    bytes.extend(canonical(
        &(export.id, export.definition, classes, definitions),
        MAX_PACKAGE_BYTES,
    )?);
    if !graph.behaviors.is_empty() {
        let behaviors = graph
            .behaviors
            .iter()
            .filter(|b| closure.contains(&b.definition))
            .map(|b| {
                (
                    b.id,
                    b.definition,
                    b.node,
                    &b.properties,
                    b.local
                        .iter()
                        .map(|p| (p.id, value_type(&p.default)))
                        .collect::<Vec<_>>(),
                    &b.outlets,
                    b.can_capture,
                )
            })
            .collect::<Vec<_>>();
        if !behaviors.is_empty() {
            bytes.extend(canonical(
                &(otrios_interfaces::oib::component_behavior::ABI, behaviors),
                MAX_GRAPH_BYTES,
            )?);
        }
    }
    Ok(Sha256::digest(&bytes).into())
}
pub fn seal_graph(mut graph: Graph) -> Result<Graph> {
    graph.required_classes.sort_by_key(|c| c.id);
    graph.definitions.sort_by_key(|d| d.id);
    graph.exports.sort_by_key(|e| e.id);
    graph.behaviors.sort_by_key(|b| b.id);
    for behavior in &mut graph.behaviors {
        behavior.properties.sort_by_key(|p| p.id);
        behavior.local.sort_by_key(|p| p.id);
        behavior.outlets.sort_by_key(|p| p.id);
    }
    for class in &mut graph.required_classes {
        class.properties.sort_by_key(|p| p.id);
        class.owned_child_classes.sort();
        class.visual_child_classes.sort();
    }
    for def in &mut graph.definitions {
        def.nodes.sort_by_key(|n| n.id);
        def.properties.sort_by_key(|p| p.id);
        def.exposed.sort_by_key(|e| e.property);
        def.slots.sort_by_key(|s| s.id);
        for n in &mut def.nodes {
            n.defaults.sort_by_key(|p| p.property);
            if let Some(r) = &mut n.reference {
                r.assignments.sort_by(|a, b| a.target.cmp(&b.target));
            }
        }
    }
    validate_graph(&graph)?;
    for index in 0..graph.exports.len() {
        graph.exports[index].contract = export_contract(&graph, graph.exports[index].id)?;
    }
    Ok(graph)
}
pub fn encode_graph(graph: &Graph) -> Result<Vec<u8>> {
    validate_graph(graph)?;
    for export in &graph.exports {
        if export.contract != export_contract(graph, export.id)? {
            return Err("Library export contract differs from source".into());
        }
    }
    canonical(graph, MAX_GRAPH_BYTES)
}
pub fn decode_graph(bytes: &[u8]) -> Result<Graph> {
    let graph: Graph = bundle::metadata_limit(bytes, MAX_GRAPH_BYTES, 32)?;
    if encode_graph(&graph)? != bytes {
        return Err("Noncanonical library graph".into());
    }
    Ok(graph)
}
fn payload_path(path: &str) -> bool {
    bundle::valid_path(path)
        && (path == INFO_PATH
            || path == GRAPH_PATH
            || path == CODE_PATH
            || path.starts_with("Notices/"))
}
pub fn pack(info: &LibraryInfo, graph: &Graph) -> Result<BundleTree> {
    pack_parts(info, graph, None)
}
pub fn pack_with_behavior(info: &LibraryInfo, graph: &Graph, bytes: &[u8]) -> Result<BundleTree> {
    pack_parts(info, graph, Some(bytes))
}
fn pack_parts(info: &LibraryInfo, graph: &Graph, bytes: Option<&[u8]>) -> Result<BundleTree> {
    let mut tree = BundleTree::from([
        (INFO_PATH.into(), encode_info(info)?),
        (GRAPH_PATH.into(), encode_graph(graph)?),
    ]);
    if let Some(bytes) = bytes {
        tree.insert(CODE_PATH.into(), bytes.to_vec());
    }
    tree.insert(
        FILES_PATH.into(),
        bundle::encode(&bundle::file_manifest(&tree))?,
    );
    inspect(&tree)?;
    Ok(tree)
}
pub fn inspect(tree: &BundleTree) -> Result<InspectedLibrary> {
    let bytes = bundle::validate_files(
        tree,
        bundle::MAX_FILES,
        MAX_PACKAGE_BYTES,
        MAX_PACKAGE_BYTES,
        payload_path,
    )?;
    let info = decode_info(tree.get(INFO_PATH).ok_or("Missing library Info")?)?;
    let graph = decode_graph(tree.get(GRAPH_PATH).ok_or("Missing library graph")?)?;
    if bundle::definitions_use_auto_layout(&graph.definitions)
        && !info
            .required_features
            .iter()
            .any(|f| f == bundle::AUTO_LAYOUT_FEATURE)
    {
        return Err("Missing required auto layout feature".into());
    }
    let component = tree.get(CODE_PATH).map(|bytes| ComponentCode {
        abi: otrios_interfaces::oib::component_behavior::ABI,
        sha256: Sha256::digest(bytes).into(),
        bytes: bytes.clone(),
    });
    if (info.format == BEHAVIOR_INFO_FORMAT) != (graph.format == BEHAVIOR_FORMAT) {
        return Err("Library package/graph behavior version mismatch".into());
    }
    validate_component(&graph, component.as_ref())?;
    let mut hash = Sha256::new();
    hash.update(DIGEST_DOMAIN);
    hash.update(&tree[FILES_PATH]);
    let revision: Revision = hash.finalize().into();
    let signature = tree
        .get(SIGNATURE_PATH)
        .map(|b| bundle::metadata::<BundleSignature>(b))
        .transpose()?;
    if signature.as_ref().is_some_and(|s| {
        s.format != SIGNATURE_FORMAT || s.signature.len() != 64 || !digest(&s.key_sha256)
    }) {
        return Err("Invalid library signature envelope".into());
    }
    Ok(InspectedLibrary {
        info,
        graph,
        component,
        revision,
        digest: hex(&revision),
        files: tree.len(),
        bytes,
        signature,
    })
}
pub fn signing_payload(digest: &str, key: &[u8; 32]) -> Result<Vec<u8>> {
    bundle::signing_payload_in_domain(digest, key, SIGNING_DOMAIN)
}
pub fn attach_signature(tree: &mut BundleTree, key: &[u8; 32], signature: &[u8]) -> Result<()> {
    if signature.len() != 64 {
        return Err("Expected Ed25519 library signature".into());
    }
    tree.insert(
        SIGNATURE_PATH.into(),
        bundle::encode(&BundleSignature {
            format: SIGNATURE_FORMAT.into(),
            key_sha256: hex(&hash(key)),
            signature: signature.to_vec(),
        })?,
    );
    inspect(tree)?;
    Ok(())
}
#[derive(Debug)]
pub struct VerifiedLibrary {
    summary: InspectedLibrary,
    tree: BundleTree,
}
impl VerifiedLibrary {
    pub fn summary(&self) -> &InspectedLibrary {
        &self.summary
    }
    pub fn graph(&self) -> &Graph {
        &self.summary.graph
    }
    pub fn component(&self) -> Option<&ComponentCode> {
        self.summary.component.as_ref()
    }
    pub fn revision(&self) -> Revision {
        self.summary.revision
    }
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.tree.iter().map(|(p, b)| (p.as_str(), b.as_slice()))
    }
    pub fn file(&self, path: &str) -> Result<&[u8]> {
        self.tree
            .get(path)
            .map(Vec::as_slice)
            .ok_or("Missing authenticated library file".into())
    }
}
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub fn verify(tree: BundleTree, key: &[u8; 32]) -> Result<VerifiedLibrary> {
    let summary = inspect(&tree)?;
    if !summary
        .info
        .library
        .0
        .starts_with(&format!("{}:", hex(&hash(key))))
    {
        return Err("Library identity differs from trusted publisher".into());
    }
    bundle::verify_signature(
        summary.signature.as_ref().ok_or("Unsigned OIB library")?,
        &summary.digest,
        key,
        SIGNATURE_FORMAT,
        SIGNING_DOMAIN,
    )?;
    Ok(VerifiedLibrary { summary, tree })
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod behavior_tests;
