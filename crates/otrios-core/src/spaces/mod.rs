// SPDX-License-Identifier: LGPL-3.0-only
//! Serialized Space management and admission. Hosts must authenticate identities
//! before calling this module and hold one authority lock through writes/reads.
use otrios_interfaces::spaces::*;
use std::collections::BTreeSet;
mod discovery;
mod observer;
mod wire;
pub use discovery::{CollectionFacts, advance_discovery, discovery, discovery_cursor};
pub use observer::{Observer, Outcome};
pub use wire::{decode_command, decode_event, encode_command, encode_event, management_status};

#[cfg(not(target_arch = "wasm32"))]
mod store;
#[cfg(not(target_arch = "wasm32"))]
pub use store::Store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Denied,
    Conflict,
    Unavailable,
    Incompatible,
    LastAdmin,
    Capacity,
    Persistence(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

pub fn valid_identity(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:@".contains(&b))
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_policy(p: &AppPolicy) -> bool {
    otrios_interfaces::canvas::valid_destination(&p.app)
        && p.schema.len() == 64
        && p.schema
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn fresh() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(crate::canvas::hex(&bytes))
}
pub fn empty() -> Registry {
    Registry {
        version: VERSION,
        generation: 0,
        spaces: vec![],
        deletions: vec![],
    }
}

/// Validates recovered state before any admission. Tombstones never expire in v1.
pub fn validate(registry: &Registry) -> Result<()> {
    if registry.version != VERSION
        || registry.spaces.len() > MAX_SPACES
        || registry.deletions.len() > MAX_DELETIONS
    {
        return Err(Error::Invalid);
    }
    let mut spaces = BTreeSet::new();
    let mut collections = BTreeSet::new();
    for d in &registry.deletions {
        if !valid_id(&d.collection)
            || d.generation == 0
            || d.generation > registry.generation
            || !collections.insert(&d.collection)
        {
            return Err(Error::Invalid);
        }
    }
    for s in &registry.spaces {
        if !valid_id(&s.id)
            || !spaces.insert(&s.id)
            || s.name.trim().is_empty()
            || s.name.len() > 128
            || s.admission_epoch == 0
            || s.admission_epoch > registry.generation
            || s.members.len() > MAX_MEMBERS
            || s.apps.len() > MAX_APPS
            || !s.members.iter().any(|m| m.role == Role::Admin)
        {
            return Err(Error::Invalid);
        }
        let mut members = BTreeSet::new();
        for m in &s.members {
            if !valid_identity(&m.identity)
                || !members.insert(&m.identity)
                || m.joined_generation > s.admission_epoch
            {
                return Err(Error::Invalid);
            }
        }
        let mut apps = BTreeSet::new();
        for a in &s.apps {
            if !valid_policy(&a.policy)
                || !apps.insert(&a.policy.app)
                || !valid_id(&a.collection)
                || !collections.insert(&a.collection)
                || a.attachment_epoch == 0
                || a.attachment_epoch > registry.generation
                || (a.attached && a.policy.closed_only && s.audience != Audience::Closed)
            {
                return Err(Error::Invalid);
            }
        }
    }
    Ok(())
}
pub fn role(space: &Space, identity: Option<&str>) -> Option<Role> {
    space
        .members
        .iter()
        .find(|m| Some(m.identity.as_str()) == identity)
        .map(|m| m.role)
}
pub fn visible(space: &Space, identity: Option<&str>) -> bool {
    space.audience != Audience::Closed || role(space, identity).is_some()
}
fn admin(space: &Space, identity: &str) -> Result<()> {
    if role(space, Some(identity)) == Some(Role::Admin) {
        Ok(())
    } else {
        Err(Error::Denied)
    }
}

/// Builds an immutable candidate. Persist it before publishing its generation.
/// `installed` is a verified host policy for Attach, never a client policy.
pub fn apply(
    registry: &Registry,
    actor: &str,
    request: &Request,
    installed: Option<&AppPolicy>,
) -> Result<Registry> {
    if !valid_identity(actor) || request.version != VERSION {
        return Err(Error::Invalid);
    }
    if request.expected_generation != registry.generation {
        return Err(Error::Conflict);
    }
    let mut next = registry.clone();
    next.generation = next.generation.checked_add(1).ok_or(Error::Capacity)?;
    let epoch = next.generation;
    if let Change::Create { name } = &request.change {
        if next.spaces.len() == MAX_SPACES {
            return Err(Error::Capacity);
        }
        if name.trim().is_empty() || name.len() > 128 {
            return Err(Error::Invalid);
        }
        next.spaces.push(Space {
            id: fresh()?,
            name: name.clone(),
            audience: Audience::Closed,
            members: vec![Member {
                identity: actor.into(),
                role: Role::Admin,
                joined_generation: epoch,
            }],
            apps: vec![],
            admission_epoch: epoch,
        });
    } else {
        let space = match &request.change {
            Change::Membership { space, .. }
            | Change::Audience { space, .. }
            | Change::Attach { space, .. }
            | Change::Remove { space, .. }
            | Change::DeleteData { space, .. } => space,
            Change::Create { .. } => unreachable!(),
        };
        // Denied conceals whether an inaccessible Space exists.
        let s = next
            .spaces
            .iter_mut()
            .find(|s| &s.id == space)
            .ok_or(Error::Denied)?;
        admin(s, actor)?;
        match &request.change {
            Change::Membership {
                identity,
                role: requested,
                ..
            } => {
                if !valid_identity(identity) {
                    return Err(Error::Invalid);
                }
                let joined = s
                    .members
                    .iter()
                    .find(|m| &m.identity == identity)
                    .map_or(epoch, |m| m.joined_generation);
                s.members.retain(|m| &m.identity != identity);
                if let Some(role) = requested {
                    if s.members.len() == MAX_MEMBERS {
                        return Err(Error::Capacity);
                    }
                    s.members.push(Member {
                        identity: identity.clone(),
                        role: *role,
                        joined_generation: joined,
                    });
                }
                if !s.members.iter().any(|m| m.role == Role::Admin) {
                    return Err(Error::LastAdmin);
                }
                s.admission_epoch = epoch;
            }
            Change::Audience { audience, .. } => {
                if *audience != Audience::Closed
                    && s.apps.iter().any(|a| a.attached && a.policy.closed_only)
                {
                    return Err(Error::Denied);
                }
                s.audience = *audience;
                s.admission_epoch = epoch;
            }
            Change::Attach { app, .. } => {
                let policy = installed
                    .filter(|p| &p.app == app && valid_policy(p))
                    .ok_or(Error::Unavailable)?;
                if policy.closed_only && s.audience != Audience::Closed {
                    return Err(Error::Denied);
                }
                if let Some(data) = s.apps.iter_mut().find(|a| &a.policy.app == app) {
                    if data.policy.schema != policy.schema {
                        return Err(Error::Incompatible);
                    }
                    data.policy = policy.clone();
                    data.attached = true;
                    data.attachment_epoch = epoch;
                } else {
                    if s.apps.len() == MAX_APPS {
                        return Err(Error::Capacity);
                    }
                    s.apps.push(AppData {
                        policy: policy.clone(),
                        collection: fresh()?,
                        attached: true,
                        attachment_epoch: epoch,
                    });
                }
            }
            Change::Remove { app, .. } => {
                let data = s
                    .apps
                    .iter_mut()
                    .find(|a| &a.policy.app == app)
                    .ok_or(Error::Unavailable)?;
                data.attached = false;
                data.attachment_epoch = epoch;
            }
            Change::DeleteData { app, remove, .. } => {
                if next.deletions.len() == MAX_DELETIONS {
                    return Err(Error::Capacity);
                }
                let data = s
                    .apps
                    .iter_mut()
                    .find(|a| &a.policy.app == app)
                    .ok_or(Error::Unavailable)?;
                next.deletions.push(Deletion {
                    collection: data.collection.clone(),
                    generation: epoch,
                });
                // Replacement and tombstone are one durable management publication.
                data.collection = fresh()?;
                data.attached &= !remove;
                data.attachment_epoch = epoch;
            }
            Change::Create { .. } => unreachable!(),
        }
    }
    validate(&next)?;
    Ok(next)
}

pub fn bind(
    registry: &Registry,
    identity: Option<&str>,
    space: &str,
    app: &str,
    installed_schema: &str,
) -> Result<Binding> {
    let s = registry
        .spaces
        .iter()
        .find(|s| s.id == space && visible(s, identity))
        .ok_or(Error::Denied)?;
    let a = s
        .apps
        .iter()
        .find(|a| a.policy.app == app && a.attached)
        .ok_or(Error::Denied)?;
    if a.policy.schema != installed_schema {
        return Err(Error::Incompatible);
    }
    Ok(Binding {
        version: VERSION,
        space: s.id.clone(),
        app: app.into(),
        collection: a.collection.clone(),
        schema: a.policy.schema.clone(),
        admission_epoch: s.admission_epoch,
        attachment_epoch: a.attachment_epoch,
    })
}

pub fn authorize(
    registry: &Registry,
    identity: Option<&str>,
    binding: &Binding,
    operation: Operation,
) -> Result<()> {
    if binding.version != VERSION {
        return Err(Error::Denied);
    }
    let s = registry
        .spaces
        .iter()
        .find(|s| s.id == binding.space)
        .ok_or(Error::Denied)?;
    let a = s
        .apps
        .iter()
        .find(|a| {
            a.attached
                && a.policy.app == binding.app
                && a.collection == binding.collection
                && a.policy.schema == binding.schema
        })
        .ok_or(Error::Denied)?;
    let membership = s
        .members
        .iter()
        .find(|m| Some(m.identity.as_str()) == identity);
    if binding.admission_epoch == 0
        || binding.admission_epoch > s.admission_epoch
        || a.attachment_epoch != binding.attachment_epoch
        || (s.admission_epoch != binding.admission_epoch
            && membership.is_none_or(|m| m.joined_generation > binding.admission_epoch))
    {
        return Err(Error::Denied);
    }
    let member = role(s, identity).is_some();
    let allowed = match operation {
        Operation::Read => visible(s, identity),
        Operation::Edit => member && a.policy.editable,
        Operation::Comment => {
            a.policy.comments
                && (member || (identity.is_some() && s.audience == Audience::PublicComment))
        }
        Operation::Moderate => member && a.policy.comments,
    };
    if allowed { Ok(()) } else { Err(Error::Denied) }
}

/// Only a host-provisioned grant may call this entry point. Membership changes
/// do not grant/revoke service identity; removal/deletion invalidate its binding.
pub fn authorize_service(
    registry: &Registry,
    identity: &str,
    service: &ServiceBinding,
) -> Result<()> {
    let b = &service.binding;
    if !valid_identity(identity) || identity != service.identity || b.version != VERSION {
        return Err(Error::Denied);
    }
    let s = registry
        .spaces
        .iter()
        .find(|s| s.id == b.space)
        .ok_or(Error::Denied)?;
    if s.apps.iter().any(|a| {
        a.attached
            && a.collection == b.collection
            && a.policy.app == b.app
            && a.policy.schema == b.schema
            && a.attachment_epoch == b.attachment_epoch
    }) {
        Ok(())
    } else {
        Err(Error::Denied)
    }
}

/// The current verified installation can reduce availability/actions immediately;
/// a cached old UI or an unchanged collection ID cannot bypass its constraints.
pub fn authorize_installed(
    registry: &Registry,
    identity: Option<&str>,
    binding: &Binding,
    operation: Operation,
    installed: &AppPolicy,
) -> Result<()> {
    installed_compatible(registry, binding, installed)?;
    if (operation == Operation::Edit && !installed.editable)
        || (matches!(operation, Operation::Comment | Operation::Moderate) && !installed.comments)
    {
        return Err(Error::Denied);
    }
    authorize(registry, identity, binding, operation)
}
pub fn authorize_service_installed(
    registry: &Registry,
    identity: &str,
    service: &ServiceBinding,
    installed: &AppPolicy,
) -> Result<()> {
    installed_compatible(registry, &service.binding, installed)?;
    authorize_service(registry, identity, service)
}
fn installed_compatible(
    registry: &Registry,
    binding: &Binding,
    installed: &AppPolicy,
) -> Result<()> {
    if !valid_policy(installed)
        || installed.app != binding.app
        || installed.schema != binding.schema
    {
        return Err(Error::Incompatible);
    }
    if installed.closed_only
        && registry
            .spaces
            .iter()
            .any(|s| s.id == binding.space && s.audience != Audience::Closed)
    {
        return Err(Error::Incompatible);
    }
    Ok(())
}
