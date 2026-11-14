// SPDX-License-Identifier: LGPL-3.0-only
//! Serialized Space management and admission. Hosts must authenticate identities
//! before calling this module and hold one authority lock through writes/reads.
use otrios_interfaces::spaces::*;
use std::collections::BTreeSet;
mod access;
mod usage;
pub use usage::{AcceptedUser, active_users};
mod discovery;
mod moves;
mod observer;
#[cfg(test)]
mod reset_tests;
mod retained;
mod schema;
pub use schema::collection_schema;
mod wire;
pub use discovery::{
    CollectionFacts, advance_discovery, discovery, discovery_cursor, discovery_with_cleanup,
};
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
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}
fn valid_layout(layout: SpaceLayout) -> bool {
    (1..=6).contains(&layout.width) && (1..=6).contains(&layout.height)
}
fn fresh() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(crate::canvas::hex(&bytes))
}
pub fn empty() -> Registry {
    Registry {
        retained: vec![],
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
        || registry.retained.len() > MAX_RETAINED_DATASETS
    {
        return Err(Error::Invalid);
    }
    let mut spaces = BTreeSet::new();
    let declared_spaces = registry
        .spaces
        .iter()
        .map(|s| s.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut collections = BTreeSet::new();
    for d in &registry.deletions {
        if !valid_id(&d.collection)
            || !d.space.is_empty()
                && (!valid_id(&d.space) || !declared_spaces.contains(d.space.as_str()))
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
            || !valid_layout(s.layout)
            || s.admission_epoch == 0
            || s.admission_epoch > registry.generation
            || s.members.len() > MAX_MEMBERS
            || s.apps.len() > MAX_APPS
            || s.apps.len() + registry.retained.iter().filter(|d| d.space == s.id).count()
                > MAX_APPS
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
                || (!a.label.is_empty() && !valid_name(&a.label))
                || (a.attached && a.policy.closed_only && s.audience != Audience::Closed)
            {
                return Err(Error::Invalid);
            }
        }
    }
    for dataset in &registry.retained {
        if !declared_spaces.contains(dataset.space.as_str())
            || !valid_policy(&dataset.policy)
            || !valid_id(&dataset.collection)
            || !collections.insert(&dataset.collection)
            || !dataset.label.is_empty() && !valid_name(&dataset.label)
            || dataset.generation == 0
            || dataset.generation > registry.generation
        {
            return Err(Error::Invalid);
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
    if space.archived {
        role(space, identity) == Some(Role::Admin)
    } else {
        space.audience != Audience::Closed || role(space, identity).is_some()
    }
}
/// Configured service principals cannot also acquire human membership roles.
pub fn admit_service_identity(registry: &Registry, identity: &str) -> Result<()> {
    if !valid_identity(identity) {
        return Err(Error::Invalid);
    }
    if registry
        .spaces
        .iter()
        .any(|s| s.members.iter().any(|m| m.identity == identity))
    {
        return Err(Error::Denied);
    }
    Ok(())
}
pub fn admit_human_management(actor: &str, request: &Request, services: &[&str]) -> Result<()> {
    if services.contains(&actor)
        || matches!(&request.change, Change::Membership { identity, .. } if services.contains(&identity.as_str()))
        || matches!(&request.change, Change::Access { changes, .. } if changes.iter().any(|c| services.contains(&c.identity.as_str())))
    {
        return Err(Error::Denied);
    }
    Ok(())
}
fn admin(space: &Space, identity: &str) -> Result<()> {
    if role(space, Some(identity)) == Some(Role::Admin) {
        Ok(())
    } else {
        Err(Error::Denied)
    }
}

/// Services use explicit service bindings; their tokens do not become human
/// editing/comment credentials merely because a Space permits signed-in visitors.
pub fn admit_human_action(identity: Option<&str>, services: &[&str]) -> Result<()> {
    if identity.is_none_or(|id| !valid_identity(id) || services.contains(&id)) {
        return Err(Error::Denied);
    }
    Ok(())
}

/// Fixed chat commands remain separate from document edits and moderation.
/// A publisher cannot relabel an arbitrary state write as a public comment.
pub fn authorize_chat(
    registry: &Registry,
    identity: Option<&str>,
    binding: &Binding,
    installed: &AppPolicy,
    command: &otrios_interfaces::chat::Command,
) -> Result<()> {
    use otrios_interfaces::chat::Command as Chat;
    let allow = |operation| authorize_installed(registry, identity, binding, operation, installed);
    match command {
        Chat::Leave | Chat::ReadAsset { .. } | Chat::Publish { .. } => allow(Operation::Read),
        Chat::Join { credential: None }
        | Chat::Profile { .. }
        | Chat::Send { .. }
        | Chat::SendAsset { .. }
        | Chat::ClaimDraft
        | Chat::Draft { .. }
        | Chat::ClearDraft { .. } => allow(Operation::Edit).or_else(|_| allow(Operation::Comment)),
        Chat::Kick { .. } | Chat::Redact { .. } => allow(Operation::Moderate),
        Chat::Join { .. } | Chat::Authenticate { .. } | Chat::SignOut => Err(Error::Denied),
    }
}

/// An explicit Admin audience change adopts compatible, currently installed
/// capability declarations without replacing collection identities or payloads.
pub fn apply_catalog(
    registry: &Registry,
    actor: &str,
    request: &Request,
    installed: &[AppPolicy],
) -> Result<Registry> {
    if installed.len() > MAX_INSTALLED_APPS || installed.iter().any(|p| !valid_policy(p)) {
        return Err(Error::Invalid);
    }
    if !valid_identity(actor) || request.version != VERSION {
        return Err(Error::Invalid);
    }
    if request.expected_generation != registry.generation {
        return Err(Error::Conflict);
    }
    let mut current = registry.clone();
    if let Change::Audience { space, audience }
    | Change::Access {
        space, audience, ..
    } = &request.change
    {
        let selected = current
            .spaces
            .iter_mut()
            .find(|s| &s.id == space)
            .ok_or(Error::Denied)?;
        admin(selected, actor)?;
        for app in &mut selected.apps {
            if let Some(policy) = installed.iter().find(|p| p.app == app.policy.app) {
                if app.attached && *audience != Audience::Closed && policy.closed_only {
                    return Err(Error::Incompatible);
                }
                if policy.schema == app.policy.schema {
                    app.policy = policy.clone();
                }
            }
        }
    }
    let attaching = match &request.change {
        Change::Attach { app, .. }
        | Change::RestoreDataset { app, .. }
        | Change::Move { app, .. }
        | Change::DeleteData {
            app, remove: false, ..
        } => installed.iter().find(|p| &p.app == app),
        _ => None,
    };
    apply(&current, actor, request, attaching)
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
    if matches!(request.change, Change::Profile { .. }) {
        return Err(Error::Unavailable);
    }
    let mut next = registry.clone();
    next.generation = next.generation.checked_add(1).ok_or(Error::Capacity)?;
    let epoch = next.generation;
    if access::mutate(&mut next, actor, &request.change)?
        || moves::mutate(&mut next, actor, &request.change, installed)?
        || retained::mutate(&mut next, actor, &request.change, installed)?
    {
        validate(&next)?;
        return Ok(next);
    }
    if let Change::Create { name } = &request.change {
        if next.spaces.len() == MAX_SPACES {
            return Err(Error::Capacity);
        }
        if !valid_name(name) {
            return Err(Error::Invalid);
        }
        next.spaces.push(Space {
            archived: false,
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
            layout: SpaceLayout::default(),
        });
    } else if let Change::PlaceSpace { space, before } = &request.change {
        let index = next
            .spaces
            .iter()
            .position(|s| &s.id == space)
            .ok_or(Error::Denied)?;
        admin(&next.spaces[index], actor)?;
        if before.as_ref().is_some_and(|id| {
            id == space
                || !next
                    .spaces
                    .iter()
                    .any(|s| &s.id == id && visible(s, Some(actor)))
        }) {
            return Err(Error::Denied);
        }
        let moved = next.spaces.remove(index);
        let target = before
            .as_ref()
            .and_then(|id| next.spaces.iter().position(|s| &s.id == id))
            .unwrap_or(next.spaces.len());
        next.spaces.insert(target, moved);
    } else {
        let space = match &request.change {
            Change::Membership { space, .. }
            | Change::Audience { space, .. }
            | Change::Attach { space, .. }
            | Change::Remove { space, .. }
            | Change::DeleteData { space, .. }
            | Change::ConfigureSpace { space, .. }
            | Change::LabelApp { space, .. }
            | Change::PlaceApp { space, .. } => space,
            Change::Profile { .. }
            | Change::Access { .. }
            | Change::Create { .. }
            | Change::PlaceSpace { .. }
            | Change::Move { .. }
            | Change::DeleteSpace { .. }
            | Change::RestoreSpace { .. }
            | Change::RestoreDataset { .. }
            | Change::DeleteDataset { .. }
            | Change::LabelDataset { .. } => unreachable!(),
        };
        // Denied conceals whether an inaccessible Space exists.
        let s = next
            .spaces
            .iter_mut()
            .find(|s| &s.id == space)
            .ok_or(Error::Denied)?;
        admin(s, actor)?;
        if s.archived
            && !matches!(
                request.change,
                Change::Membership { .. } | Change::ConfigureSpace { .. }
            )
        {
            return Err(Error::Unavailable);
        }
        match &request.change {
            Change::ConfigureSpace { name, layout, .. } => {
                if !valid_name(name) || !valid_layout(*layout) {
                    return Err(Error::Invalid);
                }
                s.name = name.trim().into();
                s.layout = *layout;
            }
            Change::LabelApp { app, name, .. } => {
                if !valid_name(name) {
                    return Err(Error::Invalid);
                }
                s.apps
                    .iter_mut()
                    .find(|a| &a.policy.app == app)
                    .ok_or(Error::Unavailable)?
                    .label = name.trim().into();
            }
            Change::PlaceApp { app, before, .. } => {
                let index = s
                    .apps
                    .iter()
                    .position(|a| &a.policy.app == app && a.attached)
                    .ok_or(Error::Unavailable)?;
                if before.as_ref().is_some_and(|id| {
                    id == app || !s.apps.iter().any(|a| &a.policy.app == id && a.attached)
                }) {
                    return Err(Error::Invalid);
                }
                let moved = s.apps.remove(index);
                let target = before
                    .as_ref()
                    .and_then(|id| s.apps.iter().position(|a| &a.policy.app == id))
                    .unwrap_or(s.apps.len());
                s.apps.insert(target, moved);
            }
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
                        label: String::new(),
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
            Change::DeleteData {
                app,
                collection,
                remove,
                ..
            } => {
                // A deliberate reset initializes the currently installed app,
                // including a breaking schema update. This is fresh data, not
                // a migration of the discarded collection.
                let replacement_policy = installed.filter(|p| &p.app == app && valid_policy(p));
                if !remove
                    && replacement_policy.is_some_and(|p| p.closed_only)
                    && s.audience != Audience::Closed
                {
                    return Err(Error::Denied);
                }
                if next.deletions.len() == MAX_DELETIONS {
                    return Err(Error::Capacity);
                }
                let data = s
                    .apps
                    .iter_mut()
                    .find(|a| &a.policy.app == app)
                    .ok_or(Error::Unavailable)?;
                if !valid_id(collection) {
                    return Err(Error::Invalid);
                }
                if &data.collection != collection {
                    return Err(Error::Conflict);
                }
                next.deletions.push(Deletion {
                    space: s.id.clone(),
                    collection: data.collection.clone(),
                    generation: epoch,
                });
                // Replacement and tombstone are one durable management publication.
                data.collection = fresh()?;
                if !remove && let Some(policy) = replacement_policy {
                    data.policy = policy.clone();
                }
                data.attached &= !remove;
                data.attachment_epoch = epoch;
                if *remove {
                    s.apps.retain(|a| a.policy.app != *app);
                }
            }
            Change::Profile { .. }
            | Change::Access { .. }
            | Change::Create { .. }
            | Change::PlaceSpace { .. }
            | Change::Move { .. }
            | Change::DeleteSpace { .. }
            | Change::RestoreSpace { .. }
            | Change::RestoreDataset { .. }
            | Change::DeleteDataset { .. }
            | Change::LabelDataset { .. } => unreachable!(),
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
        .find(|s| s.id == space && !s.archived && visible(s, identity))
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
    if s.archived
        || binding.admission_epoch == 0
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
        Operation::Edit => (member || s.audience == Audience::PublicEdit) && a.policy.editable,
        Operation::Comment => {
            a.policy.comments
                && (member
                    || s.audience == Audience::PublicEdit
                    || (identity.is_some() && s.audience == Audience::PublicComment))
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
    if !s.archived
        && s.apps.iter().any(|a| {
            a.attached
                && a.collection == b.collection
                && a.policy.app == b.app
                && a.policy.schema == b.schema
                && a.attachment_epoch == b.attachment_epoch
        })
    {
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

/// Explicit cross-Space copy: references grant no destination write permission.
/// A reviewed management generation also fences destination removal/replacement.
pub fn publication_destination(
    registry: &Registry,
    actor: &str,
    source: &Binding,
    destination_space: &str,
    expected_generation: u64,
    installed: &AppPolicy,
) -> Result<Binding> {
    if expected_generation != registry.generation {
        return Err(Error::Conflict);
    }
    if source.space == destination_space || !valid_identity(actor) {
        return Err(Error::Denied);
    }
    authorize_installed(registry, Some(actor), source, Operation::Read, installed)?;
    let destination = bind(
        registry,
        Some(actor),
        destination_space,
        &source.app,
        &installed.schema,
    )?;
    authorize_installed(
        registry,
        Some(actor),
        &destination,
        Operation::Edit,
        installed,
    )?;
    Ok(destination)
}

/// Opaque pending-input context; hosts still authorize every operation. A new
/// identity, continuous membership interval or attachment must not inherit RAM
/// intentions from its predecessor merely because its effective actions match.
pub fn actor_context(
    registry: &Registry,
    identity: Option<&str>,
    binding: &Binding,
) -> Result<[u8; 32]> {
    if identity.is_some_and(|id| !valid_identity(id)) {
        return Err(Error::Invalid);
    }
    let space = registry
        .spaces
        .iter()
        .find(|space| space.id == binding.space)
        .ok_or(Error::Denied)?;
    if !space.apps.iter().any(|app| {
        app.attached
            && app.policy.app == binding.app
            && app.collection == binding.collection
            && app.policy.schema == binding.schema
            && app.attachment_epoch == binding.attachment_epoch
    }) {
        return Err(Error::Denied);
    }
    let member = space
        .members
        .iter()
        .find(|member| Some(member.identity.as_str()) == identity);
    let interval = member.map_or(space.admission_epoch, |member| member.joined_generation);
    let canonical = flexbuffers::to_vec((
        "otrios-mount-actor-v1",
        &binding.collection,
        identity,
        member.is_some(),
        interval,
        binding.attachment_epoch,
    ))
    .map_err(|_| Error::Invalid)?;
    crate::canvas::hash(&canonical)
        .try_into()
        .map_err(|_| Error::Unavailable)
}
