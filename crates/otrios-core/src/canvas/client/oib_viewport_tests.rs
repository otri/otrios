// SPDX-License-Identifier: LGPL-3.0-only
//! Real signed Designer layouts retain responsive sizing through definition mode.
use super::tests::mounted_fixture_session;
use super::*;
use otrios_interfaces::{
    layout::ViewportPolicy,
    oib::designer::DefinitionId,
    scene::{Action, TouchResponder},
};

fn assert_extent_and_tabs(client: &mut Client, size: [f32; 2]) {
    let scene = client.scene().unwrap();
    assert_eq!([scene.root().frame.width, scene.root().frame.height], size);
    for id in [
        oib_workspace::TAB_LIBRARY,
        oib_workspace::TAB_LAYERS,
        oib_workspace::TAB_INSPECT,
    ] {
        let located = scene
            .controls()
            .into_iter()
            .find(|control| control.node.id == id)
            .unwrap();
        let point = [
            located.origin[0] + located.node.frame.width * located.scale / 2.,
            located.origin[1] + located.node.frame.height * located.scale / 2.,
        ];
        assert!(point[0] >= 0. && point[0] < size[0] && point[1] >= 0. && point[1] < size[1]);
        assert!(scene.hit_path(point).iter().any(|node| node.node.id == id));
    }
}
#[test]
fn responsive_designer_uses_real_window_geometry_in_composition_and_definition_modes() {
    let (mut client, _authority, _session) = mounted_fixture_session(true);
    let expected = ViewportPolicy::Responsive {
        minimum: [800., 480.],
    };
    assert_eq!(client.viewport_policy(), expected);
    for size in [[1280., 800.], [1920., 1080.], [1600., 900.], [800., 480.]] {
        client.set_viewport(size).unwrap();
        assert_extent_and_tabs(&mut client, size);
    }
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(12)))
        .unwrap();
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .definition_presentation
            .is_some()
    );
    assert!(
        client
            .active
            .as_ref()
            .unwrap()
            .layout
            .blueprint()
            .composition
            .is_none()
    );
    assert_eq!(client.viewport_policy(), expected);
    for size in [[1920., 1080.], [1280., 800.]] {
        client.set_viewport(size).unwrap();
        assert_extent_and_tabs(&mut client, size);
    }
}
#[test]
fn responsive_designer_defers_real_slider_geometry_until_captured_release() {
    let (mut client, _authority, _session) = mounted_fixture_session(true);
    client.set_viewport([1600., 900.]).unwrap();
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(12)))
        .unwrap();
    client.set_oib_designer_mode(DesignerMode::Test).unwrap();
    let before = client.active.as_ref().unwrap().doc.oib_root();
    let scene = client.scene().unwrap();
    let slider = scene
        .controls()
        .into_iter()
        .find(|control| {
            matches!(
                control.node.responder,
                Some(TouchResponder::Drag(Action::AppEvent(_)))
            )
        })
        .unwrap();
    let point = [
        slider.origin[0] + slider.node.frame.width * slider.scale * 0.75,
        slider.origin[1] + slider.node.frame.height * slider.scale / 2.,
    ];
    client
        .pointer_at(
            client.epoch(),
            PointerEvent {
                id: 77,
                phase: PointerPhase::Down,
                position: point,
            },
            1000,
        )
        .unwrap();
    assert!(client.input_busy());
    client.set_viewport([1920., 1080.]).unwrap();
    assert_extent_and_tabs(&mut client, [1600., 900.]);
    client
        .pointer_at(
            client.epoch(),
            PointerEvent {
                id: 77,
                phase: PointerPhase::Up,
                position: point,
            },
            1010,
        )
        .unwrap();
    client.tick(1020, 0).unwrap();
    assert!(!client.input_busy());
    assert_extent_and_tabs(&mut client, [1920., 1080.]);
    assert_eq!(client.active.as_ref().unwrap().doc.oib_root(), before);
    assert!(
        client
            .active
            .as_mut()
            .unwrap()
            .doc
            .generate()
            .unwrap()
            .is_none()
    );
}

#[test]
fn responsive_designer_same_request_reconciles_actual_presented_geometry() {
    let (mut client, _authority, _session) = mounted_fixture_session(true);
    client.set_viewport([1920., 1080.]).unwrap();
    client
        .inspect_oib_selection(Selection::Definition(DefinitionId(12)))
        .unwrap();
    assert_extent_and_tabs(&mut client, [1920., 1080.]);
    // A previous Fit mount can record a request without adopting its extent.
    // Policy entry must reconcile actual geometry even when that request repeats.
    client.viewport = Some([1280., 800.]);
    assert!(client.set_viewport([1280., 800.]).unwrap());
    assert_extent_and_tabs(&mut client, [1280., 800.]);
    assert!(!client.set_viewport([1280., 800.]).unwrap());
}
