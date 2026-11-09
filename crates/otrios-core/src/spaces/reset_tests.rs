// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

fn request(registry: &Registry, change: Change) -> Request {
    Request {
        version: VERSION,
        expected_generation: registry.generation,
        change,
    }
}
fn fixture() -> (Registry, AppPolicy) {
    let mut registry = empty();
    registry = apply(
        &registry,
        "alice",
        &request(
            &registry,
            Change::Create {
                name: "Home".into(),
            },
        ),
        None,
    )
    .unwrap();
    let policy = AppPolicy {
        app: "notes".into(),
        schema: "b".repeat(64),
        closed_only: false,
        editable: true,
        comments: false,
    };
    registry = apply(
        &registry,
        "alice",
        &request(
            &registry,
            Change::Attach {
                space: registry.spaces[0].id.clone(),
                app: policy.app.clone(),
            },
        ),
        Some(&policy),
    )
    .unwrap();
    (registry, policy)
}
fn reset(registry: &Registry) -> Request {
    request(
        registry,
        Change::DeleteData {
            space: registry.spaces[0].id.clone(),
            app: "notes".into(),
            collection: registry.spaces[0].apps[0].collection.clone(),
            remove: false,
        },
    )
}

#[test]
fn explicit_reset_atomically_adopts_installed_schema_and_retires_old_bindings() {
    let (registry, mut installed) = fixture();
    let old = bind(
        &registry,
        Some("alice"),
        &registry.spaces[0].id,
        "notes",
        &installed.schema,
    )
    .unwrap();
    let old_actor = actor_context(&registry, Some("alice"), &old).unwrap();
    installed.schema = "c".repeat(64);
    let next = apply_catalog(&registry, "alice", &reset(&registry), &[installed.clone()]).unwrap();
    let data = &next.spaces[0].apps[0];
    assert_eq!(next.generation, registry.generation + 1);
    assert_eq!(data.policy, installed);
    assert!(data.attached);
    assert_ne!(data.collection, old.collection);
    assert_eq!(next.deletions[0].collection, old.collection);
    assert!(authorize_installed(&next, Some("alice"), &old, Operation::Edit, &installed).is_err());
    let fresh = bind(&next, Some("alice"), &old.space, "notes", &installed.schema).unwrap();
    assert_ne!(
        actor_context(&next, Some("alice"), &fresh).unwrap(),
        old_actor
    );
    assert_eq!(
        registry.spaces[0].apps[0].collection, old.collection,
        "pinned registry remains intact"
    );
    assert_eq!(
        apply_catalog(&next, "alice", &reset(&registry), &[installed]),
        Err(Error::Conflict)
    );
}

#[test]
fn reset_checks_admin_collection_identity_and_new_policy_audience() {
    let (registry, mut installed) = fixture();
    installed.schema = "c".repeat(64);
    assert_eq!(
        apply_catalog(&registry, "bob", &reset(&registry), &[installed.clone()]),
        Err(Error::Denied)
    );
    let mut stale = reset(&registry);
    if let Change::DeleteData { collection, .. } = &mut stale.change {
        *collection = "a".repeat(32);
    }
    assert_eq!(
        apply_catalog(&registry, "alice", &stale, &[installed.clone()]),
        Err(Error::Conflict)
    );
    let public = apply(
        &registry,
        "alice",
        &request(
            &registry,
            Change::Audience {
                space: registry.spaces[0].id.clone(),
                audience: Audience::PublicView,
            },
        ),
        None,
    )
    .unwrap();
    installed.closed_only = true;
    assert_eq!(
        apply_catalog(&public, "alice", &reset(&public), &[installed]),
        Err(Error::Denied)
    );
}

#[test]
fn reset_without_an_installed_app_preserves_its_declared_policy() {
    let (registry, policy) = fixture();
    let next = apply_catalog(&registry, "alice", &reset(&registry), &[]).unwrap();
    assert_eq!(next.spaces[0].apps[0].policy, policy);
    assert_ne!(
        next.spaces[0].apps[0].collection,
        registry.spaces[0].apps[0].collection
    );
}
