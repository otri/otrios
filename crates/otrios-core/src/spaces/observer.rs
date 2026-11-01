// SPDX-License-Identifier: LGPL-3.0-only
//! RAM-only host observation and bounded management intents. Never replays a
//! management action after reconnect: a lost outcome requires a fresh snapshot.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    MoveStatus {
        id: u32,
        operation: String,
        status: ManagementStatus,
        generation: Option<u64>,
    },
    Decided {
        id: u32,
        status: ManagementStatus,
        created: Option<String>,
    },
    Unknown {
        id: u32,
    },
}
#[derive(Clone)]
struct MoveWatch {
    actor: String,
    operation: String,
    source: String,
    destination: String,
}
#[derive(Default)]
pub struct Observer {
    enabled: bool,
    selected: Option<String>,
    watched: bool,
    observation: u32,
    fresh: bool,
    unavailable: bool,
    snapshot: Option<Discovery>,
    cursor: Option<DiscoveryCursor>,
    next_id: u32,
    pending: Option<u32>,
    move_watch: Option<MoveWatch>,
    checking_move: bool,
    move_query_blocked: bool,
    queued: Option<Command>,
    outcome: Option<Outcome>,
}
impl Observer {
    pub fn configure(&mut self, selected: Option<&str>) -> Result<()> {
        if selected.is_some_and(|id| !valid_id(id)) {
            return Err(Error::Invalid);
        }
        self.enabled = true;
        if self.selected.as_deref() != selected {
            self.snapshot = None;
            self.cursor = None;
        }
        self.selected = selected.map(str::to_owned);
        self.reset();
        Ok(())
    }
    pub fn reset(&mut self) {
        self.watched = false;
        self.fresh = false;
        self.unavailable = false;
        self.queued = None;
        self.checking_move = false;
        self.move_query_blocked = false;
        if let Some(id) = self.pending.take() {
            self.outcome = Some(Outcome::Unknown { id });
        }
    }
    pub fn forget(&mut self) {
        self.snapshot = None;
        self.cursor = None;
        self.reset();
    }
    pub fn snapshot(&self) -> Option<&Discovery> {
        self.snapshot.as_ref()
    }
    pub fn unavailable(&self) -> bool {
        self.unavailable
    }
    pub fn awaiting_snapshot(&self) -> bool {
        self.enabled && !self.unavailable && self.snapshot.is_none()
    }
    pub fn pending(&self) -> bool {
        // Disconnected hosts may replace their runtime. While connected, wait
        // for the read-only outcome check before handing off the release.
        self.pending.is_some() || self.fresh && self.move_watch.is_some()
    }
    pub fn has_outcome(&self) -> bool {
        self.outcome.is_some()
    }
    pub fn take_outcome(&mut self) -> Option<Outcome> {
        self.outcome.take()
    }
    pub fn request(&mut self, change: Change) -> Result<u32> {
        let snapshot = self
            .snapshot
            .as_ref()
            .filter(|_| self.fresh && !self.unavailable)
            .ok_or(Error::Unavailable)?;
        if self.pending.is_some() || self.move_watch.is_some() {
            return Err(Error::Capacity);
        }
        if snapshot.identity.is_none() {
            return Err(Error::Denied);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let move_watch = if let Change::Move {
            operation,
            source,
            destination,
            ..
        } = &change
        {
            Some(MoveWatch {
                actor: snapshot.identity.clone().ok_or(Error::Denied)?,
                operation: operation.clone(),
                source: source.clone(),
                destination: destination.clone(),
            })
        } else {
            None
        };
        let command = Command::Manage {
            version: VERSION,
            id,
            request: Request {
                version: VERSION,
                expected_generation: snapshot.generation,
                change,
            },
        };
        encode_command(&command)?;
        self.next_id = id;
        self.move_watch = move_watch;
        self.pending = Some(id);
        self.queued = Some(command);
        self.outcome = None;
        Ok(id)
    }
    pub fn outgoing(&mut self) -> Result<Option<Vec<u8>>> {
        if !self.enabled || self.unavailable {
            return Ok(None);
        }
        if !self.watched {
            self.observation = self.observation.checked_add(1).ok_or(Error::Capacity)?;
            let command = Command::Watch {
                version: VERSION,
                observation: self.observation,
                selected: self.selected.clone(),
                cursor: self.cursor.clone(),
            };
            let bytes = encode_command(&command)?;
            self.watched = true;
            return Ok(Some(bytes));
        }
        if let Some(command) = self.queued.take() {
            return encode_command(&command).map(Some);
        }
        if self.pending.is_none()
            && self.fresh
            && !self.move_query_blocked
            && let Some(watch) = &self.move_watch
        {
            let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
            let command = Command::MoveStatus {
                version: VERSION,
                id,
                operation: watch.operation.clone(),
                source: watch.source.clone(),
                destination: watch.destination.clone(),
            };
            let bytes = encode_command(&command)?;
            self.next_id = id;
            self.pending = Some(id);
            self.checking_move = true;
            return Ok(Some(bytes));
        }
        Ok(None)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool> {
        if !self.enabled {
            return Err(Error::Invalid);
        }
        let event = decode_event(bytes)?;
        if !self.watched {
            return Ok(false);
        }
        match event {
            Event::MoveStatus {
                id,
                operation,
                status,
                generation,
                ..
            } => {
                if self.pending != Some(id)
                    || !self.checking_move
                    || self
                        .move_watch
                        .as_ref()
                        .is_none_or(|w| w.operation != operation)
                {
                    return Ok(false);
                }
                self.pending = None;
                self.checking_move = false;
                if status == ManagementStatus::Unavailable {
                    self.move_query_blocked = true;
                } else {
                    self.move_watch = None;
                }
                self.outcome = Some(Outcome::MoveStatus {
                    id,
                    operation,
                    status,
                    generation,
                });
                Ok(true)
            }
            Event::Snapshot {
                observation,
                snapshot,
                ..
            } => {
                let snapshot = *snapshot;
                if observation != self.observation {
                    return Ok(false);
                }
                if snapshot
                    .selected
                    .as_ref()
                    .is_some_and(|s| Some(s.space.as_str()) != self.selected.as_deref())
                {
                    return Err(Error::Invalid);
                }
                if self.cursor.as_ref().is_some_and(|cursor| {
                    cursor.authority == snapshot.cursor.authority
                        && cursor.revision > snapshot.cursor.revision
                }) {
                    return Ok(false);
                }
                if self
                    .move_watch
                    .as_ref()
                    .is_some_and(|w| snapshot.identity.as_deref() != Some(w.actor.as_str()))
                {
                    self.move_watch = None;
                    self.pending = None;
                    self.checking_move = false;
                    self.queued = None;
                    self.outcome = None;
                }
                let changed = self.snapshot.as_ref() != Some(&snapshot);
                self.cursor = Some(snapshot.cursor.clone());
                self.snapshot = Some(snapshot);
                self.fresh = true;
                self.unavailable = false;
                Ok(changed)
            }
            Event::Result {
                id,
                status,
                created,
                ..
            } => {
                if self.pending != Some(id) || self.checking_move {
                    return Ok(false);
                }
                self.pending = None;
                self.move_watch = None;
                self.outcome = Some(Outcome::Decided {
                    id,
                    status,
                    created,
                });
                Ok(true)
            }
            Event::Unavailable { .. } => {
                self.reset();
                self.watched = true;
                self.snapshot = None;
                self.unavailable = true;
                Ok(true)
            }
            Event::ResyncRequired { .. } => {
                self.cursor = None;
                self.snapshot = None;
                self.reset();
                Ok(true)
            }
        }
    }
}
