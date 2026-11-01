// SPDX-License-Identifier: LGPL-3.0-only
//! Scoped archived Spaces and exact retained-dataset decisions.
use super::*;

pub(super) fn mutate(
    registry: &mut Registry,
    actor: &str,
    change: &Change,
    installed: Option<&AppPolicy>,
) -> Result<bool> {
    let id = match change {
        Change::DeleteSpace { space, .. }
        | Change::RestoreSpace { space }
        | Change::RestoreDataset { space, .. }
        | Change::LabelDataset { space, .. }
        | Change::DeleteDataset { space, .. } => space,
        _ => return Ok(false),
    };
    let index = registry
        .spaces
        .iter()
        .position(|s| &s.id == id)
        .ok_or(Error::Denied)?;
    admin(&registry.spaces[index], actor)?;
    let generation = registry.generation;
    match change {
        Change::LabelDataset {
            app,
            collection,
            name,
            ..
        } => {
            if !valid_name(name) {
                return Err(Error::Invalid);
            }
            let space = &mut registry.spaces[index];
            if let Some(data) = space.apps.iter_mut().find(|a| {
                a.policy.app == *app
                    && a.collection == *collection
                    && (!a.attached || space.archived)
            }) {
                data.label = name.trim().into();
            } else {
                registry
                    .retained
                    .iter_mut()
                    .find(|d| d.space == *id && d.policy.app == *app && d.collection == *collection)
                    .ok_or(Error::Conflict)?
                    .label = name.trim().into();
            }
        }
        Change::DeleteSpace { remove_data, .. } => {
            let space = &mut registry.spaces[index];
            if space.archived && !remove_data {
                return Err(Error::Conflict);
            }
            if *remove_data {
                let mut collections = space
                    .apps
                    .iter()
                    .map(|a| a.collection.clone())
                    .collect::<Vec<_>>();
                collections.extend(
                    registry
                        .retained
                        .iter()
                        .filter(|d| d.space == space.id)
                        .map(|d| d.collection.clone()),
                );
                if registry.deletions.len() + collections.len() > MAX_DELETIONS {
                    return Err(Error::Capacity);
                }
                registry
                    .deletions
                    .extend(collections.into_iter().map(|collection| Deletion {
                        space: space.id.clone(),
                        collection,
                        generation,
                    }));
                registry.retained.retain(|d| d.space != space.id);
                space.apps.clear();
            }
            space.archived = true;
            space.admission_epoch = generation;
        }
        Change::RestoreSpace { .. } => {
            let space = &mut registry.spaces[index];
            if !space.archived {
                return Err(Error::Conflict);
            }
            space.archived = false;
            space.admission_epoch = generation;
            for app in &mut space.apps {
                app.attachment_epoch = generation;
            }
        }
        Change::DeleteDataset {
            app, collection, ..
        } => {
            if registry.deletions.len() == MAX_DELETIONS {
                return Err(Error::Capacity);
            }
            let space = &mut registry.spaces[index];
            if let Some(slot) = space.apps.iter().position(|a| {
                a.policy.app == *app
                    && a.collection == *collection
                    && (!a.attached || space.archived)
            }) {
                space.apps.remove(slot);
            } else {
                let backup = registry
                    .retained
                    .iter()
                    .position(|d| {
                        d.space == *id && d.policy.app == *app && d.collection == *collection
                    })
                    .ok_or(Error::Conflict)?;
                registry.retained.remove(backup);
            }
            registry.deletions.push(Deletion {
                space: id.clone(),
                collection: collection.clone(),
                generation,
            });
        }
        Change::RestoreDataset {
            app, collection, ..
        } => {
            let space = &mut registry.spaces[index];
            if space.archived {
                return Err(Error::Unavailable);
            }
            let policy = installed
                .filter(|p| p.app == *app && valid_policy(p))
                .ok_or(Error::Unavailable)?;
            if policy.closed_only && space.audience != Audience::Closed {
                return Err(Error::Denied);
            }
            if let Some(current) = space
                .apps
                .iter_mut()
                .find(|a| a.policy.app == *app && a.collection == *collection)
            {
                if current.attached {
                    return Err(Error::Conflict);
                }
                if current.policy.schema != policy.schema {
                    return Err(Error::Incompatible);
                }
                current.attached = true;
                current.policy = policy.clone();
                current.attachment_epoch = generation;
            } else {
                let source = registry
                    .retained
                    .iter()
                    .find(|d| d.space == *id && d.policy.app == *app && d.collection == *collection)
                    .cloned()
                    .ok_or(Error::Conflict)?;
                if source.policy.schema != policy.schema {
                    return Err(Error::Incompatible);
                }
                if let Some(slot) = space.apps.iter().position(|a| a.policy.app == *app) {
                    if registry.retained.len() == MAX_RETAINED_DATASETS
                        || space.apps.len()
                            + registry.retained.iter().filter(|d| d.space == *id).count()
                            == MAX_APPS
                    {
                        return Err(Error::Capacity);
                    }
                    let old = &space.apps[slot];
                    registry.retained.push(RetainedDataset {
                        space: id.clone(),
                        policy: old.policy.clone(),
                        collection: old.collection.clone(),
                        label: old.label.clone(),
                        origin: DatasetOrigin::BeforeRestore,
                        generation,
                    });
                    space.apps[slot] = AppData {
                        policy: policy.clone(),
                        collection: fresh()?,
                        attached: true,
                        attachment_epoch: generation,
                        label: source.label,
                    };
                } else {
                    if space.apps.len() == MAX_APPS {
                        return Err(Error::Capacity);
                    }
                    space.apps.push(AppData {
                        policy: policy.clone(),
                        collection: fresh()?,
                        attached: true,
                        attachment_epoch: generation,
                        label: source.label,
                    });
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(registry: &Registry, change: Change) -> Request {
        Request {
            version: VERSION,
            expected_generation: registry.generation,
            change,
        }
    }
    fn seed() -> (Registry, String, AppPolicy) {
        let mut r = empty();
        r = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::Create {
                    name: "Home".into(),
                },
            ),
            None,
        )
        .unwrap();
        let id = r.spaces[0].id.clone();
        let p = AppPolicy {
            app: "notes".into(),
            schema: "b".repeat(64),
            closed_only: false,
            editable: true,
            comments: false,
        };
        r = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::Attach {
                    space: id.clone(),
                    app: p.app.clone(),
                },
            ),
            Some(&p),
        )
        .unwrap();
        (r, id, p)
    }
    fn backup(r: &mut Registry, space: &str, p: &AppPolicy) -> String {
        let id = "f".repeat(32);
        r.retained.push(RetainedDataset {
            space: space.into(),
            policy: p.clone(),
            collection: id.clone(),
            label: "Old Notes".into(),
            origin: DatasetOrigin::BeforeReplacement,
            generation: r.generation,
        });
        validate(r).unwrap();
        id
    }
    #[test]
    fn ordinary_removed_dataset_restores_its_exact_identity_and_fences_old_binding() {
        let (r, id, p) = seed();
        let old = bind(&r, Some("alice"), &id, &p.app, &p.schema).unwrap();
        let r = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::Remove {
                    space: id.clone(),
                    app: p.app.clone(),
                },
            ),
            None,
        )
        .unwrap();
        let restored = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::RestoreDataset {
                    space: id.clone(),
                    app: p.app.clone(),
                    collection: old.collection.clone(),
                },
            ),
            Some(&p),
        )
        .unwrap();
        let new = bind(&restored, Some("alice"), &id, &p.app, &p.schema).unwrap();
        assert_eq!(new.collection, old.collection);
        assert_ne!(new.attachment_epoch, old.attachment_epoch);
        assert!(authorize(&restored, Some("alice"), &old, Operation::Edit).is_err());
        assert!(authorize(&restored, Some("alice"), &new, Operation::Edit).is_ok());
    }
    #[test]
    fn sealed_backup_restore_keeps_originals_and_uses_a_fresh_active_collection() {
        let (mut r, id, p) = seed();
        let old = r.spaces[0].apps[0].collection.clone();
        let source = backup(&mut r, &id, &p);
        let restored = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::RestoreDataset {
                    space: id.clone(),
                    app: p.app.clone(),
                    collection: source.clone(),
                },
            ),
            Some(&p),
        )
        .unwrap();
        let current = &restored.spaces[0].apps[0];
        assert!(current.attached);
        assert_ne!(current.collection, source);
        assert_ne!(current.collection, old);
        assert_eq!(restored.retained.len(), 2);
        assert!(
            restored
                .retained
                .iter()
                .any(|d| d.collection == old && d.origin == DatasetOrigin::BeforeRestore)
        );
        assert!(restored.retained.iter().any(|d| d.collection == source));
        assert_eq!(restored.spaces[0].apps.len(), 1);
    }
    #[test]
    fn archived_space_revokes_every_app_but_preserves_data_and_current_authorization() {
        let (r, id, p) = seed();
        let old = bind(&r, Some("alice"), &id, &p.app, &p.schema).unwrap();
        let archived = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::DeleteSpace {
                    space: id.clone(),
                    remove_data: false,
                },
            ),
            None,
        )
        .unwrap();
        assert!(archived.spaces[0].archived);
        assert_eq!(archived.spaces[0].apps[0].collection, old.collection);
        assert!(!visible(&archived.spaces[0], None));
        assert!(visible(&archived.spaces[0], Some("alice")));
        assert!(authorize(&archived, Some("alice"), &old, Operation::Read).is_err());
        assert!(bind(&archived, Some("alice"), &id, &p.app, &p.schema).is_err());
        assert!(
            apply(
                &archived,
                "visitor",
                &request(&archived, Change::RestoreSpace { space: id.clone() }),
                None
            )
            .is_err()
        );
        let restored = apply(
            &archived,
            "alice",
            &request(&archived, Change::RestoreSpace { space: id.clone() }),
            None,
        )
        .unwrap();
        assert_eq!(restored.spaces[0].apps[0].collection, old.collection);
        assert!(!restored.spaces[0].archived);
        assert!(authorize(&restored, Some("alice"), &old, Operation::Edit).is_err());
    }
    #[test]
    fn whole_space_data_deletion_tombstones_all_datasets_without_losing_recovery_acl() {
        let (mut r, id, p) = seed();
        let current = r.spaces[0].apps[0].collection.clone();
        let retained = backup(&mut r, &id, &p);
        let deleted = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::DeleteSpace {
                    space: id.clone(),
                    remove_data: true,
                },
            ),
            None,
        )
        .unwrap();
        assert!(deleted.spaces[0].archived);
        assert!(deleted.spaces[0].apps.is_empty());
        assert!(deleted.retained.is_empty());
        assert_eq!(deleted.spaces[0].members, r.spaces[0].members);
        for collection in [current, retained] {
            assert!(deleted.deletions.iter().any(|d| d.collection == collection));
        }
        let restored = apply(
            &deleted,
            "alice",
            &request(&deleted, Change::RestoreSpace { space: id.clone() }),
            None,
        )
        .unwrap();
        assert!(restored.spaces[0].apps.is_empty());
        assert_eq!(restored.deletions, deleted.deletions);
    }
    #[test]
    fn ghost_delete_requires_exact_inactive_dataset_and_never_deletes_active_data() {
        let (mut r, id, p) = seed();
        let active = r.spaces[0].apps[0].collection.clone();
        let retained = backup(&mut r, &id, &p);
        assert!(
            apply(
                &r,
                "alice",
                &request(
                    &r,
                    Change::DeleteDataset {
                        space: id.clone(),
                        app: p.app.clone(),
                        collection: active.clone()
                    }
                ),
                None
            )
            .is_err()
        );
        let deleted = apply(
            &r,
            "alice",
            &request(
                &r,
                Change::DeleteDataset {
                    space: id.clone(),
                    app: p.app.clone(),
                    collection: retained.clone(),
                },
            ),
            None,
        )
        .unwrap();
        assert_eq!(deleted.spaces[0].apps[0].collection, active);
        assert!(deleted.retained.is_empty());
        assert_eq!(deleted.deletions[0].collection, retained);
    }
}
