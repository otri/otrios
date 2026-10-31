// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::spaces::{self, Error, Store};
use otrios_interfaces::spaces::*;

fn request(registry: &Registry, change: Change) -> Request {
    Request {
        version: VERSION,
        expected_generation: registry.generation,
        change,
    }
}
fn policy() -> AppPolicy {
    AppPolicy {
        app: "notes".into(),
        schema: "a".repeat(64),
        closed_only: false,
        editable: true,
        comments: true,
    }
}
fn change(registry: &mut Registry, actor: &str, change: Change) {
    *registry =
        spaces::apply(registry, actor, &request(registry, change), Some(&policy())).unwrap();
}
fn fixture() -> Registry {
    let mut registry = spaces::empty();
    change(
        &mut registry,
        "alice",
        Change::Create {
            name: "Household".into(),
        },
    );
    registry
}
fn attach(registry: &mut Registry, space: &str) -> Binding {
    change(
        registry,
        "alice",
        Change::Attach {
            space: space.into(),
            app: "notes".into(),
        },
    );
    spaces::bind(registry, Some("alice"), space, "notes", &policy().schema).unwrap()
}

#[test]
fn identities_are_explicit_and_same_app_has_isolated_collections() {
    let mut r = fixture();
    let household = r.spaces[0].id.clone();
    let a = attach(&mut r, &household);
    change(
        &mut r,
        "alice",
        Change::Create {
            name: "Work".into(),
        },
    );
    let work = r.spaces[1].id.clone();
    let b = attach(&mut r, &work);
    assert_ne!(a.collection, b.collection);
    assert_eq!(r.spaces[0].audience, Audience::Closed);
    for identity in [None, Some("root"), Some("frame"), Some("publisher")] {
        assert_eq!(
            spaces::bind(&r, identity, &household, "notes", &policy().schema),
            Err(Error::Denied)
        );
        assert_eq!(
            spaces::authorize(&r, identity, &a, Operation::Read),
            Err(Error::Denied)
        );
    }
    let mut forged = a.clone();
    forged.space = work;
    assert_eq!(
        spaces::authorize(&r, Some("alice"), &forged, Operation::Edit),
        Err(Error::Denied)
    );
    assert_eq!(
        spaces::bind(&r, Some("alice"), &household, "notes", &"b".repeat(64)),
        Err(Error::Incompatible)
    );
}

#[test]
fn serialized_admin_race_and_member_denial_preserve_last_admin() {
    let mut r = fixture();
    let space = r.spaces[0].id.clone();
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Admin),
        },
    );
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "member".into(),
            role: Some(Role::Member),
        },
    );
    let remove_bob = request(
        &r,
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: None,
        },
    );
    let remove_alice = request(
        &r,
        Change::Membership {
            space: space.clone(),
            identity: "alice".into(),
            role: None,
        },
    );
    assert_eq!(
        spaces::apply(&r, "member", &remove_bob, None),
        Err(Error::Denied)
    );
    r = spaces::apply(&r, "alice", &remove_bob, None).unwrap();
    assert_eq!(
        spaces::apply(&r, "bob", &remove_alice, None),
        Err(Error::Conflict)
    );
    assert_eq!(
        spaces::apply(&r, "alice", &request(&r, remove_alice.change), None),
        Err(Error::LastAdmin)
    );
}

#[test]
fn policy_changes_and_removal_revoke_old_bindings_reattach_preserves_data() {
    let mut r = fixture();
    let space = r.spaces[0].id.clone();
    let old = attach(&mut r, &space);
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    assert_eq!(
        spaces::authorize(&r, Some("alice"), &old, Operation::Read),
        Err(Error::Denied)
    );
    let current = spaces::bind(&r, Some("bob"), &space, "notes", &policy().schema).unwrap();
    spaces::authorize(&r, Some("bob"), &current, Operation::Edit).unwrap();
    change(
        &mut r,
        "alice",
        Change::Remove {
            space: space.clone(),
            app: "notes".into(),
        },
    );
    assert_eq!(
        spaces::authorize(&r, Some("bob"), &current, Operation::Read),
        Err(Error::Denied)
    );
    let fresh = attach(&mut r, &space);
    assert_eq!(fresh.collection, old.collection);
    assert_ne!(fresh.attachment_epoch, current.attachment_epoch);
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: None,
        },
    );
    assert_eq!(
        spaces::bind(&r, Some("bob"), &space, "notes", &policy().schema),
        Err(Error::Denied)
    );
}

#[test]
fn public_comments_are_distinct_from_edits_and_closed_only_apps_block_exposure() {
    let mut r = fixture();
    let space = r.spaces[0].id.clone();
    attach(&mut r, &space);
    for audience in [Audience::PublicView, Audience::PublicComment] {
        change(
            &mut r,
            "alice",
            Change::Audience {
                space: space.clone(),
                audience,
            },
        );
        let b = spaces::bind(&r, None, &space, "notes", &policy().schema).unwrap();
        spaces::authorize(&r, None, &b, Operation::Read).unwrap();
        assert_eq!(
            spaces::authorize(&r, None, &b, Operation::Comment),
            Err(Error::Denied)
        );
        assert_eq!(
            spaces::authorize(&r, Some("visitor"), &b, Operation::Edit),
            Err(Error::Denied)
        );
        assert_eq!(
            spaces::authorize(&r, Some("visitor"), &b, Operation::Comment).is_ok(),
            audience == Audience::PublicComment
        );
        assert_eq!(
            spaces::authorize(&r, Some("visitor"), &b, Operation::Moderate),
            Err(Error::Denied)
        );
    }
    let closed = AppPolicy {
        app: "monitoring".into(),
        closed_only: true,
        editable: false,
        comments: false,
        ..policy()
    };
    let attach = request(
        &r,
        Change::Attach {
            space: space.clone(),
            app: closed.app.clone(),
        },
    );
    assert_eq!(
        spaces::apply(&r, "alice", &attach, Some(&closed)),
        Err(Error::Denied)
    );
    change(
        &mut r,
        "alice",
        Change::Audience {
            space: space.clone(),
            audience: Audience::Closed,
        },
    );
    r = spaces::apply(&r, "alice", &request(&r, attach.change), Some(&closed)).unwrap();
    let b = spaces::bind(&r, Some("alice"), &space, "monitoring", &closed.schema).unwrap();
    assert_eq!(
        spaces::authorize(&r, Some("alice"), &b, Operation::Edit),
        Err(Error::Denied)
    );
    assert_eq!(
        spaces::apply(
            &r,
            "alice",
            &request(
                &r,
                Change::Audience {
                    space,
                    audience: Audience::PublicView
                }
            ),
            None
        ),
        Err(Error::Denied)
    );
}

#[test]
fn deletion_is_atomic_replaces_identity_and_survives_registry_recovery() {
    let path = std::env::temp_dir().join(format!("otrios-spaces-test-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let file = path.join("registry.flex");
    let mut store = Store::open(file.clone()).unwrap();
    store
        .commit(
            "alice",
            &request(
                store.registry().unwrap(),
                Change::Create {
                    name: "Test".into(),
                },
            ),
            None,
        )
        .unwrap();
    let space = store.registry().unwrap().spaces[0].id.clone();
    store
        .commit(
            "alice",
            &request(
                store.registry().unwrap(),
                Change::Attach {
                    space: space.clone(),
                    app: "notes".into(),
                },
            ),
            Some(&policy()),
        )
        .unwrap();
    let b = spaces::bind(
        store.registry().unwrap(),
        Some("alice"),
        &space,
        "notes",
        &policy().schema,
    )
    .unwrap();
    store
        .commit(
            "alice",
            &request(
                store.registry().unwrap(),
                Change::DeleteData {
                    space: space.clone(),
                    app: "notes".into(),
                    remove: false,
                },
            ),
            None,
        )
        .unwrap();
    drop(store);
    let recovered = Store::open(file.clone()).unwrap();
    let r = recovered.registry().unwrap();
    assert_eq!(r.deletions[0].collection, b.collection);
    assert_eq!(
        spaces::authorize(r, Some("alice"), &b, Operation::Edit),
        Err(Error::Denied)
    );
    assert_ne!(
        spaces::bind(r, Some("alice"), &space, "notes", &policy().schema)
            .unwrap()
            .collection,
        b.collection
    );
    let mut bytes = std::fs::read(&file).unwrap();
    bytes[39] ^= 1;
    std::fs::write(&file, bytes).unwrap();
    assert!(Store::open(file).is_err());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn service_bindings_are_distinct_from_membership_and_cannot_survive_collection_replacement() {
    let mut r = fixture();
    let space = r.spaces[0].id.clone();
    let binding = attach(&mut r, &space);
    let service = ServiceBinding {
        identity: "collector".into(),
        binding: binding.clone(),
    };
    assert_eq!(
        spaces::authorize(&r, Some("collector"), &binding, Operation::Edit),
        Err(Error::Denied)
    );
    spaces::authorize_service(&r, "collector", &service).unwrap();
    assert_eq!(
        spaces::authorize_service(&r, "publisher", &service),
        Err(Error::Denied)
    );
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    spaces::authorize_service(&r, "collector", &service).unwrap();
    change(
        &mut r,
        "alice",
        Change::DeleteData {
            space,
            app: "notes".into(),
            remove: false,
        },
    );
    assert_eq!(
        spaces::authorize_service(&r, "collector", &service),
        Err(Error::Denied)
    );
}

#[test]
fn old_bindings_cannot_bypass_current_installed_schema_or_closed_only_policy() {
    let mut r = fixture();
    let space = r.spaces[0].id.clone();
    attach(&mut r, &space);
    change(
        &mut r,
        "alice",
        Change::Audience {
            space: space.clone(),
            audience: Audience::PublicView,
        },
    );
    let b = spaces::bind(&r, Some("alice"), &space, "notes", &policy().schema).unwrap();
    spaces::authorize_installed(&r, Some("alice"), &b, Operation::Read, &policy()).unwrap();
    let closed = AppPolicy {
        closed_only: true,
        ..policy()
    };
    assert_eq!(
        spaces::authorize_installed(&r, Some("alice"), &b, Operation::Read, &closed),
        Err(Error::Incompatible)
    );
    assert_eq!(
        spaces::authorize_installed(&r, None, &b, Operation::Read, &closed),
        Err(Error::Incompatible)
    );
    let changed = AppPolicy {
        schema: "b".repeat(64),
        ..policy()
    };
    assert_eq!(
        spaces::authorize_installed(&r, Some("alice"), &b, Operation::Edit, &changed),
        Err(Error::Incompatible)
    );
    let readonly = AppPolicy {
        editable: false,
        ..policy()
    };
    assert_eq!(
        spaces::authorize_installed(&r, Some("alice"), &b, Operation::Edit, &readonly),
        Err(Error::Denied)
    );
}
