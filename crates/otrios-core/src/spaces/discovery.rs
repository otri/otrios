// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded, authorization-filtered host metadata. App documents never enter it.
use super::*;
use std::collections::BTreeMap;

pub fn discovery_cursor() -> Result<DiscoveryCursor> {
    Ok(DiscoveryCursor {
        authority: fresh()?,
        revision: 0,
    })
}
pub fn advance_discovery(cursor: &DiscoveryCursor) -> Result<DiscoveryCursor> {
    if let Some(revision) = cursor.revision.checked_add(1) {
        Ok(DiscoveryCursor {
            authority: cursor.authority.clone(),
            revision,
        })
    } else {
        discovery_cursor()
    }
}

/// Facts supplied by trusted storage/catalog adapters, never by a client.
#[derive(Clone, Debug)]
pub struct CollectionFacts {
    pub stored_bytes: Option<u64>,
    pub available: bool,
}

pub fn discovery(
    registry: &Registry,
    identity: Option<&str>,
    selected: Option<&str>,
    installed: &[InstalledApp],
    facts: &BTreeMap<String, CollectionFacts>,
    cursor: DiscoveryCursor,
) -> Result<Discovery> {
    if !valid_id(&cursor.authority)
        || identity.is_some_and(|id| !valid_identity(id))
        || installed.len() > MAX_INSTALLED_APPS
        || installed.iter().any(|app| !valid_installed(app))
    {
        return Err(Error::Invalid);
    }
    let spaces = registry
        .spaces
        .iter()
        .filter(|s| visible(s, identity))
        .map(|s| SpaceSummary {
            id: s.id.clone(),
            name: s.name.clone(),
            audience: s.audience,
            role: role(s, identity),
        })
        .collect();
    let selected = registry
        .spaces
        .iter()
        .find(|s| Some(s.id.as_str()) == selected && visible(s, identity))
        .map(|s| {
            let admin = role(s, identity) == Some(Role::Admin);
            let apps = s
                .apps
                .iter()
                .filter(|a| a.attached || admin)
                .map(|a| {
                    let app = installed.iter().find(|p| p.policy.app == a.policy.app);
                    let facts = facts.get(&a.collection);
                    let availability = match app {
                        None => AppAvailability::Unavailable,
                        Some(p) if p.policy.schema != a.policy.schema => {
                            AppAvailability::Incompatible
                        }
                        Some(p)
                            if (p.policy.closed_only && s.audience != Audience::Closed)
                                || facts.is_none_or(|f| !f.available) =>
                        {
                            AppAvailability::Unavailable
                        }
                        Some(_) => AppAvailability::Available,
                    };
                    AppSummary {
                        app: a.policy.app.clone(),
                        title: app.map_or_else(|| a.policy.app.clone(), |p| p.title.clone()),
                        collection: a.collection.clone(),
                        attached: a.attached,
                        availability,
                        stored_bytes: admin.then(|| facts.and_then(|f| f.stored_bytes)).flatten(),
                        launch: if a.attached && availability == AppAvailability::Available {
                            app.cloned()
                        } else {
                            None
                        },
                    }
                })
                .collect();
            SpaceDetail {
                space: s.id.clone(),
                apps,
                members: admin.then(|| s.members.clone()),
                installed: if admin { installed.to_vec() } else { vec![] },
            }
        });
    let snapshot = Discovery {
        version: VERSION,
        cursor,
        generation: registry.generation,
        identity: identity.map(str::to_owned),
        spaces,
        selected,
    };
    validate_discovery(&snapshot)?;
    Ok(snapshot)
}

fn valid_installed(app: &InstalledApp) -> bool {
    valid_policy(&app.policy)
        && !app.title.is_empty()
        && app.title.len() <= 96
        && app.ui_ref == format!("app:{}/ui", app.policy.app)
        && !app.state_ref.is_empty()
        && app.state_ref.len() <= 128
}

pub(super) fn validate_discovery(snapshot: &Discovery) -> Result<()> {
    if snapshot.version != VERSION
        || !valid_id(&snapshot.cursor.authority)
        || snapshot.spaces.len() > MAX_SPACES
        || snapshot
            .identity
            .as_ref()
            .is_some_and(|id| !valid_identity(id))
    {
        return Err(Error::Invalid);
    }
    let mut ids = BTreeSet::new();
    for s in &snapshot.spaces {
        if !valid_id(&s.id) || !ids.insert(&s.id) || s.name.trim().is_empty() || s.name.len() > 128
        {
            return Err(Error::Invalid);
        }
    }
    if let Some(detail) = &snapshot.selected {
        let summary = snapshot
            .spaces
            .iter()
            .find(|s| s.id == detail.space)
            .ok_or(Error::Invalid)?;
        let admin = summary.role == Some(Role::Admin);
        if detail.apps.len() > MAX_APPS
            || detail.installed.len() > MAX_INSTALLED_APPS
            || (!admin && (detail.members.is_some() || !detail.installed.is_empty()))
            || detail.installed.iter().any(|p| !valid_installed(p))
        {
            return Err(Error::Invalid);
        }
        if let Some(members) = &detail.members
            && (members.len() > MAX_MEMBERS || members.iter().any(|m| !valid_identity(&m.identity)))
        {
            return Err(Error::Invalid);
        }
        let mut apps = BTreeSet::new();
        for a in &detail.apps {
            if !otrios_interfaces::canvas::valid_destination(&a.app)
                || !apps.insert(&a.app)
                || !valid_id(&a.collection)
                || a.title.is_empty()
                || a.title.len() > 128
                || (!admin && (!a.attached || a.stored_bytes.is_some()))
                || a.launch.as_ref().is_some_and(|p| {
                    !valid_installed(p)
                        || p.policy.app != a.app
                        || !a.attached
                        || a.availability != AppAvailability::Available
                })
            {
                return Err(Error::Invalid);
            }
        }
    }
    Ok(())
}
