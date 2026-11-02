// SPDX-License-Identifier: LGPL-3.0-only
//! A bounded single-Space ACL publication for the trusted People & access form.
use super::*;
pub(super) fn mutate(registry: &mut Registry, actor: &str, change: &Change) -> Result<bool> {
    let Change::Access {
        space,
        audience,
        changes,
    } = change
    else {
        return Ok(false);
    };
    if changes.len() > 16
        || changes.iter().any(|c| !valid_identity(&c.identity))
        || changes
            .iter()
            .map(|c| &c.identity)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != changes.len()
    {
        return Err(Error::Invalid);
    }
    let s = registry
        .spaces
        .iter_mut()
        .find(|s| s.id == *space)
        .ok_or(Error::Denied)?;
    admin(s, actor)?;
    if s.archived && *audience != s.audience {
        return Err(Error::Unavailable);
    }
    if *audience != Audience::Closed && s.apps.iter().any(|a| a.attached && a.policy.closed_only) {
        return Err(Error::Denied);
    }
    let mut members = s.members.clone();
    for edit in changes {
        let joined_generation = members
            .iter()
            .find(|m| m.identity == edit.identity)
            .map_or(registry.generation, |m| m.joined_generation);
        members.retain(|m| m.identity != edit.identity);
        if let Some(role) = edit.role {
            members.push(Member {
                identity: edit.identity.clone(),
                role,
                joined_generation,
            });
        }
    }
    if !members.iter().any(|m| m.role == Role::Admin) {
        return Err(Error::LastAdmin);
    }
    if members.len() > MAX_MEMBERS {
        return Err(Error::Capacity);
    }
    s.members = members;
    s.audience = *audience;
    s.admission_epoch = registry.generation;
    Ok(true)
}
