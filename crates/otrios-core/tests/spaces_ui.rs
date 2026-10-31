// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::{authoring as a, canvas::*, spaces};
use otrios_interfaces::{canvas::*, keyboard::Edit, spaces::*};

const SPACE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn snapshot(role: Role, revision: u64) -> Discovery {
    let installed = InstalledApp {
        policy: AppPolicy {
            app: "counter".into(),
            schema: "b".repeat(64),
            closed_only: false,
            editable: true,
            comments: false,
        },
        title: "Counter".into(),
        ui_ref: "app:counter/ui".into(),
        state_ref: "app:counter/state".into(),
    };
    Discovery {
        version: 1,
        cursor: DiscoveryCursor {
            authority: "c".repeat(32),
            revision,
        },
        generation: revision,
        identity: Some("alice".into()),
        spaces: vec![SpaceSummary {
            id: SPACE.into(),
            name: "Household".into(),
            audience: Audience::Closed,
            role: Some(role),
        }],
        selected: Some(SpaceDetail {
            space: SPACE.into(),
            apps: vec![AppSummary {
                app: "counter".into(),
                title: "Counter".into(),
                collection: "d".repeat(32),
                attached: true,
                availability: AppAvailability::Available,
                stored_bytes: (role == Role::Admin).then_some(1234),
                launch: Some(installed.clone()),
            }],
            installed: if role == Role::Admin {
                vec![installed]
            } else {
                vec![]
            },
            members: (role == Role::Admin).then(|| {
                vec![
                    Member {
                        identity: "alice".into(),
                        role,
                        joined_generation: 1,
                    },
                    Member {
                        identity: "closed-admin-secret".into(),
                        role: Role::Member,
                        joined_generation: 1,
                    },
                ]
            }),
        }),
    }
}
fn fixture(role: Role) -> (Client, Ui) {
    let field = a::StateField::normalized(1, "value", 0);
    let ui = a::AppUi::new("counter", "Counter", a::schema("counter-v1", &[field])).build(
        "Counter",
        vec![
            a::button("Increase")
                .frame([24., 160., 240., 64.])
                .on_press(a::Event::toggle(field)),
            a::button("Remove app")
                .frame([24., 240., 240., 64.])
                .on_press(a::Event::toggle(field)),
        ],
    );
    let bytes = encode_ui(&ui).unwrap();
    let mut client = Client::default();
    client.set_font(std::sync::Arc::new(
        otrios_core::dashboard::font::Font::from_zstd(
            include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            otrios_core::dashboard::font_reference::RESOURCE_SHA256,
        )
        .unwrap(),
    ));
    client
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app: App {
                        id: "counter".into(),
                        title: "Counter".into(),
                        ui_ref: ui.ui_ref.clone(),
                        state_ref: "app:counter/state".into(),
                        status: "canvas-app".into(),
                        dependencies: vec![],
                    },
                    path: "".into(),
                    ui_hash: hash(&bytes),
                    private: false,
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client.configure_spaces(Some(SPACE)).unwrap();
    client.set_session_controls(true);
    assert!(client.spaces_outgoing().unwrap().is_some());
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: snapshot(role, 1),
                resync: true,
            })
            .unwrap(),
        )
        .unwrap();
    (client, ui)
}
fn labels(client: &Client) -> Vec<String> {
    client
        .scene()
        .unwrap()
        .controls()
        .iter()
        .map(|v| v.node.semantics.as_ref().unwrap().label.clone())
        .collect()
}
fn click(client: &mut Client, label: &str) {
    let id = client
        .scene()
        .unwrap()
        .controls()
        .iter()
        .find(|v| v.node.semantics.as_ref().unwrap().label == label)
        .unwrap_or_else(|| panic!("missing {label}: {:?}", labels(client)))
        .node
        .id;
    assert!(client.activate(client.epoch(), id).unwrap());
}

#[test]
fn trusted_controls_reflow_and_only_admins_receive_data_and_membership_actions() {
    for role in [Role::Admin, Role::Member] {
        let (mut client, _) = fixture(role);
        assert!(client.spaces_ui());
        assert_eq!(
            labels(&client).iter().any(|l| l == "Apps & Data"),
            role == Role::Admin
        );
        for viewport in [[1280., 800.], [800., 1280.], [780., 844.]] {
            client.set_viewport(viewport).unwrap();
            let scene = client.scene().unwrap();
            for control in scene.controls() {
                assert!(control.origin[0] >= 0. && control.origin[1] >= 0.);
                assert!(
                    control.origin[0] + control.node.frame.width * control.scale
                        <= viewport[0] + 0.1,
                    "{:?} origin={:?} frame={:?} scale={} viewport={viewport:?} root={:?}",
                    control.node.semantics,
                    control.origin,
                    control.node.frame,
                    control.scale,
                    scene.root().frame
                );
                assert!(
                    control.origin[1] + control.node.frame.height * control.scale
                        <= viewport[1] + 0.1
                );
            }
        }
    }
}

#[test]
fn app_removal_is_an_explicit_generation_checked_host_request_and_member_rosters_revoke() {
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Apps & Data");
    click(&mut client, "Counter");
    click(&mut client, "Remove app");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("management")
    };
    assert_eq!(request.expected_generation, 1);
    assert_eq!(
        request.change,
        Change::Remove {
            space: SPACE.into(),
            app: "counter".into()
        }
    );
    click(&mut client, "Home");
    click(&mut client, "Apps & Data");
    click(&mut client, "Members");
    let old_epoch = client.epoch();
    assert!(format!("{:?}", client.scene().unwrap()).contains("closed-admin-secret"));
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: snapshot(Role::Member, 2),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(
        client.take_collection_revoked(),
        "held native/browser roster frames must be redacted without renderer credit"
    );
    assert!(!format!("{:?}", client.scene().unwrap()).contains("closed-admin-secret"));
    assert!(
        !labels(&client)
            .iter()
            .any(|s| ["Apps & Data", "Members", "Remove app"].contains(&s.as_str()))
    );
    assert!(!client.activate(old_epoch, 1).unwrap());
}

#[test]
fn local_forms_use_the_shared_keyboard_without_synchronizing_drafts_as_app_data() {
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Spaces");
    click(&mut client, "Create Space");
    click(&mut client, "Space name");
    assert!(client.keyboard_visible());
    client
        .hardware_edit(&Edit::Commit("Workshop".into()))
        .unwrap();
    assert_eq!(
        client.typed_value(1).unwrap(),
        otrios_interfaces::layout::Value::Text("Workshop".into())
    );
    client.set_viewport([800., 1280.]).unwrap();
    click(&mut client, "Create Space");
    assert!(
        client.outgoing().unwrap().is_none(),
        "form text never enters the Canvas state channel"
    );
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("management")
    };
    assert_eq!(
        request.change,
        Change::Create {
            name: "Workshop".into()
        }
    );
}

#[test]
fn unavailable_metadata_discards_the_private_host_view_and_switching_forgets_the_old_scope() {
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Apps & Data");
    click(&mut client, "Members");
    client
        .receive_spaces(&spaces::encode_event(&Event::Unavailable { version: 1 }).unwrap())
        .unwrap();
    assert!(client.scene().is_err());
    assert!(client.take_collection_revoked());
    let (mut client, _) = fixture(Role::Admin);
    client.select_space(&"e".repeat(32)).unwrap();
    assert!(client.spaces_snapshot().is_none());
    assert!(client.scene().is_err());
    assert!(client.take_collection_revoked());
    assert!(
        matches!(spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap(), Command::Watch { selected: Some(id), .. } if id == "e".repeat(32))
    );
}

fn launch(client: &mut Client, ui: &Ui, authority: &mut Document) -> Session {
    click(client, "Counter");
    let ClientMessage::Mount {
        epoch,
        writer,
        resume,
        ..
    } = decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    let (mut session, _) = authority.open_session(&writer, resume).unwrap();
    let bytes = encode_ui(ui).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: authority.generate(&mut session).unwrap().unwrap(),
                binding: Some(MountBinding {
                    version: 1,
                    collection: "d".repeat(32),
                    editable: true,
                    comments: false,
                    moderate: false,
                }),
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::AcceptMount { epoch }
    );
    client
        .receive(&encode_server(&ServerMessage::Activated { epoch }).unwrap())
        .unwrap();
    assert!(!client.spaces_ui());
    let quit = client
        .scene()
        .unwrap()
        .controls()
        .iter()
        .find(|v| v.node.semantics.as_ref().unwrap().label == "Quit")
        .unwrap()
        .node
        .id;
    assert_eq!(client.control_value(quit).unwrap(), None);
    assert!(!client.adjust_control(quit, 1, 0).unwrap());
    session
}

#[test]
fn quit_waits_for_confirmation_preserves_data_and_publisher_controls_cannot_manage_spaces() {
    let (mut client, ui) = fixture(Role::Admin);
    let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    let mut session = launch(&mut client, &ui, &mut authority);
    // A publisher may reuse host labels and handler numbers. Only the trusted
    // local screen owns the management action table.
    let publisher_button = client
        .scene()
        .unwrap()
        .controls()
        .iter()
        .find(|v| v.node.semantics.as_ref().unwrap().label == "Remove app")
        .unwrap()
        .node
        .id;
    // This fixture does not display its field, so changing it need not redraw.
    client.activate(client.epoch(), publisher_button).unwrap();
    assert!(!client.confirmed());
    assert!(client.spaces_outgoing().unwrap().is_none());
    assert_eq!(client.value(1).unwrap(), 1000);
    let old_epoch = client.epoch();
    assert!(!client.show_spaces().unwrap());
    assert!(!client.spaces_ui());
    let mut leaves = 0;
    for step in 0..40 {
        client.tick(1000 + step * 33, 0).unwrap();
        while let Some(bytes) = client.outgoing().unwrap() {
            match decode_client(&bytes).unwrap() {
                ClientMessage::Sync { bytes, .. } => {
                    authority.receive(&mut session, &bytes).unwrap();
                }
                ClientMessage::Leave { epoch } => {
                    assert_eq!(epoch, old_epoch);
                    leaves += 1;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        if let Some(bytes) = authority.generate(&mut session).unwrap() {
            client
                .receive(
                    &encode_server(&ServerMessage::Sync {
                        epoch: old_epoch,
                        bytes,
                    })
                    .unwrap(),
                )
                .unwrap();
        }
        if client.spaces_ui() && leaves == 1 {
            break;
        }
    }
    assert!(client.spaces_ui());
    assert_eq!(leaves, 1);
    assert_eq!(authority.value(1).unwrap(), 1000);
    assert!(client.spaces_outgoing().unwrap().is_none());
    launch(&mut client, &ui, &mut authority);
    assert_eq!(client.value(1).unwrap(), 1000);
}

#[test]
fn denied_managed_reconnect_redacts_the_previous_replica_without_renderer_credit() {
    let (mut client, ui) = fixture(Role::Member);
    let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    launch(&mut client, &ui, &mut authority);
    client.reset_connection();
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("reconnect")
    };
    client
        .receive(&encode_server(&ServerMessage::AccessRequired { epoch }).unwrap())
        .unwrap();
    assert!(client.take_collection_revoked());
    assert!(client.scene().is_err());
    assert!(
        client.spaces_outgoing().unwrap().is_some(),
        "refresh admission metadata"
    );
}

#[test]
fn signed_in_discovery_does_not_cancel_an_admissible_deep_link() {
    let (mut client, ui) = fixture(Role::Member);
    client.select_space(SPACE).unwrap();
    assert!(client.spaces_outgoing().unwrap().is_some());
    client.open(&ui.ui_ref, "app:counter/state");
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    client
        .receive(&encode_server(&ServerMessage::AccessRequired { epoch }).unwrap())
        .unwrap();
    assert!(client.spaces_outgoing().unwrap().is_some());
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 3,
                snapshot: snapshot(Role::Member, 2),
                resync: true,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(
        !client.spaces_ui(),
        "sign-in metadata must preserve the requested destination"
    );
    assert_eq!(client.pending_ui_ref(), ui.ui_ref);
    client.retry_mount();
    assert_eq!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::CancelMount { epoch }
    );
    assert!(matches!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::Mount { .. }
    ));
}

#[test]
fn unchanged_resync_keeps_live_controls_and_reopening_the_same_scope_reports_a_new_frame() {
    let (mut client, _) = fixture(Role::Admin);
    let bytes = spaces::encode_event(&Event::Snapshot {
        observation: 1,
        snapshot: snapshot(Role::Admin, 1),
        resync: true,
    })
    .unwrap();
    let epoch = client.epoch();
    assert!(!client.receive_spaces(&bytes).unwrap());
    assert_eq!(
        client.epoch(),
        epoch,
        "unchanged metadata cannot invalidate the displayed controls"
    );
    click(&mut client, "Apps & Data");
    client.select_space(SPACE).unwrap();
    assert!(client.spaces_outgoing().unwrap().is_some());
    assert!(
        !client.receive_spaces(&bytes).unwrap(),
        "old observation cannot reactivate controls"
    );
    let fresh = spaces::encode_event(&Event::Snapshot {
        observation: 2,
        snapshot: snapshot(Role::Admin, 1),
        resync: true,
    })
    .unwrap();
    assert!(
        client.receive_spaces(&fresh).unwrap(),
        "a new local activation needs a presentation even when metadata is unchanged"
    );
    assert!(client.spaces_ui());
    click(&mut client, "Apps & Data");
}

#[test]
fn home_waits_for_discovery_instead_of_launching_the_first_installed_app() {
    for managed in [true, false] {
        let (mut client, _) = fixture(Role::Admin);
        client.select_space(SPACE).unwrap();
        client.navigate("").unwrap();
        assert!(
            client.outgoing().unwrap().is_none(),
            "discovery must precede choosing Home"
        );
        assert!(client.spaces_outgoing().unwrap().is_some());
        let event = if managed {
            Event::Snapshot {
                observation: 2,
                snapshot: snapshot(Role::Admin, 2),
                resync: true,
            }
        } else {
            Event::Unavailable { version: 1 }
        };
        assert!(
            client
                .receive_spaces(&spaces::encode_event(&event).unwrap())
                .unwrap()
        );
        if managed {
            assert!(client.spaces_ui());
            assert!(client.outgoing().unwrap().is_none());
            click(&mut client, "Counter");
        }
        assert!(matches!(
            decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
            ClientMessage::Mount { .. }
        ));
    }
}

#[test]
fn metadata_before_revocation_cannot_strand_or_repaint_a_removed_app() {
    for reattached in [false, true] {
        let (mut client, ui) = fixture(Role::Member);
        let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
        launch(&mut client, &ui, &mut authority);
        let epoch = client.epoch();
        let mut removed = snapshot(Role::Member, 2);
        removed.selected.as_mut().unwrap().apps.clear();
        let removed_event = spaces::encode_event(&Event::Snapshot {
            observation: 1,
            snapshot: removed.clone(),
            resync: false,
        })
        .unwrap();
        client.receive_spaces(&removed_event).unwrap();
        assert!(!client.spaces_ui());
        client
            .receive(&encode_server(&ServerMessage::AccessRequired { epoch }).unwrap())
            .unwrap();
        assert!(client.take_collection_revoked());
        assert!(client.spaces_snapshot().is_none());
        let Command::Watch { observation, .. } =
            spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("refresh")
        };
        assert_eq!(observation, 2);
        assert!(
            !client
                .receive_spaces(
                    &spaces::encode_event(&Event::Snapshot {
                        observation: 1,
                        snapshot: snapshot(Role::Admin, 1),
                        resync: true,
                    })
                    .unwrap()
                )
                .unwrap()
        );
        assert!(
            client.scene().is_err(),
            "an old observer cannot repaint a private roster"
        );
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation,
                    snapshot: if reattached {
                        snapshot(Role::Member, 3)
                    } else {
                        removed
                    },
                    resync: true,
                })
                .unwrap(),
            )
            .unwrap();
        if reattached {
            assert!(!client.auth_required());
            assert!(
                matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(), ClientMessage::Mount { epoch: next, .. } if next>epoch)
            );
        } else {
            assert!(client.spaces_ui());
            assert!(!labels(&client).contains(&"Counter".into()));
        }
    }
}
