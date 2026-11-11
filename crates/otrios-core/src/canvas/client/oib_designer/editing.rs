// SPDX-License-Identifier: LGPL-3.0-only
//! Direct tooling fields and their shared core input route. Raw drafts stay local.
use super::super::oib_property_editor::{EditResult, Part};
use super::*;
use crate::canvas::oib::document::{PropertyRegisterView, PropertySources, SourceRole};
use crate::dashboard::{
    font::Font,
    keyboard::{KEY_COUNT, Keyboard},
};
use otrios_interfaces::keyboard::TextInput;
const KEY_BASE: u32 = 2_695_000_000;

pub(super) fn sources(
    state: &StateAdapter,
    seed: &otrios_interfaces::oib::Seed,
    address: &PropertyAddress,
) -> Result<PropertySources> {
    let mut trace = state.oib_style_sources(seed, address)?;
    if matches!(address, PropertyAddress::Structural { .. }) {
        let field = state
            .oib_structural_properties(&selection(address))?
            .into_iter()
            .find(|field| field.address.as_ref() == Some(address))
            .ok_or("Design field no longer available")?;
        trace.value = field.value;
        trace.inherited = field.inherited;
        trace.overridden = field.overridden;
    }
    Ok(trace)
}

pub(super) fn selection(address: &PropertyAddress) -> Selection {
    match address {
        PropertyAddress::Structural { target, .. } => match target {
            otrios_interfaces::oib::designer::StructuralTarget::Style(style)
            | otrios_interfaces::oib::designer::StructuralTarget::StyleField { style, .. } => {
                Selection::Style(*style)
            }
            otrios_interfaces::oib::designer::StructuralTarget::Variable(variable)
            | otrios_interfaces::oib::designer::StructuralTarget::VariableMode {
                variable, ..
            } => Selection::Variable(*variable),
            otrios_interfaces::oib::designer::StructuralTarget::Definition(definition)
            | otrios_interfaces::oib::designer::StructuralTarget::Property { definition, .. } => {
                Selection::Definition(*definition)
            }
            otrios_interfaces::oib::designer::StructuralTarget::Node { definition, node } => {
                Selection::DefinitionNode {
                    definition: *definition,
                    node: *node,
                }
            }
            otrios_interfaces::oib::designer::StructuralTarget::Instance(object) => {
                Selection::Object(object.clone())
            }
            otrios_interfaces::oib::designer::StructuralTarget::Preset { definition, .. } => {
                Selection::Definition(*definition)
            }
        },
        PropertyAddress::Placement { object, .. } => Selection::Object(object.clone()),
        PropertyAddress::Definition {
            definition, node, ..
        } => Selection::DefinitionNode {
            definition: *definition,
            node: *node,
        },
        PropertyAddress::Occurrence { instance, path } => {
            Selection::Object(otrios_interfaces::oib::designer::ObjectAddress {
                instance: *instance,
                references: path.references.clone(),
                node: path.node,
            })
        }
    }
}
fn label(b: &mut Builder, y: f32, value: &str, color: [u8; 4]) -> Result<SceneNode> {
    let value: String = value
        .chars()
        .scan(0, |n, c| {
            *n += c.len_utf8();
            (*n <= 256).then_some(c)
        })
        .collect();
    Ok(text(
        b.id()?,
        Rect::new(0., y, 320., 32.),
        &value,
        2,
        b.palette.role(color),
    ))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn inspector(
    b: &mut Builder,
    drafts: &mut PropertyDrafts,
    properties: &[PropertyInfo],
    conflicts: &[u64],
    status: &str,
    editable: bool,
    revision: u64,
    structural_revision: u64,
    structural_generation: u64,
    notice: Option<&str>,
    details: Option<&PropertySources>,
    registers: &[PropertyRegisterView],
    seed: &otrios_interfaces::oib::Seed,
    connections: &[(
        PropertyAddress,
        Vec<(String, otrios_interfaces::oib::InstanceOperation)>,
    )],
    layout_enabled: bool,
    layout_bounds: bool,
    padding_independent: bool,
) -> Result<(Vec<SceneNode>, f32)> {
    let layout_target = layout_editor::target(seed, properties, structural_generation)
        .filter(|_| editable && layout_enabled);
    let separate = layout_target
        .as_ref()
        .is_some_and(|target| layout_editor::separate(target, padding_independent));
    let visible_properties =
        layout_editor::visible(properties, layout_target.as_ref(), layout_bounds, separate);
    let properties = visible_properties.as_slice();
    // Clean fields outside this selection are disposable. Dirty/invalid fields
    // are never silently evicted and retain their original full address.
    let retired: Vec<_> = drafts
        .drafts()
        .iter()
        .filter(|d| {
            !d.dirty
                && !properties
                    .iter()
                    .any(|p| p.address.as_ref() == Some(&d.address))
        })
        .map(|d| d.address.clone())
        .collect();
    for address in retired {
        drafts.cancel(&address);
    }
    let mut nodes = vec![label(b, 0., "Selection", WHITE)?];
    let mut y = 44.;
    if let Some(notice) = notice {
        nodes.push(label(b, y, notice, ACCENT)?);
        y += 40.;
    }
    let hidden: Vec<_> = drafts
        .drafts()
        .iter()
        .filter(|d| {
            d.dirty
                && !properties
                    .iter()
                    .any(|p| p.address.as_ref() == Some(&d.address))
        })
        .map(|d| d.address.clone())
        .collect();
    for address in hidden {
        nodes.push(b.focus_button(
            0.,
            y,
            320.,
            "Unapplied edit - inspect",
            LocalAction::Select(selection(&address)),
        )?);
        y += 56.;
        nodes.push(b.focus_button(
            0.,
            y,
            320.,
            "Revert unapplied edit",
            LocalAction::Discard(address),
        )?);
        y += 56.;
    }
    if let Some(target) = &layout_target {
        nodes.extend(layout_editor::build(
            b,
            drafts,
            properties,
            target,
            structural_revision,
            &mut y,
            layout_bounds,
            separate,
            status.contains("Saving"),
        )?);
    }
    for p in properties {
        if layout_target.is_some() && p.address.as_ref().is_some_and(|address| matches!(address, PropertyAddress::Structural {property,..} if layout_editor::numeric(*property))) { continue; }
        let mut heading = label(b, y + 8., &p.descriptor.label, WHITE)?;
        heading.frame.width = 208.;
        nodes.push(heading);
        if let Some(address) = &p.address {
            let expanded = details.is_some_and(|d| &d.address == address);
            let mut info = b.focus_button(
                216.,
                y,
                104.,
                if expanded { "v Info" } else { "> Info" },
                LocalAction::Details(address.clone()),
            )?;
            if let Some(semantics) = &mut info.semantics {
                semantics.label = if expanded {
                    "Hide property info"
                } else {
                    "Show property info"
                }
                .into();
                semantics.checked = expanded;
            }
            if expanded {
                super::style::set_surface_fill(&mut info.children[0], b.palette.selected)?;
                if let Primitive::Text { color, .. } = &mut info.children[1].primitive {
                    *color = b.palette.background;
                }
            }
            nodes.push(info);
        }
        y += 56.;
        property_context(
            b,
            &mut nodes,
            &mut y,
            p,
            details,
            registers,
            seed,
            editable,
            connections,
        )?;
        let allowed = editable && !p.cell.is_some_and(|c| conflicts.contains(&c));
        let pending = status.contains("Saving")
            && p.address
                .as_ref()
                .is_some_and(|a| drafts.draft(a).is_some());
        let field_revision = if p.address.as_ref().is_some_and(|address| {
            matches!(
                address,
                PropertyAddress::Placement { .. } | PropertyAddress::Structural { .. }
            )
        }) {
            structural_revision
        } else {
            revision
        };
        if !allowed || (!pending && drafts.open(p, field_revision).is_err()) {
            nodes.push(label(
                b,
                y,
                &p.value.as_ref().map_or_else(
                    || {
                        if p.source == "bound" {
                            "App value".into()
                        } else {
                            "Unavailable".into()
                        }
                    },
                    value_label,
                ),
                MUTED,
            )?);
            y += 36.;
            nodes.push(label(
                b,
                y,
                if p.source == "variable" {
                    "Variable"
                } else if allowed {
                    "Read only - app binding"
                } else {
                    "Read only / resolve conflict"
                },
                MUTED,
            )?);
            y += 44.;
            continue;
        }
        let address = p.address.as_ref().ok_or("Missing property address")?;
        let draft = drafts.draft(address).ok_or("Missing property draft")?;
        for field in &draft.fields {
            if field.part == Part::Opacity {
                nodes.push(label(b, y, "Opacity %", MUTED)?);
                y += 36.;
            }
            let mut input = b.focus_button(0., y, 320., "", LocalAction::FocusField(field.id))?;
            // Match the browser editor surface so native values are visibly
            // editable instead of blending into the properties panel.
            style::set_surface_fill(&mut input.children[0], b.palette.background)?;
            b.actions.remove(&input.id);
            input.id = field.id;
            input.semantics = Some(SwitchSemantics {
                label: match field.part {
                    Part::Hex => format!("{} hex", p.descriptor.label),
                    Part::Opacity => format!("{} opacity", p.descriptor.label),
                    Part::Value => p.descriptor.label.clone(),
                },
                checked: false,
            });
            b.actions
                .insert(field.id, LocalAction::FocusField(field.id));
            if let Primitive::Text { value, .. } = &mut input.children[1].primitive {
                *value = field.buffer().text().to_owned();
            }
            nodes.push(input);
            y += 56.;
        }
        if let Ok(Value::Color(current)) = draft.preview() {
            for (index, rgb) in [
                [85, 136, 255],
                [218, 165, 32],
                [255, 136, 85],
                [255, 90, 135],
                [165, 110, 255],
                [60, 200, 150],
                [255, 255, 255],
                [30, 34, 42],
            ]
            .into_iter()
            .enumerate()
            {
                let color = [rgb[0], rgb[1], rgb[2], current[3]];
                let mut swatch = b.focus_button(
                    (index % 4) as f32 * 80.,
                    y + (index / 4) as f32 * 56.,
                    72.,
                    "",
                    LocalAction::PickColor(address.clone(), color),
                )?;
                style::set_surface_fill(&mut swatch.children[0], color)?;
                swatch.semantics = Some(SwitchSemantics {
                    label: format!("Choose {}", value_label(&Value::Color(color))),
                    checked: color == current,
                });
                nodes.push(swatch);
            }
            y += 112.;
            for (index, opacity) in [0, 50, 100].into_iter().enumerate() {
                nodes.push(b.focus_button(
                    index as f32 * 108.,
                    y,
                    104.,
                    &format!("{opacity}%"),
                    LocalAction::Opacity(address.clone(), opacity),
                )?);
            }
            y += 56.;
        }
        if draft.fields.is_empty()
            && let Ok(Value::Boolean(value)) = draft.preview()
        {
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                if value { "On" } else { "Off" },
                LocalAction::DraftBool(address.clone(), !value),
            )?);
            y += 56.;
        }
        {
            nodes.push(b.focus_button(0., y, 148., "Done", LocalAction::Apply(address.clone()))?);
            nodes.push(b.focus_button(
                164.,
                y,
                148.,
                "Revert",
                LocalAction::Discard(address.clone()),
            )?);
            y += 56.;
        }
        if draft.error.is_some() {
            nodes.push(label(b, y, "Invalid value - edit or Revert", ACCENT)?);
            y += 40.;
        }
        if p.overridden && matches!(address, PropertyAddress::Occurrence { .. }) {
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                "Reset override",
                LocalAction::Write(address.clone(), None),
            )?);
            y += 56.;
        }
        if p.overridden
            && p.inherited.is_some()
            && matches!(address, PropertyAddress::Structural { .. })
        {
            nodes.push(b.focus_button(
                0.,
                y,
                320.,
                "Restore authored design",
                LocalAction::RestoreDesign(address.clone(), structural_generation),
            )?);
            y += 56.;
        }
        nodes.push(label(
            b,
            y,
            if matches!(address, PropertyAddress::Placement { .. }) {
                "Parent-local placement"
            } else if p.overridden {
                "Local override"
            } else {
                "Inherited / authored"
            },
            MUTED,
        )?);
        y += 36.;
        if let Some(inherited) = &p.inherited {
            nodes.push(label(
                b,
                y,
                &format!("Source: {}", value_label(inherited)),
                MUTED,
            )?);
            y += 40.;
        }
        y += 16.;
    }
    if properties.is_empty() {
        nodes.push(label(b, y, "No editable properties", MUTED)?);
        y += 40.;
    }
    if y > MAX_CONTENT {
        return Err("Designer inspector coordinate budget exceeded".into());
    }
    Ok((nodes, y))
}

#[allow(clippy::too_many_arguments)]
fn property_context(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    y: &mut f32,
    property: &PropertyInfo,
    details: Option<&PropertySources>,
    registers: &[PropertyRegisterView],
    seed: &otrios_interfaces::oib::Seed,
    editable: bool,
    connections: &[(
        PropertyAddress,
        Vec<(String, otrios_interfaces::oib::InstanceOperation)>,
    )],
) -> Result<()> {
    if let Some(trace) = details.filter(|d| property.address.as_ref() == Some(&d.address)) {
        nodes.push(label(
            b,
            *y,
            if matches!(trace.address, PropertyAddress::Placement { .. }) {
                "Placement - structural batch"
            } else if matches!(trace.address, PropertyAddress::Structural { .. }) {
                "Shared design - structural batch"
            } else if trace.binding.is_some() {
                "App binding"
            } else if trace.overridden {
                "Live override at occurrence"
            } else {
                "Inherited source"
            },
            MUTED,
        )?);
        *y += 40.;
        if let Some(value) = &trace.inherited {
            nodes.push(label(
                b,
                *y,
                &format!("Reset reveals {}", value_label(value)),
                MUTED,
            )?);
            *y += 40.;
        }
        let choices = connections
            .iter()
            .find(|(address, _)| address == &trace.address)
            .filter(|_| editable);
        if trace.binding.is_some() || choices.is_some() {
            nodes.push(label(
                b,
                *y,
                if choices.is_some() {
                    "App value; choose source"
                } else {
                    "App binding - read only"
                },
                MUTED,
            )?);
            *y += 40.;
            if let Some((_, choices)) = choices {
                for (name, operation) in choices {
                    nodes.push(b.focus_button(
                        0.,
                        *y,
                        320.,
                        name,
                        LocalAction::Structure(operation.clone()),
                    )?);
                    *y += 56.;
                }
            }
        }
        if let Some(source) = &trace.immediate_source {
            nodes.push(b.focus_button(
                0.,
                *y,
                320.,
                "Go to outer property",
                LocalAction::Source(source.clone()),
            )?);
            *y += 56.;
        }
        if let Some(source) = &trace.terminal_shared {
            let name = if let PropertyAddress::Definition { definition, .. } = source {
                seed.designer
                    .as_ref()
                    .and_then(|d| d.definitions.iter().find(|d| d.id == *definition))
                    .map_or("shared", |d| d.name.as_str())
            } else {
                "shared"
            };
            nodes.push(b.focus_button(
                0.,
                *y,
                320.,
                &format!("Source: {name}"),
                LocalAction::Source(source.clone()),
            )?);
            *y += 56.;
        }
        if trace
            .hops
            .iter()
            .any(|h| !h.masked && matches!(h.role, SourceRole::AuthoredLiteral(_)))
        {
            nodes.push(label(b, *y, "Authored assignment", MUTED)?);
            *y += 40.;
        }
        let masked = trace.hops.iter().filter(|h| h.masked).count();
        if masked > 0 {
            nodes.push(label(
                b,
                *y,
                &format!("{masked} masked source layers"),
                MUTED,
            )?);
            *y += 40.;
        }
    }
    if editable
        && let Some(view) = registers
            .iter()
            .find(|v| property.address.as_ref() == Some(&v.address))
    {
        nodes.push(label(b, *y, "Choose a conflict value", ACCENT)?);
        *y += 40.;
        for value in &view.alternatives {
            let raw = value
                .as_ref()
                .map_or_else(|| "inherited".into(), value_label);
            let short: String = raw
                .chars()
                .scan(0, |n, c| {
                    *n += c.len_utf8();
                    (*n <= 64).then_some(c)
                })
                .collect();
            nodes.push(b.focus_button(
                0.,
                *y,
                320.,
                &format!("Choose {short}"),
                LocalAction::Choose(Box::new(view.clone()), value.clone()),
            )?);
            *y += 56.;
        }
        if matches!(view.address, PropertyAddress::Occurrence { .. }) {
            nodes.push(b.focus_button(
                0.,
                *y,
                320.,
                "Reset override",
                LocalAction::Write(view.address.clone(), None),
            )?);
            *y += 56.;
        }
    }
    Ok(())
}

impl Designer {
    pub(in crate::canvas::client) fn keyboard_key(&self, id: u32) -> Option<u16> {
        self.editor_focused().then(|| keyboard_key(id)).flatten()
    }
    pub(in crate::canvas::client) fn focused_property(&self) -> Option<PropertyAddress> {
        self.drafts.focused().map(|f| f.address.clone())
    }
    pub(in crate::canvas::client) fn editor_focused(&self) -> bool {
        self.drafts.focused().is_some()
    }
    pub(in crate::canvas::client) fn editor_inputs(
        &self,
        scene: &Scene,
        font: &Font,
    ) -> Vec<TextInput> {
        scene
            .controls()
            .into_iter()
            .filter_map(|owner| {
                let draft = self
                    .drafts
                    .drafts()
                    .iter()
                    .find(|d| d.fields.iter().any(|f| f.id == owner.node.id))?;
                let field = draft.fields.iter().find(|f| f.id == owner.node.id)?;
                let focused = self
                    .drafts
                    .focused()
                    .is_some_and(|f| f.address == draft.address && f.part == field.part);
                let bounds = [
                    owner.origin[0],
                    owner.origin[1],
                    owner.node.frame.width * owner.scale,
                    owner.node.frame.height * owner.scale,
                ];
                if focused {
                    self.drafts.focused_input(bounds, font)
                } else {
                    Some(field.input(false, bounds, font))
                }
            })
            .collect()
    }
    pub(in crate::canvas::client) fn present_editor_fields(
        &self,
        root: &mut SceneNode,
        font: &Font,
    ) {
        for draft in self.drafts.drafts() {
            for field in &draft.fields {
                let Some(node) = find_node(root, field.id) else {
                    continue;
                };
                let focused = self
                    .drafts
                    .focused()
                    .is_some_and(|f| f.address == draft.address && f.part == field.part);
                let width = (node.frame.width - 16.).max(1.);
                let offset = if focused {
                    (field.buffer().caret_x(font, 2.) + font.text_width("|", 2.) - width).max(0.)
                } else {
                    0.
                };
                let (value, x) = field.buffer().window(offset, width, font, 2., focused);
                if let Some(label) = node.children.get_mut(1) {
                    label.frame.x = 8. + x;
                    if let Primitive::Text { value: text, .. } = &mut label.primitive {
                        *text = value;
                    }
                }
            }
        }
    }
    pub(in crate::canvas::client) fn present_editor_keyboard(
        &self,
        root: &mut SceneNode,
        keyboard: &Keyboard,
    ) -> Result<()> {
        if !self.focus_layout || !keyboard.visible || !self.editor_focused() {
            return Ok(());
        }
        let width = root.frame.width;
        let palette = self.appearance.palette();
        let mut children = vec![background(KEY_BASE, width, 240., palette.surface, 0.)];
        for key in 0..KEY_COUNT {
            let (row, col, columns): (u16, u16, u16) = match key {
                0..=11 => (0, key, 12),
                12..=22 => (1, key - 12, 11),
                23..=33 => (2, key - 23, 11),
                _ => (3, key - 34, 4),
            };
            let w = width / f32::from(columns);
            let id = KEY_BASE + 1 + u32::from(key) * 3;
            let pressed = keyboard.pressed(key) || keyboard.feedback_key() == Some(key);
            // Reuse the key's control canvas for the two paint layers rather
            // than adding a decorative wrapper per key to the combined scene.
            let mut node = style::surface_with_outline(
                [id, id + 1, KEY_BASE + 300 + u32::from(key)],
                w - 8.,
                52.,
                if pressed {
                    palette.selected
                } else {
                    palette.surface
                },
                palette.line,
                8.,
            );
            node.frame.x = f32::from(col) * w + 4.;
            node.frame.y = 8. + f32::from(row) * 56.;
            node.children.push(text(
                id + 2,
                Rect::new(8., 16., (w - 24.).max(1.), 28.),
                &keyboard.label(key),
                2,
                if pressed {
                    palette.background
                } else {
                    palette.text
                },
            ));
            node.responder = Some(TouchResponder::Activate(Action::KeyboardKey(key)));
            node.semantics = Some(SwitchSemantics {
                label: keyboard.label(key),
                checked: false,
            });
            children.push(node);
        }
        let mut keyboard = canvas(
            KEY_BASE + 200,
            Rect::new(0., root.frame.height - 240., width, 240.),
            children,
        );
        keyboard.responder = Some(TouchResponder::Consume);
        root.children.push(keyboard);
        Ok(())
    }
}
pub(in crate::canvas::client) fn keyboard_key(id: u32) -> Option<u16> {
    let index = id.checked_sub(KEY_BASE + 1)?;
    (index % 3 == 0 && index / 3 < u32::from(KEY_COUNT)).then_some((index / 3) as u16)
}
pub(super) fn repaint(m: &mut Mount) -> Result<()> {
    let mut d = m.designer.take().ok_or("Missing designer")?;
    let result = (|| {
        let authoring_seed = m.design_doc().oib_authoring_seed()?;
        let seed = &authoring_seed;
        let properties = d.properties.clone();
        let objects = d.objects.clone();
        let status = m.oib_view.as_ref().map_or("", |v| v.info.status.as_str());
        // Draft, notice, focus and details changes do not rebuild the shared
        // hierarchy or its action generations. Structural activation refreshes
        // those through Mount::refresh_designer before this local repaint.
        d.rebuild_panels(
            seed,
            objects,
            &properties,
            &m.design_doc().oib_conflicts(),
            status,
            m.binding.as_ref().is_none_or(|b| b.ui_editable),
            false,
        )
    })();
    m.designer = Some(d);
    result?;
    m.invalidate();
    let layout = resolve_layout(m)?;
    if let Some(d) = &mut m.designer {
        d.sync(&layout)?;
    }
    Ok(())
}
impl Client {
    pub(in crate::canvas::client) fn designer_details(
        &mut self,
        address: &PropertyAddress,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let authoring_seed = m.design_doc().oib_authoring_seed()?;
        let seed = &authoring_seed;
        let trace = sources(m.design_doc(), seed, address)?;
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        d.details = if d.details.as_ref().is_some_and(|d| &d.address == address) {
            None
        } else {
            Some(trace)
        };
        repaint(m)?;
        Ok(true)
    }
    pub(in crate::canvas::client) fn designer_select_after_blur(
        &mut self,
        selection: Selection,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        self.designer_blur_focused()?;
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let blocked = m.oib_activation_blocked();
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        if !d.objects.iter().any(|o| o.selection == selection) {
            d.editor_notice = Some("Selection is no longer available".into());
            d.revision = None;
            repaint(m)?;
            return Ok(false);
        }
        d.pending_source = None;
        if blocked && d.selection != selection {
            d.pending_selection = Some(selection);
            d.editor_notice = Some("Selection opens after confirmation".into());
            d.revision = None;
            repaint(m)?;
            return Ok(false);
        }
        d.pending_selection = None;
        self.inspect_oib_selection(selection)
    }
    pub(in crate::canvas::client) fn designer_go_source(
        &mut self,
        address: &PropertyAddress,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        self.designer_blur_focused()?;
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.oib_activation_blocked() {
            let d = m.designer.as_mut().ok_or("Missing designer")?;
            d.pending_source = Some(address.clone());
            d.pending_selection = None;
            d.editor_notice = Some("Source opens after confirmation".into());
            d.revision = None;
            repaint(m)?;
            return Ok(false);
        }
        if let Some(d) = self.active.as_mut().and_then(|m| m.designer.as_mut()) {
            d.pending_source = None;
            d.pending_selection = None;
        }
        let target = selection(address);
        self.inspect_oib_selection(target.clone())?;
        if self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .is_none_or(|d| d.selection != target)
        {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let authoring_seed = m.design_doc().oib_authoring_seed()?;
        let seed = &authoring_seed;
        let trace = m.design_doc().oib_style_sources(seed, address)?;
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        d.details = Some(trace);
        d.workspace_task = Task::Inspect;
        repaint(m)?;
        Ok(true)
    }
    pub(in crate::canvas::client) fn designer_choose_property(
        &mut self,
        view: &PropertyRegisterView,
        value: Option<Value>,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        m.sync_working_copy()?;
        let result = m
            .working_copy
            .as_mut()
            .ok_or("Missing working copy")?
            .stage_choice(view, value);
        let accepted = result.as_ref().is_ok_and(|changed| *changed);
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        if accepted {
            d.drafts.cancel(&view.address);
        }
        d.editor_notice = match result {
            Err(_) => Some("Choices changed - inspect again".into()),
            Ok(_) => None,
        };
        d.revision = None;
        if accepted {
            m.working_changed()?;
        }
        repaint(m)?;
        Ok(accepted)
    }

    pub(in crate::canvas::client) fn designer_preview_color(
        &mut self,
        address: &PropertyAddress,
        color: Option<[u8; 4]>,
        opacity: Option<u8>,
        boolean: Option<bool>,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        let result = if let Some(color) = color {
            d.drafts.set_color(address, color)
        } else if let Some(opacity) = opacity {
            d.drafts.set_opacity(address, f32::from(opacity))
        } else if let Some(value) = boolean {
            d.drafts.set_boolean(address, value)
        } else {
            return Ok(false);
        };
        result.map_err(|e| format!("Property preview: {e:?}"))?;
        repaint(m)?;
        Ok(true)
    }
    // Trusted keyboard traversal may reveal only a current field with a live
    // typed action. Public activation retains its separate visible-ID guard.
    fn reveal_editor_field(&mut self, id: u32) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let Some(m) = self.active.as_mut() else {
            return Ok(false);
        };
        let layout = cached_layout(m)?;
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        if d.actions.get(&id) != Some(&LocalAction::FocusField(id)) {
            return Ok(false);
        }
        let Some(panel) = d.panels.last_mut() else {
            return Ok(false);
        };
        let Some(field) = panel.nodes.iter().find(|n| n.id == id) else {
            return Ok(false);
        };
        let path = layout
            .path(u32::from(panel.viewport))
            .ok_or("Missing inspector viewport")?;
        let height = path
            .last()
            .ok_or("Missing inspector frame")?
            .node
            .frame
            .height;
        let offset = panel.scroll.info().offset[1];
        let next = if field.frame.y < offset {
            field.frame.y
        } else if field.frame.y + field.frame.height > offset + height {
            field.frame.y + field.frame.height - height
        } else {
            offset
        };
        panel.scroll.set_offset([0., next]);
        m.invalidate_presentation();
        Ok(true)
    }
    pub(in crate::canvas::client) fn designer_focus_field(&mut self, id: u32) -> Result<bool> {
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.working_copy.as_ref().is_some_and(|w| w.applying()))
        {
            return Ok(false);
        }
        if self.input_busy() {
            return Ok(false);
        }
        let scene = self.scene()?;
        if !scene.controls().iter().any(|c| c.node.id == id) {
            return Ok(false);
        }
        let Some(m) = self.active.as_ref() else {
            return Ok(false);
        };
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        let d = m.designer.as_ref().ok_or("Missing designer")?;
        let target = d
            .drafts
            .drafts()
            .iter()
            .find(|draft| draft.fields.iter().any(|f| f.id == id))
            .map(|d| d.address.clone());
        let Some(target) = target else {
            return Ok(false);
        };
        let old = d.drafts.focused().map(|f| f.address.clone());
        if old.as_ref().is_some_and(|old| old != &target) {
            self.designer_blur_focused()?;
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        d.drafts
            .focus(id)
            .map_err(|e| format!("Property focus: {e:?}"))?;
        let end = d
            .drafts
            .draft(&target)
            .and_then(|d| d.fields.iter().find(|f| f.id == id))
            .map_or(0, |f| f.buffer().text().len());
        d.drafts
            .place_cursor(id, end)
            .map_err(|e| format!("Property caret: {e:?}"))?;
        d.workspace_task = Task::Inspect;
        m.keyboard.focus();
        repaint(m)?;
        self.reveal_editor_field(id)?;
        Ok(true)
    }
    pub(in crate::canvas::client) fn designer_commit_property(
        &mut self,
        address: &PropertyAddress,
        blur: bool,
    ) -> Result<bool> {
        if self.input_busy() {
            return Ok(false);
        }
        let m = self.active.as_mut().ok_or("No Canvas")?;
        if m.binding.as_ref().is_some_and(|b| !b.ui_editable) {
            return Ok(false);
        }
        m.sync_working_copy()?;
        let authoring_seed = m.design_doc().oib_authoring_seed()?;
        let seed = &authoring_seed;
        let fresh = match address {
            PropertyAddress::Placement { object, .. } => {
                m.design_doc().oib_placement_properties(object)?
            }
            PropertyAddress::Structural { .. } => m
                .design_doc()
                .oib_structural_properties(&selection(address))?,
            _ => super::super::super::oib::document::properties(
                seed,
                &m.design_doc().oib_allocations()?,
                &selection(address),
                &m.design_doc().oib_active()?,
            )?,
        }
        .into_iter()
        .find(|p| p.address.as_ref() == Some(address));
        let structural = matches!(
            address,
            PropertyAddress::Structural { .. } | PropertyAddress::Placement { .. }
        );
        let stale_working = m.working_copy.as_ref().is_some_and(|w| w.stale(&m.doc));
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        // Candidate admission uses base+1 for every private command. The editor
        // metadata is the working-copy revision and therefore also distinguishes
        // successive private changes while preserving the draft's captured base.
        let revision = if structural {
            d.editor_structural_revision
        } else {
            d.editor_revision
        };
        let result = fresh
            .ok_or("Property unavailable".to_string())
            .and_then(|fresh| {
                d.drafts
                    .prepare(address, &fresh, revision)
                    .map_err(|e| format!("{e:?}"))
            });
        let mut accepted = false;
        match result {
            Ok(candidate) => {
                let result = if structural && (candidate.base_revision != revision || stale_working)
                {
                    Err("Document changed; Revert this field and review the current design".into())
                } else {
                    m.stage_property(address, Some(candidate.value.clone()))
                };
                let failure = result.as_ref().err().cloned();
                // An identical staged value is already part of the working
                // copy. Done still acknowledges the field without a new edit.
                accepted = result.is_ok();
                let d = m.designer.as_mut().ok_or("Missing designer")?;
                let current_revision = if structural {
                    d.editor_structural_revision
                } else {
                    d.editor_revision
                };
                let acknowledgement = if accepted {
                    d.drafts.acknowledge_at(&candidate, true, current_revision)
                } else {
                    d.drafts.acknowledge(&candidate, false)
                };
                acknowledgement.map_err(|e| format!("Property acknowledgement: {e:?}"))?;
                d.editor_notice = failure;
            }
            Err(_) => {
                m.designer.as_mut().ok_or("Missing designer")?.editor_notice =
                    Some("Invalid / unavailable edit retained".into())
            }
        }
        if blur {
            m.designer
                .as_mut()
                .ok_or("Missing designer")?
                .drafts
                .clear_focus();
            m.keyboard.hide();
        }
        repaint(m)?;
        Ok(accepted)
    }
    pub(in crate::canvas::client) fn designer_discard_property(
        &mut self,
        address: &PropertyAddress,
    ) -> Result<bool> {
        let m = self.active.as_mut().ok_or("No Canvas")?;
        let d = m.designer.as_mut().ok_or("Missing designer")?;
        let changed = d.drafts.cancel(address);
        d.editor_notice = None;
        if !d.editor_focused() {
            m.keyboard.hide();
        }
        repaint(m)?;
        Ok(changed)
    }
    pub(in crate::canvas::client) fn designer_blur_focused(&mut self) -> Result<bool> {
        let Some(m) = &self.active else {
            return Ok(false);
        };
        let Some(d) = &m.designer else {
            return Ok(false);
        };
        let Some(focus) = d.drafts.focused() else {
            return Ok(false);
        };
        let address = focus.address.clone();
        let dirty = d.drafts.draft(&address).is_some_and(|d| d.dirty);
        if dirty {
            self.designer_commit_property(&address, true)?;
        } else {
            let m = self.active.as_mut().ok_or("No Canvas")?;
            m.designer
                .as_mut()
                .ok_or("Missing designer")?
                .drafts
                .clear_focus();
            m.keyboard.hide();
            repaint(m)?;
        }
        Ok(true)
    }
    pub(in crate::canvas::client) fn designer_blur_input(
        &mut self,
        id: u32,
    ) -> Result<Option<bool>> {
        let belongs = self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .is_some_and(|d| {
                d.drafts.focused().is_some_and(|f| {
                    d.drafts.draft(&f.address).is_some_and(|d| {
                        d.fields
                            .iter()
                            .any(|field| field.id == id && field.part == f.part)
                    })
                })
            });
        if belongs {
            self.designer_blur_focused().map(Some)
        } else {
            Ok(None)
        }
    }
    pub(in crate::canvas::client) fn designer_edit(&mut self, edit: &Edit) -> Result<Option<bool>> {
        let Some(m) = &mut self.active else {
            return Ok(None);
        };
        if m.working_copy.as_ref().is_some_and(|w| w.applying()) {
            return Ok(Some(false));
        }
        let Some(d) = &mut m.designer else {
            return Ok(None);
        };
        let Some(focus) = d.drafts.focused().cloned() else {
            return Ok(None);
        };
        let id = d
            .drafts
            .draft(&focus.address)
            .and_then(|d| d.field(focus.part))
            .ok_or("Missing property field")?
            .id;
        let edit = d.drafts.edit(id, edit);
        match edit {
            Ok(EditResult::CommitRequested) => self
                .designer_commit_property(&focus.address, true)
                .map(Some),
            Ok(EditResult::Traverse { backward }) => {
                let fields: Vec<_> = d
                    .properties
                    .iter()
                    .filter_map(|p| p.address.as_ref())
                    .filter_map(|a| d.drafts.draft(a))
                    .flat_map(|d| d.fields.iter().map(|f| f.id))
                    .collect();
                let next = fields.iter().position(|f| *f == id).map(|i| {
                    fields[(i + if backward { fields.len() - 1 } else { 1 }) % fields.len()]
                });
                if let Some(next) = next {
                    if !self.reveal_editor_field(next)? {
                        return Ok(Some(false));
                    }
                    self.designer_focus_field(next).map(Some)
                } else {
                    Ok(Some(false))
                }
            }
            Ok(EditResult::Local(changed)) => {
                repaint(m)?;
                Ok(Some(changed))
            }
            Err(_) => {
                d.editor_notice = Some("Invalid input retained".into());
                repaint(m)?;
                Ok(Some(true))
            }
        }
    }
    /// Explicit local cancellation; browser Escape never commits a tooling draft.
    pub fn cancel_text_input(&mut self, epoch: u32, id: u32) -> Result<bool> {
        if epoch != self.epoch() {
            return Ok(false);
        }
        let address = self
            .active
            .as_ref()
            .and_then(|m| m.designer.as_ref())
            .and_then(|d| d.drafts.focused())
            .map(|f| f.address.clone());
        if let Some(address) = address {
            let valid = self.text_inputs()?.iter().any(|i| i.id == id && i.focused);
            return if valid {
                self.designer_discard_property(&address)
            } else {
                Ok(false)
            };
        }
        self.blur_text_input(epoch, id)
    }
}

#[cfg(test)]
mod staging_tests {
    use super::*;

    #[test]
    fn done_stages_repeated_focused_edits_without_live_writes() {
        let (mut client, _authority, _session) = super::super::tests::mounted_fixture_session(true);
        let address = super::super::tests::editor_address(3);
        client.inspect_oib_selection(selection(&address)).unwrap();
        let live_root = client.active.as_ref().unwrap().doc.oib_root();
        let live_generation = client.active.as_ref().unwrap().doc.oib_generation();
        let field = super::super::tests::focus_address(&mut client, address.clone());
        for text in ["Private label", "Second label"] {
            assert!(
                client
                    .edit_text_input(
                        client.epoch(),
                        field,
                        &Edit::Replace {
                            text: text.into(),
                            cursor: text.len() as u32,
                            anchor: text.len() as u32,
                            selection_only: false,
                        }
                    )
                    .unwrap()
            );
            assert!(client.designer_commit_property(&address, false).unwrap());
            let m = client.active.as_ref().unwrap();
            assert_eq!(
                m.designer
                    .as_ref()
                    .unwrap()
                    .drafts
                    .focused()
                    .unwrap()
                    .address,
                address
            );
            assert_eq!(m.doc.oib_root(), live_root);
            assert_eq!(m.doc.oib_generation(), live_generation);
            let source = m.design_doc();
            let properties = crate::canvas::oib::document::properties(
                &source.oib_authoring_seed().unwrap(),
                &source.oib_allocations().unwrap(),
                &selection(&address),
                &source.oib_active().unwrap(),
            )
            .unwrap();
            assert_eq!(
                properties
                    .iter()
                    .find(|p| p.address.as_ref() == Some(&address))
                    .unwrap()
                    .value,
                Some(Value::Text(text.into()))
            );
        }
        assert!(
            client
                .active
                .as_ref()
                .unwrap()
                .working_copy
                .as_ref()
                .unwrap()
                .dirty()
        );
        assert!(
            client
                .active
                .as_mut()
                .unwrap()
                .doc
                .generate()
                .unwrap()
                .is_none()
        );
    }
}
