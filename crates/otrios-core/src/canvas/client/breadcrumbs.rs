// SPDX-License-Identifier: LGPL-3.0-only
//! Local navigation follows admitted catalog placement, independently of app state.
use super::*;
use crate::dashboard::components::{canvas, text};
use otrios_interfaces::{
    scene::{Primitive, Rect, SwitchSemantics},
    spaces::Discovery,
};
pub(super) const BASE: u32 = 4_100_100_000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Navigate(String),
    Account,
    Close,
    Preference(shell_style::Preference),
    Profile,
    SignIn,
    SignOut,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Item {
    pub label: String,
    pub path: String,
    action: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Trail {
    pub items: Vec<Item>,
    pub host: bool,
    pub preference: shell_style::Preference,
    pub account_open: bool,
    pub name: String,
    pub identity: Option<String>,
    pub sign_in: bool,
    commands: BTreeMap<u8, Command>,
}
impl Trail {
    pub fn new(
        catalog: &Catalog,
        app: &str,
        snapshot: Option<&Discovery>,
        space: Option<&str>,
        ui: &Ui,
        host: bool,
    ) -> Result<Self> {
        let entry = catalog
            .apps
            .iter()
            .find(|e| e.app.id == app)
            .ok_or("Missing navigation entry")?;
        let mut items = vec![Item {
            label: "OTriOS".into(),
            path: String::new(),
            action: None,
        }];
        if !entry.path.is_empty() {
            let admitted =
                snapshot.and_then(|s| s.spaces.iter().find(|s| Some(s.id.as_str()) == space));
            let mut parents = catalog
                .apps
                .iter()
                .filter(|e| {
                    !e.path.is_empty()
                        && entry
                            .path
                            .strip_prefix(&e.path)
                            .is_some_and(|tail| tail.starts_with('/'))
                })
                .filter(|e| admitted.is_none_or(|s| s.apps.iter().any(|a| a.app == e.app.id)))
                .collect::<Vec<_>>();
            parents.sort_by_key(|e| e.path.len());
            items.extend(parents.into_iter().map(|e| Item {
                label: e.app.title.clone(),
                path: e.path.clone(),
                action: None,
            }));
            items.push(Item {
                label: entry.app.title.clone(),
                path: entry.path.clone(),
                action: None,
            });
        }
        let mut actions = (0..=u8::MAX)
            .rev()
            .filter(|action| !ui.navigation.iter().any(|n| n.action == *action));
        let ancestors = items.len().saturating_sub(1);
        for item in items.iter_mut().take(ancestors) {
            item.action = Some(actions.next().ok_or("Navigation control budget exceeded")?);
        }
        let mut commands = items
            .iter()
            .filter_map(|i| {
                i.action
                    .map(|action| (action, Command::Navigate(i.path.clone())))
            })
            .collect::<BTreeMap<_, _>>();
        for command in [
            Command::Account,
            Command::Close,
            Command::Profile,
            Command::SignIn,
            Command::SignOut,
            Command::Preference(shell_style::Preference::System),
            Command::Preference(shell_style::Preference::Light),
            Command::Preference(shell_style::Preference::Dark),
        ] {
            commands.insert(
                actions.next().ok_or("Shell control budget exceeded")?,
                command,
            );
        }
        let identity = snapshot.and_then(|s| s.identity.clone());
        let name = snapshot
            .and_then(|s| s.profile.as_ref().map(|p| p.name.clone()))
            .or_else(|| identity.clone())
            .unwrap_or_else(|| "Visitor".into());
        Ok(Self {
            items,
            host,
            preference: shell_style::Preference::System,
            account_open: false,
            name,
            identity,
            sign_in: true,
            commands,
        })
    }
    pub fn command(&self, action: u8) -> Option<Command> {
        self.commands.get(&action).cloned()
    }
    fn action(&self, command: &Command) -> u8 {
        *self
            .commands
            .iter()
            .find(|(_, c)| *c == command)
            .expect("registered shell command")
            .0
    }
}
fn find_mut(root: &mut SceneNode, id: u32) -> Option<&mut SceneNode> {
    if root.id == id {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| find_mut(child, id))
}
fn replace(root: &mut SceneNode, id: u32, node: &SceneNode) -> bool {
    if root.id == id {
        *root = node.clone();
        return true;
    }
    root.children
        .iter_mut()
        .any(|child| replace(child, id, node))
}
/// Narrow phone headers use a second location row instead of shrinking targets.
pub(in crate::canvas) fn compact_extra(width: f32) -> f32 {
    if width < 600. { 36. } else { 0. }
}
pub(super) fn present(root: &mut SceneNode, mount: &Mount) {
    let Some(trail) = &mount.breadcrumbs else {
        return;
    };
    fn remove_account_actions(node: &mut SceneNode, ui: &Ui) {
        node.children.retain(|child| !matches!(child.responder,Some(TouchResponder::Activate(Action::AppEvent(control))) if ui.handlers.iter().any(|h|h.control==control&&h.operation==Operation::SignOut)));
        for child in &mut node.children {
            remove_account_actions(child, ui);
        }
    }
    remove_account_actions(root, &mount.ui);
    let designer = mount.designer.as_ref().filter(|d| d.focus_layout());
    let palette = mount.shell_appearance.palette();
    let (foreground, divider) = (palette.text, palette.divider);
    let compact = compact_extra(mount.viewport[0]) > 0.;
    let scale = if compact {
        2
    } else {
        otrios_interfaces::theme::HEADER_FONT_SCALE
    };
    let height = mount
        .font
        .as_ref()
        .map_or(40., |f| f.metrics().line_height(scale as f32));
    let row_height = if compact { 48. } else { 60. };
    let y = (row_height - height) / 2.;
    let width = if mount.chat.is_some() {
        find_mut(root, 7_000_000)
            .map(|header| match header.primitive {
                Primitive::Viewport { scale } => header.frame.width / scale,
                _ => header.frame.width,
            })
            .unwrap_or(root.frame.width)
    } else {
        root.frame.width
    } - 48.;
    let measure = |value: &str| {
        mount
            .font
            .as_ref()
            .map_or(value.chars().count() as f32 * 20., |f| {
                f.text_width(value, scale as f32)
            })
    };
    let mut children = vec![];
    let mut x = 0.;
    let mut row = 0.;
    for (index, item) in trail.items.iter().enumerate() {
        if compact && index == 1 {
            x = 0.;
            row = row_height;
        }
        if index > 0 {
            children.push(text(
                BASE + index as u32 * 4,
                Rect::new(x, y + row, 24., height),
                ">",
                scale,
                foreground,
            ));
            x += measure(">") + 24.;
        }
        let item_width = measure(&item.label) + 8.;
        let mut node = canvas(
            BASE + index as u32 * 4 + 1,
            Rect::new(x, row, item_width, row_height),
            vec![text(
                BASE + index as u32 * 4 + 2,
                Rect::new(0., y, item_width, height),
                &item.label,
                scale,
                foreground,
            )],
        );
        if let Some(action) = item.action {
            node.responder = Some(TouchResponder::Activate(Action::AppNavigate(action)));
            node.semantics = Some(SwitchSemantics {
                label: if item.path.is_empty() {
                    "Back to OTriOS".into()
                } else {
                    item.label.clone()
                },
                checked: false,
            });
        }
        children.push(node);
        x += item_width + 12.;
    }
    let mut line = crate::dashboard::components::background(BASE + 100, width, 1., divider, 0.);
    let header_height = 72.
        + if compact && trail.items.len() > 1 {
            36.
        } else {
            0.
        };
    line.frame = Rect::new(0., header_height - 1., width, 1.);
    line.responder = Some(TouchResponder::PassThrough);
    children.push(line);
    let caption = trail
        .identity
        .as_ref()
        .map(|_| {
            trail
                .name
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string()
        })
        .unwrap_or_else(|| "Sign in".into());
    let account_width = if trail.identity.is_some() { 60. } else { 108. };
    let mut account_children = vec![];
    if trail.identity.is_some() {
        let mut circle = crate::dashboard::components::node(
            BASE + 202,
            Rect::new(4., 4., 52., 52.),
            Primitive::Circle {
                color: if mount.shell_appearance == oib_workspace::Appearance::Dark {
                    [61, 52, 37, 255]
                } else {
                    [238, 229, 210, 255]
                },
            },
        );
        circle.responder = Some(TouchResponder::PassThrough);
        account_children.push(circle);
    }
    let caption_scale = if trail.identity.is_some() { scale } else { 2 };
    let caption_height = mount
        .font
        .as_ref()
        .map_or(32., |f| f.metrics().line_height(caption_scale as f32));
    let caption_width = mount
        .font
        .as_ref()
        .map_or(60., |f| f.text_width(&caption, caption_scale as f32));
    account_children.push(text(
        BASE + 201,
        Rect::new(
            (account_width - caption_width) / 2.,
            (60. - caption_height) / 2.,
            caption_width + 4.,
            caption_height,
        ),
        &caption,
        caption_scale,
        foreground,
    ));
    let mut account = canvas(
        BASE + 200,
        Rect::new(width - account_width, -2., account_width, 60.),
        account_children,
    );
    account.responder = Some(TouchResponder::Activate(Action::AppNavigate(
        trail.action(&Command::Account),
    )));
    account.semantics = Some(SwitchSemantics {
        label: if trail.identity.is_some() {
            "Profile"
        } else {
            "Sign in"
        }
        .into(),
        checked: trail.account_open,
    });
    children.push(account);
    let header = canvas(
        BASE + 101,
        Rect::new(24., 24., width.max(1.), header_height),
        children,
    );
    if mount.chat.is_some()
        && let Some(chat_header) = find_mut(root, 7_000_000)
    {
        chat_header.children.retain(|child| {
            child
                .semantics
                .as_ref()
                .is_some_and(|s| matches!(s.label.as_str(), "Participants" | "Chat identity"))
        });
        chat_header.children.insert(0, header);
        return;
    }
    if !trail.host
        && designer.is_none()
        && let Some(original) = mount
            .layout
            .blueprint()
            .templates
            .first()
            .and_then(|t| t.root.children.get(1))
        && replace(root, u32::from(original.id), &header)
    {
        return;
    }
    // Before content/overlays so keyboards and modal barriers retain priority.
    if trail.host {
        root.children.retain(|n| {
            !(n.frame.y < 104.
                && n.semantics
                    .as_ref()
                    .is_some_and(|s| matches!(s.label.as_str(), "Profile" | "Sign in")))
        });
    }
    let index = if trail.host {
        root.children
            .iter()
            .position(|n| {
                n.frame.y == 0.
                    && n.frame.height == super::super::host_ui::map_top(mount.viewport[0])
            })
            .map(|index| index + 1)
    } else {
        None
    }
    .unwrap_or(root.children.len().min(1));
    root.children.insert(index, header);
}

fn menu_button(
    trail: &Trail,
    id: u32,
    frame: Rect,
    label: &str,
    command: Command,
    selected: bool,
    p: oib_workspace::Palette,
) -> SceneNode {
    let mut background = crate::dashboard::components::background(
        id + 1,
        frame.width,
        frame.height,
        if selected { p.selected } else { p.surface },
        12.,
    );
    background.responder = Some(TouchResponder::PassThrough);
    let mut node = canvas(
        id,
        frame,
        vec![
            background,
            text(
                id + 2,
                Rect::new(12., 12., frame.width - 24., 32.),
                label,
                2,
                if selected { p.background } else { p.text },
            ),
        ],
    );
    node.responder = Some(TouchResponder::Activate(Action::AppNavigate(
        trail.action(&command),
    )));
    node.semantics = Some(SwitchSemantics {
        label: label.into(),
        checked: selected,
    });
    node
}
pub(super) fn present_account(root: &mut SceneNode, mount: &Mount) {
    let Some(trail) = mount.breadcrumbs.as_ref().filter(|t| t.account_open) else {
        return;
    };
    // Paint the current view as an inert backdrop while the account menu owns input.
    for child in &mut root.children {
        child.responder = Some(TouchResponder::PassThrough);
        child.semantics = None;
    }
    let p = mount.shell_appearance.palette();
    let width = 360_f32.min(root.frame.width - 32.).max(240.);
    let x = (root.frame.width - width - 24.).max(8.);
    let top = (root.frame.height - 300.).clamp(8., 100.);
    let mut dismiss = canvas(
        BASE + 300,
        Rect::new(0., 0., root.frame.width, root.frame.height),
        vec![],
    );
    dismiss.responder = Some(TouchResponder::Activate(Action::AppNavigate(
        trail.action(&Command::Close),
    )));
    dismiss.semantics = Some(SwitchSemantics {
        label: "Close account menu".into(),
        checked: false,
    });
    let mut background =
        crate::dashboard::components::background(BASE + 301, width, 340., p.surface, 12.);
    background.frame = Rect::new(x, top, width, 340.);
    background.responder = Some(TouchResponder::Consume);
    let mut children = vec![
        text(
            BASE + 302,
            Rect::new(16., 16., width - 32., 32.),
            &trail.name,
            2,
            p.text,
        ),
        text(
            BASE + 303,
            Rect::new(16., 62., width - 32., 28.),
            "Appearance",
            2,
            p.muted,
        ),
    ];
    let segment = (width - 32.) / 3.;
    for (index, (label, preference)) in [
        ("System", shell_style::Preference::System),
        ("Light", shell_style::Preference::Light),
        ("Dark", shell_style::Preference::Dark),
    ]
    .into_iter()
    .enumerate()
    {
        children.push(menu_button(
            trail,
            BASE + 310 + index as u32 * 4,
            Rect::new(16. + index as f32 * segment, 100., segment - 4., 52.),
            label,
            Command::Preference(preference),
            trail.preference == preference,
            p,
        ));
    }
    if trail.identity.is_some() {
        children.push(menu_button(
            trail,
            BASE + 330,
            Rect::new(16., 176., width - 32., 52.),
            "Account settings",
            Command::Profile,
            false,
            p,
        ));
    }
    if mount.session_controls && trail.identity.is_some() {
        children.push(menu_button(
            trail,
            BASE + 334,
            Rect::new(16., 240., width - 32., 52.),
            "Sign out",
            Command::SignOut,
            false,
            p,
        ));
    } else if trail.identity.is_none() && trail.sign_in {
        children.push(menu_button(
            trail,
            BASE + 338,
            Rect::new(16., 176., width - 32., 52.),
            "Sign in",
            Command::SignIn,
            false,
            p,
        ));
    }
    let mut fill =
        crate::dashboard::components::background(BASE + 304, width - 2., 338., p.surface, 11.);
    fill.frame = Rect::new(1., 1., width - 2., 338.);
    fill.responder = Some(TouchResponder::PassThrough);
    children.insert(0, fill);
    background.frame = Rect::new(0., 0., width, 340.);
    background.responder = Some(TouchResponder::PassThrough);
    children.insert(0, background);
    let mut panel = canvas(BASE + 305, Rect::new(x, top, width, 340.), children);
    panel.responder = Some(TouchResponder::Consume);
    dismiss.children.push(panel);
    fn count(node: &SceneNode) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }
    if count(root) + count(&dismiss) > crate::dashboard::scene::MAX_NODES {
        // A full admitted app can spend its entire scene budget. Keep account
        // controls available without retaining a second interactive execution.
        root.children.clear();
        root.children.push(crate::dashboard::components::background(
            BASE + 306,
            root.frame.width,
            root.frame.height,
            p.background,
            0.,
        ));
    }
    root.children.push(dismiss);
}
impl Client {
    pub(super) fn shell_command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Navigate(path) => {
                self.navigate(&path)?;
            }
            Command::Account | Command::Close => {
                if self.active.as_ref().is_some_and(|m| {
                    m.local_modal_open() && !m.breadcrumbs.as_ref().is_some_and(|t| t.account_open)
                }) {
                    return Ok(false);
                }
                let m = self.active.as_mut().ok_or("No Canvas")?;
                if let Some(trail) = &mut m.breadcrumbs {
                    trail.account_open = matches!(command, Command::Account) && !trail.account_open;
                }
                m.invalidate();
            }
            Command::Preference(preference) => {
                self.set_appearance(
                    preference.name(),
                    self.system_appearance == oib_workspace::Appearance::Dark,
                )?;
            }
            Command::Profile => {
                if let Some(m) = &mut self.active {
                    if let Some(trail) = &mut m.breadcrumbs {
                        trail.account_open = false;
                    }
                    m.invalidate();
                }
                self.open_profile()?;
            }
            Command::SignIn => {
                if let Some(trail) = self.active.as_mut().and_then(|m| m.breadcrumbs.as_mut()) {
                    trail.account_open = false;
                }
                self.sign_in_requested = true;
                self.auth_required = true;
                if let Some(m) = &mut self.active {
                    m.invalidate();
                }
            }
            Command::SignOut => {
                if let Some(m) = &mut self.active {
                    if let Some(trail) = &mut m.breadcrumbs {
                        trail.account_open = false;
                    }
                    m.invalidate();
                }
                if !self.request_leave(oib_leave::Target::SignOut)? {
                    self.sign_out_requested = true;
                }
            }
        }
        Ok(true)
    }
    pub(super) fn open_profile(&mut self) -> Result<bool> {
        if self.request_leave(oib_leave::Target::Profile)? {
            return Ok(true);
        }
        self.host.requested_profile = true;
        self.show_spaces()
    }
}
