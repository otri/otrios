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
            layout: Default::default(),
            apps: vec![MapApp {
                app: "counter".into(),
                title: "Counter".into(),
                availability: AppAvailability::Available,
            }],
            attached_count: 1,
            id: SPACE.into(),
            name: "Household".into(),
            audience: Audience::Closed,
            role: Some(role),
        }],
        selected: Some(SpaceDetail {
            cleanup: (role == Role::Admin)
                .then(CleanupSummary::default)
                .map(Box::new),
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

fn home(client: &mut Client) {
    let current = labels(client);
    if current.contains(&"Home".into()) {
        click(client, "Home");
    } else if current.contains(&"Close".into()) {
        click(client, "Close");
    }
    if labels(client).contains(&"Done".into()) {
        click(client, "Done");
    }
}
fn open_data(client: &mut Client) {
    if !labels(client).contains(&"Apps & Data".into()) {
        home(client);
        click(client, "Edit");
        click(client, "Space settings: Household");
    }
    click(client, "Apps & Data");
}

#[test]
fn trusted_controls_reflow_and_only_admins_receive_data_and_membership_actions() {
    for role in [Role::Admin, Role::Member] {
        let (mut client, _) = fixture(role);
        assert!(client.spaces_ui());
        assert_eq!(
            labels(&client).iter().any(|l| l == "Edit"),
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
    open_data(&mut client);
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
    home(&mut client);
    open_data(&mut client);
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
    home(&mut client);
    click(&mut client, "Edit");
    click(&mut client, "Add Space");
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
    open_data(&mut client);
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
    launch_access(client, ui, authority, true)
}
fn launch_access(
    client: &mut Client,
    ui: &Ui,
    authority: &mut Document,
    editable: bool,
) -> Session {
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
                    version: 2,
                    actor: [1; 32],
                    collection: "d".repeat(32),
                    editable,
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
        .find(|v| v.node.semantics.as_ref().unwrap().label == "Back to OTriOS")
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

fn local_namespace(generation: u64, removed: &[&str]) -> otrios_interfaces::device_apps::Namespace {
    otrios_interfaces::device_apps::Namespace {
        version: 1,
        generation,
        removed: removed.iter().map(|s| (*s).into()).collect(),
    }
}

#[test]
fn local_uninstall_flushes_confirmed_edits_leaves_other_clients_and_reattaches_same_data() {
    let (mut client, ui) = fixture(Role::Admin);
    let catalog_hash = client.catalog_hash().to_vec();
    let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    let mut session = launch(&mut client, &ui, &mut authority);
    let epoch = client.epoch();
    client.event(epoch, ui.handlers[0].control, 0, 0).unwrap();
    assert!(!client.confirmed());
    client
        .update_local_apps(Some(local_namespace(1, &["counter"])))
        .unwrap();
    assert!(!client.event(epoch, ui.handlers[0].control, 0, 0).unwrap());
    client.tick(1000, 0).unwrap();
    assert!(
        !client.spaces_ui(),
        "unconfirmed edits must drain before disposal"
    );
    let mut leaves = 0;
    for step in 0..40 {
        client.tick(1100 + step * 33, 0).unwrap();
        while let Some(bytes) = client.outgoing().unwrap() {
            match decode_client(&bytes).unwrap() {
                ClientMessage::Sync { bytes, .. } => {
                    authority.receive(&mut session, &bytes).unwrap();
                }
                ClientMessage::Leave { epoch: left } => {
                    assert_eq!(left, epoch);
                    leaves += 1;
                }
                other => panic!("uninstall sent unexpected authority command {other:?}"),
            }
        }
        if let Some(bytes) = authority.generate(&mut session).unwrap() {
            client
                .receive(&encode_server(&ServerMessage::Sync { epoch, bytes }).unwrap())
                .unwrap();
        }
        if client.spaces_ui() && leaves == 1 {
            break;
        }
    }
    assert!(client.spaces_ui());
    assert!(
        client.take_collection_revoked(),
        "held presentation must be invalidated"
    );
    assert_eq!(leaves, 1);
    assert_eq!(authority.value(1).unwrap(), 1000);
    assert_eq!(client.catalog_hash(), catalog_hash);
    assert!(
        client.spaces_outgoing().unwrap().is_none(),
        "no Space management operation"
    );
    assert!(labels(&client).iter().any(|label| label == "Counter"));
    click(&mut client, "Counter");
    assert!(
        client.outgoing().unwrap().is_none(),
        "unavailable map tiles cannot launch removed local code"
    );
    client.open(&ui.ui_ref, "app:counter/state");
    assert!(client.outgoing().unwrap().is_none());
    // A newer remote discovery snapshot cannot reinstall code locally.
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: snapshot(Role::Admin, 2),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(labels(&client).iter().any(|label| label == "Counter"));
    click(&mut client, "Counter");
    assert!(
        client.outgoing().unwrap().is_none(),
        "unavailable map tiles cannot launch removed local code"
    );
    let retained = &client
        .spaces_snapshot()
        .unwrap()
        .selected
        .as_ref()
        .unwrap()
        .apps[0];
    assert!(retained.attached);
    assert_eq!(retained.collection, "d".repeat(32));
    let (mut other, _) = fixture(Role::Member);
    launch(&mut other, &ui, &mut authority);
    assert_eq!(other.value(1).unwrap(), 1000);
    client
        .update_local_apps(Some(local_namespace(2, &[])))
        .unwrap();
    launch(&mut client, &ui, &mut authority);
    assert_eq!(client.value(1).unwrap(), 1000);
}

#[test]
fn local_uninstall_cancels_staged_mount_and_rejects_missing_or_rolled_back_namespace() {
    let (mut client, ui) = fixture(Role::Member);
    click(&mut client, "Counter");
    let ClientMessage::Mount { epoch, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount");
    };
    let bytes = encode_ui(&ui).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: vec![],
                binding: Some(MountBinding {
                    version: 2,
                    actor: [1; 32],
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
    // Model Accept already queued while the authority's Activated is in flight.
    assert_eq!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::AcceptMount { epoch }
    );
    client
        .update_local_apps(Some(local_namespace(2, &["counter"])))
        .unwrap();
    assert_eq!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::CancelMount { epoch }
    );
    assert_eq!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::Leave { epoch }
    );
    assert!(
        !client
            .receive(&encode_server(&ServerMessage::Activated { epoch }).unwrap())
            .unwrap()
    );
    assert!(
        client
            .update_local_apps(Some(local_namespace(1, &[])))
            .is_err()
    );
    assert!(
        client
            .update_local_apps(Some(local_namespace(2, &[])))
            .is_err()
    );
    assert!(client.update_local_apps(None).is_err());
    client.open(&ui.ui_ref, "app:counter/state");
    assert!(client.outgoing().unwrap().is_none());
    assert!(
        client
            .mount_local(&encode_ui(&ui).unwrap(), "app:counter/state", &[], &[])
            .is_err()
    );
    client
        .update_local_apps(Some(local_namespace(3, &[])))
        .unwrap();
    client.open(&ui.ui_ref, "app:counter/state");
    assert!(matches!(
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::Mount { .. }
    ));
}

#[test]
fn local_uninstall_waits_for_captured_release_and_preserves_its_final_edit() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let (mut client, ui) = fixture(Role::Member);
    let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    launch(&mut client, &ui, &mut authority);
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let button = scene
        .controls()
        .into_iter()
        .find(|v| v.node.semantics.as_ref().unwrap().label == "Increase")
        .unwrap();
    let position = [button.origin[0] + 20., button.origin[1] + 20.];
    client
        .pointer(
            epoch,
            PointerEvent {
                id: 1,
                phase: PointerPhase::Down,
                position,
            },
        )
        .unwrap();
    assert!(client.input_busy());
    client
        .update_local_apps(Some(local_namespace(1, &["counter"])))
        .unwrap();
    client.tick(1000, 0).unwrap();
    assert!(!client.spaces_ui());
    assert!(
        !client
            .pointer(
                epoch,
                PointerEvent {
                    id: 2,
                    phase: PointerPhase::Down,
                    position
                }
            )
            .unwrap()
    );
    client
        .pointer(
            epoch,
            PointerEvent {
                id: 1,
                phase: PointerPhase::Up,
                position,
            },
        )
        .unwrap();
    assert!(!client.input_busy());
    assert_eq!(
        client.value(1).unwrap(),
        1000,
        "captured release still commits its edit"
    );
    assert!(!client.confirmed());
    client.tick(1100, 0).unwrap();
    assert!(!client.spaces_ui(), "released edit must also be confirmed");
}

#[test]
fn disconnected_uninstall_discards_only_unconfirmed_ram_and_cannot_reconnect_removed_code() {
    for offline_first in [false, true] {
        let (mut client, ui) = fixture(Role::Member);
        let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
        launch(&mut client, &ui, &mut authority);
        client
            .event(client.epoch(), ui.handlers[0].control, 0, 0)
            .unwrap();
        assert!(!client.confirmed());
        if offline_first {
            client.set_local_apps_online(false);
        }
        client
            .update_local_apps(Some(local_namespace(1, &["counter"])))
            .unwrap();
        client.reset_connection();
        client.set_local_apps_online(true); // A new socket cannot revive the old lease.
        client.tick(1000, 0).unwrap();
        assert!(client.spaces_ui());
        assert!(client.take_collection_revoked());
        while let Some(bytes) = client.outgoing().unwrap() {
            assert!(matches!(
                decode_client(&bytes).unwrap(),
                ClientMessage::Leave { .. }
            ));
        }
        assert_eq!(authority.value(1).unwrap(), 0);
        client
            .update_local_apps(Some(local_namespace(2, &[])))
            .unwrap();
        launch(&mut client, &ui, &mut authority);
        assert_eq!(
            client.value(1).unwrap(),
            0,
            "discarded intent must not resume after reinstall"
        );
    }
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
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(),
        ClientMessage::Leave { epoch: disposed } if disposed == epoch),
        "acknowledge only after the old replica and held view were discarded"
    );
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
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(), ClientMessage::Leave { epoch: disposed } if disposed == epoch)
    );
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
    open_data(&mut client);
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
    open_data(&mut client);
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
        removed.spaces[0].apps.clear();
        removed.spaces[0].attached_count = 0;
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
        assert!(
            matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(), ClientMessage::Leave { epoch: disposed } if disposed == epoch)
        );
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

#[test]
fn deletion_requires_trusted_confirmation_and_carries_the_reviewed_collection() {
    for remove in [false, true] {
        let (mut client, _) = fixture(Role::Admin);
        open_data(&mut client);
        click(&mut client, "Counter");
        click(
            &mut client,
            if remove {
                "Remove & delete"
            } else {
                "Delete data"
            },
        );
        assert!(client.spaces_outgoing().unwrap().is_none());
        assert!(labels(&client).contains(&"Permanently delete".into()));
        let view = format!("{:?}", client.scene().unwrap());
        assert!(view.contains("Household") && view.contains("Counter"));
        click(&mut client, "Cancel");
        assert!(client.spaces_outgoing().unwrap().is_none());
        click(
            &mut client,
            if remove {
                "Remove & delete"
            } else {
                "Delete data"
            },
        );
        click(&mut client, "Permanently delete");
        let Command::Manage { request, .. } =
            spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("delete request");
        };
        assert_eq!(request.expected_generation, 1);
        assert_eq!(
            request.change,
            Change::DeleteData {
                space: SPACE.into(),
                app: "counter".into(),
                collection: "d".repeat(32),
                remove
            }
        );
    }
}

#[test]
fn changed_collection_or_lost_admin_role_invalidates_a_deletion_dialog() {
    for member in [false, true] {
        let (mut client, _) = fixture(Role::Admin);
        open_data(&mut client);
        click(&mut client, "Counter");
        click(&mut client, "Delete data");
        let mut changed = snapshot(if member { Role::Member } else { Role::Admin }, 2);
        changed.selected.as_mut().unwrap().apps[0].collection = "e".repeat(32);
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation: 1,
                    snapshot: changed,
                    resync: false,
                })
                .unwrap(),
            )
            .unwrap();
        assert!(!labels(&client).contains(&"Permanently delete".into()));
        assert!(client.spaces_outgoing().unwrap().is_none());
        if !member {
            click(&mut client, "Counter");
            click(&mut client, "Delete data");
            click(&mut client, "Permanently delete");
            let Command::Manage { request, .. } =
                spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
            else {
                panic!("new confirmation");
            };
            assert_eq!(request.expected_generation, 2);
            assert!(
                matches!(request.change, Change::DeleteData { collection, .. } if collection == "e".repeat(32))
            );
        }
    }
}

#[test]
fn cleanup_ui_reports_retention_deadlines_without_claiming_an_offline_wipe() {
    let (mut client, _) = fixture(Role::Admin);
    let mut progress = snapshot(Role::Admin, 2);
    progress.selected.as_mut().unwrap().cleanup = Some(Box::new(CleanupSummary {
        deleted_collections: 1,
        local_pending: 1,
        backup_until_unix_ms: Some(1_735_689_600_000),
        legacy_qualification_required: true,
        ..CleanupSummary::default()
    }));
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: progress,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    open_data(&mut client);
    click(&mut client, "Cleanup");
    let view = format!("{:?}", client.scene().unwrap());
    for text in [
        "pending: 1",
        "2025-01-01 00:00 UTC",
        "await migration qualification",
        "cannot be verified",
    ] {
        assert!(view.contains(text), "missing cleanup fact: {text}");
    }
}

#[test]
fn a_returning_offline_client_discards_pending_edits_when_the_collection_is_replaced() {
    let (mut client, ui) = fixture(Role::Member);
    let mut old = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    launch(&mut client, &ui, &mut old);
    client
        .event(client.epoch(), ui.handlers[0].control, 0, 0)
        .unwrap();
    assert_eq!(client.value(1).unwrap(), 1000);
    assert!(!client.confirmed());
    client.reset_connection();
    let ClientMessage::Mount {
        epoch,
        writer,
        collection,
        ..
    } = decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("returning mount");
    };
    assert_eq!(collection, Some("d".repeat(32)));
    let mut replacement = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    let (mut session, _) = replacement.open_session(&writer, 0).unwrap();
    let bytes = encode_ui(&ui).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui_hash: hash(&bytes),
                ui: bytes,
                code: vec![],
                state: replacement.generate(&mut session).unwrap().unwrap(),
                binding: Some(MountBinding {
                    version: 2,
                    actor: [1; 32],
                    collection: "e".repeat(32),
                    editable: true,
                    comments: false,
                    moderate: false,
                }),
                initial_view: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(
        matches!(decode_client(&client.outgoing().unwrap().unwrap()).unwrap(), ClientMessage::AcceptMount { epoch: accepted } if accepted == epoch)
    );
    client
        .receive(&encode_server(&ServerMessage::Activated { epoch }).unwrap())
        .unwrap();
    assert_eq!(
        client.value(1).unwrap(),
        0,
        "same app/reference cannot rebase edits from a deleted collection"
    );
    while let Some(bytes) = client.outgoing().unwrap() {
        if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes).unwrap() {
            assert!(!matches!(
                decode_state_request(&bytes).unwrap(),
                StateRequest::Write { .. }
            ));
        }
    }
}

#[test]
fn only_admins_can_confirm_public_audiences_and_role_loss_cancels_the_dialog() {
    let (mut client, _) = fixture(Role::Admin);
    open_data(&mut client);
    click(&mut client, "Sharing");
    click(&mut client, "Public view & comment");
    assert!(client.spaces_outgoing().unwrap().is_none());
    assert!(labels(&client).contains(&"Make public".into()));
    click(&mut client, "Cancel");
    click(&mut client, "Public view");
    click(&mut client, "Make public");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("audience request");
    };
    assert_eq!(
        request.change,
        Change::Audience {
            space: SPACE.into(),
            audience: Audience::PublicView
        }
    );
    let (mut client, _) = fixture(Role::Admin);
    open_data(&mut client);
    click(&mut client, "Sharing");
    click(&mut client, "Public view");
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
    assert!(!labels(&client).contains(&"Make public".into()));
    assert!(!labels(&client).contains(&"Sharing".into()));
}

#[test]
fn read_only_document_controls_do_not_queue_writes_or_fail_the_view() {
    let (mut client, ui) = fixture(Role::Member);
    let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    launch_access(&mut client, &ui, &mut authority, false);
    assert!(!client.scene().unwrap().controls().iter().any(|c| {
        c.node
            .semantics
            .as_ref()
            .is_some_and(|s| s.label == "Increase")
    }));
    assert!(
        !client
            .event(client.epoch(), ui.handlers[0].control, 1000, 0)
            .unwrap()
    );
    assert_eq!(client.value(1).unwrap(), 0);
    assert!(client.confirmed());
    while let Some(bytes) = client.outgoing().unwrap() {
        if let ClientMessage::Sync { bytes, .. } = decode_client(&bytes).unwrap() {
            assert!(!matches!(
                decode_state_request(&bytes).unwrap(),
                StateRequest::Write { .. }
            ));
        }
    }
    assert!(client.scene().is_ok());
}

#[test]
fn map_has_common_entry_profile_and_edit_without_role_or_spaces_headings() {
    for role in [Role::Member, Role::Admin] {
        let (mut client, _) = fixture(role);
        let controls = labels(&client);
        assert!(controls.contains(&"Profile".into()));
        assert!(controls.contains(&"Counter".into()));
        assert!(!controls.contains(&"Spaces".into()));
        assert_eq!(controls.contains(&"Edit".into()), role == Role::Admin);
        let view = format!("{:?}", client.scene().unwrap());
        assert!(!view.contains("Your Spaces"));
        click(&mut client, "Profile");
        assert!(labels(&client).contains(&"Sign out".into()));
        click(&mut client, "Close");
        assert!(labels(&client).contains(&"Counter".into()));
    }
}

#[test]
fn app_name_commits_on_enter_without_changing_bundle_or_launch_destination() {
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Edit");
    click(&mut client, "Counter");
    let panel = labels(&client);
    for unwanted in ["Rename", "Move...", "Delete App"] {
        assert!(!panel.contains(&unwanted.into()));
    }
    for action in ["Name", "Share App", "Export Data", "Replace Data"] {
        assert!(panel.contains(&action.into()));
    }
    assert!(format!("{:?}", client.scene().unwrap()).contains("counter"));
    click(&mut client, "Name");
    client.hardware_edit(&Edit::SelectAll).unwrap();
    client.hardware_edit(&Edit::Commit("Tally".into())).unwrap();
    client.hardware_edit(&Edit::Enter).unwrap();
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("label command")
    };
    assert_eq!(
        request.change,
        Change::LabelApp {
            space: SPACE.into(),
            app: "counter".into(),
            name: "Tally".into()
        }
    );
    assert_eq!(request.expected_generation, 1);
    assert!(
        !client.reload_ready(),
        "confirmed management precedes release handoff"
    );
}

#[test]
fn title_and_corner_keyboard_actions_are_shared_generation_checked_metadata() {
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Edit");
    let scene = client.scene().unwrap();
    let resize = scene
        .controls()
        .iter()
        .find(|n| n.node.semantics.as_ref().unwrap().label == "Resize: Household")
        .unwrap()
        .node
        .id;
    assert!(client.order_draggable(resize));
    client.adjust_control(resize, 50, -50).unwrap();
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("resize")
    };
    assert_eq!(request.expected_generation, 1);
    assert_eq!(
        request.change,
        Change::ConfigureSpace {
            space: SPACE.into(),
            name: "Household".into(),
            layout: SpaceLayout {
                width: 4,
                height: 2,
                background: Background::None
            }
        }
    );
}

#[test]
fn empty_spaces_are_edit_only_and_cancelled_drags_cannot_write() {
    let (mut client, _) = fixture(Role::Admin);
    let mut empty = snapshot(Role::Admin, 2);
    empty.spaces[0].apps.clear();
    empty.spaces[0].attached_count = 0;
    empty.selected.as_mut().unwrap().apps.clear();
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: empty,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(!labels(&client).contains(&"Space: Household".into()));
    click(&mut client, "Edit");
    assert!(labels(&client).contains(&"Space: Household".into()));
    let scene = client.scene().unwrap();
    let title = scene
        .controls()
        .into_iter()
        .find(|n| n.node.semantics.as_ref().unwrap().label == "Space: Household")
        .unwrap();
    let p = [title.origin[0] + 20., title.origin[1] + 20.];
    let epoch = client.epoch();
    for phase in [
        otrios_interfaces::scene::PointerPhase::Down,
        otrios_interfaces::scene::PointerPhase::Move,
        otrios_interfaces::scene::PointerPhase::Cancel,
    ] {
        client
            .pointer(
                epoch,
                otrios_interfaces::scene::PointerEvent {
                    id: 3,
                    phase,
                    position: p,
                },
            )
            .unwrap();
    }
    assert!(client.spaces_outgoing().unwrap().is_none());
    assert!(client.reload_ready());
}

#[test]
fn dropping_an_app_on_a_later_tile_reorders_without_launching_or_rebinding() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase};
    let (mut client, _) = fixture(Role::Admin);
    let mut next = snapshot(Role::Admin, 2);
    next.spaces[0].apps.push(MapApp {
        app: "sketches".into(),
        title: "Sketches".into(),
        availability: AppAvailability::Unavailable,
    });
    next.spaces[0].attached_count = 2;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: next,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    click(&mut client, "Edit");
    let point = |label: &str| {
        let scene = client.scene().unwrap();
        let nodes = scene.controls();
        let node = nodes
            .iter()
            .find(|v| v.node.semantics.as_ref().unwrap().label == label)
            .unwrap();
        [
            node.origin[0] + node.node.frame.width * node.scale / 2.,
            node.origin[1] + 36. * node.scale,
        ]
    };
    let start = point("Counter");
    let end = point("Sketches");
    let epoch = client.epoch();
    client
        .pointer(
            epoch,
            PointerEvent {
                id: 12,
                phase: PointerPhase::Down,
                position: start,
            },
        )
        .unwrap();
    client
        .pointer(
            epoch,
            PointerEvent {
                id: 12,
                phase: PointerPhase::Move,
                position: end,
            },
        )
        .unwrap();
    assert!(!client.reload_ready());
    client
        .pointer(
            epoch,
            PointerEvent {
                id: 12,
                phase: PointerPhase::Up,
                position: end,
            },
        )
        .unwrap();
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("reorder request")
    };
    assert_eq!(request.expected_generation, 2);
    assert_eq!(
        request.change,
        Change::PlaceApp {
            space: SPACE.into(),
            app: "counter".into(),
            before: None
        }
    );
    assert!(client.outgoing().unwrap().is_none());
    assert!(client.spaces_ui());
}

#[test]
fn map_scroll_reverses_repeatedly_without_remounting_or_losing_controls() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
    let (mut client, _) = fixture(Role::Admin);
    client.set_viewport([390., 440.]).unwrap();
    let mut next = snapshot(Role::Admin, 2);
    next.spaces[0].layout.height = 6;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: next,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    let epoch = client.epoch();
    let limit = client.resource_usage().map_scroll.unwrap().limit[1];
    assert!(limit > 500.);
    assert!(labels(&client).contains(&"Counter".into()));
    for unwanted in ["Scroll up", "Scroll down"] {
        assert!(!labels(&client).contains(&unwanted.into()));
    }
    assert!(
        format!("{:?}", client.scene().unwrap()).contains("4200000000"),
        "edge fade at the top"
    );
    for round in 0..4 {
        let time = round * 1000;
        for (phase, y, at, expected) in [
            (Down, 330., time, 0.),
            (Move, -4000., time + 20, limit),
            (Move, -3990., time + 40, limit - 10.),
            (Move, 4000., time + 60, 0.),
            (Move, 3990., time + 80, 10.),
            (Move, 4000., time + 100, 0.),
            (Up, 4000., time + 200, 0.),
        ] {
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 42,
                        phase,
                        position: [20., y],
                    },
                    at,
                )
                .unwrap();
            assert_eq!(
                client.resource_usage().map_scroll.unwrap().offset[1],
                expected
            );
            assert_eq!(
                client.epoch(),
                epoch,
                "scrolling must retain the admitted map"
            );
            let scene = client.scene().unwrap();
            for c in scene.controls() {
                assert!(c.origin[1] >= 0. && c.origin[1] + c.node.frame.height * c.scale <= 440.);
            }
        }
        assert!(!client.input_busy());
        assert!(
            labels(&client).contains(&"Counter".into()),
            "reverse restores controls"
        );
    }
    assert!(client.scroll_map(epoch, 100000.).unwrap());
    assert_eq!(client.resource_usage().map_scroll.unwrap().offset[1], limit);
    assert!(
        !format!("{:?}", client.scene().unwrap()).contains("4200000000"),
        "fade disappears at the bottom"
    );
    assert!(!labels(&client).contains(&"Counter".into()));
    assert!(client.scroll_map(epoch, -100000.).unwrap());
    assert!(labels(&client).contains(&"Counter".into()));
    assert!(!client.scroll_map(epoch - 1, 100.).unwrap());
    assert!(!client.scroll_map(epoch, f32::NAN).unwrap());
    assert!(client.spaces_outgoing().unwrap().is_none());
    assert!(client.outgoing().unwrap().is_none());
}

#[test]
fn map_momentum_and_cancel_use_sample_time_and_release_capture() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
    let (mut client, _) = fixture(Role::Admin);
    client.set_viewport([390., 440.]).unwrap();
    let mut next = snapshot(Role::Admin, 2);
    next.spaces[0].layout.height = 6;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: next,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    let epoch = client.epoch();
    for (phase, y, at) in [
        (Down, 330., 0),
        (Move, 290., 20),
        (Move, 250., 40),
        (Move, 210., 60),
        (Up, 210., 60),
    ] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 42,
                    phase,
                    position: [20., y],
                },
                at,
            )
            .unwrap();
    }
    let before = client.resource_usage().map_scroll.unwrap().offset[1];
    assert!(!client.input_busy());
    assert!(client.tick(100, 0).unwrap());
    assert!(client.resource_usage().map_scroll.unwrap().offset[1] > before);
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 43,
                phase: Down,
                position: [20., 300.],
            },
            110,
        )
        .unwrap();
    assert!(client.input_busy());
    client.cancel().unwrap();
    assert!(!client.input_busy());
    let before = client.resource_usage().map_scroll.unwrap().offset;
    client.tick(1000, 0).unwrap();
    assert_eq!(client.resource_usage().map_scroll.unwrap().offset, before);
    assert_eq!(client.epoch(), epoch);
}

#[test]
fn managed_app_breadcrumb_returns_home_from_corner_padding() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
    let (mut client, ui) = fixture(Role::Admin);
    let mut authority = Document::new(ui.state_schema.clone(), "app:counter/state").unwrap();
    launch(&mut client, &ui, &mut authority);
    let epoch = client.epoch();
    assert!(!format!("{:?}", client.scene().unwrap()).contains("Quit"));
    for phase in [Down, Up] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 1,
                    phase,
                    position: [5., 5.],
                },
                100,
            )
            .unwrap();
    }
    assert!(client.spaces_ui());
    assert!(!client.input_busy());
}

#[test]
fn populated_map_keeps_header_footer_and_viewport_fixed_during_scroll() {
    let (mut client, _) = fixture(Role::Admin);
    let mut next = snapshot(Role::Admin, 2);
    let base = next.spaces[0].clone();
    next.selected = None;
    let apps = [
        "bundle-counter",
        "notes",
        "sketches",
        "component-playground",
        "layout-lab",
        "layout-text-scroll",
        "layout-document",
        "input-lab",
        "animation-lab",
        "three-d-lab",
    ];
    next.spaces = [10, 1, 1, 2]
        .into_iter()
        .enumerate()
        .map(|(index, count)| {
            let mut space = base.clone();
            space.id = format!("{:032x}", index + 10);
            space.name = format!("Populated {index}");
            space.layout.width = [3, 1, 1, 3][index];
            space.apps = apps[..count]
                .iter()
                .map(|id| MapApp {
                    app: (*id).into(),
                    title: format!("{id} example"),
                    availability: AppAvailability::Available,
                })
                .collect();
            space.attached_count = count as u16;
            space
        })
        .collect();
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: next,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    let fixed = |client: &Client| {
        let scene = client.scene().unwrap();
        ["Profile", "Edit"].map(|label| {
            let controls = scene.controls();
            let c = controls
                .iter()
                .find(|c| c.node.semantics.as_ref().unwrap().label == label)
                .expect("fixed chrome remains visible");
            (c.origin, c.node.frame)
        })
    };
    let before = fixed(&client);
    let epoch = client.epoch();
    for delta in [96., 96., 100000., -96., -100000.] {
        client.scroll_map(epoch, delta).unwrap();
        assert_eq!(fixed(&client), before);
        // The page background stays at the origin; only the explicit map's
        // content is translated. The outer page is also a clipped canvas.
        let scene = client.scene().unwrap();
        assert_eq!(scene.root().children[0].frame.y, 0.);
    }
}

#[test]
fn common_entry_sign_in_survives_viewport_and_metadata_rebuilds() {
    let (mut client, _) = fixture(Role::Member);
    let mut visitor = snapshot(Role::Member, 2);
    visitor.identity = None;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: visitor.clone(),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    click(&mut client, "Sign in");
    assert!(client.auth_required());
    client.set_viewport([800., 615.]).unwrap();
    assert!(client.auth_required());
    visitor.generation += 1;
    visitor.cursor.revision += 1;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: visitor,
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(client.auth_required());
    client.retry_mount();
    assert!(!client.auth_required());
}
