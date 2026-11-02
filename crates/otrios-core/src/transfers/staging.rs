// SPDX-License-Identifier: LGPL-3.0-only
//! Qualified temporary uploads. A review never changes an active binding.
use super::*;
use crate::{canvas, spaces};
use otrios_interfaces::spaces::{AppPolicy, Audience, Registry, RootSchema};
const TTL_MS: u64 = 10 * 60 * 1000;

/// Captured from installed code and current metadata under the admission lock.
/// The archive cannot choose a primary root, target binding or host catalog.
#[derive(Clone)]
pub struct ReplacementTarget {
    pub dataset: DatasetRef,
    pub policy: AppPolicy,
    pub primary: String,
    pub roots: Vec<RootPolicy>,
    pub catalog: Vec<u8>,
    pub generation: u64,
    pub audience: Audience,
}
impl ReplacementTarget {
    pub fn authorize(
        &self,
        registry: &Registry,
        actor: &str,
        catalog: &[u8],
    ) -> std::result::Result<(), spaces::Error> {
        authorize_dataset(registry, actor, &self.dataset, &self.policy)
            .map_err(|_| spaces::Error::Denied)?;
        let space = registry
            .spaces
            .iter()
            .find(|s| s.id == self.dataset.space)
            .ok_or(spaces::Error::Denied)?;
        if registry.generation != self.generation
            || self.catalog != catalog
            || self.catalog.len() != 32
            || self.audience != space.audience
        {
            return Err(spaces::Error::Conflict);
        }
        if space.archived {
            return Err(spaces::Error::Unavailable);
        }
        Ok(())
    }
}

/// Complete codec qualification can run away from the admission lock. This
/// object is still unadmitted until a current authority issues its review.
pub struct DataCandidate {
    target: ReplacementTarget,
    data: QualifiedCollection,
    digest: String,
    bytes: u64,
}
impl DataCandidate {
    pub fn qualify(target: ReplacementTarget, archive: &[u8]) -> Result<Self> {
        if target.catalog.len() != 32
            || !target.roots.iter().any(|r| r.state_ref == target.primary)
            || crate::spaces::collection_schema(
                &target
                    .roots
                    .iter()
                    .map(|p| RootSchema {
                        state_ref: p.state_ref.clone(),
                        schema: p.schema.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(canvas::err)?
                != target.policy.schema
        {
            return Err("Replacement installed root/catalog closure".into());
        }
        let data = inspect_data(super::archive::decode(super::archive::Kind::Data, archive)?)?;
        let data = QualifiedCollection::qualify(
            data,
            &target.dataset.app,
            &target.dataset.schema,
            &target.roots,
        )?;
        Ok(Self {
            target,
            data,
            digest: canvas::hex(&canvas::hash(archive)),
            bytes: archive.len() as u64,
        })
    }
    pub fn admit(
        self,
        registry: &Registry,
        actor: &str,
        catalog: &[u8],
        now: u64,
    ) -> std::result::Result<StagedReplacement, spaces::Error> {
        self.target.authorize(registry, actor, catalog)?;
        let id = || {
            let mut bytes = [0; 16];
            getrandom::fill(&mut bytes).map_err(|_| spaces::Error::Unavailable)?;
            Ok::<_, spaces::Error>(canvas::hex(&bytes))
        };
        let review = DataReview {
            stage: id()?,
            replacement: Replacement {
                space: self.target.dataset.space.clone(),
                app: self.target.dataset.app.clone(),
                collection: self.target.dataset.collection.clone(),
                replacement: id()?,
                expected_generation: self.target.generation,
            },
            audience: self.target.audience,
            schema_sha256: self.target.dataset.schema.clone(),
            archive_sha256: self.digest,
            bytes: self.bytes,
            expires_unix_ms: now.checked_add(TTL_MS).ok_or(spaces::Error::Capacity)?,
        };
        Ok(StagedReplacement {
            actor: actor.into(),
            created: now,
            review,
            target: self.target,
            data: self.data,
        })
    }
}

/// Hosts bound the number and aggregate memory/storage of these objects. They
/// must discard one before starting another upload when using a single credit.
pub struct StagedReplacement {
    actor: String,
    created: u64,
    review: DataReview,
    target: ReplacementTarget,
    data: QualifiedCollection,
}
impl StagedReplacement {
    pub fn review(&self) -> &DataReview {
        &self.review
    }
    pub fn expired(&self, now: u64) -> bool {
        now < self.created || now >= self.review.expires_unix_ms
    }
    pub fn owned_by(&self, actor: &str, id: &str) -> bool {
        self.actor == actor && self.review.stage == id
    }
    pub fn authorize(
        &self,
        registry: &Registry,
        actor: &str,
        catalog: &[u8],
        now: u64,
    ) -> std::result::Result<(), spaces::Error> {
        if self.actor != actor {
            return Err(spaces::Error::Denied);
        }
        if self.expired(now) {
            return Err(spaces::Error::Unavailable);
        }
        self.target.authorize(registry, actor, catalog)
    }
    /// Consume once before the durable operation. An uncertain result is resolved
    /// through the fresh CID's read-only outcome, never by reusing the stage.
    pub fn into_parts(self) -> (DataReview, ReplacementTarget, QualifiedCollection) {
        (self.review, self.target, self.data)
    }
}
