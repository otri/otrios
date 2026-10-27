// SPDX-License-Identifier: LGPL-3.0-only
use super::Result;
use super::client::{Client, Focus, Panel, VIEWPORT};
use super::icons::*;
use crate::dashboard::{
    components::{canvas, node, text},
    scene::Scene,
};
use otrios_interfaces::scene::*;
use unicode_segmentation::UnicodeSegmentation;
struct Paint {
    nodes: Vec<SceneNode>,
    next: u32,
}
impl Paint {
    fn id(&mut self) -> u32 {
        self.next += 32;
        self.next
    }
    fn rect(&mut self, r: [f32; 4], color: Color, radius: f32) {
        let id = self.id();
        self.nodes.push(node(
            id,
            Rect::new(r[0], r[1], r[2], r[3]),
            Primitive::Superellipse {
                radius: radius.min(r[2].min(r[3]) / 2.),
                exponent: 4.,
                color,
            },
        ));
    }
    fn text(&mut self, x: f32, y: f32, w: f32, value: &str, scale: u32, color: Color) {
        let id = self.id();
        let mut s = String::new();
        for g in value.graphemes(true) {
            if s.len() + g.len() > 256 {
                break;
            }
            s.push_str(g);
        }
        self.nodes.push(text(
            id,
            Rect::new(x, y, w.max(1.), (8. * scale as f32 + 12.).max(20.)),
            &s,
            scale,
            color,
        ));
    }
    fn button(&mut self, command: u16, r: [f32; 4], label: &str, caption: &str, primary: bool) {
        let id = self.id();
        let color = if primary { BRASS } else { CARD };
        let mut children = vec![node(
            id + 1,
            Rect::new(0., 0., r[2], r[3]),
            Primitive::Superellipse {
                radius: 8.,
                exponent: 4.,
                color,
            },
        )];
        if !caption.is_empty() {
            children.push(text(
                id + 2,
                Rect::new(14., (r[3] - 20.) / 2., (r[2] - 20.).max(1.), 28.),
                caption,
                2,
                if primary { BG } else { TEXT },
            ));
        }
        let mut button = canvas(id, Rect::new(r[0], r[1], r[2], r[3]), children);
        button.responder = Some(TouchResponder::Activate(Action::AppEvent(
            (command + 40) as u8,
        )));
        button.semantics = Some(SwitchSemantics {
            label: label.chars().take(90).collect(),
            checked: false,
        });
        self.nodes.push(button);
    }
    fn avatar_button(
        &mut self,
        command: u16,
        r: [f32; 4],
        avatar_id: u8,
        label: &str,
        selected: bool,
    ) {
        self.button(command, r, label, "", selected);
        let id = self.id();
        let mut icon = avatar(avatar_id, id, r[2].min(r[3]) - 12.);
        icon.frame.x = 6.;
        icon.frame.y = 6.;
        self.nodes.last_mut().unwrap().children.push(icon);
    }
    fn member(&mut self, command: u16, x: f32, y: f32, width: f32, member: &super::Member) {
        self.button(
            command,
            [x, y, width, 52.],
            &format!(
                "{}{}",
                if member.admin { "Admin " } else { "" },
                member.profile.name
            ),
            "",
            false,
        );
        let id = self.id();
        let mut icon = avatar(member.profile.avatar, id, 36.);
        icon.frame.x = 8.;
        icon.frame.y = 8.;
        let label = text(
            id + 20,
            Rect::new(54., 16., (width - 62.).max(1.), 28.),
            &format!(
                "{}{}",
                if member.admin { "@ " } else { "" },
                member.profile.name
            ),
            2,
            if member.admin { BRASS } else { TEXT },
        );
        let last = self.nodes.last_mut().unwrap();
        last.children.extend([icon, label]);
    }
}
pub fn room_height(size: [f32; 2], keyboard: bool) -> f32 {
    size[1] - if keyboard { 320. * size[0] / 1280. } else { 0. }
}
impl Client {
    pub(crate) fn effect_vertices(&self) -> Result<Vec<f32>> {
        if self.panel != Panel::None || self.kicked.is_some() || self.particles.is_empty() {
            return Ok(vec![]);
        }
        let [w, h] = self.viewport;
        let mut p = Paint {
            nodes: vec![],
            next: 9_000_000,
        };
        for particle in &self.particles {
            let t = self.now.saturating_sub(particle.at) as f32 / 1000.;
            let mut color = if particle.steam { MUTED } else { COPPER };
            color[3] = ((1. - t / 0.9).max(0.) * 200.) as u8;
            let s = if particle.steam { 10. + t * 12. } else { 4. };
            let x = particle.origin[0] + particle.velocity[0] * t;
            let y = particle.origin[1] + particle.velocity[1] * t;
            p.rect(
                [x.clamp(24., w - 24. - s), y.clamp(100., h - 24. - s), s, s],
                color,
                // Sparks are tiny quads. Rounded rectangles would tessellate
                // 52 triangles and evaluate trigonometry for every particle.
                0.,
            );
        }
        let scene = Scene::new(canvas(8_000_000, Rect::new(0., 0., w, h), p.nodes))?;
        Ok(crate::dashboard::render_scene(&scene).vertices)
    }
    pub(crate) fn scene(&self, base: &Scene, size: [f32; 2], keyboard: bool) -> Result<Scene> {
        let [w, full_h] = size;
        let h = room_height(size, keyboard);
        let side = if w >= 1100. { 288. } else { 0. };
        let page_w = w - 48. - side;
        let mut p = Paint {
            nodes: vec![],
            next: 2_000_000,
        };
        p.rect([0., 0., w, full_h], BG, 0.);
        p.button(
            0,
            [24., 24., 140., 60.],
            "Back to OTriOS",
            "< OTriOS",
            false,
        );
        p.text(184., 38., (w - 430.).max(200.), "The Brass Relay", 4, TEXT);
        p.button(
            3,
            [w - 224., 24., 96., 60.],
            "Members",
            &format!("{} users", self.members.len()),
            false,
        );
        p.button(4, [w - 116., 24., 92., 60.], "Edit profile", "You", false);
        let page_y = 108.;
        let draft_h = if self.drafts.is_empty() { 32. } else { 72. };
        let content_h = (h - page_y - 92. - draft_h).max(40.);
        p.rect([24., page_y, page_w, content_h + draft_h + 68.], CARD, 10.);
        let mut viewport = canvas(
            u32::from(VIEWPORT),
            Rect::new(88., page_y + 16., (page_w - 80.).max(1.), content_h - 24.),
            vec![],
        );
        viewport.responder = None;
        p.nodes.push(viewport);
        if self.messages.is_empty() {
            p.text(
                52.,
                page_y + 46.,
                page_w - 64.,
                if self.online {
                    "The wire is open. Send the first dispatch."
                } else {
                    "Connecting to The Brass Relay..."
                },
                2,
                MUTED,
            );
        }
        if self.older {
            p.button(
                13,
                [w.min(page_w) - 220., page_y + 8., 190., 40.],
                "Load older messages",
                if self.history_wait {
                    "Loading..."
                } else {
                    "Older dispatches"
                },
                false,
            );
        }
        let dy = page_y + content_h;
        if self.new_messages > 0 {
            p.button(
                12,
                [48., dy - 48., 260., 44.],
                "Show new messages",
                &format!("{} new dispatches", self.new_messages),
                true,
            );
        }
        let mut active = self
            .drafts
            .iter()
            .filter(|(_, d)| !d.text.is_empty())
            .take(1);
        if let Some((id, draft)) = active.next() {
            let who = self
                .members
                .iter()
                .find(|m| m.profile.id == *id)
                .map_or("Someone", |m| m.profile.name.as_str());
            p.rect([40., dy, page_w - 32., 58.], BG, 3.);
            p.text(
                50.,
                dy + 5.,
                page_w - 52.,
                &format!(
                    "{who} - live draft{}",
                    if self.drafts.len() > 1 {
                        format!(" (+{} typing)", self.drafts.len() - 1)
                    } else {
                        String::new()
                    }
                ),
                1,
                COPPER,
            );
            p.text(
                50.,
                dy + 27.,
                page_w - 52.,
                &draft.text.graphemes(true).take(96).collect::<String>(),
                2,
                MUTED,
            );
        }
        let cy = h - 88.;
        p.button(19, [36., cy, 40., 60.], "Commands", "/", false);
        p.button(
            1,
            [84., cy, page_w - 260., 60.],
            "Message composer",
            "",
            false,
        );
        let (display, shift) = if self.compose.text().is_empty() {
            (
                if self.focus == Some(Focus::Compose) {
                    "|".into()
                } else {
                    "Message...".into()
                },
                0.,
            )
        } else if let Some(font) = &self.font {
            let (value, shift) = self.compose.window(
                self.input_view.offset,
                page_w - 284.,
                font,
                2.,
                self.focus == Some(Focus::Compose),
            );
            (
                value
                    .graphemes(true)
                    .scan(0, |n, g| {
                        *n += g.len();
                        (*n <= 256).then_some(g)
                    })
                    .collect(),
                shift,
            )
        } else {
            (self.compose.display(self.focus == Some(Focus::Compose)), 0.)
        };
        let id = p.id();
        p.nodes.last_mut().unwrap().children.push(text(
            id,
            Rect::new(12. + shift, 20., page_w - 284., 28.),
            &display,
            2,
            if self.compose.text().is_empty() {
                MUTED
            } else {
                TEXT
            },
        ));
        p.button(
            5,
            [page_w - 164., cy, 82., 60.],
            if self.private {
                "Private draft: enable live drafts"
            } else {
                "Live drafts: compose privately"
            },
            if self.private { "Private" } else { "Live" },
            false,
        );
        p.button(
            2,
            [page_w - 74., cy, 94., 60.],
            "Send message",
            "Send",
            true,
        );
        if !self.notice.is_empty() {
            p.rect([42., cy - 58., page_w - 36., 48.], BG, 6.);
            p.text(54., cy - 43., page_w - 60., &self.notice, 2, BRASS);
        }
        if !self.online && self.kicked.is_none() {
            p.button(
                11,
                [48., page_y + 100., 200., 52.],
                "Reconnect room",
                "Reconnect",
                false,
            );
        }
        if side > 0. {
            let x = w - 288.;
            p.text(x + 12., 118., 252., "OPERATORS ON THE WIRE", 1, BRASS);
            for (i, member) in self.members.iter().take(8).enumerate() {
                p.member(100 + i as u16, x, 156. + i as f32 * 58., 264., member);
            }
            p.button(
                3,
                [x, full_h.min(h) - 90., 264., 56.],
                "All room members",
                "Members",
                false,
            );
        }
        if !self.pending.is_empty() {
            p.text(44., dy + 4., page_w - 36., "Dispatch pending...", 1, BRASS);
        }
        if let Some((reason, rejoin_at)) = &self.kicked {
            p.rect([24., 108., w - 48., (h - 132.).max(80.)], BG, 10.);
            p.text(56., 160., w - 112., "You have left the wire", 3, TEXT);
            p.text(
                56.,
                216.,
                w - 112.,
                if reason.is_empty() {
                    "A moderator removed this room session."
                } else {
                    reason
                },
                2,
                MUTED,
            );
            p.button(
                11,
                [56., 276., 300., 64.],
                "Rejoin room",
                if self.wall >= *rejoin_at {
                    "Rejoin The Brass Relay"
                } else {
                    "Please let the boiler cool"
                },
                true,
            );
        }
        if self.panel != Panel::None {
            let id = p.id();
            let mut dim = node(
                id,
                Rect::new(0., 0., w, full_h),
                Primitive::Superellipse {
                    radius: 0.,
                    exponent: 4.,
                    color: [0, 0, 0, 185],
                },
            );
            dim.responder = Some(TouchResponder::Consume);
            p.nodes.push(dim);
            let pw = 720_f32.min(w - 48.);
            let x = (w - pw) / 2.;
            let y = 100_f32.min((h - 200.).max(20.));
            let ph = match self.panel {
                Panel::Profile => {
                    if keyboard {
                        260_f32
                    } else {
                        580_f32
                    }
                }
                Panel::Members => 650.,
                Panel::Auth => 380.,
                Panel::Help => 520.,
                _ => 280.,
            }
            .min((h - y - 24.).max(200.));
            p.rect([x, y, pw, ph], CARD, 12.);
            p.button(
                6,
                [x + pw - 72., y + 16., 52., 52.],
                "Close dialog",
                "x",
                false,
            );
            let title = match self.panel {
                Panel::Profile => "Your calling card",
                Panel::Members => "Operators on the wire",
                Panel::Person => self
                    .selected
                    .as_ref()
                    .map_or("Member", |m| m.profile.name.as_str()),
                Panel::Kick => "Kick this member?",
                Panel::Help => "The operator's handbook",
                Panel::Auth => "Monitoring sign-in",
                Panel::None => "",
            };
            p.text(x + 24., y + 28., pw - 120., title, 3, TEXT);
            match self.panel {
                Panel::Profile => {
                    if self.temporary {
                        p.text(
                            x + 24.,
                            y + ph - 174.,
                            pw - 48.,
                            "Temporary guest - identity changes on reload",
                            1,
                            BRASS,
                        );
                    }
                    p.button(
                        8,
                        [x + 24., y + 88., pw - 48., 60.],
                        "Edit room name",
                        "",
                        false,
                    );
                    let id = p.id();
                    let value = if self.focus == Some(Focus::Name) {
                        self.input_display(self.font.as_deref())
                    } else {
                        self.nickname.text().into()
                    };
                    p.nodes.last_mut().unwrap().children.push(text(
                        id,
                        Rect::new(12., 20., pw - 72., 28.),
                        &value,
                        2,
                        TEXT,
                    ));
                    if !keyboard {
                        let cell = (pw - 64.) / 8.;
                        for i in 0..16_u8 {
                            let avatar_id = self.avatar_page * 16 + i;
                            let col = i % 8;
                            let row = i / 8;
                            p.avatar_button(
                                20 + u16::from(avatar_id),
                                [
                                    x + 24. + f32::from(col) * cell,
                                    y + 176. + f32::from(row) * cell,
                                    cell - 4.,
                                    cell - 4.,
                                ],
                                avatar_id,
                                &super::icons::name(avatar_id),
                                self.avatar == avatar_id,
                            );
                        }
                        p.button(
                            9,
                            [x + 24., y + 176. + cell * 2. + 12., 100., 48.],
                            "Previous avatars",
                            "<",
                            false,
                        );
                        p.text(
                            x + 148.,
                            y + 192. + cell * 2.,
                            pw - 290.,
                            &format!("{} / 4", self.avatar_page + 1),
                            2,
                            MUTED,
                        );
                        p.button(
                            10,
                            [x + pw - 124., y + 176. + cell * 2. + 12., 100., 48.],
                            "Next avatars",
                            ">",
                            false,
                        );
                        p.button(
                            18,
                            [x + 24., y + ph - 132., pw - 48., 48.],
                            "Change particle effects",
                            match self.effects {
                                0 => "Effects: Off",
                                1 => "Effects: Quiet",
                                _ => "Effects: Playful",
                            },
                            false,
                        );
                    }
                    p.button(
                        7,
                        [x + pw - 224., y + ph - 70., 200., 52.],
                        "Save profile",
                        "Save",
                        true,
                    );
                }
                Panel::Members => {
                    p.text(
                        x + 24.,
                        y + 82.,
                        pw - 48.,
                        &format!("{} on the wire - @ means Admin", self.members.len()),
                        2,
                        MUTED,
                    );
                    for (i, member) in self
                        .members
                        .iter()
                        .enumerate()
                        .skip(self.member_page * 8)
                        .take(8)
                    {
                        p.member(
                            100 + i as u16,
                            x + 24.,
                            y + 128. + (i % 8) as f32 * 56.,
                            pw - 48.,
                            member,
                        );
                    }
                    p.button(
                        190,
                        [x + 24., y + ph - 62., 72., 44.],
                        "Previous members",
                        "<",
                        false,
                    );
                    p.button(
                        191,
                        [x + 108., y + ph - 62., 72., 44.],
                        "Next members",
                        ">",
                        false,
                    );
                    p.button(
                        14,
                        [x + pw - 280., y + ph - 62., 256., 44.],
                        if self.you.as_ref().is_some_and(|m| m.admin) {
                            "Sign out admin"
                        } else {
                            "Monitoring admin sign-in"
                        },
                        if self.you.as_ref().is_some_and(|m| m.admin) {
                            "Sign out admin"
                        } else {
                            "Admin sign-in"
                        },
                        false,
                    );
                }
                Panel::Person => {
                    p.button(
                        182,
                        [x + 24., y + 128., 200., 56.],
                        "Salute member",
                        "Salute",
                        true,
                    );
                    if self.you.as_ref().is_some_and(|m| m.admin)
                        && self.selected.as_ref().is_some_and(|m| {
                            self.you
                                .as_ref()
                                .is_none_or(|you| m.profile.id != you.profile.id)
                        })
                    {
                        p.button(
                            181,
                            [x + pw - 248., y + 128., 224., 56.],
                            "Kick member",
                            "Kick from room",
                            false,
                        );
                    }
                }
                Panel::Kick => {
                    if let Some(target) = &self.selected {
                        p.text(
                            x + 24.,
                            y + 104.,
                            pw - 48.,
                            &format!("Remove {} from the room?", target.profile.name),
                            2,
                            TEXT,
                        );
                        p.text(
                            x + 24.,
                            y + 142.,
                            pw - 48.,
                            "All their room tabs leave. Rejoin after 60 seconds.",
                            1,
                            MUTED,
                        );
                        p.button(
                            180,
                            [x + pw - 248., y + ph - 80., 224., 56.],
                            "Confirm kick",
                            "Kick member",
                            true,
                        );
                    }
                }
                Panel::Help => {
                    for (i, line) in [
                        "/me action - send a stage direction",
                        "/roll 2d6 - consult the probability engine",
                        "/salute name - tip your hat",
                        "/steam - a small puff from the boiler",
                        "/nick name - change your calling card",
                        "/avatar - choose your emblem",
                        "/who - see who is on the wire",
                        "/kick name - admin: review a removal",
                        "//text - send a literal slash",
                    ]
                    .iter()
                    .enumerate()
                    {
                        p.text(x + 24., y + 94. + i as f32 * 38., pw - 48., line, 2, MUTED);
                    }
                }
                Panel::Auth => {
                    for (command, offset, label, buffer, focus) in [
                        (
                            15,
                            100.,
                            "Monitoring username",
                            &self.username,
                            Focus::Username,
                        ),
                        (
                            16,
                            180.,
                            "Monitoring password",
                            &self.password,
                            Focus::Password,
                        ),
                    ] {
                        p.button(
                            command,
                            [x + 24., y + offset, pw - 48., 60.],
                            label,
                            "",
                            false,
                        );
                        let id = p.id();
                        let value = if self.focus == Some(focus) {
                            self.input_display(self.font.as_deref())
                        } else if focus == Focus::Password {
                            "*".repeat(buffer.text().graphemes(true).count().min(36))
                        } else {
                            buffer.text().into()
                        };
                        p.nodes.last_mut().unwrap().children.push(text(
                            id,
                            Rect::new(12., 20., pw - 72., 28.),
                            if value.is_empty() { label } else { &value },
                            2,
                            TEXT,
                        ));
                    }
                    p.button(
                        17,
                        [x + pw - 224., y + ph - 84., 200., 56.],
                        "Sign in admin",
                        "Sign in",
                        true,
                    );
                }
                _ => {}
            }
        }
        if keyboard
            && let Some(keys) = base
                .root()
                .children
                .iter()
                .rev()
                .find(|n| n.children.len() > 30)
        {
            let mut keys = keys.clone();
            keys.frame.x = 0.;
            keys.frame.y = -480.;
            keys.frame.width = 1280.;
            keys.frame.height = 800.;
            let mut viewport = node(
                500_000,
                Rect::new(0., h, w, full_h - h),
                Primitive::Viewport { scale: w / 1280. },
            );
            viewport.children.push(keys);
            p.nodes.push(viewport);
        }
        Scene::new(canvas(900_000, Rect::new(0., 0., w, full_h), p.nodes))
    }
}
