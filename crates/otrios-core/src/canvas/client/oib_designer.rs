// SPDX-License-Identifier: LGPL-3.0-only
//! Device-local D1 tooling. All identity/source resolution stays in oib::document.
mod api_editor;
mod authoring;
mod editing;
mod layout_editor;
mod style;
use super::oib_property_editor::PropertyDrafts;
use super::oib_workspace::{self, Appearance, Task};
use super::*;
use crate::canvas::oib::document::{
    HierarchyKey, HierarchyRow, HierarchyScope, ObjectInfo, PropertyInfo,
};
use crate::dashboard::components::{ACCENT, MUTED, WHITE, background, canvas, text};
use otrios_interfaces::{
    oib::designer::{DesignerMode, InstanceId, PropertyAddress, PropertyEditor, Selection},
    scene::{Primitive, Rect, SwitchSemantics},
};

// Separate from u16 authored nodes, keyboard, ink and host chrome namespaces.
const FIRST: u32 = 2_700_000_000;
const END: u32 = 2_800_000_000;
const ROW: f32 = 38.;
const MAX_CONTENT: f32 = 8192.;
// Local retained geometry, NOT the 512-node/128-control presented scene budget.
// Two coordinate-bounded panels can retain more rows than one scene can admit.
// A hierarchy row uses at most six nodes/two actions per 38px; an inspector
// button uses three nodes/one action per 38px (labels use one per 28px).
// Together their 8192px caps fit below these defensive cache ceilings.
const MAX_CACHED_NODES: usize = 2048;
const MAX_CACHED_ACTIONS: usize = 768;
// Align culling to eight-row slabs. Frame timing shows topology rebuilds at
// four-row boundaries can miss a display period during a 30 Hz pan. Keep
// bounded overscan to amortize that work without raising scene ceilings. Placement-only pans reuse retained GPU
// geometry until either viewport edge crosses a slab boundary. Crossing one
// changes scene topology and necessarily rebuilds the shared paint cache once.
const WINDOW_STEP: f32 = 8. * ROW;
#[derive(Clone, Debug, PartialEq)]
enum LocalAction {
    Select(Selection),
    NewComponent,
    Undo,
    Redo,
    DiscardWorkingCopy,
    ApplyWorkingCopy,
    Palette,
    Placement(Selection),
    EditComponent(Selection),
    DeleteComponent(u64),
    ReviewDefinition(otrios_interfaces::oib::designer::DefinitionId),
    ReviewProperty(
        otrios_interfaces::oib::designer::DefinitionId,
        otrios_interfaces::oib::designer::PropertyId,
    ),
    Structure(otrios_interfaces::oib::InstanceOperation),
    FocusField(u32),
    Apply(PropertyAddress),
    Discard(PropertyAddress),
    PickColor(PropertyAddress, [u8; 4]),
    Opacity(PropertyAddress, u8),
    DraftBool(PropertyAddress, bool),
    Details(PropertyAddress),
    Source(PropertyAddress),
    Choose(
        Box<super::super::oib::document::PropertyRegisterView>,
        Option<Value>,
    ),
    TreeExpand(HierarchyKey),
    Expand(Selection),
    Mode(DesignerMode),
    Workspace(Task),
    Appearance(Appearance),
    ApiOpen(bool),
    ApiProperty(Option<otrios_interfaces::oib::designer::PropertyPath>),
    LayoutNoop,
    LayoutBounds(bool),
    LayoutPadding(bool, Option<otrios_interfaces::oib::InstanceOperation>),
    Write(PropertyAddress, Option<Value>),
    RestoreDesign(PropertyAddress, u64),
    Resolve(PropertyAddress),
}
impl LocalAction {
    fn outside_panels(&self) -> bool {
        matches!(
            self,
            Self::Workspace(_)
                | Self::Appearance(_)
                | Self::Mode(_)
                | Self::EditComponent(_)
                | Self::Placement(_)
                | Self::DeleteComponent(_)
                | Self::ReviewDefinition(_)
                | Self::ReviewProperty(..)
                | Self::NewComponent
                | Self::Undo
                | Self::Redo
                | Self::DiscardWorkingCopy
                | Self::ApplyWorkingCopy
        )
    }
}
#[derive(Clone)]
struct ManagedComponent {
    instance: u64,
    root: Selection,
}
#[derive(Clone)]
struct Capture {
    pointer: u32,
    start: [f32; 2],
    moving: bool,
    action: Option<LocalAction>,
    panel: Option<usize>,
}
#[derive(Clone)]
struct Panel {
    viewport: u16,
    nodes: Vec<SceneNode>,
    content: f32,
    scroll: super::super::text_scroll::Scroll,
}
#[derive(Clone)]
pub(super) struct ToolCache {
    generation: u64,
    selection: Selection,
    operations: Vec<(String, otrios_interfaces::oib::InstanceOperation)>,
}
fn retain_inspector_ids(
    previous: &[SceneNode],
    next: &mut [SceneNode],
    previous_actions: &BTreeMap<u32, LocalAction>,
    next_actions: &mut BTreeMap<u32, LocalAction>,
    diagnostics: std::ops::Range<usize>,
) {
    fn compatible(
        old: &SceneNode,
        new: &SceneNode,
        old_actions: &BTreeMap<u32, LocalAction>,
        new_actions: &BTreeMap<u32, LocalAction>,
        owned: bool,
    ) -> bool {
        let action = old_actions.get(&old.id);
        let owned = owned || action.is_some();
        old.frame.width == new.frame.width
            && old.frame.height == new.frame.height
            && old.responder == new.responder
            && std::mem::discriminant(&old.primitive) == std::mem::discriminant(&new.primitive)
            && action == new_actions.get(&new.id)
            // A typed control owns its decorative surface/label identities.
            // Standalone diagnostics have no such owner: match their actual
            // content, rather than assigning an old heading ID to a new notice.
            && (owned || (old.primitive == new.primitive && old.semantics == new.semantics))
            && old.children.len() == new.children.len()
            && old.children.iter().zip(&new.children).all(|(old, new)| {
                compatible(old, new, old_actions, new_actions, owned)
            })
    }
    fn retain(old: &SceneNode, new: &mut SceneNode, actions: &mut BTreeMap<u32, LocalAction>) {
        if let Some(action) = actions.remove(&new.id) {
            actions.insert(old.id, action);
        }
        new.id = old.id;
        for (old, new) in old.children.iter().zip(&mut new.children) {
            retain(old, new, actions);
        }
    }
    let mut used = vec![false; previous.len()];
    for (index, node) in next.iter_mut().enumerate() {
        let matches = |previous_index: usize| {
            !used[previous_index]
                // Only typed controls can follow rows across positions. Text
                // diagnostics are not semantic keys for finding another row.
                && (previous_index == index || previous_actions.contains_key(&previous[previous_index].id))
                && compatible(
                    &previous[previous_index], node, previous_actions, next_actions,
                    // Selection title and status are fixed presentation slots
                    // for this unchanged inspector owner, not text-derived keys.
                    diagnostics.contains(&index) && previous_index == index,
                )
        };
        // Normal value/status updates stay linear. Inserted/removed rows may
        // move an unchanged typed control, whose identity follows its action.
        let matched = (index < previous.len() && matches(index))
            .then_some(index)
            .or_else(|| (0..previous.len()).find(|index| matches(*index)));
        if let Some(index) = matched {
            used[index] = true;
            retain(&previous[index], node, next_actions);
        }
    }
}

pub(super) struct Designer {
    pub(super) selection: Selection,
    focus_layout: bool,
    workspace_task: Task,
    appearance: Appearance,
    palette_open: bool,
    placement_open: bool,
    layout_bounds: bool,
    padding_independent: bool,
    api_open: bool,
    api_property: Option<otrios_interfaces::oib::designer::PropertyPath>,
    api_model: Option<api_editor::Model>,
    management: Option<ManagedComponent>,
    management_nodes: Vec<SceneNode>,
    structure_tools: Vec<(String, otrios_interfaces::oib::InstanceOperation)>,
    pub(super) graph_tools: Option<ToolCache>,
    connection_tools: Vec<(
        PropertyAddress,
        Vec<(String, otrios_interfaces::oib::InstanceOperation)>,
    )>,
    tree: Vec<HierarchyRow>,
    tree_scope: HierarchyScope,
    tree_closed: Vec<HierarchyKey>,
    tree_initialized: Vec<HierarchyScope>,
    reveal_selection: bool,
    pub(super) drafts: PropertyDrafts,
    properties: Vec<PropertyInfo>,
    editor_notice: Option<String>,
    editor_revision: u64,
    editor_structural_revision: u64,
    editor_structural_generation: u64,
    details: Option<super::super::oib::document::PropertySources>,
    registers: Vec<super::super::oib::document::PropertyRegisterView>,
    pending_source: Option<PropertyAddress>,
    pending_selection: Option<Selection>,
    mode: DesignerMode,
    pending_mode: Option<DesignerMode>,
    pending_viewport: Option<[f32; 2]>,
    collapsed: Vec<Selection>,
    objects: Vec<ObjectInfo>,
    panels: Vec<Panel>,
    actions: BTreeMap<u32, LocalAction>,
    // Never recycle IDs on selection/disclosure/diagnostic rebuilds. A stale
    // accessibility ID must not acquire a different typed action later.
    next_id: u32,
    capture: Option<Capture>,
    // Rebuild chrome on diagnostics/selection changes, never on a pan sample.
    pub(super) revision: Option<(
        crate::merkle::Hash,
        Option<crate::merkle::Hash>,
        bool,
        bool,
        bool,
    )>,
}
impl Clone for Designer {
    fn clone(&self) -> Self {
        Self {
            selection: self.selection.clone(),
            focus_layout: self.focus_layout,
            workspace_task: self.workspace_task,
            appearance: self.appearance,
            palette_open: self.palette_open,
            placement_open: self.placement_open,
            layout_bounds: self.layout_bounds,
            padding_independent: self.padding_independent,
            api_open: self.api_open,
            api_property: self.api_property.clone(),
            api_model: self.api_model.clone(),
            management: self.management.clone(),
            management_nodes: self.management_nodes.clone(),
            structure_tools: self.structure_tools.clone(),
            graph_tools: self.graph_tools.clone(),
            connection_tools: self.connection_tools.clone(),
            tree: self.tree.clone(),
            tree_scope: self.tree_scope,
            tree_closed: self.tree_closed.clone(),
            tree_initialized: self.tree_initialized.clone(),
            reveal_selection: self.reveal_selection,
            drafts: self.drafts.clone(),
            properties: self.properties.clone(),
            editor_notice: self.editor_notice.clone(),
            editor_revision: self.editor_revision,
            editor_structural_revision: self.editor_structural_revision,
            editor_structural_generation: self.editor_structural_generation,
            details: self.details.clone(),
            registers: self.registers.clone(),
            pending_source: self.pending_source.clone(),
            pending_selection: self.pending_selection.clone(),
            mode: self.mode,
            pending_mode: self.pending_mode,
            pending_viewport: self.pending_viewport,
            collapsed: self.collapsed.clone(),
            panels: self.panels.clone(),
            actions: self.actions.clone(),
            next_id: self.next_id,
            capture: self.capture.clone(),
            revision: self.revision,
            objects: self
                .objects
                .iter()
                .map(|o| ObjectInfo {
                    selection: o.selection.clone(),
                    name: o.name.clone(),
                    class: o.class,
                    depth: o.depth,
                    layout_node: o.layout_node,
                })
                .collect(),
        }
    }
}
impl Designer {
    pub(super) fn new(seed: &otrios_interfaces::oib::Seed) -> Option<Self> {
        let metadata = seed.designer.as_ref()?;
        let mut panels = vec![];
        for viewport in metadata
            .hierarchy_viewport
            .into_iter()
            .chain([seed.inspector_viewport])
        {
            panels.push(Panel {
                viewport,
                nodes: vec![],
                content: ROW,
                scroll: super::super::text_scroll::Scroll::new(viewport, [0., ROW]),
            });
        }
        Some(Self {
            selection: Selection::Document(metadata.id),
            focus_layout: seed.instance_surface.is_some() && metadata.hierarchy_viewport.is_some(),
            workspace_task: Task::Inspect,
            appearance: Appearance::Dark,
            palette_open: false,
            placement_open: false,
            layout_bounds: false,
            padding_independent: false,
            api_open: false,
            api_property: None,
            api_model: None,
            management: None,
            management_nodes: vec![],
            structure_tools: vec![],
            graph_tools: None,
            connection_tools: vec![],
            tree: vec![],
            tree_scope: HierarchyScope::Composition,
            tree_closed: vec![],
            tree_initialized: vec![],
            reveal_selection: false,
            drafts: PropertyDrafts::new(2_800_000_000, 2_900_000_000 - 1),
            properties: vec![],
            editor_notice: None,
            editor_revision: 0,
            editor_structural_revision: 0,
            editor_structural_generation: 0,
            details: None,
            registers: vec![],
            pending_source: None,
            pending_selection: None,
            mode: DesignerMode::Edit,
            pending_mode: None,
            pending_viewport: None,
            collapsed: vec![],
            objects: vec![],
            panels,
            actions: BTreeMap::new(),
            next_id: FIRST,
            capture: None,
            revision: None,
        })
    }
    pub(super) fn usage(&self, authoring: bool) -> super::DesignerUsage {
        super::DesignerUsage {
            task: match self.workspace_task {
                Task::Library => "Library",
                Task::Layers => "Layers",
                Task::Inspect => "Inspect",
            },
            mode: match self.mode {
                DesignerMode::Edit => "Edit",
                DesignerMode::Test => "Test",
            },
            selection: format!("{:?}", self.selection),
            authoring,
        }
    }
    pub(super) fn focus_layout(&self) -> bool {
        self.focus_layout
    }
    pub(super) fn management_visible(&self) -> bool {
        self.focus_layout && self.management.is_some()
    }
    pub(super) fn workspace_task(&self) -> Task {
        self.workspace_task
    }
    pub(super) fn workspace_mode(&self) -> DesignerMode {
        self.mode
    }
    pub(super) fn appearance(&self) -> Appearance {
        self.appearance
    }
    fn definition_scope(&self) -> Option<otrios_interfaces::oib::designer::DefinitionId> {
        match self.selection {
            Selection::Definition(id) | Selection::DefinitionNode { definition: id, .. } => {
                Some(id)
            }
            _ => None,
        }
    }
    fn task_scope(&self, task: Task) -> HierarchyScope {
        if task == Task::Library || self.definition_scope().is_some() {
            HierarchyScope::Components
        } else {
            HierarchyScope::Composition
        }
    }
    fn local_action(&self, id: u32) -> Option<LocalAction> {
        if self.focus_layout {
            let action = match id {
                oib_workspace::TAB_LIBRARY => Some(LocalAction::Workspace(Task::Library)),
                oib_workspace::TAB_LAYERS => Some(LocalAction::Workspace(Task::Layers)),
                oib_workspace::TAB_INSPECT => Some(LocalAction::Workspace(Task::Inspect)),
                oib_workspace::MODE_EDIT => Some(LocalAction::Mode(DesignerMode::Edit)),
                oib_workspace::MODE_TEST => Some(LocalAction::Mode(DesignerMode::Test)),
                oib_workspace::APPEARANCE_DARK => Some(LocalAction::Appearance(Appearance::Dark)),
                oib_workspace::APPEARANCE_LIGHT => Some(LocalAction::Appearance(Appearance::Light)),
                oib_workspace::NEW_COMPONENT => Some(LocalAction::NewComponent),
                oib_workspace::UNDO => Some(LocalAction::Undo),
                oib_workspace::REDO => Some(LocalAction::Redo),
                oib_workspace::DISCARD => Some(LocalAction::DiscardWorkingCopy),
                oib_workspace::APPLY => Some(LocalAction::ApplyWorkingCopy),
                _ => None,
            };
            if action.is_some() {
                return action;
            }
        }
        self.actions.get(&id).cloned()
    }
    pub(super) fn root_selection(&self, instance: u64) -> Option<Selection> {
        self.objects.iter().find(|o| matches!(&o.selection,Selection::Object(a) if a.instance.0 == instance && a.references.is_empty())).map(|o| o.selection.clone())
    }
    fn intercepts_preview(&self) -> bool {
        self.mode == DesignerMode::Edit
    }
    pub(super) fn busy(&self) -> bool {
        self.capture.is_some()
    }
    pub(super) fn defer_viewport(&mut self, viewport: [f32; 2]) {
        self.pending_viewport = Some(viewport);
    }
    pub(super) fn cancel(&mut self) {
        self.capture = None;
        for panel in &mut self.panels {
            panel.scroll.cancel();
        }
    }
    pub(super) fn invalidate_bounds(&mut self) {
        for panel in &mut self.panels {
            panel.scroll.bounds_dirty = true;
        }
    }
    pub(super) fn sync(&mut self, scene: &Scene) -> Result<()> {
        for panel in &mut self.panels {
            if panel.scroll.bounds_dirty {
                panel.scroll.sync(scene)?;
            }
        }
        Ok(())
    }
    pub(super) fn inspector_scroll_info(&self) -> Option<super::super::text_scroll::Info> {
        self.panels.last().map(|panel| panel.scroll.info())
    }
    pub(super) fn inspector_viewport(&self) -> Option<u32> {
        self.panels.last().map(|panel| u32::from(panel.viewport))
    }
    pub(super) fn tick(&mut self, now: u64) -> bool {
        self.panels
            .iter_mut()
            .fold(false, |changed, p| p.scroll.tick(now) || changed)
    }
    fn select(&mut self, selection: Selection) -> bool {
        if self.selection == selection && self.workspace_task == Task::Inspect {
            return false;
        }
        if self.workspace_task != Task::Inspect {
            self.invalidate_bounds();
        }
        self.workspace_task = Task::Inspect;
        if self.focus_layout {
            self.tree_scope = match &selection {
                Selection::Definition(_) | Selection::DefinitionNode { .. } => {
                    HierarchyScope::Components
                }
                _ => HierarchyScope::Composition,
            };
            self.reveal_selection = true;
        }
        self.layout_bounds = false;
        self.padding_independent = false;
        self.api_open = false;
        self.api_property = None;
        self.api_model = None;
        self.selection = selection;
        self.revision = None;
        if let Some(panel) = self.panels.last_mut() {
            panel.scroll.set_offset([0., 0.]);
        }
        true
    }
    fn set_mode(&mut self, mode: DesignerMode, blocked: bool) -> bool {
        if blocked {
            let changed = self.pending_mode != Some(mode);
            self.pending_mode = Some(mode);
            return changed;
        }
        let changed = self.mode != mode || self.pending_mode.is_some();
        self.pending_mode = None;
        self.mode = mode;
        if changed {
            self.revision = None;
        }
        changed
    }
    fn rebuild(
        &mut self,
        seed: &otrios_interfaces::oib::Seed,
        objects: Vec<ObjectInfo>,
        properties: &[PropertyInfo],
        conflicts: &[u64],
        status: &str,
        editable: bool,
    ) -> Result<()> {
        self.rebuild_panels(seed, objects, properties, conflicts, status, editable, true)
    }
    #[allow(clippy::too_many_arguments)]
    fn rebuild_panels(
        &mut self,
        seed: &otrios_interfaces::oib::Seed,
        objects: Vec<ObjectInfo>,
        properties: &[PropertyInfo],
        conflicts: &[u64],
        status: &str,
        editable: bool,
        structure: bool,
    ) -> Result<()> {
        // Bound even the hidden/disclosed semantic model, independently from
        // the retained subtree and presented scene bounds. Runtime admission
        // supplies a smaller bounded graph; this is a defensive local ceiling.
        if objects.len() > MAX_CACHED_NODES {
            return Err("Designer object model budget exceeded".into());
        }
        let mut builder = Builder::at(self.next_id);
        builder.palette = self.appearance.palette();
        let previous_management = self.management_nodes.clone();
        if structure {
            self.management_nodes.clear();
        }
        if structure && let Some(target) = &self.management {
            let mut edit = builder.focus_button(
                0.,
                0.,
                156.,
                "Edit",
                LocalAction::EditComponent(target.root.clone()),
            )?;
            edit.semantics
                .as_mut()
                .ok_or("Missing Edit semantics")?
                .label = "Edit component".into();
            let mut delete = builder.focus_button(
                164.,
                0.,
                156.,
                "Remove...",
                LocalAction::DeleteComponent(target.instance),
            )?;
            delete
                .semantics
                .as_mut()
                .ok_or("Missing Delete semantics")?
                .label = "Remove instance...".into();
            let mut nodes = vec![edit, delete];
            retain_inspector_ids(
                &previous_management,
                &mut nodes,
                &self.actions,
                &mut builder.actions,
                0..0,
            );
            self.management_nodes = nodes;
        }
        let hierarchy = if structure && self.panels.len() == 2 {
            Some(if self.focus_layout {
                focus_hierarchy(
                    &mut builder,
                    &self.tree,
                    &self.selection,
                    &self.tree_closed,
                    self.workspace_task,
                    self.mode,
                    AuthoringTools {
                        operations: &self.structure_tools,
                        delete_definition: match self.selection {
                            Selection::Definition(id)
                            | Selection::DefinitionNode { definition: id, .. } => Some(id),
                            _ => None,
                        },
                        open: self.palette_open,
                        placement: self
                            .management
                            .as_ref()
                            .filter(|_| {
                                seed.designer.as_ref().is_some_and(|d| {
                                    d.definitions.iter().any(|d| !d.slots.is_empty())
                                })
                            })
                            .map(|m| (&m.root, self.placement_open)),
                    },
                )?
            } else {
                hierarchy(
                    &mut builder,
                    &objects,
                    &self.selection,
                    &self.collapsed,
                    self.mode,
                )?
            })
        } else {
            None
        };
        self.properties = properties.to_vec();
        let mut inspector = if self.focus_layout {
            editing::inspector(
                &mut builder,
                &mut self.drafts,
                properties,
                conflicts,
                status,
                editable,
                self.editor_revision,
                self.editor_structural_revision,
                self.editor_structural_generation,
                self.editor_notice.as_deref(),
                self.details.as_ref(),
                &self.registers,
                seed,
                &self.connection_tools,
                self.mode == DesignerMode::Edit,
                self.layout_bounds,
                self.padding_independent,
            )?
        } else {
            inspector(
                &mut builder,
                seed,
                &self.selection,
                properties,
                conflicts,
                status,
                editable,
            )?
        };

        if self.focus_layout && api_editor::definition(&self.selection).is_some() {
            let mut height = 0.;
            let api = api_editor::build(
                &mut builder,
                self.api_model.as_ref(),
                self.api_open,
                &mut height,
            )?;
            // Keep the selection title first; API is a separate disclosure above
            // layout and literal fields, inside the same scrolling inspector.
            for node in inspector.0.iter_mut().skip(1) {
                node.frame.y += height;
            }
            let mut api = api;
            for node in &mut api {
                node.frame.y += 44.;
            }
            inspector.0.splice(1..1, api);
            inspector.1 += height;
        }
        if let Some(selected) = objects.iter().find(|o| o.selection == self.selection)
            && let Some(title) = inspector.0.first_mut()
            && let Primitive::Text { value, .. } = &mut title.primitive
        {
            let class = seed
                .designer
                .as_ref()
                .and_then(|d| d.classes.iter().find(|c| c.id == selected.class));
            *value = format!(
                "{} : {}",
                selected.name,
                class.map_or("signed class", |c| c.name.as_str())
            );
        }
        if self.focus_layout
            && let Some(wrapper) = self
                .tree
                .iter()
                .find(|r| r.selection.as_ref() == Some(&self.selection))
                .and_then(|r| r.reference_wrapper.clone())
        {
            inspector.0.push(builder.focus_button(
                0.,
                inspector.1 + 16.,
                320.,
                "Inspect reference",
                LocalAction::Select(Selection::Object(wrapper)),
            )?);
            inspector.1 += 72.;
        }
        // Without a hierarchy surface retain a local mode switch in inspector.
        let mut nodes = inspector.0;
        if self.panels.len() == 1 {
            for node in &mut nodes {
                node.frame.y += ROW;
            }
            let action = LocalAction::Mode(if self.mode == DesignerMode::Edit {
                DesignerMode::Test
            } else {
                DesignerMode::Edit
            });
            nodes.insert(
                0,
                builder.button(0., 0., 240., &format!("Mode: {:?}", self.mode), action)?,
            );
        }
        if let Some((mut nodes, content)) = hierarchy {
            if self.focus_layout {
                retain_inspector_ids(
                    &self.panels[0].nodes,
                    &mut nodes,
                    &self.actions,
                    &mut builder.actions,
                    0..0,
                );
            }
            self.panels[0].nodes = nodes;
            self.panels[0].content = content;
        }
        let single = self.panels.len() == 1;
        let panel = self.panels.last_mut().ok_or("Missing designer inspector")?;
        if !structure || self.focus_layout {
            // Local value/status changes preserve independent typed controls,
            // even when another row is inserted or moves. Different actions
            // and newly disclosed Reset/conflict controls receive fresh IDs.
            retain_inspector_ids(
                &panel.nodes,
                &mut nodes,
                &self.actions,
                &mut builder.actions,
                usize::from(single)..usize::from(single) + 1,
            );
            let mut retired = Vec::new();
            fn ids(node: &SceneNode, out: &mut Vec<u32>) {
                out.push(node.id);
                for child in &node.children {
                    ids(child, out);
                }
            }
            for node in &panel.nodes {
                ids(node, &mut retired);
            }
            self.actions.retain(|id, _| !retired.contains(id));
        }
        panel.nodes = nodes;
        panel.content = inspector.1 + if single { ROW } else { 0. };
        for panel in &mut self.panels {
            if panel.content > MAX_CONTENT {
                return Err("Designer panel coordinate budget exceeded".into());
            }
            panel.scroll.set_content([0., panel.content]);
        }
        if structure {
            self.actions = builder.actions;
        } else {
            self.actions.extend(builder.actions);
        }
        self.next_id = builder.next;
        self.objects = objects;
        Ok(())
    }
    pub(super) fn present(
        &self,
        root: &mut SceneNode,
        layout: &Scene,
        preview: Option<u32>,
        status: &str,
        confirmed: bool,
    ) -> Result<()> {
        // Structural rejection must remain visible even while the detailed
        // inspector notice is scrolled out of view. Reuse persistent chrome;
        // this adds no scene nodes, input targets or shared state.
        if let Some(title) = find_node(root, oib_workspace::PROPERTIES_TITLE)
            && let Primitive::Text { value, color, .. } = &mut title.primitive
        {
            let rejected = status.contains("Structure rejected:");
            *value = if rejected {
                "Change rejected"
            } else {
                match self.workspace_task {
                    Task::Library => "Library",
                    Task::Layers => "Layers",
                    Task::Inspect => "Inspect",
                }
            }
            .into();
            *color = self.appearance.palette().text;
        }
        if let Some(node) = find_node(root, oib_workspace::STATUS)
            && let Primitive::Text { value, .. } = &mut node.primitive
        {
            *value = if self.drafts.drafts().iter().any(|d| d.error.is_some()) {
                "Fix invalid values"
            } else if status.contains("rejected")
                || status.contains("Conflict")
                || status.contains("review")
            {
                "Changes need review"
            } else if !confirmed || status.contains("Saving") || status.contains("Applying") {
                "Applying changes"
            } else if status.contains("Unapplied") || self.drafts.drafts().iter().any(|d| d.dirty) {
                "Unapplied changes"
            } else {
                "All changes applied"
            }
            .into();
        }
        if let Some(toolbar) = find_node(root, oib_workspace::PROPERTIES_ACTIONS) {
            toolbar.children = self.management_nodes.clone();
            let width = ((toolbar.frame.width - 8.) / 2.).max(1.);
            for (index, node) in toolbar.children.iter_mut().enumerate() {
                node.frame.x = index as f32 * (width + 8.);
                node.frame.width = width;
                style::resize_surface(&mut node.children[0], width, 52.)?;
                node.children[1].frame.width = (width - 16.).max(1.);
            }
        }
        for panel in &self.panels {
            let node =
                find_node(root, u32::from(panel.viewport)).ok_or("Missing designer viewport")?;
            let hierarchy_panel = self
                .panels
                .first()
                .is_some_and(|p| p.viewport == panel.viewport)
                && self.panels.len() > 1;
            if self.focus_layout && (hierarchy_panel == (self.workspace_task == Task::Inspect)) {
                node.children.clear();
                node.responder = None;
                continue;
            }
            let offset = panel.scroll.info().offset[1];
            // The responsive workspace retains panel geometry over a software
            // keyboard. Smaller slabs keep their combined overscan bounded.
            let window_step = if self.focus_layout {
                2. * ROW
            } else {
                WINDOW_STEP
            };
            let top = (offset / window_step).floor() * window_step;
            // Anchor both edges to the same slab. Letting the lower edge
            // advance independently changes topology halfway through a slab.
            // This fixed span equals the previous maximum aligned span, so it
            // adds no worst-case overscan or scene budget.
            let span = (node.frame.height / window_step).ceil() + 1.;
            let bottom = top + span * window_step;
            // Retained top-level rows contain all their own paint and actions.
            // Clone only rows intersecting the viewport's aligned window; no
            // glyph/inspector construction or full-tree clone on a pan sample.
            node.children = panel
                .nodes
                .iter()
                .filter(|row| row.frame.y < bottom && row.frame.y + row.frame.height > top)
                .cloned()
                .collect();
            node.responder = None;
            // The signed viewport sets the real available width. Rows occupy it
            // completely, including padding/corners, without covering siblings.
            fn fit(nodes: &mut [SceneNode], width: f32) -> Result<()> {
                for n in nodes {
                    if matches!(n.primitive, Primitive::Canvas { .. })
                        && n.frame.x == 0.
                        && matches!(n.frame.width, 240. | 288. | 320.)
                    {
                        if let Some(bg) = n.children.first_mut() {
                            style::resize_surface(bg, width, bg.frame.height)?;
                        }
                        n.frame.width = width;
                    }
                }
                Ok(())
            }
            fit(&mut node.children, node.frame.width)?;
            panel.scroll.present(root);
            host_panel::present_panel(root, u32::from(panel.viewport));
        }
        if let Some(preview) = preview
            && matches!(self.selection, Selection::Object(_))
            && let Some(object) = self.objects.iter().find(|o| o.selection == self.selection)
            && let Some(node) = object.layout_node
            && let (Some(path), Some(view)) = (layout.path(u32::from(node)), layout.path(preview))
        {
            let selected = path.last().ok_or("Missing selection frame")?;
            let viewport = view.last().ok_or("Missing preview frame")?;
            if path.iter().any(|p| p.node.id == preview) {
                let content_scale = viewport.scale
                    * if let Primitive::Viewport { scale } = viewport.node.primitive {
                        scale
                    } else {
                        1.
                    };
                let scale = selected.scale / content_scale;
                let x = (selected.origin[0] - viewport.origin[0]) / content_scale;
                let y = (selected.origin[1] - viewport.origin[1]) / content_scale;
                let w = selected.node.frame.width * scale;
                let h = selected.node.frame.height * scale;
                let mut outline = canvas(FIRST - 10, Rect::new(x, y, w, h), vec![]);
                outline.responder = Some(TouchResponder::PassThrough);
                for (i, rect) in [
                    Rect::new(0., 0., w, 2.),
                    Rect::new(0., h - 2., w, 2.),
                    Rect::new(0., 0., 2., h),
                    Rect::new(w - 2., 0., 2., h),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut edge = background(
                        FIRST - 9 + i as u32,
                        rect.width,
                        rect.height,
                        self.appearance.palette().focus,
                        0.,
                    );
                    edge.frame = rect;
                    outline.children.push(edge);
                }
                if let Some(viewport) = find_node(root, preview) {
                    viewport.children.push(outline);
                }
            }
        }
        Ok(())
    }
    fn semantic_action(&self, scene: &Scene, id: u32) -> Option<LocalAction> {
        // Cached rows outside the clipped semantic scene are not activatable,
        // including overscan rows and old IDs retained by accessibility hosts.
        scene
            .controls()
            .iter()
            .any(|p| p.node.id == id)
            .then(|| self.local_action(id))
            .flatten()
    }
    fn tap_action(&self, capture: &Capture, scene: &Scene, point: [f32; 2]) -> Option<LocalAction> {
        if capture.panel.is_none() {
            if capture
                .action
                .as_ref()
                .is_some_and(LocalAction::outside_panels)
            {
                return scene
                    .hit_path(point)
                    .iter()
                    .any(|p| self.local_action(p.node.id) == capture.action)
                    .then(|| capture.action.clone())
                    .flatten();
            }
            return capture.action.clone();
        }
        let hit = scene.hit_path(point);
        let current = hit.iter().rev().find_map(|p| self.actions.get(&p.node.id));
        capture.action.clone().filter(|a| Some(a) == current)
    }
}
fn find_node(node: &mut SceneNode, id: u32) -> Option<&mut SceneNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter_mut().find_map(|n| find_node(n, id))
}
struct Builder {
    palette: oib_workspace::Palette,
    next: u32,
    nodes: usize,
    actions: BTreeMap<u32, LocalAction>,
}
impl Builder {
    #[cfg(test)]
    fn new() -> Self {
        Self::at(FIRST)
    }
    fn at(next: u32) -> Self {
        Self {
            palette: Appearance::Dark.palette(),
            next,
            nodes: 0,
            actions: BTreeMap::new(),
        }
    }
    fn id(&mut self) -> Result<u32> {
        self.nodes += 1;
        if self.nodes > MAX_CACHED_NODES {
            return Err("Designer retained node budget exceeded".into());
        }
        let id = self.next;
        if id >= END {
            return Err("Designer local identity budget exhausted".into());
        }
        self.next += 1;
        Ok(id)
    }
    fn label(&mut self, x: f32, y: f32, value: &str, color: [u8; 4]) -> Result<SceneNode> {
        // Text is a diagnostic, never a key. Keep UTF-8 boundaries and the scene
        // text limit; complete typed identities remain in selection/actions.
        let value: String = value
            .chars()
            .scan(0, |bytes, c| {
                *bytes += c.len_utf8();
                (*bytes <= 256).then_some(c)
            })
            .collect();
        Ok(text(
            self.id()?,
            Rect::new(x, y, 240., 24.),
            &value,
            1,
            self.palette.role(color),
        ))
    }
    fn focus_button(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        label: &str,
        action: LocalAction,
    ) -> Result<SceneNode> {
        self.focus_control(x, y, width, label, action, true)
    }
    fn focus_row(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        label: &str,
        action: LocalAction,
    ) -> Result<SceneNode> {
        self.focus_control(x, y, width, label, action, false)
    }
    fn focus_control(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        label: &str,
        action: LocalAction,
        outlined: bool,
    ) -> Result<SceneNode> {
        let mut row = self.button_style(x, y, width, label, action, outlined)?;
        row.frame.height = 52.;
        style::resize_surface(&mut row.children[0], width, 52.)?;
        let title = &mut row.children[1];
        title.frame = Rect::new(8., 16., (width - 16.).max(1.), 28.);
        if let Primitive::Text { scale, .. } = &mut title.primitive {
            *scale = 2;
        }
        Ok(row)
    }
    fn button(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        label: &str,
        action: LocalAction,
    ) -> Result<SceneNode> {
        self.button_style(x, y, width, label, action, true)
    }
    fn button_style(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        label: &str,
        action: LocalAction,
        outlined: bool,
    ) -> Result<SceneNode> {
        let id = self.id()?;
        let mut bg = if outlined {
            style::surface_with_outline(
                [self.id()?, self.id()?, self.id()?],
                width,
                32.,
                self.palette.surface,
                self.palette.line,
                8.,
            )
        } else {
            background(self.id()?, width, 32., self.palette.surface, 4.)
        };
        bg.responder = Some(TouchResponder::PassThrough);
        let label_node = self.label(8., 8., label, WHITE)?;
        let mut node = canvas(id, Rect::new(x, y, width, 32.), vec![bg, label_node]);
        // One local sentinel gives whole-rectangle hit/semantic behavior. The
        // local router consumes it before InputRouter; no app event is allocated.
        node.responder = Some(TouchResponder::Activate(Action::KeyboardKey(u16::MAX)));
        node.semantics = Some(SwitchSemantics {
            label: label.into(),
            checked: false,
        });
        if self.actions.len() >= MAX_CACHED_ACTIONS {
            return Err("Designer retained action budget exceeded".into());
        }
        self.actions.insert(id, action);
        Ok(node)
    }
}
fn hierarchy(
    b: &mut Builder,
    objects: &[ObjectInfo],
    selected: &Selection,
    collapsed: &[Selection],
    mode: DesignerMode,
) -> Result<(Vec<SceneNode>, f32)> {
    let mut nodes = vec![b.button(
        0.,
        0.,
        240.,
        &format!("Mode: {mode:?} (switch)"),
        LocalAction::Mode(if mode == DesignerMode::Edit {
            DesignerMode::Test
        } else {
            DesignerMode::Edit
        }),
    )?];
    let mut y = ROW;
    let mut hidden_below = None;
    for (index, object) in objects.iter().enumerate() {
        if hidden_below.is_some_and(|depth| object.depth > depth) {
            continue;
        }
        hidden_below = None;
        let has_children = objects
            .get(index + 1)
            .is_some_and(|n| n.depth > object.depth);
        let closed = collapsed.contains(&object.selection);
        let indent = f32::from(object.depth) * 10.;
        let marker = if &object.selection == selected {
            "* "
        } else {
            ""
        };
        let mut row = b.button(
            0.,
            y,
            240.,
            &format!(
                "{marker}{}{}",
                " ".repeat(usize::from(object.depth) * 2),
                object.name
            ),
            LocalAction::Select(object.selection.clone()),
        )?;
        if has_children {
            // Separate disclosure control; selection always remains available.
            row.children.push(b.button(
                indent,
                0.,
                24.,
                if closed { "+" } else { "-" },
                LocalAction::Expand(object.selection.clone()),
            )?);
            if let Some(label) = row.children.get_mut(1) {
                label.frame.x = indent + 30.;
            }
        }
        nodes.push(row);
        y += ROW;
        if closed {
            hidden_below = Some(object.depth);
        }
    }
    if y > MAX_CONTENT {
        return Err("Full designer hierarchy exceeds coordinate budget".into());
    }
    Ok((nodes, y))
}
struct AuthoringTools<'a> {
    operations: &'a [(String, otrios_interfaces::oib::InstanceOperation)],
    delete_definition: Option<otrios_interfaces::oib::designer::DefinitionId>,
    open: bool,
    placement: Option<(&'a Selection, bool)>,
}
fn focus_hierarchy(
    b: &mut Builder,
    tree: &[HierarchyRow],
    selected: &Selection,
    collapsed: &[HierarchyKey],
    task: Task,
    mode: DesignerMode,
    tools: AuthoringTools<'_>,
) -> Result<(Vec<SceneNode>, f32)> {
    let AuthoringTools {
        operations: structure,
        delete_definition,
        open: palette_open,
        placement,
    } = tools;
    let mut nodes = vec![];
    let mut y = 0.;
    if mode == DesignerMode::Edit && !structure.is_empty() {
        let mut palette = b.focus_button(
            0.,
            y,
            288.,
            if palette_open {
                "Close insert palette"
            } else {
                "Insert component..."
            },
            LocalAction::Palette,
        )?;
        if let Some(semantics) = &mut palette.semantics {
            semantics.checked = palette_open;
        }
        if palette_open {
            style::set_surface_fill(&mut palette.children[0], b.palette.selected)?;
            if let Primitive::Text { color, .. } = &mut palette.children[1].primitive {
                *color = b.palette.background;
            }
        }
        nodes.push(palette);
        y += 56.;
        for (label, operation) in structure {
            if task == Task::Layers && !palette_open {
                continue;
            }
            if operation.operations.iter().any(|edit| {
                matches!(
                    edit,
                    otrios_interfaces::oib::authoring::DocumentEdit::Definition(
                        otrios_interfaces::oib::authoring::DefinitionEdit::DeleteDefinition { .. }
                    )
                )
            }) {
                continue;
            }
            let insert = operation.operations.iter().any(|edit| {
                matches!(
                    edit,
                    otrios_interfaces::oib::authoring::DocumentEdit::Hierarchy(
                        otrios_interfaces::oib::authoring::HierarchyEdit::Insert { .. }
                    )
                )
            });
            if insert && !palette_open {
                continue;
            }
            nodes.push(b.focus_button(
                0.,
                y,
                288.,
                label,
                LocalAction::Structure(operation.clone()),
            )?);
            y += 56.;
        }
    }
    if mode == DesignerMode::Edit
        && let Some(definition) = delete_definition
    {
        nodes.push(b.focus_button(
            0.,
            y,
            288.,
            "Delete component...",
            LocalAction::ReviewDefinition(definition),
        )?);
        y += 56.;
    }
    if let Some((selection, open)) = placement {
        nodes.push(b.focus_button(
            0.,
            y,
            288.,
            if open {
                "Hide placement"
            } else {
                "Edit placement"
            },
            LocalAction::Placement(selection.clone()),
        )?);
        y += 56.;
    }
    for row in tree {
        let mut parent = row.parent.as_ref();
        let mut depth = 0;
        let mut hidden = false;
        while let Some(key) = parent {
            hidden |= collapsed.contains(key);
            depth += 1;
            parent = tree
                .iter()
                .find(|r| &r.key == key)
                .and_then(|r| r.parent.as_ref());
        }
        if hidden {
            continue;
        }
        let indent = (depth as f32 * 16.).min(112.);
        let children = tree.iter().any(|r| r.parent.as_ref() == Some(&row.key));
        let mut node = if let Some(selection) = &row.selection {
            b.focus_row(
                0.,
                y,
                288.,
                &row.name,
                LocalAction::Select(selection.clone()),
            )?
        } else {
            let label = text(
                b.id()?,
                Rect::new(indent + 48., 16., (232. - indent).max(1.), 28.),
                &row.name,
                2,
                b.palette.muted,
            );
            canvas(
                b.id()?,
                Rect::new(0., y, 288., 52.),
                vec![background(b.id()?, 288., 52., b.palette.surface, 0.), label],
            )
        };
        if let Some(label) = node.children.get_mut(1) {
            label.frame.x = indent + 48.;
            label.frame.width = (232. - indent).max(1.);
        }
        let selected_row = row.selection.as_ref() == Some(selected);
        if selected_row {
            style::set_surface_fill(&mut node.children[0], b.palette.selected)?;
            if let Primitive::Text { color, .. } = &mut node.children[1].primitive {
                *color = b.palette.background;
            }
        }
        if children {
            let mut disclosure = b.focus_row(
                indent,
                0.,
                48.,
                if collapsed.contains(&row.key) {
                    ">"
                } else {
                    "v"
                },
                LocalAction::TreeExpand(row.key.clone()),
            )?;
            if selected_row {
                style::set_surface_fill(&mut disclosure.children[0], b.palette.selected)?;
                if let Primitive::Text { color, .. } = &mut disclosure.children[1].primitive {
                    *color = b.palette.background;
                }
            }
            node.children.push(disclosure);
        }
        nodes.push(node);
        y += 56.;
    }
    if y > MAX_CONTENT {
        return Err("Full designer hierarchy exceeds coordinate budget".into());
    }
    Ok((nodes, y))
}
fn value_label(value: &Value) -> String {
    match value {
        Value::Text(s) => s.clone(),
        Value::Number(n) => format!("{n}"),
        Value::Boolean(b) => b.to_string(),
        Value::Color(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3]),
        _ => "Unsupported".into(),
    }
}
fn valid_choice(p: &PropertyInfo, value: &Value) -> bool {
    use otrios_interfaces::layout::ValueType;
    match (&p.descriptor.value_type, value) {
        (ValueType::Number, Value::Number(n)) => {
            n.is_finite()
                && (0. ..=1000.).contains(n)
                && p.descriptor
                    .number_range
                    .is_none_or(|r| (r[0]..=r[1]).contains(n))
        }
        (ValueType::Text, Value::Text(s)) => !s.is_empty() && s.len() <= 96 && !s.contains('\0'),
        (ValueType::Boolean, Value::Boolean(_)) | (ValueType::Color, Value::Color(_)) => true,
        _ => false,
    }
}
fn choices(seed: &otrios_interfaces::oib::Seed, p: &PropertyInfo) -> Vec<Value> {
    let descriptor = &p.descriptor;
    if p.cell.is_none()
        || p.address.is_none()
        || !descriptor.policy.live_literal
        || descriptor.editor == PropertyEditor::ReadOnly
    {
        return vec![];
    }
    let mut result = match descriptor.editor {
        PropertyEditor::Toggle => vec![Value::Boolean(false), Value::Boolean(true)],
        PropertyEditor::NumberSuggestions
        | PropertyEditor::TextSuggestions
        | PropertyEditor::Palette => seed
            .cells
            .iter()
            .find(|c| Some(c.id) == p.cell)
            .map_or_else(Vec::new, |c| c.choices.clone()),
        PropertyEditor::ReadOnly => vec![],
    };
    result.retain(|value| {
        valid_choice(p, value)
            && matches!(
                (descriptor.editor, value),
                (PropertyEditor::Toggle, Value::Boolean(_))
                    | (PropertyEditor::NumberSuggestions, Value::Number(_))
                    | (PropertyEditor::TextSuggestions, Value::Text(_))
                    | (PropertyEditor::Palette, Value::Color(_))
            )
    });
    if descriptor.editor == PropertyEditor::NumberSuggestions
        && let (Some(step), Some(range), Some(Value::Number(current))) = (
            descriptor.number_step,
            descriptor.number_range,
            p.value.as_ref(),
        )
        && step.is_finite()
        && step > 0.
    {
        for n in [
            (*current - step).max(range[0]),
            (*current + step).min(range[1]),
        ] {
            let value = Value::Number(n);
            if valid_choice(p, &value) && !result.contains(&value) {
                result.push(value);
            }
        }
    }
    result
}
fn identity_lines(selection: &Selection) -> Vec<String> {
    match selection {
        Selection::Document(id) => vec![format!("Document #{}", id.0)],
        Selection::Definition(id) => vec![format!("Definition #{}", id.0)],
        Selection::DefinitionNode { definition, node } => {
            vec![format!("Definition #{} / node #{}", definition.0, node.0)]
        }
        Selection::Object(address) => {
            let mut lines = vec![format!(
                "Instance #{} / node #{}",
                address.instance.0, address.node.0
            )];
            if address.references.is_empty() {
                lines.push("Path: local root".into());
            }
            for refs in address.references.chunks(3) {
                lines.push(format!(
                    "Path: {}",
                    refs.iter()
                        .map(|id| id.0.to_string())
                        .collect::<Vec<_>>()
                        .join(" / ")
                ));
            }
            lines
        }
    }
}
fn inspector(
    b: &mut Builder,
    seed: &otrios_interfaces::oib::Seed,
    selection: &Selection,
    properties: &[PropertyInfo],
    conflicts: &[u64],
    status: &str,
    editable: bool,
) -> Result<(Vec<SceneNode>, f32)> {
    let mut nodes = vec![
        b.label(0., 0., "Selection / signed identity", WHITE)?,
        b.label(0., 28., status, ACCENT)?,
    ];
    let mut y = 60.;
    for line in identity_lines(selection) {
        nodes.push(b.label(0., y, &line, MUTED)?);
        y += 28.;
    }
    let mut group = None;
    // Runtime returns exposed properties first, followed by class ancestry.
    // Preserve that semantic order rather than deriving another class resolver.
    for p in properties {
        if group != Some(p.descriptor.group) {
            group = Some(p.descriptor.group);
            nodes.push(b.label(0., y, &format!("{:?}", p.descriptor.group), ACCENT)?);
            y += 28.;
        }
        nodes.push(b.label(
            0.,
            y,
            &format!(
                    "{}: {} {}",
                    p.descriptor.label,
                    p.value
                        .as_ref()
                        .map_or_else(|| "bound/source only".into(), value_label),
                    p.descriptor.units.as_deref().unwrap_or("")
                ),
            WHITE,
        )?);
        y += 28.;
        nodes.push(b.label(
            0.,
            y,
            &format!(
                "{} / {} / {}{}",
                p.origin,
                p.source,
                if p.overridden {
                    "live override"
                } else {
                    "inherited"
                },
                if p.cell.is_some_and(|c| conflicts.contains(&c)) {
                    " / CONFLICT (active retained)"
                } else {
                    ""
                }
            ),
            MUTED,
        )?);
        y += 28.;
        if let Some(inherited) = &p.inherited {
            nodes.push(b.label(
                0.,
                y,
                &format!("Inherit: {}", value_label(inherited)),
                MUTED,
            )?);
            y += 28.;
        }
        let values = choices(seed, p);
        if editable
            && p.cell.is_some()
            && p.descriptor.policy.live_literal
            && p.descriptor.editor != PropertyEditor::ReadOnly
            && let Some(address) = &p.address
        {
            for value in values {
                nodes.push(b.button(
                    0.,
                    y,
                    240.,
                    &format!("Set {}", value_label(&value)),
                    LocalAction::Write(address.clone(), Some(value)),
                )?);
                y += ROW;
            }
            if matches!(address, PropertyAddress::Occurrence { .. })
                && (p.overridden || p.cell.is_some_and(|c| conflicts.contains(&c)))
            {
                nodes.push(b.button(
                    0.,
                    y,
                    240.,
                    "Reset to inherit (causal clear)",
                    LocalAction::Write(address.clone(), None),
                )?);
                y += ROW;
            }
            if p.cell.is_some_and(|c| conflicts.contains(&c)) {
                nodes.push(b.button(
                    0.,
                    y,
                    240.,
                    "Resolve: explicit signed default",
                    LocalAction::Resolve(address.clone()),
                )?);
                y += ROW;
            }
        } else {
            nodes.push(b.label(0., y, "Read only / signed connection", MUTED)?);
            y += 28.;
        }
    }
    if properties.is_empty() {
        nodes.push(b.label(0., y, "Signed identity / no editable properties", MUTED)?);
        y += 28.;
    }
    if y > MAX_CONTENT {
        return Err("Designer inspector coordinate budget exceeded".into());
    }
    Ok((nodes, y))
}

impl Mount {
    pub(super) fn designer_editing(&self) -> bool {
        self.designer
            .as_ref()
            .is_some_and(|d| d.mode == DesignerMode::Edit)
    }
    pub(super) fn refresh_designer(&mut self) -> Result<bool> {
        if self.designer.is_none() {
            return Ok(false);
        }
        let blocked = self.oib_activation_blocked();
        let mut designer = self.designer.take().ok_or("Missing designer")?;
        let mut previous = None;
        let mut inspector_only = false;
        let previous_viewport = self.viewport;
        let previous_management = designer.management_visible();
        let previous_task = designer.workspace_task;
        let result = (|| {
            let mut changed = false;
            if !blocked && let Some(mode) = designer.pending_mode {
                previous = Some(designer.clone());
                changed |= designer.set_mode(mode, false);
            }
            if !blocked && let Some(viewport) = designer.pending_viewport.take() {
                previous.get_or_insert_with(|| designer.clone());
                self.viewport = viewport;
                designer.invalidate_bounds();
                changed = true;
            }
            if blocked && !designer.objects.is_empty() {
                return Ok(changed);
            }
            let root = self
                .design_doc()
                .oib_root()
                .ok_or("Missing designer state")?;
            let editable = self.binding.as_ref().is_none_or(|b| b.ui_editable);
            // Status formatting walks authored source and capacity metadata.
            // Its inputs are immutable OIB state plus the cheap local flight
            // flag; do not repeat that work on unchanged owner ticks.
            let revision = (
                root,
                self.doc.oib_root(),
                self.doc.confirmed(),
                self.doc.oib_saving(),
                editable,
            );
            if designer.revision.as_ref() == Some(&revision) {
                return Ok(changed);
            }
            let _profile = crate::canvas::profiling::span("client.designer.rebuild");
            let status = self
                .editor_status()
                .unwrap_or_else(|| self.design_doc().oib_status_text());
            if status.contains("Structure rejected:") {
                designer.editor_notice = Some(status.clone());
            } else if designer
                .editor_notice
                .as_ref()
                .is_some_and(|s| s.contains("Structure rejected:"))
            {
                designer.editor_notice = None;
            }
            previous.get_or_insert_with(|| designer.clone());
            let authoring_seed = self.design_doc().oib_authoring_seed()?;
            let seed = &authoring_seed;
            let allocations = self.design_doc().oib_allocations()?;
            if self.oib_paint_refresh
                && !changed
                && designer.revision.is_some()
                && designer.pending_selection.is_none()
                && designer.pending_source.is_none()
                && designer.details.is_none()
                && designer.registers.is_empty()
                && self.design_doc().oib_conflicts().is_empty()
            {
                let mut properties = super::super::oib::document::properties(
                    seed,
                    &allocations,
                    &designer.selection,
                    &self.design_doc().oib_active()?,
                )?;
                properties.extend(
                    self.design_doc()
                        .oib_structural_properties(&designer.selection)?,
                );
                if designer.placement_open
                    && let Selection::Object(object) = &designer.selection
                    && let Ok(placement) = self.design_doc().oib_placement_properties(object)
                {
                    properties.extend(placement);
                }
                designer.editor_revision = self
                    .working_copy
                    .as_ref()
                    .map_or_else(|| self.doc.oib_generation().unwrap_or(0), |w| w.revision);
                designer.editor_structural_revision = self.working_copy.as_ref().map_or_else(
                    || self.doc.oib_structural_generation().unwrap_or(0),
                    |w| w.revision,
                );
                designer.editor_structural_generation =
                    self.design_doc().oib_structural_generation().unwrap_or(0);
                designer.rebuild_panels(
                    seed,
                    designer.objects.clone(),
                    &properties,
                    &[],
                    &status,
                    editable,
                    false,
                )?;
                designer.revision = Some(revision);
                inspector_only = true;
                return Ok(true);
            }
            let (source_ui, _) = self.design_doc().oib_compiled(self.oib_selected)?;
            let source_seed =
                super::super::oib::seed(&source_ui).ok_or("Missing source presentation")?;
            let objects = super::super::oib::document::objects(source_seed, &allocations)?;
            if let Some(target) = designer.pending_selection.take() {
                if objects.iter().any(|o| o.selection == target) {
                    designer.select(target);
                    designer.details = None;
                    designer.editor_notice = None;
                } else {
                    designer.editor_notice = Some("Selection is no longer available".into());
                }
            }
            if let Some(address) = designer.pending_source.take() {
                let target = editing::selection(&address);
                let trace = super::super::oib::document::property_sources(
                    seed,
                    &allocations,
                    &address,
                    &self.design_doc().oib_active()?,
                );
                if objects.iter().any(|o| o.selection == target) && trace.is_ok() {
                    designer.select(target);
                    designer.details = Some(trace?);
                    designer.workspace_task = Task::Inspect;
                    designer.editor_notice = None;
                } else {
                    designer.editor_notice = Some("Source is no longer available".into());
                }
            }

            if designer.focus_layout {
                let live = self
                    .design_doc()
                    .oib_instances()?
                    .into_iter()
                    .filter(|i| i.active)
                    .map(|i| i.id)
                    .collect();
                let mut tree = super::super::oib::document::hierarchy(
                    seed,
                    &allocations,
                    &live,
                    designer.tree_scope,
                )?;
                if designer.workspace_task == Task::Layers
                    && let Some(definition) = designer.definition_scope()
                {
                    // Keep the admitted definition's ownership/visual hierarchy,
                    // without unrelated library roots or composition instances.
                    tree.retain(|row| {
                        matches!(row.selection,
                        Some(Selection::Definition(id))
                        | Some(Selection::DefinitionNode { definition: id, .. })
                            if id == definition)
                    });
                }
                if !designer.tree_initialized.contains(&designer.tree_scope) {
                    let mut first_panel = true;
                    for row in &tree {
                        let close = match &row.selection {
                            Some(Selection::Object(a)) if row.component.is_some() => {
                                if !a.references.is_empty() {
                                    true
                                } else if a.instance
                                    == seed
                                        .designer
                                        .as_ref()
                                        .ok_or("Missing designer metadata")?
                                        .presented_root
                                {
                                    false
                                } else {
                                    let close = !first_panel;
                                    first_panel = false;
                                    close
                                }
                            }
                            Some(Selection::Definition(_)) => true,
                            _ => false,
                        };
                        if close {
                            designer.tree_closed.push(row.key.clone());
                        }
                    }
                    designer.tree_initialized.push(designer.tree_scope);
                }
                if designer.workspace_task == Task::Layers
                    && designer.definition_scope().is_some()
                    && designer.reveal_selection
                {
                    // A newly created definition must reveal its root and children
                    // without borrowing the Library's collapsed-root state.
                    for row in &tree {
                        if matches!(row.selection, Some(Selection::Definition(_)))
                            || row.owner.is_none()
                        {
                            designer.tree_closed.retain(|key| key != &row.key);
                        }
                    }
                }
                if designer.reveal_selection {
                    let mut parent = tree
                        .iter()
                        .find(|r| r.selection.as_ref() == Some(&designer.selection))
                        .and_then(|r| r.parent.clone());
                    while let Some(key) = parent {
                        designer.tree_closed.retain(|c| c != &key);
                        parent = tree
                            .iter()
                            .find(|r| r.key == key)
                            .and_then(|r| r.parent.clone());
                    }
                    designer.reveal_selection = false;
                }
                designer.tree = tree;
            }

            if !objects.iter().any(|o| o.selection == designer.selection) {
                designer.select(Selection::Document(
                    seed.designer
                        .as_ref()
                        .ok_or("Missing designer metadata")?
                        .id,
                ));
            }
            if let Selection::Object(object) = &designer.selection
                && allocations
                    .iter()
                    .any(|allocation| allocation.id == object.instance.0)
            {
                self.oib_selected = Some(object.instance.0);
                if let Some(view) = &mut self.oib_view {
                    view.info.selected_instance = self.oib_selected;
                }
            }
            designer.editor_revision = self
                .working_copy
                .as_ref()
                .map_or_else(|| self.doc.oib_generation().unwrap_or(0), |w| w.revision);
            designer.editor_structural_revision = self.working_copy.as_ref().map_or_else(
                || self.doc.oib_structural_generation().unwrap_or(0),
                |w| w.revision,
            );
            designer.editor_structural_generation =
                self.design_doc().oib_structural_generation().unwrap_or(0);
            designer.structure_tools.clear();
            designer.management = None;
            if editable && designer.mode == DesignerMode::Edit
                && let Selection::Object(address) = &designer.selection
                && let Some(allocation) = allocations.iter().find(|a| a.id == address.instance.0)
                && self.design_doc().oib_canvas_factories()?.iter().any(|(id, _)| *id == allocation.prototype)
                && let Some(root) = objects.iter().find(|o| matches!(&o.selection, Selection::Object(a) if a.instance == address.instance && a.references.is_empty()))
            {
                designer.management = Some(ManagedComponent { instance: allocation.id, root: root.selection.clone() });
            }
            // Structural candidates can run full admission. Prepare them only
            // while the Library or Layers task is visible.
            if editable
                && self.doc.confirmed()
                && designer.mode == DesignerMode::Edit
                && (!designer.focus_layout || designer.workspace_task != Task::Inspect)
            {
                use otrios_interfaces::oib::InstanceOperation;
                let generation = self.design_doc().oib_structural_generation().unwrap_or(0);
                if designer.graph_tools.as_ref().is_none_or(|cache| {
                    cache.generation != generation || cache.selection != designer.selection
                }) {
                    designer.graph_tools = Some(ToolCache {
                        generation,
                        selection: designer.selection.clone(),
                        operations: authoring::tools(
                            self.design_doc(),
                            seed,
                            &designer.selection,
                            &allocations,
                        )?,
                    });
                }
                if let Some(cache) = &designer.graph_tools {
                    designer
                        .structure_tools
                        .extend(cache.operations.iter().cloned());
                }
                let destination = if let Selection::Object(a) = &designer.selection {
                    self.design_doc().oib_destination(a.instance.0)?
                } else {
                    otrios_interfaces::oib::authoring::Destination::Canvas
                };
                let all_roots: Vec<_> = self
                    .design_doc()
                    .oib_instances()?
                    .into_iter()
                    .filter(|i| i.active)
                    .collect();
                let roots: Vec<_> = all_roots
                    .iter()
                    .filter(|i| {
                        self.design_doc()
                            .oib_destination(i.id)
                            .is_ok_and(|d| d == destination)
                    })
                    .cloned()
                    .collect();
                let selected = match &designer.selection {
                    Selection::Object(a) if a.references.is_empty() => roots.iter().position(|i| {
                        i.id == a.instance.0
                            && designer.root_selection(i.id).as_ref() == Some(&designer.selection)
                    }),
                    _ => None,
                };
                for (prototype, name) in
                    self.design_doc().oib_canvas_factories().unwrap_or_default()
                {
                    if self.design_doc().oib_available_slots_for(prototype) > 0 {
                        designer.structure_tools.push((
                            format!("Insert {name}"),
                            InstanceOperation::insert(generation, prototype,
                                selected.filter(|_| destination == otrios_interfaces::oib::authoring::Destination::Canvas).map(|index| roots[index].id), None),
                        ));
                    }
                }
                if let Some(index) = selected {
                    let instance = roots[index].id;
                    let reorder = |before| InstanceOperation::reorder(generation, instance, before);
                    if index > 0 {
                        designer
                            .structure_tools
                            .push(("Move earlier".into(), reorder(Some(roots[index - 1].id))));
                    }
                    if index + 1 < roots.len() {
                        designer.structure_tools.push((
                            "Move later".into(),
                            reorder(roots.get(index + 2).map(|i| i.id)),
                        ));
                    }
                }
                if let Selection::Object(component) = &designer.selection {
                    use otrios_interfaces::oib::authoring::{
                        Destination, HierarchyBatch, HierarchyEdit,
                    };
                    let operation = |edit| HierarchyBatch {
                        generation,
                        operations: vec![edit],
                    };
                    for (slot_name, destination) in super::super::oib::document::placement::slots(
                        seed,
                        &allocations,
                        component,
                    )? {
                        for (prototype, name) in
                            self.design_doc().oib_canvas_factories().unwrap_or_default()
                        {
                            if self.design_doc().oib_available_slots_for(prototype) == 0 {
                                continue;
                            }
                            let batch = operation(HierarchyEdit::Insert {
                                prototype: InstanceId(prototype),
                                name: None,
                                destination: destination.clone(),
                                before: None,
                                frame: None,
                            });
                            if self.design_doc().oib_hierarchy_allowed(&batch) {
                                designer.structure_tools.push((
                                    format!("Into {slot_name}: {name}"),
                                    InstanceOperation::hierarchy(batch),
                                ));
                            }
                        }
                    }
                    if let Some(index) = selected {
                        let current = &roots[index];
                        let numbers = self.design_doc().oib_placement_properties(component)?;
                        let mut frame = [0.; 4];
                        for (index, info) in numbers.iter().enumerate() {
                            if let Some(Value::Number(n)) = info.value {
                                frame[index] = n;
                            }
                        }
                        let canvas_count = all_roots
                            .iter()
                            .filter(|i| {
                                self.design_doc()
                                    .oib_destination(i.id)
                                    .is_ok_and(|d| d == Destination::Canvas)
                            })
                            .count();
                        let pitch = seed
                            .instance_surface
                            .as_ref()
                            .ok_or("Missing factory surface")?
                            .preview_stride;
                        let canvas_frame = [0., canvas_count as f32 * pitch, frame[2], frame[3]];
                        if destination != Destination::Canvas {
                            let batch = operation(HierarchyEdit::Move {
                                instance: InstanceId(current.id),
                                destination: Destination::Canvas,
                                before: None,
                                frame: Some(canvas_frame),
                            });
                            if self.design_doc().oib_hierarchy_allowed(&batch) {
                                designer.structure_tools.push((
                                    "Move to canvas".into(),
                                    InstanceOperation::hierarchy(batch),
                                ));
                            }
                        }
                        for parent in &all_roots {
                            if parent.id == current.id {
                                continue;
                            }
                            let Some(parent_object) =
                                objects.iter().find_map(|o| match &o.selection {
                                    Selection::Object(a)
                                        if a.instance.0 == parent.id && a.references.is_empty() =>
                                    {
                                        Some(a)
                                    }
                                    _ => None,
                                })
                            else {
                                continue;
                            };
                            for (slot_name, target) in
                                super::super::oib::document::placement::slots(
                                    seed,
                                    &allocations,
                                    parent_object,
                                )?
                            {
                                if target == destination {
                                    continue;
                                }
                                let batch = operation(HierarchyEdit::Move {
                                    instance: InstanceId(current.id),
                                    destination: target,
                                    before: None,
                                    frame: Some([0., 0., frame[2], frame[3]]),
                                });
                                if self.design_doc().oib_hierarchy_allowed(&batch) {
                                    designer.structure_tools.push((
                                        format!("{}: {slot_name}", parent.name),
                                        InstanceOperation::hierarchy(batch),
                                    ));
                                }
                            }
                        }
                        let batch = operation(HierarchyEdit::Duplicate {
                            instance: InstanceId(current.id),
                            name: None,
                            destination: Destination::Canvas,
                            before: None,
                            frame: Some(canvas_frame),
                        });
                        if self.design_doc().oib_hierarchy_allowed(&batch) {
                            designer.structure_tools.push((
                                "Duplicate subtree".into(),
                                InstanceOperation::hierarchy(batch),
                            ));
                        }
                    }
                }
                if let Selection::DefinitionNode { definition, node } = &designer.selection
                    && let Some(def) = seed
                        .designer
                        .as_ref()
                        .and_then(|d| d.definitions.iter().find(|d| d.id == *definition))
                    && let Some(parent) =
                        def.nodes.iter().find(|n| n.visual_children.contains(node))
                    && let Some(index) = parent.visual_children.iter().position(|n| n == node)
                    && let Some(owner) = def
                        .nodes
                        .iter()
                        .find(|n| n.id == *node)
                        .and_then(|n| n.owner)
                {
                    use otrios_interfaces::oib::authoring::{DefinitionBatch, DefinitionEdit};
                    let operation = |before| {
                        InstanceOperation::definitions(DefinitionBatch {
                            generation,
                            operations: vec![DefinitionEdit::MoveNode {
                                definition: *definition,
                                node: *node,
                                owner,
                                visual_parent: Some(parent.id),
                                before,
                            }],
                        })
                    };
                    if index > 0 {
                        designer.structure_tools.push((
                            "Move shared earlier".into(),
                            operation(Some(parent.visual_children[index - 1])),
                        ));
                    }
                    if index + 1 < parent.visual_children.len() {
                        designer.structure_tools.push((
                            "Move shared later".into(),
                            operation(parent.visual_children.get(index + 2).copied()),
                        ));
                    }
                }
            }

            let mut properties = super::super::oib::document::properties(
                seed,
                &allocations,
                &designer.selection,
                &self.design_doc().oib_active()?,
            )?;
            properties.extend(
                self.design_doc()
                    .oib_structural_properties(&designer.selection)?,
            );
            if designer.placement_open
                && let Selection::Object(object) = &designer.selection
                && let Ok(placement) = self.design_doc().oib_placement_properties(object)
            {
                properties.extend(placement);
            }
            designer.connection_tools.clear();
            if editable && designer.mode == DesignerMode::Edit {
                for property in &properties {
                    if property.descriptor.policy.app_binding
                        && let Some(address) = &property.address
                    {
                        let tools = self.design_doc().oib_connection_choices(address)?;
                        if !tools.is_empty() {
                            designer.connection_tools.push((address.clone(), tools));
                        }
                    }
                }
            }
            designer.api_model = if designer.api_open {
                api_editor::model(
                    self.design_doc(),
                    seed,
                    &designer.selection,
                    designer.api_property.as_ref(),
                    editable && designer.mode == DesignerMode::Edit,
                )?
            } else {
                None
            };
            let conflicts = self.design_doc().oib_conflicts();
            designer.registers = properties
                .iter()
                .filter(|p| p.cell.is_some_and(|c| conflicts.contains(&c)))
                .filter_map(|p| p.address.as_ref())
                .map(|a| self.design_doc().oib_property_register(a))
                .collect::<Result<_>>()?;
            if let Some(address) = designer.details.as_ref().map(|d| d.address.clone()) {
                designer.details = editing::sources(self.design_doc(), seed, &address).ok();
            }
            // Dynamic occurrence cells carry the signed prototype's choices.
            // Their IDs are absent from the unexpanded seed; reuse the runtime
            // expansion rather than deriving a second factory/path lookup here.
            let expanded = super::super::oib::instances::expanded_seed(seed, &allocations)?;
            designer.rebuild(
                &expanded,
                objects,
                &properties,
                &self.design_doc().oib_conflicts(),
                &status,
                editable,
            )?;
            designer.revision = Some(revision);
            Ok(true)
        })();
        self.designer = Some(designer);
        let result = result.and_then(|changed| {
            if changed {
                let layout = if inspector_only {
                    cached_layout(self)?
                } else {
                    Arc::new(resolve_layout(self)?)
                };
                if let Some(designer) = &mut self.designer {
                    designer.sync(&layout)?;
                }
                present_scene(self, &layout)?;
            }
            Ok(changed)
        });
        if result.is_err() {
            self.viewport = previous_viewport;
        }
        if result.is_err()
            && let Some(previous) = previous
        {
            self.designer = Some(previous);
        }
        if result.as_ref().is_ok_and(|changed| *changed) {
            if self.viewport != previous_viewport
                || self
                    .designer
                    .as_ref()
                    .is_some_and(|d| d.workspace_task != previous_task)
                || self
                    .designer
                    .as_ref()
                    .is_some_and(|d| d.management_visible() != previous_management)
            {
                self.invalidate();
            } else {
                self.invalidate_presentation();
            }
        }
        result
    }
}
impl Client {
    /// Local typed scope, including nonvisual objects and definition nodes.
    pub fn inspect_oib_selection(&mut self, selection: Selection) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.oib_activation_blocked() {
            return Ok(false);
        }
        let Some(designer) = &mut m.designer else {
            return Ok(false);
        };
        if !designer.objects.iter().any(|o| o.selection == selection) {
            return Err("Unknown designer selection".into());
        }
        let previous = designer.clone();
        let reveal_placement = designer.drafts.drafts().iter().any(|d| d.dirty
            && matches!(&d.address, PropertyAddress::Placement { object, .. } if Selection::Object(object.clone()) == selection));
        if reveal_placement {
            designer.placement_open = true;
            designer.revision = None;
        }
        let changed = designer.select(selection) || reveal_placement;
        if changed && previous.workspace_task != Task::Inspect {
            // Selecting a layer opens Inspect, which changes the projected dock.
            // Discard the cached task geometry before synchronizing its bounds.
            m.invalidate();
        }
        if changed && let Err(error) = m.refresh() {
            m.designer = Some(previous);
            return Err(error);
        }
        Ok(changed)
    }
    pub fn insert_oib(&mut self, prototype: u64, before: Option<u64>) -> Result<bool> {
        let generation = self
            .active
            .as_ref()
            .ok_or("No Canvas")?
            .doc
            .oib_structural_generation()
            .ok_or("Missing OIB generation")?;
        self.edit_oib_structure(otrios_interfaces::oib::InstanceOperation::insert(
            generation, prototype, before, None,
        ))
    }
    pub fn reorder_oib(&mut self, instance: u64, before: Option<u64>) -> Result<bool> {
        if before == Some(instance) {
            return Ok(false);
        }
        let generation = self
            .active
            .as_ref()
            .ok_or("No Canvas")?
            .doc
            .oib_structural_generation()
            .ok_or("Missing OIB generation")?;
        self.edit_oib_structure(otrios_interfaces::oib::InstanceOperation::reorder(
            generation, instance, before,
        ))
    }
    /// Submit an explicitly captured shared-definition batch through the same
    /// edit-mode, input, draft and Space admission gates as the visible tools.
    pub fn edit_oib_definitions(
        &mut self,
        batch: otrios_interfaces::oib::authoring::DefinitionBatch,
    ) -> Result<bool> {
        self.edit_oib_structure(otrios_interfaces::oib::InstanceOperation::definitions(
            batch,
        ))
    }
    pub fn edit_oib_hierarchy(
        &mut self,
        batch: otrios_interfaces::oib::authoring::HierarchyBatch,
    ) -> Result<bool> {
        self.edit_oib_structure(otrios_interfaces::oib::InstanceOperation::hierarchy(batch))
    }
    fn edit_oib_structure(
        &mut self,
        operation: otrios_interfaces::oib::InstanceOperation,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable)
            || m.oib_activation_blocked()
            || m.designer.as_ref().is_some_and(|d| {
                d.mode != DesignerMode::Edit
                    || d.editor_focused()
                    || d.drafts.drafts().iter().any(|draft| draft.dirty)
            })
        {
            return Ok(false);
        }
        let changed = m.doc.edit_oib_structure(operation)?;
        if changed {
            m.refresh()?;
            m.invalidate();
        }
        Ok(changed)
    }
    fn designer_delete_component(&mut self, instance: u64) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable)
            || m.oib_activation_blocked()
            || !m.designer.as_ref().is_some_and(|d| {
                d.mode == DesignerMode::Edit
                    && d.management
                        .as_ref()
                        .is_some_and(|t| t.instance == instance)
            })
        {
            return Ok(false);
        }
        let live = m
            .design_doc()
            .oib_instances()?
            .iter()
            .any(|i| i.id == instance && i.active);
        if live {
            return self.designer_review_instance_delete(
                otrios_interfaces::oib::designer::InstanceId(instance),
            );
        }
        m.discard_removed_occurrence_drafts(&[otrios_interfaces::oib::designer::InstanceId(
            instance,
        )])?;
        Ok(false)
    }
    /// Finish a field before hiding Inspect. The older general blur route keeps
    /// invalid text but clears focus, so restore that owner when Apply failed.
    fn designer_blur_workspace_field(&mut self) -> Result<bool> {
        let focused = self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .and_then(|d| {
                let focus = d.drafts.focused()?;
                let field = d.drafts.draft(&focus.address)?.field(focus.part)?;
                Some((focus.address.clone(), field.id))
            });
        self.designer_blur_focused()?;
        if let Some((address, id)) = focused {
            let m = self.active.as_mut().ok_or("No Canvas")?;
            let d = m.designer.as_mut().ok_or("Missing designer")?;
            if d.drafts.draft(&address).is_some_and(|draft| draft.dirty) {
                d.drafts
                    .focus(id)
                    .map_err(|e| format!("Property focus: {e:?}"))?;
                m.keyboard.focus();
                m.invalidate();
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// Queue a local mode switch while any gesture/edit owns the current graph.
    pub fn set_oib_designer_mode(&mut self, mode: DesignerMode) -> Result<bool> {
        if self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .is_some_and(|d| d.mode == mode && d.pending_mode.is_none())
        {
            return Ok(false);
        }
        let busy = self.input_busy();
        // A click on the mode control ends ordinary field focus. Preserve a
        // captured pointer, an invalid draft, and the existing receipt barrier.
        if !busy && !self.designer_blur_workspace_field()? {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let blocked = busy || m.oib_activation_blocked();
        let Some(designer) = &mut m.designer else {
            return Ok(false);
        };
        let previous = designer.clone();
        let changed = designer.set_mode(mode, blocked);
        if changed {
            if let Err(error) = m.sync_designer_preview() {
                m.designer = Some(previous);
                return Err(error);
            }
            m.refresh_designer()?;
            m.invalidate_presentation();
        }
        Ok(changed)
    }
    fn designer_stage_structure(
        &mut self,
        operation: otrios_interfaces::oib::InstanceOperation,
    ) -> Result<bool> {
        if self.input_busy() || !self.designer_blur_workspace_field()? {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable)
            || m.oib_activation_blocked()
            || !m.designer_editing()
        {
            return Ok(false);
        }
        let created = operation.operations.iter().find_map(|op| match op {
            otrios_interfaces::oib::authoring::DocumentEdit::Definition(
                otrios_interfaces::oib::authoring::DefinitionEdit::CreateDefinition {
                    definition,
                    ..
                },
            ) => Some(*definition),
            _ => None,
        });
        let changed = m.stage_structure(operation)?;
        if changed && let Some(definition) = created {
            let d = m.designer.as_mut().ok_or("Missing designer")?;
            d.pending_selection = Some(Selection::Definition(definition));
            d.revision = None;
            m.refresh()?;
        }
        Ok(changed)
    }
    fn designer_new_component(&mut self) -> Result<bool> {
        let m = self.active.as_ref().ok_or("No Canvas")?;
        let seed = m.design_doc().oib_authoring_seed()?;
        let Some(op) =
            authoring::new_component(m.design_doc(), &seed, &m.design_doc().oib_allocations()?)?
        else {
            return Ok(false);
        };
        self.designer_stage_structure(op)
    }
    fn designer_history(&mut self, redo: bool) -> Result<bool> {
        if self.input_busy() || !self.designer_blur_workspace_field()? {
            return Ok(false);
        };
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if !m.designer_editing() || m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        };
        m.sync_working_copy()?;
        m.remember_edit_location();
        let w = m.working_copy.as_mut().ok_or("Missing working copy")?;
        let changed = if redo { w.redo()? } else { w.undo()? };
        let location = w.location();
        let review = w.history_review().is_some();
        if review {
            m.show_history_review();
            return Ok(true);
        }
        if changed {
            if let Some(location) = &location {
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                d.pending_selection = Some(location.selection.clone());
            }
            m.working_changed()?;
            if let Some(location) = location {
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                d.workspace_task = location.task;
                if location.task != Task::Inspect {
                    d.tree_scope = d.task_scope(location.task);
                }
                d.revision = None;
                m.refresh()?;
                m.invalidate();
            }
        }
        Ok(changed)
    }
    pub(super) fn designer_discard_working_copy(&mut self) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        };
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.working_copy.as_ref().is_some_and(|w| w.applying()) {
            return Ok(false);
        };
        if let Some(working) = &mut m.working_copy {
            working.reset_to_live(&m.doc)?;
        } else {
            m.working_copy = Some(super::oib_working_copy::WorkingCopy::new(&m.doc)?);
        }
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        let addresses: Vec<_> = d
            .drafts
            .drafts()
            .iter()
            .map(|f| f.address.clone())
            .collect();
        for address in addresses {
            d.drafts.cancel(&address);
        }
        d.details = None;
        d.editor_notice = None;
        m.keyboard.hide();
        m.working_changed()?;
        Ok(true)
    }
    pub(super) fn designer_apply_working_copy(&mut self) -> Result<bool> {
        if self.input_busy() || !self.designer_blur_workspace_field()? {
            return Ok(false);
        };
        let addresses: Vec<_> = self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .ok_or("Missing designer")?
            .drafts
            .drafts()
            .iter()
            .filter(|d| d.dirty)
            .map(|d| d.address.clone())
            .collect();
        for address in addresses {
            self.designer_commit_property(&address, false)?;
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if !m.designer_editing()
            || !m.doc.confirmed()
            || m.binding.as_ref().is_some_and(|b| !b.ui_editable)
            || m.designer.as_ref().is_some_and(|d| {
                d.drafts
                    .drafts()
                    .iter()
                    .any(|f| f.dirty || f.error.is_some())
            })
        {
            return Ok(false);
        };
        let changed = m
            .working_copy
            .as_mut()
            .ok_or("Missing working copy")?
            .submit(&mut m.doc)?;
        if let Some(d) = &mut m.designer {
            d.revision = None;
        }
        m.refresh()?;
        m.invalidate_presentation();
        Ok(changed)
    }
    fn designer_action(&mut self, action: LocalAction) -> Result<bool> {
        if self.active.as_ref().is_some_and(Mount::local_modal_open) {
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.working_copy.as_ref().is_some_and(|w| w.applying()))
            && !matches!(
                action,
                LocalAction::Workspace(_)
                    | LocalAction::Appearance(_)
                    | LocalAction::Mode(_)
                    | LocalAction::Select(_)
                    | LocalAction::TreeExpand(_)
                    | LocalAction::Expand(_)
            )
        {
            return Ok(false);
        }
        match action {
            LocalAction::ApiOpen(_) | LocalAction::ApiProperty(_) => {
                if self.input_busy() {
                    return Ok(false);
                }
                self.designer_blur_focused()?;
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                match action {
                    LocalAction::ApiOpen(open) => {
                        d.api_open = open;
                        d.api_property = None;
                    }
                    LocalAction::ApiProperty(property) => d.api_property = property,
                    _ => unreachable!(),
                }
                d.revision = None;
                m.refresh_designer()?;
                m.invalidate_presentation();
                Ok(true)
            }
            LocalAction::NewComponent => self.designer_new_component(),
            LocalAction::Undo => self.designer_history(false),
            LocalAction::Redo => self.designer_history(true),
            LocalAction::DiscardWorkingCopy => self.designer_discard_working_copy(),
            LocalAction::ApplyWorkingCopy => self.designer_apply_working_copy(),
            LocalAction::EditComponent(selection) => {
                if self.input_busy() {
                    return Ok(false);
                }
                self.designer_select_after_blur(selection)?;
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if let Some(d) = &mut m.designer
                    && let Some(panel) = d.panels.last_mut()
                {
                    panel.scroll.set_offset([0., 0.]);
                }
                m.invalidate_presentation();
                Ok(true)
            }
            LocalAction::Placement(selection) => {
                let was_open = self
                    .active
                    .as_ref()
                    .and_then(|m| m.designer.as_ref())
                    .is_some_and(|d| d.placement_open && d.selection == selection);
                self.designer_select_after_blur(selection)?;
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                d.placement_open = !was_open;
                d.revision = None;
                m.refresh_designer()?;
                m.invalidate_presentation();
                Ok(true)
            }
            LocalAction::DeleteComponent(instance) => self.designer_delete_component(instance),
            LocalAction::Details(address) => self.designer_details(&address),
            LocalAction::Source(address) => self.designer_go_source(&address),
            LocalAction::Choose(view, value) => self.designer_choose_property(&view, value),
            LocalAction::PickColor(address, color) => {
                self.designer_preview_color(&address, Some(color), None, None)
            }
            LocalAction::Opacity(address, opacity) => {
                self.designer_preview_color(&address, None, Some(opacity), None)
            }
            LocalAction::DraftBool(address, value) => {
                self.designer_preview_color(&address, None, None, Some(value))
            }
            LocalAction::FocusField(id) => self.designer_focus_field(id),
            LocalAction::Apply(address) => self.designer_commit_property(&address, false),
            LocalAction::Discard(address) => self.designer_discard_property(&address),
            LocalAction::Select(selection) => self.designer_select_after_blur(selection),
            LocalAction::TreeExpand(key) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                if let Some(index) = d.tree_closed.iter().position(|k| k == &key) {
                    d.tree_closed.remove(index);
                } else {
                    d.tree_closed.push(key);
                }
                d.revision = None;
                m.refresh_designer()?;
                Ok(true)
            }
            LocalAction::Palette => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                d.palette_open = !d.palette_open;
                d.revision = None;
                m.refresh_designer()?;
                Ok(true)
            }
            LocalAction::ReviewDefinition(definition) => self.designer_review_delete(definition),
            LocalAction::ReviewProperty(definition, property) => {
                self.designer_review_property_delete(definition, property)
            }
            LocalAction::Structure(operation) => {
                if let Some(definition) = operation.operations.iter().find_map(|edit| match edit {
                    otrios_interfaces::oib::authoring::DocumentEdit::Definition(
                        otrios_interfaces::oib::authoring::DefinitionEdit::DeleteDefinition {
                            definition,
                        },
                    ) => Some(*definition),
                    _ => None,
                }) {
                    self.designer_review_delete(definition)
                } else {
                    self.designer_stage_structure(operation)
                }
            }
            LocalAction::Workspace(task) => {
                if self.input_busy() {
                    return Ok(false);
                }
                if self
                    .active
                    .as_ref()
                    .and_then(|m| m.designer.as_ref())
                    .is_some_and(|d| d.workspace_task == task)
                {
                    return Ok(false);
                }
                if task != Task::Inspect {
                    if !self.designer_blur_workspace_field()? {
                        return Ok(false);
                    }
                    // Failed validation keeps its field visible and focused.
                    if self
                        .active
                        .as_ref()
                        .and_then(|m| m.designer.as_ref())
                        .is_some_and(Designer::editor_focused)
                    {
                        return Ok(false);
                    }
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                if d.workspace_task == task {
                    return Ok(false);
                }
                d.workspace_task = task;
                if task != Task::Inspect {
                    d.tree_scope = d.task_scope(task);
                    d.reveal_selection = task == Task::Layers;
                }
                d.revision = None;
                d.invalidate_bounds();
                m.refresh_designer()?;
                m.invalidate();
                Ok(true)
            }
            LocalAction::LayoutNoop => Ok(false),
            LocalAction::LayoutBounds(open) => {
                if self.input_busy() || !self.designer_blur_workspace_field()? {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                m.designer.as_mut().ok_or("Missing designer")?.layout_bounds = open;
                editing::repaint(m)?;
                Ok(true)
            }
            LocalAction::LayoutPadding(independent, normalize) => {
                if self.input_busy() || !self.designer_blur_workspace_field()? {
                    return Ok(false);
                }
                if let Some(operation) = normalize
                    && !self.designer_stage_structure(operation)?
                {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                m.designer
                    .as_mut()
                    .ok_or("Missing designer")?
                    .padding_independent = independent;
                editing::repaint(m)?;
                Ok(true)
            }
            LocalAction::Appearance(appearance) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                if d.appearance == appearance {
                    return Ok(false);
                }
                d.appearance = appearance;
                d.revision = None;
                m.refresh_designer()?;
                m.invalidate();
                Ok(true)
            }
            LocalAction::Mode(mode) => self.set_oib_designer_mode(mode),
            LocalAction::Expand(selection) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                if let Some(index) = d.collapsed.iter().position(|s| s == &selection) {
                    d.collapsed.remove(index);
                } else {
                    d.collapsed.push(selection);
                }
                d.revision = None;
                m.refresh_designer()?;
                Ok(true)
            }
            LocalAction::RestoreDesign(address, generation) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
                    return Ok(false);
                }
                if Some(generation) != m.design_doc().oib_structural_generation() {
                    return Ok(false);
                }
                let changed = m.stage_property(&address, None)?;
                if changed {
                    if let Some(d) = &mut m.designer {
                        d.drafts.cancel(&address);
                    }
                    m.refresh()?;
                    m.invalidate_presentation();
                }
                Ok(changed)
            }
            LocalAction::Write(address, value) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
                    return Ok(false);
                }
                let reset = value.is_none();
                let changed = m.stage_property(&address, value)?;
                if changed
                    && reset
                    && let Some(d) = &mut m.designer
                {
                    d.drafts.cancel(&address);
                    if !d.editor_focused() {
                        m.keyboard.hide();
                    }
                }
                if changed {
                    m.refresh()?;
                    m.invalidate_presentation();
                }
                Ok(changed)
            }
            LocalAction::Resolve(address) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
                    return Ok(false);
                }
                let authoring_seed = m.design_doc().oib_authoring_seed()?;
                let seed = &authoring_seed;
                let cell = super::super::oib::document::cell_for_address(
                    seed,
                    &m.design_doc().oib_allocations()?,
                    &address,
                )?;
                // Resolve is deliberately the existing explicit signed literal;
                // it must never be implemented as a clear-to-inherit action.
                let cell = seed
                    .cells
                    .iter()
                    .find(|c| c.id == cell)
                    .ok_or("Missing property")?;
                let value = cell
                    .authored_override
                    .as_ref()
                    .unwrap_or(&cell.default)
                    .clone();
                let changed = m.stage_property(&address, Some(value))?;
                if changed {
                    m.refresh()?;
                    m.invalidate_presentation();
                }
                Ok(changed)
            }
        }
    }
    pub(super) fn designer_activate(&mut self, id: u32) -> Result<Option<bool>> {
        if let Some(result) = self.leave_activate(id)? {
            return Ok(Some(result));
        }
        let Some(m) = &self.active else {
            return Ok(None);
        };
        let Some(d) = &m.designer else {
            return Ok(None);
        };
        if let Some(key) = d.keyboard_key(id) {
            if !self.scene()?.controls().iter().any(|c| c.node.id == id) {
                return Ok(Some(false));
            }
            let m = self.active.as_mut().ok_or("No Canvas")?;
            let edit = m.keyboard.press(key);
            m.invalidate();
            if let Some(edit) = edit {
                return self.hardware_edit(&edit).map(Some);
            }
            return Ok(Some(true));
        }
        if (FIRST..2_900_000_000).contains(&id) || d.local_action(id).is_some() {
            let action = d.semantic_action(self.scene()?.as_ref(), id);
            return action.map_or(Ok(Some(false)), |a| self.designer_action(a).map(Some));
        }
        if d.mode == DesignerMode::Edit
            && let Some(preview) = m.preview_scroll.as_ref()
            && self
                .scene()?
                .path(id)
                .is_some_and(|path| path.iter().any(|p| p.node.id == preview.node()))
        {
            return Ok(Some(false));
        }
        Ok(None)
    }
    /// Run before preview pan and application input. Local captures pin typed
    /// actions/full occurrence identities until release; slots cannot retarget.
    pub(super) fn designer_pointer(
        &mut self,
        event: PointerEvent,
        at: u64,
    ) -> Result<Option<bool>> {
        if let Some(result) = self.leave_pointer(event)? {
            return Ok(Some(result));
        }
        let Some(m) = &self.active else {
            return Ok(None);
        };
        if m.designer.is_none() {
            return Ok(None);
        }
        if event.phase != PointerPhase::Down
            && m.designer.as_ref().is_none_or(|d| d.capture.is_none())
        {
            return Ok(None);
        }
        if event.phase == PointerPhase::Down && self.input_busy() {
            return Ok(Some(false));
        }
        // Once captured, Scroll uses its pinned Down geometry for Move/Cancel
        // and non-tap release. Do not rebuild the presented/culling scene merely
        // to advance input before the renderer has credit for another frame.
        let captured_pan = m
            .designer
            .as_ref()
            .and_then(|d| d.capture.as_ref())
            .is_some_and(|c| {
                event.phase == PointerPhase::Move
                    || event.phase == PointerPhase::Cancel
                    || (event.phase == PointerPhase::Up && c.moving)
            });
        let scene = if captured_pan {
            cached_layout(m)?
        } else {
            self.scene()?
        };
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let mut designer = m.designer.take().ok_or("Missing designer")?;
        let result = (|| -> Result<(Option<bool>, Option<LocalAction>)> {
            if event.phase == PointerPhase::Down {
                let hit = scene.hit_path(event.position);
                // Only the real hit ancestry admits an action. Modal siblings,
                // Consume barriers and PassThrough overlays retain scene rules.
                let action = hit
                    .iter()
                    .rev()
                    .find_map(|p| designer.local_action(p.node.id));
                if action.as_ref().is_some_and(LocalAction::outside_panels) {
                    designer.capture = Some(Capture {
                        pointer: event.id,
                        start: event.position,
                        moving: false,
                        action,
                        panel: None,
                    });
                    return Ok((Some(true), None));
                }
                let mut panel_index = None;
                for (index, panel) in designer.panels.iter_mut().enumerate() {
                    if admits_viewport(&scene, &hit, u32::from(panel.viewport), event.position)
                        && panel.scroll.pointer(&scene, event, at)?.is_some()
                    {
                        panel_index = Some(index);
                        break;
                    }
                }
                if panel_index.is_some() {
                    designer.capture = Some(Capture {
                        pointer: event.id,
                        start: event.position,
                        moving: false,
                        action,
                        panel: panel_index,
                    });
                    return Ok((Some(true), None));
                }
                if designer.intercepts_preview()
                    && let Some(scroll) = &mut m.preview_scroll
                    && admits_viewport(&scene, &hit, scroll.node(), event.position)
                {
                    let selection = hit.iter().rev().find_map(|p| {
                        if let Some(preview) = &m.definition_presentation {
                            return preview.preview.selection_for_node(p.node.id);
                        }
                        designer
                            .objects
                            .iter()
                            .find(|o| o.layout_node.map(u32::from) == Some(p.node.id))
                            .map(|o| o.selection.clone())
                    });
                    scroll.pointer(&scene, event, at)?;
                    designer.capture = Some(Capture {
                        pointer: event.id,
                        start: event.position,
                        moving: false,
                        action: selection.map(LocalAction::Select),
                        panel: None,
                    });
                    return Ok((Some(true), None));
                }
                return Ok((None, None));
            }
            let Some(mut capture) = designer.capture.take() else {
                return Ok((None, None));
            };
            if capture.pointer != event.id {
                designer.capture = Some(capture);
                return Ok((Some(false), None));
            }
            capture.moving |= (event.position[0] - capture.start[0])
                .abs()
                .max((event.position[1] - capture.start[1]).abs())
                > 8.;
            let tap = event.phase == PointerPhase::Up && !capture.moving;
            let action = if tap {
                designer.tap_action(&capture, &scene, event.position)
            } else {
                None
            };
            let scroll = if let Some(index) = capture.panel {
                Some(&mut designer.panels[index].scroll)
            } else {
                m.preview_scroll.as_mut()
            };
            if let Some(scroll) = scroll {
                scroll.pointer(&scene, event, at)?;
                if tap {
                    scroll.cancel();
                }
            }
            if !matches!(event.phase, PointerPhase::Up | PointerPhase::Cancel) {
                designer.capture = Some(capture);
            }
            Ok((Some(true), action))
        })();
        m.designer = Some(designer);
        let (handled, action) = result?;
        if handled.is_some() {
            m.invalidate_presentation();
        }
        if let Some(action) = action {
            self.designer_action(action)?;
        }
        Ok(handled)
    }
    pub(super) fn designer_scroll_at(
        &mut self,
        delta: f32,
        position: [f32; 2],
    ) -> Result<Option<bool>> {
        let scene = self.scene()?;
        let hit = scene.hit_path(position);
        let Some(m) = &mut self.active else {
            return Ok(None);
        };
        let Some(d) = &mut m.designer else {
            return Ok(None);
        };
        for panel in &mut d.panels {
            if admits_viewport(&scene, &hit, u32::from(panel.viewport), position) {
                let before = panel.scroll.info().offset;
                panel.scroll.set_offset([0., before[1] + delta]);
                let changed = before != panel.scroll.info().offset;
                if changed {
                    m.invalidate_presentation();
                }
                return Ok(Some(changed));
            }
        }
        Ok(None)
    }
}
fn admits_viewport(
    scene: &Scene,
    hit: &[crate::dashboard::scene::Located<'_>],
    viewport: u32,
    position: [f32; 2],
) -> bool {
    let Some(path) = scene.path(viewport) else {
        return false;
    };
    let Some(p) = path.last() else {
        return false;
    };
    // Bounds from every clipping ancestor apply to local routing as well as
    // painting. An offscreen portion must not capture a sibling's input.
    if path.iter().any(|p| {
        matches!(
            p.node.primitive,
            Primitive::Canvas { clip: true } | Primitive::Viewport { .. }
        ) && ((position[0] - p.origin[0]) / p.scale < 0.
            || (position[1] - p.origin[1]) / p.scale < 0.
            || (position[0] - p.origin[0]) / p.scale >= p.node.frame.width
            || (position[1] - p.origin[1]) / p.scale >= p.node.frame.height)
    }) {
        return false;
    }
    let parent = path.iter().rev().nth(1).map(|p| p.node.id);
    let x = (position[0] - p.origin[0]) / p.scale;
    let y = (position[1] - p.origin[1]) / p.scale;
    let inside = hit.iter().any(|p| p.node.id == viewport);
    let background = hit.is_empty()
        || (hit.iter().any(|p| Some(p.node.id) == parent)
            && !hit.iter().any(|p| {
                matches!(
                    p.node.responder,
                    Some(
                        TouchResponder::Consume
                            | TouchResponder::Activate(_)
                            | TouchResponder::Drag(_)
                    )
                )
            }));
    x >= 0.
        && y >= 0.
        && x < p.node.frame.width
        && y < p.node.frame.height
        && (inside || background)
}

#[cfg(test)]
#[path = "oib_designer_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "oib_workspace_focus_tests.rs"]
mod workspace_focus_tests;

#[cfg(test)]
#[path = "oib_layout_editor_tests.rs"]
mod layout_editor_tests;

#[cfg(test)]
#[path = "oib_viewport_tests.rs"]
mod viewport_tests;

#[cfg(test)]
#[path = "oib_endpoint_removal_tests.rs"]
mod endpoint_removal_tests;

impl Mount {
    pub(super) fn discard_removed_occurrence_drafts(
        &mut self,
        instances: &[otrios_interfaces::oib::designer::InstanceId],
    ) -> Result<()> {
        self.discard_removed_designer_drafts(|address| matches!(editing::selection(address), Selection::Object(object) if instances.contains(&object.instance)))
    }
    pub(super) fn discard_removed_definition_drafts(
        &mut self,
        definition: otrios_interfaces::oib::designer::DefinitionId,
    ) -> Result<()> {
        self.discard_removed_designer_drafts(|address| matches!(editing::selection(address),
            Selection::Definition(id) | Selection::DefinitionNode { definition: id, .. } if id == definition))
    }
    pub(super) fn discard_removed_property_drafts(
        &mut self,
        definition: otrios_interfaces::oib::designer::DefinitionId,
        property: otrios_interfaces::oib::designer::PropertyId,
    ) -> Result<()> {
        let source = self.design_doc().oib_history_source()?;
        self.discard_removed_designer_drafts(|address| {
            crate::canvas::oib::endpoint_impact::addresses_endpoint(
                &source, address, definition, property,
            )
        })
    }
    fn discard_removed_designer_drafts(
        &mut self,
        removed: impl Fn(&PropertyAddress) -> bool,
    ) -> Result<()> {
        let d = self.designer.as_mut().ok_or("Missing designer")?;
        let removed: Vec<_> = d
            .drafts
            .drafts()
            .iter()
            .filter(|draft| removed(&draft.address))
            .map(|draft| draft.address.clone())
            .collect();
        for address in removed {
            d.drafts.cancel(&address);
        }
        if !d.editor_focused() {
            self.keyboard.hide();
        }
        d.details = None;
        d.pending_source = None;
        d.pending_selection = None;
        self.refresh()?;
        self.invalidate();
        Ok(())
    }
}
