// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::{authoring as a, canvas::*, spaces};
use otrios_interfaces::transfers::*;
use otrios_interfaces::{canvas::*, keyboard::Edit, spaces::*};

const SPACE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn data_review() -> DataReview {
    DataReview {
        stage: "e".repeat(32),
        replacement: Replacement {
            space: SPACE.into(),
            app: "counter".into(),
            collection: "d".repeat(32),
            replacement: "f".repeat(32),
            expected_generation: 1,
        },
        audience: Audience::Closed,
        schema_sha256: "b".repeat(64),
        archive_sha256: "1".repeat(64),
        bytes: 100,
        expires_unix_ms: u64::MAX,
    }
}
fn transfer_fixture() -> Client {
    let (mut client, _) = fixture(Role::Admin);
    client
        .set_transfer_host("https://otrios.example", true, true, true)
        .unwrap();
    client
}
#[test]
fn exact_data_export_and_replacement_are_host_io_then_an_explicit_shared_session_commit() {
    let mut client = transfer_fixture();
    click(&mut client, "Edit");
    click(&mut client, "Counter");
    let right = |client: &Client, label: &str| {
        let scene = client.scene().unwrap();
        let control = scene
            .controls()
            .into_iter()
            .find(|c| c.node.semantics.as_ref().is_some_and(|s| s.label == label))
            .unwrap();
        control.origin[0] + control.node.frame.width * control.scale
    };
    let content_right = right(&client, "Export Data");
    click(&mut client, "Export Data");
    let export = client.take_transfer_task().unwrap();
    assert_eq!(export.io, TransferIo::Export);
    assert_eq!(
        export.target,
        TransferTarget::Data {
            dataset: DatasetRef {
                space: SPACE.into(),
                app: "counter".into(),
                collection: "d".repeat(32),
                schema: "b".repeat(64)
            }
        }
    );
    assert!(client.spaces_outgoing().unwrap().is_none());
    assert!(
        !client.reload_ready(),
        "connected export waits for its host result"
    );
    client
        .receive_transfer(
            export.id,
            TransferReply::Exported {
                name: "counter.otdata".into(),
            },
        )
        .unwrap();
    assert!(!labels(&client).contains(&"Saved".into()));
    assert!(
        (right(&client, "Close") - content_right).abs() < 0.01,
        "the wider transfer Close button shares the content's right inset"
    );
    click(&mut client, "Close");
    click(&mut client, "Replace Data");
    click(&mut client, "Choose File");
    let picker = client.take_transfer_task().unwrap();
    assert_eq!(picker.io, TransferIo::Pick);
    let file = TransferFile {
        id: "9".repeat(32),
        name: "archive.otdata".into(),
        bytes: 100,
    };
    client
        .receive_transfer(
            picker.id,
            TransferReply::Files {
                files: vec![file.clone()],
            },
        )
        .unwrap();
    click(&mut client, "Choose archive.otdata");
    let upload = client.take_transfer_task().unwrap();
    assert_eq!(upload.target, export.target);
    assert_eq!(upload.io, TransferIo::File { file: file.id });
    assert!(client.spaces_outgoing().unwrap().is_none());
    client
        .receive_transfer(
            upload.id,
            TransferReply::Data {
                review: data_review(),
            },
        )
        .unwrap();
    assert!(labels(&client).contains(&"Confirm Replacement".into()));
    assert!(
        client.spaces_outgoing().unwrap().is_none(),
        "review cannot publish"
    );
    assert_eq!(
        client
            .spaces_snapshot()
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .apps[0]
            .collection,
        "d".repeat(32)
    );
    click(&mut client, "Confirm Replacement");
    assert!(
        matches!(spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap(), Command::ReplaceData { stage, .. } if stage == data_review().stage)
    );
    assert!(
        client.spaces_outgoing().unwrap().is_none(),
        "no replacement replay"
    );
}
#[test]
fn closing_a_data_review_or_its_inflight_picker_disposes_the_stage_without_replacement() {
    for close_early in [false, true] {
        let mut client = transfer_fixture();
        click(&mut client, "Edit");
        click(&mut client, "Counter");
        click(&mut client, "Replace Data");
        click(&mut client, "Choose File");
        let upload = client.take_transfer_task().unwrap();
        if close_early {
            click(&mut client, "Close");
        }
        client
            .receive_transfer(
                upload.id,
                TransferReply::Data {
                    review: data_review(),
                },
            )
            .unwrap();
        if !close_early {
            click(&mut client, "Close");
        }
        assert!(
            matches!(spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap(), Command::CancelData { stage, .. } if stage == data_review().stage)
        );
        assert!(!labels(&client).contains(&"Confirm Replacement".into()));
    }
}
#[test]
fn role_loss_and_mismatched_review_cannot_offer_or_send_a_transfer_commit() {
    for revoke in [false, true] {
        let mut client = transfer_fixture();
        click(&mut client, "Edit");
        click(&mut client, "Counter");
        click(&mut client, "Replace Data");
        click(&mut client, "Choose File");
        let task = client.take_transfer_task().unwrap();
        let mut review = data_review();
        if revoke {
            client
                .receive_spaces(
                    &spaces::encode_event(&Event::Snapshot {
                        observation: 1,
                        snapshot: Box::new(snapshot(Role::Member, 2)),
                        resync: false,
                    })
                    .unwrap(),
                )
                .unwrap();
        } else {
            review.replacement.collection = "8".repeat(32);
        }
        client
            .receive_transfer(task.id, TransferReply::Data { review })
            .unwrap();
        assert!(!labels(&client).contains(&"Confirm Replacement".into()));
        assert!(client.take_transfer_task().is_none());
        assert!(client.spaces_outgoing().unwrap().is_none());
    }
}
#[test]
fn app_upload_tile_and_explicit_host_wide_review_precede_import_commit() {
    let mut client = transfer_fixture();
    click(&mut client, "Edit");
    click(&mut client, "Add app: Household");
    assert!(labels(&client).contains(&"Upload".into()));
    click(&mut client, "Upload");
    click(&mut client, "Shared app URL");
    client
        .hardware_edit(&Edit::Commit(format!(
            "https://publisher.example/apps/counter/{}",
            "7".repeat(64)
        )))
        .unwrap();
    click(&mut client, "Review URL");
    let task = client.take_transfer_task().unwrap();
    assert!(matches!(task.io, TransferIo::Url { url } if url.ends_with("/download")));
    let review = AppReview {
        stage: "6".repeat(32),
        space: SPACE.into(),
        app: "counter".into(),
        digest: "7".repeat(64),
        title: "Canonical Counter".into(),
        revision: u64::MAX,
        bytes: 100,
        installed: Some("8".repeat(64)),
        affected_spaces: 3,
        expected_generation: 1,
        catalog: vec![],
        release: "9".repeat(64),
        audience: Audience::Closed,
        expires_unix_ms: u64::MAX,
    };
    assert!(client.spaces_outgoing().unwrap().is_none());
    client
        .receive_transfer(task.id, TransferReply::App { review })
        .unwrap();
    let panel = format!("{:?}", client.scene().unwrap());
    assert!(panel.contains("Canonical Counter"));
    assert!(panel.contains("Affected Spaces: 3"));
    assert!(client.spaces_outgoing().unwrap().is_none());
    click(&mut client, "Confirm Import");
    assert!(
        matches!(spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap(), Command::ImportApp { stage, .. } if stage == "6".repeat(32))
    );
    assert!(client.spaces_outgoing().unwrap().is_none());
}
#[test]
fn share_link_is_immutable_and_appears_only_after_confirmed_publication() {
    let mut client = transfer_fixture();
    let mut state = snapshot(Role::Admin, 2);
    state.catalog = client.catalog_hash().to_vec();
    let code = &mut state.selected.as_mut().unwrap().installed[0];
    code.bundle_sha256 = Some("7".repeat(64));
    code.title = "Canonical Counter".into();
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: Box::new(state),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    click(&mut client, "Edit");
    click(&mut client, "Counter");
    click(&mut client, "Share App");
    assert!(!labels(&client).contains(&"Copy link".into()));
    assert!(client.spaces_outgoing().unwrap().is_none());
    click(&mut client, "Publish App");
    let Command::ShareApp {
        id,
        app,
        digest,
        expected_generation,
        ..
    } = spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("share")
    };
    assert_eq!(app, "counter");
    assert_eq!(digest, "7".repeat(64));
    assert_eq!(expected_generation, 2);
    assert!(!labels(&client).contains(&"Copy link".into()));
    client
        .receive_spaces(
            &spaces::encode_event(&Event::AppShared {
                version: VERSION,
                id,
                status: ManagementStatus::Applied,
                share: Some(AppShare {
                    app,
                    digest,
                    title: "Canonical Counter".into(),
                }),
            })
            .unwrap(),
        )
        .unwrap();
    click(&mut client, "Copy link");
    let task = client.take_transfer_task().unwrap();
    assert_eq!(
        task.io,
        TransferIo::Copy {
            text: format!("https://otrios.example/apps/counter/{}", "7".repeat(64))
        }
    );
    assert!(client.spaces_outgoing().unwrap().is_none());
}
fn snapshot(role: Role, revision: u64) -> Discovery {
    let installed = InstalledApp {
        bundle_sha256: None,
        code_bytes: None,
        revision: None,
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
        profile: None,
        catalog: vec![],
        version: 1,
        cursor: DiscoveryCursor {
            authority: "c".repeat(32),
            revision,
        },
        generation: revision,
        identity: Some("alice".into()),
        spaces: vec![SpaceSummary {
            archived: false,
            retained: vec![],
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
            active_users: None,
            retained: vec![],
            cleanup: (role == Role::Admin)
                .then(CleanupSummary::default)
                .map(Box::new),
            space: SPACE.into(),
            apps: vec![AppSummary {
                active_users: None,
                app: "counter".into(),
                title: "Counter".into(),
                collection: "d".repeat(32),
                attached: true,
                availability: AppAvailability::Available,
                stored_bytes: (role == Role::Admin).then_some(1234),
                launch: Some(installed.clone().into()),
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
                snapshot: Box::new(snapshot(role, 1)),
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
fn open_access(client: &mut Client) {
    home(client);
    click(client, "Edit");
    click(client, "Space settings: Household");
    click(client, "People & access");
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
    // A short modal owns its own pan without exposing or remounting the map.
    use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
    let (mut client, _) = fixture(Role::Admin);
    client.set_viewport([390., 440.]).unwrap();
    click(&mut client, "Edit");
    click(&mut client, "Counter");
    assert!(client.resource_usage().map_scroll.is_none());
    assert!(client.resource_usage().panel_scroll.as_ref().unwrap().limit[1] > 0.);
    for hidden in ["Profile", "Done", "Space: Household"] {
        assert!(!labels(&client).contains(&hidden.into()));
    }
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let field = scene
        .controls()
        .into_iter()
        .find(|n| n.node.semantics.as_ref().unwrap().label == "Name")
        .unwrap()
        .origin;
    for (phase, y, at) in [
        (Down, field[1] + 32., 100),
        (Move, -100., 200),
        (Up, -100., 300),
    ] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 12,
                    phase,
                    position: [field[0] + 32., y],
                },
                at,
            )
            .unwrap();
    }
    assert_eq!(client.epoch(), epoch);
    assert!(
        !client.keyboard_visible(),
        "a pan starting on the name must not activate the field"
    );
    assert!(
        client
            .resource_usage()
            .panel_scroll
            .as_ref()
            .unwrap()
            .offset[1]
            > 0.
    );
    client.scroll_map(epoch, 1e6).unwrap();
    assert!(labels(&client).contains(&"Close".into()));
    client.scroll_map(epoch, -1e6).unwrap();
    assert!(labels(&client).contains(&"Name".into()));
    assert!(client.spaces_outgoing().unwrap().is_none());
    // Settings must remain admitted beside a populated map and native keyboard.
    let (mut client, _) = fixture(Role::Admin);
    let mut populated = snapshot(Role::Admin, 2);
    for index in 0..13 {
        let mut tile = populated.spaces[0].apps[0].clone();
        tile.app = format!("sample-{}", char::from(b'a' + index));
        tile.title = format!("Sample {index}");
        populated.spaces[0].apps.push(tile);
    }
    populated.spaces[0].attached_count = 14;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: Box::new(populated),
                resync: true,
            })
            .unwrap(),
        )
        .unwrap();
    click(&mut client, "Edit");
    let scene = client.scene().unwrap();
    let settings = scene
        .controls()
        .into_iter()
        .find(|c| c.node.semantics.as_ref().unwrap().label == "Space settings: Household")
        .unwrap();
    let position = [settings.origin[0] + 20., settings.origin[1] + 20.];
    let epoch = client.epoch();
    for phase in [Down, Up] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    phase,
                    id: 91,
                    position,
                },
                500,
            )
            .unwrap();
    }
    assert!(labels(&client).contains(&"People & access".into()));
    assert!(!labels(&client).contains(&"Profile".into()));
    assert!(!client.accessibility_text().unwrap().contains("Sample"));
    click(&mut client, "Name");
    assert!(client.keyboard_visible());
    client.hardware_edit(&Edit::SelectAll).unwrap();
    let scene = client.scene().unwrap();
    let key = scene
        .controls()
        .into_iter()
        .find(|c| c.node.semantics.as_ref().unwrap().label == "w")
        .unwrap();
    let position = [
        key.origin[0] + key.node.frame.width * key.scale / 2.,
        key.origin[1] + key.node.frame.height * key.scale / 2.,
    ];
    let epoch = client.epoch();
    for phase in [Down, Up] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    phase,
                    id: 92,
                    position,
                },
                600,
            )
            .unwrap();
    }
    assert_eq!(
        client.typed_value(1).unwrap(),
        otrios_interfaces::layout::Value::Text("w".into())
    );
    assert!(client.spaces_outgoing().unwrap().is_none());
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
    open_access(&mut client);
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
                snapshot: Box::new(snapshot(Role::Admin, 2)),
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
                snapshot: Box::new(snapshot(Role::Member, 2)),
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
        snapshot: Box::new(snapshot(Role::Admin, 1)),
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
    open_access(&mut client);
    client.select_space(SPACE).unwrap();
    assert!(client.spaces_outgoing().unwrap().is_some());
    assert!(
        !client.receive_spaces(&bytes).unwrap(),
        "old observation cannot reactivate controls"
    );
    let fresh = spaces::encode_event(&Event::Snapshot {
        observation: 2,
        snapshot: Box::new(snapshot(Role::Admin, 1)),
        resync: true,
    })
    .unwrap();
    assert!(
        client.receive_spaces(&fresh).unwrap(),
        "a new local activation needs a presentation even when metadata is unchanged"
    );
    assert!(client.spaces_ui());
    open_access(&mut client);
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
                snapshot: Box::new(snapshot(Role::Admin, 2)),
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
            snapshot: Box::new(removed.clone()),
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
                        snapshot: Box::new(snapshot(Role::Admin, 1)),
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
                    snapshot: Box::new(if reattached {
                        snapshot(Role::Member, 3)
                    } else {
                        removed
                    }),
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
fn unified_access_form_reviews_publication_preserves_local_drafts_and_revokes_on_role_loss() {
    let (mut client, _) = fixture(Role::Admin);
    open_access(&mut client);
    click(&mut client, "Role: alice");
    click(&mut client, "Save");
    assert!(
        client.spaces_outgoing().unwrap().is_none(),
        "keep at least one Admin"
    );
    click(&mut client, "Role: closed-admin-secret");
    click(&mut client, "Access");
    click(&mut client, "Public · Signed-in comments");
    click(&mut client, "Save");
    assert!(
        client.spaces_outgoing().unwrap().is_none(),
        "review before public access"
    );
    click(&mut client, "Cancel");
    click(&mut client, "Save");
    click(&mut client, "Apply");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("access request")
    };
    assert_eq!(
        request.change,
        Change::Access {
            space: SPACE.into(),
            audience: Audience::PublicComment,
            changes: vec![
                MembershipEdit {
                    identity: "alice".into(),
                    role: Some(Role::Member)
                },
                MembershipEdit {
                    identity: "closed-admin-secret".into(),
                    role: Some(Role::Admin)
                }
            ]
        }
    );
    let (mut client, _) = fixture(Role::Admin);
    open_access(&mut client);
    click(&mut client, "Access");
    click(&mut client, "Public · Read only");
    click(&mut client, "Save");
    let old_epoch = client.epoch();
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: Box::new(snapshot(Role::Member, 2)),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(client.take_collection_revoked());
    assert!(!labels(&client).contains(&"Apply".into()));
    assert!(!format!("{:?}", client.scene().unwrap()).contains("closed-admin-secret"));
    assert!(!client.activate(old_epoch, 1).unwrap());
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
    let (mut client, _) = fixture(Role::Member);
    click(&mut client, "Profile");
    click(&mut client, "Edit profile");
    click(&mut client, "Display name");
    client.hardware_edit(&Edit::SelectAll).unwrap();
    client.hardware_edit(&Edit::Commit("Maya".into())).unwrap();
    click(&mut client, "Cancel");
    assert!(client.spaces_outgoing().unwrap().is_none());
    click(&mut client, "Edit profile");
    click(&mut client, "Display name");
    client.hardware_edit(&Edit::SelectAll).unwrap();
    client.hardware_edit(&Edit::Commit("Maya".into())).unwrap();
    click(&mut client, "Save");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("self-profile")
    };
    assert_eq!(
        request.change,
        Change::Profile {
            name: "Maya".into(),
            expected_revision: 0
        }
    );
    assert_eq!(
        client.spaces_snapshot().unwrap().identity.as_deref(),
        Some("alice")
    );
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
    let mut renamed = snapshot(Role::Admin, 2);
    renamed.selected.as_mut().unwrap().apps[0].title = "Existing label".into();
    renamed.spaces[0].apps[0].title = "Existing label".into();
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: Box::new(renamed),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        client.typed_value(1).unwrap(),
        otrios_interfaces::layout::Value::Text("Existing label".into()),
        "untouched names adopt newer authority metadata"
    );
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
    assert_eq!(request.expected_generation, 2);
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
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Edit");
    click(&mut client, "Space settings: Household");
    click(&mut client, "Custom background color");
    assert!(!labels(&client).contains(&"Close".into()));
    click(&mut client, "Hex color");
    client.hardware_edit(&Edit::SelectAll).unwrap();
    client
        .hardware_edit(&Edit::Commit("bad hex".into()))
        .unwrap();
    click(&mut client, "Apply color");
    assert!(client.spaces_outgoing().unwrap().is_none());
    click(&mut client, "Hex color");
    client.hardware_edit(&Edit::SelectAll).unwrap();
    client
        .hardware_edit(&Edit::Commit("#14283C".into()))
        .unwrap();
    click(&mut client, "Apply color");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("custom color")
    };
    assert!(matches!(
        request.change,
        Change::ConfigureSpace {
            layout: SpaceLayout {
                background: Background::Rgb([20, 40, 60]),
                ..
            },
            ..
        }
    ));
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
                snapshot: Box::new(empty),
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
                snapshot: Box::new(next),
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
                snapshot: Box::new(next),
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
                snapshot: Box::new(next),
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
                snapshot: Box::new(next),
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
                snapshot: Box::new(visitor.clone()),
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
                snapshot: Box::new(visitor),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(client.auth_required());
    client.retry_mount();
    assert!(!client.auth_required());
}

#[test]
fn title_drag_holds_native_epoch_and_scrolls_to_an_offscreen_space() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
    for width in [390., 1280.] {
        let (mut client, _) = fixture(Role::Admin);
        client.set_viewport([width, 600.]).unwrap();
        let mut next = snapshot(Role::Admin, 2);
        next.spaces[0].layout.width = 6;
        next.spaces[0].layout.height = 6;
        let mut relay = next.spaces[0].clone();
        relay.id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
        relay.name = "Brass Relay".into();
        relay.layout.height = 1;
        next.spaces.push(relay);
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation: 1,
                    snapshot: Box::new(next),
                    resync: false,
                })
                .unwrap(),
            )
            .unwrap();
        click(&mut client, "Edit");
        let epoch = client.epoch();
        assert!(client.scroll_map(epoch, 100000.).unwrap());
        assert!(!labels(&client).contains(&"Space: Household".into()));
        let scene = client.scene().unwrap();
        let title = scene
            .controls()
            .into_iter()
            .find(|c| c.node.semantics.as_ref().unwrap().label == "Space: Brass Relay")
            .unwrap();
        // Touch blank title padding, then emulate native frames between samples.
        let start = [title.origin[0] + 4., title.origin[1] + 4.];
        for (phase, point, at) in [(Down, start, 100), (Move, [start[0], 108.], 120)] {
            client
                .pointer_at(
                    epoch,
                    PointerEvent {
                        id: 7,
                        phase,
                        position: point,
                    },
                    at,
                )
                .unwrap();
            client.scene().unwrap();
            assert_eq!(
                client.epoch(),
                epoch,
                "native capture must survive preview frames"
            );
        }
        assert!(client.input_busy());
        let mut presence = client.spaces_snapshot().unwrap().clone();
        presence.cursor.revision += 1;
        presence.selected.as_mut().unwrap().active_users = Some(2);
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation: 1,
                    snapshot: Box::new(presence),
                    resync: false,
                })
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            client.epoch(),
            epoch,
            "participant counts must not cancel a map gesture"
        );
        for at in (128..10000).step_by(16) {
            client.tick(at, 0).unwrap();
            client.scene().unwrap();
            assert_eq!(client.epoch(), epoch);
            if client.resource_usage().map_scroll.unwrap().offset[1] == 0. {
                break;
            }
        }
        assert_eq!(client.resource_usage().map_scroll.unwrap().offset[1], 0.);
        assert!(labels(&client).contains(&"Space: Household".into()));
        // The title is stationary at the edge throughout autoscroll.
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 7,
                    phase: Up,
                    position: [start[0], 108.],
                },
                10000,
            )
            .unwrap();
        let Command::Manage { request, .. } =
            spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("Space reorder")
        };
        assert_eq!(
            request.change,
            Change::PlaceSpace {
                space: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                before: Some(SPACE.into())
            }
        );
        assert_eq!(request.expected_generation, 2);
        assert!(!client.input_busy());
        assert!(client.outgoing().unwrap().is_none());
    }
}

#[test]
fn edit_drag_autoscroll_stops_in_middle_and_cancel_writes_nothing() {
    use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
    let (mut client, _) = fixture(Role::Admin);
    client.set_viewport([390., 600.]).unwrap();
    let mut next = snapshot(Role::Admin, 2);
    next.spaces[0].layout.height = 6;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: Box::new(next),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    click(&mut client, "Edit");
    let epoch = client.epoch();
    let scene = client.scene().unwrap();
    let title = scene
        .controls()
        .into_iter()
        .find(|c| c.node.semantics.as_ref().unwrap().label == "Space: Household")
        .unwrap();
    let start = [title.origin[0] + 10., title.origin[1] + 10.];
    for (phase, position, at) in [(Down, start, 100), (Move, [100., 510.], 120)] {
        client
            .pointer_at(
                epoch,
                PointerEvent {
                    id: 3,
                    phase,
                    position,
                },
                at,
            )
            .unwrap();
    }
    for at in (128..512).step_by(16) {
        client.tick(at, 0).unwrap();
    }
    let offset = client.resource_usage().map_scroll.unwrap().offset[1];
    assert!(offset > 80.);
    client
        .pointer_at(
            epoch,
            PointerEvent {
                id: 3,
                phase: Move,
                position: [100., 300.],
            },
            520,
        )
        .unwrap();
    client.tick(600, 0).unwrap();
    assert_eq!(
        client.resource_usage().map_scroll.unwrap().offset[1],
        offset
    );
    client.cancel().unwrap();
    client.tick(1000, 0).unwrap();
    assert_eq!(client.epoch(), epoch);
    assert!(!client.input_busy());
    assert!(client.spaces_outgoing().unwrap().is_none());
    assert_eq!(
        client.resource_usage().map_scroll.unwrap().offset[1],
        offset
    );
}

#[test]
fn map_delete_app_defaults_to_keep_data_and_checked_delete_targets_reviewed_collection() {
    for remove_data in [false, true] {
        let (mut client, _) = fixture(Role::Admin);
        click(&mut client, "Edit");
        click(&mut client, "Delete App: Counter in Household");
        assert!(client.spaces_outgoing().unwrap().is_none());
        assert!(labels(&client).contains(&"Remove Data".into()));
        if remove_data {
            click(&mut client, "Remove Data");
            assert!(client.spaces_outgoing().unwrap().is_none());
        }
        click(&mut client, "Delete");
        let Command::Manage { request, id, .. } =
            spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("delete request")
        };
        assert_eq!(request.expected_generation, 1);
        assert_eq!(
            request.change,
            if remove_data {
                Change::DeleteData {
                    space: SPACE.into(),
                    app: "counter".into(),
                    collection: "d".repeat(32),
                    remove: true,
                }
            } else {
                Change::Remove {
                    space: SPACE.into(),
                    app: "counter".into(),
                }
            }
        );
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Result {
                    version: VERSION,
                    id,
                    status: ManagementStatus::Applied,
                    created: None,
                })
                .unwrap(),
            )
            .unwrap();
        assert!(!format!("{:?}", client.scene().unwrap()).contains("Saved."));
    }
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Edit");
    click(&mut client, "Delete App: Counter in Household");
    click(&mut client, "Remove Data");
    click(&mut client, "Cancel");
    assert!(client.spaces_outgoing().unwrap().is_none());
    click(&mut client, "Delete App: Counter in Household");
    click(&mut client, "Delete");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("remove")
    };
    assert!(matches!(request.change, Change::Remove { .. }));
}
#[test]
fn map_delete_review_cannot_follow_generation_changes_or_lost_admin_access() {
    for role in [Role::Admin, Role::Member] {
        let (mut client, _) = fixture(Role::Admin);
        click(&mut client, "Edit");
        click(&mut client, "Delete App: Counter in Household");
        let mut changed = snapshot(role, 2);
        changed.selected.as_mut().unwrap().apps[0].collection = "e".repeat(32);
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation: 1,
                    snapshot: Box::new(changed),
                    resync: false,
                })
                .unwrap(),
            )
            .unwrap();
        assert!(!labels(&client).contains(&"Delete".into()));
        assert!(client.spaces_outgoing().unwrap().is_none());
    }
}
fn with_retained(mut snapshot: Discovery, backup: bool) -> Discovery {
    let data = DatasetSummary {
        app: "counter".into(),
        collection: if backup {
            "e".repeat(32)
        } else {
            "d".repeat(32)
        },
        title: if backup {
            "Earlier Counter".into()
        } else {
            "Counter".into()
        },
        availability: AppAvailability::Available,
        stored_bytes: Some(1234),
        origin: backup.then_some(DatasetOrigin::BeforeReplacement),
    };
    if !backup {
        snapshot.spaces[0].apps.clear();
        snapshot.spaces[0].attached_count = 0;
        let app = &mut snapshot.selected.as_mut().unwrap().apps[0];
        app.attached = false;
        app.launch = None;
    }
    snapshot.spaces[0].retained.push(data.clone());
    snapshot.selected.as_mut().unwrap().retained.push(data);
    snapshot
}
#[test]
fn retained_tiles_are_edit_only_and_restore_and_delete_name_the_exact_dataset() {
    for backup in [false, true] {
        for delete in [false, true] {
            let (mut client, _) = fixture(Role::Admin);
            client
                .receive_spaces(
                    &spaces::encode_event(&Event::Snapshot {
                        observation: 1,
                        snapshot: Box::new(with_retained(snapshot(Role::Admin, 2), backup)),
                        resync: false,
                    })
                    .unwrap(),
                )
                .unwrap();
            let title = if backup { "Earlier Counter" } else { "Counter" };
            let label = format!("{title} (data) in Household");
            assert!(!labels(&client).contains(&label));
            click(&mut client, "Edit");
            assert!(labels(&client).contains(&label));
            if delete {
                click(&mut client, &format!("Delete data: {title} in Household"));
            } else {
                click(&mut client, &label);
                click(&mut client, "Restore App");
            }
            assert!(client.spaces_outgoing().unwrap().is_none());
            click(&mut client, if delete { "Delete" } else { "Restore" });
            let Command::Manage { request, .. } =
                spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
            else {
                panic!("dataset action")
            };
            let cid = if backup {
                "e".repeat(32)
            } else {
                "d".repeat(32)
            };
            assert_eq!(request.expected_generation, 2);
            assert_eq!(
                request.change,
                if delete {
                    Change::DeleteDataset {
                        space: SPACE.into(),
                        app: "counter".into(),
                        collection: cid,
                    }
                } else {
                    Change::RestoreDataset {
                        space: SPACE.into(),
                        app: "counter".into(),
                        collection: cid,
                    }
                }
            );
        }
    }
}
#[test]
fn deleted_space_recovers_from_profile_with_current_admin_access() {
    let (mut client, _) = fixture(Role::Admin);
    click(&mut client, "Edit");
    click(&mut client, "Space settings: Household");
    click(&mut client, "Delete Space");
    assert!(client.spaces_outgoing().unwrap().is_none());
    click(&mut client, "Delete");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("space deletion")
    };
    assert_eq!(
        request.change,
        Change::DeleteSpace {
            space: SPACE.into(),
            remove_data: false
        }
    );
    let (mut client, _) = fixture(Role::Admin);
    let mut deleted = with_retained(snapshot(Role::Admin, 2), false);
    deleted.spaces[0].archived = true;
    client
        .receive_spaces(
            &spaces::encode_event(&Event::Snapshot {
                observation: 1,
                snapshot: Box::new(deleted),
                resync: false,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(!labels(&client).contains(&"Space: Household".into()));
    click(&mut client, "Edit");
    assert!(!labels(&client).contains(&"Space: Household".into()));
    click(&mut client, "Profile");
    click(&mut client, "Deleted Spaces");
    click(&mut client, "Restore Space: Household");
    let Command::Manage { request, .. } =
        spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("space restoration")
    };
    assert_eq!(
        request.change,
        Change::RestoreSpace {
            space: SPACE.into()
        }
    );
}

#[test]
fn cross_space_pickup_reviews_modes_publication_and_exact_current_bindings() {
    for (mode, ghost) in [
        (MoveMode::AppAndData, false),
        (MoveMode::AppOnly, false),
        (MoveMode::DataOnly, false),
        (MoveMode::DataOnly, true),
    ] {
        let (mut client, _) = fixture(Role::Admin);
        let mut state = snapshot(Role::Admin, 2);
        let destination = "f".repeat(32);
        let mut target = state.spaces[0].clone();
        target.id = destination.clone();
        target.name = "The Brass Relay".into();
        target.audience = Audience::PublicView;
        state.spaces.push(target);
        if ghost {
            state = with_retained(state, false);
        }
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation: 1,
                    snapshot: Box::new(state.clone()),
                    resync: false,
                })
                .unwrap(),
            )
            .unwrap();
        click(&mut client, "Edit");
        let id = client
            .scene()
            .unwrap()
            .controls()
            .iter()
            .find(|n| {
                n.node.semantics.as_ref().unwrap().label
                    == if ghost {
                        "Counter (data) in Household"
                    } else {
                        "Counter in Household"
                    }
            })
            .unwrap()
            .node
            .id;
        if mode == MoveMode::DataOnly {
            use otrios_interfaces::scene::{PointerEvent, PointerPhase::*};
            let scene = client.scene().unwrap();
            let from = scene.path(id).unwrap().last().unwrap().origin;
            let target = scene
                .controls()
                .into_iter()
                .find(|n| n.node.semantics.as_ref().unwrap().label == "Space: The Brass Relay")
                .unwrap()
                .origin;
            let epoch = client.epoch();
            for (phase, position, at) in [
                (Down, [from[0] + 30., from[1] + 40.], 100),
                (Move, [target[0] + 100., target[1] + 20.], 200),
                (Up, [target[0] + 100., target[1] + 20.], 300),
            ] {
                client
                    .pointer_at(
                        epoch,
                        PointerEvent {
                            id: 7,
                            phase,
                            position,
                        },
                        at,
                    )
                    .unwrap();
            }
        } else {
            assert!(
                client.adjust_control(id, 0, 0).unwrap(),
                "keyboard pickup uses the drop review"
            );
            assert!(
                client.spaces_outgoing().unwrap().is_none(),
                "pickup does not mutate"
            );
            click(&mut client, "Move to The Brass Relay");
        }
        assert_eq!(client.take_space_switch(), Some(destination.clone()));
        client.select_space(&destination).unwrap();
        let Command::Watch { observation, .. } =
            spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("watch destination")
        };
        let detail = state.selected.as_mut().unwrap();
        detail.space = destination.clone();
        detail.apps[0].collection = "e".repeat(32);
        client
            .receive_spaces(
                &spaces::encode_event(&Event::Snapshot {
                    observation,
                    snapshot: Box::new(state),
                    resync: true,
                })
                .unwrap(),
            )
            .unwrap();
        let caption = match mode {
            MoveMode::AppAndData => "App and data",
            MoveMode::AppOnly => "App only",
            MoveMode::DataOnly => "Data only",
        };
        click(&mut client, caption);
        assert!(
            client.spaces_outgoing().unwrap().is_none(),
            "mode choice does not mutate"
        );
        click(
            &mut client,
            if mode == MoveMode::AppOnly {
                "Move"
            } else {
                "Move and publish data"
            },
        );
        let Command::Manage { request, .. } =
            spaces::decode_command(&client.spaces_outgoing().unwrap().unwrap()).unwrap()
        else {
            panic!("scoped move")
        };
        assert_eq!(request.expected_generation, 2);
        let Change::Move {
            operation,
            source,
            destination: actual,
            collection,
            destination_collection,
            mode: actual_mode,
            ..
        } = request.change
        else {
            panic!("move")
        };
        assert_eq!(operation.len(), 32);
        assert_eq!(source, SPACE);
        assert_eq!(actual, destination);
        assert_eq!(collection, "d".repeat(32));
        assert_eq!(destination_collection, Some("e".repeat(32)));
        assert_eq!(actual_mode, mode);
    }
}
