// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::merkle::{self, Hash};
use std::sync::Arc;

/// One selected publication and causal transfer. The envelope is data, not
/// admission. Hosts must authenticate and negotiate before routing it.
#[derive(Clone, Debug)]
pub struct Offer {
    pub epoch: u64,
    pub lease: u64,
    pub generation: u64,
    pub revision: Hash,
    pub more_before: bool,
    pub more_after: bool,
    pub transfer: merkle::Offer,
}
/// At most one acknowledged selection plus one in-flight selection per lease.
/// Window replacement uses a second lease; hosts cancel the old lease after the
/// replacement qualifies. There is never a queue of intermediate publications.
pub struct Lease {
    epoch: u64,
    id: u64,
    generation: u64,
    plan: Selection,
    grant: Grant,
    authority: merkle::Authority,
    subscription: merkle::Subscription,
    flight: Option<Offer>,
    cancelled: bool,
}
impl Lease {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn plan(&self) -> &Selection {
        &self.plan
    }
    pub fn acknowledge(&mut self, cursor: merkle::Cursor, root: Option<Hash>) -> Result<()> {
        self.subscription.acknowledge(cursor, root)?;
        if self
            .flight
            .as_ref()
            .is_some_and(|o| o.transfer.cursor == cursor && o.transfer.root == root)
        {
            self.flight = None;
        }
        Ok(())
    }
    pub fn resync(&mut self) {
        self.subscription.resync();
        self.flight = None;
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.subscription.cancel();
        self.flight = None;
        // Release the selected records as well as the ACK/transfer roots.
        self.authority = merkle::Authority::new(self.epoch, merkle::Snapshot::empty());
    }
}

/// Per-peer window owner: one active lease and one replacement, including their
/// bounded ACK/transfer roots. A newer request cancels the superseded transfer.
#[derive(Default)]
pub struct Session {
    generation: u64,
    active: Option<Lease>,
    pending: Option<Lease>,
}
impl Session {
    pub fn plan(&self) -> Option<&Selection> {
        self.pending
            .as_ref()
            .or(self.active.as_ref())
            .map(Lease::plan)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn lease_count(&self) -> usize {
        usize::from(self.active.is_some()) + usize::from(self.pending.is_some())
    }
    pub fn waiting_for_ack(&self) -> bool {
        self.pending
            .as_ref()
            .or(self.active.as_ref())
            .is_some_and(|lease| lease.flight.is_some())
    }
    pub fn acknowledge(
        &mut self,
        generation: u64,
        cursor: merkle::Cursor,
        root: Option<Hash>,
    ) -> Result<()> {
        if generation != self.generation {
            return Err(Error::Stale);
        }
        if let Some(pending) = &mut self.pending {
            if cursor.lease != pending.id {
                return Err(Error::Stale);
            }
            pending.acknowledge(cursor, root)?;
            if let Some(active) = &mut self.active {
                active.cancel();
            }
            self.active = self.pending.take();
            return Ok(());
        }
        self.active
            .as_mut()
            .ok_or(Error::Stale)?
            .acknowledge(cursor, root)
    }
    pub fn resync(&mut self, generation: u64) -> Result<()> {
        if generation != self.generation {
            return Err(Error::Stale);
        }
        self.pending
            .as_mut()
            .or(self.active.as_mut())
            .ok_or(Error::Stale)?
            .resync();
        Ok(())
    }
    pub fn cancel(&mut self) {
        for lease in [&mut self.active, &mut self.pending] {
            if let Some(mut lease) = lease.take() {
                lease.cancel();
            }
        }
    }
}

/// One serialized publication/selection owner. Host durability precedes publish;
/// expected revision protects competing prepared publications. This is not IPC.
pub struct Authority {
    epoch: u64,
    next_lease: u64,
    current: Collection,
}
impl Authority {
    pub fn new(epoch: u64, current: Collection) -> Self {
        Self {
            epoch,
            next_lease: 0,
            current,
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn collection(&self) -> &Collection {
        &self.current
    }
    /// Adapter for the existing session's ordered binary command lane. Trusted
    /// admission supplies the grant independently for every command.
    pub fn handle(
        &mut self,
        session: &mut Session,
        grant: Grant,
        command: Command,
    ) -> Result<Option<Offer>> {
        self.authorize_session(session, grant.clone())?;
        match command {
            Command::Select { generation, plan } => {
                self.request(session, grant, generation, plan).map(Some)
            }
            Command::Acknowledge {
                generation,
                cursor,
                root,
            } => {
                session.acknowledge(generation, cursor, root)?;
                self.poll(session)
            }
            Command::Resync { generation } => {
                session.resync(generation)?;
                self.poll(session)
            }
            Command::Cancel { generation } => {
                if generation != session.generation {
                    return Err(Error::Stale);
                }
                session.cancel();
                Ok(None)
            }
        }
    }
    pub fn publish(&mut self, expected: Hash, next: Collection) -> Result<()> {
        if self.current.revision() != expected {
            return Err(Error::Conflict);
        }
        if self.current.id != next.id || self.current.schema != next.schema {
            return Err(Error::Schema);
        }
        self.current = next;
        Ok(())
    }
    pub fn request(
        &mut self,
        session: &mut Session,
        grant: Grant,
        generation: u64,
        plan: Selection,
    ) -> Result<Offer> {
        if generation <= session.generation {
            return Err(Error::Stale);
        }
        // Validate the plan/policy before disturbing the old qualified view.
        if grant.collection != self.current.id
            || plan.collection != self.current.id
            || plan.fields.iter().any(|f| !grant.fields.contains(f))
        {
            return Err(Error::Forbidden);
        }
        window::check_plan(self.current.id, &self.current.schema, &plan)?;
        session.generation = generation;
        if let Some(mut old) = session.pending.take() {
            old.cancel();
        }
        let mut pending = self.subscribe(grant, generation, plan)?;
        let offer = self.offer(&mut pending)?.ok_or(Error::Invalid)?;
        session.pending = Some(pending);
        Ok(offer)
    }
    pub fn poll(&self, session: &mut Session) -> Result<Option<Offer>> {
        let Some(lease) = session.pending.as_mut().or(session.active.as_mut()) else {
            return Ok(None);
        };
        // A failed replacement cannot allow an old-generation offer to escape.
        if lease.generation != session.generation {
            return Ok(None);
        }
        if lease.flight.is_some() {
            return Err(Error::Backpressure);
        }
        self.offer(lease)
    }
    pub fn authorize_session(&self, session: &mut Session, grant: Grant) -> Result<()> {
        let forbidden = [&session.active, &session.pending]
            .into_iter()
            .flatten()
            .any(|lease| {
                lease.plan.collection != grant.collection
                    || lease.plan.fields.iter().any(|f| !grant.fields.contains(f))
            });
        if forbidden {
            session.cancel();
            return Err(Error::Forbidden);
        }
        for lease in [&mut session.active, &mut session.pending]
            .into_iter()
            .flatten()
        {
            lease.grant = grant.clone();
        }
        Ok(())
    }
    pub fn subscribe(&mut self, grant: Grant, generation: u64, plan: Selection) -> Result<Lease> {
        if generation == 0 {
            return Err(Error::Invalid);
        }
        let window = self.current.select(self.epoch, &grant, &plan)?;
        let id = self.next_lease.checked_add(1).ok_or(Error::Limit)?;
        let mut authority =
            merkle::Authority::with_lease_counter(self.epoch, window.snapshot, self.next_lease);
        let subscription = authority.subscribe(&[vec![]], vec![])?;
        self.next_lease = id;
        let mut lease = Lease {
            epoch: self.epoch,
            id,
            generation,
            plan,
            grant,
            authority,
            subscription,
            flight: None,
            cancelled: false,
        };
        self.offer(&mut lease)?;
        Ok(lease)
    }
    pub fn offer(&self, lease: &mut Lease) -> Result<Option<Offer>> {
        if lease.cancelled {
            return Err(Error::Forbidden);
        }
        if lease.epoch != self.epoch {
            return Err(Error::Stale);
        }
        if let Some(offer) = &lease.flight {
            return Ok(Some(offer.clone()));
        }
        let window = self.current.select(self.epoch, &lease.grant, &lease.plan)?;
        lease
            .authority
            .publish(lease.authority.snapshot().hash(), window.snapshot)?;
        let Some(transfer) = lease.authority.offer(&mut lease.subscription)? else {
            return Ok(None);
        };
        let offer = Offer {
            epoch: self.epoch,
            lease: lease.id,
            generation: lease.generation,
            revision: window.revision,
            more_before: window.more_before,
            more_after: window.more_after,
            transfer,
        };
        lease.flight = Some(offer.clone());
        Ok(Some(offer))
    }
    /// Policy changes cancel the lease even without any data/hash publication.
    /// The host sends an explicit revoke control event; the client clears RAM.
    pub fn update_grant(&self, lease: &mut Lease, grant: Grant) -> Result<()> {
        if lease.grant.collection != grant.collection
            || lease.plan.fields.iter().any(|f| !grant.fields.contains(f))
        {
            lease.cancel();
            return Err(Error::Forbidden);
        }
        lease.grant = grant;
        Ok(())
    }
}

#[derive(Clone)]
struct State {
    lease: u64,
    plan: Selection,
    replica: merkle::Replica,
    window: Arc<Window>,
}
/// One active view plus a single supersedable replacement. begin_request keeps
/// the previous qualified view visible; apply prepares off to the side and only
/// publishes after membership, scope and every record qualify. RAM-only.
pub struct Replica {
    epoch: u64,
    collection: u64,
    schema: Schema,
    generation: u64,
    requested: Option<Selection>,
    active: Option<State>,
    revoked: bool,
}
impl Replica {
    pub fn new(epoch: u64, collection: u64, schema: Schema) -> Self {
        Self {
            epoch,
            collection,
            schema,
            generation: 0,
            requested: None,
            active: None,
            revoked: false,
        }
    }
    pub fn begin_request(&mut self, generation: u64, plan: Selection) -> Result<()> {
        if self.revoked {
            return Err(Error::Forbidden);
        }
        if generation <= self.generation {
            return Err(Error::Stale);
        }
        window::check_plan(self.collection, &self.schema, &plan)?;
        self.generation = generation;
        self.requested = Some(plan);
        Ok(())
    }
    pub fn window(&self) -> Option<&Arc<Window>> {
        self.active.as_ref().map(|s| &s.window)
    }
    pub fn apply(&mut self, offer: &Offer) -> Result<bool> {
        if self.revoked {
            return Err(Error::Forbidden);
        }
        if offer.epoch != self.epoch
            || offer.generation != self.generation
            || offer.transfer.cursor.epoch != self.epoch
            || offer.transfer.cursor.lease != offer.lease
        {
            return Err(Error::Stale);
        }
        if offer.transfer.bytes().len() > window::MAX_VIEW_BYTES {
            return Err(Error::Limit);
        }
        let plan = self.requested.as_ref().ok_or(Error::Stale)?;
        let mut replica = if let Some(active) = &self.active {
            if active.lease == offer.lease && active.plan == *plan {
                active.replica.clone()
            } else {
                merkle::Replica::for_lease(self.epoch, offer.lease, vec![])?
            }
        } else {
            merkle::Replica::for_lease(self.epoch, offer.lease, vec![])?
        };
        let changed = replica.apply(offer.transfer.bytes())?;
        let window = window::qualify(
            plan,
            &self.schema,
            replica.snapshot().ok_or(Error::Invalid)?.clone(),
            offer.revision,
            offer.more_before,
            offer.more_after,
        )?;
        self.active = Some(State {
            lease: offer.lease,
            plan: plan.clone(),
            replica,
            window: Arc::new(window),
        });
        Ok(changed)
    }
    /// Private view lifetime ends immediately, independent of renderer capacity.
    pub fn revoke(&mut self) {
        self.revoked = true;
        self.requested = None;
        self.active = None;
    }
}
