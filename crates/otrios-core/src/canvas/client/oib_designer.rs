// SPDX-License-Identifier: LGPL-3.0-only
//! Device-local D1 tooling. All identity/source resolution stays in oib::document.
mod editing;
use super::oib_property_editor::PropertyDrafts;
use super::*;
use crate::canvas::oib::document::{
    HierarchyKey, HierarchyRow, HierarchyScope, ObjectInfo, PropertyInfo,
};
use crate::dashboard::components::{ACCENT, CARD, MUTED, WHITE, background, canvas, text};
use otrios_interfaces::{
    oib::designer::{DesignerMode, PropertyAddress, PropertyEditor, Selection},
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
    Layers,
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
    TreeScope(HierarchyScope),
    Expand(Selection),
    Mode(DesignerMode),
    Write(PropertyAddress, Option<Value>),
    Resolve(PropertyAddress),
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
pub(super) struct Designer {
    selection: Selection,
    focus_layout: bool,
    layers_open: bool,
    tree: Vec<HierarchyRow>,
    tree_scope: HierarchyScope,
    tree_closed: Vec<HierarchyKey>,
    tree_initialized: Vec<HierarchyScope>,
    reveal_selection: bool,
    drafts: PropertyDrafts,
    properties: Vec<PropertyInfo>,
    editor_notice: Option<String>,
    editor_revision: u64,
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
    revision: Option<(crate::merkle::Hash, bool, String, bool)>,
}
impl Clone for Designer {
    fn clone(&self) -> Self {
        Self {
            selection: self.selection.clone(),
            focus_layout: self.focus_layout,
            layers_open: self.layers_open,
            tree: self.tree.clone(),
            tree_scope: self.tree_scope,
            tree_closed: self.tree_closed.clone(),
            tree_initialized: self.tree_initialized.clone(),
            reveal_selection: self.reveal_selection,
            drafts: self.drafts.clone(),
            properties: self.properties.clone(),
            editor_notice: self.editor_notice.clone(),
            editor_revision: self.editor_revision,
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
            revision: self.revision.clone(),
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
            layers_open: false,
            tree: vec![],
            tree_scope: HierarchyScope::Composition,
            tree_closed: vec![],
            tree_initialized: vec![],
            reveal_selection: false,
            drafts: PropertyDrafts::new(2_800_000_000, 2_900_000_000 - 1),
            properties: vec![],
            editor_notice: None,
            editor_revision: 0,
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
    pub(super) fn focus_layout(&self) -> bool {
        self.focus_layout
    }
    pub(super) fn layers_open(&self) -> bool {
        self.layers_open
    }
    fn local_action(&self, id: u32) -> Option<LocalAction> {
        if self.focus_layout && id == super::oib_focus_layout::LAYERS {
            Some(LocalAction::Layers)
        } else {
            self.actions.get(&id).cloned()
        }
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
    pub(super) fn tick(&mut self, now: u64) -> bool {
        self.panels
            .iter_mut()
            .fold(false, |changed, p| p.scroll.tick(now) || changed)
    }
    fn select(&mut self, selection: Selection) -> bool {
        if self.selection == selection {
            return false;
        }
        if self.focus_layout {
            self.tree_scope = match &selection {
                Selection::Definition(_) | Selection::DefinitionNode { .. } => {
                    HierarchyScope::Components
                }
                _ => HierarchyScope::Composition,
            };
            self.reveal_selection = true;
        }
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
        // Bound even the hidden/disclosed semantic model, independently from
        // the retained subtree and presented scene bounds. Runtime admission
        // supplies a smaller bounded graph; this is a defensive local ceiling.
        if objects.len() > MAX_CACHED_NODES {
            return Err("Designer object model budget exceeded".into());
        }
        let mut builder = Builder::at(self.next_id);
        let hierarchy = if self.panels.len() == 2 {
            Some(if self.focus_layout {
                focus_hierarchy(
                    &mut builder,
                    &self.tree,
                    &self.selection,
                    &self.tree_closed,
                    self.tree_scope,
                    self.mode,
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
                self.editor_notice.as_deref(),
                self.details.as_ref(),
                &self.registers,
                seed,
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
        if let Some((nodes, content)) = hierarchy {
            self.panels[0].nodes = nodes;
            self.panels[0].content = content;
        }
        let single = self.panels.len() == 1;
        let panel = self.panels.last_mut().ok_or("Missing designer inspector")?;
        panel.nodes = nodes;
        panel.content = inspector.1 + if single { ROW } else { 0. };
        for panel in &mut self.panels {
            if panel.content > MAX_CONTENT {
                return Err("Designer panel coordinate budget exceeded".into());
            }
            panel.scroll.set_content([0., panel.content]);
        }
        self.actions = builder.actions;
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
    ) -> Result<()> {
        for panel in &self.panels {
            let node =
                find_node(root, u32::from(panel.viewport)).ok_or("Missing designer viewport")?;
            if self.focus_layout
                && !self.layers_open
                && self
                    .panels
                    .first()
                    .is_some_and(|p| p.viewport == panel.viewport)
            {
                node.children.clear();
                node.responder = None;
                continue;
            }
            let offset = panel.scroll.info().offset[1];
            let top = (offset / WINDOW_STEP).floor() * WINDOW_STEP;
            let bottom = ((offset + node.frame.height) / WINDOW_STEP).ceil() * WINDOW_STEP;
            // Retained top-level rows contain all their own paint and actions.
            // Clone only rows intersecting the viewport's aligned window; no
            // glyph/inspector construction or full-tree clone on a pan sample.
            node.children = panel
                .nodes
                .iter()
                .filter(|row| row.frame.y < bottom && row.frame.y + row.frame.height > top)
                .cloned()
                .collect();
            if Some(panel.viewport) == self.panels.last().map(|p| p.viewport) {
                let status_y = if self.panels.len() == 1 {
                    ROW + 28.
                } else {
                    28.
                };
                if let Some(label) = node.children.iter_mut().find(|n| n.frame.y == status_y)
                    && let Primitive::Text { value, .. } = &mut label.primitive
                {
                    let status = if let Some(mode) = self.pending_mode {
                        format!("{status} / {mode:?} pending")
                    } else {
                        status.to_string()
                    };
                    *value = status.chars().take(128).collect();
                }
            }
            node.responder = None;
            // The signed viewport sets the real available width. Rows occupy it
            // completely, including padding/corners, without covering siblings.
            fn fit(nodes: &mut [SceneNode], width: f32) {
                for n in nodes {
                    if matches!(n.primitive, Primitive::Canvas { .. })
                        && n.frame.x == 0.
                        && matches!(n.frame.width, 240. | 288. | 320.)
                    {
                        n.frame.width = width;
                        if let Some(bg) = n.children.first_mut() {
                            bg.frame.width = width;
                        }
                    }
                }
            }
            fit(&mut node.children, node.frame.width);
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
                    let mut edge =
                        background(FIRST - 9 + i as u32, rect.width, rect.height, ACCENT, 0.);
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
            if capture.action == Some(LocalAction::Layers) {
                return scene
                    .hit_path(point)
                    .iter()
                    .any(|p| p.node.id == super::oib_focus_layout::LAYERS)
                    .then_some(LocalAction::Layers);
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
            color,
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
        let mut row = self.button(x, y, width, label, action)?;
        row.frame.height = 52.;
        row.children[0].frame.height = 52.;
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
        let id = self.id()?;
        let mut bg = background(self.id()?, width, 32., CARD, 4.);
        bg.responder = Some(TouchResponder::Bubble);
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
fn focus_hierarchy(
    b: &mut Builder,
    tree: &[HierarchyRow],
    selected: &Selection,
    collapsed: &[HierarchyKey],
    scope: HierarchyScope,
    mode: DesignerMode,
) -> Result<(Vec<SceneNode>, f32)> {
    let mut nodes = vec![
        b.focus_button(
            0.,
            0.,
            140.,
            if scope == HierarchyScope::Composition {
                "Layers *"
            } else {
                "Layers"
            },
            LocalAction::TreeScope(HierarchyScope::Composition),
        )?,
        b.focus_button(
            148.,
            0.,
            140.,
            if scope == HierarchyScope::Components {
                "Components *"
            } else {
                "Components"
            },
            LocalAction::TreeScope(HierarchyScope::Components),
        )?,
        b.focus_button(
            0.,
            56.,
            288.,
            &format!("Mode: {mode:?}"),
            LocalAction::Mode(if mode == DesignerMode::Edit {
                DesignerMode::Test
            } else {
                DesignerMode::Edit
            }),
        )?,
    ];
    let mut y = 116.;
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
            b.focus_button(
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
                MUTED,
            );
            canvas(
                b.id()?,
                Rect::new(0., y, 288., 52.),
                vec![background(b.id()?, 288., 52., CARD, 0.), label],
            )
        };
        if let Some(label) = node.children.get_mut(1) {
            label.frame.x = indent + 48.;
            label.frame.width = (232. - indent).max(1.);
        }
        if row.selection.as_ref() == Some(selected)
            && let Primitive::Superellipse { color, .. } = &mut node.children[0].primitive
        {
            *color = [49, 66, 96, 255];
        }
        if children {
            node.children.push(b.focus_button(
                indent,
                0.,
                48.,
                if collapsed.contains(&row.key) {
                    ">"
                } else {
                    "v"
                },
                LocalAction::TreeExpand(row.key.clone()),
            )?);
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
        let previous_viewport = self.viewport;
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
            let root = self.doc.oib_root().ok_or("Missing designer state")?;
            let status = self.doc.oib_status_text();
            let editable = self.binding.as_ref().is_none_or(|b| b.ui_editable);
            let revision = (root, self.doc.confirmed(), status.clone(), editable);
            if designer.revision.as_ref() == Some(&revision) {
                return Ok(changed);
            }
            previous.get_or_insert_with(|| designer.clone());
            let seed = super::super::oib::seed(&self.ui).ok_or("Missing designer seed")?;
            let allocations = self.doc.oib_allocations()?;
            let objects = super::super::oib::document::objects(seed, &allocations)?;
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
                    &self.doc.oib_active()?,
                );
                if objects.iter().any(|o| o.selection == target) && trace.is_ok() {
                    designer.select(target);
                    designer.details = Some(trace?);
                    designer.layers_open = false;
                    designer.editor_notice = None;
                } else {
                    designer.editor_notice = Some("Source is no longer available".into());
                }
            }

            if designer.focus_layout {
                let live = self
                    .doc
                    .oib_instances()?
                    .into_iter()
                    .filter(|i| i.active)
                    .map(|i| i.id)
                    .collect();
                let tree = super::super::oib::document::hierarchy(
                    seed,
                    &allocations,
                    &live,
                    designer.tree_scope,
                )?;
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
            designer.editor_revision = self.doc.oib_generation().unwrap_or(0);
            let properties = super::super::oib::document::properties(
                seed,
                &allocations,
                &designer.selection,
                &self.doc.oib_active()?,
            )?;
            let conflicts = self.doc.oib_conflicts();
            designer.registers = properties
                .iter()
                .filter(|p| p.cell.is_some_and(|c| conflicts.contains(&c)))
                .filter_map(|p| p.address.as_ref())
                .map(|a| self.doc.oib_property_register(a))
                .collect::<Result<_>>()?;
            if let Some(address) = designer.details.as_ref().map(|d| d.address.clone()) {
                designer.details = super::super::oib::document::property_sources(
                    seed,
                    &allocations,
                    &address,
                    &self.doc.oib_active()?,
                )
                .ok();
            }
            // Dynamic occurrence cells carry the signed prototype's choices.
            // Their IDs are absent from the unexpanded seed; reuse the runtime
            // expansion rather than deriving a second factory/path lookup here.
            let expanded = super::super::oib::instances::expanded_seed(seed, &allocations)?;
            designer.rebuild(
                &expanded,
                objects,
                &properties,
                &self.doc.oib_conflicts(),
                &status,
                editable,
            )?;
            designer.revision = Some(revision);
            Ok(true)
        })();
        self.designer = Some(designer);
        let result = result.and_then(|changed| {
            if changed {
                let layout = resolve_layout(self)?;
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
            if self.viewport != previous_viewport {
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
        let changed = designer.select(selection);
        if changed && let Err(error) = m.refresh_designer() {
            m.designer = Some(previous);
            return Err(error);
        }
        Ok(changed)
    }
    /// Queue a local mode switch while any gesture/edit owns the current graph.
    pub fn set_oib_designer_mode(&mut self, mode: DesignerMode) -> Result<bool> {
        let busy = self.input_busy();
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let blocked = busy || m.oib_activation_blocked();
        let Some(designer) = &mut m.designer else {
            return Ok(false);
        };
        let changed = designer.set_mode(mode, blocked);
        if changed {
            m.refresh_designer()?;
            m.invalidate_presentation();
        }
        Ok(changed)
    }
    fn designer_action(&mut self, action: LocalAction) -> Result<bool> {
        match action {
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
            LocalAction::TreeScope(scope) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                d.tree_scope = scope;
                d.revision = None;
                d.panels[0].scroll.set_offset([0., 0.]);
                m.refresh_designer()?;
                Ok(true)
            }
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
            LocalAction::Layers => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                d.layers_open = !d.layers_open;
                d.invalidate_bounds();
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
            LocalAction::Write(address, value) => {
                if self.input_busy() {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
                    return Ok(false);
                }
                let reset = value.is_none();
                let changed = m.doc.edit_oib_property(&address, value)?;
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
                let seed = super::super::oib::seed(&m.ui).ok_or("Missing designer seed")?;
                let cell = super::super::oib::document::cell_for_address(
                    seed,
                    &m.doc.oib_allocations()?,
                    &address,
                )?;
                // Resolve is deliberately the existing explicit signed literal;
                // it must never be implemented as a clear-to-inherit action.
                let changed = m
                    .doc
                    .oib_inspector_action(cell, otrios_interfaces::oib::InspectorAction::Resolve)?;
                if changed {
                    m.refresh()?;
                    m.invalidate_presentation();
                }
                Ok(changed)
            }
        }
    }
    pub(super) fn designer_activate(&mut self, id: u32) -> Result<Option<bool>> {
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
        if (FIRST..2_900_000_000).contains(&id) || id == super::oib_focus_layout::LAYERS {
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
        let Some(m) = &self.active else {
            return Ok(None);
        };
        if m.designer.is_none() {
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
                if action == Some(LocalAction::Layers) {
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
mod tests;
