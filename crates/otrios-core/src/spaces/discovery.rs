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
    discovery_with_cleanup(
        registry,
        identity,
        selected,
        installed,
        facts,
        &BTreeMap::new(),
        cursor,
    )
}

pub fn discovery_with_cleanup(
    registry: &Registry,
    identity: Option<&str>,
    selected: Option<&str>,
    installed: &[InstalledApp],
    facts: &BTreeMap<String, CollectionFacts>,
    cleanup: &BTreeMap<String, CleanupSummary>,
    cursor: DiscoveryCursor,
) -> Result<Discovery> {
    if !valid_id(&cursor.authority)
        || identity.is_some_and(|id| !valid_identity(id))
        || installed.len() > MAX_INSTALLED_APPS
        || installed.iter().any(|app| !valid_installed(app))
    {
        return Err(Error::Invalid);
    }
    let mut previews_left = MAX_APPS;
    let spaces = registry
        .spaces
        .iter()
        .filter(|s| visible(s, identity))
        .map(|s| {
            let preview_count = s
                .apps
                .iter()
                .filter(|a| a.attached && !s.archived)
                .count()
                .min(MAX_INSTALLED_APPS)
                .min(previews_left);
            previews_left -= preview_count;
            let mut retained = if role(s, identity) == Some(Role::Admin) {
                retained_summaries(registry, s, installed, facts)
            } else {
                vec![]
            };
            retained.truncate(previews_left);
            previews_left -= retained.len();
            SpaceSummary {
                archived: s.archived,
                retained,
                id: s.id.clone(),
                name: s.name.clone(),
                audience: s.audience,
                role: role(s, identity),
                layout: s.layout,
                attached_count: s.apps.iter().filter(|a| a.attached && !s.archived).count() as u16,
                apps: s
                    .apps
                    .iter()
                    .filter(|a| a.attached && !s.archived)
                    .take(preview_count)
                    .map(|a| {
                        let app = installed.iter().find(|p| p.policy.app == a.policy.app);
                        MapApp {
                            app: a.policy.app.clone(),
                            title: title(a, app),
                            availability: availability(a, app, facts, s.audience, false),
                        }
                    })
                    .collect(),
            }
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
                    let storage = facts.get(&a.collection);
                    let availability = availability(a, app, facts, s.audience, true);
                    AppSummary {
                        active_users: None,
                        app: a.policy.app.clone(),
                        title: title(a, app),
                        collection: a.collection.clone(),
                        attached: a.attached,
                        availability,
                        stored_bytes: admin
                            .then(|| storage.and_then(|f| f.stored_bytes))
                            .flatten(),
                        launch: if !s.archived
                            && a.attached
                            && availability == AppAvailability::Available
                        {
                            app.cloned()
                        } else {
                            None
                        },
                    }
                })
                .collect();
            SpaceDetail {
                active_users: None,
                retained: if admin {
                    retained_summaries(registry, s, installed, facts)
                } else {
                    vec![]
                },
                space: s.id.clone(),
                cleanup: if admin {
                    cleanup.get(&s.id).cloned().map(Box::new)
                } else {
                    None
                },
                apps,
                members: admin.then(|| s.members.clone()),
                installed: if admin { installed.to_vec() } else { vec![] },
            }
        });
    let snapshot = Discovery {
        profile: None,
        catalog: vec![],
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

fn title(data: &AppData, installed: Option<&InstalledApp>) -> String {
    if data.label.is_empty() {
        installed.map_or_else(|| data.policy.app.clone(), |p| p.title.clone())
    } else {
        data.label.clone()
    }
}
fn availability(
    data: &AppData,
    app: Option<&InstalledApp>,
    facts: &BTreeMap<String, CollectionFacts>,
    audience: Audience,
    require_state: bool,
) -> AppAvailability {
    match app {
        None => AppAvailability::Unavailable,
        Some(p) if p.policy.schema != data.policy.schema => AppAvailability::Incompatible,
        Some(p)
            if (p.policy.closed_only && audience != Audience::Closed)
                || facts
                    .get(&data.collection)
                    .map_or(require_state, |f| !f.available) =>
        {
            AppAvailability::Unavailable
        }
        Some(_) => AppAvailability::Available,
    }
}
fn valid_installed(app: &InstalledApp) -> bool {
    valid_policy(&app.policy)
        && !app.title.is_empty()
        && app.title.len() <= 96
        && app.ui_ref == format!("app:{}/ui", app.policy.app)
        && app.revision.is_none_or(|r| r > 0)
        && !app.state_ref.is_empty()
        && app.state_ref.len() <= 128
}

pub(super) fn validate_discovery(snapshot: &Discovery) -> Result<()> {
    if (!snapshot.catalog.is_empty() && snapshot.catalog.len() != 32)
        || snapshot.version != VERSION
        || snapshot
            .profile
            .as_ref()
            .is_some_and(|p| snapshot.identity.is_none() || p.revision == 0 || !valid_name(&p.name))
        || !valid_id(&snapshot.cursor.authority)
        || snapshot.spaces.len() > MAX_SPACES
        || snapshot
            .spaces
            .iter()
            .map(|s| s.apps.len() + s.retained.len())
            .sum::<usize>()
            > MAX_APPS
        || snapshot
            .identity
            .as_ref()
            .is_some_and(|id| !valid_identity(id))
    {
        return Err(Error::Invalid);
    }
    let mut ids = BTreeSet::new();
    let mut datasets = BTreeSet::new();
    for s in &snapshot.spaces {
        if !valid_id(&s.id)
            || !ids.insert(&s.id)
            || s.name.trim().is_empty()
            || s.name.len() > 128
            || !valid_layout(s.layout)
            || s.archived && s.role != Some(Role::Admin)
            || !s.retained.is_empty() && s.role != Some(Role::Admin)
            || s.retained.len() > MAX_APPS
            || s.retained
                .iter()
                .any(|d| !valid_dataset(d) || !datasets.insert(&d.collection))
            || s.retained
                .iter()
                .map(|d| &d.collection)
                .collect::<BTreeSet<_>>()
                .len()
                != s.retained.len()
            || s.apps.len() > MAX_INSTALLED_APPS
            || usize::from(s.attached_count) > MAX_APPS
            || s.apps.len() > usize::from(s.attached_count)
            || s.apps.iter().any(|a| {
                !otrios_interfaces::canvas::valid_destination(&a.app) || !valid_name(&a.title)
            })
            || s.apps.iter().map(|a| &a.app).collect::<BTreeSet<_>>().len() != s.apps.len()
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
            || detail.retained.len() > MAX_APPS
            || !admin && !detail.retained.is_empty()
            || detail.retained.iter().any(|d| !valid_dataset(d))
            || detail
                .retained
                .iter()
                .map(|d| &d.collection)
                .collect::<BTreeSet<_>>()
                .len()
                != detail.retained.len()
            || detail.installed.len() > MAX_INSTALLED_APPS
            || (!admin
                && (detail.active_users.is_some()
                    || detail.apps.iter().any(|a| a.active_users.is_some())
                    || detail.members.is_some()
                    || !detail.installed.is_empty()
                    || detail.cleanup.is_some()))
            || detail.cleanup.as_ref().is_some_and(|c| {
                c.deleted_collections > MAX_DELETIONS as u32
                    || c.local_pending > c.deleted_collections
            })
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

fn valid_dataset(dataset: &DatasetSummary) -> bool {
    valid_id(&dataset.collection)
        && otrios_interfaces::canvas::valid_destination(&dataset.app)
        && valid_name(&dataset.title)
}
fn retained_summaries(
    registry: &Registry,
    space: &Space,
    installed: &[InstalledApp],
    facts: &BTreeMap<String, CollectionFacts>,
) -> Vec<DatasetSummary> {
    let mut result = space
        .apps
        .iter()
        .filter(|a| !a.attached || space.archived)
        .map(|a| {
            let app = installed.iter().find(|p| p.policy.app == a.policy.app);
            DatasetSummary {
                app: a.policy.app.clone(),
                collection: a.collection.clone(),
                title: title(a, app),
                availability: availability(a, app, facts, space.audience, true),
                stored_bytes: facts.get(&a.collection).and_then(|f| f.stored_bytes),
                origin: None,
            }
        })
        .collect::<Vec<_>>();
    let mut backups = registry
        .retained
        .iter()
        .filter(|d| d.space == space.id)
        .collect::<Vec<_>>();
    backups.sort_by(|a, b| {
        a.generation
            .cmp(&b.generation)
            .then(a.collection.cmp(&b.collection))
    });
    result.extend(backups.into_iter().map(|d| {
        let app = installed.iter().find(|p| p.policy.app == d.policy.app);
        let data = AppData {
            policy: d.policy.clone(),
            collection: d.collection.clone(),
            label: d.label.clone(),
            attached: false,
            attachment_epoch: d.generation,
        };
        let suffix = match d.origin {
            DatasetOrigin::BeforeRestore => "before restore",
            DatasetOrigin::BeforeReplacement => "before replacement",
        };
        let base = title(&data, app);
        let mut caption = format!(
            "{} {suffix} · {}",
            base.chars().take(70).collect::<String>(),
            d.generation
        );
        while caption.len() > 128 {
            caption.pop();
        }
        DatasetSummary {
            app: d.policy.app.clone(),
            collection: d.collection.clone(),
            title: caption,
            availability: availability(&data, app, facts, space.audience, true),
            stored_bytes: facts.get(&d.collection).and_then(|f| f.stored_bytes),
            origin: Some(d.origin),
        }
    }));
    result
}
