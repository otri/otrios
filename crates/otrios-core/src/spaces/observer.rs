// SPDX-License-Identifier: LGPL-3.0-only
//! RAM-only host observation and bounded management intents. Never replays a
//! management action after reconnect: a lost outcome requires a fresh snapshot.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Decided {
        id: u32,
        status: ManagementStatus,
        created: Option<String>,
    },
    Unknown {
        id: u32,
    },
}
#[derive(Default)]
pub struct Observer {
    enabled: bool,
    selected: Option<String>,
    watched: bool,
    fresh: bool,
    unavailable: bool,
    snapshot: Option<Discovery>,
    cursor: Option<DiscoveryCursor>,
    next_id: u32,
    pending: Option<u32>,
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
        if let Some(id) = self.pending.take() {
            self.outcome = Some(Outcome::Unknown { id });
        }
    }
    pub fn snapshot(&self) -> Option<&Discovery> {
        self.snapshot.as_ref()
    }
    pub fn unavailable(&self) -> bool {
        self.unavailable
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
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
        if self.pending.is_some() {
            return Err(Error::Capacity);
        }
        if snapshot.identity.is_none() {
            return Err(Error::Denied);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
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
            let command = Command::Watch {
                version: VERSION,
                selected: self.selected.clone(),
                cursor: self.cursor.clone(),
            };
            let bytes = encode_command(&command)?;
            self.watched = true;
            return Ok(Some(bytes));
        }
        self.queued
            .take()
            .map(|command| encode_command(&command))
            .transpose()
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<bool> {
        if !self.enabled || !self.watched {
            return Err(Error::Invalid);
        }
        match decode_event(bytes)? {
            Event::Snapshot { snapshot, .. } => {
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
                if self.pending != Some(id) {
                    return Ok(false);
                }
                self.pending = None;
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
