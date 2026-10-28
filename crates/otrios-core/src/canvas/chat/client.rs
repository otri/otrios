// SPDX-License-Identifier: LGPL-3.0-only
//! Mount-local room state. Keystrokes never become CRDT writes.
use super::super::document::{Block, BlockId, Inline, Source};
use super::*;
use crate::dashboard::{font::Font, keyboard::TextBuffer};
use otrios_interfaces::{
    keyboard::Edit,
    scene::{PointerEvent, PointerPhase},
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
pub const VIEWPORT: u16 = 60_000;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
    None,
    Members,
    Profile,
    Person,
    Kick,
    Help,
    Auth,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Compose,
    Name,
    Username,
    Password,
}
pub(super) enum Spark {
    Compose(usize),
    Draft(String, usize),
}
pub(super) struct RemoteDraft {
    pub layout: super::text::Wrapped,
    pub caret: usize,
    pub text: String,
    pub expires: u64,
    pub generation: u64,
    pub revision: u64,
}
pub(super) struct Particle {
    pub at: u64,
    pub origin: [f32; 2],
    pub velocity: [f32; 2],
    pub steam: bool,
}
pub(super) struct Pending {
    nonce: String,
    epoch: u64,
    text: String,
    sent: bool,
}
pub(crate) enum Action {
    Changed,
    Home,
    BrowserLogin,
}
pub struct Client {
    pub(super) you: Option<Member>,
    pub(super) members: Vec<Member>,
    pub(super) drafts: BTreeMap<String, RemoteDraft>,
    pub(super) messages: VecDeque<Message>,
    pub(super) collection: crate::collection::Client,
    pub(super) collection_anchor: crate::collection::Anchor,
    pub(crate) collection_revoked: bool,
    pub(crate) more_after: bool,
    pub(super) authors_revision: u64,
    pub(super) source_blocks: BTreeMap<BlockId, ([u8; 32], Arc<Block>)>,
    pub(super) message_revisions: BTreeMap<u64, [u8; 32]>,
    avatars: BTreeMap<u8, Arc<Inline>>,
    pub(super) source_messages: BTreeMap<u64, (u32, u32)>,
    pub(super) panel: Panel,
    pub(super) focus: Option<Focus>,
    pub(super) compose: TextBuffer,
    pub(super) compose_layout: super::text::Wrapped,
    pending_spark: Option<Spark>,
    pub(super) nickname: TextBuffer,
    pub(super) username: TextBuffer,
    pub(super) password: TextBuffer,
    pub(super) avatar: u8,
    pub(super) avatar_page: u8,
    pub(super) member_page: usize,
    pub(super) selected: Option<Member>,
    pub(super) private: bool,
    pub(super) effects: u8,
    pub(super) native: bool,
    pub(super) online: bool,
    pub(super) temporary: bool,
    pub(super) kicked: Option<(String, u64)>,
    pub(super) notice: String,
    notice_until: u64,
    pub(super) now: u64,
    pub(super) wall: u64,
    pub(super) particles: Vec<Particle>,
    pub(super) input_view: super::super::text_viewport::TextViewport,
    pub(super) new_messages: usize,
    pub(super) older: bool,
    pub(super) history_wait: bool,
    pub(super) pending: VecDeque<Pending>,
    queue: VecDeque<Command>,
    pub(super) joining: bool,
    epoch: u64,
    draft_generation: u64,
    draft_revision: u64,
    draft_sent: String,
    draft_at: u64,
    draft_dirty: bool,
    disclosed: bool,
    last_effect: u64,
    pub(super) reset: bool,
    pub(super) tail: bool,
    pub(super) font: Option<std::sync::Arc<Font>>,
    pub(super) viewport: [f32; 2],
    pub(super) header_width: Option<f32>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Info {
    pub participant: String,
    pub name: String,
    pub avatar: u8,
    pub admin: bool,
    pub online: bool,
    pub messages: usize,
    pub authors_revision: u64,
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub pending: usize,
    pub drafts: usize,
    pub history_loading: bool,
    pub selected_bytes: usize,
    pub replica_bytes: usize,
    pub lease_generation: u64,
    pub kicked: bool,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            you: None,
            members: vec![],
            drafts: BTreeMap::new(),
            messages: VecDeque::new(),
            collection: crate::collection::Client::new(COLLECTION, super::message_schema()),
            collection_anchor: crate::collection::Anchor::Tail,
            collection_revoked: false,
            more_after: false,
            authors_revision: 0,
            source_blocks: BTreeMap::new(),
            message_revisions: BTreeMap::new(),
            avatars: BTreeMap::new(),
            source_messages: BTreeMap::new(),
            panel: Panel::None,
            focus: None,
            compose: TextBuffer::multiline(2048),
            compose_layout: Default::default(),
            pending_spark: None,
            nickname: TextBuffer::with_limit(96),
            username: TextBuffer::with_limit(320),
            password: TextBuffer::with_limit(1024),
            avatar: 0,
            avatar_page: 0,
            member_page: 0,
            selected: None,
            private: false,
            effects: 2,
            native: false,
            online: false,
            temporary: false,
            kicked: None,
            notice: String::new(),
            notice_until: 0,
            now: 0,
            wall: 0,
            particles: vec![],
            input_view: Default::default(),
            new_messages: 0,
            older: false,
            history_wait: false,
            pending: VecDeque::new(),
            queue: VecDeque::new(),
            joining: false,
            epoch: 0,
            draft_generation: 0,
            draft_revision: 0,
            draft_sent: String::new(),
            draft_at: 0,
            draft_dirty: false,
            disclosed: false,
            last_effect: 0,
            reset: false,
            tail: false,
            font: None,
            viewport: [1280., 800.],
            header_width: None,
        }
    }
}
// Gregorian civil date from Unix days. Timestamps and separators use UTC.
fn day_label(day: u64, today: u64) -> String {
    if day == today {
        return "Today".into();
    }
    if day.checked_add(1) == Some(today) {
        return "Yesterday".into();
    }
    let z = day.min(3_000_000) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(m <= 2);
    format!("{year:04}-{m:02}-{d:02}")
}
impl Client {
    pub(crate) fn set_header_width(&mut self, width: f32) {
        self.header_width = Some(width);
    }
    pub(crate) fn prepare(
        &mut self,
        size: [f32; 2],
        keyboard: bool,
        font: Option<std::sync::Arc<Font>>,
    ) {
        self.viewport = [size[0], super::view::room_height(size, keyboard)];
        self.font = font;
        self.refresh_text_layouts();
    }
    pub(crate) fn native(&mut self, native: bool) {
        self.native = native;
    }
    pub fn info(&self) -> Info {
        Info {
            participant: self
                .you
                .as_ref()
                .map_or_else(String::new, |y| y.profile.id.clone()),
            name: self
                .you
                .as_ref()
                .map_or_else(String::new, |y| y.profile.name.clone()),
            avatar: self.avatar,
            admin: self.you.as_ref().is_some_and(|y| y.admin),
            online: self.online,
            messages: self.messages.len(),
            authors_revision: self.authors_revision,
            first_sequence: self.messages.front().map_or(0, |m| m.sequence),
            last_sequence: self.messages.back().map_or(0, |m| m.sequence),
            pending: self.pending.len(),
            drafts: self.drafts.len(),
            history_loading: self.history_wait,
            selected_bytes: self
                .collection
                .window()
                .map_or(0, |window| window.selected_bytes),
            replica_bytes: self
                .collection
                .window()
                .map_or(0, |window| window.snapshot().canonical_bytes()),
            lease_generation: self.collection.generation(),
            kicked: self.kicked.is_some(),
        }
    }
    pub fn recent_messages(&self) -> Vec<Message> {
        self.messages
            .iter()
            .rev()
            .take(64)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
    pub(crate) fn close_modal(&mut self) -> bool {
        if self.panel == Panel::None {
            return false;
        }
        self.panel = Panel::None;
        self.focus = None;
        self.password.replace("");
        true
    }
    pub(crate) fn synchronize_document(
        &mut self,
        viewport: &mut super::super::document::Viewport,
    ) -> Result<()> {
        if self.reset {
            let source = self.message_source()?;
            viewport.replace_blocks(source)?;
            self.reset = false;
        }
        if self.tail {
            viewport.follow_tail();
            self.tail = false;
            self.new_messages = 0;
        }
        Ok(())
    }
    fn avatar_inline(&mut self, avatar: u8) -> Result<Arc<Inline>> {
        if avatar >= 64 {
            return Err("Invalid sent avatar".into());
        }
        if let Some(inline) = self.avatars.get(&avatar) {
            return Ok(inline.clone());
        }
        let scene = crate::dashboard::scene::Scene::new(super::icons::avatar(avatar, 1, 32.))?;
        let mesh = crate::dashboard::render_scene(&scene);
        let inline = Inline::new(mesh.vertices, [12., -4.])?;
        while self.avatars.values().map(|i| i.bytes()).sum::<usize>() + inline.bytes()
            > super::super::document::MAX_WINDOW_INLINE_BYTES
        {
            let Some(first) = self.avatars.keys().next().copied() else {
                return Err("Avatar source budget".into());
            };
            self.avatars.remove(&first);
        }
        self.avatars.insert(avatar, inline.clone());
        Ok(inline)
    }
    fn message_source(&mut self) -> Result<Source> {
        let mut blocks = Vec::new();
        let mut cache = BTreeMap::new();
        let mut positions = BTreeMap::new();
        if let Some(first) = self.messages.front() {
            let spacer = BlockId {
                object: first.sequence,
                part: 2,
            };
            let block = self
                .source_blocks
                .get(&spacer)
                .map(|v| v.1.clone())
                .map_or_else(
                    || {
                        Block::label(
                            spacer,
                            String::new(),
                            64.,
                            BlockId {
                                object: first.sequence,
                                part: 0,
                            },
                        )
                    },
                    Ok,
                )?;
            cache.insert(spacer, ([0; 32], block.clone()));
            blocks.push(block);
        }
        let mut previous_day = None;
        // The input is already the bounded coherent projection, never a log.
        for message in self.messages.iter().cloned().collect::<Vec<_>>() {
            let day = message.at / 86_400_000;
            if previous_day != Some(day) {
                let id = BlockId {
                    object: message.sequence,
                    part: 1,
                };
                let label = day_label(day, self.wall / 86_400_000);
                let revision: [u8; 32] = super::super::hash(label.as_bytes()).try_into().unwrap();
                let block = if let Some((_, block)) = self
                    .source_blocks
                    .get(&id)
                    .filter(|(old, _)| *old == revision)
                {
                    block.clone()
                } else {
                    Block::label(
                        id,
                        label,
                        64.,
                        BlockId {
                            object: message.sequence,
                            part: 0,
                        },
                    )?
                };
                cache.insert(id, (revision, block.clone()));
                blocks.push(block);
                previous_day = Some(day);
            }
            let id = BlockId {
                object: message.sequence,
                part: 0,
            };
            let revision = *self
                .message_revisions
                .get(&message.sequence)
                .ok_or("Missing selected message revision")?;
            let block = if let Some((_, block)) = self
                .source_blocks
                .get(&id)
                .filter(|(old, _)| *old == revision)
            {
                block.clone()
            } else {
                let minutes = message.at / 60_000;
                let header = format!(
                    "{:02}:{:02}  {}",
                    minutes / 60 % 24,
                    minutes % 60,
                    message.sender.name
                );
                let body = if message.kind == Kind::Action {
                    format!("* {} {}", message.sender.name, message.text)
                } else {
                    message.text.clone()
                };
                Block::new(
                    id,
                    format!("{header}\n{}\n\n", body.replace('\r', "")),
                    64.,
                    Some(self.avatar_inline(message.sender.avatar)?),
                )?
            };
            positions.insert(message.sequence, (blocks.len() as u32, 0));
            cache.insert(id, (revision, block.clone()));
            blocks.push(block);
        }
        let source = Source::from_blocks(blocks)?;
        self.source_blocks = cache;
        self.source_messages = positions;
        Ok(source)
    }
    pub(super) fn say(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_until = self.now + 4000;
    }
    fn queue(&mut self, command: Command) {
        if self.queue.len() < 16 {
            self.queue.push_back(command);
        } else {
            self.say("Room command queue is full; please wait");
        }
    }
    pub fn disconnected(&mut self) {
        self.collection.disconnected();
        self.online = false;
        self.joining = false;
        self.drafts.clear();
        self.draft_generation = 0;
        self.queue.clear();
        for pending in &mut self.pending {
            pending.sent = false;
        }
        self.draft_dirty = true;
    }
    pub fn confirmed(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn editing(&self) -> bool {
        self.focus.is_some()
    }
    pub fn quiet(&mut self, reduced: bool) {
        if reduced {
            self.effects = 0;
            self.particles.clear();
        }
    }
    pub fn outgoing(&mut self, credential: Option<String>) -> Result<Option<Vec<u8>>> {
        if !self.online && !self.joining && self.kicked.is_none() {
            self.joining = true;
            return encode_command(&Command::Join { credential }).map(Some);
        }
        if !self.online {
            return Ok(None);
        }
        if let Some(command) = self.queue.pop_front() {
            return encode_command(&command).map(Some);
        }
        if let Some(pending) = self.pending.iter_mut().find(|p| !p.sent) {
            pending.sent = true;
            return encode_command(&Command::Send {
                epoch: pending.epoch,
                nonce: pending.nonce.clone(),
                text: pending.text.clone(),
            })
            .map(Some);
        }
        let wanted = if self.private
            || self.compose.text().starts_with('/') && !self.compose.text().starts_with("//")
        {
            String::new()
        } else {
            let mut result = String::new();
            for cluster in self.compose.text().graphemes(true).take(256) {
                if result.len() + cluster.len() > 1024 {
                    break;
                }
                result.push_str(cluster);
            }
            result
        };
        if self.draft_generation != 0
            && ((self.draft_dirty && self.now.saturating_sub(self.draft_at) >= 100)
                || (wanted.is_empty() && !self.draft_sent.is_empty())
                || (!wanted.is_empty() && self.now.saturating_sub(self.draft_at) >= 1000))
        {
            self.draft_revision = self.draft_revision.saturating_add(1);
            self.draft_at = self.now;
            self.draft_dirty = false;
            self.draft_sent = wanted.clone();
            return encode_command(&Command::Draft {
                generation: self.draft_generation,
                revision: self.draft_revision,
                text: wanted,
            })
            .map(Some);
        }
        Ok(None)
    }
    fn refresh_authors(&mut self) {
        if let Some(selected) = &mut self.selected
            && let Some(member) = self
                .members
                .iter()
                .find(|m| m.profile.id == selected.profile.id)
        {
            *selected = member.clone();
        }
    }
    pub fn receive(&mut self, event: Event, follow: bool) -> Result<Option<String>> {
        let mut credential = None;
        match event {
            Event::Left => {
                self.clear_collection();
                self.disconnected();
            }
            Event::Welcome {
                you,
                temporary,
                credential: token,
                epoch,
                members,
                ..
            } => {
                self.temporary = temporary;
                self.epoch = epoch;
                self.avatar = you.profile.avatar;
                self.nickname.replace(&you.profile.name);
                self.you = Some(you);
                self.members = members;
                self.online = true;
                self.joining = true;
                self.kicked = None;
                credential = token;
                self.collection_revoked = false;
                let anchor = if self.messages.is_empty() || follow {
                    self.tail = true;
                    crate::collection::Anchor::Tail
                } else {
                    crate::collection::Anchor::Around(self.messages.front().unwrap().sequence)
                };
                self.select_window(anchor)?;
            }
            Event::Members { members } => {
                if members.len() > 68 {
                    return Err("Roster exceeds room capacity".into());
                }
                if let Some(you) = &self.you
                    && let Some(member) = members.iter().find(|m| m.profile.id == you.profile.id)
                {
                    let mut member = member.clone();
                    member.admin = you.admin;
                    self.you = Some(member);
                }
                self.members = members;
                self.refresh_authors();
                self.drafts
                    .retain(|id, _| self.members.iter().any(|m| m.profile.id == *id));
            }
            Event::Receipt { nonce, .. } => {
                let mine = self.pending.iter().any(|p| p.nonce == nonce);
                self.pending.retain(|p| p.nonce != nonce);
                if mine {
                    self.tail = true;
                    if !matches!(self.collection_anchor, crate::collection::Anchor::Tail) {
                        self.select_window(crate::collection::Anchor::Tail)?;
                    }
                }
            }
            Event::DraftLease { generation } => {
                self.draft_generation = generation;
                self.draft_revision = 0;
                self.draft_dirty = true;
            }
            Event::Draft {
                participant,
                generation,
                revision,
                text,
                expires,
            } => {
                if self
                    .you
                    .as_ref()
                    .is_some_and(|you| you.profile.id == participant)
                {
                    if generation == self.draft_generation {
                        self.draft_revision = self.draft_revision.max(revision);
                    }
                    return Ok(credential);
                }
                if text.len() > 1024 {
                    return Err("Draft byte limit".into());
                }
                if let Some(old) = self.drafts.get(&participant)
                    && old.generation == generation
                    && old.revision >= revision
                {
                    return Ok(credential);
                }
                if !text.is_empty()
                    && self.drafts.len() < 68
                    && self.members.iter().any(|m| m.profile.id == participant)
                {
                    let changed = super::text::changed_end(
                        self.drafts
                            .get(&participant)
                            .map_or("", |d| d.text.as_str()),
                        &text,
                    );
                    let caret = changed.unwrap_or_else(|| {
                        self.drafts
                            .get(&participant)
                            .map_or(text.len(), |d| d.caret)
                    });
                    let deadline = self.now + expires.saturating_sub(self.wall).min(3000);
                    let layout = self
                        .drafts
                        .remove(&participant)
                        .map_or_else(Default::default, |d| d.layout);
                    self.drafts.insert(
                        participant.clone(),
                        RemoteDraft {
                            text,
                            caret,
                            layout,
                            expires: deadline,
                            generation,
                            revision,
                        },
                    );
                    self.refresh_text_layouts();
                    if let Some(byte) = changed {
                        self.pending_spark = Some(Spark::Draft(participant, byte));
                    }
                } else {
                    self.drafts.remove(&participant);
                }
            }
            Event::Effect { kind, .. } => self.emit(
                kind == "steam",
                [self.viewport[0] * 0.5, self.viewport[1] - 100.],
            ),
            Event::Permissions { admin } => {
                if let Some(you) = &mut self.you {
                    you.admin = admin;
                }
                self.password.replace("");
                self.panel = Panel::None;
                self.focus = None;
                self.say(if admin {
                    "Monitoring admin signed in"
                } else {
                    "Admin signed out"
                });
            }
            Event::Kicked { reason, rejoin_at } => {
                self.clear_collection();
                self.online = false;
                self.joining = false;
                self.kicked = Some((reason, rejoin_at));
                self.drafts.clear();
                self.particles.clear();
                self.queue.clear();
                self.panel = Panel::None;
                self.focus = None;
                self.pending.clear();
            }
            Event::Error { message, nonce } => {
                if let Some(nonce) = nonce.as_ref()
                    && let Some(p) = self.pending.iter().find(|p| p.nonce == *nonce)
                    && self.compose.text().is_empty()
                {
                    self.compose.replace(&p.text);
                    self.compose.place_cursor(p.text.len());
                }
                self.pending.retain(|p| Some(&p.nonce) != nonce.as_ref());
                self.joining = true;
                self.say(message);
            }
            Event::Resync => self.disconnected(),
        }
        Ok(credential)
    }
    pub fn tick(&mut self, now: u64, wall: u64) -> (bool, bool) {
        let date_changed = self.wall != 0 && self.wall / 86_400_000 != wall / 86_400_000;
        if date_changed && !self.messages.is_empty() {
            self.reset = true;
        }
        self.now = now;
        self.wall = wall;
        let old = self.particles.len();
        self.particles.retain(|p| now.saturating_sub(p.at) < 900);
        let drafts = self.drafts.len();
        self.drafts.retain(|_, d| d.expires > now);
        let clear = !self.notice.is_empty() && now >= self.notice_until;
        if clear {
            self.notice.clear();
        }
        self.refresh_text_layouts();
        let moving = self.input_view.tick(now);
        let mut emitted = false;
        if let Some(anchor) = self.pending_spark.take() {
            if let Some(position) = self.spark_origin(&anchor) {
                self.emit(false, position);
                emitted = true;
            } else if matches!(anchor, Spark::Compose(_)) && self.input_view.animating() {
                self.pending_spark = Some(anchor);
            }
        }
        (
            date_changed || drafts != self.drafts.len() || clear || moving,
            old != 0 || emitted,
        )
    }
    fn emit(&mut self, steam: bool, position: [f32; 2]) {
        if self.effects == 0 || self.now.saturating_sub(self.last_effect) < 40 {
            return;
        }
        self.last_effect = self.now;
        for i in 0..if self.effects == 1 { 1 } else { 3 } {
            if self.particles.len() >= 128 {
                break;
            }
            let angle = (self.now % 127) as f32 * 0.07 + i as f32 * 1.9;
            self.particles.push(Particle {
                at: self.now,
                origin: position,
                velocity: [angle.sin() * 45., -35. - angle.cos().abs() * 45.],
                steam,
            });
        }
    }
    fn editor(&self) -> &TextBuffer {
        match self.focus {
            Some(Focus::Name) => &self.nickname,
            Some(Focus::Username) => &self.username,
            Some(Focus::Password) => &self.password,
            _ => &self.compose,
        }
    }
    pub(crate) fn host_input(
        &self,
        command: u16,
        id: u32,
    ) -> Option<otrios_interfaces::keyboard::TextInput> {
        let (buffer, focus) = match (self.panel, command) {
            (Panel::None, 1) => (&self.compose, Focus::Compose),
            (Panel::Profile, 8) => (&self.nickname, Focus::Name),
            (Panel::Auth, 15) => (&self.username, Focus::Username),
            (Panel::Auth, 16) => (&self.password, Focus::Password),
            _ => return None,
        };
        let mut input = buffer.host_input(id, self.focus == Some(focus));
        input.secret = focus == Focus::Password;
        input.inset = if focus == Focus::Compose {
            [12.; 4]
        } else {
            [
                12.,
                (60. - self.line_height()) / 2.,
                12.,
                (60. - self.line_height()) / 2.,
            ]
        };
        Some(input)
    }
    pub(crate) fn blur_input(&mut self) {
        self.focus = None;
    }
    pub(crate) fn host_scroll(&mut self, offset: f32) -> bool {
        if self.focus != Some(Focus::Compose) || !offset.is_finite() {
            return false;
        }
        let offset = offset.clamp(0., self.input_view.limit());
        let changed = self.input_view.offset != offset || self.input_view.animating();
        self.input_view.cancel();
        self.input_view.offset = offset;
        changed
    }
    pub fn edit(&mut self, edit: &Edit) -> Result<bool> {
        if edit == &Edit::Enter {
            if self.focus == Some(Focus::Compose) {
                self.send()?;
            } else {
                self.focus = None;
            }
            return Ok(true);
        }
        if let Edit::Tab { .. } = edit {
            self.focus = Some(match self.focus {
                Some(Focus::Username) => Focus::Password,
                Some(Focus::Password) => Focus::Username,
                _ => Focus::Compose,
            });
            return Ok(true);
        }
        let buffer = match self.focus {
            Some(Focus::Compose) => &mut self.compose,
            Some(Focus::Name) => &mut self.nickname,
            Some(Focus::Username) => &mut self.username,
            Some(Focus::Password) => &mut self.password,
            None => return Ok(false),
        };
        let old_text = buffer.text().to_owned();
        let changed = buffer.edit(edit);
        let text_changed = old_text != buffer.text();
        self.draft_dirty |= self.focus == Some(Focus::Compose) && text_changed;
        self.refresh_text_layouts();
        if self.focus == Some(Focus::Compose) {
            self.follow_caret();
            if text_changed {
                self.pending_spark = Some(Spark::Compose(self.compose.cursor()));
            }
        }
        Ok(changed)
    }
    fn send(&mut self) -> Result<()> {
        let text = self.compose.text().trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }
        if text == "/help" {
            self.panel = Panel::Help;
            self.compose.replace("");
            return Ok(());
        }
        if text == "/who" {
            self.panel = Panel::Members;
            self.compose.replace("");
            return Ok(());
        }
        if text == "/avatar" {
            self.open_profile();
            self.compose.replace("");
            return Ok(());
        }
        if let Some(name) = text.strip_prefix("/nick ") {
            self.queue(Command::Profile {
                name: name.into(),
                avatar: self.avatar,
            });
            self.compose.replace("");
            return Ok(());
        }
        if let Some(who) = text.strip_prefix("/kick ") {
            let selected = self
                .members
                .iter()
                .find(|m| m.profile.name.to_lowercase() == who.to_lowercase())
                .cloned();
            if let Some(selected) = selected {
                self.selected = Some(selected);
                self.panel = Panel::Kick;
                self.compose.replace("");
            } else {
                self.say("Use a full member name, or select them in Members");
            }
            return Ok(());
        }
        if !self.online {
            self.say("Reconnect before sending");
            return Ok(());
        }
        if self.pending.len() >= 8 {
            self.say("Wait for the pending dispatches");
            return Ok(());
        }
        self.pending.push_back(Pending {
            nonce: random(16)?,
            epoch: (self.wall / 600_000).max(self.epoch),
            text,
            sent: false,
        });
        self.compose.replace("");
        self.input_view = Default::default();
        self.pending_spark = None;
        self.draft_dirty = true;
        self.tail = true;
        Ok(())
    }
    fn open_profile(&mut self) {
        if let Some(you) = &self.you {
            self.nickname.replace(&you.profile.name);
            self.nickname.place_cursor(you.profile.name.len());
            self.avatar = you.profile.avatar;
            self.avatar_page = (self.avatar % 8) / 2;
        }
        self.panel = Panel::Profile;
        self.focus = None;
    }
    pub(crate) fn action(&mut self, command: u16) -> Result<Action> {
        match command {
            0 => return Ok(Action::Home),
            1 => {
                self.focus = Some(Focus::Compose);
                self.queue(Command::ClaimDraft);
                if !self.disclosed && !self.private {
                    self.say("Live drafts are visible before sending");
                    self.disclosed = true;
                }
            }
            2 => self.send()?,
            3 => {
                self.panel = Panel::Members;
                self.focus = None;
            }
            4 => self.open_profile(),
            5 => {
                self.private = !self.private;
                self.draft_dirty = true;
                self.notice.clear();
            }
            6 => {
                self.panel = Panel::None;
                self.focus = None;
                self.password.replace("");
            }
            7 => {
                self.queue(Command::Profile {
                    name: self.nickname.text().into(),
                    avatar: self.avatar,
                });
                self.panel = Panel::None;
                self.focus = None;
            }
            8 => {
                self.focus = Some(Focus::Name);
            }
            9 => {
                self.avatar_page = self.avatar_page.saturating_sub(1);
            }
            10 => {
                self.avatar_page = (self.avatar_page + 1).min(3);
            }
            11 => {
                if self.kicked.as_ref().is_none_or(|(_, at)| self.wall >= *at) {
                    self.kicked = None;
                    self.joining = false;
                }
            }
            12 => {
                self.new_messages = 0;
                self.tail = true;
                self.history_wait = true;
                self.select_window(crate::collection::Anchor::Tail)?;
            }
            13 => self.older()?,
            14 => {
                if self.you.as_ref().is_some_and(|y| y.admin) {
                    self.queue(Command::SignOut);
                } else if self.native {
                    self.panel = Panel::Auth;
                    self.focus = Some(Focus::Username);
                } else {
                    return Ok(Action::BrowserLogin);
                }
            }
            15 => self.focus = Some(Focus::Username),
            16 => self.focus = Some(Focus::Password),
            17 => {
                self.queue(Command::Authenticate {
                    username: self.username.text().into(),
                    password: self.password.text().into(),
                });
                self.password.replace("");
                self.focus = None;
            }
            18 => {
                self.effects = (self.effects + 1) % 3;
                if self.effects == 0 {
                    self.particles.clear();
                }
            }
            19 => {
                self.panel = Panel::Help;
                self.focus = None;
            }
            20..=83 => self.avatar = (command - 20) as u8,
            100..=167 => {
                self.selected = self.members.get((command - 100) as usize).cloned();
                self.panel = Panel::Person;
                self.focus = None;
            }
            180 => {
                if let Some(target) = self.selected.clone() {
                    self.queue(Command::Kick {
                        participant: target.profile.id,
                        generation: target.generation,
                        reason: String::new(),
                    });
                    self.panel = Panel::None;
                }
            }
            181 => self.panel = Panel::Kick,
            182 => {
                if self.pending.len() >= 8 {
                    self.say("Wait for the pending dispatches");
                } else if let Some(target) = &self.selected {
                    let text = format!("/salute {}", target.profile.name);
                    self.pending.push_back(Pending {
                        nonce: random(16)?,
                        epoch: (self.wall / 600_000).max(self.epoch),
                        text,
                        sent: false,
                    });
                    self.panel = Panel::None;
                }
            }
            190 => self.member_page = self.member_page.saturating_sub(1),
            191 => {
                self.member_page =
                    (self.member_page + 1).min(self.members.len().saturating_sub(1) / 8)
            }
            _ => return Err("Unknown room UI action".into()),
        }
        Ok(Action::Changed)
    }
    pub fn older(&mut self) -> Result<()> {
        if self.older
            && !self.collection.waiting()
            && let Some(first) = self.messages.front()
        {
            self.tail = false;
            self.select_window(crate::collection::Anchor::Around(first.sequence))?;
        }
        Ok(())
    }
    pub fn pointer(&mut self, event: PointerEvent, now: u64) -> Option<bool> {
        let [x, y] = event.position;
        if self.panel != Panel::None {
            return None;
        }
        if event.phase == PointerPhase::Down {
            if x < self.input_left()
                || x >= self.input_left() + self.input_width()
                || y < self.compose_y()
                || y > self.compose_y() + self.input_height()
            {
                return None;
            }
            self.input_view.down(event.id, y, now);
            return Some(true);
        }
        if !self.input_view.owns(event.id) {
            return None;
        }
        match event.phase {
            PointerPhase::Move => self.input_view.motion(y, now),
            PointerPhase::Up => {
                let dragged = self.input_view.up(y, now);
                if !dragged {
                    self.focus = Some(Focus::Compose);
                    if let Some(font) = &self.font {
                        let at = self.compose_layout.hit_test(
                            [
                                x - self.input_left(),
                                y - self.compose_y() - 12. + self.input_view.offset,
                            ],
                            font,
                        );
                        self.compose.place_cursor(at);
                    }
                    self.queue(Command::ClaimDraft);
                }
            }
            PointerPhase::Cancel => self.input_view.cancel(),
            _ => {}
        }
        Some(true)
    }
    pub(super) fn page_width(&self) -> f32 {
        self.viewport[0] - 48. - if self.viewport[0] >= 1100. { 288. } else { 0. }
    }
    pub(super) fn input_left(&self) -> f32 {
        96.
    }
    pub(super) fn input_width(&self) -> f32 {
        self.page_width() - 284.
    }
    pub(super) fn compose_y(&self) -> f32 {
        self.viewport[1] - 28. - self.input_height()
    }
    fn refresh_text_layouts(&mut self) {
        let mut rewrapped = false;
        if let Some(font) = &self.font {
            rewrapped = self
                .compose_layout
                .update(self.compose.text(), self.input_width(), font);
            let width = self.page_width() - 52.;
            for draft in self.drafts.values_mut() {
                draft.layout.update(&draft.text, width, font);
            }
        }
        self.input_view.bounds(
            self.compose_layout.lines.len() as f32 * self.line_height(),
            self.input_height() - 24.,
        );
        if rewrapped && self.focus == Some(Focus::Compose) {
            self.follow_caret();
        }
    }
    fn follow_caret(&mut self) {
        if let Some(font) = &self.font {
            let caret = self.compose_layout.position(self.compose.cursor(), font);
            self.input_view.follow(
                caret[1],
                self.line_height(),
                self.input_height() - 24.,
                self.now,
            );
        }
    }
    pub(super) fn line_height(&self) -> f32 {
        self.compose_layout.line_height.max(20.)
    }
    pub(crate) fn input_height(&self) -> f32 {
        let rows = ((self.viewport[1] - 300.) / self.line_height())
            .floor()
            .clamp(1., 4.) as usize;
        (self.compose_layout.lines.len().clamp(1, rows) as f32 * self.line_height() + 24.).max(60.)
    }
    pub(crate) fn draft_height(&self) -> f32 {
        self.drafts.values().next().map_or(32., |d| {
            (d.layout.lines.len().clamp(1, 4) as f32 * d.layout.line_height.max(20.) + 36.).max(72.)
        })
    }
    pub(super) fn draft_y(&self) -> f32 {
        (self.compose_y() - 4. - self.draft_height()).max(172.)
    }
    pub(super) fn draft_start(draft: &RemoteDraft) -> usize {
        draft.layout.line_at(draft.caret).saturating_sub(3)
    }
    fn spark_origin(&self, anchor: &Spark) -> Option<[f32; 2]> {
        if self.panel != Panel::None {
            return None;
        }
        let font = self.font.as_ref()?;
        match anchor {
            Spark::Compose(byte) => {
                if self.compose.text().is_empty() {
                    return None;
                }
                let p = self
                    .compose_layout
                    .position((*byte).min(self.compose.text().len()), font);
                let y = p[1] - self.input_view.offset;
                (y >= -0.5 && y + self.line_height() <= self.input_height() - 23.).then_some([
                    self.input_left() + p[0],
                    self.compose_y() + 12. + y + self.line_height() * 0.5,
                ])
            }
            Spark::Draft(id, byte) => {
                let (visible, draft) = self.drafts.iter().next()?;
                if visible != id {
                    return None;
                }
                let p = draft.layout.position(*byte, font);
                Some([
                    50. + p[0],
                    self.draft_y() + 27. + p[1]
                        - Self::draft_start(draft) as f32 * draft.layout.line_height
                        + draft.layout.line_height * 0.5,
                ])
            }
        }
    }
    pub(super) fn composer_lines(&self) -> Vec<(f32, String)> {
        self.compose_layout
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, _)| {
                let y = i as f32 * self.line_height() - self.input_view.offset;
                (y + self.line_height() > 0. && y < self.input_height() - 24.)
                    .then(|| (y, self.compose_layout.display(i, None)))
            })
            .collect()
    }
    pub(super) fn input_display(&self, font: Option<&Font>) -> String {
        let editor = self.editor();
        if self.focus == Some(Focus::Password) {
            return "*".repeat(editor.text().graphemes(true).count().min(36));
        }
        let value = if let Some(font) = font {
            let width = if self.focus == Some(Focus::Compose) {
                self.input_width()
            } else {
                500.
            };
            let offset = if self.focus == Some(Focus::Compose) {
                self.input_view.offset
            } else {
                (editor.caret_x(font, 2.) - width + 32.).max(0.)
            };
            editor
                .window(offset, width, font, 2., self.focus.is_some())
                .0
        } else {
            editor.display(self.focus.is_some())
        };
        let mut result = String::new();
        for cluster in value.graphemes(true) {
            if result.len() + cluster.len() > 256 {
                break;
            }
            result.push_str(cluster);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(sequence: u64, day: u64) -> Message {
        Message {
            sequence,
            at: day * 86_400_000 + 12 * 3_600_000,
            sender: Profile {
                id: "a".repeat(32),
                name: "Captain Sprocket".into(),
                avatar: 7,
            },
            kind: Kind::Message,
            text: "A clockwork pigeon!".into(),
            nonce: format!("{sequence:032x}"),
        }
    }
    #[test]
    fn day_groups_and_headers_remain_stable_across_append_and_history_rebuild() {
        assert_eq!(day_label(0, 20), "1970-01-01");
        assert_eq!(day_label(19675, 20000), "2023-11-14");
        let mut client = Client {
            wall: 20 * 86_400_000,
            ..Client::default()
        };
        client
            .messages
            .extend([message(1, 19), message(2, 19), message(3, 20)]);
        for message in &client.messages {
            client.message_revisions.insert(
                message.sequence,
                super::super::super::hash(&flexbuffers::to_vec(message).unwrap())
                    .try_into()
                    .unwrap(),
            );
        }
        let source = client.message_source().unwrap();
        assert_eq!(source.block(1), Some("Yesterday"));
        assert!(source.block(2).unwrap().contains("12:00  Captain Sprocket"));
        assert_eq!(source.block(4), Some("Today"));
        assert_eq!(client.source_messages[&2], (3, 0));
        let id = BlockId { object: 2, part: 0 };
        let retained = client.source_blocks[&id].1.clone();
        client.messages.pop_front();
        client.message_revisions.remove(&1);
        client.message_source().unwrap();
        assert!(Arc::ptr_eq(&retained, &client.source_blocks[&id].1));
        assert!(
            !client
                .source_blocks
                .contains_key(&BlockId { object: 1, part: 0 })
        );
        assert!(retained.text().contains("A clockwork pigeon!"));
    }
    #[test]
    fn every_avatar_fits_the_shared_inline_source_budget() {
        let mut client = Client::default();
        for avatar in 0..64 {
            let inline = client.avatar_inline(avatar).unwrap();
            assert!(inline.bytes() <= 128 * 1024);
        }
        assert_eq!(client.avatars.len(), 64);
        assert!(
            client.avatars.values().map(|i| i.bytes()).sum::<usize>()
                <= super::super::super::document::MAX_WINDOW_INLINE_BYTES
        );
    }
    #[test]
    fn roster_rename_leaves_loaded_sent_profiles_immutable() {
        let mut client = Client::default();
        let original = message(1, 20);
        let mut other = message(2, 20);
        other.sender.id = "b".repeat(32);
        // Names alone never identify an author.
        client.messages.extend([original.clone(), other.clone()]);
        let mut profile = original.sender.clone();
        profile.name = "Admiral Newname".into();
        profile.avatar = 42;
        let members = vec![Member {
            profile: profile.clone(),
            generation: 1,
            admin: false,
        }];
        client.reset = false;
        client
            .receive(
                Event::Members {
                    members: members.clone(),
                },
                false,
            )
            .unwrap();
        assert!(!client.reset);
        assert_eq!(client.messages[0].sender, original.sender);
        assert_eq!(client.messages[0].sequence, original.sequence);
        assert_eq!(client.messages[0].text, original.text);
        assert_eq!(client.messages[1], other);
        client.reset = false;
        client.receive(Event::Members { members }, false).unwrap();
        assert!(!client.reset, "unchanged rosters do not rebuild text");
    }
    #[test]
    fn new_sends_refresh_epoch_and_pending_retries_keep_original_nonce() {
        let mut client = Client {
            epoch: 1,
            wall: 10 * 600_000,
            online: true,
            ..Client::default()
        };
        client.focus = Some(Focus::Compose);
        client
            .edit(&Edit::Commit("First line\nsecond line".into()))
            .unwrap();
        client.send().unwrap();
        let sent = decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap();
        assert!(matches!(&sent, Command::Send {epoch:10,text,..} if text.contains('\n')));
        client.disconnected();
        client.online = true;
        let retry = decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap();
        assert_eq!(sent, retry);
    }
}

#[cfg(test)]
mod anchor_tests {
    use super::*;
    #[test]
    fn browser_scroll_drives_spark_anchor_without_a_second_follow_animation() {
        let mut c = Client {
            focus: Some(Focus::Compose),
            ..Client::default()
        };
        c.compose
            .replace(&"Clockwork text in a long browser composer. ".repeat(20));
        c.prepare(
            [800., 1280.],
            false,
            Some(std::sync::Arc::new(crate::dashboard::font::test_font())),
        );
        let cursor = c.compose_layout.lines[2].start;
        c.compose.place_cursor(cursor);
        c.host_scroll(0.);
        let before = c.spark_origin(&Spark::Compose(cursor)).unwrap();
        assert!(c.host_scroll(c.line_height()));
        let after = c.spark_origin(&Spark::Compose(cursor)).unwrap();
        assert!((before[1] - after[1] - c.line_height()).abs() < 0.01);
        assert!(!c.input_view.animating());
        assert!(!c.host_scroll(f32::NAN));
        c.host_scroll(f32::MAX);
        assert_eq!(c.input_view.offset, c.input_view.limit());
        c.focus = None;
        assert!(!c.host_scroll(0.));
    }
    #[test]
    fn replacement_sparks_follow_wrapped_local_and_remote_source_positions() {
        let font = std::sync::Arc::new(crate::dashboard::font::test_font());
        let member = Member {
            profile: Profile {
                id: "1".repeat(32),
                name: "Clockwork".into(),
                avatar: 1,
            },
            generation: 1,
            admin: false,
        };
        let mut c = Client {
            members: vec![member.clone()],
            focus: Some(Focus::Compose),
            ..Client::default()
        };
        c.prepare([800., 1280.], false, Some(font.clone()));
        c.tick(1000, 1_700_000_000_000);
        c.edit(&Edit::Commit(
            "A brass dispatch with room to replace words.".into(),
        ))
        .unwrap();
        c.tick(1100, 1_700_000_000_000);
        let local = c.spark_origin(&Spark::Compose(c.compose.cursor())).unwrap();
        assert_eq!(c.particles.last().unwrap().origin, local);
        assert!(local[0] > c.input_left() + 32.);
        c.edit(&Edit::SelectAll).unwrap();
        let count = c.particles.len();
        c.tick(1150, 1_700_000_000_000);
        assert_eq!(c.particles.len(), count);
        c.edit(&Edit::Commit(
            "An exceedingly long clockwork dispatch. ".repeat(20),
        ))
        .unwrap();
        for now in (1200..1600).step_by(20) {
            c.tick(now, 1_700_000_000_000);
        }
        assert!(c.compose_layout.lines.len() > 4);
        assert!(c.input_height() <= c.line_height() * 4. + 24.);
        assert!(
            c.spark_origin(&Spark::Compose(c.compose.cursor()))
                .is_some()
        );
        c.particles.clear();
        let base = crate::dashboard::scene::Scene::new(crate::dashboard::components::canvas(
            1,
            otrios_interfaces::scene::Rect::new(0., 0., 800., 1280.),
            vec![],
        ))
        .unwrap();
        let composer_id = |c: &Client| {
            c.scene(&base, [800., 1280.], false)
                .unwrap()
                .controls()
                .into_iter()
                .find(|n| {
                    n.node
                        .semantics
                        .as_ref()
                        .is_some_and(|s| s.label == "Message composer")
                })
                .unwrap()
                .node
                .id
        };
        let control = composer_id(&c);
        for (revision, text) in [
            (1, "A live draft with an unchanged tail"),
            (2, "A live REPLACEMENT with an unchanged tail"),
        ] {
            c.receive(
                Event::Draft {
                    participant: member.profile.id.clone(),
                    generation: 1,
                    revision,
                    text: text.into(),
                    expires: 1_700_000_003_000,
                },
                false,
            )
            .unwrap();
            c.tick(1700 + revision * 100, 1_700_000_000_000);
        }
        let expected = c
            .spark_origin(&Spark::Draft(member.profile.id.clone(), 18))
            .unwrap();
        assert_eq!(c.drafts[&member.profile.id].caret, 18);
        assert_eq!(c.particles.last().unwrap().origin, expected);
        let count = c.particles.len();
        c.receive(
            Event::Draft {
                participant: member.profile.id.clone(),
                generation: 1,
                revision: 3,
                text: "A live REPLACEMENT with an unchanged tail".into(),
                expires: 1_700_000_003_000,
            },
            false,
        )
        .unwrap();
        c.tick(2000, 1_700_000_000_000);
        assert_eq!(composer_id(&c), control);
        c.receive(
            Event::Draft {
                participant: member.profile.id.clone(),
                generation: 1,
                revision: 4,
                text: "A long live draft that wraps across multiple lines. ".repeat(4),
                expires: 1_700_000_003_000,
            },
            false,
        )
        .unwrap();
        assert!(c.draft_height() > 72.);
        assert_eq!(
            composer_id(&c),
            control,
            "Wrapped preview rows must not renumber the composer control"
        );
        assert_eq!(
            c.particles.len(),
            count,
            "Lease renewal must not emit a typing spark"
        );
    }
}
