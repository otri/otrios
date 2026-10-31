// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::spaces::{self, Error, Store};
use otrios_interfaces::spaces::*;

#[test]
fn configured_service_principals_cannot_create_spaces_or_acquire_human_roles() {
    let registry = fixture();
    spaces::admit_service_identity(&registry, "collector").unwrap();
    assert_eq!(
        spaces::admit_service_identity(&registry, "alice"),
        Err(Error::Denied)
    );
    let create = request(
        &registry,
        Change::Create {
            name: "Unexpected service Space".into(),
        },
    );
    assert_eq!(
        spaces::admit_human_management("collector", &create, &["collector"]),
        Err(Error::Denied)
    );
    let member = request(
        &registry,
        Change::Membership {
            space: registry.spaces[0].id.clone(),
            identity: "collector".into(),
            role: Some(Role::Admin),
        },
    );
    assert_eq!(
        spaces::admit_human_management("alice", &member, &["collector"]),
        Err(Error::Denied)
    );
    spaces::admit_human_management("alice", &create, &["collector"]).unwrap();
}

#[test]
fn collection_schema_covers_all_distinct_roots_and_is_independent_of_screen_order() {
    use otrios_interfaces::{
        canvas::{Field, StateSchema},
        layout::Value,
    };
    let a = RootSchema {
        state_ref: "app:notes/pages".into(),
        schema: StateSchema {
            id: "pages-v1".into(),
            fields: vec![Field {
                id: 1,
                name: "title".into(),
                default: Value::Text(String::new()),
                writable: true,
            }],
        },
    };
    let mut b = a.clone();
    b.state_ref = "app:notes/settings".into();
    let single = spaces::collection_schema(std::slice::from_ref(&a)).unwrap();
    assert_eq!(
        spaces::collection_schema(&[a.clone(), a.clone()]).unwrap(),
        single
    );
    let both = spaces::collection_schema(&[a.clone(), b.clone()]).unwrap();
    assert_ne!(both, single);
    assert_eq!(
        spaces::collection_schema(&[b.clone(), a.clone()]).unwrap(),
        both
    );
    b.schema.fields[0].writable = false;
    assert_ne!(
        spaces::collection_schema(&[a.clone(), b.clone()]).unwrap(),
        both
    );
    b.state_ref = a.state_ref.clone();
    assert_eq!(
        spaces::collection_schema(&[a.clone(), b]),
        Err(Error::Incompatible)
    );
    assert_eq!(
        spaces::collection_schema(&vec![a.clone(); MAX_ROOTS + 1]),
        Err(Error::Capacity)
    );
    let mut invalid = a;
    invalid.state_ref.push('\0');
    assert_eq!(spaces::collection_schema(&[invalid]), Err(Error::Invalid));
}

#[test]
fn discovery_filters_rosters_retained_data_and_storage_before_encoding() {
    use std::collections::BTreeMap;
    let mut registry = fixture();
    let space = registry.spaces[0].id.clone();
    let binding = attach(&mut registry, &space);
    change(
        &mut registry,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    change(
        &mut registry,
        "alice",
        Change::Remove {
            space: space.clone(),
            app: "notes".into(),
        },
    );
    let installed = vec![InstalledApp {
        policy: policy(),
        title: "Notes".into(),
        ui_ref: "app:notes/ui".into(),
        state_ref: "app:notes/state".into(),
    }];
    let facts = BTreeMap::from([(
        binding.collection.clone(),
        spaces::CollectionFacts {
            stored_bytes: Some(2048),
            available: true,
        },
    )]);
    let cursor = spaces::discovery_cursor().unwrap();
    let snapshot = |identity| {
        spaces::discovery(
            &registry,
            identity,
            Some(&space),
            &installed,
            &facts,
            cursor.clone(),
        )
        .unwrap()
    };
    let admin = snapshot(Some("alice"));
    let detail = admin.selected.as_ref().unwrap();
    assert_eq!(detail.members.as_ref().unwrap().len(), 2);
    assert_eq!(detail.installed, installed);
    assert_eq!(detail.apps[0].collection, binding.collection);
    assert_eq!(detail.apps[0].stored_bytes, Some(2048));
    assert!(!detail.apps[0].attached);
    assert!(detail.apps[0].launch.is_none());
    let member = snapshot(Some("bob"));
    let detail = member.selected.as_ref().unwrap();
    assert!(detail.members.is_none());
    assert!(detail.apps.is_empty());
    assert!(detail.installed.is_empty());
    for identity in [None, Some("stranger")] {
        let hidden = snapshot(identity);
        assert!(hidden.spaces.is_empty());
        assert!(hidden.selected.is_none());
        let bytes = spaces::encode_event(&Event::Snapshot {
            observation: 1,
            snapshot: hidden,
            resync: true,
        })
        .unwrap();
        for private in ["Household", "alice", "notes", &binding.collection] {
            assert!(
                !bytes
                    .windows(private.len())
                    .any(|w| w == private.as_bytes())
            );
        }
    }
    for snapshot in [admin, member] {
        let event = Event::Snapshot {
            observation: 1,
            snapshot,
            resync: true,
        };
        assert_eq!(
            spaces::decode_event(&spaces::encode_event(&event).unwrap()).unwrap(),
            event
        );
    }
}

#[test]
fn discovery_preserves_unavailable_or_incompatible_collections_without_a_launch_grant() {
    use std::collections::BTreeMap;
    let mut registry = fixture();
    let space = registry.spaces[0].id.clone();
    let binding = attach(&mut registry, &space);
    let mut installed = InstalledApp {
        policy: policy(),
        title: "Notes".into(),
        ui_ref: "app:notes/ui".into(),
        state_ref: "app:notes/state".into(),
    };
    let facts = BTreeMap::from([(
        binding.collection.clone(),
        spaces::CollectionFacts {
            stored_bytes: Some(200),
            available: true,
        },
    )]);
    for expected in [
        AppAvailability::Unavailable,
        AppAvailability::Incompatible,
        AppAvailability::Available,
    ] {
        installed.policy.schema = if expected == AppAvailability::Incompatible {
            "b".repeat(64)
        } else {
            policy().schema
        };
        let apps = if expected == AppAvailability::Unavailable {
            vec![]
        } else {
            vec![installed.clone()]
        };
        let snapshot = spaces::discovery(
            &registry,
            Some("alice"),
            Some(&space),
            &apps,
            &facts,
            spaces::discovery_cursor().unwrap(),
        )
        .unwrap();
        let row = &snapshot.selected.unwrap().apps[0];
        assert_eq!(row.availability, expected);
        assert_eq!(row.collection, binding.collection);
        assert_eq!(row.stored_bytes, Some(200));
        assert_eq!(row.launch.is_some(), expected == AppAvailability::Available);
    }
}

#[test]
fn management_wire_bounds_versions_shapes_and_untrusted_flex_before_deserialization() {
    let command = Command::Manage {
        version: 1,
        id: 1,
        request: Request {
            version: 1,
            expected_generation: 0,
            change: Change::Create {
                name: "Space".into(),
            },
        },
    };
    let bytes = spaces::encode_command(&command).unwrap();
    assert_eq!(spaces::decode_command(&bytes).unwrap(), command);
    for end in 0..bytes.len() {
        assert!(spaces::decode_command(&bytes[..end]).is_err());
    }
    let mut huge = bytes.clone();
    huge.resize(MAX_COMMAND_BYTES + 1, 0);
    assert_eq!(spaces::decode_command(&huge), Err(Error::Capacity));
    let invalid = Command::Watch {
        observation: 1,
        version: 2,
        selected: None,
        cursor: None,
    };
    assert_eq!(spaces::encode_command(&invalid), Err(Error::Invalid));
    let invalid = Command::Watch {
        observation: 1,
        version: 1,
        selected: Some("invalid".into()),
        cursor: None,
    };
    assert_eq!(spaces::encode_command(&invalid), Err(Error::Invalid));
    for event in [
        Event::Unavailable { version: 1 },
        Event::ResyncRequired { version: 1 },
        Event::Result {
            version: 1,
            id: 9,
            status: ManagementStatus::LastAdmin,
            created: None,
        },
    ] {
        assert_eq!(
            spaces::decode_event(&spaces::encode_event(&event).unwrap()).unwrap(),
            event
        );
    }
    // The guard, rather than a panicking FlexBuffers accessor, sees malformed roots.
    for size in 0..256 {
        let mut bytes = b"OTriOS\0spaces-command\0v1\0".to_vec();
        bytes.extend(vec![255; size]);
        assert!(spaces::decode_command(&bytes).is_err());
    }
}

fn request(registry: &Registry, change: Change) -> Request {
    Request {
        version: VERSION,
        expected_generation: registry.generation,
        change,
    }
}

#[test]
fn maximum_selected_space_metadata_fits_one_bounded_snapshot_without_document_payloads() {
    use std::collections::BTreeMap;
    let mut registry = fixture();
    let mut selected = registry.spaces[0].clone();
    selected.name = "S".repeat(128);
    for index in 1..MAX_MEMBERS {
        selected.members.push(Member {
            identity: format!("{index:03}{}", "m".repeat(93)),
            role: Role::Member,
            joined_generation: 1,
        });
    }
    let mut installed = vec![];
    let mut facts = BTreeMap::new();
    for index in 0..MAX_APPS {
        let app = format!(
            "app-{}{}{}",
            "a".repeat(58),
            char::from(b'a' + (index / 26) as u8),
            char::from(b'a' + (index % 26) as u8)
        );
        let mut policy = policy();
        policy.app = app.clone();
        let collection = format!("{:032x}", index + 1);
        selected.apps.push(AppData {
            policy: policy.clone(),
            collection: collection.clone(),
            attached: true,
            attachment_epoch: 1,
        });
        facts.insert(
            collection,
            spaces::CollectionFacts {
                stored_bytes: Some(u64::MAX),
                available: true,
            },
        );
        if index < MAX_INSTALLED_APPS {
            installed.push(InstalledApp {
                policy,
                title: "T".repeat(96),
                ui_ref: format!("app:{app}/ui"),
                state_ref: "r".repeat(128),
            });
        }
    }
    registry.spaces[0] = selected;
    for index in 1..MAX_SPACES {
        let mut space = registry.spaces[0].clone();
        space.id = format!("{:032x}", 10000 + index);
        space.apps.clear();
        space.members.truncate(1);
        registry.spaces.push(space);
    }
    spaces::validate(&registry).unwrap();
    let snapshot = spaces::discovery(
        &registry,
        Some("alice"),
        Some(&registry.spaces[0].id),
        &installed,
        &facts,
        spaces::discovery_cursor().unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.spaces.len(), MAX_SPACES);
    let detail = snapshot.selected.as_ref().unwrap();
    assert_eq!(detail.apps.len(), MAX_APPS);
    assert_eq!(detail.members.as_ref().unwrap().len(), MAX_MEMBERS);
    let event = Event::Snapshot {
        observation: 1,
        snapshot,
        resync: true,
    };
    let bytes = spaces::encode_event(&event).unwrap();
    assert!(bytes.len() <= MAX_EVENT_BYTES);
    assert_eq!(spaces::decode_event(&bytes).unwrap(), event);
}

#[test]
fn observer_refreshes_on_reconnect_and_never_replays_uncertain_management() {
    use std::collections::BTreeMap;
    let registry = fixture();
    let space = &registry.spaces[0].id;
    let snapshot = spaces::discovery(
        &registry,
        Some("alice"),
        Some(space),
        &[],
        &BTreeMap::new(),
        spaces::discovery_cursor().unwrap(),
    )
    .unwrap();
    let event = spaces::encode_event(&Event::Snapshot {
        observation: 1,
        snapshot: snapshot.clone(),
        resync: true,
    })
    .unwrap();
    let mut observer = spaces::Observer::default();
    assert!(observer.outgoing().unwrap().is_none());
    observer.configure(Some(space)).unwrap();
    assert!(matches!(
        spaces::decode_command(&observer.outgoing().unwrap().unwrap()).unwrap(),
        Command::Watch { cursor: None, .. }
    ));
    assert!(observer.receive(&event).unwrap());
    let id = observer
        .request(Change::Create {
            name: "Second".into(),
        })
        .unwrap();
    assert_eq!(
        observer.request(Change::Create {
            name: "Duplicate".into()
        }),
        Err(Error::Capacity)
    );
    assert!(
        matches!(spaces::decode_command(&observer.outgoing().unwrap().unwrap()).unwrap(), Command::Manage { id: actual, .. } if actual == id)
    );
    observer.reset();
    assert!(
        !observer.receive(&event).unwrap(),
        "an old response cannot satisfy a new observation"
    );
    assert_eq!(
        observer.take_outcome(),
        Some(spaces::Outcome::Unknown { id })
    );
    assert!(!observer.pending());
    assert!(
        matches!(spaces::decode_command(&observer.outgoing().unwrap().unwrap()).unwrap(), Command::Watch { cursor: Some(cursor), .. } if cursor == snapshot.cursor)
    );
    assert_eq!(
        observer.request(Change::Create {
            name: "Unvalidated".into()
        }),
        Err(Error::Unavailable)
    );
    assert!(observer.outgoing().unwrap().is_none());
    assert!(!observer.receive(&event).unwrap());
    observer
        .receive(
            &spaces::encode_event(&Event::Snapshot {
                observation: 2,
                snapshot: snapshot.clone(),
                resync: true,
            })
            .unwrap(),
        )
        .unwrap();
    let next = observer
        .request(Change::Create {
            name: "Deliberate retry".into(),
        })
        .unwrap();
    assert_ne!(id, next);
    observer.outgoing().unwrap().unwrap();
    let applied = spaces::encode_event(&Event::Result {
        version: 1,
        id: next,
        status: ManagementStatus::Applied,
        created: Some("a".repeat(32)),
    })
    .unwrap();
    observer.receive(&applied).unwrap();
    assert!(matches!(
        observer.take_outcome(),
        Some(spaces::Outcome::Decided {
            status: ManagementStatus::Applied,
            ..
        })
    ));
    observer
        .receive(&spaces::encode_event(&Event::ResyncRequired { version: 1 }).unwrap())
        .unwrap();
    assert!(observer.snapshot().is_none());
    assert!(matches!(
        spaces::decode_command(&observer.outgoing().unwrap().unwrap()).unwrap(),
        Command::Watch { cursor: None, .. }
    ));
    observer
        .receive(&spaces::encode_event(&Event::Unavailable { version: 1 }).unwrap())
        .unwrap();
    assert!(observer.unavailable());
    assert!(observer.outgoing().unwrap().is_none());
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
fn revocation_is_scoped_and_removal_reattach_preserves_data() {
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
        Ok(())
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
    drop(recovered);
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

#[test]
fn removing_and_readmitting_a_member_cannot_reactivate_their_previous_mount() {
    let mut r = fixture();
    let space = r.spaces[0].id.clone();
    attach(&mut r, &space);
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Member),
        },
    );
    let old = spaces::bind(&r, Some("bob"), &space, "notes", &policy().schema).unwrap();
    let admin = spaces::bind(&r, Some("alice"), &space, "notes", &policy().schema).unwrap();
    change(
        &mut r,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: None,
        },
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
    assert_eq!(
        spaces::authorize(&r, Some("bob"), &old, Operation::Read),
        Err(Error::Denied)
    );
    spaces::authorize(&r, Some("alice"), &admin, Operation::Edit).unwrap();
    let fresh = spaces::bind(&r, Some("bob"), &space, "notes", &policy().schema).unwrap();
    spaces::authorize(&r, Some("bob"), &fresh, Operation::Edit).unwrap();
}

#[test]
fn initial_registry_framing_recovers_older_members_and_upgrades_on_commit() {
    let path = std::env::temp_dir().join(format!("otrios-spaces-legacy-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let file = path.join("registry.flex");
    let mut value = serde_json::to_value(fixture()).unwrap();
    for space in value["spaces"].as_array_mut().unwrap() {
        for member in space["members"].as_array_mut().unwrap() {
            member.as_object_mut().unwrap().remove("joined_generation");
        }
    }
    let payload = flexbuffers::to_vec(&value).unwrap();
    let mut bytes = b"OTSP1".to_vec();
    bytes.extend(otrios_core::canvas::hash(&payload));
    bytes.extend(payload);
    std::fs::write(&file, bytes).unwrap();
    let mut store = Store::open(file.clone()).unwrap();
    let r = store.registry().unwrap();
    assert_eq!(r.spaces[0].members[0].joined_generation, 0);
    let space = r.spaces[0].id.clone();
    store
        .commit(
            "alice",
            &request(
                r,
                Change::Membership {
                    space,
                    identity: "bob".into(),
                    role: Some(Role::Member),
                },
            ),
            None,
        )
        .unwrap();
    assert_eq!(&std::fs::read(&file).unwrap()[..5], b"OTSP5");
    std::fs::remove_dir_all(path).unwrap();
}
