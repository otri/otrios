// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::{authoring as a, canvas::*};
use otrios_interfaces::{
    canvas::*,
    channels as session, ink,
    scene::{PointerEvent, PointerPhase},
};

fn fixture(editable: bool) -> Client {
    let x = a::StateField::normalized(1, "x", 0);
    let y = a::StateField::normalized(2, "y", 0);
    let ui = a::AppUi::new("pad", "Pad", a::schema("pad-v1", &[x, y]))
        .with_touch_pad("pad")
        .build(
            "Pad",
            vec![
                a::canvas(vec![a::surface(a::CARD, 10.).pin_edges(0.)])
                    .key("pad")
                    .frame([24., 160., 1200., 600.])
                    .on_drag("Touch pad", a::Event::pad(x, y)),
            ],
        );
    let bytes = encode_ui(&ui).unwrap();
    let mut client = Client::default();
    client
        .admit_catalog(
            &encode_catalog(&Catalog {
                format: "otrios-app-catalog-v1".into(),
                apps: vec![CatalogEntry {
                    app: App {
                        id: "pad".into(),
                        title: "Pad".into(),
                        ui_ref: ui.ui_ref.clone(),
                        state_ref: "app:pad/state".into(),
                        status: "canvas-app".into(),
                        dependencies: vec![],
                    },
                    path: "pad".into(),
                    ui_hash: hash(&bytes),
                    private: false,
                }],
            })
            .unwrap(),
        )
        .unwrap();
    client.open(&ui.ui_ref, "app:pad/state");
    let ClientMessage::Mount { epoch, writer, .. } =
        decode_client(&client.outgoing().unwrap().unwrap()).unwrap()
    else {
        panic!("mount")
    };
    let mut document = Document::new(ui.state_schema, "app:pad/state").unwrap();
    let (mut replica, _) = document.open_session(&writer, 0).unwrap();
    client
        .receive(
            &encode_server(&ServerMessage::Mounted {
                epoch,
                ui: bytes.clone(),
                ui_hash: hash(&bytes),
                state: document.generate(&mut replica).unwrap().unwrap(),
                code: vec![],
                binding: Some(MountBinding {
                    version: 1,
                    collection: "a".repeat(32),
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
    client
}

#[test]
fn ink_uses_its_accepted_epoch_and_read_only_admission_cannot_publish_points() {
    // Replay the same pointer positions after read-only admission, even though
    // the current presentation no longer advertises the writable responder.
    let reference = fixture(true).scene().unwrap();
    let pad = reference
        .controls()
        .into_iter()
        .find(|view| view.node.semantics.as_ref().unwrap().label == "Touch pad")
        .unwrap();
    for editable in [true, false] {
        let mut client = fixture(editable);
        assert_eq!(
            client.scene().unwrap().controls().iter().any(|view| view
                .node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == "Touch pad")),
            editable
        );
        client
            .pointer(
                client.epoch(),
                PointerEvent {
                    id: 1,
                    phase: PointerPhase::Down,
                    position: [
                        pad.origin[0] + pad.node.frame.width * pad.scale / 4.,
                        pad.origin[1] + pad.node.frame.height * pad.scale / 4.,
                    ],
                },
            )
            .unwrap();
        let bytes = client.ink_outgoing(1000);
        if editable {
            let (epoch, payload) = session::scope(&bytes).unwrap();
            assert_eq!(epoch, client.epoch());
            assert_eq!(ink::Point::decode(payload).unwrap().position, [250, 250]);
        } else {
            assert!(bytes.is_empty());
        }
        let point = ink::Point {
            phase: 0,
            stroke: 1,
            position: [400, 600],
        };
        assert!(
            client
                .receive_ink(
                    &session::scoped(client.epoch(), ink::envelope(77, 0, point)),
                    1001
                )
                .unwrap()
        );
    }
}

#[test]
fn old_auxiliary_frames_are_discarded_before_decoding_and_cannot_reopen_a_disposed_view() {
    let mut client = fixture(true);
    let epoch = client.epoch();
    let stale = session::scoped(epoch + 1, vec![255]);
    assert!(!client.receive_ink(&stale, 1000).unwrap());
    assert!(!client.receive_chat(&stale).unwrap());
    assert!(!client.receive_collection(&stale).unwrap());
    assert!(client.receive_ink(&[1, 2, 3], 1000).is_err());
    assert!(client.receive_chat(&[0, 0, 0, 0]).is_err());
    client.clear_private();
    assert!(client.scene().is_err());
    let old = session::scoped(epoch, vec![255]);
    assert!(!client.receive_ink(&old, 1001).unwrap());
    assert!(!client.receive_chat(&old).unwrap());
    assert!(!client.receive_collection(&old).unwrap());
    assert!(client.scene().is_err());
}
