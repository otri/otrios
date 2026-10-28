// SPDX-License-Identifier: LGPL-3.0-only
//! RAM-only consumer of one coherent collection window. The host supplies a
//! monotonic request counter shared across its app activations and rejects
//! network frames from prior connection epochs before calling this controller.
use super::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    Unchanged,
    View,
    Revoked,
    Expired(Option<u64>),
    Rejected(Status),
}
pub struct Client {
    collection: u64,
    schema: Schema,
    generation: u64,
    active_generation: u64,
    epoch: Option<u64>,
    replica: Option<Replica>,
    active_plan: Option<Selection>,
    wanted: Option<Selection>,
    pending: bool,
    awaiting: bool,
    control: Option<Command>,
}
impl Client {
    pub fn new(collection: u64, schema: Schema) -> Self {
        Self {
            collection,
            schema,
            generation: 0,
            active_generation: 0,
            epoch: None,
            replica: None,
            active_plan: None,
            wanted: None,
            pending: false,
            awaiting: false,
            control: None,
        }
    }
    pub fn plan(&self) -> Option<&Selection> {
        self.wanted.as_ref()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn waiting(&self) -> bool {
        self.pending || self.awaiting
    }
    pub fn window(&self) -> Option<&Arc<Window>> {
        self.replica.as_ref().and_then(Replica::window)
    }
    pub fn select(&mut self, plan: Selection) -> Result<()> {
        window::check_plan(self.collection, &self.schema, &plan)?;
        self.wanted = Some(plan);
        self.pending = true;
        self.control = None;
        Ok(())
    }
    pub fn disconnected(&mut self) {
        self.replica = None;
        self.active_plan = None;
        self.epoch = None;
        self.wanted = None;
        self.active_generation = 0;
        self.pending = false;
        self.awaiting = false;
        self.control = None;
    }
    pub fn stop(&mut self) {
        let generation = self.generation;
        self.disconnected();
        if generation != 0 {
            self.control = Some(Command::Cancel { generation });
        }
    }
    pub fn outgoing(&mut self, counter: &mut u64) -> Result<Option<Command>> {
        if self.pending {
            *counter = counter.checked_add(1).ok_or(Error::Limit)?;
            self.generation = *counter;
            let plan = self.wanted.as_ref().ok_or(Error::Invalid)?.clone();
            if let Some(replica) = &mut self.replica {
                replica.begin_request(self.generation, plan.clone())?;
            }
            self.pending = false;
            self.awaiting = true;
            return Ok(Some(Command::Select {
                generation: self.generation,
                plan,
            }));
        }
        Ok(self.control.take())
    }
    pub fn receive(&mut self, event: Event) -> Result<Change> {
        match event {
            Event::AnchorExpired {
                epoch,
                generation,
                collection,
                neighbor,
            } => {
                if self.wanted.is_none()
                    || generation != self.generation
                    || collection != self.collection
                    || self.pending
                {
                    return Ok(Change::Unchanged);
                }
                if self.epoch.is_some_and(|current| current != epoch) {
                    return Ok(Change::Unchanged);
                }
                self.epoch = Some(epoch);
                self.awaiting = false;
                Ok(Change::Expired(neighbor))
            }
            Event::View(offer) => {
                if self.pending || self.wanted.is_none() || offer.generation != self.generation {
                    return Ok(Change::Unchanged);
                }
                if self.epoch.is_some_and(|epoch| epoch != offer.epoch) {
                    return Ok(Change::Unchanged);
                }
                if self.replica.is_none() {
                    let mut replica =
                        Replica::new(offer.epoch, self.collection, self.schema.clone());
                    replica.begin_request(
                        self.generation,
                        self.wanted.as_ref().ok_or(Error::Invalid)?.clone(),
                    )?;
                    self.replica = Some(replica);
                    self.epoch = Some(offer.epoch);
                }
                let replica = self.replica.as_mut().ok_or(Error::Invalid)?;
                let changed = match replica.apply(&offer) {
                    Ok(changed) => changed,
                    Err(Error::Merkle(merkle::Error::ResyncRequired)) => {
                        self.awaiting = true;
                        self.control = Some(Command::Resync {
                            generation: self.generation,
                        });
                        return Ok(Change::Unchanged);
                    }
                    Err(Error::Stale) => return Ok(Change::Unchanged),
                    Err(error) => return Err(error),
                };
                self.active_generation = self.generation;
                self.active_plan = self.wanted.clone();
                self.awaiting = false;
                self.control = Some(Command::Acknowledge {
                    generation: self.generation,
                    cursor: offer.transfer.cursor,
                    root: offer.transfer.root,
                });
                Ok(if changed {
                    Change::View
                } else {
                    Change::Unchanged
                })
            }
            Event::Revoked {
                epoch,
                generation,
                collection,
            } => {
                if self.wanted.is_none() && self.active_generation == 0 {
                    return Ok(Change::Unchanged);
                }
                if collection != self.collection
                    || self.epoch.is_some_and(|current| current != epoch)
                    || generation != self.generation && generation != self.active_generation
                {
                    return Ok(Change::Unchanged);
                }
                self.disconnected();
                Ok(Change::Revoked)
            }
            Event::Rejected { generation, status } => {
                if generation != 0 && generation != self.generation {
                    return Ok(Change::Unchanged);
                }
                if self.wanted.is_none() {
                    return Ok(Change::Unchanged);
                }
                if self.pending && generation != 0 {
                    return Ok(Change::Unchanged);
                }
                self.awaiting = false;
                if generation == 0 && matches!(status, Status::ResyncRequired | Status::Unavailable)
                {
                    self.replica = None;
                    self.active_plan = None;
                    self.epoch = None;
                    self.active_generation = 0;
                    self.pending = self.wanted.is_some();
                    self.control = None;
                    return Ok(Change::Unchanged);
                }
                if matches!(
                    status,
                    Status::Forbidden | Status::Limit | Status::Schema | Status::Invalid
                ) && let Some(plan) = &self.active_plan
                    && self.wanted.as_ref() != Some(plan)
                {
                    // Failed replacements retain a coherent pin. Re-establish
                    // its live coverage with a fresh monotonic generation;
                    // the failed request may already have superseded the old
                    // generation on the authority.
                    self.wanted = Some(plan.clone());
                    self.pending = true;
                    self.control = None;
                }
                Ok(Change::Rejected(status))
            }
        }
    }
}
