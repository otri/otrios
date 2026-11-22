// SPDX-License-Identifier: LGPL-3.0-only
//! Local playback and bounded transcript controls; never creates room commands.
use super::*;
use crate::dashboard::{components::canvas, input::InputRouter, scene::Scene};
use otrios_interfaces::scene::{
    Action, PointerEvent, PointerPhase, Rect, SwitchSemantics, TouchResponder,
};
use std::collections::{BTreeMap, VecDeque};
const BASE: u32 = 3_600_000_000;
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Control {
    pub id: u32,
    pub sequence: u64,
    pub revision: [u8; 32],
    pub label: String,
    pub bounds: [f32; 4],
}
struct Playing {
    sequence: u64,
    event: MessageEvent,
    at: u64,
}
struct Touch {
    id: u32,
    sequence: u64,
    revision: [u8; 32],
    start: [f32; 2],
    moved: bool,
}
#[derive(Default)]
pub(super) struct Replay {
    playing: VecDeque<Playing>,
    last_live: u64,
    last_replay: Option<u64>,
    next_id: u32,
    ids: BTreeMap<u64, u32>,
    controls: Vec<Control>,
    avatars: BTreeMap<String, [f32; 2]>,
    clip: [f32; 4],
    revision: u64,
    touch: Option<Touch>,
}
fn title(event: &MessageEvent) -> &'static str {
    match event {
        MessageEvent::Steam { .. } => "steam",
        MessageEvent::Boop { .. } => "boop",
        MessageEvent::Salute { .. } => "salute",
        MessageEvent::Dice { .. } => "dice roll",
    }
}
fn inside(bounds: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= bounds[0]
        && p[1] >= bounds[1]
        && p[0] < bounds[0] + bounds[2]
        && p[1] < bounds[1] + bounds[3]
}
impl Replay {
    pub fn status(&self) -> (usize, u64) {
        (self.playing.len(), self.last_live)
    }
    pub fn stop(&mut self) {
        self.playing.clear();
    }
    pub fn tick(&mut self, now: u64) -> bool {
        let active = !self.playing.is_empty();
        self.playing
            .retain(|p| now.saturating_sub(p.at) < super::effects::duration(&p.event));
        active
    }
    pub fn live(&mut self, sequence: u64, event: MessageEvent, now: u64, enabled: bool) {
        if sequence <= self.last_live {
            return;
        }
        self.last_live = sequence;
        if enabled {
            self.play(sequence, event, now);
        }
    }
    fn play(&mut self, sequence: u64, event: MessageEvent, now: u64) {
        // A new throw replaces the previous throw, keeping the low-power host bounded.
        if matches!(event, MessageEvent::Dice { .. }) {
            self.playing
                .retain(|p| !matches!(p.event, MessageEvent::Dice { .. }));
        }
        if self.playing.len() >= 3 {
            self.playing.pop_front();
        }
        self.playing.push_back(Playing {
            sequence,
            event,
            at: now,
        });
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }
    pub fn scene(&self, viewport: [f32; 2]) -> Result<Scene> {
        let nodes = self
            .controls
            .iter()
            .map(|control| {
                let [x, y, w, h] = control.bounds;
                let mut node = canvas(control.id, Rect::new(x, y, w, h), vec![]);
                node.responder = Some(TouchResponder::Activate(Action::KeyboardKey(u16::MAX)));
                node.semantics = Some(SwitchSemantics {
                    label: control.label.clone(),
                    checked: false,
                });
                node
            })
            .collect();
        Scene::new(canvas(
            BASE,
            Rect::new(0., 0., viewport[0], viewport[1]),
            nodes,
        ))
    }
    fn sync(
        &mut self,
        messages: &VecDeque<Message>,
        revisions: &BTreeMap<u64, [u8; 32]>,
        positions: &BTreeMap<u64, (u32, u32)>,
        origins: &[(u32, [f32; 2])],
        clip: [f32; 4],
        line: f32,
        visible: bool,
    ) -> bool {
        self.clip = clip;
        self.ids
            .retain(|sequence, _| messages.iter().any(|m| m.sequence == *sequence));
        let mut controls = vec![];
        self.avatars.clear();
        if visible {
            for message in messages {
                let Some((block, _)) = positions.get(&message.sequence) else {
                    continue;
                };
                let Some((_, origin)) = origins.iter().find(|(id, _)| id == block) else {
                    continue;
                };
                if origin[1] >= clip[1] && origin[1] + 36. <= clip[3] {
                    self.avatars.insert(
                        message.sender.id.clone(),
                        [origin[0] + 28., origin[1] + 16.],
                    );
                }
                let Some(event) = &message.event else {
                    continue;
                };
                let x = (origin[0] + 4.).max(clip[0]);
                let y = (origin[1] + line + 4.).max(clip[1]);
                let right = clip[2] - 4.;
                let bottom = (origin[1] + line + 48.).min(clip[3]);
                if right <= x || bottom <= y {
                    continue;
                }
                let Some(revision) = revisions.get(&message.sequence) else {
                    continue;
                };
                let id = *self.ids.entry(message.sequence).or_insert_with(|| {
                    self.next_id = self
                        .next_id
                        .checked_add(1)
                        .filter(|id| *id < 100_000_000)
                        .unwrap_or(1);
                    BASE + self.next_id
                });
                let mut label = format!("Replay {} from {}", title(event), message.sender.name);
                while label.len() > 96 {
                    label.pop();
                }
                controls.push(Control {
                    id,
                    sequence: message.sequence,
                    revision: *revision,
                    label,
                    bounds: [x, y, right - x, bottom - y],
                });
            }
        }
        if self.controls == controls {
            return false;
        }
        self.controls = controls;
        self.revision = self.revision.wrapping_add(1);
        true
    }
    fn origin(&self, playing: &Playing) -> [f32; 2] {
        if let MessageEvent::Boop { target, .. } | MessageEvent::Salute { target, .. } =
            &playing.event
            && let Some(origin) = self.avatars.get(&target.id)
        {
            return *origin;
        }
        self.controls
            .iter()
            .find(|c| c.sequence == playing.sequence)
            .map_or(
                [
                    (self.clip[0] + self.clip[2]) * 0.5,
                    self.clip[1] + (self.clip[3] - self.clip[1]) * 0.75,
                ],
                |c| {
                    [
                        c.bounds[0] + c.bounds[2] * 0.5,
                        c.bounds[1] + c.bounds[3] * 0.5,
                    ]
                },
            )
    }
    pub fn model_frame(
        &self,
        now: u64,
        viewport: [f32; 2],
        quiet: bool,
        enabled: bool,
    ) -> Result<Option<crate::canvas::model::Frame>> {
        if !enabled
            || option_env!("OTRIOS_RELAY_EFFECTS_DISABLED") == Some("1")
            || self.clip[2] <= self.clip[0]
            || self.clip[3] <= self.clip[1]
        {
            return Ok(None);
        }
        let Some(playing) = self
            .playing
            .iter()
            .rev()
            .find(|p| matches!(p.event, MessageEvent::Salute { .. }))
        else {
            return Ok(None);
        };
        super::model_effects::hat_frame(now.saturating_sub(playing.at), viewport, quiet)
    }
    pub fn vertices(
        &self,
        now: u64,
        viewport: [f32; 2],
        dark: bool,
        quiet: bool,
        enabled: bool,
    ) -> Vec<f32> {
        let mut out = vec![];
        let color = if dark {
            [0.85, 0.72, 0.46, 0.9]
        } else {
            [0.20, 0.30, 0.24, 0.9]
        };
        // A compact replay triangle in the existing avatar gutter. Painting has
        // no responder; the separate semantic overlay routes through InputRouter.
        for control in &self.controls {
            let [x, y, w, h] = control.bounds;
            let cx = x + 22_f32.min(w * 0.5);
            let cy = y + h * 0.5;
            let s = 7_f32.min(w * 0.25).min(h * 0.25);
            for p in [[cx - s * 0.6, cy - s], [cx + s, cy], [cx - s * 0.6, cy + s]] {
                out.extend([p[0], p[1], color[0], color[1], color[2], color[3]]);
            }
        }
        if !enabled || option_env!("OTRIOS_RELAY_EFFECTS_DISABLED") == Some("1") {
            return out;
        }
        if self.clip[2] <= self.clip[0] || self.clip[3] <= self.clip[1] {
            return out;
        }
        for playing in &self.playing {
            let origin = self.origin(playing);
            let vertices = super::effects::vertices(
                &playing.event,
                now.saturating_sub(playing.at),
                viewport,
                origin,
                dark,
                quiet,
            );
            out.extend(vertices);
        }
        // Leave headroom for shared focus and short-lived typing sparks.
        out.truncate(out.len().min(60_000 * 6) / 18 * 18);
        out
    }
}
impl Client {
    pub(crate) fn sync_replays(&mut self, viewport: &super::super::document::Viewport) -> bool {
        self.replay.sync(
            &self.messages,
            &self.message_revisions,
            &self.source_messages,
            &viewport.visible_block_origins(),
            viewport.clip(),
            self.line_height(),
            self.panel == super::client::Panel::None && self.kicked.is_none(),
        )
    }
    pub(crate) fn replay_revision(&self) -> u64 {
        self.replay.revision()
    }
    pub(crate) fn replay_scene(&self) -> Result<Scene> {
        if self.replay_controls().is_empty() {
            Scene::new(canvas(
                BASE,
                Rect::new(0., 0., self.viewport[0], self.viewport[1]),
                vec![],
            ))
        } else {
            self.replay.scene(self.viewport)
        }
    }
    pub(crate) fn replay_controls(&self) -> &[Control] {
        if self.panel != super::client::Panel::None || self.kicked.is_some() {
            &[]
        } else {
            self.replay.controls()
        }
    }
    pub(crate) fn replay_activate(&mut self, id: u32) -> Result<bool> {
        let scene = self.replay_scene()?;
        if InputRouter::activate(&scene, id).action.is_none() {
            return Ok(false);
        }
        let Some(control) = self.replay.controls.iter().find(|c| c.id == id).cloned() else {
            return Ok(false);
        };
        if self.message_revisions.get(&control.sequence) != Some(&control.revision) {
            return Ok(false);
        }
        let Some(event) = self
            .messages
            .iter()
            .find(|m| m.sequence == control.sequence)
            .and_then(|m| m.event.clone())
        else {
            return Ok(false);
        };
        if self.effects == 0 {
            self.say("Effects are off in your calling card");
            return Ok(true);
        }
        if self
            .replay
            .last_replay
            .is_some_and(|at| self.now.saturating_sub(at) < 250)
        {
            return Ok(false);
        }
        self.replay.last_replay = Some(self.now);
        self.replay.play(control.sequence, event, self.now);
        Ok(true)
    }
    /// Observe the reader's gesture without taking its scrolling capture. A
    /// release must still address the same admitted message revision.
    pub(crate) fn replay_pointer(&mut self, event: PointerEvent) -> Result<bool> {
        if self.panel != super::client::Panel::None {
            self.replay.touch = None;
            return Ok(false);
        }
        if event.phase == PointerPhase::Down {
            self.replay.touch = self
                .replay
                .controls
                .iter()
                .find(|c| inside(c.bounds, event.position))
                .map(|c| Touch {
                    id: event.id,
                    sequence: c.sequence,
                    revision: c.revision,
                    start: event.position,
                    moved: false,
                });
        } else if let Some(touch) = &mut self.replay.touch {
            if touch.id != event.id {
                return Ok(false);
            }
            touch.moved |= (touch.start[0] - event.position[0]).abs() > 8.
                || (touch.start[1] - event.position[1]).abs() > 8.;
            if matches!(event.phase, PointerPhase::Up | PointerPhase::Cancel) {
                let touch = self.replay.touch.take().unwrap();
                if event.phase == PointerPhase::Up && !touch.moved {
                    let id = self
                        .replay
                        .controls
                        .iter()
                        .find(|c| {
                            c.sequence == touch.sequence
                                && c.revision == touch.revision
                                && inside(c.bounds, event.position)
                        })
                        .map(|c| c.id);
                    if let Some(id) = id {
                        return self.replay_activate(id);
                    }
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message() -> Message {
        Message {
            assets: vec![],
            sequence: 7,
            at: 1,
            sender: Profile {
                id: "a".repeat(32),
                name: "Ema".into(),
                avatar: 1,
            },
            kind: Kind::Notice,
            text: "Ema rolls 2d6: 3 + 6 = 9".into(),
            nonce: "b".repeat(32),
            event: Some(MessageEvent::Dice {
                seed: 42,
                sides: 6,
                rolls: vec![3, 6],
            }),
        }
    }
    #[test]
    fn history_stays_quiet_and_replay_is_local_revision_bound_and_drag_safe() {
        let mut client = Client::default();
        client.online = true;
        client.messages.push_back(message());
        client.message_revisions.insert(7, [1; 32]);
        client.source_messages.insert(7, (0, 0));
        client.now = 1000;
        let sync = |c: &mut Client| {
            c.replay.sync(
                &c.messages,
                &c.message_revisions,
                &c.source_messages,
                &[(0, [24., 120.])],
                [24., 100., 366., 500.],
                24.,
                true,
            )
        };
        assert!(sync(&mut client));
        assert_eq!(client.replay.status().0, 0, "history never autoplays");
        let control = client.replay_controls()[0].clone();
        let point = [control.bounds[0] + 10., control.bounds[1] + 10.];
        client
            .replay_pointer(PointerEvent {
                id: 1,
                phase: PointerPhase::Down,
                position: point,
            })
            .unwrap();
        client
            .replay_pointer(PointerEvent {
                id: 1,
                phase: PointerPhase::Move,
                position: [point[0], point[1] + 15.],
            })
            .unwrap();
        assert!(
            !client
                .replay_pointer(PointerEvent {
                    id: 1,
                    phase: PointerPhase::Up,
                    position: point
                })
                .unwrap()
        );
        assert_eq!(client.replay.status().0, 0, "scrolling is never a replay");
        assert!(client.replay_activate(control.id).unwrap());
        assert_eq!(client.replay.playing[0].event, message().event.unwrap());
        assert!(
            client.outgoing(None).unwrap().is_none(),
            "replay creates no room command"
        );
        client.now = 1500;
        client.message_revisions.insert(7, [2; 32]);
        assert!(
            !client.replay_activate(control.id).unwrap(),
            "stale revision cannot replay"
        );
        sync(&mut client);
        assert!(client.replay_activate(control.id).unwrap());
        client.quiet(true);
        assert_eq!(
            client.replay.status().0,
            0,
            "reduced motion clears playback"
        );
        client
            .replay
            .live(9, MessageEvent::Steam { seed: 1 }, 2000, false);
        assert_eq!(client.replay.status(), (0, 9));
        client.effects = 2;
        client
            .replay
            .live(9, MessageEvent::Steam { seed: 1 }, 3000, true);
        assert_eq!(
            client.replay.status().0,
            0,
            "a repeated live event never autoplays twice"
        );
    }
}
