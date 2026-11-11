// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use crate::dashboard::components::canvas;
use oib_workspace::{Appearance, MODAL_ACTION_START, MODAL_LINK_START, ModalAction, ModalLink};
use otrios_interfaces::scene::{Rect, SceneNode};

fn modal() -> Modal {
    Modal {
        title: "Component dependencies".into(),
        lines: (0..80).map(|n| format!("Used by component {n}")).collect(),
        links: (0..80)
            .map(|n| ModalLink {
                line: n,
                id: MODAL_LINK_START + n as u32,
            })
            .collect(),
        actions: vec![
            ModalAction {
                id: MODAL_ACTION_START,
                label: "Cancel".into(),
                enabled: true,
            },
            ModalAction {
                id: MODAL_ACTION_START + 1,
                label: "Delete".into(),
                enabled: false,
            },
        ],
        scroll: Some(0),
    }
}
fn scene(modal: &Modal, width: f32, height: f32) -> Scene {
    let mut root = canvas(1, Rect::new(0., 0., width, height), vec![]);
    oib_workspace::present_modal(&mut root, Some(modal), false, Appearance::Dark).unwrap();
    Scene::new(root).unwrap()
}
fn point(scene: &Scene, id: u32) -> [f32; 2] {
    let path = scene.path(id).unwrap();
    let node = path.last().unwrap();
    [node.origin[0] + 24., node.origin[1] + 24.]
}
fn count(node: &SceneNode) -> usize {
    1 + node.children.iter().map(count).sum::<usize>()
}
fn event(id: u32, phase: PointerPhase, position: [f32; 2]) -> PointerEvent {
    PointerEvent {
        id,
        phase,
        position,
    }
}

#[test]
fn dialog_links_scroll_cull_wrap_and_keep_clipped_rows_clickable() {
    for (width, height) in [(800., 480.), (1280., 800.), (1920., 1080.)] {
        let mut model = modal();
        model.lines[0] = "Used by an admitted component with a long descriptive name and a nested reference path that wraps".into();
        let initial = scene(&model, width, height);
        let path = initial.path(MODAL_LINK_START).unwrap();
        assert!(path.last().unwrap().node.frame.height >= 64.);
        assert!(initial.controls().len() < 24);
        assert!(count(initial.root()) < 160);
        assert!(initial.path(MODAL_LINK_START + 79).is_none());
        let limit = oib_workspace::modal_scroll_limit(&model, width, height).unwrap();
        model.scroll = Some(limit);
        let end = scene(&model, width, height);
        assert!(end.path(MODAL_LINK_START + 79).is_some());
        assert!(end.path(MODAL_LINK_START).is_none());
        assert!(end.controls().len() < 24);
        assert!(count(end.root()) < 160);
        model.scroll = Some(13);
        let clipped = scene(&model, width, height);
        let viewport = clipped.path(MODAL_CONTENT).unwrap();
        let viewport = viewport.last().unwrap();
        let at = [viewport.origin[0] + 24., viewport.origin[1] + 5.];
        assert!(
            clipped
                .hit_path(at)
                .iter()
                .any(|n| n.node.id == MODAL_LINK_START)
        );
        let (mut capture, mut scroll) = (None, 13);
        assert!(
            update(
                &mut capture,
                event(1, PointerPhase::Down, at),
                &clipped,
                &model,
                &mut scroll
            )
            .unwrap()
            .action
            .is_none()
        );
        assert_eq!(
            update(
                &mut capture,
                event(1, PointerPhase::Up, at),
                &clipped,
                &model,
                &mut scroll
            )
            .unwrap()
            .action,
            Some(MODAL_LINK_START)
        );
        assert!(capture.is_none());
    }
}

#[test]
fn dialog_capture_cancels_drag_foreign_release_and_background_actions() {
    let model = modal();
    let scene = scene(&model, 800., 480.);
    let at = point(&scene, MODAL_LINK_START);
    let (mut capture, mut scroll) = (None, 0);
    update(
        &mut capture,
        event(1, PointerPhase::Down, at),
        &scene,
        &model,
        &mut scroll,
    )
    .unwrap();
    assert!(
        update(
            &mut capture,
            event(2, PointerPhase::Up, at),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action
        .is_none()
    );
    assert!(capture.is_some());
    assert!(
        update(
            &mut capture,
            event(1, PointerPhase::Cancel, at),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action
        .is_none()
    );
    assert!(capture.is_none());
    update(
        &mut capture,
        event(1, PointerPhase::Down, at),
        &scene,
        &model,
        &mut scroll,
    )
    .unwrap();
    assert!(
        update(
            &mut capture,
            event(1, PointerPhase::Up, [1., 1.]),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action
        .is_none()
    );
    update(
        &mut capture,
        event(1, PointerPhase::Down, at),
        &scene,
        &model,
        &mut scroll,
    )
    .unwrap();
    let moved = [at[0], at[1] - 100.];
    assert!(
        update(
            &mut capture,
            event(1, PointerPhase::Move, moved),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .changed
    );
    assert_eq!(scroll, 100);
    assert!(
        update(
            &mut capture,
            event(1, PointerPhase::Up, moved),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action
        .is_none()
    );
    let close = point(&scene, MODAL_ACTION_START);
    update(
        &mut capture,
        event(1, PointerPhase::Down, close),
        &scene,
        &model,
        &mut scroll,
    )
    .unwrap();
    assert_eq!(
        update(
            &mut capture,
            event(1, PointerPhase::Up, close),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action,
        Some(MODAL_ACTION_START)
    );
    let disabled = point(&scene, MODAL_ACTION_START + 1);
    update(
        &mut capture,
        event(1, PointerPhase::Down, disabled),
        &scene,
        &model,
        &mut scroll,
    )
    .unwrap();
    assert!(
        update(
            &mut capture,
            event(1, PointerPhase::Up, disabled),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action
        .is_none()
    );
    update(
        &mut capture,
        event(1, PointerPhase::Down, [1., 1.]),
        &scene,
        &model,
        &mut scroll,
    )
    .unwrap();
    assert!(
        update(
            &mut capture,
            event(1, PointerPhase::Up, [1., 1.]),
            &scene,
            &model,
            &mut scroll
        )
        .unwrap()
        .action
        .is_none()
    );
}

#[test]
fn dialog_full_list_measurement_preserves_the_existing_content_gate() {
    let mut model = modal();
    model.lines = (0..200).map(|n| format!("Used by component {n}")).collect();
    model.links = (0..200)
        .map(|n| ModalLink {
            line: n,
            id: MODAL_LINK_START + n as u32,
        })
        .collect();
    assert!(oib_workspace::modal_content_height(&model, 800.).unwrap() > 8192.);
    assert!(oib_workspace::modal_scroll_limit(&model, 800., 480.).is_err());
    let mut root = canvas(1, Rect::new(0., 0., 800., 480.), vec![]);
    assert!(
        oib_workspace::present_modal(&mut root, Some(&model), false, Appearance::Dark).is_err()
    );
    model = modal();
    model.links[1].id = model.links[0].id;
    assert!(oib_workspace::modal_scroll_limit(&model, 800., 480.).is_err());
    model = modal();
    model.links[1].line = model.links[0].line;
    assert!(oib_workspace::modal_scroll_limit(&model, 800., 480.).is_err());
}
