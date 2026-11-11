// SPDX-License-Identifier: LGPL-3.0-only
//! Focused real Scene checks; not Frame/live or Linux IPC acceptance.
#![cfg(feature = "dashboard")]
pub use otrios_core::dashboard;
#[path = "../src/canvas/client/oib_designer/style.rs"]
mod style;

use dashboard::{
    components::{background, canvas, text},
    input::InputRouter,
    scene::Scene,
};
use otrios_interfaces::{
    scene::*,
    theme::{BG, BRASS, CARD, LINE, TEXT},
};
use std::sync::Arc;
use style::{resize_surface, set_surface_fill, surface_with_outline};

fn shape(node: &SceneNode) -> (Color, f32) {
    let Primitive::Superellipse {
        color,
        radius,
        exponent,
    } = node.primitive
    else {
        panic!("expected superellipse")
    };
    assert_eq!(exponent, 4.);
    (color, radius)
}

fn admit(node: SceneNode) -> Scene {
    Scene::new(canvas(0, Rect::new(0., 0., 800., 600.), vec![node])).unwrap()
}

#[test]
fn authored_outline_and_inset_survive_resize_and_fill_updates() {
    let mut node = surface_with_outline([10, 11, 12], 100., 40., CARD, LINE, 12.);
    assert_eq!(node.frame, Rect::new(0., 0., 100., 40.));
    assert_eq!(node.primitive, Primitive::Canvas { clip: true });
    assert_eq!(node.children[0].frame, Rect::new(0., 0., 100., 40.));
    assert_eq!(node.children[1].frame, Rect::new(1.5, 1.5, 97., 37.));
    assert_eq!(shape(&node.children[0]), (LINE, 12.));
    assert_eq!(shape(&node.children[1]), (CARD, 10.5));
    admit(node.clone());

    node.frame.x = 20.;
    node.frame.y = 30.;
    resize_surface(&mut node, 80., 16.).unwrap();
    assert_eq!(node.frame, Rect::new(20., 30., 80., 16.));
    assert_eq!(node.children[0].frame, Rect::new(0., 0., 80., 16.));
    assert_eq!(node.children[1].frame, Rect::new(1.5, 1.5, 77., 13.));
    assert_eq!(shape(&node.children[0]), (LINE, 8.));
    assert_eq!(shape(&node.children[1]), (CARD, 6.5));
    let before = node.clone();
    set_surface_fill(&mut node, BG).unwrap();
    let mut expected = before;
    if let Primitive::Superellipse { color, .. } = &mut expected.children[1].primitive {
        *color = BG;
    }
    assert_eq!(node, expected);
    assert_eq!(
        [node.id, node.children[0].id, node.children[1].id],
        [10, 11, 12]
    );
    assert!(
        std::iter::once(&node)
            .chain(&node.children)
            .all(|n| n.responder == Some(TouchResponder::PassThrough))
    );
    admit(node);
}

#[test]
fn finite_positive_geometry_and_radii_are_admitted_at_boundaries() {
    for (w, h) in [
        (0., -1.),
        (f32::NAN, f32::INFINITY),
        (9000., 9000.),
        (0.01, 2.),
        (1., 1.),
    ] {
        for r in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -10., 0., 10000.] {
            let mut node = surface_with_outline([10, 11, 12], w, h, CARD, LINE, r);
            admit(node.clone());
            for (width, height) in [(0.01, 0.02), (8192., 8192.), (3., 3.), (80., 40.)] {
                resize_surface(&mut node, width, height).unwrap();
                set_surface_fill(&mut node, BRASS).unwrap();
                admit(node.clone());
            }
        }
    }
}

#[test]
fn plain_flat_rows_preserve_identity_position_input_and_exponent() {
    let mut row = background(17, 120., 38., CARD, 12.);
    row.frame.x = 8.;
    row.frame.y = 9.;
    row.responder = Some(TouchResponder::PassThrough);
    if let Primitive::Superellipse { exponent, .. } = &mut row.primitive {
        *exponent = 2.;
    }
    let mut expected = row.clone();
    resize_surface(&mut row, 80., 10.).unwrap();
    set_surface_fill(&mut row, BRASS).unwrap();
    expected.frame.width = 80.;
    expected.frame.height = 10.;
    expected.primitive = Primitive::Superellipse {
        color: BRASS,
        radius: 5.,
        exponent: 2.,
    };
    assert_eq!(row, expected);
    admit(row);
}

#[test]
fn unsupported_or_malformed_nodes_and_invalid_sizes_are_atomic_errors() {
    let good = surface_with_outline([10, 11, 12], 100., 40., CARD, LINE, 12.);
    let mut malformed = vec![
        canvas(20, Rect::new(0., 0., 100., 40.), vec![]),
        text(21, Rect::new(0., 0., 100., 40.), "label", 2, TEXT),
    ];
    for change in 0..10 {
        let mut node = good.clone();
        match change {
            0 => node.children.reverse(),
            1 => node.children[0].frame.width -= 1.,
            2 => node.children[1].frame.x = 0.,
            3 => node.responder = None,
            4 => node.children[1].responder = Some(TouchResponder::Consume),
            5 => node.children[1]
                .children
                .push(background(30, 4., 4., CARD, 0.)),
            6 => node.children[1].id = node.children[0].id,
            7 => {
                node.children[0].primitive = Primitive::Superellipse {
                    color: BRASS,
                    radius: 12.,
                    exponent: 1.,
                }
            }
            8 => {
                node.children[1].primitive = Primitive::Superellipse {
                    color: CARD,
                    radius: 2.,
                    exponent: 4.,
                }
            }
            _ => node.primitive = Primitive::Canvas { clip: false },
        }
        malformed.push(node);
    }
    for mut node in malformed {
        let before = node.clone();
        assert!(resize_surface(&mut node, 60., 20.).is_err());
        assert_eq!(node, before);
        assert!(set_surface_fill(&mut node, BG).is_err());
        assert_eq!(node, before);
    }
    for size in [0., -1., f32::NAN, f32::INFINITY, 8193.] {
        for (w, h) in [(size, 40.), (100., size)] {
            let mut node = good.clone();
            assert!(resize_surface(&mut node, w, h).is_err());
            assert_eq!(node, good);
        }
    }
}

#[test]
fn decoration_exposes_siblings_and_parent_whole_view_hits_respect_clipping() {
    let mut underlying = canvas(1, Rect::new(0., 0., 100., 40.), vec![]);
    underlying.responder = Some(TouchResponder::Activate(Action::AppEvent(1)));
    let decoration = surface_with_outline([10, 11, 12], 100., 40., CARD, LINE, 16.);
    let scene = Scene::new(canvas(
        0,
        Rect::new(0., 0., 100., 40.),
        vec![underlying, decoration.clone()],
    ))
    .unwrap();
    assert_eq!(scene.hit_path([50., 20.]).last().unwrap().node.id, 1);

    let mut parent = canvas(2, Rect::new(10., 10., 100., 40.), vec![decoration]);
    parent.responder = Some(TouchResponder::Activate(Action::AppEvent(2)));
    let scene = Arc::new(Scene::new(canvas(0, Rect::new(0., 0., 80., 60.), vec![parent])).unwrap());
    for point in [[10., 10.], [50., 30.], [79., 49.]] {
        assert_eq!(scene.hit_path(point).last().unwrap().node.id, 2);
        let mut router = InputRouter::default();
        let down = router.dispatch(
            scene.clone(),
            PointerEvent {
                id: 1,
                position: point,
                phase: PointerPhase::Down,
            },
        );
        assert_eq!(down.responder, Some(2));
        let up = router.dispatch(
            scene.clone(),
            PointerEvent {
                id: 1,
                position: point,
                phase: PointerPhase::Up,
            },
        );
        assert_eq!(up.action, Some(Action::AppEvent(2)));
    }
    assert!(scene.hit_path([80., 30.]).is_empty());
    assert!(scene.hit_path([9., 30.]).is_empty());
}
