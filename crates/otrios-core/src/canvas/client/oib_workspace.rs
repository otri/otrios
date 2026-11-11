// SPDX-License-Identifier: LGPL-3.0-only
//! Device-local responsive Designer chrome. Authored preview paint is never themed.
use crate::dashboard::{
    components::{background, canvas, text},
    scene::Scene,
};
use otrios_interfaces::{
    oib::{Seed, designer::DesignerMode},
    scene::{Action, Color, Primitive, Rect, SceneNode, SwitchSemantics, TouchResponder},
    theme,
};

const BASE: u32 = 2_680_000_000;
pub(super) const TAB_LIBRARY: u32 = BASE + 1;
pub(super) const TAB_LAYERS: u32 = BASE + 2;
pub(super) const TAB_INSPECT: u32 = BASE + 3;
pub(super) const MODE_EDIT: u32 = BASE + 4;
pub(super) const MODE_TEST: u32 = BASE + 5;
pub(super) const APPEARANCE_DARK: u32 = BASE + 6;
pub(super) const APPEARANCE_LIGHT: u32 = BASE + 7;
pub(super) const STATUS: u32 = BASE + 20;
pub(super) const PROPERTIES_TITLE: u32 = BASE + 21;
pub(super) const PROPERTIES_ACTIONS: u32 = BASE + 22;
pub(super) const NEW_COMPONENT: u32 = BASE + 40;
pub(super) const UNDO: u32 = BASE + 41;
pub(super) const REDO: u32 = BASE + 42;
pub(super) const DISCARD: u32 = BASE + 43;
pub(super) const APPLY: u32 = BASE + 44;
pub(super) const MODAL_ACTION_START: u32 = BASE + 400;
pub(super) const MODAL_ACTION_END: u32 = BASE + 420;
pub(super) const MODAL_CONTENT: u32 = BASE + 505;
pub(super) const MODAL_LINK_START: u32 = BASE + 10_000;
pub(super) const MODAL_LINK_END: u32 = MODAL_LINK_START + 255;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EditorChrome {
    pub(super) management: bool,
    pub(super) new_component: bool,
    pub(super) undo: bool,
    pub(super) redo: bool,
    pub(super) discard: bool,
    pub(super) apply: bool,
    pub(super) modal: Option<Modal>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Modal {
    pub(super) title: String,
    pub(super) lines: Vec<String>,
    /// Navigable original lines. Wrapping never creates separate commands.
    pub(super) links: Vec<ModalLink>,
    pub(super) actions: Vec<ModalAction>,
    /// None keeps a small decision fully visible; Some enables bounded scrolling.
    pub(super) scroll: Option<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModalLink {
    pub(super) line: usize,
    pub(super) id: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModalAction {
    pub(super) id: u32,
    pub(super) label: String,
    pub(super) enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Task {
    Library,
    Layers,
    #[default]
    Inspect,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Appearance {
    #[default]
    Dark,
    Light,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Palette {
    pub(super) background: Color,
    pub(super) surface: Color,
    pub(super) text: Color,
    pub(super) muted: Color,
    pub(super) line: Color,
    pub(super) selected: Color,
    pub(super) focus: Color,
}
pub(super) fn palette(appearance: Appearance) -> Palette {
    match appearance {
        Appearance::Dark => Palette {
            background: [0, 0, 0, 255],
            surface: [16, 16, 16, 255],
            text: [245, 245, 245, 255],
            muted: [184, 184, 184, 255],
            line: [112, 112, 112, 255],
            selected: [245, 245, 245, 255],
            focus: [255, 255, 255, 255],
        },
        Appearance::Light => Palette {
            background: [255, 255, 255, 255],
            surface: [244, 244, 244, 255],
            text: [21, 21, 21, 255],
            muted: [82, 82, 82, 255],
            line: [112, 112, 112, 255],
            selected: [21, 21, 21, 255],
            focus: [0, 0, 0, 255],
        },
    }
}
impl Appearance {
    pub(super) fn palette(self) -> Palette {
        palette(self)
    }
}
impl Palette {
    /// Only call for explicit chrome roles, never arbitrary authored scene colors.
    pub(super) fn role(self, color: Color) -> Color {
        match color {
            theme::BG => self.background,
            theme::CARD => self.surface,
            theme::TEXT => self.text,
            theme::MUTED => self.muted,
            theme::LINE => self.line,
            theme::BRASS | theme::COPPER => self.focus,
            _ => color,
        }
    }
}

fn find(node: &SceneNode, id: u32) -> Option<&SceneNode> {
    if node.id == id {
        Some(node)
    } else {
        node.children.iter().find_map(|child| find(child, id))
    }
}
fn back(node: &SceneNode, action: Option<u8>) -> Option<&SceneNode> {
    if action
        .is_some_and(|id| node.responder == Some(TouchResponder::Activate(Action::AppEvent(id))))
    {
        Some(node)
    } else {
        node.children.iter().find_map(|child| back(child, action))
    }
}
fn decorative(mut node: SceneNode) -> SceneNode {
    node.responder = Some(TouchResponder::PassThrough);
    node.semantics = None;
    node
}
fn rect(id: u32, frame: Rect, color: Color, radius: f32) -> SceneNode {
    let mut node = background(id, frame.width, frame.height, color, radius);
    node.frame = frame;
    decorative(node)
}
fn label(id: u32, frame: Rect, value: &str, scale: u32, color: Color) -> SceneNode {
    decorative(text(id, frame, value, scale, color))
}
fn control(id: u32, frame: Rect, value: &str, selected: bool, tab: bool, p: Palette) -> SceneNode {
    let mut children = Vec::new();
    if tab {
        children.push(rect(
            id + 1000,
            Rect::new(4., 45., (frame.width - 8.).max(1.), 3.),
            if selected { p.text } else { p.background },
            0.,
        ));
    } else {
        children.push(rect(
            id + 1000,
            Rect::new(2., 2., (frame.width - 4.).max(1.), 44.),
            if selected { p.selected } else { p.surface },
            5.,
        ));
    }
    children.push(label(
        id + 2000,
        Rect::new(8., 10., (frame.width - 16.).max(1.), 30.),
        value,
        2,
        if selected && !tab {
            p.background
        } else if selected {
            p.text
        } else {
            p.muted
        },
    ));
    let mut node = canvas(id, frame, children);
    node.responder = Some(TouchResponder::Activate(Action::KeyboardKey(u16::MAX)));
    node.semantics = Some(SwitchSemantics {
        label: value.into(),
        checked: selected,
    });
    node
}
fn command(id: u32, frame: Rect, value: &str, enabled: bool, p: Palette) -> SceneNode {
    let mut node = canvas(
        id,
        frame,
        vec![
            rect(id + 1000, Rect::new(0., 0., frame.width, 48.), p.line, 5.),
            rect(
                id + 1100,
                Rect::new(1., 1., frame.width - 2., 46.),
                p.surface,
                4.,
            ),
            label(
                id + 2000,
                Rect::new(12., 10., frame.width - 24., 28.),
                value,
                2,
                if enabled { p.text } else { p.muted },
            ),
        ],
    );
    // Disabled commands remain visible, consume taps, and are absent from the
    // actionable semantic list until Scene supports a disabled semantic state.
    node.responder = Some(if enabled {
        TouchResponder::Activate(Action::KeyboardKey(u16::MAX))
    } else {
        TouchResponder::Consume
    });
    if enabled {
        node.semantics = Some(SwitchSemantics {
            label: value.into(),
            checked: false,
        });
    } else {
        node.children[0].responder = Some(TouchResponder::Consume);
    }
    node
}
pub(super) fn wrapped_lines(lines: &[String], columns: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in lines {
        let mut current = String::new();
        for word in line.split_whitespace() {
            if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > columns {
                result.push(std::mem::take(&mut current));
            }
            for character in word.chars() {
                if current.chars().count() == columns {
                    result.push(std::mem::take(&mut current));
                }
                current.push(character);
            }
            current.push(' ');
        }
        result.push(current.trim_end().to_string());
    }
    result
}
fn modal_overlay(
    modal: &Modal,
    width: f32,
    available_height: f32,
    root_height: f32,
    p: Palette,
) -> Result<SceneNode, String> {
    if modal.scroll.is_some() || !modal.links.is_empty() {
        return scrolling_modal_overlay(modal, width, available_height, root_height, p);
    }
    let card_width = (width - 64.).clamp(288., 760.);
    let content_width = card_width - 48.;
    let columns = ((content_width / 12.).floor() as usize).clamp(1, 72);
    let lines = wrapped_lines(&modal.lines, columns);
    let title = wrapped_lines(std::slice::from_ref(&modal.title), columns);
    if lines.len() + title.len() > 48 || modal.actions.len() > 21 {
        return Err("Designer dialog exceeds visible content limit".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut buttons = Vec::new();
    let mut x = 24.;
    let action_top = 32. + title.len() as f32 * 24. + 16. + lines.len() as f32 * 24. + 24.;
    let mut y = action_top;
    for action in &modal.actions {
        if !(MODAL_ACTION_START..=MODAL_ACTION_END).contains(&action.id) || !ids.insert(action.id) {
            return Err("Invalid Designer dialog action".into());
        }
        let button_width = (action.label.chars().count() as f32 * 12. + 32.).max(112.);
        if button_width > content_width {
            return Err("Designer dialog action label is too long".into());
        }
        if x > 24. && x + button_width > card_width - 24. {
            x = 24.;
            y += 56.;
        }
        buttons.push(command(
            action.id,
            Rect::new(x, y, button_width, 48.),
            &action.label,
            action.enabled,
            p,
        ));
        x += button_width + 8.;
    }
    let card_height = if modal.actions.is_empty() {
        action_top
    } else {
        y + 48.
    } + 24.;
    if card_height > available_height - 32. {
        return Err("Designer dialog does not fit viewport".into());
    }
    let mut card_children = vec![
        rect(
            BASE + 501,
            Rect::new(0., 0., card_width, card_height),
            p.line,
            8.,
        ),
        rect(
            BASE + 502,
            Rect::new(1., 1., card_width - 2., card_height - 2.),
            p.surface,
            7.,
        ),
    ];
    let mut text_y = 24.;
    for (index, line) in title.iter().chain(lines.iter()).enumerate() {
        if index == title.len() {
            text_y += 16.;
        }
        card_children.push(label(
            BASE + 600 + index as u32,
            Rect::new(24., text_y, content_width, 24.),
            line,
            2,
            if index < title.len() { p.text } else { p.muted },
        ));
        text_y += 24.;
    }
    card_children.extend(buttons);
    let card = canvas(
        BASE + 503,
        Rect::new(
            (width - card_width) / 2.,
            (available_height - card_height) / 2.,
            card_width,
            card_height,
        ),
        card_children,
    );
    let mut backdrop = background(BASE + 504, width, root_height, [0, 0, 0, 176], 0.);
    backdrop.responder = Some(TouchResponder::Consume);
    let mut overlay = canvas(
        BASE + 500,
        Rect::new(0., 0., width, root_height),
        vec![backdrop, card],
    );
    overlay.responder = Some(TouchResponder::Consume);

    Ok(overlay)
}
struct ScrollDialog {
    card_width: f32,
    content_width: f32,
    title: Vec<String>,
    rows: Vec<DialogRow>,
    buttons: Vec<(Rect, usize)>,
    body_top: f32,
    body_height: f32,
    content_height: f32,
    card_height: f32,
}
struct DialogRow {
    y: f32,
    height: f32,
    lines: Vec<String>,
    link: Option<u32>,
    text_index: usize,
    label: String,
}
fn dialog_rows(
    modal: &Modal,
    content_width: f32,
    validate_ids: bool,
) -> Result<(Vec<DialogRow>, f32), String> {
    let columns = ((content_width / 12.).floor() as usize).clamp(1, 72);
    let mut linked = std::collections::BTreeMap::new();
    let mut link_ids = std::collections::BTreeSet::new();
    for link in &modal.links {
        if validate_ids && !(MODAL_LINK_START..=MODAL_LINK_END).contains(&link.id)
            || link.line >= modal.lines.len()
            || modal.lines[link.line].trim().is_empty()
            || linked.insert(link.line, link.id).is_some()
            || !link_ids.insert(link.id)
        {
            return Err("Invalid Designer dialog link".into());
        }
    }
    let mut rows = vec![];
    let mut content_height = 0.;
    let mut text_index = 0;
    for (index, line) in modal.lines.iter().enumerate() {
        let link = linked.get(&index).copied();
        let columns = if link.is_some() {
            (((content_width - 24.) / 12.).floor() as usize).clamp(1, 72)
        } else {
            columns
        };
        let lines = wrapped_lines(std::slice::from_ref(line), columns);
        let height = if link.is_some() {
            (lines.len() as f32 * 24. + 16.).max(48.)
        } else {
            lines.len() as f32 * 24.
        };
        rows.push(DialogRow {
            y: content_height,
            height,
            lines,
            link,
            text_index,
            label: line.clone(),
        });
        content_height += height;
        text_index += rows.last().unwrap().lines.len();
    }
    let content_height = content_height.max(48.);
    Ok((rows, content_height))
}
/// Measure the complete list before deciding whether the existing 8192px body
/// limit requires bounded paging. Rendering still validates every action ID.
pub(super) fn modal_content_height(modal: &Modal, width: f32) -> Result<f32, String> {
    let content_width = (width - 64.).clamp(288., 760.) - 48.;
    Ok(dialog_rows(modal, content_width, false)?.1)
}
fn scroll_dialog(modal: &Modal, width: f32, available_height: f32) -> Result<ScrollDialog, String> {
    let card_width = (width - 64.).clamp(288., 760.);
    let content_width = card_width - 48.;
    let columns = ((content_width / 12.).floor() as usize).clamp(1, 72);
    let title = wrapped_lines(std::slice::from_ref(&modal.title), columns);
    let (rows, content_height) = dialog_rows(modal, content_width, true)?;
    if title.len() > 4 || content_height > 8192. || modal.actions.len() > 21 {
        return Err("Designer review content exceeds its bounded viewport".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut buttons = vec![];
    let (mut x, mut y) = (24., 0.);
    for (index, action) in modal.actions.iter().enumerate() {
        if !(MODAL_ACTION_START..=MODAL_ACTION_END).contains(&action.id) || !ids.insert(action.id) {
            return Err("Invalid Designer dialog action".into());
        }
        let button_width = (action.label.chars().count() as f32 * 12. + 32.).max(112.);
        if button_width > content_width {
            return Err("Designer dialog action label is too long".into());
        }
        if x > 24. && x + button_width > card_width - 24. {
            x = 24.;
            y += 56.;
        }
        buttons.push((Rect::new(x, y, button_width, 48.), index));
        x += button_width + 8.;
    }
    let footer_height = if buttons.is_empty() { 0. } else { y + 48. };
    let body_top = 24. + title.len() as f32 * 24. + 16.;
    let body_height =
        content_height.min(available_height - 32. - body_top - 24. - footer_height - 24.);
    if body_height < 48. {
        return Err("Designer review does not fit viewport".into());
    }
    let card_height = body_top + body_height + 24. + footer_height + 24.;
    Ok(ScrollDialog {
        card_width,
        content_width,
        title,
        rows,
        buttons,
        body_top,
        body_height,
        content_height,
        card_height,
    })
}
pub(super) fn modal_scroll_limit(modal: &Modal, width: f32, height: f32) -> Result<u16, String> {
    let layout = scroll_dialog(modal, width, height)?;
    Ok((layout.content_height - layout.body_height).max(0.).ceil() as u16)
}
fn scrolling_modal_overlay(
    modal: &Modal,
    width: f32,
    available_height: f32,
    root_height: f32,
    p: Palette,
) -> Result<SceneNode, String> {
    let layout = scroll_dialog(modal, width, available_height)?;
    let ScrollDialog {
        card_width,
        content_width,
        title,
        rows,
        buttons,
        body_top,
        body_height,
        content_height,
        card_height,
    } = layout;
    let offset =
        f32::from(modal.scroll.unwrap_or(0)).clamp(0., (content_height - body_height).max(0.));
    let mut children = vec![
        rect(
            BASE + 501,
            Rect::new(0., 0., card_width, card_height),
            p.line,
            8.,
        ),
        rect(
            BASE + 502,
            Rect::new(1., 1., card_width - 2., card_height - 2.),
            p.surface,
            7.,
        ),
    ];
    for (index, line) in title.iter().enumerate() {
        children.push(label(
            BASE + 560 + index as u32,
            Rect::new(24., 24. + index as f32 * 24., content_width, 24.),
            line,
            2,
            p.text,
        ));
    }
    let mut content_backdrop = rect(
        BASE + 506,
        Rect::new(0., 0., content_width, body_height),
        p.surface,
        0.,
    );
    content_backdrop.responder = Some(TouchResponder::Consume);
    let mut body = vec![content_backdrop];
    for row in &rows {
        let y = row.y - offset;
        if y + row.height <= 0. || y >= body_height {
            continue;
        }
        if let Some(id) = row.link {
            let mut children = vec![
                rect(
                    id + 1000,
                    Rect::new(0., 0., content_width, row.height),
                    p.line,
                    5.,
                ),
                rect(
                    id + 2000,
                    Rect::new(1., 1., content_width - 2., row.height - 2.),
                    p.surface,
                    4.,
                ),
            ];
            for (index, line) in row.lines.iter().enumerate() {
                let line_y = y + 8. + index as f32 * 24.;
                if line_y + 24. <= 0. || line_y >= body_height {
                    continue;
                }
                children.push(label(
                    BASE + 14_000 + (row.text_index + index) as u32,
                    Rect::new(12., 8. + index as f32 * 24., content_width - 24., 24.),
                    line,
                    2,
                    p.text,
                ));
            }
            let mut node = canvas(id, Rect::new(0., y, content_width, row.height), children);
            node.responder = Some(TouchResponder::Activate(Action::KeyboardKey(u16::MAX)));
            let mut semantic_label = row.label.clone();
            if semantic_label.len() > 96 {
                while semantic_label.len() > 93 {
                    semantic_label.pop();
                }
                semantic_label.push_str("...");
            }
            node.semantics = Some(SwitchSemantics {
                label: semantic_label,
                checked: false,
            });
            body.push(node);
        } else {
            for (index, line) in row.lines.iter().enumerate() {
                let line_y = y + index as f32 * 24.;
                if line_y + 24. >= 0. && line_y <= body_height {
                    body.push(label(
                        BASE + 600 + (row.text_index + index) as u32,
                        Rect::new(0., line_y, content_width, 24.),
                        line,
                        2,
                        p.muted,
                    ));
                }
            }
        }
    }
    let mut viewport = canvas(
        MODAL_CONTENT,
        Rect::new(24., body_top, content_width, body_height),
        body,
    );
    viewport.responder = Some(TouchResponder::Consume);
    children.push(viewport);
    for (mut frame, index) in buttons {
        frame.y += body_top + body_height + 24.;
        let action = &modal.actions[index];
        children.push(command(action.id, frame, &action.label, action.enabled, p));
    }
    let card = canvas(
        BASE + 503,
        Rect::new(
            (width - card_width) / 2.,
            (available_height - card_height) / 2.,
            card_width,
            card_height,
        ),
        children,
    );
    let mut backdrop = background(BASE + 504, width, root_height, [0, 0, 0, 176], 0.);
    backdrop.responder = Some(TouchResponder::Consume);
    let mut overlay = canvas(
        BASE + 500,
        Rect::new(0., 0., width, root_height),
        vec![backdrop, card],
    );
    overlay.responder = Some(TouchResponder::Consume);
    Ok(overlay)
}
/// Dialog scrolling changes only this local overlay; authored layout remains cached.
pub(super) fn present_modal(
    root: &mut SceneNode,
    modal: Option<&Modal>,
    keyboard: bool,
    appearance: Appearance,
) -> Result<(), String> {
    root.children.retain(|node| node.id != BASE + 500);
    if let Some(modal) = modal {
        let height = (root.frame.height - if keyboard { 240. } else { 0. }).max(1.);
        root.children.push(modal_overlay(
            modal,
            root.frame.width,
            height,
            root.frame.height,
            appearance.palette(),
        )?);
    }
    Ok(())
}
fn segment_surface(id: u32, frame: Rect, p: Palette) -> SceneNode {
    let mut node = canvas(
        id,
        frame,
        vec![
            rect(id + 1, Rect::new(0., 0., frame.width, 48.), p.line, 7.),
            rect(
                id + 2,
                Rect::new(1., 1., frame.width - 2., 46.),
                p.surface,
                6.,
            ),
        ],
    );
    node.responder = Some(TouchResponder::PassThrough);
    node
}
fn hide(node: &mut SceneNode) {
    node.frame.x = -4096.;
    node.frame.y = -4096.;
    node.primitive = Primitive::Canvas { clip: true };
    node.children.clear();
    node.responder = None;
    node.semantics = None;
}

/// Reposition only signed viewport containers; their authored descendants retain
/// exact frames, colors, responders and semantic identities. Inactive viewports
/// remain findable but carry no current input/paint children.
#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    scene: &Scene,
    seed: &Seed,
    back_action: Option<u8>,
    keyboard: bool,
    task: Task,
    mode: DesignerMode,
    appearance: Appearance,
    editor: &EditorChrome,
) -> Result<Scene, String> {
    let management = editor.management;
    let metadata = seed.designer.as_ref().ok_or("Missing designer metadata")?;
    let hierarchy_id = metadata
        .hierarchy_viewport
        .ok_or("Missing hierarchy viewport")?;
    let preview_id = seed
        .instance_surface
        .as_ref()
        .ok_or("Missing designer preview")?
        .preview_viewport;
    let mut preview = find(scene.root(), u32::from(preview_id))
        .ok_or("Missing preview viewport")?
        .clone();
    let mut hierarchy = find(scene.root(), u32::from(hierarchy_id))
        .ok_or("Missing hierarchy viewport")?
        .clone();
    let mut inspector = find(scene.root(), u32::from(seed.inspector_viewport))
        .ok_or("Missing inspector viewport")?
        .clone();
    let p = appearance.palette();
    let width = scene.root().frame.width;
    let available_height = (scene.root().frame.height - if keyboard { 240. } else { 0. }).max(1.);
    let mut chrome = vec![rect(
        BASE + 100,
        Rect::new(0., 0., width, scene.root().frame.height),
        p.background,
        0.,
    )];
    let mut preview_back = None;
    if let Some(original) = back(scene.root(), back_action) {
        let mut node = original.clone();
        node.frame = Rect::new(16., 12., 48., 48.);
        node.primitive = Primitive::Canvas { clip: true };
        node.children = vec![
            rect(BASE + 101, Rect::new(0., 0., 48., 48.), p.surface, 6.),
            label(BASE + 102, Rect::new(16., 10., 28., 28.), "<", 2, p.text),
        ];
        preview_back = Some(node);
    } else if let Some(action) = back_action {
        let mut node = control(
            BASE + 30,
            Rect::new(16., 12., 48., 48.),
            "<",
            false,
            false,
            p,
        );
        node.responder = Some(TouchResponder::Activate(Action::AppEvent(action)));
        node.semantics = Some(SwitchSemantics {
            label: "Back to OTriOS".into(),
            checked: false,
        });
        preview_back = Some(node);
    }
    chrome.push(label(
        BASE + 103,
        Rect::new(80., 4., (width - 96.).max(1.), 40.),
        "OIB Designer",
        3,
        p.text,
    ));
    chrome.push(label(
        STATUS,
        Rect::new(80., 42., (width - 96.).max(1.), 28.),
        "All changes applied",
        2,
        p.muted,
    ));
    if let Some(node) = preview_back {
        chrome.push(node);
    }

    let mut x = 16.;
    let mut y = 104.;
    let mut group_origins = Vec::with_capacity(3);
    for group_width in [288., 192., 192.] {
        if x > 16. && x + group_width > width - 16. {
            x = 16.;
            y += 80.;
        }
        group_origins.push([x, y]);
        x += group_width + 16.;
    }
    let [nav, interaction, appearance_at] =
        <[[f32; 2]; 3]>::try_from(group_origins).map_err(|_| "Missing workspace groups")?;
    for (index, (id, value, selected)) in [
        (TAB_LIBRARY, "Library", task == Task::Library),
        (TAB_LAYERS, "Layers", task == Task::Layers),
        (TAB_INSPECT, "Inspect", task == Task::Inspect),
    ]
    .into_iter()
    .enumerate()
    {
        chrome.push(control(
            id,
            Rect::new(nav[0] + index as f32 * 96., nav[1], 96., 48.),
            value,
            selected,
            true,
            p,
        ));
    }
    for (group_id, origin, title) in [
        (BASE + 110, interaction, "Interaction"),
        (BASE + 120, appearance_at, "Appearance"),
    ] {
        chrome.push(label(
            group_id + 3,
            Rect::new(origin[0], origin[1] - 28., 192., 28.),
            title,
            2,
            p.muted,
        ));
        chrome.push(segment_surface(
            group_id,
            Rect::new(origin[0], origin[1], 192., 48.),
            p,
        ));
    }
    for (index, (id, value, selected)) in [
        (MODE_EDIT, "Edit", mode == DesignerMode::Edit),
        (MODE_TEST, "Test", mode == DesignerMode::Test),
    ]
    .into_iter()
    .enumerate()
    {
        chrome.push(control(
            id,
            Rect::new(
                interaction[0] + index as f32 * 96.,
                interaction[1],
                96.,
                48.,
            ),
            value,
            selected,
            false,
            p,
        ));
    }
    for (index, (id, value, selected)) in [
        (APPEARANCE_DARK, "Dark", appearance == Appearance::Dark),
        (APPEARANCE_LIGHT, "Light", appearance == Appearance::Light),
    ]
    .into_iter()
    .enumerate()
    {
        chrome.push(control(
            id,
            Rect::new(
                appearance_at[0] + index as f32 * 96.,
                appearance_at[1],
                96.,
                48.,
            ),
            value,
            selected,
            false,
            p,
        ));
    }
    let mut command_x = 16.;
    let mut command_y = y + 64.;
    for (id, value, enabled, command_width) in [
        (NEW_COMPONENT, "New component", editor.new_component, 192.),
        (UNDO, "Undo", editor.undo, 96.),
        (REDO, "Redo", editor.redo, 96.),
        (DISCARD, "Discard", editor.discard, 112.),
        (APPLY, "Apply", editor.apply, 112.),
    ] {
        if command_x > 16. && command_x + command_width > width - 16. {
            command_x = 16.;
            command_y += 56.;
        }
        chrome.push(command(
            id,
            Rect::new(command_x, command_y, command_width, 48.),
            value,
            enabled,
            p,
        ));
        command_x += command_width + 8.;
    }
    let header_height = command_y + 64.;
    chrome.push(rect(
        BASE + 130,
        Rect::new(0., header_height - 1., width, 1.),
        p.line,
        0.,
    ));
    let body_top = header_height + 16.;
    let body_height = (available_height - body_top - 16.).max(1.);
    let dock_width = 352_f32.max((width * 0.28).min(392.));
    let dock_x = (width - dock_width - 16.).max(16.);
    preview.frame = Rect::new(16., body_top, (dock_x - 32.).max(1.), body_height);
    let inspector_top = if management { 120. } else { 56. };
    let title = match task {
        Task::Library => "Library",
        Task::Layers => "Layers",
        Task::Inspect => "Inspect",
    };
    let mut dock_children = vec![
        rect(
            BASE + 131,
            Rect::new(0., 0., dock_width, body_height),
            p.surface,
            0.,
        ),
        label(
            PROPERTIES_TITLE,
            Rect::new(16., 8., dock_width - 32., 40.),
            title,
            3,
            p.text,
        ),
    ];
    // Inactive task viewports keep real dimensions so scroll bounds/anchors do
    // not collapse to a 1px viewport while their paint/input children are absent.
    inspector.frame = Rect::new(
        16.,
        inspector_top,
        dock_width - 32.,
        (body_height - inspector_top - 16.).max(1.),
    );
    hierarchy.frame = Rect::new(16., 56., dock_width - 32., (body_height - 72.).max(1.));
    if task == Task::Inspect {
        hide(&mut hierarchy);
        dock_children.push(inspector);
        if management {
            let actions = canvas(
                PROPERTIES_ACTIONS,
                Rect::new(16., 56., dock_width - 32., 52.),
                vec![],
            );
            dock_children.push(actions);
        }
        dock_children.push(hierarchy);
    } else {
        hide(&mut inspector);
        dock_children.push(hierarchy);
        dock_children.push(inspector);
    }
    let dock = canvas(
        BASE + 132,
        Rect::new(dock_x, body_top, dock_width, body_height),
        dock_children,
    );
    let mut children = vec![preview, dock];
    children.splice(0..0, chrome.drain(..1));
    children.extend(chrome);
    if let Some(modal) = &editor.modal {
        children.push(modal_overlay(
            modal,
            width,
            available_height,
            scene.root().frame.height,
            p,
        )?);
    }
    Scene::new(canvas(scene.root().id, scene.root().frame, children))
}

#[cfg(test)]
mod tests {
    use super::*;
    use otrios_interfaces::oib::{
        InstanceSurface,
        designer::{ClassId, Document, DocumentId, InstanceId, Mappings},
    };

    fn fixture(width: f32, height: f32) -> (Scene, Seed) {
        let action = |id, event| {
            let mut node = background(id, 80., 48., [123, 45, 67, 255], 4.);
            node.responder = Some(TouchResponder::Activate(Action::AppEvent(event)));
            node.semantics = Some(SwitchSemantics {
                label: format!("Authored {id}"),
                checked: false,
            });
            node
        };
        let scene = Scene::new(canvas(
            1,
            Rect::new(0., 0., width, height),
            vec![
                canvas(2, Rect::new(0., 0., 320., 180.), vec![action(6, 9)]),
                canvas(3, Rect::new(0., 0., 320., 180.), vec![action(7, 10)]),
                canvas(4, Rect::new(0., 0., 320., 180.), vec![action(8, 11)]),
                action(5, 7),
            ],
        ))
        .unwrap();
        let seed = Seed {
            format: "test".into(),
            graph: None,
            resolved_libraries: vec![],
            expanded: false,
            designer: Some(Document {
                id: DocumentId(1),
                name: "Test".into(),
                class: ClassId(1),
                presented_root: InstanceId(1),
                hierarchy_viewport: Some(3),
                classes: vec![],
                definitions: vec![],
                instances: vec![],
                mappings: Mappings {
                    objects: vec![],
                    definitions: vec![],
                    roots: vec![],
                },
                placements: vec![],
                connections: vec![],
            }),
            main_oib: 0,
            main_class: "Test".into(),
            components: vec![],
            instances: vec![],
            cells: vec![],
            inspector_viewport: 4,
            inspector_content: [320., 180.],
            instance_surface: Some(InstanceSurface {
                layout: None,
                preview_viewport: 2,
                preview_stride: 0.,
                inspector_start: 0.,
                inspector_stride: 0.,
                prototypes: vec![],
                node_base: 100,
                node_stride: 10,
            }),
        };
        (scene, seed)
    }
    fn count(node: &SceneNode) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }

    #[test]
    fn task_dock_preserves_preview_and_hides_inactive_hits() {
        for width in [800., 1024., 1280., 1920.] {
            let (original, seed) = fixture(width, 800.);
            for task in [Task::Library, Task::Layers, Task::Inspect] {
                let scene = project(
                    &original,
                    &seed,
                    Some(7),
                    false,
                    task,
                    DesignerMode::Edit,
                    Appearance::Dark,
                    &EditorChrome::default(),
                )
                .unwrap();
                assert_eq!(
                    find(scene.root(), 2).unwrap().children,
                    find(original.root(), 2).unwrap().children
                );
                let (active, hidden) = if task == Task::Inspect {
                    (4, 3)
                } else {
                    (3, 4)
                };
                assert!(find(scene.root(), active).unwrap().frame.width >= 320.);
                assert!(find(scene.root(), hidden).unwrap().children.is_empty());
                assert!(
                    scene
                        .controls()
                        .iter()
                        .any(|control| control.node.id == if active == 3 { 7 } else { 8 })
                );
                assert!(find(scene.root(), hidden).unwrap().semantics.is_none());
                assert!(
                    !scene
                        .controls()
                        .iter()
                        .any(|control| control.node.id == if hidden == 3 { 7 } else { 8 })
                );
                assert_eq!(
                    find(scene.root(), 5).unwrap().responder,
                    find(original.root(), 5).unwrap().responder
                );
                assert_eq!(
                    find(scene.root(), 5).unwrap().semantics,
                    find(original.root(), 5).unwrap().semantics
                );
                assert!(count(scene.root()) < 110);
                assert!(scene.controls().len() <= 10);
                for id in [
                    TAB_LIBRARY,
                    TAB_LAYERS,
                    TAB_INSPECT,
                    MODE_EDIT,
                    MODE_TEST,
                    APPEARANCE_DARK,
                    APPEARANCE_LIGHT,
                ] {
                    let path = scene.path(id).unwrap();
                    let node = path.last().unwrap().node;
                    assert_eq!(node.frame.height, 48.);
                    assert!(node.frame.width >= 48.);
                }
            }
        }
    }
    #[test]
    fn keyboard_reserves_space_and_chrome_has_real_selected_semantics() {
        let (original, seed) = fixture(1280., 800.);
        let normal = project(
            &original,
            &seed,
            None,
            false,
            Task::Layers,
            DesignerMode::Test,
            Appearance::Light,
            &EditorChrome {
                management: true,
                ..Default::default()
            },
        )
        .unwrap();
        let keyboard = project(
            &original,
            &seed,
            None,
            true,
            Task::Layers,
            DesignerMode::Test,
            Appearance::Light,
            &EditorChrome {
                management: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            find(normal.root(), 2).unwrap().frame.height
                - find(keyboard.root(), 2).unwrap().frame.height,
            240.
        );
        for (id, selected) in [
            (TAB_LAYERS, true),
            (TAB_INSPECT, false),
            (MODE_TEST, true),
            (MODE_EDIT, false),
            (APPEARANCE_LIGHT, true),
            (APPEARANCE_DARK, false),
        ] {
            assert_eq!(
                find(normal.root(), id)
                    .unwrap()
                    .semantics
                    .as_ref()
                    .unwrap()
                    .checked,
                selected
            );
        }
        let path = normal.path(MODE_EDIT).unwrap();
        let origin = path.last().unwrap().origin;
        let point = [origin[0] + 24., origin[1] + 24.];
        assert!(
            normal
                .hit_path(point)
                .iter()
                .any(|node| node.node.id == MODE_EDIT)
        );
    }
    fn commands() -> EditorChrome {
        EditorChrome {
            new_component: true,
            undo: true,
            redo: true,
            discard: true,
            apply: true,
            ..Default::default()
        }
    }
    #[test]
    fn commands_wrap_without_covering_tasks_or_preview_and_disabled_are_inert() {
        for width in [640., 800., 1280., 1920.] {
            let (original, seed) = fixture(width, 800.);
            for keyboard in [false, true] {
                let editor = commands();
                let scene = project(
                    &original,
                    &seed,
                    None,
                    keyboard,
                    Task::Inspect,
                    DesignerMode::Edit,
                    Appearance::Dark,
                    &editor,
                )
                .unwrap();
                let preview = scene.path(2).unwrap();
                let preview = preview.last().unwrap();
                for id in [NEW_COMPONENT, UNDO, REDO, DISCARD, APPLY] {
                    let path = scene.path(id).unwrap();
                    let n = path.last().unwrap();
                    assert_eq!(n.node.frame.height, 48.);
                    assert!(n.node.frame.width >= 48.);
                    assert!(n.origin[0] + n.node.frame.width <= width - 16.);
                    assert!(n.origin[1] + 48. < preview.origin[1]);
                    assert!(
                        scene
                            .hit_path([n.origin[0] + 24., n.origin[1] + 24.])
                            .iter()
                            .any(|h| h.node.id == id)
                    );
                }
                assert!(count(scene.root()) < 110);
                assert!(scene.controls().len() <= 15);
                assert_eq!(
                    find(scene.root(), 2).unwrap().children,
                    find(original.root(), 2).unwrap().children
                );
                let disabled = project(
                    &original,
                    &seed,
                    None,
                    keyboard,
                    Task::Inspect,
                    DesignerMode::Edit,
                    Appearance::Dark,
                    &EditorChrome::default(),
                )
                .unwrap();
                for id in [NEW_COMPONENT, UNDO, REDO, DISCARD, APPLY] {
                    assert!(!disabled.controls().iter().any(|n| n.node.id == id));
                    let path = disabled.path(id).unwrap();
                    let n = path.last().unwrap();
                    let hits = disabled.hit_path([n.origin[0] + 24., n.origin[1] + 24.]);
                    assert!(
                        hits.iter()
                            .any(|n| n.node.responder == Some(TouchResponder::Consume))
                    );
                    assert!(!hits.iter().any(|n| n.node.id == 2));
                }
            }
        }
    }
    #[test]
    fn modal_blocks_underlying_hits_and_keeps_commands_reachable() {
        for (width, height, keyboard) in [
            (800., 800., false),
            (1280., 800., true),
            (1920., 1080., false),
        ] {
            let (original, seed) = fixture(width, height);
            let editor = EditorChrome {
                modal: Some(Modal {
                    scroll: None,
                    links: vec![],
                    title: "Discard changes?".into(),
                    lines: vec!["Your component has unapplied changes.".into()],
                    actions: vec![
                        ModalAction {
                            id: MODAL_ACTION_START,
                            label: "Cancel".into(),
                            enabled: true,
                        },
                        ModalAction {
                            id: MODAL_ACTION_START + 1,
                            label: "Discard".into(),
                            enabled: false,
                        },
                    ],
                }),
                ..commands()
            };
            let scene = project(
                &original,
                &seed,
                None,
                keyboard,
                Task::Inspect,
                DesignerMode::Edit,
                Appearance::Dark,
                &editor,
            )
            .unwrap();
            for point in [[1., 1.], [width - 1., height - 1.], [24., 200.]] {
                let hit = scene.hit_path(point);
                assert!(hit.iter().any(|n| n.node.id == BASE + 500));
                assert!(
                    !hit.iter()
                        .any(|n| [2, 3, 4, NEW_COMPONENT].contains(&n.node.id))
                );
                assert!(
                    hit.iter()
                        .any(|n| n.node.responder == Some(TouchResponder::Consume))
                );
            }
            let path = scene.path(MODAL_ACTION_START).unwrap();
            let n = path.last().unwrap();
            assert_eq!(n.node.frame.height, 48.);
            assert!(
                scene
                    .hit_path([n.origin[0] + 24., n.origin[1] + 24.])
                    .iter()
                    .any(|n| n.node.id == MODAL_ACTION_START)
            );
            assert!(
                !scene
                    .controls()
                    .iter()
                    .any(|n| n.node.id == MODAL_ACTION_START + 1)
            );
            assert!(count(scene.root()) < 140);
            assert!(scene.controls().len() < 20);
        }
        assert!(
            wrapped_lines(&["x".repeat(100)], 32)
                .iter()
                .all(|l| l.chars().count() <= 32)
        );
    }
    #[test]
    fn dialogs_reject_duplicate_ids_and_content_that_cannot_fit() {
        let p = palette(Appearance::Dark);
        let action = ModalAction {
            id: MODAL_ACTION_START,
            label: "Cancel".into(),
            enabled: true,
        };
        let mut modal = Modal {
            scroll: None,
            links: vec![],
            title: "Review changes".into(),
            lines: vec![],
            actions: vec![action.clone(), action.clone()],
        };
        assert!(modal_overlay(&modal, 1280., 800., 800., p).is_err());
        modal.actions = vec![ModalAction {
            id: APPLY,
            ..action
        }];
        assert!(modal_overlay(&modal, 1280., 800., 800., p).is_err());
        modal.actions.clear();
        modal.lines = vec!["A long explanation that needs more room.".repeat(100)];
        assert!(modal_overlay(&modal, 800., 560., 800., p).is_err());
    }
    #[test]
    fn palette_maps_only_explicit_legacy_roles() {
        let dark = palette(Appearance::Dark);
        let light = palette(Appearance::Light);
        assert_ne!(dark.background, light.background);
        assert_eq!(light.role(theme::TEXT), light.text);
        assert_eq!(dark.role(theme::BRASS), dark.focus);
        assert_eq!(light.role([123, 45, 67, 255]), [123, 45, 67, 255]);
    }
}
