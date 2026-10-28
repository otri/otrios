// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use std::sync::Arc;

/// One bounded frame; cloning only shares immutable transport bytes.
#[derive(Clone, Debug)]
pub struct Offer {
    pub cursor: Cursor,
    pub root: Option<Hash>,
    bytes: Arc<[u8]>,
}
impl Offer {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Decode the existing canonical transfer envelope without publishing it.
    /// A replica still qualifies its scope, baseline and materialized DAG.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let transfer = wire::decode_transfer(bytes)?;
        Ok(Self {
            cursor: transfer.cursor,
            root: transfer.root,
            bytes: bytes.into(),
        })
    }
}
/// Single authority owner: subscribe, capture and publication are serialized by
/// &mut self. A host may put this behind its publication lock, never split them.
/// Durability is the embedding server's job; publish here is RAM-only.
#[derive(Clone)]
pub struct Authority {
    epoch: u64,
    next_lease: u64,
    current: Snapshot,
}
#[derive(Clone)]
pub struct Subscription {
    epoch: u64,
    lease: u64,
    path: Path,
    coverage: Option<Coverage>,
    projected: Option<(Hash, Snapshot)>,
    next_sequence: u64,
    acknowledged: Option<(Cursor, Option<Snapshot>)>,
    flight: Option<(Offer, Option<Snapshot>)>,
    cancelled: bool,
}
impl Authority {
    /// Epoch must change whenever the host loses lease/cursor state.
    pub fn new(epoch: u64, current: Snapshot) -> Self {
        Self::with_lease_counter(epoch, current, 0)
    }
    /// Embedding collection authorities allocate globally unique lease IDs while
    /// retaining independent bounded selected roots for each peer.
    pub(crate) fn with_lease_counter(epoch: u64, current: Snapshot, next_lease: u64) -> Self {
        Self {
            epoch,
            next_lease,
            current,
        }
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.current
    }
    pub fn publish(&mut self, expected: Hash, next: Snapshot) -> Result<()> {
        if self.current.hash() != expected {
            return Err(Error::Conflict);
        }
        self.current = next;
        Ok(())
    }
    /// allowed_roots comes from TRUSTED SERVER POLICY, never client bytes.
    /// A known hash or a requested path is not a read grant. No get-by-hash API.
    /// A grant includes every descendant of each listed root. Revoke via cancel.
    pub fn subscribe(&mut self, allowed_roots: &[Path], path: Path) -> Result<Subscription> {
        path_check(&path)?;
        if !allowed_roots
            .iter()
            .any(|root| root.len() <= MAX_DEPTH && path.starts_with(root))
        {
            return Err(Error::Forbidden);
        }
        self.register(path, None)
    }
    /// One cursor/ACK advances all requested paths together. Coverage is fixed
    /// for the lease; changing it requires a new authorized subscription.
    pub fn subscribe_coverage(
        &mut self,
        allowed_roots: &[Path],
        coverage: Coverage,
    ) -> Result<Subscription> {
        coverage.authorize(allowed_roots)?;
        self.register(vec![], Some(coverage))
    }
    fn register(&mut self, path: Path, coverage: Option<Coverage>) -> Result<Subscription> {
        self.next_lease = self
            .next_lease
            .checked_add(1)
            .ok_or(Error::ClockExhausted)?;
        let mut subscription = Subscription {
            epoch: self.epoch,
            lease: self.next_lease,
            path,
            coverage,
            projected: None,
            next_sequence: 0,
            acknowledged: None,
            flight: None,
            cancelled: false,
        };
        // Capture initial root while registering; even a missing slot is watched.
        self.offer(&mut subscription)?;
        Ok(subscription)
    }
    /// At most one in-flight frame plus one acknowledged pinned subtree. While
    /// slow, return the same offer; after its ACK, compare against current/latest.
    /// No queue of intermediate revisions and no lost CRDT causal metadata.
    pub fn offer(&self, subscription: &mut Subscription) -> Result<Option<Offer>> {
        if subscription.cancelled {
            return Err(Error::Forbidden);
        }
        if subscription.epoch != self.epoch {
            return Err(Error::Stale);
        }
        if let Some((offer, _)) = &subscription.flight {
            return Ok(Some(offer.clone()));
        }
        let new = if let Some(coverage) = &subscription.coverage {
            if subscription
                .projected
                .as_ref()
                .is_none_or(|(root, _)| *root != self.current.hash())
            {
                subscription.projected =
                    Some((self.current.hash(), self.current.project(coverage)?.0));
            }
            subscription
                .projected
                .as_ref()
                .map(|(_, view)| view.clone())
        } else {
            match self.current.subtree(&subscription.path) {
                Ok(s) => Some(s),
                Err(Error::Missing) => None,
                Err(e) => return Err(e),
            }
        };
        let old = subscription
            .acknowledged
            .as_ref()
            .and_then(|(_, s)| s.as_ref());
        if subscription.acknowledged.is_some()
            && old.map(Snapshot::hash) == new.as_ref().map(Snapshot::hash)
        {
            return Ok(None);
        }
        subscription.next_sequence = subscription
            .next_sequence
            .checked_add(1)
            .ok_or(Error::ClockExhausted)?;
        let cursor = subscription.cursor();
        let bytes = wire::encode_transfer(
            cursor,
            &subscription.path,
            subscription.acknowledged.is_none(),
            old,
            new.as_ref(),
        )?;
        let offer = Offer {
            cursor,
            root: new.as_ref().map(Snapshot::hash),
            bytes: bytes.into(),
        };
        subscription.flight = Some((offer.clone(), new));
        Ok(Some(offer))
    }
}
impl Subscription {
    pub fn cursor(&self) -> Cursor {
        Cursor {
            epoch: self.epoch,
            lease: self.lease,
            sequence: self.next_sequence,
        }
    }
    pub fn path(&self) -> &[u64] {
        &self.path
    }
    /// ACK exactly the offered cursor and digest, not a client-invented baseline.
    pub fn acknowledge(&mut self, cursor: Cursor, root: Option<Hash>) -> Result<()> {
        if self.cancelled {
            return Err(Error::Forbidden);
        }
        if self
            .acknowledged
            .as_ref()
            .is_some_and(|(c, s)| *c == cursor && s.as_ref().map(Snapshot::hash) == root)
        {
            return Ok(());
        }
        if !self
            .flight
            .as_ref()
            .is_some_and(|(offer, _)| offer.cursor == cursor && offer.root == root)
        {
            return Err(Error::Stale);
        }
        let (_, snapshot) = self.flight.take().expect("checked flight");
        self.acknowledged = Some((cursor, snapshot));
        Ok(())
    }
    /// Explicit resync after a gap or lost client RAM; next offer is a full
    /// snapshot with a newer sequence, keeping this lease's scope unchanged.
    pub fn resync(&mut self) {
        self.acknowledged = None;
        self.flight = None;
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.acknowledged = None;
        self.flight = None;
        self.projected = None;
    }
}
/// Read replica of exactly one authorized scope. Never writes durable storage.
/// Incoming revisions replace authority state; local pending edits must be kept
/// separately and merged by the host, not silently overwritten in this replica.
#[derive(Clone)]
pub struct Replica {
    epoch: u64,
    lease: u64,
    path: Path,
    cursor: Option<Cursor>,
    snapshot: Option<Snapshot>,
    cancelled: bool,
    last_transfer: Option<Hash>,
}
impl Replica {
    pub fn new(subscription: &Subscription) -> Self {
        Self {
            epoch: subscription.epoch,
            lease: subscription.lease,
            path: subscription.path.clone(),
            cursor: None,
            snapshot: None,
            cancelled: false,
            last_transfer: None,
        }
    }
    /// Used by a transport host after receiving an independently authorized lease.
    pub fn for_lease(epoch: u64, lease: u64, path: Path) -> Result<Self> {
        path_check(&path)?;
        Ok(Self {
            epoch,
            lease,
            path,
            cursor: None,
            snapshot: None,
            cancelled: false,
            last_transfer: None,
        })
    }
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }
    pub fn cursor(&self) -> Option<Cursor> {
        self.cursor
    }
    /// Parse, validate and materialize ALL changed nodes before replacing the
    /// visible root. Corrupt, partial, extra or out-of-scope frames expose nothing.
    pub fn apply(&mut self, bytes: &[u8]) -> Result<bool> {
        if self.cancelled {
            return Err(Error::Forbidden);
        }
        let transfer = wire::decode_transfer(bytes)?;
        if transfer.cursor.epoch != self.epoch
            || transfer.cursor.lease != self.lease
            || transfer.path != self.path
        {
            return Err(Error::Stale);
        }
        let fingerprint = transfer.fingerprint();
        if self.cursor == Some(transfer.cursor) {
            return if self.last_transfer == Some(fingerprint) {
                Ok(false)
            } else {
                Err(Error::Invalid)
            };
        }
        if self
            .cursor
            .is_some_and(|c| c.sequence >= transfer.cursor.sequence)
        {
            return Err(Error::Stale);
        }
        if !transfer.full
            && (self
                .cursor
                .is_none_or(|c| c.sequence.checked_add(1) != Some(transfer.cursor.sequence))
                || self.snapshot.as_ref().map(Snapshot::hash) != transfer.base)
        {
            return Err(Error::ResyncRequired);
        }
        let old = if transfer.full {
            None
        } else {
            self.snapshot.as_ref()
        };
        let next = transfer.materialize(old)?;
        if next
            .as_ref()
            .is_some_and(|s| self.path.len() + s.0.height > MAX_DEPTH)
        {
            return Err(Error::Limit);
        }
        let changed =
            self.snapshot.as_ref().map(Snapshot::hash) != next.as_ref().map(Snapshot::hash);
        self.snapshot = next;
        self.cursor = Some(transfer.cursor);
        self.last_transfer = Some(fingerprint);
        Ok(changed)
    }
    /// Drop private working state and reject late frames for this revoked lease.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.snapshot = None;
        self.cursor = None;
        self.last_transfer = None;
    }
    /// Diagnostic count includes metadata-only revisions, not presentation work.
    pub fn transferred_nodes(bytes: &[u8]) -> Result<usize> {
        Ok(wire::decode_transfer(bytes)?.record_count())
    }
}
