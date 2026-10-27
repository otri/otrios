// SPDX-License-Identifier: LGPL-3.0-only
//! Reusable admitted collection lane for an already negotiated shared session.
//! One lane binds one resource until explicit cancellation. The embedding host
//! owns authentication and supplies access independently of command bytes.
use super::*;
use crate::merkle::Hash;
use std::collections::BTreeMap;

const MAX_COLLECTIONS: usize = 16;

#[derive(Clone, Copy, Default)]
pub struct Access {
    pub authenticated: bool,
    pub admitted: bool,
}
#[derive(Clone, Default)]
pub struct Policy {
    pub public_fields: Vec<u64>,
    pub authenticated_fields: Vec<u64>,
    pub admitted_fields: Vec<u64>,
}
impl Policy {
    fn grant(&self, collection: u64, access: Access) -> Grant {
        let mut fields = self.public_fields.clone();
        if access.authenticated {
            fields.extend_from_slice(&self.authenticated_fields);
        }
        if access.admitted {
            fields.extend_from_slice(&self.admitted_fields);
        }
        fields.sort_unstable();
        fields.dedup();
        Grant { collection, fields }
    }
}
struct Source {
    authority: Authority,
    policy: Policy,
}
#[derive(Default)]
pub struct ChannelSession {
    collection: Option<u64>,
    session: Session,
}
impl ChannelSession {
    pub fn generation(&self) -> u64 {
        self.session.generation()
    }
    pub fn collection(&self) -> Option<u64> {
        self.collection
    }
    pub fn cancel(&mut self) {
        self.session.cancel();
        self.collection = None;
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    View(Offer),
    Rejected {
        generation: u64,
        status: Status,
    },
    Revoked {
        epoch: u64,
        generation: u64,
        collection: u64,
    },
}
impl From<Error> for Status {
    fn from(error: Error) -> Self {
        match error {
            Error::Limit | Error::Merkle(merkle::Error::Limit) => Self::Limit,
            Error::Schema | Error::Merkle(merkle::Error::Schema) => Self::Schema,
            Error::Forbidden | Error::Merkle(merkle::Error::Forbidden) => Self::Forbidden,
            Error::Stale
            | Error::Conflict
            | Error::Merkle(merkle::Error::Stale | merkle::Error::Conflict) => Self::Stale,
            Error::AnchorExpired => Self::AnchorExpired,
            Error::Backpressure => Self::Backpressure,
            Error::Merkle(merkle::Error::ResyncRequired) => Self::ResyncRequired,
            _ => Self::Invalid,
        }
    }
}
pub struct Channel {
    epoch: u64,
    sources: BTreeMap<u64, Source>,
    notify: Option<Box<dyn Fn() + Send + Sync>>,
}
impl Channel {
    /// Epoch changes whenever the owner loses its lease/authority RAM.
    pub fn new(epoch: u64) -> Self {
        Self {
            epoch,
            sources: BTreeMap::new(),
            notify: None,
        }
    }
    /// The callback must be nonblocking; it is a coalesced publication hint,
    /// never a copy of record bodies or a second writable collection owner.
    pub fn set_notifier(&mut self, notify: impl Fn() + Send + Sync + 'static) {
        self.notify = Some(Box::new(notify));
    }
    fn changed(&self) {
        if let Some(notify) = &self.notify {
            notify();
        }
    }
    pub fn install(&mut self, collection: Collection, policy: Policy) -> Result<()> {
        if self.sources.contains_key(&collection.id) {
            return Err(Error::Conflict);
        }
        if self.sources.len() >= MAX_COLLECTIONS {
            return Err(Error::Limit);
        }
        for fields in [
            &policy.public_fields,
            &policy.authenticated_fields,
            &policy.admitted_fields,
        ] {
            if fields.len() > MAX_FIELDS
                || fields
                    .iter()
                    .any(|f| !collection.schema.fields.contains_key(f))
            {
                return Err(Error::Schema);
            }
        }
        self.sources.insert(
            collection.id,
            Source {
                authority: Authority::new(self.epoch, collection),
                policy,
            },
        );
        self.changed();
        Ok(())
    }
    pub fn collection(&self, id: u64) -> Result<&Collection> {
        self.sources
            .get(&id)
            .map(|source| source.authority.collection())
            .ok_or(Error::Invalid)
    }
    pub fn required_access(&self, session: &ChannelSession, access: Access) -> Access {
        let Some(plan) = session.session.plan() else {
            return Access::default();
        };
        let mut required = Access::default();
        if let Some(source) = self.sources.get(&plan.collection) {
            for field in &plan.fields {
                if source.policy.public_fields.contains(field) {
                    continue;
                }
                if access.authenticated && source.policy.authenticated_fields.contains(field) {
                    required.authenticated = true;
                } else if access.admitted && source.policy.admitted_fields.contains(field) {
                    required.admitted = true;
                }
            }
        }
        required
    }
    pub fn publish(&mut self, id: u64, expected: Hash, next: Collection) -> Result<()> {
        self.sources
            .get_mut(&id)
            .ok_or(Error::Invalid)?
            .authority
            .publish(expected, next)?;
        self.changed();
        Ok(())
    }
    pub fn set_policy(&mut self, id: u64, policy: Policy) -> Result<()> {
        let source = self.sources.get_mut(&id).ok_or(Error::Invalid)?;
        for fields in [
            &policy.public_fields,
            &policy.authenticated_fields,
            &policy.admitted_fields,
        ] {
            if fields.len() > MAX_FIELDS
                || fields
                    .iter()
                    .any(|f| !source.authority.collection().schema.fields.contains_key(f))
            {
                return Err(Error::Schema);
            }
        }
        source.policy = policy;
        self.changed();
        Ok(())
    }
    /// Recheck policy even while the peer holds an unacknowledged offer.
    pub fn authorize(&self, session: &mut ChannelSession, access: Access) -> Option<Event> {
        let id = session.collection?;
        let source = self.sources.get(&id)?;
        if source
            .authority
            .authorize_session(&mut session.session, source.policy.grant(id, access))
            .is_err()
        {
            return Some(Event::Revoked {
                epoch: self.epoch,
                generation: session.generation(),
                collection: id,
            });
        }
        None
    }
    pub fn handle(
        &mut self,
        session: &mut ChannelSession,
        access: Access,
        command: Command,
    ) -> Option<Event> {
        if let Some(revoked) = self.authorize(session, access) {
            return Some(revoked);
        }
        let generation = command.generation();
        if let Command::Select { plan, .. } = &command
            && session.collection.is_some_and(|id| id != plan.collection)
        {
            return Some(Event::Rejected {
                generation,
                status: Status::Invalid,
            });
        }
        let id = match &command {
            Command::Select { plan, .. } => Some(plan.collection),
            _ => session.collection,
        };
        let Some(id) = id else {
            return Some(Event::Rejected {
                generation,
                status: Status::Stale,
            });
        };
        let Some(source) = self.sources.get_mut(&id) else {
            // Do not disclose whether a private/uninstalled resource exists.
            return Some(Event::Rejected {
                generation,
                status: Status::Unavailable,
            });
        };
        let cancel = matches!(command, Command::Cancel { .. });
        match source.authority.handle(
            &mut session.session,
            source.policy.grant(id, access),
            command,
        ) {
            Ok(offer) => {
                if cancel {
                    session.collection = None;
                } else {
                    session.collection = Some(id);
                }
                offer.map(Event::View)
            }
            Err(error) => Some(Event::Rejected {
                generation,
                status: error.into(),
            }),
        }
    }
    pub fn poll(&self, session: &mut ChannelSession, access: Access) -> Option<Event> {
        if let Some(revoked) = self.authorize(session, access) {
            return Some(revoked);
        }
        let id = session.collection?;
        let source = self.sources.get(&id)?;
        match source.authority.poll(&mut session.session) {
            Ok(offer) => offer.map(Event::View),
            Err(Error::Backpressure) => None, // ACK credit explicitly remains outstanding.
            Err(error) => Some(Event::Rejected {
                generation: session.generation(),
                status: error.into(),
            }),
        }
    }
}
