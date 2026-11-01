// SPDX-License-Identifier: LGPL-3.0-only
//! Trusted mount-local publication review. The source mount stays admitted while
//! choosing a destination, so revocation clears the preview before disposal ACK.
use super::*;
use otrios_interfaces::spaces::{Discovery, SpaceSummary};
#[derive(Default)]
pub(crate) struct Sharing {
    pub(crate) source_name: String,
    pub(crate) selected: Option<InitialMessage>,
    pub(crate) targets: Vec<SpaceSummary>,
    pub(crate) target: Option<SpaceSummary>,
    pub(crate) generation: u64,
    pub(crate) page: usize,
    pub(crate) target_page: usize,
    pub(crate) operations: VecDeque<PendingOperation>,
    pub(crate) download: Option<(String, u64, [u8; 32], u8)>,
    pub(crate) upload: bool,
}
pub(crate) struct PendingOperation {
    pub(crate) command: Command,
    pub(crate) sent: bool,
}
impl PendingOperation {
    pub(crate) fn nonce(&self) -> Option<&str> {
        match &self.command {
            Command::Publish { nonce, .. }
            | Command::SendAsset { nonce, .. }
            | Command::Redact { nonce, .. } => Some(nonce),
            _ => None,
        }
    }
}
impl Sharing {
    pub(crate) fn clear_preview(&mut self) {
        self.selected = None;
        self.target = None;
        self.download = None;
        self.upload = false;
    }
}
impl Client {
    pub(crate) fn publication_targets(&mut self, snapshot: &Discovery) {
        let targets = snapshot
            .spaces
            .iter()
            .filter(|space| {
                space.role.is_some()
                    && snapshot
                        .selected
                        .as_ref()
                        .is_none_or(|source| source.space != space.id)
            })
            .cloned()
            .collect::<Vec<_>>();
        if (self.sharing.generation != snapshot.generation || self.sharing.targets != targets)
            && matches!(self.panel, Panel::Destinations | Panel::Publish)
        {
            self.sharing.target = None;
            self.panel = Panel::Dispatch;
            self.say("Space access changed; review the destination again");
        }
        self.sharing.source_name = snapshot
            .selected
            .as_ref()
            .and_then(|selected| {
                snapshot
                    .spaces
                    .iter()
                    .find(|space| space.id == selected.space)
            })
            .map_or_else(String::new, |space| {
                format!("{} · {}", &space.id[..8], space.name)
            });
        self.sharing.targets = targets;
        self.sharing.generation = snapshot.generation;
    }
    pub(crate) fn publication_unavailable(&mut self) {
        self.sharing.clear_preview();
        self.sharing.targets.clear();
        self.sharing.source_name.clear();
        self.sharing.generation = 0;
        if matches!(
            self.panel,
            Panel::Dispatches
                | Panel::Dispatch
                | Panel::Destinations
                | Panel::Publish
                | Panel::Redact
                | Panel::Assets
        ) {
            self.panel = Panel::None;
        }
    }
    pub(super) fn operation_reply(&mut self, nonce: &str) -> bool {
        let before = self.sharing.operations.len();
        self.sharing
            .operations
            .retain(|operation| operation.nonce() != Some(nonce));
        before != self.sharing.operations.len()
    }
    pub(super) fn operation_outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(operation) = self
            .sharing
            .operations
            .iter_mut()
            .find(|operation| !operation.sent)
        {
            operation.sent = true;
            return encode_command(&operation.command).map(Some);
        }
        Ok(None)
    }
    pub(crate) fn selection_current(&mut self) {
        if let Some(selected) = &self.sharing.selected
            && self.message_revisions.get(&selected.message.sequence) != Some(&selected.revision)
        {
            self.sharing.clear_preview();
            self.panel = Panel::None;
            self.say("The selected dispatch changed; select it again");
        }
    }
    pub(crate) fn take_download(&mut self) -> Option<(Asset, Vec<u8>)> {
        self.download.take()
    }
    pub(crate) fn take_upload_request(&mut self) -> bool {
        std::mem::take(&mut self.sharing.upload)
    }
    pub(crate) fn upload(&mut self, name: &str, bytes: &[u8]) -> Result<bool> {
        if !self.managed || self.read_only || !self.online || self.you.is_none() {
            return Ok(false);
        }
        if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
            return Err("Attachment byte limit".into());
        }
        if self.sharing.operations.len() >= 2
            || self
                .sharing
                .operations
                .iter()
                .any(|op| matches!(op.command, Command::SendAsset { .. }))
        {
            self.say("Wait for the pending operation");
            return Ok(true);
        }
        let asset = Asset {
            name: name.into(),
            bytes: bytes.len().try_into().map_err(err)?,
            sha256: super::super::super::hex(&super::super::super::hash(bytes)),
        };
        super::super::validate_asset(&asset)?;
        self.sharing.operations.push_back(PendingOperation {
            sent: false,
            command: Command::SendAsset {
                epoch: (self.wall / 600_000).max(self.epoch),
                nonce: random(16)?,
                text: self.compose.text().into(),
                name: name.into(),
                bytes: bytes.to_vec(),
            },
        });
        self.compose.replace("");
        self.focus = None;
        self.draft_dirty = true;
        self.say("Attachment pending...");
        Ok(true)
    }
    pub(super) fn sharing_action(&mut self, command: u16) -> Result<Option<Action>> {
        match command {
            200..=205 if self.panel == Panel::Assets => {
                if self.sharing.download.is_some() {
                    self.say("Wait for the requested attachment");
                    return Ok(Some(Action::Changed));
                }
                if let Some(selected) = &self.sharing.selected
                    && selected
                        .message
                        .assets
                        .get((command - 200) as usize)
                        .is_some()
                {
                    let request = (
                        random(16)?,
                        selected.message.sequence,
                        selected.revision,
                        (command - 200) as u8,
                    );
                    self.sharing.download = Some(request.clone());
                    self.queue(Command::ReadAsset {
                        nonce: request.0,
                        sequence: request.1,
                        revision: request.2,
                        index: request.3,
                    });
                    self.say("Attachment requested...");
                }
            }
            200..=205 if self.panel == Panel::Dispatches => {
                let index = self.sharing.page * 6 + (command - 200) as usize;
                self.sharing.selected = self.messages.iter().rev().nth(index).and_then(|message| {
                    self.message_revisions
                        .get(&message.sequence)
                        .map(|revision| InitialMessage {
                            message: message.clone(),
                            revision: *revision,
                        })
                });
                self.sharing.target = None;
                self.panel = if self.sharing.selected.is_some() {
                    Panel::Dispatch
                } else {
                    Panel::Dispatches
                };
            }
            206 if self.panel == Panel::Dispatches => {
                self.sharing.page = self.sharing.page.saturating_sub(1)
            }
            207 if self.panel == Panel::Dispatches => {
                self.sharing.page =
                    (self.sharing.page + 1).min(self.messages.len().saturating_sub(1) / 6)
            }
            200..=207 => return Ok(Some(Action::Changed)),
            208 => {
                self.sharing.clear_preview();
                self.sharing.page = 0;
                self.panel = Panel::Dispatches;
                self.focus = None;
            }
            209 if self.panel == Panel::Dispatch => {
                self.panel = Panel::Destinations;
                self.sharing.target_page = 0;
            }
            100..=167 if self.panel == Panel::Destinations => {
                self.sharing.target = self
                    .sharing
                    .targets
                    .get(self.sharing.target_page * 6 + (command - 100) as usize)
                    .cloned();
                if self.sharing.target.is_some() {
                    self.panel = Panel::Publish;
                }
            }
            190 if self.panel == Panel::Destinations => {
                self.sharing.target_page = self.sharing.target_page.saturating_sub(1)
            }
            191 if self.panel == Panel::Destinations => {
                self.sharing.target_page = (self.sharing.target_page + 1)
                    .min(self.sharing.targets.len().saturating_sub(1) / 6)
            }
            210 if self.panel == Panel::Publish => {
                if self.sharing.operations.len() >= 2 {
                    self.say("Wait for the pending operation");
                    return Ok(Some(Action::Changed));
                }
                if let (Some(source), Some(target)) = (&self.sharing.selected, &self.sharing.target)
                    && self.sharing.targets.iter().any(|current| current == target)
                    && self.message_revisions.get(&source.message.sequence)
                        == Some(&source.revision)
                {
                    self.sharing.operations.push_back(PendingOperation {
                        sent: false,
                        command: Command::Publish {
                            epoch: (self.wall / 600_000).max(self.epoch),
                            nonce: random(16)?,
                            sequence: source.message.sequence,
                            revision: source.revision,
                            destination_space: target.id.clone(),
                            expected_generation: self.sharing.generation,
                        },
                    });
                    self.sharing.clear_preview();
                    self.panel = Panel::None;
                    self.say("Publication pending...");
                } else {
                    self.say("Review the selection and destination again");
                }
            }
            209 | 210 => return Ok(Some(Action::Changed)),
            211 if self.panel == Panel::Dispatch
                && self.you.as_ref().is_some_and(|you| you.admin) =>
            {
                self.panel = Panel::Redact
            }
            212 if self.panel == Panel::Redact
                && self.you.as_ref().is_some_and(|you| you.admin) =>
            {
                if let Some(selected) = &self.sharing.selected {
                    if self.sharing.operations.len() >= 2 {
                        self.say("Wait for the pending operation");
                        return Ok(Some(Action::Changed));
                    }
                    self.sharing.operations.push_back(PendingOperation {
                        sent: false,
                        command: Command::Redact {
                            nonce: random(16)?,
                            sequence: selected.message.sequence,
                            revision: selected.revision,
                        },
                    });
                    self.sharing.clear_preview();
                    self.panel = Panel::None;
                    self.say("Deletion requested...");
                }
            }
            211 | 212 => return Ok(Some(Action::Changed)),
            213 if !self.native && self.panel == Panel::Dispatch => self.panel = Panel::Assets,
            214 if self.panel == Panel::Help && self.managed && !self.native && !self.read_only => {
                self.sharing.upload = true;
                self.panel = Panel::None;
                self.focus = None;
            }
            213 | 214 => return Ok(Some(Action::Changed)),
            _ => return Ok(None),
        }
        Ok(Some(Action::Changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otrios_interfaces::spaces::{
        Audience, Discovery, DiscoveryCursor, Role, SpaceDetail, SpaceSummary,
    };
    const SOURCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fn snapshot() -> Discovery {
        Discovery {
            version: 1,
            generation: 1,
            cursor: DiscoveryCursor {
                authority: "c".repeat(32),
                revision: 1,
            },
            identity: Some("reader".into()),
            spaces: vec![
                SpaceSummary {
                    layout: Default::default(),
                    apps: vec![],
                    attached_count: 0,
                    id: SOURCE.into(),
                    name: "Private source".into(),
                    audience: Audience::PublicView,
                    role: None,
                },
                SpaceSummary {
                    layout: Default::default(),
                    apps: vec![],
                    attached_count: 0,
                    id: "b".repeat(32),
                    name: "Destination".into(),
                    audience: Audience::PublicView,
                    role: Some(Role::Member),
                },
            ],
            selected: Some(SpaceDetail {
                space: SOURCE.into(),
                apps: vec![],
                installed: vec![],
                members: None,
                cleanup: None,
            }),
        }
    }
    fn client(read_only: bool) -> Client {
        let mut client = Client::default();
        client.managed(Some(255));
        client.access(!read_only);
        client.wall = 1_000_000;
        let asset = Asset {
            name: "private.bin".into(),
            bytes: 4,
            sha256: super::super::super::super::hex(&super::super::super::super::hash(&[
                0, 1, 255, 42,
            ])),
        };
        client
            .receive_initial(
                &flexbuffers::to_vec(Initial {
                    version: 1,
                    members: vec![],
                    older: false,
                    messages: vec![InitialMessage {
                        revision: [1; 32],
                        message: Message {
                            assets: vec![asset],
                            sequence: 1,
                            at: 1_000_000,
                            sender: Profile {
                                id: "d".repeat(32),
                                name: "Private author".into(),
                                avatar: 1,
                            },
                            kind: Kind::Message,
                            text: "Confirmed private selection".into(),
                            nonce: "e".repeat(32),
                        },
                    }],
                })
                .unwrap(),
            )
            .unwrap();
        if !read_only {
            client.online = true;
            client.you = Some(Member {
                profile: Profile {
                    id: "f".repeat(32),
                    name: "Member".into(),
                    avatar: 2,
                },
                generation: 1,
                admin: true,
            });
        }
        client.publication_targets(&snapshot());
        client
    }
    fn select(client: &mut Client) {
        for command in [208, 200] {
            client.action(command).unwrap();
        }
    }
    #[test]
    fn sharing_dialog_pointer_targets_remain_reachable_in_short_and_portrait_views() {
        use crate::dashboard::{components::canvas, input::InputRouter, scene::Scene};
        use otrios_interfaces::scene::{Action, PointerEvent, PointerPhase, Rect};
        use std::sync::Arc;
        for size in [[800., 505.], [534., 900.], [1280., 800.]] {
            let mut client = client(false);
            client.viewport = size;
            for command in [208, 200, 209, 100] {
                client.action(command).unwrap();
            }
            let selected = client.sharing.selected.as_mut().unwrap();
            selected.message.assets = (0..4)
                .map(|index| {
                    let mut asset = selected.message.assets[0].clone();
                    asset.name = format!("attachment-{index}.bin");
                    asset
                })
                .collect();
            let message = selected.message.clone();
            client.messages = (1..=6)
                .map(|sequence| {
                    let mut message = message.clone();
                    message.sequence = sequence;
                    message
                })
                .collect();
            client.sharing.targets = (0..6)
                .map(|index| {
                    let mut target = client.sharing.targets[0].clone();
                    target.id = format!("{index:032x}");
                    target.name = format!("Destination {index}");
                    target
                })
                .collect();
            let base = Scene::new(canvas(1, Rect::new(0., 0., size[0], size[1]), vec![])).unwrap();
            for panel in [
                Panel::Help,
                Panel::Dispatches,
                Panel::Dispatch,
                Panel::Destinations,
                Panel::Publish,
                Panel::Redact,
                Panel::Assets,
            ] {
                client.panel = panel;
                let scene = Arc::new(client.scene(&base, size, false).unwrap());
                for control in scene
                    .controls()
                    .into_iter()
                    .filter(|c| c.node.id >= 3_000_000 + 100 * 2048 && c.node.id < 4_000_000)
                {
                    let position = [
                        control.origin[0] + control.node.frame.width / 2.,
                        control.origin[1] + control.node.frame.height / 2.,
                    ];
                    assert!(
                        position[0] < size[0] && position[1] < size[1],
                        "offscreen: {:?} {:?}",
                        panel,
                        control.node.semantics
                    );
                    let mut input = InputRouter::default();
                    for phase in [PointerPhase::Down, PointerPhase::Up] {
                        let result = input.dispatch(
                            scene.clone(),
                            PointerEvent {
                                id: 1,
                                phase,
                                position,
                            },
                        );
                        if phase == PointerPhase::Up {
                            let expected = match control.node.responder {
                                Some(otrios_interfaces::scene::TouchResponder::Activate(
                                    Action::AppEvent(id),
                                )) => Some(Action::AppEvent(id)),
                                _ => None,
                            };
                            assert_eq!(
                                result.action, expected,
                                "covered: {:?} {:?} {:?}",
                                size, panel, control.node.semantics
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn publication_from_read_admission_requires_review_and_a_matching_receipt_without_joining() {
        let mut client = client(true);
        select(&mut client);
        for command in [209, 100, 210] {
            client.action(command).unwrap();
        }
        assert!(!client.confirmed());
        let Command::Publish {
            nonce,
            sequence,
            revision,
            destination_space,
            expected_generation,
            ..
        } = decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap()
        else {
            panic!("publication");
        };
        assert_eq!(sequence, 1);
        assert_eq!(revision, [1; 32]);
        assert_eq!(destination_space, "b".repeat(32));
        assert_eq!(expected_generation, 1);
        assert!(client.outgoing(None).unwrap().is_none());
        client
            .receive(
                Event::Published {
                    nonce: "0".repeat(32),
                    sequence: 1,
                },
                false,
            )
            .unwrap();
        assert!(!client.confirmed());
        client
            .receive(Event::Published { nonce, sequence: 1 }, false)
            .unwrap();
        assert!(client.confirmed());
        assert!(client.you.is_none());
        assert_eq!(client.messages[0].text, "Confirmed private selection");
    }
    #[test]
    fn changed_destination_and_unavailable_discovery_clear_the_private_publication_preview() {
        let mut client = client(true);
        select(&mut client);
        for command in [209, 100] {
            client.action(command).unwrap();
        }
        assert_eq!(client.panel, Panel::Publish);
        let mut changed = snapshot();
        changed.generation += 1;
        changed.spaces[1].role = None;
        client.publication_targets(&changed);
        client.action(210).unwrap();
        assert!(client.outgoing(None).unwrap().is_none());
        assert!(client.confirmed());
        client.publication_unavailable();
        assert!(client.sharing.selected.is_none());
        assert!(client.sharing.target.is_none());
        assert!(client.sharing.targets.is_empty());
        assert_eq!(client.panel, Panel::None);
    }
    #[test]
    fn uploads_and_moderation_remain_unconfirmed_until_their_own_replies() {
        let mut client = client(false);
        client.upload("file.bin", &[0, 1, 255, 42]).unwrap();
        assert!(!client.confirmed());
        let Command::SendAsset { nonce, bytes, .. } =
            decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap()
        else {
            panic!("upload");
        };
        assert_eq!(bytes, [0, 1, 255, 42]);
        client
            .receive(
                Event::Receipt {
                    nonce: "0".repeat(32),
                    sequence: 2,
                },
                false,
            )
            .unwrap();
        assert!(!client.confirmed());
        client
            .receive(Event::Receipt { nonce, sequence: 2 }, false)
            .unwrap();
        assert!(client.confirmed());
        select(&mut client);
        client.action(211).unwrap();
        client.action(212).unwrap();
        assert!(!client.confirmed());
        let Command::Redact {
            nonce, sequence, ..
        } = decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap()
        else {
            panic!("moderation");
        };
        assert_eq!(sequence, 1);
        client
            .receive(
                Event::Error {
                    nonce: Some(nonce),
                    message: "Reviewed row changed".into(),
                },
                false,
            )
            .unwrap();
        assert!(client.confirmed());
        assert_eq!(client.messages.len(), 1);
    }
    #[test]
    fn attachment_downloads_require_an_exact_live_user_request_and_replays_are_discarded() {
        let mut client = client(true);
        let asset = client.messages[0].assets[0].clone();
        let reply = |nonce| Event::Asset {
            nonce,
            sequence: 1,
            revision: [1; 32],
            asset: asset.clone(),
            bytes: vec![0, 1, 255, 42],
        };
        client.receive(reply("0".repeat(32)), false).unwrap();
        assert!(client.take_download().is_none());
        select(&mut client);
        client.action(213).unwrap();
        client.action(200).unwrap();
        let Command::ReadAsset { nonce, .. } =
            decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap()
        else {
            panic!("read");
        };
        client.receive(reply("0".repeat(32)), false).unwrap();
        assert!(client.take_download().is_none());
        client.receive(reply(nonce.clone()), false).unwrap();
        assert_eq!(client.take_download().unwrap().1, [0, 1, 255, 42]);
        client.receive(reply(nonce), false).unwrap();
        assert!(client.take_download().is_none());
        client.action(200).unwrap();
        let Command::ReadAsset { nonce, .. } =
            decode_command(&client.outgoing(None).unwrap().unwrap()).unwrap()
        else {
            panic!("second read");
        };
        client.message_revisions.clear();
        client.selection_current();
        client.receive(reply(nonce), false).unwrap();
        assert!(client.take_download().is_none());
    }
}
