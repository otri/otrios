// SPDX-License-Identifier: LGPL-3.0-only
//! Local named style/variable authoring over the ordinary private Apply path.
use super::*;
use otrios_interfaces::oib::{designer::*, styles as s};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    Components,
    Styles,
    Variables,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pick {
    Style(DefinitionId),
    Variable(s::StyleId, s::FieldId),
}
const PAGE: usize = 96; // 96 × 56 stays below the existing 8192-unit panel ceiling.
fn title(b: &mut Builder, y: f32, value: &str) -> Result<SceneNode> {
    let mut node = b.label(0., y, value, WHITE)?;
    node.frame.width = 320.;
    if let Primitive::Text { scale, .. } = &mut node.primitive {
        *scale = 2;
    }
    Ok(node)
}
pub(super) fn tabs(b: &mut Builder, section: Section) -> Result<Vec<SceneNode>> {
    let mut nodes = vec![];
    for (index, (value, current)) in [
        ("Components", Section::Components),
        ("Styles", Section::Styles),
        ("Variables", Section::Variables),
    ]
    .into_iter()
    .enumerate()
    {
        let x = index as f32 * 108.;
        let mut tab = b.focus_row(x, 0., 104., value, LocalAction::LibrarySection(current))?;
        style::set_surface_fill(&mut tab.children[0], b.palette.background)?;
        if let Primitive::Text { scale, .. } = &mut tab.children[1].primitive {
            *scale = 1;
        }
        tab.semantics.as_mut().unwrap().checked = section == current;
        if section == current {
            tab.children
                .push(background(b.id()?, 104., 2., b.palette.text, 0.));
            tab.children.last_mut().unwrap().frame.y = 48.;
            tab.children.last_mut().unwrap().responder = Some(TouchResponder::PassThrough);
        }
        nodes.push(tab);
    }
    Ok(nodes)
}
pub(super) fn component(
    seed: &otrios_interfaces::oib::Seed,
    selection: &Selection,
) -> Option<DefinitionId> {
    let doc = seed.designer.as_ref()?;
    let id = match selection {
        Selection::Definition(id) | Selection::DefinitionNode { definition: id, .. } => *id,
        Selection::Object(address) => {
            let root = doc.instances.iter().find(|i| i.id == address.instance)?;
            let (definition, node) = super::super::super::oib::document::path_node(
                doc,
                root.definition,
                &address.references,
                address.node,
            )
            .ok()?;
            if node.id != definition.root {
                return None;
            }
            definition.id
        }
        _ => return None,
    };
    Some(id)
}
fn source(seed: &otrios_interfaces::oib::Seed) -> Option<&s::DocumentStyles> {
    seed.graph.as_ref()?.styles.as_ref()
}
fn local(seed: &otrios_interfaces::oib::Seed) -> Option<&s::System> {
    let doc = seed.graph.as_ref()?;
    source(seed)?
        .catalog
        .systems
        .iter()
        .find(|system| system.reference.owner == s::Owner::Document(doc.id))
}
fn editable_component(seed: &otrios_interfaces::oib::Seed, definition: DefinitionId) -> bool {
    let Some(doc) = seed.designer.as_ref() else {
        return false;
    };
    let Some(def) = doc.definitions.iter().find(|d| d.id == definition) else {
        return false;
    };
    let Some(root) = def.nodes.iter().find(|n| n.id == def.root) else {
        return false;
    };
    super::super::super::oib::document::node_descriptors(doc, def, root).is_ok_and(|properties| {
        properties.iter().any(|p| {
            p.value_type == otrios_interfaces::layout::ValueType::Color
                && p.policy.live_literal
                && p.editor != PropertyEditor::ReadOnly
        })
    })
}
pub(super) fn library(
    b: &mut Builder,
    seed: &otrios_interfaces::oib::Seed,
    section: Section,
    page: usize,
    pick: Option<Pick>,
    next: u64,
    editable: bool,
) -> Result<(Vec<SceneNode>, f32)> {
    let mut nodes = tabs(b, section)?;
    let mut y = 64.;
    let system = local(seed);
    match section {
        Section::Components => return Ok((nodes, y)),
        Section::Styles => {
            if editable {
                for def in seed
                    .designer
                    .as_ref()
                    .into_iter()
                    .flat_map(|g| &g.definitions)
                    .filter(|d| {
                        editable_component(seed, d.id)
                            && !matches!(pick, Some(Pick::Style(id)) if id != d.id)
                    })
                {
                    nodes.push(b.focus_button(
                        0.,
                        y,
                        320.,
                        &format!("New style for {}", def.name),
                        LocalAction::Style(s::StyleEdit::CreateStyle {
                            id: next,
                            name: format!("style_{next}"),
                            definition: def.id,
                        }),
                    )?);
                    y += 56.;
                }
            }
            let styles = system.map_or(&[][..], |s| s.styles.as_slice());
            if styles.is_empty() {
                nodes.push(title(b, y, "No styles")?);
                y += 44.;
            }
            for item in styles.iter().skip(page * PAGE).take(PAGE) {
                let action = if let Some(Pick::Style(definition)) = pick {
                    let class = seed
                        .designer
                        .as_ref()
                        .and_then(|d| d.definitions.iter().find(|d| d.id == definition))
                        .and_then(|d| d.nodes.iter().find(|n| n.id == d.root))
                        .map(|n| n.class);
                    if !system.is_some_and(|system| {
                        system.contracts.iter().any(|c| {
                            c.id == item.contract
                                && class.is_some_and(|class| c.classes.contains(&class))
                        })
                    }) {
                        continue;
                    }
                    LocalAction::Style(s::StyleEdit::Assign {
                        definition,
                        style: Some(item.id),
                    })
                } else {
                    LocalAction::Select(Selection::Style(item.id))
                };
                nodes.push(b.focus_row(0., y, 320., &item.name, action)?);
                y += 56.;
            }
            paging(b, &mut nodes, &mut y, page, styles.len())?;
            if let Some(Pick::Style(definition)) = pick {
                nodes.push(b.focus_button(
                    0.,
                    y,
                    320.,
                    "Cancel",
                    LocalAction::Select(Selection::Definition(definition)),
                )?);
                y += 56.;
            }
        }
        Section::Variables => {
            if editable {
                nodes.push(b.focus_button(
                    0.,
                    y,
                    320.,
                    "New variable",
                    LocalAction::Style(s::StyleEdit::CreateVariable {
                        id: next,
                        name: format!("background_{next}"),
                        values: s::ColorModes {
                            light: [245, 245, 245, 255],
                            dark: [24, 24, 24, 255],
                        },
                    }),
                )?);
                y += 56.;
            }
            if matches!(pick, Some(Pick::Variable(..))) {
                nodes.push(title(b, y, "Choose color variable")?);
                y += 44.;
            }
            let variables = system.map_or(&[][..], |s| s.variables.as_slice());
            if variables.is_empty() {
                nodes.push(title(b, y, "No variables")?);
                y += 44.;
            }
            for item in variables.iter().skip(page * PAGE).take(PAGE) {
                let action =
                    if let (Some(Pick::Variable(style, field)), Some(system)) = (pick, system) {
                        LocalAction::Style(s::StyleEdit::SetField {
                            style,
                            field,
                            value: s::FieldValue::Variable(s::VariableRef {
                                system: system.reference.clone(),
                                variable: item.id,
                            }),
                        })
                    } else {
                        LocalAction::Select(Selection::Variable(item.id))
                    };
                nodes.push(b.focus_row(0., y, 320., &item.name, action)?);
                y += 56.;
            }
            paging(b, &mut nodes, &mut y, page, variables.len())?;
            if let Some(Pick::Variable(style, _)) = pick {
                nodes.push(b.focus_button(
                    0.,
                    y,
                    320.,
                    "Cancel",
                    LocalAction::Select(Selection::Style(style)),
                )?);
                y += 56.;
            }
        }
    }
    Ok((nodes, y))
}
fn paging(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    y: &mut f32,
    page: usize,
    count: usize,
) -> Result<()> {
    if page > 0 {
        nodes.push(b.focus_button(0., *y, 152., "Previous", LocalAction::StylePage(page - 1))?);
    }
    if (page + 1) * PAGE < count {
        nodes.push(b.focus_button(160., *y, 160., "More", LocalAction::StylePage(page + 1))?);
    }
    if page > 0 || (page + 1) * PAGE < count {
        *y += 56.;
    }
    Ok(())
}
pub(super) fn inspector(
    b: &mut Builder,
    seed: &otrios_interfaces::oib::Seed,
    selection: &Selection,
    next: u64,
    editable: bool,
    selected_field: Option<s::FieldId>,
    component_context: Option<DefinitionId>,
    mode: s::Mode,
) -> Result<(Vec<SceneNode>, f32)> {
    let mut nodes = vec![];
    let mut y = 0.;
    let system = local(seed);
    if let Some(definition) = component(seed, selection).filter(|id| editable_component(seed, *id))
    {
        nodes.push(title(b, y, "Definition style")?);
        y += 40.;
        if let Some(active) = source(seed).and_then(|source| {
            source
                .components
                .iter()
                .find(|binding| binding.definition == definition)
        }) {
            if let Some(style) =
                system.and_then(|s| s.styles.iter().find(|s| s.id == active.style.style))
            {
                nodes.push(b.focus_row(
                    0.,
                    y,
                    320.,
                    &style.name,
                    LocalAction::Select(Selection::Style(style.id)),
                )?);
                y += 56.;
            }
            if editable {
                nodes.push(b.focus_button(
                    0.,
                    y,
                    320.,
                    "Remove style",
                    LocalAction::Style(s::StyleEdit::Assign {
                        definition,
                        style: None,
                    }),
                )?);
                y += 56.;
            }
        }
        if editable {
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                "New style",
                LocalAction::Style(s::StyleEdit::CreateStyle {
                    id: next,
                    name: format!("style_{next}"),
                    definition,
                }),
            )?);
            y += 56.;
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                "Choose style...",
                LocalAction::StyleChoose(definition),
            )?);
            y += 56.;
        }
    }
    if let (Selection::Style(id), Some(system)) = (selection, system)
        && let Some(item) = system.styles.iter().find(|item| item.id == *id)
    {
        if editable
            && let Some(definition) = component_context
            && let Some(def) = seed
                .designer
                .as_ref()
                .and_then(|g| g.definitions.iter().find(|d| d.id == definition))
        {
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                &format!("Use for {}", def.name),
                LocalAction::Style(s::StyleEdit::Assign {
                    definition,
                    style: Some(*id),
                }),
            )?);
            y += 56.;
        }
        for field in &item.defaults {
            let name = system
                .contracts
                .iter()
                .find(|c| c.id == item.contract)
                .and_then(|c| c.fields.iter().find(|f| f.id == field.field))
                .map_or("Color", |f| f.name.as_str());
            let mut button = b.focus_row(
                0.,
                y,
                320.,
                &format!("Edit {name}"),
                LocalAction::StyleField(field.field),
            )?;
            button.semantics.as_mut().unwrap().checked = selected_field
                .or_else(|| item.defaults.first().map(|f| f.field))
                == Some(field.field);
            nodes.push(button);
            y += 56.;
        }
        for field in item.defaults.iter().filter(|field| {
            selected_field.or_else(|| item.defaults.first().map(|f| f.field)) == Some(field.field)
        }) {
            let name = system
                .contracts
                .iter()
                .find(|c| c.id == item.contract)
                .and_then(|c| c.fields.iter().find(|f| f.id == field.field))
                .map_or("Color", |f| f.name.as_str());
            nodes.push(title(b, y, &format!("{name} source"))?);
            y += 40.;
            if let s::FieldValue::Variable(reference) = &field.value {
                let variable = system.variables.iter().find(|v| v.id == reference.variable);
                nodes.push(b.focus_row(
                    0.,
                    y,
                    320.,
                    variable.map_or("Missing variable", |v| v.name.as_str()),
                    LocalAction::Select(Selection::Variable(reference.variable)),
                )?);
                y += 56.;
                if editable {
                    let color = variable.map_or([255; 4], |v| match mode {
                        s::Mode::Light => v.values.light,
                        s::Mode::Dark => v.values.dark,
                    });
                    nodes.push(b.focus_button(
                        0.,
                        y,
                        320.,
                        "Use literal",
                        LocalAction::Style(s::StyleEdit::SetField {
                            style: *id,
                            field: field.field,
                            value: s::FieldValue::Literal(color),
                        }),
                    )?);
                    y += 56.;
                }
            }
            if editable {
                nodes.push(b.focus_button(
                    0.,
                    y,
                    320.,
                    "Choose variable...",
                    LocalAction::StyleVariable(*id, field.field),
                )?);
                y += 56.;
            }
        }
        if editable {
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                "Delete style",
                LocalAction::Style(s::StyleEdit::DeleteStyle { style: *id }),
            )?);
            y += 56.;
        }
    }
    if let Selection::Variable(id) = selection
        && editable
    {
        nodes.push(b.focus_button(
            0.,
            y,
            320.,
            "Delete variable",
            LocalAction::Style(s::StyleEdit::DeleteVariable { variable: *id }),
        )?);
        y += 56.;
    }
    Ok((nodes, y))
}

pub(super) fn filter_properties(
    seed: &otrios_interfaces::oib::Seed,
    selection: &Selection,
    selected: Option<s::FieldId>,
    properties: Vec<PropertyInfo>,
) -> Vec<PropertyInfo> {
    let Selection::Style(style) = selection else {
        return properties;
    };
    let selected = selected.or_else(|| {
        local(seed)?
            .styles
            .iter()
            .find(|s| s.id == *style)?
            .defaults
            .first()
            .map(|f| f.field)
    });
    properties.into_iter().filter(|property| !matches!(&property.address,
        Some(PropertyAddress::Structural { target: StructuralTarget::StyleField { field, .. }, .. }) if Some(*field) != selected)).collect()
}

impl Client {
    pub(super) fn designer_style_action(&mut self, action: LocalAction) -> Result<bool> {
        use otrios_interfaces::oib::authoring::{DefinitionEdit, DocumentEdit, EditBatch};
        if self.input_busy() || !self.designer_blur_workspace_field()? {
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .is_some_and(Designer::editor_focused)
        {
            return Ok(false);
        }
        if let LocalAction::Style(edit) = action {
            let (batch, next_selection, context, library) = {
                let m = self.active.as_ref().ok_or("No Canvas")?;
                let d = m.designer.as_ref().ok_or("Missing designer")?;
                if d.mode != DesignerMode::Edit
                    || m.binding.as_ref().is_some_and(|b| !b.ui_editable)
                {
                    return Ok(false);
                }
                let seed = m.design_doc().oib_authoring_seed()?;
                let graph = seed.graph.as_ref().ok_or("Missing style source")?;
                let mut operations = vec![];
                if let s::StyleEdit::Assign {
                    definition,
                    style: Some(style),
                } = &edit
                {
                    let system = local(&seed).ok_or("Missing local styles")?;
                    let style = system
                        .styles
                        .iter()
                        .find(|s| s.id == *style)
                        .ok_or("Unknown style")?;
                    let contract = system
                        .contracts
                        .iter()
                        .find(|c| c.id == style.contract)
                        .ok_or("Unknown style contract")?;
                    if let Some(def) = graph.definitions.iter().find(|def| def.id == *definition) {
                        let root = def
                            .nodes
                            .iter()
                            .find(|node| node.id == def.root)
                            .ok_or("Missing component root")?;
                        for default in &root.defaults {
                            if contract
                                .fields
                                .iter()
                                .any(|field| field.id.0 == default.property.0)
                            {
                                operations.push(DocumentEdit::Definition(
                                    DefinitionEdit::ClearDefault {
                                        definition: *definition,
                                        node: root.id,
                                        property: default.property,
                                    },
                                ));
                            }
                        }
                    }
                }
                let selection = match &edit {
                    s::StyleEdit::CreateStyle { id, .. } => Some(Selection::Style(s::StyleId(*id))),
                    s::StyleEdit::CreateVariable { id, .. } => {
                        Some(Selection::Variable(s::VariableId(*id)))
                    }
                    s::StyleEdit::SetField { style, .. } => Some(Selection::Style(*style)),
                    s::StyleEdit::Assign { definition, .. } => {
                        Some(Selection::Definition(*definition))
                    }
                    s::StyleEdit::DeleteStyle { .. } | s::StyleEdit::DeleteVariable { .. } => {
                        Some(Selection::Document(graph.id))
                    }
                    _ => None,
                };
                let context = if let s::StyleEdit::CreateStyle { definition, .. } = &edit {
                    Some(*definition)
                } else {
                    d.style_component
                };
                let library = match edit {
                    s::StyleEdit::DeleteStyle { .. } => Some(Section::Styles),
                    s::StyleEdit::DeleteVariable { .. } => Some(Section::Variables),
                    _ => None,
                };
                operations.push(DocumentEdit::Style(edit.clone()));
                if let (
                    s::StyleEdit::CreateVariable { id, .. },
                    Some(Pick::Variable(style, field)),
                ) = (&edit, d.style_pick)
                {
                    let system = local(&seed).ok_or("Missing variable source")?;
                    operations.push(DocumentEdit::Style(s::StyleEdit::SetField {
                        style,
                        field,
                        value: s::FieldValue::Variable(s::VariableRef {
                            system: system.reference.clone(),
                            variable: s::VariableId(*id),
                        }),
                    }));
                }
                (
                    EditBatch {
                        generation: m
                            .design_doc()
                            .oib_structural_generation()
                            .ok_or("Missing style generation")?,
                        operations,
                    },
                    selection,
                    context,
                    library,
                )
            };
            let changed = self.designer_stage_structure(batch)?;
            if !changed {
                return Ok(false);
            }
            let m = self.active.as_mut().ok_or("No Canvas")?;
            let d = m.designer.as_mut().ok_or("Missing designer")?;
            if let Some(selection) = next_selection {
                d.select(selection);
            }
            d.style_component = context;
            d.style_pick = None;
            if let Some(section) = library {
                d.library_section = section;
                d.workspace_task = Task::Library;
            }
            if matches!(edit, s::StyleEdit::CreateStyle { .. }) {
                d.library_section = Section::Styles;
            }
            if matches!(edit, s::StyleEdit::CreateVariable { .. }) {
                d.library_section = Section::Variables;
            }
            d.revision = None;
            m.refresh()?;
            m.invalidate();
            return Ok(true);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        match action {
            LocalAction::LibrarySection(section) => {
                d.library_section = section;
                d.style_page = 0;
                d.style_pick = None;
                d.workspace_task = Task::Library;
            }
            LocalAction::StylePage(page) => d.style_page = page,
            LocalAction::StyleChoose(definition) => {
                d.library_section = Section::Styles;
                d.style_page = 0;
                d.style_pick = Some(Pick::Style(definition));
                d.workspace_task = Task::Library;
            }
            LocalAction::StyleVariable(style, field) => {
                d.library_section = Section::Variables;
                d.style_page = 0;
                d.style_pick = Some(Pick::Variable(style, field));
                d.workspace_task = Task::Library;
            }
            LocalAction::StyleField(field) => d.style_field = Some(field),
            _ => return Ok(false),
        }
        d.revision = None;
        d.invalidate_bounds();
        if d.workspace_task == Task::Library {
            if let Some(panel) = d.panels.first_mut() {
                panel.scroll.set_offset([0., 0.]);
            }
        }
        m.refresh_designer()?;
        m.invalidate();
        Ok(true)
    }
}
