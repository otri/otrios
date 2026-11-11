// SPDX-License-Identifier: LGPL-3.0-only
//! A definition's disposable sample surface. This never edits a live allocation
//! or imports business data; it reuses canonical lowering and verified libraries.
use super::*;
use otrios_interfaces::{
    canvas::{Field, StateSchema},
    layout::{AxisSizing, Instance as LayoutInstance, NodeLayout, Sizing, Template},
};

/// Virtual identities are bounded, private and never written to DocumentSource.
const VIRTUAL_BASE: u64 = 1_000_000;

#[derive(Clone)]
pub(crate) struct DefinitionPreview {
    pub(crate) ui: Ui,
    pub(crate) layout: Layout,
    pub(crate) definition: DefinitionId,
    pub(crate) instance: InstanceId,
    pub(crate) objects: Vec<ObjectMapping>,
}
impl DefinitionPreview {
    /// A transportless sample document also supplies literal guest contexts.
    /// Its OIB seed is this disposable preview, never the live app document.
    pub(crate) fn sample_adapter(&self) -> Result<crate::canvas::StateAdapter> {
        Ok(crate::canvas::StateAdapter::for_ui(
            &self.ui,
            &format!(
                "designer-definition:{}:{}",
                self.definition.0, self.instance.0
            ),
            &[],
            [0xd5; 16],
        )?
        .preview_clone())
    }
    /// Imported descendants select their local reference boundary. Editing that
    /// boundary does not pretend the imported implementation is locally owned.
    pub(crate) fn selection_for_node(&self, renderer_node: u32) -> Option<Selection> {
        let mapping = self
            .objects
            .iter()
            .find(|mapping| mapping.layout_node.map(u32::from) == Some(renderer_node))?;
        Some(Selection::DefinitionNode {
            definition: self.definition,
            node: mapping
                .address
                .references
                .first()
                .copied()
                .unwrap_or(mapping.address.node),
        })
    }
}
fn closure(
    graph: &source::Document,
    id: DefinitionId,
    found: &mut BTreeSet<DefinitionId>,
) -> Result<()> {
    if found.insert(id) {
        if found.len() > MAX_DEFINITIONS {
            return Err("Preview definition closure exceeds budget".into());
        }
        for node in &definition(graph, id)?.nodes {
            if let Some(reference) = &node.reference {
                closure(graph, reference.definition, found)?;
            }
        }
    }
    Ok(())
}
fn path_with_prefix(prefix: &[NodeId], path: &PropertyPath) -> PropertyPath {
    let mut references = prefix.to_vec();
    references.extend_from_slice(&path.references);
    PropertyPath {
        references,
        node: path.node,
        property: path.property,
    }
}
/// Preserve authored forwarding aliases instead of giving an exposed input and
/// its implementation target unrelated sample registers.
fn forwarded_targets(
    graph: &source::Document,
    id: DefinitionId,
    prefix: &mut Vec<NodeId>,
    targets: &mut BTreeSet<PropertyPath>,
) -> Result<()> {
    if prefix.len() > 8 {
        return Err("Preview reference depth budget".into());
    }
    let definition = definition(graph, id)?;
    for exposed in &definition.exposed {
        for target in &exposed.targets {
            targets.insert(path_with_prefix(prefix, target));
        }
    }
    for node in &definition.nodes {
        if let Some(reference) = &node.reference {
            prefix.push(node.id);
            for assignment in &reference.assignments {
                if matches!(assignment.source, AssignmentSource::Forward(_)) {
                    targets.insert(path_with_prefix(prefix, &assignment.target));
                }
            }
            forwarded_targets(graph, reference.definition, prefix, targets)?;
            prefix.pop();
        }
    }
    Ok(())
}
fn canvas_node(id: u16, frame: [f32; 4], children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode {
        id,
        layout: None,
        reference_root: false,
        resize: None,
        frame: frame.map(literal),
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: true },
        responder: None,
        semantics: None,
        children,
    }
}
/// Compile exactly one selected definition against synthetic local sample fields.
/// The supplied UI/source remain byte-identical, including unused definitions,
/// imports, fixed application objects and every real preset/instance assignment.
pub(crate) fn definition_preview(
    ui: &Ui,
    selected: DefinitionId,
    viewport: [f32; 2],
) -> Result<DefinitionPreview> {
    compile_preview(ui, selected, viewport, &BTreeMap::new())
}

/// Semantic definition overrides are projected as private occurrence defaults.
/// Authored literal/forward connections keep their normal precedence; immutable
/// imported library bodies are never modified to display a working copy.
pub(crate) fn definition_preview_with_values(
    ui: &Ui,
    selected: DefinitionId,
    viewport: [f32; 2],
    values: &BTreeMap<PropertyAddress, Option<Value>>,
) -> Result<DefinitionPreview> {
    if values.is_empty() {
        return super::definition_preview(ui, selected, viewport);
    }
    compile_preview(ui, selected, viewport, values)
}
fn compile_preview(
    ui: &Ui,
    selected: DefinitionId,
    viewport: [f32; 2],
    values: &BTreeMap<PropertyAddress, Option<Value>>,
) -> Result<DefinitionPreview> {
    if viewport
        .iter()
        .any(|value| !value.is_finite() || !(48. ..=8192.).contains(value))
    {
        return Err("Invalid definition preview viewport".into());
    }
    let input = super::super::seed(ui).ok_or("Definition preview requires OIB source")?;
    let resolved = super::super::imports::resolve(input)?;
    validate_source(&resolved)?;
    let selected_definition = definition(&resolved, selected)?;
    if selected_definition
        .nodes
        .iter()
        .find(|node| node.id == selected_definition.root)
        .and_then(|node| node.visual.as_ref())
        .is_none()
    {
        return Err("A nonvisual definition has no component preview".into());
    }
    let shell = resolved
        .instances
        .iter()
        .find(|instance| instance.id == resolved.presented_root)
        .ok_or("Missing preview shell")?
        .clone();
    let shell_preset = preset(&resolved, shell.preset)?.clone();
    if shell_preset.definition == selected {
        return Err("The document shell is not a component preview".into());
    }
    let virtual_id = (VIRTUAL_BASE..VIRTUAL_BASE + 64)
        .find(|candidate| {
            resolved.id.0 != *candidate
                && resolved
                    .definitions
                    .iter()
                    .all(|definition| definition.id.0 != *candidate)
                && resolved
                    .instances
                    .iter()
                    .all(|instance| instance.id.0 != *candidate)
                && resolved
                    .presets
                    .iter()
                    .all(|preset| preset.id != *candidate)
        })
        .ok_or("Definition preview virtual identity budget")?;
    let mut needed = BTreeSet::new();
    closure(&resolved, selected, &mut needed)?;
    needed.insert(shell_preset.definition);
    let mut private = input.clone();
    let graph = private
        .graph
        .as_mut()
        .ok_or("Missing canonical preview graph")?;
    graph
        .definitions
        .retain(|definition| needed.contains(&definition.id));
    let private_shell = graph
        .definitions
        .iter_mut()
        .find(|definition| definition.id == shell_preset.definition)
        .ok_or("Preview shell must be locally owned")?;
    let mut root = private_shell
        .nodes
        .iter()
        .find(|node| node.id == private_shell.root)
        .cloned()
        .ok_or("Missing nonvisual preview shell root")?;
    if root.visual.is_some() || root.reference.is_some() {
        return Err("Preview shell must remain nonvisual".into());
    }
    root.visual_children.clear();
    private_shell.nodes = vec![root];
    private_shell.exposed.clear();
    private_shell.slots.clear();
    let shell_root = private_shell.root;
    let mut private_shell_preset = shell_preset.clone();
    private_shell_preset.assignments.clear();
    graph.presets = vec![
        private_shell_preset,
        source::Preset {
            id: virtual_id,
            name: selected_definition.name.clone(),
            definition: selected,
            assignments: vec![],
            frame: None,
            palette: true,
        },
    ];
    graph.presets.sort_by_key(|preset| preset.id);
    graph.instances = vec![
        shell.clone(),
        source::Instance {
            id: InstanceId(virtual_id),
            name: selected_definition.name.clone(),
            preset: virtual_id,
            owner: Owner::Document,
            visual_parent: Some(ObjectAddress {
                instance: shell.id,
                references: vec![],
                node: shell_root,
            }),
            order: 0,
        },
    ];
    graph.instances.sort_by_key(|instance| instance.id);
    graph.grants.clear();
    // First lower the same definition with only authored defaults. The canonical
    // resolver applies node defaults, reference literals and exposed forwarding.
    let private_graph = private
        .graph
        .as_ref()
        .ok_or("Missing private preview graph")?;
    let baseline_node_base = private_graph
        .preview_viewport
        .max(private_graph.inspector_viewport)
        .max(private_graph.hierarchy_viewport.unwrap_or(0))
        .checked_add(1)
        .ok_or("Preview tooling identity budget")?;
    let back_handlers: Vec<_> = ui
        .handlers
        .iter()
        .filter(|handler| handler.operation == Operation::Back)
        .cloned()
        .collect();
    let control_base = back_handlers
        .iter()
        .map(|handler| handler.control)
        .filter(|control| *control < 128)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Preview back/control identity budget")?;
    let baseline = build(&private, baseline_node_base, control_base)?;
    let effective = super::super::imports::resolve(&private)?;
    let mut forwards = BTreeSet::new();
    forwarded_targets(&effective, selected, &mut vec![], &mut forwards)?;
    let mut projected = BTreeMap::new();
    for path in paths(&effective, selected)? {
        let (definition, node) = terminal(&effective, selected, &path)?;
        let address = PropertyAddress::Definition {
            definition: definition.id,
            node: node.id,
            property: path.property,
        };
        let Some(Some(value)) = values.get(&address) else {
            continue;
        };
        let descriptor = document::node_descriptors(
            baseline
                .seed
                .designer
                .as_ref()
                .ok_or("Missing preview metadata")?,
            definition,
            node,
        )?
        .into_iter()
        .find(|descriptor| descriptor.id == path.property)
        .ok_or("Missing preview property descriptor")?;
        if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
            return Err("Definition preview override is not an editable property".into());
        }
        document::validate_property_value(&descriptor, value)?;
        if forwards.contains(&path)
            || document::graph_authored_literal(&baseline.seed, InstanceId(virtual_id), &path)?
                .is_some()
        {
            continue;
        }
        projected.insert(path, source::Source::Literal(value.clone()));
    }
    private
        .graph
        .as_mut()
        .ok_or("Missing private preview graph")?
        .presets
        .iter_mut()
        .find(|preset| preset.id == virtual_id)
        .ok_or("Missing preview preset")?
        .assignments = projected
        .iter()
        .map(|(target, source)| source::Assignment {
            target: target.clone(),
            source: source.clone(),
        })
        .collect();
    let baseline = build(&private, baseline_node_base, control_base)?;
    let mut fields = vec![];
    let mut assignments = projected;
    let mut grants = vec![];
    for path in paths(&effective, selected)? {
        let (definition, node) = terminal(&effective, selected, &path)?;
        let descriptor = document::node_descriptors(
            baseline
                .seed
                .designer
                .as_ref()
                .ok_or("Missing preview metadata")?,
            definition,
            node,
        )?
        .into_iter()
        .find(|descriptor| descriptor.id == path.property)
        .ok_or("Missing preview input descriptor")?;
        if !descriptor.policy.app_binding || forwards.contains(&path) {
            continue;
        }
        if fields.len() >= 64 {
            return Err("Definition preview sample field budget".into());
        }
        let default = document::graph_property(
            &baseline.seed,
            InstanceId(virtual_id),
            &path,
            &BTreeMap::new(),
        )?
        .1
        .ok_or("Preview default cannot read a business binding")?;
        let field = u16::try_from(fields.len() + 1).map_err(|_| "Preview field identity budget")?;
        let (sample_default, projection) = if let Value::Boolean(value) = default {
            (
                Value::Number(if value { 1000. } else { 0. }),
                Projection::Nonzero,
            )
        } else {
            (default.clone(), Projection::Identity)
        };
        fields.push(Field {
            id: field,
            name: format!("sample_{field}"),
            default: sample_default,
            writable: true,
        });
        assignments.insert(path, source::Source::AppBinding { field, projection });
        match default {
            Value::Number(_) => grants.push(source::ActionGrant {
                field,
                kind: GestureKind::Slider,
                operation: source::ActionOperation::Set,
            }),
            Value::Boolean(_) => grants.push(source::ActionGrant {
                field,
                kind: GestureKind::Activate,
                operation: source::ActionOperation::Toggle,
            }),
            _ => {}
        }
    }
    let graph = private.graph.as_mut().ok_or("Missing private graph")?;
    graph
        .presets
        .iter_mut()
        .find(|preset| preset.id == virtual_id)
        .ok_or("Missing preview preset")?
        .assignments = assignments
        .into_iter()
        .map(|(target, source)| source::Assignment { target, source })
        .collect();
    graph.grants = grants;
    let preview_id = graph.preview_viewport;
    let inspector_id = graph.inspector_viewport;
    let hierarchy_id = graph.hierarchy_viewport;
    let node_base = preview_id
        .max(inspector_id)
        .max(hierarchy_id.unwrap_or(0))
        .checked_add(1)
        .ok_or("Preview tooling identity budget")?;
    let lowering = build(&private, node_base, control_base)?;
    if lowering.previews.len() != 1 {
        return Err("Definition preview must contain one virtual root".into());
    }
    let objects = lowering
        .seed
        .designer
        .as_ref()
        .ok_or("Missing preview mappings")?
        .mappings
        .objects
        .iter()
        .filter(|mapping| mapping.address.instance == InstanceId(virtual_id))
        .cloned()
        .collect();
    let mut preview = canvas_node(
        preview_id,
        [0., 0., viewport[0], viewport[1]],
        lowering.previews,
    );
    preview.layout = Some(NodeLayout {
        width: AxisSizing {
            mode: Sizing::Fill,
            ..Default::default()
        },
        height: AxisSizing {
            mode: Sizing::Fill,
            ..Default::default()
        },
        ..Default::default()
    });
    let mut chrome = vec![
        preview,
        canvas_node(inspector_id, [-8000., -8000., 1., 1.], vec![]),
    ];
    if let Some(id) = hierarchy_id {
        chrome.push(canvas_node(id, [-8000., -8000., 1., 1.], vec![]));
    }
    let mut root = canvas_node(0, [0., 0., 1280., 800.], chrome);
    root.layout = Some(NodeLayout {
        width: AxisSizing {
            mode: Sizing::Fill,
            ..Default::default()
        },
        height: AxisSizing {
            mode: Sizing::Fill,
            ..Default::default()
        },
        ..Default::default()
    });
    let viewport_policy = Layout::from_bytes(&ui.layout)?.viewport_policy();
    let blueprint = Blueprint {
        schema: if viewport_policy.is_fit() {
            otrios_interfaces::layout::AUTO_LAYOUT_FORMAT
        } else {
            otrios_interfaces::layout::RESPONSIVE_VIEWPORT_FORMAT
        }
        .into(),
        viewport: viewport_policy,
        composition: None,
        templates: vec![Template {
            id: 0,
            name: "Definition sample surface".into(),
            parameters: vec![],
            root,
        }],
        root: LayoutInstance {
            id: 0,
            template: 0,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    };
    let mut result = ui.clone();
    result.title = selected_definition.name.clone();
    result.state_schema = StateSchema {
        id: "oib-definition-samples-v1".into(),
        fields,
    };
    super::super::super::validate_schema(&result.state_schema)?;
    result.layout = Layout::encode(&blueprint)?;
    result.selectors = lowering.selectors;
    result.handlers = back_handlers;
    result.handlers.extend(lowering.handlers);
    result.navigation.clear();
    result.behavior = None;
    result.presentation = Some(Presentation::Oib(Box::new(lowering.seed)));
    let layout = super::super::compile(&result, &BTreeMap::new())?;
    super::super::component_behaviors::validate(&result, &layout)?;
    let mut bindings = BTreeMap::new();
    for selector in &result.selectors {
        let value = result
            .state_schema
            .fields
            .iter()
            .find(|field| field.id == selector.field)
            .ok_or("Missing preview sample field")?
            .default
            .clone();
        let (projected, _, _) = project(value, &selector.projection)?;
        bindings.insert(selector.local, projected);
    }
    layout.resolve_ordered_with_font(
        &bindings,
        viewport,
        &[],
        Some(crate::dashboard::font::admission_font()?),
    )?;
    Ok(DefinitionPreview {
        ui: result,
        layout,
        definition: selected,
        instance: InstanceId(virtual_id),
        objects,
    })
}

#[cfg(test)]
#[path = "graph_preview/tests.rs"]
mod tests;
