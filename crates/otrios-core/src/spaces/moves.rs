// SPDX-License-Identifier: LGPL-3.0-only
//! Scoped cross-Space decisions. Store stages owned bytes before publishing them.
use super::*;

pub(super) fn mutate(
    registry: &mut Registry,
    actor: &str,
    change: &Change,
    installed: Option<&AppPolicy>,
) -> Result<bool> {
    let Change::Move {
        operation,
        source,
        destination,
        app,
        collection,
        destination_collection,
        mode,
    } = change
    else {
        return Ok(false);
    };
    if !valid_id(operation) || source == destination {
        return Err(Error::Invalid);
    }
    let from = registry
        .spaces
        .iter()
        .position(|s| s.id == *source)
        .ok_or(Error::Denied)?;
    let to = registry
        .spaces
        .iter()
        .position(|s| s.id == *destination)
        .ok_or(Error::Denied)?;
    admin(&registry.spaces[from], actor)?;
    admin(&registry.spaces[to], actor)?;
    if registry.spaces[to].archived {
        return Err(Error::Unavailable);
    }
    let generation = registry.generation;
    let preferred = registry.spaces[from]
        .apps
        .iter()
        .find(|a| a.policy.app == *app && a.collection == *collection)
        .cloned();
    let sealed = registry
        .retained
        .iter()
        .find(|d| d.space == *source && d.policy.app == *app && d.collection == *collection)
        .cloned();
    let original = preferred
        .clone()
        .or_else(|| {
            sealed.as_ref().map(|d| AppData {
                policy: d.policy.clone(),
                collection: d.collection.clone(),
                label: d.label.clone(),
                attached: false,
                attachment_epoch: generation,
            })
        })
        .ok_or(Error::Conflict)?;
    let attached =
        preferred.as_ref().is_some_and(|a| a.attached) && !registry.spaces[from].archived;
    if *mode != MoveMode::DataOnly && !attached {
        return Err(Error::Denied);
    }
    let policy = installed
        .filter(|p| p.app == *app && valid_policy(p))
        .ok_or(Error::Unavailable)?;
    if original.policy.schema != policy.schema {
        return Err(Error::Incompatible);
    }
    let target = registry.spaces[to]
        .apps
        .iter()
        .find(|a| a.policy.app == *app)
        .cloned();
    if target.as_ref().map(|a| &a.collection) != destination_collection.as_ref() {
        return Err(Error::Conflict);
    }
    if target
        .as_ref()
        .is_some_and(|a| a.policy.schema != policy.schema)
    {
        return Err(Error::Incompatible);
    }
    let public = registry.spaces[to].audience != Audience::Closed;
    if public
        && (policy.closed_only
            || (*mode != MoveMode::AppOnly && original.policy.closed_only)
            || target.as_ref().is_some_and(|a| a.policy.closed_only))
    {
        return Err(Error::Denied);
    }
    if *mode == MoveMode::AppOnly {
        let current = registry.spaces[from]
            .apps
            .iter_mut()
            .find(|a| a.collection == *collection)
            .ok_or(Error::Conflict)?;
        current.attached = false;
        current.attachment_epoch = generation;
        if let Some(existing) = registry.spaces[to]
            .apps
            .iter_mut()
            .find(|a| a.policy.app == *app)
        {
            if !existing.attached {
                existing.attached = true;
                existing.attachment_epoch = generation;
                existing.policy = policy.clone();
            }
        } else {
            registry.spaces[to].apps.push(AppData {
                policy: policy.clone(),
                collection: fresh()?,
                attached: true,
                attachment_epoch: generation,
                label: original.label,
            });
        }
    } else {
        if registry.deletions.len() == MAX_DELETIONS {
            return Err(Error::Capacity);
        }
        if sealed.is_some() {
            registry.retained.retain(|d| d.collection != *collection);
        } else if *mode == MoveMode::DataOnly && attached {
            let current = registry.spaces[from]
                .apps
                .iter_mut()
                .find(|a| a.collection == *collection)
                .ok_or(Error::Conflict)?;
            current.collection = fresh()?;
            current.attachment_epoch = generation;
        } else {
            registry.spaces[from]
                .apps
                .retain(|a| a.collection != *collection);
        }
        if let Some(outgoing) = &target {
            registry.retained.push(RetainedDataset {
                space: destination.clone(),
                policy: outgoing.policy.clone(),
                collection: outgoing.collection.clone(),
                label: outgoing.label.clone(),
                origin: DatasetOrigin::BeforeReplacement,
                generation,
            });
        }
        let next = AppData {
            policy: policy.clone(),
            collection: fresh()?,
            attached: *mode == MoveMode::AppAndData || target.as_ref().is_some_and(|a| a.attached),
            attachment_epoch: generation,
            label: target
                .as_ref()
                .map_or_else(|| original.label.clone(), |a| a.label.clone()),
        };
        if let Some(slot) = registry.spaces[to]
            .apps
            .iter()
            .position(|a| a.policy.app == *app)
        {
            registry.spaces[to].apps[slot] = next;
        } else {
            registry.spaces[to].apps.push(next);
        }
        registry.deletions.push(Deletion {
            space: source.clone(),
            collection: collection.clone(),
            generation,
        });
    }
    if registry.retained.len() > MAX_RETAINED_DATASETS
        || registry.spaces.iter().any(|s| {
            s.apps.len() + registry.retained.iter().filter(|d| d.space == s.id).count() > MAX_APPS
        })
    {
        return Err(Error::Capacity);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn change(r: &Registry, c: Change, p: Option<&AppPolicy>) -> Registry {
        apply(
            r,
            "alice",
            &Request {
                version: VERSION,
                expected_generation: r.generation,
                change: c,
            },
            p,
        )
        .unwrap()
    }
    fn seed(occupied: bool) -> (Registry, String, String, AppPolicy) {
        let mut r = empty();
        r = change(
            &r,
            Change::Create {
                name: "Home".into(),
            },
            None,
        );
        r = change(
            &r,
            Change::Create {
                name: "Work".into(),
            },
            None,
        );
        let source = r.spaces[0].id.clone();
        let destination = r.spaces[1].id.clone();
        let policy = AppPolicy {
            app: "notes".into(),
            schema: "b".repeat(64),
            closed_only: false,
            editable: true,
            comments: false,
        };
        r = change(
            &r,
            Change::Attach {
                space: source.clone(),
                app: policy.app.clone(),
            },
            Some(&policy),
        );
        if occupied {
            r = change(
                &r,
                Change::Attach {
                    space: destination.clone(),
                    app: policy.app.clone(),
                },
                Some(&policy),
            );
        }
        (r, source, destination, policy)
    }
    fn request(r: &Registry, source: &str, destination: &str, mode: MoveMode) -> Request {
        Request {
            version: VERSION,
            expected_generation: r.generation,
            change: Change::Move {
                operation: "d".repeat(32),
                source: source.into(),
                destination: destination.into(),
                app: "notes".into(),
                collection: r.spaces[0].apps[0].collection.clone(),
                destination_collection: r.spaces[1].apps.first().map(|a| a.collection.clone()),
                mode,
            },
        }
    }
    #[test]
    fn app_and_data_choices_preserve_unselected_data_and_never_duplicate_an_active_bundle() {
        for mode in [MoveMode::AppAndData, MoveMode::AppOnly, MoveMode::DataOnly] {
            for occupied in [false, true] {
                let (r, source, destination, p) = seed(occupied);
                let old = r.spaces[0].apps[0].collection.clone();
                let target = r.spaces[1].apps.first().cloned();
                let old_target = occupied
                    .then(|| bind(&r, Some("alice"), &destination, &p.app, &p.schema).unwrap());
                let next = apply(
                    &r,
                    "alice",
                    &request(&r, &source, &destination, mode),
                    Some(&p),
                )
                .unwrap();
                let arrived = &next.spaces[1].apps[0];
                assert_eq!(next.spaces[0].members, r.spaces[0].members);
                assert_eq!(next.spaces[1].members, r.spaces[1].members);
                assert_eq!(next.spaces[1].apps.len(), 1);
                assert_eq!(arrived.attached, mode != MoveMode::DataOnly || occupied);
                if mode == MoveMode::AppOnly {
                    assert!(!next.spaces[0].apps[0].attached);
                    assert_eq!(next.spaces[0].apps[0].collection, old);
                    assert!(next.deletions.is_empty());
                    if let Some(binding) = old_target {
                        assert_eq!(binding.collection, arrived.collection);
                        authorize(&next, Some("alice"), &binding, Operation::Edit).unwrap();
                    }
                } else {
                    assert_ne!(arrived.collection, old);
                    assert!(next.deletions.iter().any(|d| d.collection == old));
                    if let Some(target) = target {
                        assert_ne!(arrived.collection, target.collection);
                        assert!(
                            next.retained
                                .iter()
                                .any(|d| d.collection == target.collection
                                    && d.origin == DatasetOrigin::BeforeReplacement)
                        );
                    }
                    if mode == MoveMode::DataOnly {
                        assert!(next.spaces[0].apps[0].attached);
                        assert_ne!(next.spaces[0].apps[0].collection, old);
                    } else {
                        assert!(next.spaces[0].apps.is_empty());
                    }
                }
            }
        }
    }
    #[test]
    fn a_preferred_or_sealed_ghost_moves_only_its_selected_dataset() {
        for sealed in [false, true] {
            let (mut r, source, destination, p) = seed(false);
            let cid = r.spaces[0].apps[0].collection.clone();
            if sealed {
                let a = r.spaces[0].apps.remove(0);
                r.retained.push(RetainedDataset {
                    space: source.clone(),
                    policy: a.policy,
                    collection: a.collection,
                    label: a.label,
                    origin: DatasetOrigin::BeforeRestore,
                    generation: r.generation,
                });
            } else {
                r = change(
                    &r,
                    Change::Remove {
                        space: source.clone(),
                        app: p.app.clone(),
                    },
                    None,
                );
            }
            let request = Request {
                version: VERSION,
                expected_generation: r.generation,
                change: Change::Move {
                    operation: "d".repeat(32),
                    source: source.clone(),
                    destination: destination.clone(),
                    app: p.app.clone(),
                    collection: cid.clone(),
                    destination_collection: None,
                    mode: MoveMode::DataOnly,
                },
            };
            let next = apply(&r, "alice", &request, Some(&p)).unwrap();
            assert!(next.spaces[0].apps.is_empty());
            assert!(next.retained.is_empty());
            assert!(!next.spaces[1].apps[0].attached);
            assert_ne!(next.spaces[1].apps[0].collection, cid);
        }
    }
    #[test]
    fn move_checks_both_admins_exact_bindings_schema_and_publication_policy_before_any_change() {
        let (r, source, destination, p) = seed(true);
        let req = request(&r, &source, &destination, MoveMode::AppAndData);
        assert_eq!(apply(&r, "bob", &req, Some(&p)).unwrap_err(), Error::Denied);
        let mut no_admin = r.clone();
        no_admin.spaces[1].members[0].identity = "bob".into();
        assert_eq!(
            apply(&no_admin, "alice", &req, Some(&p)).unwrap_err(),
            Error::Denied
        );
        let mut stale = req.clone();
        stale.expected_generation -= 1;
        assert_eq!(
            apply(&r, "alice", &stale, Some(&p)).unwrap_err(),
            Error::Conflict
        );
        let mut changed = r.clone();
        changed.spaces[1].apps[0].collection = "e".repeat(32);
        assert_eq!(
            apply(&changed, "alice", &req, Some(&p)).unwrap_err(),
            Error::Conflict
        );
        let mut incompatible = r.clone();
        incompatible.spaces[1].apps[0].policy.schema = "c".repeat(64);
        assert_eq!(
            apply(&incompatible, "alice", &req, Some(&p)).unwrap_err(),
            Error::Incompatible
        );
        let mut publication = r.clone();
        publication.spaces[0].apps[0].policy.closed_only = true;
        publication.spaces[1].audience = Audience::PublicView;
        assert_eq!(
            apply(&publication, "alice", &req, Some(&p)).unwrap_err(),
            Error::Denied
        );
        let code_only = request(&publication, &source, &destination, MoveMode::AppOnly);
        assert!(apply(&publication, "alice", &code_only, Some(&p)).is_ok());
        assert_eq!(r.spaces[0].apps.len(), 1);
        assert!(r.retained.is_empty());
    }
}
