// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
fn literal(value: f32) -> Expr {
    Expr::Literal(Value::Number(value))
}
fn node(id: u16, width: f32, height: f32, children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode {
        id,
        layout: None,
        reference_root: false,
        resize: None,
        frame: [literal(0.), literal(0.), literal(width), literal(height)],
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: true },
        responder: None,
        semantics: None,
        children,
    }
}
fn policy(width: Sizing, height: Sizing, flow: Option<AutoLayout>) -> NodeLayout {
    NodeLayout {
        width: AxisSizing {
            mode: width,
            ..Default::default()
        },
        height: AxisSizing {
            mode: height,
            ..Default::default()
        },
        container: flow,
        ..Default::default()
    }
}
fn blueprint(child: TemplateNode) -> Blueprint {
    Blueprint {
        viewport: ViewportPolicy::Fit,
        schema: "otrios-layout-v6".into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "Auto Layout fixture".into(),
            parameters: vec![],
            root: node(0, 1280., 800., vec![child]),
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    }
}
fn resolved(child: TemplateNode) -> Scene {
    Layout::from_blueprint(blueprint(child))
        .unwrap()
        .resolve_slots(&BTreeMap::new())
        .unwrap()
}
fn frame(scene: &Scene, id: u32) -> Rect {
    scene.path(id).unwrap().last().unwrap().node.frame
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "{a} != {b}");
}
#[test]
fn every_direction_alignment_and_sizing_mode_uses_real_group_geometry() {
    let mut cases = 0;
    for direction in [Direction::Horizontal, Direction::Vertical] {
        let main = usize::from(direction == Direction::Vertical);
        for mode in [Sizing::Hug, Sizing::Fixed, Sizing::Fill] {
            for packing in [Packing::Start, Packing::Center, Packing::End] {
                for alignment in [
                    CrossAlignment::Start,
                    CrossAlignment::Center,
                    CrossAlignment::End,
                ] {
                    let flow = AutoLayout {
                        direction,
                        padding: [8.; 4],
                        gap: 4.,
                        packing,
                        alignment,
                    };
                    let mut group = node(
                        1,
                        152.,
                        96.,
                        vec![
                            node(2, 20., 20., vec![]),
                            node(3, 30., 30., vec![]),
                            node(4, 20., 20., vec![]),
                        ],
                    );
                    group.layout = Some(policy(mode, mode, Some(flow)));
                    let scene = resolved(group);
                    let group = frame(&scene, 1);
                    let size = [group.width, group.height];
                    let expected = match mode {
                        Sizing::Hug => {
                            if main == 0 {
                                [94., 46.]
                            } else {
                                [46., 94.]
                            }
                        }
                        Sizing::Fixed => [152., 96.],
                        Sizing::Fill => [1280., 800.],
                    };
                    assert_eq!(size, expected);
                    let spare = (size[main] - 16. - 78.).max(0.);
                    let mut cursor = 8.
                        + match packing {
                            Packing::Center => spare / 2.,
                            Packing::End => spare,
                            _ => 0.,
                        };
                    for (id, length) in [(2, 20.), (3, 30.), (4, 20.)] {
                        let child = frame(&scene, id);
                        let p = [child.x, child.y];
                        near(p[main], cursor);
                        let cross_spare = (size[1 - main] - 16. - length).max(0.);
                        near(
                            p[1 - main],
                            8. + match alignment {
                                CrossAlignment::Center => cross_spare / 2.,
                                CrossAlignment::End => cross_spare,
                                _ => 0.,
                            },
                        );
                        cursor += length + 4.;
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 54);
}
#[test]
fn fill_redistributes_bounds_and_preserves_minimum_overflow() {
    let mut a = node(2, 20., 20., vec![]);
    a.layout = Some(policy(Sizing::Fill, Sizing::Fixed, None));
    a.layout.as_mut().unwrap().width.max = Some(30.);
    let mut b = node(3, 20., 20., vec![]);
    b.layout = Some(policy(Sizing::Fill, Sizing::Fixed, None));
    let mut group = node(1, 100., 40., vec![a, b]);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Fixed,
        Some(AutoLayout {
            direction: Direction::Horizontal,
            ..Default::default()
        }),
    ));
    let scene = resolved(group.clone());
    assert_eq!(frame(&scene, 2).width, 30.);
    assert_eq!(frame(&scene, 3).width, 70.);
    group.children[0].layout.as_mut().unwrap().width.max = None;
    group.children[0].layout.as_mut().unwrap().width.min = 80.;
    group.children[1].layout.as_mut().unwrap().width.min = 60.;
    let scene = resolved(group);
    assert_eq!(frame(&scene, 2).width, 80.);
    assert_eq!(frame(&scene, 3).width, 60.);
    assert_eq!(frame(&scene, 3).x, 80.);
}
#[test]
fn free_children_do_not_change_hug_and_use_padding_box_coordinates() {
    let mut free = node(3, 100., 100., vec![]);
    free.frame[0] = literal(30.);
    free.frame[1] = literal(40.);
    free.layout = Some(NodeLayout {
        placement: Placement::Free,
        ..Default::default()
    });
    let mut group = node(1, 100., 100., vec![node(2, 20., 20., vec![]), free]);
    group.layout = Some(policy(
        Sizing::Hug,
        Sizing::Hug,
        Some(AutoLayout {
            padding: [4., 5., 6., 7.],
            ..Default::default()
        }),
    ));
    let scene = resolved(group);
    assert_eq!(
        [frame(&scene, 1).width, frame(&scene, 1).height],
        [30., 32.]
    );
    assert_eq!([frame(&scene, 3).x, frame(&scene, 3).y], [34., 45.]);
}
#[test]
fn fill_in_hug_same_axis_has_dependency_path_and_other_axis_is_legal() {
    let mut child = node(2, 20., 20., vec![]);
    child.layout = Some(policy(Sizing::Fill, Sizing::Fixed, None));
    let mut group = node(1, 100., 100., vec![child]);
    group.layout = Some(policy(
        Sizing::Hug,
        Sizing::Fixed,
        Some(AutoLayout::default()),
    ));
    let error = Layout::from_blueprint(blueprint(group.clone()))
        .err()
        .unwrap();
    assert!(
        error.contains("dependency") && error.contains("width") && error.contains('2'),
        "{error}"
    );
    group.layout.as_mut().unwrap().width.mode = Sizing::Fixed;
    group.layout.as_mut().unwrap().height.mode = Sizing::Hug;
    let scene = resolved(group);
    assert_eq!(frame(&scene, 1).height, 20.);
    assert_eq!(frame(&scene, 2).width, 100.);
}
#[test]
fn text_hug_uses_admitted_font_and_fixed_height_is_independent() {
    let mut text_node = node(1, 150., 42., vec![]);
    text_node.layout = Some(policy(Sizing::Hug, Sizing::Fixed, None));
    text_node.shape = Shape::Text {
        value: Expr::State(Binding::Local(7)),
        scale: literal(2.),
        color: Expr::Literal(Value::Color([255; 4])),
    };
    let layout = Layout::from_blueprint(blueprint(text_node)).unwrap();
    let slots = BTreeMap::from([(7, Value::Text("Slider".into()))]);
    assert!(
        layout
            .resolve_slots(&slots)
            .unwrap_err()
            .contains("admitted font")
    );
    let font = crate::dashboard::font::test_font();
    let scene = layout
        .resolve_ordered_with_font(&slots, [1280., 800.], &[], Some(&font))
        .unwrap();
    near(frame(&scene, 1).width, font.text_width("Slider", 2.));
    assert_eq!(frame(&scene, 1).height, 42.);
    let next = BTreeMap::from([(7, Value::Text("Labelled Slider".into()))]);
    let scene = layout
        .resolve_ordered_with_font(&next, [1280., 800.], &[], Some(&font))
        .unwrap();
    near(
        frame(&scene, 1).width,
        font.text_width("Labelled Slider", 2.),
    );
}
#[test]
fn legacy_pinned_interior_uses_allocated_outer_size_and_clip_hits() {
    let mut interior = node(3, 20., 20., vec![]);
    interior.resize = Some(ResizeRule {
        reference_size: [20., 20.],
        horizontal: AxisResize::Pins {
            leading: Some(0.),
            trailing: Some(0.),
        },
        vertical: AxisResize::Fixed,
    });
    interior.responder = Some(Responder::AppEvent {
        control: 200,
        drag: false,
    });
    let mut reference = node(2, 20., 20., vec![interior]);
    reference.layout = Some(policy(Sizing::Fill, Sizing::Fixed, None));
    let mut group = node(1, 100., 40., vec![reference]);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Fixed,
        Some(AutoLayout {
            direction: Direction::Horizontal,
            ..Default::default()
        }),
    ));
    let scene = resolved(group);
    assert_eq!(frame(&scene, 2).width, 100.);
    assert_eq!(frame(&scene, 3).width, 100.);
    assert!(scene.hit_path([90., 10.]).iter().any(|p| p.node.id == 3));
    assert!(!scene.hit_path([110., 10.]).iter().any(|p| p.node.id == 3));
}
#[test]
fn invalid_bounds_and_legacy_rule_mix_are_rejected_before_resolution() {
    let mut group = node(1, 100., 100., vec![]);
    group.layout = Some(policy(
        Sizing::Hug,
        Sizing::Hug,
        Some(AutoLayout::default()),
    ));
    group.layout.as_mut().unwrap().width.min = 50.;
    group.layout.as_mut().unwrap().width.max = Some(40.);
    assert!(Layout::from_blueprint(blueprint(group.clone())).is_err());
    group.layout.as_mut().unwrap().width.max = None;
    group.resize = Some(ResizeRule {
        reference_size: [100., 100.],
        horizontal: AxisResize::Fixed,
        vertical: AxisResize::Fixed,
    });
    assert!(Layout::from_blueprint(blueprint(group)).is_err());
}

#[test]
fn fill_freezes_only_the_constraining_side_when_min_and_max_both_bind() {
    let lower = AxisSizing {
        mode: Sizing::Fill,
        min: 80.,
        max: Some(90.),
    };
    assert_eq!(
        distribute(
            100.,
            &[
                lower,
                AxisSizing {
                    mode: Sizing::Fill,
                    min: 0.,
                    max: Some(10.)
                }
            ]
        ),
        vec![90., 10.]
    );
    assert_eq!(
        distribute(
            100.,
            &[
                AxisSizing { max: None, ..lower },
                AxisSizing {
                    mode: Sizing::Fill,
                    min: 0.,
                    max: Some(30.)
                }
            ]
        ),
        vec![80., 20.]
    );
}

#[test]
fn empty_hug_container_clamps_padding_once_and_zero_children_keep_gap() {
    let mut group = node(1, 320., 180., vec![]);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Hug,
        Some(AutoLayout {
            padding: [16.; 4],
            ..Default::default()
        }),
    ));
    group.layout.as_mut().unwrap().height.min = 48.;
    let scene = resolved(group);
    assert_eq!(frame(&scene, 1).height, 48.);
    let mut group = node(
        1,
        100.,
        30.,
        vec![node(2, 0., 20., vec![]), node(3, 20., 20., vec![])],
    );
    group.layout = Some(policy(
        Sizing::Hug,
        Sizing::Fixed,
        Some(AutoLayout {
            direction: Direction::Horizontal,
            gap: 8.,
            ..Default::default()
        }),
    ));
    let scene = resolved(group);
    assert!(scene.path(2).is_none());
    assert_eq!(frame(&scene, 3).x, 8.);
    assert_eq!(frame(&scene, 1).width, 28.);
}
#[test]
fn top_level_fill_uses_actual_viewport_without_legacy_composition() {
    let mut blueprint = blueprint(node(1, 20., 20., vec![]));
    blueprint.templates[0].root.layout = Some(policy(
        Sizing::Fill,
        Sizing::Fill,
        Some(AutoLayout::default()),
    ));
    let layout = Layout::from_blueprint(blueprint).unwrap();
    let scene = layout
        .resolve_viewport(&BTreeMap::new(), [1920., 1080.])
        .unwrap();
    assert_eq!(
        [scene.root().frame.width, scene.root().frame.height],
        [1920., 1080.]
    );
}
#[test]
fn reusable_layout_cannot_hide_in_older_layout_version() {
    let mut group = node(1, 100., 100., vec![]);
    group.layout = Some(NodeLayout::default());
    let mut blueprint = blueprint(group);
    blueprint.schema = "otrios-layout-v5".into();
    assert!(
        Layout::from_blueprint(blueprint)
            .err()
            .unwrap()
            .contains("requires layout v6")
    );
}
#[test]
fn legacy_stretch_child_in_hug_has_a_static_dependency_error() {
    let mut child = node(2, 20., 20., vec![]);
    child.resize = Some(ResizeRule {
        reference_size: [100., 100.],
        horizontal: AxisResize::Pins {
            leading: Some(0.),
            trailing: Some(0.),
        },
        vertical: AxisResize::Fixed,
    });
    let mut group = node(1, 100., 100., vec![child]);
    group.layout = Some(policy(
        Sizing::Hug,
        Sizing::Fixed,
        Some(AutoLayout::default()),
    ));
    assert!(
        Layout::from_blueprint(blueprint(group))
            .err()
            .unwrap()
            .contains("dependency")
    );
}

#[test]
fn fractional_fill_keeps_logical_f32_coordinates_without_pixel_rounding() {
    let children = (2..5)
        .map(|id| {
            let mut child = node(id, 1., 20., vec![]);
            child.layout = Some(policy(Sizing::Fill, Sizing::Fixed, None));
            child
        })
        .collect();
    let mut group = node(1, 100., 20., children);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Fixed,
        Some(AutoLayout {
            direction: Direction::Horizontal,
            ..Default::default()
        }),
    ));
    let scene = resolved(group);
    let share = 100.0_f32 / 3.;
    for id in 2..5 {
        assert_eq!(frame(&scene, id).width, share);
        near(frame(&scene, id).x, (id - 2) as f32 * share);
    }
    near(frame(&scene, 4).x + frame(&scene, 4).width, 100.);
}

#[test]
#[ignore = "timing diagnostic; run release-mode explicitly, no hardware-dependent threshold"]
fn auto_layout_resolution_timing_diagnostic() {
    use std::{hint::black_box, time::Instant};
    let children = (2..202).map(|id| node(id, 20., 3., vec![])).collect();
    let mut group = node(1, 320., 600., children);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Hug,
        Some(AutoLayout::default()),
    ));
    let layout = Layout::from_blueprint(blueprint(group)).unwrap();
    let bindings = BTreeMap::new();
    black_box(layout.resolve_slots(&bindings).unwrap());
    let iterations = 500;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(layout.resolve_slots(black_box(&bindings)).unwrap());
    }
    let elapsed = started.elapsed();
    let nodes = 202 * iterations;
    println!(
        "AUTO_LAYOUT_TIMING iterations={iterations} nodes={nodes} elapsed_ms={:.3} layout_nodes_per_second={:.0}; full logical layout and Scene admission on this host, excludes GL and Frame physical presentation",
        elapsed.as_secs_f64() * 1000.,
        nodes as f64 / elapsed.as_secs_f64()
    );
}

#[test]
fn cross_axis_overflow_preserves_alignment_and_visible_clipped_hits() {
    let mut child = node(2, 20., 60., vec![]);
    child.responder = Some(Responder::AppEvent {
        control: 7,
        drag: false,
    });
    let mut group = node(1, 100., 40., vec![child]);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Fixed,
        Some(AutoLayout {
            direction: Direction::Horizontal,
            alignment: CrossAlignment::Center,
            ..Default::default()
        }),
    ));
    let scene = resolved(group);
    assert_eq!(frame(&scene, 2).y, -10.);
    assert_eq!(frame(&scene, 2).height, 60.);
    assert!(scene.hit_path([10., 2.]).iter().any(|n| n.node.id == 2));
    assert!(!scene.hit_path([10., 45.]).iter().any(|n| n.node.id == 2));
}

#[test]
fn reference_boundary_keeps_intrinsic_design_but_allocates_legacy_root_to_caller() {
    let mut interior = node(4, 320., 20., vec![]);
    interior.resize = Some(ResizeRule {
        reference_size: [320., 40.],
        horizontal: AxisResize::Pins {
            leading: Some(0.),
            trailing: Some(0.),
        },
        vertical: AxisResize::Fixed,
    });
    let mut imported = node(3, 320., 40., vec![interior]);
    imported.reference_root = true;
    let mut reference = node(2, 320., 40., vec![imported]);
    reference.layout = Some(policy(Sizing::Fill, Sizing::Fixed, None));
    let mut group = node(1, 200., 80., vec![reference]);
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Fixed,
        Some(AutoLayout::default()),
    ));
    let scene = resolved(group.clone());
    assert_eq!(frame(&scene, 2).width, 200.);
    assert_eq!(frame(&scene, 3).width, 200.);
    assert_eq!(frame(&scene, 4).width, 200.);
    group.children[0].layout.as_mut().unwrap().width.mode = Sizing::Hug;
    let scene = resolved(group);
    assert_eq!(frame(&scene, 2).width, 320.);
    assert_eq!(frame(&scene, 3).width, 320.);
}

#[test]
fn space_between_adds_spare_space_to_minimum_gap_and_single_child_starts() {
    let mut group = node(
        1,
        100.,
        40.,
        vec![node(2, 20., 20., vec![]), node(3, 20., 20., vec![])],
    );
    group.layout = Some(policy(
        Sizing::Fixed,
        Sizing::Fixed,
        Some(AutoLayout {
            direction: Direction::Horizontal,
            gap: 10.,
            packing: Packing::SpaceBetween,
            ..Default::default()
        }),
    ));
    let scene = resolved(group.clone());
    assert_eq!(frame(&scene, 2).x, 0.);
    assert_eq!(frame(&scene, 3).x, 80.);
    group.children.pop();
    let scene = resolved(group);
    assert_eq!(frame(&scene, 2).x, 0.);
}

#[test]
fn free_fill_child_does_not_create_hug_dependency_without_a_flow_container() {
    let mut free = node(2, 600., 400., vec![]);
    free.layout = Some(NodeLayout {
        placement: Placement::Free,
        width: AxisSizing {
            mode: Sizing::Fill,
            ..AxisSizing::default()
        },
        height: AxisSizing {
            mode: Sizing::Fill,
            ..AxisSizing::default()
        },
        ..NodeLayout::default()
    });
    let mut group = node(1, 800., 600., vec![free]);
    group.layout = Some(NodeLayout {
        width: AxisSizing {
            mode: Sizing::Hug,
            min: 48.,
            max: None,
        },
        height: AxisSizing {
            mode: Sizing::Hug,
            min: 64.,
            max: None,
        },
        ..NodeLayout::default()
    });
    let scene = resolved(group);
    assert_eq!(
        [frame(&scene, 1).width, frame(&scene, 1).height],
        [48., 64.]
    );
    assert_eq!(
        [frame(&scene, 2).width, frame(&scene, 2).height],
        [48., 64.]
    );
}
