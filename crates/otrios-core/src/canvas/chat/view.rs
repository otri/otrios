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
    buttons: [u8; 201],
    line_height: f32,
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
    fn surface(&mut self, r: [f32; 4], fill: Color, border: Color, radius: f32) {
        self.rect(r, border, radius);
        self.rect(
            [r[0] + 1.5, r[1] + 1.5, r[2] - 3., r[3] - 3.],
            fill,
            (radius - 1.5).max(0.),
        );
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
            Rect::new(x, y, w.max(1.), (14. * scale as f32).max(20.)),
            &s,
            scale,
            color,
        ));
    }
    fn button(&mut self, command: u16, r: [f32; 4], label: &str, caption: &str, primary: bool) {
        // Wrapping drafts must not change the identity of later controls while
        // a host still has an input event queued against the previous frame.
        let occurrence = &mut self.buttons[usize::from(command)];
        let id = 3_000_000 + u32::from(command) * 2048 + u32::from(*occurrence) * 8;
        *occurrence += 1;
        let color = if primary {
            BRASS
        } else if matches!(command, 1 | 8 | 15 | 16) {
            BG
        } else {
            CARD
        };
        let mut children = vec![
            node(
                id + 1,
                Rect::new(0., 0., r[2], r[3]),
                Primitive::Superellipse {
                    radius: 12.,
                    exponent: 4.,
                    color: if primary { [235, 204, 148, 255] } else { LINE },
                },
            ),
            node(
                id + 2,
                Rect::new(1.5, 1.5, r[2] - 3., r[3] - 3.),
                Primitive::Superellipse {
                    radius: 10.5,
                    exponent: 4.,
                    color,
                },
            ),
        ];
        if !caption.is_empty() {
            children.push(text(
                id + 3,
                Rect::new(
                    14.,
                    (r[3] - self.line_height) / 2.,
                    (r[2] - 28.).max(1.),
                    self.line_height,
                ),
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
        self.button(command, r, label, "", false);
        if selected {
            let button = self.nodes.last_mut().unwrap();
            if let Primitive::Superellipse { color, .. } = &mut button.children[0].primitive {
                *color = BRASS;
            }
            if let Primitive::Superellipse { color, .. } = &mut button.children[1].primitive {
                *color = [54, 47, 37, 255];
            }
            button.semantics.as_mut().unwrap().checked = true;
        }
        let id = self.id();
        let mut icon = avatar(avatar_id, id, r[2].min(r[3]) - 12.);
        icon.frame.x = (r[2] - icon.frame.width) / 2.;
        icon.frame.y = (r[3] - icon.frame.height) / 2.;
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
            Rect::new(
                54.,
                (52. - self.line_height) / 2.,
                (width - 62.).max(1.),
                self.line_height,
            ),
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
            buttons: [0; 201],
            line_height: self.line_height(),
        };
        for particle in &self.particles {
            let t = self.now.saturating_sub(particle.at) as f32 / 1000.;
            let mut color = if particle.steam { MUTED } else { COPPER };
            color[3] = ((1. - t / 0.9).max(0.) * 200.) as u8;
            let s = if particle.steam { 10. + t * 12. } else { 4. };
            let x = particle.origin[0] + particle.velocity[0] * t;
            let y = particle.origin[1] + particle.velocity[1] * t;
            if x + s < 0. || y + s < 0. || x >= w || y >= h {
                continue;
            }
            p.rect(
                [x, y, s, s],
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
        // Header typography follows the host's standard app layout, independently
        // of the denser chat body and keyboard-shortened visual viewport.
        let header_w = self.header_width.unwrap_or(w.max(800.));
        let header_ratio = w / header_w;
        let header_bottom = 96. * header_ratio;
        let page_w = w - 48. - side;
        let mut p = Paint {
            nodes: vec![],
            next: 2_000_000,
            buttons: [0; 201],
            line_height: self.line_height(),
        };
        p.rect([0., 0., w, full_h], BG, 0.);
        let header_start = p.nodes.len();
        let heading_scale = otrios_interfaces::theme::HEADER_FONT_SCALE;
        let measure = |value: &str| {
            self.font
                .as_ref()
                .map_or(value.len() as f32 * 8. * heading_scale as f32, |f| {
                    f.text_width(value, heading_scale as f32)
                })
        };
        let back_width = measure("< OTriOS");
        let line_height = self
            .font
            .as_ref()
            .map_or(40., |f| f.metrics().line_height(heading_scale as f32));
        let heading_y = 24. + (60. - line_height) / 2.;
        p.button(
            0,
            [24., 24., back_width + 8., 60.],
            "Back to OTriOS",
            "",
            false,
        );
        let back = p.nodes.last_mut().unwrap();
        for child in &mut back.children {
            if let Primitive::Superellipse { color, .. } = &mut child.primitive {
                *color = BG;
            }
        }
        back.children.push(text(
            back.id + 3,
            Rect::new(0., heading_y - 24., back_width + 8., line_height),
            "< OTriOS",
            heading_scale,
            TEXT,
        ));
        let separator_x = 24. + back_width + 18.;
        p.text(separator_x, heading_y, 24., "•", heading_scale, BRASS);
        let title_x = separator_x + measure("•") + 18.;
        p.text(
            title_x,
            heading_y,
            (header_w - 240. - title_x).max(1.),
            "The Brass Relay",
            heading_scale,
            TEXT,
        );
        p.rect([24., 95., header_w - 48., 1.], LINE, 0.);
        p.button(
            3,
            [header_w - 224., 24., 96., 60.],
            "Members",
            &format!("{} users", self.members.len()),
            false,
        );
        p.button(
            4,
            [header_w - 116., 24., 92., 60.],
            "Edit profile",
            "You",
            false,
        );
        let mut header = node(
            7_000_000,
            Rect::new(0., 0., w, header_bottom),
            Primitive::Viewport {
                scale: header_ratio,
            },
        );
        header.children = p.nodes.split_off(header_start);
        p.nodes.push(header);
        let page_y = header_bottom + 12.;
        let draft_h = self.draft_height();
        let content_h = self.draft_y() - page_y;
        p.surface(
            [
                24.,
                page_y,
                page_w,
                content_h + draft_h + self.input_height() + 8.,
            ],
            CARD,
            LINE,
            10.,
        );
        let mut viewport = canvas(
            u32::from(VIEWPORT),
            Rect::new(24., page_y + 16., (page_w - 16.).max(1.), content_h - 24.),
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
        if self.new_messages > 0 || self.more_after {
            p.button(
                12,
                [48., dy - 48., 260., 44.],
                "Show new messages",
                "Latest dispatches",
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
            p.rect([40., dy, page_w - 32., draft_h - 14.], BG, 3.);
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
            let start = Self::draft_start(draft);
            for line in start..draft.layout.lines.len().min(start + 4) {
                p.text(
                    50.,
                    dy + 27. + (line - start) as f32 * draft.layout.line_height,
                    page_w - 52.,
                    &draft.layout.display(line, None),
                    2,
                    MUTED,
                );
            }
        }
        let cy = self.compose_y();
        let input_h = self.input_height();
        p.button(19, [36., cy, 40., input_h], "Commands", "/", false);
        p.button(
            1,
            [84., cy, page_w - 260., input_h],
            "Message composer",
            "",
            false,
        );
        let mut rows = vec![];
        if self.compose.text().is_empty() {
            let id = p.id();
            rows.push(text(
                id,
                Rect::new(0., 0., self.input_width(), self.line_height()),
                if self.focus == Some(Focus::Compose) {
                    "|"
                } else {
                    "Message..."
                },
                2,
                MUTED,
            ));
        } else {
            for (y, value) in self.composer_lines() {
                let id = p.id();
                rows.push(text(
                    id,
                    Rect::new(0., y, self.input_width(), self.line_height()),
                    &value,
                    2,
                    TEXT,
                ));
            }
        }
        if !self.compose.text().is_empty()
            && self.focus == Some(Focus::Compose)
            && let Some(font) = &self.font
        {
            let caret = self.compose_layout.position(self.compose.cursor(), font);
            let id = p.id();
            rows.push(text(
                id,
                Rect::new(
                    caret[0],
                    caret[1] - self.input_view.offset,
                    font.text_width("|", 2.),
                    self.line_height(),
                ),
                "|",
                2,
                TEXT,
            ));
        }
        let id = p.id();
        p.nodes.last_mut().unwrap().children.push(canvas(
            id,
            Rect::new(12., 12., self.input_width(), input_h - 24.),
            rows,
        ));
        p.button(
            5,
            [page_w - 164., cy, 82., input_h],
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
            [page_w - 74., cy, 94., input_h],
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
            let compact_profile = keyboard || h < 720.;
            let y = if self.panel == Panel::Profile && h < 800. {
                20.
            } else {
                100_f32.min((h - 200.).max(20.))
            };
            let ph = match self.panel {
                Panel::Profile => {
                    if compact_profile {
                        260_f32
                    } else {
                        676_f32
                    }
                }
                Panel::Members => 650.,
                Panel::Auth => 380.,
                Panel::Help => 520.,
                _ => 280.,
            }
            .min((h - y - 24.).max(200.));
            p.surface([x, y, pw, ph], CARD, BRASS, 12.);
            p.button(
                6,
                [x + pw - 72., y + 16., 52., 52.],
                "Close dialog",
                "",
                false,
            );
            let close = p.nodes.last_mut().unwrap();
            for (i, points) in [vec![[19., 19.], [33., 33.]], vec![[19., 33.], [33., 19.]]]
                .into_iter()
                .enumerate()
            {
                close.children.push(node(
                    close.id + 4 + i as u32,
                    Rect::new(0., 0., 52., 52.),
                    Primitive::Path {
                        points,
                        width: 1.8,
                        color: TEXT,
                    },
                ));
            }
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
                    p.text(x + 24., y + 70., pw - 48., "Your room name", 2, MUTED);
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
                        [x + 24., y + 104., pw - 48., 60.],
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
                        Rect::new(
                            12.,
                            (60. - self.line_height()) / 2.,
                            pw - 72.,
                            self.line_height(),
                        ),
                        &value,
                        2,
                        TEXT,
                    ));
                    if !compact_profile {
                        p.text(
                            x + 24.,
                            y + 178.,
                            pw - 48.,
                            "Pick your tiny alter ego",
                            2,
                            MUTED,
                        );
                        let cell = (pw - 64.) / 6.;
                        for i in 0..16_u8 {
                            // Every page presents two identities from each family.
                            let avatar_id = (i / 2) * 8 + self.avatar_page * 2 + i % 2;
                            let col = i % 6;
                            let row = i / 6;
                            p.avatar_button(
                                20 + u16::from(avatar_id),
                                [
                                    x + 24. + f32::from(col) * cell,
                                    y + 212. + f32::from(row) * 88.,
                                    cell - 8.,
                                    80.,
                                ],
                                avatar_id,
                                &super::icons::name(avatar_id),
                                self.avatar == avatar_id,
                            );
                        }
                        p.button(
                            9,
                            [x + 24., y + 488., 100., 48.],
                            "Previous avatars",
                            "<",
                            false,
                        );
                        p.text(
                            x + 148.,
                            y + 488. + (48. - self.line_height()) / 2.,
                            pw - 290.,
                            &format!("{} / 4", self.avatar_page + 1),
                            2,
                            MUTED,
                        );
                        p.button(
                            10,
                            [x + pw - 124., y + 488., 100., 48.],
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
                        6,
                        [x + pw - 364., y + ph - 70., 128., 52.],
                        "Cancel profile",
                        "Cancel",
                        false,
                    );
                    p.button(
                        7,
                        [x + pw - 224., y + ph - 70., 200., 52.],
                        "Save profile",
                        "That's me",
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
                            Rect::new(
                                12.,
                                (60. - self.line_height()) / 2.,
                                pw - 72.,
                                self.line_height(),
                            ),
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
            keys.frame.y = 0.;
            keys.frame.width = 1280.;
            keys.frame.height = 320.;
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
