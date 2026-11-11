// SPDX-License-Identifier: LGPL-3.0-only
//! Canonical graph lowering into the existing layout, input and property engines.
use super::*;
use otrios_interfaces::oib::designer::Document;
use otrios_interfaces::{
    canvas::{GestureKind, Handler, Projection, Selector},
    layout::{Binding, Responder},
    oib::{designer::*, graph as source},
};

#[path = "graph_preview.rs"]
mod preview;
pub(crate) use preview::{DefinitionPreview, definition_preview, definition_preview_with_values};

/// Admitted typed chrome plus disposable graph output. Internal transformations
/// update `blueprint`; conversion to an artifact Ui serializes at the boundary.
#[derive(Clone)]
pub(crate) struct PreparedUi {
    pub(crate) ui: Ui,
    pub(crate) blueprint: Blueprint,
}
impl PreparedUi {
    pub(crate) fn from_ui(ui: &Ui) -> Result<Self> {
        Ok(Self {
            ui: ui.clone(),
            blueprint: Layout::from_bytes(&ui.layout)?.blueprint().clone(),
        })
    }
    pub(crate) fn repaint(
        &mut self,
        colors: &BTreeMap<u16, otrios_interfaces::scene::Color>,
    ) -> Result<()> {
        repaint_blueprint(&mut self.blueprint, colors)
    }
    pub(crate) fn to_ui(&self) -> Result<Ui> {
        let mut ui = self.ui.clone();
        ui.layout = Layout::encode(&self.blueprint)?;
        Ok(ui)
    }
    /// Reuse a validated runtime encoding only when its complete typed input is
    /// identical. Artifact and runtime layouts can differ for legacy targets;
    /// those continue through the independent artifact encoder.
    pub(crate) fn to_ui_with_layout(&self, layout: &Layout) -> Result<Ui> {
        if !layout.has_same_encoding_input(&self.blueprint) {
            return self.to_ui();
        }
        let _profile = crate::canvas::profiling::span("oib.compiled.shared_encoding");
        let mut ui = self.ui.clone();
        ui.layout = layout.bytes().to_vec();
        Ok(ui)
    }
    pub(crate) fn finish(mut self) -> Result<Ui> {
        self.ui.layout = Layout::encode(&self.blueprint)?;
        Ok(self.ui)
    }
}

/// Recolor admitted terminal outputs without changing geometry or bindings.
pub(crate) fn repaint_blueprint(
    blueprint: &mut Blueprint,
    colors: &BTreeMap<u16, otrios_interfaces::scene::Color>,
) -> Result<()> {
    for (id, color) in colors {
        let node = blueprint
            .templates
            .iter_mut()
            .find_map(|template| super::node(&mut template.root, *id))
            .ok_or("Missing retained OIB paint target")?;
        let expression = match &mut node.shape {
            Shape::Text { color, .. }
            | Shape::Superellipse { color, .. }
            | Shape::Circle { color } => color,
            _ => return Err("Unsupported retained OIB paint target".into()),
        };
        *expression = Expr::Literal(Value::Color(*color));
    }
    Ok(())
}

fn error(message: &str) -> String {
    message.into()
}
fn literal(number: f32) -> Expr {
    Expr::Literal(Value::Number(number))
}
fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && !value.contains('\0')
}
fn definition(graph: &source::Document, id: DefinitionId) -> Result<&ComponentDefinition> {
    graph
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| error("Unknown graph definition"))
}
fn preset(graph: &source::Document, id: u64) -> Result<&source::Preset> {
    graph
        .presets
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| error("Unknown graph preset"))
}
fn projection_allowed(value: &Projection) -> bool {
    matches!(
        value,
        Projection::Identity
            | Projection::Percent
            | Projection::Nonzero
            | Projection::Number { .. }
            | Projection::ChooseText { .. }
            | Projection::ChooseColor { .. }
    )
}
fn intern(selectors: &mut Vec<Selector>, field: u16, projection: Projection) -> Result<Binding> {
    if field == 0 || field > 64 || !projection_allowed(&projection) {
        return Err(error("Invalid graph app binding"));
    }
    if let Some(selector) = selectors
        .iter()
        .find(|s| s.field == field && s.projection == projection)
    {
        return Ok(Binding::Local(selector.local));
    }
    if selectors.len() >= 128 {
        return Err(error("Graph selector budget"));
    }
    let local = u16::try_from(selectors.len() + 1).map_err(err)?;
    selectors.push(Selector {
        local,
        field,
        projection,
    });
    Ok(Binding::Local(local))
}
fn assignment(value: &source::Assignment, selectors: &mut Vec<Selector>) -> Result<Assignment> {
    Ok(Assignment {
        target: value.target.clone(),
        source: match &value.source {
            source::Source::Literal(value) => AssignmentSource::Literal(value.clone()),
            source::Source::Forward(path) => AssignmentSource::Forward(path.clone()),
            source::Source::AppBinding { field, projection } => {
                AssignmentSource::AppBinding(intern(selectors, *field, projection.clone())?)
            }
        },
    })
}
pub(super) fn source_selectors(seed: &Seed) -> Result<Vec<Selector>> {
    let graph = seed
        .graph
        .as_ref()
        .ok_or("Canonical OIB graph is required")?;
    let mut selectors = vec![];
    for p in graph.presets.iter().filter(|p| p.palette) {
        for a in &p.assignments {
            assignment(a, &mut selectors)?;
        }
    }
    for i in &graph.instances {
        let p = preset(graph, i.preset)?;
        if !p.palette {
            for a in &p.assignments {
                assignment(a, &mut selectors)?;
            }
        }
    }
    for grant in &graph.grants {
        intern(
            &mut selectors,
            grant.field,
            match grant.operation {
                source::ActionOperation::Set => Projection::Identity,
                source::ActionOperation::Toggle => Projection::Nonzero,
            },
        )?;
    }
    Ok(selectors)
}
fn frame(value: [f32; 4]) -> Result<()> {
    if value
        .iter()
        .any(|n| !n.is_finite() || (*n == 0. && n.is_sign_negative()))
        || value[0] < 0.
        || value[1] < 0.
        || value[0] > 16000.
        || value[1] > 16000.
        || value[2] <= 0.
        || value[3] <= 0.
        || value[2] > 1000.
        || value[3] > 1000.
    {
        return Err(error("Invalid graph preset frame"));
    }
    Ok(())
}
fn validate_source(graph: &source::Document) -> Result<()> {
    if graph.id.0 == 0
        || !text(&graph.name)
        || graph.definitions.is_empty()
        || graph.definitions.len() > MAX_DEFINITIONS
        || graph.classes.len() > MAX_CLASS_DESCRIPTORS
        || graph.presets.is_empty()
        || graph.presets.len() > MAX_DEFINITIONS
        || graph.instances.is_empty()
        || graph.instances.len() > MAX_AUTHORED_OBJECTS
        || graph
            .definitions
            .iter()
            .map(|d| d.nodes.len())
            .sum::<usize>()
            > MAX_AUTHORED_OBJECTS
        || !graph.preview_stride.is_finite()
        || !(48. ..=1000.).contains(&graph.preview_stride)
        || graph.preview_viewport == graph.inspector_viewport
        || graph
            .inspector_content
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > 16000.)
        || graph.grants.len() > 64
    {
        return Err(error("Invalid or oversized canonical OIB graph"));
    }
    for definition in &graph.definitions {
        for node in &definition.nodes {
            if let Some(visual) = &node.visual
                && let Some(policy) = &visual.layout
            {
                crate::dashboard::layout::auto_layout::validate_policy(policy)?;
                if visual.resize.is_some()
                    || (policy.container.is_some()
                        && (node.reference.is_some()
                            || !matches!(visual.shape, Shape::Canvas { .. })))
                {
                    return Err(error(
                        "Auto Layout cannot combine legacy resize or use a non-container flow",
                    ));
                }
            }
            if node.reference.as_ref().is_some_and(|reference| {
                reference
                    .assignments
                    .iter()
                    .any(|a| matches!(a.source, AssignmentSource::AppBinding(_)))
            }) {
                return Err(error(
                    "Canonical definition references cannot contain renderer-local bindings",
                ));
            }
            if let Some(visual) = &node.visual
                && (visual.parameters.len() > 16
                    || visual
                        .parameters
                        .iter()
                        .any(|p| !projection_allowed(&p.projection))
                    || visual
                        .parameters
                        .iter()
                        .map(|p| p.local)
                        .collect::<BTreeSet<_>>()
                        .len()
                        != visual.parameters.len()
                    || !matches!(
                        visual.shape,
                        Shape::Canvas { .. }
                            | Shape::Text { .. }
                            | Shape::Superellipse { .. }
                            | Shape::Circle { .. }
                    ))
            {
                return Err(error("Unsupported graph visual declaration"));
            }
        }
    }
    let mut ids = BTreeSet::new();
    for value in &graph.presets {
        if value.id == 0 || !ids.insert(value.id) || !text(&value.name) {
            return Err(error("Invalid graph preset identity"));
        }
        definition(graph, value.definition)?;
        if let Some(value) = value.frame {
            frame(value)?;
        }
    }
    ids.clear();
    for value in &graph.instances {
        if value.id.0 == 0 || !ids.insert(value.id.0) || !text(&value.name) {
            return Err(error("Invalid graph instance identity"));
        }
        preset(graph, value.preset)?;
    }
    let shell = graph
        .instances
        .iter()
        .find(|i| i.id == graph.presented_root)
        .ok_or("Missing graph presented root")?;
    if preset(graph, shell.preset)?.palette
        || shell.owner != Owner::Document
        || shell.visual_parent.is_some()
    {
        return Err(error("Presented graph root must be a document-owned shell"));
    }
    let shell_definition = definition(graph, preset(graph, shell.preset)?.definition)?;
    let canvas = ObjectAddress {
        instance: shell.id,
        references: vec![],
        node: shell_definition.root,
    };
    let mut orders = BTreeSet::new();
    for instance in &graph.instances {
        let preset = preset(graph, instance.preset)?;
        if instance.owner != Owner::Document {
            return Err(error(
                "Initial graph objects must be document-owned; author owned slot content through document edits",
            ));
        }
        if preset.palette {
            if instance.visual_parent.as_ref() != Some(&canvas) || !orders.insert(instance.order) {
                return Err(error(
                    "Initial visual instances require unique canvas sibling order",
                ));
            }
        } else if instance.id != graph.presented_root {
            let def = definition(graph, preset.definition)?;
            if instance.visual_parent.is_some() || def.nodes.iter().any(|n| n.visual.is_some()) {
                return Err(error(
                    "Initial fixed objects must be nonvisual; visual objects use palette presets",
                ));
            }
        }
    }
    Ok(())
}
fn paths(graph: &source::Document, start: DefinitionId) -> Result<Vec<PropertyPath>> {
    fn walk(
        graph: &source::Document,
        id: DefinitionId,
        prefix: &mut Vec<NodeId>,
        seen: &mut BTreeSet<DefinitionId>,
        out: &mut Vec<PropertyPath>,
    ) -> Result<()> {
        if prefix.len() > 8 || !seen.insert(id) {
            return Err(error("Cyclic/deep graph component reference"));
        }
        let def = definition(graph, id)?;
        let metadata = Document {
            id: graph.id,
            name: graph.name.clone(),
            class: graph.class,
            presented_root: graph.presented_root,
            hierarchy_viewport: graph.hierarchy_viewport,
            classes: graph.classes.clone(),
            definitions: vec![],
            instances: vec![],
            mappings: Mappings {
                objects: vec![],
                definitions: vec![],
                roots: vec![],
            },
            placements: vec![],
            connections: vec![],
        };
        for node in &def.nodes {
            for property in document::node_descriptors(&metadata, def, node)? {
                out.push(PropertyPath {
                    references: prefix.clone(),
                    node: node.id,
                    property: property.id,
                });
            }
            if let Some(reference) = &node.reference {
                prefix.push(node.id);
                walk(graph, reference.definition, prefix, seen, out)?;
                prefix.pop();
            }
        }
        seen.remove(&id);
        Ok(())
    }
    let mut out = vec![];
    walk(graph, start, &mut vec![], &mut BTreeSet::new(), &mut out)?;
    out.sort();
    if out.len() > 128 || out.windows(2).any(|w| w[0] == w[1]) {
        return Err(error("Graph property path budget"));
    }
    Ok(out)
}
fn terminal<'a>(
    graph: &'a source::Document,
    start: DefinitionId,
    path: &PropertyPath,
) -> Result<(&'a ComponentDefinition, &'a DefinitionNode)> {
    let mut def = definition(graph, start)?;
    for boundary in &path.references {
        let reference = def
            .nodes
            .iter()
            .find(|n| n.id == *boundary)
            .and_then(|n| n.reference.as_ref())
            .ok_or("Invalid graph reference path")?;
        def = definition(graph, reference.definition)?;
    }
    let node = def
        .nodes
        .iter()
        .find(|n| n.id == path.node)
        .ok_or("Unknown graph property node")?;
    Ok((def, node))
}
fn root(
    graph: &source::Document,
    value: &source::Preset,
    selectors: &mut Vec<Selector>,
) -> Result<ComponentInstance> {
    let shell = graph
        .instances
        .iter()
        .find(|i| i.id == graph.presented_root)
        .ok_or("Missing graph shell")?;
    let shell_def = definition(graph, preset(graph, shell.preset)?.definition)?;
    Ok(ComponentInstance {
        id: InstanceId(value.id),
        name: value.name.clone(),
        definition: value.definition,
        owner: Owner::Document,
        visual_parent: Some(ObjectAddress {
            instance: graph.presented_root,
            references: vec![],
            node: shell_def.root,
        }),
        order: graph
            .presets
            .iter()
            .position(|p| p.id == value.id)
            .unwrap_or(0) as u16,
        assignments: value
            .assignments
            .iter()
            .map(|a| assignment(a, selectors))
            .collect::<Result<_>>()?,
    })
}
struct Lowering {
    seed: Seed,
    selectors: Vec<Selector>,
    handlers: Vec<Handler>,
    previews: Vec<TemplateNode>,
}
fn build(input: &Seed, node_base: u16, control_base: u8) -> Result<Lowering> {
    let _profile = crate::canvas::profiling::span("oib.graph.lower");
    let graph = super::imports::resolve(input)?;
    validate_source(&graph)?;
    let mut selectors = vec![];
    let mut metadata = Document {
        id: graph.id,
        name: graph.name.clone(),
        class: graph.class,
        presented_root: graph.presented_root,
        hierarchy_viewport: graph.hierarchy_viewport,
        classes: graph.classes.clone(),
        definitions: graph.definitions.clone(),
        instances: vec![],
        mappings: Mappings {
            objects: vec![],
            definitions: vec![],
            roots: vec![],
        },
        placements: vec![],
        connections: vec![],
    };
    document::validate_declarations(&metadata.classes, &metadata.definitions)?;
    for value in graph.presets.iter().filter(|p| p.palette) {
        metadata
            .instances
            .push(root(&graph, value, &mut selectors)?);
    }
    for value in &graph.instances {
        let p = preset(&graph, value.preset)?;
        if !p.palette {
            metadata.instances.push(ComponentInstance {
                id: value.id,
                name: value.name.clone(),
                definition: p.definition,
                owner: value.owner,
                visual_parent: value.visual_parent.clone(),
                order: value.order,
                assignments: p
                    .assignments
                    .iter()
                    .map(|a| assignment(a, &mut selectors))
                    .collect::<Result<_>>()?,
            });
        }
    }
    metadata.instances.sort_by_key(|i| i.id);
    for grant in &graph.grants {
        let projection = match grant.operation {
            source::ActionOperation::Set => Projection::Identity,
            source::ActionOperation::Toggle => Projection::Nonzero,
        };
        intern(&mut selectors, grant.field, projection)?;
    }
    let mut cells = vec![];
    for def in &graph.definitions {
        for node in &def.nodes {
            for descriptor in document::node_descriptors(&metadata, def, node)? {
                if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly
                {
                    continue;
                }
                let default = node
                    .defaults
                    .iter()
                    .find(|d| d.property == descriptor.id)
                    .map_or_else(|| descriptor.default.clone(), |d| d.value.clone());
                let id = cells.len() as u64 + 1;
                metadata.mappings.definitions.push(DefinitionCellMapping {
                    definition: def.id,
                    node: node.id,
                    property: descriptor.id,
                    cell: id,
                });
                cells.push(Cell {
                    id,
                    owner: def.id.0,
                    name: descriptor.name,
                    default: default.clone(),
                    authored_override: None,
                    choices: vec![default],
                    inherit: None,
                    targets: vec![],
                });
            }
        }
    }
    metadata
        .mappings
        .definitions
        .sort_by_key(|m| (m.definition, m.node, m.property));
    let mut seed = Seed {
        format: FORMAT.into(),
        graph: input.graph.clone(),
        resolved_libraries: input.resolved_libraries.clone(),
        expanded: false,
        designer: Some(metadata),
        main_oib: graph.id.0,
        main_class: "otrios.declarative-v1".into(),
        components: graph
            .definitions
            .iter()
            .map(|d| Object {
                id: d.id.0,
                name: d.name.clone(),
                component: None,
            })
            .collect(),
        instances: graph
            .presets
            .iter()
            .filter(|p| p.palette)
            .map(|p| Object {
                id: p.id,
                name: p.name.clone(),
                component: Some(p.definition.0),
            })
            .collect(),
        cells,
        inspector_viewport: graph.inspector_viewport,
        inspector_content: graph.inspector_content,
        instance_surface: None,
    };
    for p in graph.presets.iter().filter(|p| p.palette) {
        let mut rows = vec![];
        for path in paths(&graph, p.definition)? {
            let (def, node) = terminal(&graph, p.definition, &path)?;
            let descriptor =
                document::node_descriptors(seed.designer.as_ref().unwrap(), def, node)?
                    .into_iter()
                    .find(|d| d.id == path.property)
                    .ok_or("Unknown graph property")?;
            if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
                continue;
            }
            let inherited = seed
                .designer
                .as_ref()
                .unwrap()
                .mappings
                .definitions
                .iter()
                .find(|m| {
                    m.definition == def.id && m.node == node.id && m.property == path.property
                })
                .ok_or("Missing graph shared property")?
                .cell;
            let base = seed
                .cells
                .iter()
                .find(|c| c.id == inherited)
                .ok_or("Missing graph property fallback")?
                .clone();
            let id = seed.cells.len() as u64 + 1;
            rows.push(RootCellKey {
                path: path.clone(),
                initial_cell: id,
            });
            seed.cells.push(Cell {
                id,
                owner: p.id,
                name: descriptor.name,
                default: base.default,
                authored_override: None,
                choices: base.choices,
                inherit: Some(inherited),
                targets: vec![],
            });
        }
        seed.designer
            .as_mut()
            .unwrap()
            .mappings
            .roots
            .push(RootCellMapping {
                prototype: InstanceId(p.id),
                cells: rows,
            });
    }
    if seed.cells.len() > MAX_CELLS {
        return Err(error("Graph initial property budget"));
    }
    // Resolve authored literals after every occurrence table exists.
    for p in graph.presets.iter().filter(|p| p.palette) {
        let rows = seed
            .designer
            .as_ref()
            .unwrap()
            .mappings
            .roots
            .iter()
            .find(|t| t.prototype.0 == p.id)
            .unwrap()
            .cells
            .clone();
        for row in rows {
            let authored = document::graph_authored_literal(&seed, InstanceId(p.id), &row.path)?;
            seed.cells
                .iter_mut()
                .find(|c| c.id == row.initial_cell)
                .unwrap()
                .authored_override = authored;
        }
    }
    let mut compiler = Compiler {
        graph: &graph,
        seed,
        selectors,
        handlers: vec![],
        values: BTreeMap::new(),
        next_node: node_base,
        next_control: control_base,
    };
    let mut previews = compiler.fixed_roots()?;
    let mut prototypes = vec![];
    for p in graph.presets.iter().filter(|p| p.palette) {
        let start = compiler.next_node;
        let mut preview = compiler
            .node(
                p,
                p.definition,
                &[],
                definition(&graph, p.definition)?.root,
                0,
            )?
            .ok_or("Graph palette root must be visual")?;
        if let Some(frame) = p.frame {
            preview.frame = frame.map(literal);
        }
        let nodes = (start..compiler.next_node).collect::<Vec<_>>();
        prototypes.push(Prototype {
            instance: p.id,
            palette_only: !graph.instances.iter().any(|i| i.preset == p.id),
            preview_root: preview.id,
            inspector_root: None,
            nodes,
        });
        previews.push(preview);
    }
    compiler
        .seed
        .designer
        .as_mut()
        .unwrap()
        .mappings
        .objects
        .sort_by(|a, b| a.address.cmp(&b.address));
    compiler.seed.instance_surface = Some(InstanceSurface {
        layout: None,
        preview_viewport: graph.preview_viewport,
        preview_stride: graph.preview_stride,
        inspector_start: 1.,
        inspector_stride: 1.,
        node_base: compiler.next_node,
        node_stride: prototypes
            .iter()
            .map(|p| p.nodes.len() as u16)
            .max()
            .unwrap_or(0),
        prototypes,
    });
    // Validate every admitted source path, including endpoints with no current
    // visual consumer. Lowering reachable geometry alone cannot prove that an
    // unused incoming forwarding route survived a structural removal.
    document::validate(&compiler.seed)?;
    Ok(Lowering {
        seed: compiler.seed,
        selectors: compiler.selectors,
        handlers: compiler.handlers,
        previews,
    })
}
struct Compiler<'a> {
    graph: &'a source::Document,
    seed: Seed,
    selectors: Vec<Selector>,
    handlers: Vec<Handler>,
    values: BTreeMap<u64, Option<Value>>,
    next_node: u16,
    next_control: u8,
}
#[derive(Clone)]
struct ParameterValue {
    expression: Expr,
    cell: Option<u64>,
    scale: f32,
    offset: f32,
}
impl Compiler<'_> {
    fn fixed_roots(&mut self) -> Result<Vec<TemplateNode>> {
        let mut out = vec![];
        for initial in &self.graph.instances {
            let mut p = preset(self.graph, initial.preset)?.clone();
            if p.palette {
                continue;
            }
            p.id = initial.id.0;
            p.name = initial.name.clone();
            let def = definition(self.graph, p.definition)?.clone();
            if initial.id == self.graph.presented_root {
                let root = def
                    .nodes
                    .iter()
                    .find(|n| n.id == def.root)
                    .ok_or("Missing graph shell root")?;
                if root.visual.is_some() || root.reference.is_some() {
                    return Err(error("The presented shell attaches to the tooling canvas"));
                }
                self.seed
                    .designer
                    .as_mut()
                    .unwrap()
                    .mappings
                    .objects
                    .push(ObjectMapping {
                        address: ObjectAddress {
                            instance: initial.id,
                            references: vec![],
                            node: def.root,
                        },
                        layout_node: Some(self.graph.preview_viewport),
                    });
                for child in &root.visual_children {
                    if let Some(child) = self.node(&p, def.id, &[], *child, 0)? {
                        out.push(child);
                    }
                }
                for child in def.nodes.iter().filter(|n| {
                    n.owner == Some(def.root)
                        && !def
                            .nodes
                            .iter()
                            .any(|parent| parent.visual_children.contains(&n.id))
                }) {
                    if self.node(&p, def.id, &[], child.id, 0)?.is_some() {
                        return Err(error("Shell visual object is absent from paint order"));
                    }
                }
            } else if let Some(mut root) = self.node(&p, def.id, &[], def.root, 0)? {
                if let Some(frame) = p.frame {
                    root.frame = frame.map(literal);
                }
                out.push(root);
            }
        }
        Ok(out)
    }
    fn parameter(
        &mut self,
        p: &source::Preset,
        prefix: &[NodeId],
        value: &source::Parameter,
    ) -> Result<ParameterValue> {
        match &value.source {
            source::ParameterSource::InstanceName {
                prefix,
                suffix,
                lowercase,
            } => {
                if value.projection != Projection::Identity
                    || prefix.len() + suffix.len() + p.name.len() > 96
                {
                    return Err(error("Invalid graph instance-name parameter"));
                }
                let name = if *lowercase {
                    p.name.to_lowercase()
                } else {
                    p.name.clone()
                };
                let text = format!("{prefix}{name}{suffix}");
                if text.len() > 96 {
                    return Err(error("Graph instance-name caption budget"));
                }
                Ok(ParameterValue {
                    expression: Expr::Literal(Value::Text(text)),
                    cell: None,
                    scale: 1.,
                    offset: 0.,
                })
            }
            source::ParameterSource::Property(path) => {
                let mut full = path.clone();
                let mut references = prefix.to_vec();
                references.extend(&path.references);
                full.references = references;
                let (cell, literal, binding) =
                    document::graph_property(&self.seed, InstanceId(p.id), &full, &self.values)?;
                if let Some(binding) = binding {
                    let Binding::Local(local) = binding;
                    let original = self
                        .selectors
                        .iter()
                        .find(|s| s.local == local)
                        .ok_or("Missing graph source selector")?
                        .clone();
                    let binding = if value.projection == Projection::Identity {
                        binding
                    } else {
                        if original.projection != Projection::Identity {
                            return Err(error(
                                "Graph projection cannot compose a projected app source",
                            ));
                        }
                        intern(
                            &mut self.selectors,
                            original.field,
                            value.projection.clone(),
                        )?
                    };
                    Ok(ParameterValue {
                        expression: Expr::State(binding),
                        cell: None,
                        scale: 1.,
                        offset: 0.,
                    })
                } else {
                    let original = literal.ok_or("Missing graph literal source")?;
                    let (projected, scale, offset) = project(original, &value.projection)?;
                    if cell.is_some()
                        && !matches!(
                            value.projection,
                            Projection::Identity | Projection::Number { .. }
                        )
                    {
                        return Err(error(
                            "Mutable literal graph parameters need an identity or numeric projection",
                        ));
                    }
                    Ok(ParameterValue {
                        expression: Expr::Literal(projected),
                        cell,
                        scale,
                        offset,
                    })
                }
            }
        }
    }
    fn node(
        &mut self,
        p: &source::Preset,
        def_id: DefinitionId,
        prefix: &[NodeId],
        node_id: NodeId,
        depth: usize,
    ) -> Result<Option<TemplateNode>> {
        if depth > 12 || self.next_node >= 1000 {
            return Err(error("Graph layout depth/node budget"));
        }
        let def = definition(self.graph, def_id)?;
        let node = def
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or("Missing graph node")?
            .clone();
        let address = ObjectAddress {
            instance: InstanceId(p.id),
            references: prefix.to_vec(),
            node: node.id,
        };
        let id = if node.visual.is_some() {
            let id = self.next_node;
            self.next_node = self
                .next_node
                .checked_add(1)
                .ok_or("Graph node IDs exhausted")?;
            Some(id)
        } else {
            None
        };
        self.seed
            .designer
            .as_mut()
            .unwrap()
            .mappings
            .objects
            .push(ObjectMapping {
                address,
                layout_node: id,
            });
        let mut children = vec![];
        if let Some(reference) = &node.reference {
            let mut inner = prefix.to_vec();
            inner.push(node.id);
            if let Some(mut root) = self.node(
                p,
                reference.definition,
                &inner,
                definition(self.graph, reference.definition)?.root,
                depth + 1,
            )? {
                // A reference owns the outer allocation; the expanded root keeps
                // intrinsic metrics but presents inside that allocated border box.
                root.reference_root = node.visual.as_ref().is_some_and(|v| v.layout.is_some());
                children.push(root);
            }
        }
        for child in &node.visual_children {
            if let Some(child) = self.node(p, def_id, prefix, *child, depth + 1)? {
                children.push(child);
            }
        }
        for child in def.nodes.iter().filter(|n| {
            n.owner == Some(node.id)
                && !def
                    .nodes
                    .iter()
                    .any(|parent| parent.visual_children.contains(&n.id))
        }) {
            if self.node(p, def_id, prefix, child.id, depth + 1)?.is_some() {
                return Err(error(
                    "Visual owned child is missing from graph paint order",
                ));
            }
        }
        let Some(mut visual) = node.visual else {
            if !children.is_empty() {
                return Err(error("Nonvisual graph node has visual content"));
            }
            return Ok(None);
        };
        let id = id.unwrap();
        let mut parameters = BTreeMap::new();
        for parameter in &visual.parameters {
            parameters.insert(parameter.local, self.parameter(p, prefix, parameter)?);
        }
        let mut outputs = vec![];
        for (index, expression) in visual.frame.iter_mut().enumerate() {
            let property = [
                Property::X,
                Property::Y,
                Property::FrameWidth,
                Property::Height,
            ][index];
            output(expression, property, &parameters, &mut outputs)?;
        }
        output(
            &mut visual.visible,
            Property::Visible,
            &parameters,
            &mut outputs,
        )?;
        match &mut visual.shape {
            Shape::Canvas { .. } => {}
            Shape::Text {
                value,
                scale,
                color,
            } => {
                output(value, Property::Text, &parameters, &mut outputs)?;
                replace(scale, &parameters)?;
                output(color, Property::Color, &parameters, &mut outputs)?;
            }
            Shape::Superellipse { radius, color, .. } => {
                replace(radius, &parameters)?;
                output(color, Property::Color, &parameters, &mut outputs)?;
            }
            Shape::Circle { color } => output(color, Property::Color, &parameters, &mut outputs)?,
            _ => return Err(error("Unsupported graph shape")),
        }
        if let Some(semantics) = &mut visual.semantics {
            output(
                &mut semantics.label,
                Property::SemanticLabel,
                &parameters,
                &mut outputs,
            )?;
            replace(&mut semantics.checked, &parameters)?;
        }
        let responder = if let Some(action) = &visual.action {
            let mut path = action.source.clone();
            let mut references = prefix.to_vec();
            references.extend(&path.references);
            path.references = references;
            let descriptor = document::graph_descriptor(&self.seed, InstanceId(p.id), &path)?;
            let class = self
                .graph
                .classes
                .iter()
                .find(|c| c.id == node.class)
                .ok_or("Unknown graph action class")?
                .implementation;
            let compatible = matches!(
                (action.kind, action.operation, class, descriptor.value_type),
                (
                    GestureKind::Slider,
                    source::ActionOperation::Set,
                    ClassKind::O2DSlider,
                    otrios_interfaces::layout::ValueType::Number,
                ) | (
                    GestureKind::Activate,
                    source::ActionOperation::Toggle,
                    ClassKind::O2DButton | ClassKind::O2DToggle,
                    otrios_interfaces::layout::ValueType::Boolean,
                )
            );
            if !compatible || !descriptor.policy.app_binding {
                return Err(error(
                    "Graph action does not match its admitted class and source type",
                ));
            }
            if let Some(binding) =
                document::graph_property(&self.seed, InstanceId(p.id), &path, &BTreeMap::new())?.2
            {
                let Binding::Local(local) = binding;
                let field = self
                    .selectors
                    .iter()
                    .find(|s| s.local == local)
                    .ok_or("Missing action app selector")?
                    .field;
                if !self.graph.grants.iter().any(|g| {
                    g.field == field && g.kind == action.kind && g.operation == action.operation
                }) {
                    return Err(error("Graph action exceeds signed field/effect grant"));
                }
                let control = self.next_control;
                self.next_control = self
                    .next_control
                    .checked_add(1)
                    .filter(|c| *c <= 128)
                    .ok_or("Graph control budget")?;
                let operation = match action.operation {
                    source::ActionOperation::Set => Operation::Set,
                    source::ActionOperation::Toggle => Operation::Toggle,
                };
                let valid = matches!(
                    (action.kind, action.operation),
                    (GestureKind::Slider, source::ActionOperation::Set)
                        | (GestureKind::Activate, source::ActionOperation::Toggle)
                );
                if !valid {
                    return Err(error("Unsupported graph control/effect pair"));
                }
                self.handlers.push(Handler {
                    control,
                    event: u16::from(control),
                    kind: action.kind,
                    operation,
                    fields: vec![field],
                });
                Some(Responder::AppEvent {
                    control,
                    drag: action.kind == GestureKind::Slider,
                })
            } else {
                None
            }
        } else {
            None
        };
        if visual.action.is_some() && responder.is_none() {
            visual.semantics = None;
            outputs.retain(|(_, property, _, _)| *property != Property::SemanticLabel);
        }
        for (cell, property, scale, offset) in outputs {
            self.seed
                .cells
                .iter_mut()
                .find(|c| c.id == cell)
                .ok_or("Missing graph output property")?
                .targets
                .push(Target {
                    node: id,
                    property,
                    scale,
                    offset,
                });
        }
        Ok(Some(TemplateNode {
            reference_root: false,
            layout: visual.layout,
            id,
            resize: visual.resize,
            frame: visual.frame,
            visible: visual.visible,
            shape: visual.shape,
            responder,
            semantics: visual.semantics,
            children,
        }))
    }
}
fn project(value: Value, projection: &Projection) -> Result<(Value, f32, f32)> {
    match (projection, value) {
        (Projection::Identity, value) => Ok((value, 1., 0.)),
        (Projection::Number { scale, offset }, Value::Number(n))
            if scale.is_finite() && offset.is_finite() =>
        {
            Ok((Value::Number(n * scale + offset), *scale, *offset))
        }
        (Projection::Percent, Value::Number(n)) => {
            Ok((Value::Text(format!("{:.0}%", n / 10.)), 1., 0.))
        }
        (Projection::Nonzero, Value::Number(n)) => Ok((Value::Boolean(n != 0.), 1., 0.)),
        (Projection::ChooseText { zero, nonzero }, Value::Number(n)) => Ok((
            Value::Text(if n == 0. {
                zero.clone()
            } else {
                nonzero.clone()
            }),
            1.,
            0.,
        )),
        (Projection::ChooseColor { zero, nonzero }, Value::Number(n)) => {
            Ok((Value::Color(if n == 0. { *zero } else { *nonzero }), 1., 0.))
        }
        _ => Err(error("Invalid graph parameter projection/type")),
    }
}
fn replace(expression: &mut Expr, parameters: &BTreeMap<u16, ParameterValue>) -> Result<()> {
    match expression {
        Expr::Parameter(local) => {
            *expression = parameters
                .get(local)
                .ok_or("Unknown graph visual parameter")?
                .expression
                .clone()
        }
        Expr::State(_) => {
            return Err(error(
                "Graph visuals use typed property parameters, not renderer selectors",
            ));
        }
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            replace(a, parameters)?;
            replace(b, parameters)?;
        }
        Expr::Select { condition, yes, no } => {
            replace(condition, parameters)?;
            replace(yes, parameters)?;
            replace(no, parameters)?;
        }
        _ => {}
    }
    Ok(())
}
fn affine(expression: &Expr) -> Option<(u16, f32, f32)> {
    match expression {
        Expr::Parameter(local) => Some((*local, 1., 0.)),
        Expr::Add(a, b) => match (&**a, &**b) {
            (_, Expr::Literal(Value::Number(n))) => affine(a).map(|(p, s, o)| (p, s, o + n)),
            (Expr::Literal(Value::Number(n)), _) => affine(b).map(|(p, s, o)| (p, s, o + n)),
            _ => None,
        },
        Expr::Subtract(a, b) => match &**b {
            Expr::Literal(Value::Number(n)) => affine(a).map(|(p, s, o)| (p, s, o - n)),
            _ => None,
        },
        Expr::Multiply(a, b) => match (&**a, &**b) {
            (_, Expr::Literal(Value::Number(n))) => affine(a).map(|(p, s, o)| (p, s * n, o * n)),
            (Expr::Literal(Value::Number(n)), _) => affine(b).map(|(p, s, o)| (p, s * n, o * n)),
            _ => None,
        },
        _ => None,
    }
}
fn output(
    expression: &mut Expr,
    property: Property,
    parameters: &BTreeMap<u16, ParameterValue>,
    out: &mut Vec<(u64, Property, f32, f32)>,
) -> Result<()> {
    if let Some((parameter, scale, offset)) = affine(expression)
        && let Some(value) = parameters.get(&parameter)
        && let Some(cell) = value.cell
    {
        out.push((
            cell,
            property,
            value.scale * scale,
            value.offset * scale + offset,
        ));
    }
    replace(expression, parameters)
}
fn collect_nodes(root: &TemplateNode, ids: &mut BTreeSet<u16>, controls: &mut BTreeSet<u8>) {
    ids.insert(root.id);
    if let Some(Responder::AppEvent { control, .. }) = root.responder {
        controls.insert(control);
    }
    for child in &root.children {
        collect_nodes(child, ids, controls);
    }
}
/// Remove only compiler-owned application output. Signed chrome remains intact.
pub fn strip(ui: &Ui) -> Result<Ui> {
    let _profile = crate::canvas::profiling::span("oib.graph.strip");
    if super::seed(ui).is_none() {
        return Ok(ui.clone());
    }
    strip_prepared(PreparedUi::from_ui(ui)?)?.finish()
}
fn strip_prepared(mut prepared: PreparedUi) -> Result<PreparedUi> {
    let result = &mut prepared.ui;
    let seed = super::seed(result).ok_or("Missing OIB seed")?;
    if seed.graph.is_none() {
        return Err(error("Canonical OIB graph is required"));
    }
    let blueprint = &mut prepared.blueprint;
    let graph = seed.graph.as_ref().unwrap();
    let mut controls = BTreeSet::new();
    for host in [graph.preview_viewport, graph.inspector_viewport] {
        let viewport = blueprint
            .templates
            .iter_mut()
            .find_map(|t| super::node(&mut t.root, host))
            .ok_or("Missing graph tooling viewport")?;
        for child in &viewport.children {
            collect_nodes(child, &mut BTreeSet::new(), &mut controls);
        }
        viewport.children.clear();
    }
    fn stateful_expr(e: &Expr) -> bool {
        match e {
            Expr::State(_) => true,
            Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
                stateful_expr(a) || stateful_expr(b)
            }
            Expr::Select { condition, yes, no } => {
                stateful_expr(condition) || stateful_expr(yes) || stateful_expr(no)
            }
            _ => false,
        }
    }
    fn stateful(node: &TemplateNode) -> bool {
        node.frame.iter().any(stateful_expr)
            || stateful_expr(&node.visible)
            || node
                .semantics
                .as_ref()
                .is_some_and(|s| stateful_expr(&s.label) || stateful_expr(&s.checked))
            || match &node.shape {
                Shape::Text {
                    value,
                    scale,
                    color,
                } => stateful_expr(value) || stateful_expr(scale) || stateful_expr(color),
                Shape::Circle { color } => stateful_expr(color),
                Shape::Superellipse { radius, color, .. } => {
                    stateful_expr(radius) || stateful_expr(color)
                }
                _ => false,
            }
            || node.children.iter().any(stateful)
    }
    if blueprint.templates.iter().any(|t| stateful(&t.root)) {
        return Err(error(
            "OIB tooling chrome must not depend on app-state selectors",
        ));
    }
    result.handlers.retain(|h| !controls.contains(&h.control));
    // OIB chrome is intentionally independent of app values. Every app selector
    // is derived from graph bindings; hidden inspector placeholders are absent.
    result.selectors.clear();
    Ok(prepared)
}

/// Reconstruct the disposable property and object tables during checkpoint load.
pub fn materialize_seed(seed: &Seed) -> Result<Seed> {
    let _profile = crate::canvas::profiling::span("oib.graph.materialize_seed");
    let graph = seed
        .graph
        .as_ref()
        .ok_or("Canonical OIB graph is required")?;
    let base = graph
        .preview_viewport
        .max(graph.inspector_viewport)
        .max(graph.hierarchy_viewport.unwrap_or(0))
        .checked_add(1)
        .ok_or("Graph chrome node bound")?;
    Ok(build(seed, base, 1)?.seed)
}
pub fn with_seed_source(
    seed: &Seed,
    definitions: &[ComponentDefinition],
    presets: &[source::Preset],
) -> Result<Seed> {
    super::imports::validate_local_definitions(seed, definitions)?;
    let mut candidate = seed.clone();
    let graph = candidate
        .graph
        .as_mut()
        .ok_or("Canonical OIB graph is required")?;
    graph.definitions = definitions.to_vec();
    graph.presets = presets.to_vec();
    graph
        .instances
        .retain(|i| i.id == graph.presented_root || graph.presets.iter().any(|p| p.id == i.preset));
    materialize_seed(&candidate)
}
pub fn with_source(
    ui: &Ui,
    definitions: &[ComponentDefinition],
    presets: &[source::Preset],
) -> Result<Ui> {
    with_source_prepared(PreparedUi::from_ui(ui)?, definitions, presets)?.finish()
}
pub(crate) fn with_source_prepared(
    prepared: PreparedUi,
    definitions: &[ComponentDefinition],
    presets: &[source::Preset],
) -> Result<PreparedUi> {
    super::imports::validate_local_definitions(
        super::seed(&prepared.ui).ok_or("Missing OIB graph")?,
        definitions,
    )?;
    let mut candidate = strip_prepared(prepared)?;
    let Some(Presentation::Oib(seed)) = &mut candidate.ui.presentation else {
        return Err(error("Missing OIB graph"));
    };
    let graph = seed
        .graph
        .as_mut()
        .ok_or("Canonical OIB graph is required")?;
    graph.definitions = definitions.to_vec();
    graph.presets = presets.to_vec();
    graph
        .instances
        .retain(|i| i.id == graph.presented_root || graph.presets.iter().any(|p| p.id == i.preset));
    materialize_prepared(candidate)
}
/// Compile a canonical signed graph into the existing bounded layout engine.
pub fn materialize(ui: &Ui) -> Result<Ui> {
    let _profile = crate::canvas::profiling::span("oib.graph.materialize");
    if super::seed(ui).is_none() {
        return Ok(ui.clone());
    }
    materialize_prepared(PreparedUi::from_ui(ui)?)?.finish()
}
fn materialize_prepared(prepared: PreparedUi) -> Result<PreparedUi> {
    let mut prepared = strip_prepared(prepared)?;
    let result = &mut prepared.ui;
    let blueprint = &mut prepared.blueprint;
    let seed = super::seed(result).ok_or("Missing OIB graph")?.clone();
    let mut nodes = BTreeSet::new();
    let mut controls = BTreeSet::new();
    for template in &blueprint.templates {
        collect_nodes(&template.root, &mut nodes, &mut controls);
    }
    let base = nodes
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Graph node bound")?;
    let control = controls
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Graph control bound")?;
    let lowering = build(&seed, base, control)?;
    let graph = seed.graph.as_ref().unwrap();
    let preview = blueprint
        .templates
        .iter_mut()
        .find_map(|t| super::node(&mut t.root, graph.preview_viewport))
        .ok_or("Missing graph preview viewport")?;
    if !matches!(preview.shape, Shape::Canvas { clip: true }) || !preview.children.is_empty() {
        return Err(error(
            "Graph preview chrome must be an empty clipped canvas",
        ));
    }
    preview.children = lowering.previews;
    fn has_auto(node: &TemplateNode) -> bool {
        node.layout.is_some() || node.children.iter().any(has_auto)
    }
    if blueprint.viewport.is_fit() && blueprint.templates.iter().any(|t| has_auto(&t.root)) {
        blueprint.schema = otrios_interfaces::layout::AUTO_LAYOUT_FORMAT.into();
    }

    result.selectors = lowering.selectors;
    result.handlers.extend(lowering.handlers);
    result.presentation = Some(Presentation::Oib(Box::new(lowering.seed)));
    Ok(prepared)
}

/// Compile the admitted graph directly for the current resident roots. Presets
/// are source records, not hidden live objects or reusable renderer identities.
pub fn expand(
    ui: &Ui,
    allocations: &[Allocation],
    live: &BTreeSet<u64>,
    _selected: Option<u64>,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<Ui> {
    expand_prepared(PreparedUi::from_ui(ui)?, allocations, live, values)?.finish()
}
pub(crate) fn expand_prepared(
    prepared: PreparedUi,
    allocations: &[Allocation],
    live: &BTreeSet<u64>,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<PreparedUi> {
    let _profile = crate::canvas::profiling::span("oib.graph.expand");
    if live.len() > MAX_LIVE_INSTANCES {
        return Err(error("OIB live instance capacity exhausted"));
    }
    let mut prepared = strip_prepared(prepared)?;
    let result = &mut prepared.ui;
    let blueprint = &mut prepared.blueprint;
    let original = super::seed(result).ok_or("Missing OIB graph")?;
    let graph = super::imports::resolve(original)?;
    let mut nodes = BTreeSet::new();
    let mut controls = BTreeSet::new();
    for template in &blueprint.templates {
        collect_nodes(&template.root, &mut nodes, &mut controls);
    }
    let base = nodes
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Graph node bound")?;
    let control = controls
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Graph control bound")?;
    let mut seed = super::instances::expanded_seed(original, allocations)?;
    for cell in &mut seed.cells {
        cell.targets.clear();
    }
    seed.designer
        .as_mut()
        .ok_or("Missing materialized graph metadata")?
        .mappings
        .objects
        .clear();
    let mut compiler = Compiler {
        graph: &graph,
        seed,
        selectors: source_selectors(original)?,
        handlers: vec![],
        values: values.clone(),
        next_node: base,
        next_control: control,
    };
    let mut previews = compiler.fixed_roots()?;
    let mut row = 0;
    for allocation in allocations {
        let mut p = preset(&graph, allocation.prototype)?.clone();
        p.id = allocation.id;
        p.name = allocation.name.clone();
        let before = compiler.handlers.len();
        let mut preview = compiler
            .node(
                &p,
                p.definition,
                &[],
                definition(&graph, p.definition)?.root,
                0,
            )?
            .ok_or("Graph root must be visual")?;
        if let Some(frame) = p.frame {
            preview.frame = frame.map(literal);
        }
        preview.frame[1] = literal(row as f32 * graph.preview_stride);
        if live.contains(&allocation.id) {
            previews.push(preview);
            row += 1;
        } else {
            compiler.handlers.truncate(before);
        }
    }
    let preview = blueprint
        .templates
        .iter_mut()
        .find_map(|t| super::node(&mut t.root, graph.preview_viewport))
        .ok_or("Missing graph preview")?;
    preview.children = previews;
    compiler
        .seed
        .designer
        .as_mut()
        .unwrap()
        .mappings
        .objects
        .sort_by(|a, b| a.address.cmp(&b.address));
    compiler.seed.instances.retain(|i| live.contains(&i.id));
    compiler
        .seed
        .cells
        .retain(|c| c.inherit.is_none() || live.contains(&c.owner));
    // A frame authored on this occurrence takes precedence over the linked
    // definition's frame parameters. Child parameters still inherit normally.
    let metadata = compiler.seed.designer.as_ref().unwrap();
    let overridden_roots: BTreeSet<_> = allocations
        .iter()
        .filter(|a| {
            preset(&graph, a.prototype).is_ok_and(|p| p.frame.is_some())
                || metadata
                    .placements
                    .iter()
                    .any(|p| p.instance.0 == a.id && p.frame.is_some())
        })
        .filter_map(|a| {
            let root = definition(&graph, DefinitionId(a.component)).ok()?.root;
            metadata
                .mappings
                .objects
                .iter()
                .find(|m| {
                    m.address
                        == ObjectAddress {
                            instance: InstanceId(a.id),
                            references: vec![],
                            node: root,
                        }
                })?
                .layout_node
        })
        .collect();
    for cell in &mut compiler.seed.cells {
        cell.targets.retain(|t| {
            !overridden_roots.contains(&t.node)
                || !matches!(
                    t.property,
                    Property::X
                        | Property::Y
                        | Property::Width
                        | Property::FrameWidth
                        | Property::Height
                        | Property::OffsetY
                )
        });
    }
    result.selectors = compiler.selectors;
    result.handlers.extend(compiler.handlers);
    result.presentation = Some(Presentation::Oib(Box::new(compiler.seed)));
    super::document::placement::project_blueprint(result, blueprint, allocations, live)?;
    Ok(prepared)
}

#[cfg(test)]
#[path = "graph_auto_layout_tests.rs"]
mod auto_layout_tests;
