// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::layout::{AutoLayout, AxisSizing, NodeLayout, Sizing};
#[allow(dead_code)]
#[path = "../../../../tests/support/oib_library_fixture.rs"]
mod fixture;
fn blank() -> ComponentDefinition {
    ComponentDefinition {
        id: DefinitionId(900),
        name: "Blank component".into(),
        root: NodeId(1),
        nodes: vec![DefinitionNode {
            id: NodeId(1),
            name: "Root".into(),
            class: ClassId(4),
            owner: None,
            visual_children: vec![],
            defaults: vec![],
            reference: None,
            visual: Some(source::Visual {
                layout: Some(NodeLayout {
                    height: AxisSizing {
                        mode: Sizing::Hug,
                        min: 48.,
                        max: None,
                    },
                    container: Some(AutoLayout {
                        padding: [16.; 4],
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                resize: None,
                frame: [literal(0.), literal(0.), literal(320.), literal(180.)],
                visible: Expr::Literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                semantics: None,
                action: None,
                parameters: vec![],
            }),
        }],
        exposed: vec![],
        slots: vec![],
    }
}
fn defaults(preview: &DefinitionPreview) -> BTreeMap<u16, Value> {
    preview
        .ui
        .selectors
        .iter()
        .map(|selector| {
            let value = preview
                .ui
                .state_schema
                .fields
                .iter()
                .find(|field| field.id == selector.field)
                .unwrap()
                .default
                .clone();
            (
                selector.local,
                project(value, &selector.projection).unwrap().0,
            )
        })
        .collect()
}
#[test]
fn unused_blank_component_gets_48_high_virtual_preview_without_insert() {
    let mut ui = fixture::imported();
    fixture::seed_mut(&mut ui)
        .graph
        .as_mut()
        .unwrap()
        .definitions
        .push(blank());
    let original = flexbuffers::to_vec(&ui).unwrap();
    let preview = definition_preview(&ui, DefinitionId(900), [640., 480.]).unwrap();
    assert_eq!(flexbuffers::to_vec(&ui).unwrap(), original);
    let scene = preview
        .layout
        .resolve_ordered_with_font(
            &defaults(&preview),
            [640., 480.],
            &[],
            Some(crate::dashboard::font::admission_font().unwrap()),
        )
        .unwrap();
    let root = preview
        .objects
        .iter()
        .find(|mapping| mapping.address.node == NodeId(1))
        .unwrap()
        .layout_node
        .unwrap();
    let frame = scene
        .path(u32::from(root))
        .unwrap()
        .last()
        .unwrap()
        .node
        .frame;
    assert_eq!([frame.width, frame.height], [320., 48.]);
    let private = crate::canvas::oib::seed(&preview.ui)
        .unwrap()
        .graph
        .as_ref()
        .unwrap();
    assert_eq!(private.instances.len(), 2);
    assert_eq!(private.presets.len(), 2);
    assert_eq!(
        preview.selection_for_node(u32::from(root)),
        Some(Selection::DefinitionNode {
            definition: DefinitionId(900),
            node: NodeId(1)
        })
    );
    assert!(ui.presentation.as_ref().is_some());
}
#[test]
fn selected_library_definition_has_only_its_virtual_surface_and_real_sample_slider() {
    let ui = fixture::imported();
    let original = flexbuffers::to_vec(&ui).unwrap();
    let preview = definition_preview(&ui, DefinitionId(12), [640., 480.]).unwrap();
    assert_eq!(flexbuffers::to_vec(&ui).unwrap(), original);
    assert!(preview.ui.behavior.is_none());
    assert!(preview.ui.navigation.is_empty());
    assert!(!preview.ui.handlers.is_empty());
    assert!(
        preview
            .ui
            .handlers
            .iter()
            .any(|handler| handler.operation == Operation::Set)
    );
    assert!(
        preview
            .objects
            .iter()
            .all(|mapping| mapping.address.instance == preview.instance)
    );
    assert!(
        preview
            .objects
            .iter()
            .all(|mapping| mapping.address.references.is_empty())
    );
    let mut bindings = defaults(&preview);
    let baseline = preview
        .layout
        .resolve_ordered_with_font(
            &bindings,
            [640., 480.],
            &[],
            Some(crate::dashboard::font::admission_font().unwrap()),
        )
        .unwrap();
    let control = baseline
        .controls()
        .into_iter()
        .find(|control| {
            matches!(
                control.node.responder,
                Some(otrios_interfaces::scene::TouchResponder::Drag(_))
            )
        })
        .unwrap()
        .node
        .id;
    let field = preview
        .ui
        .handlers
        .iter()
        .find(|handler| handler.operation == Operation::Set)
        .unwrap()
        .fields[0];
    for selector in preview
        .ui
        .selectors
        .iter()
        .filter(|selector| selector.field == field)
    {
        bindings.insert(
            selector.local,
            project(Value::Number(750.), &selector.projection)
                .unwrap()
                .0,
        );
    }
    let next = preview
        .layout
        .resolve_ordered_with_font(
            &bindings,
            [640., 480.],
            &[],
            Some(crate::dashboard::font::admission_font().unwrap()),
        )
        .unwrap();
    assert!(next.path(control).is_some());
    assert_ne!(baseline.root(), next.root());
}
#[test]
fn invalid_preview_bounds_and_nonvisual_document_are_explicit_errors() {
    let ui = fixture::imported();
    assert!(definition_preview(&ui, DefinitionId(12), [f32::NAN, 480.]).is_err());
    let graph = fixture::seed(&ui).graph.as_ref().unwrap();
    let shell = graph
        .instances
        .iter()
        .find(|instance| instance.id == graph.presented_root)
        .unwrap();
    let definition = graph
        .presets
        .iter()
        .find(|preset| preset.id == shell.preset)
        .unwrap()
        .definition;
    assert!(
        definition_preview(&ui, definition, [640., 480.])
            .err()
            .unwrap()
            .contains("nonvisual")
    );
}

fn guest_ui() -> Ui {
    use otrios_interfaces::oib::{
        component_behavior as abi,
        graph::LockedExport,
        library::{BehaviorDeclaration, BehaviorOutlet, BehaviorProperty, ComponentCode},
    };
    use sha2::{Digest, Sha256};
    let mut ui = fixture::imported();
    let output = abi::encode_output(&abi::Output {
        commands: vec![abi::Command::Emit {
            outlet: 1,
            value: Value::Number(750.),
        }],
    })
    .unwrap();
    let bytes: String = output.iter().map(|byte| format!("\\{byte:02x}")).collect();
    let code = wat::parse_str(format!(
        r#"(module
      (memory (export "memory") 1 1)
      (data (i32.const 4096) "{bytes}")
      (func (export "abi_version") (result i32) i32.const 1)
      (func (export "input_buffer") (result i32) i32.const 0)
      (func (export "dispatch") (param i32 i32) (result i64) i64.const {}))"#,
        ((output.len() as u64) << 32) | 4096
    ))
    .unwrap();
    let seed = fixture::seed_mut(&mut ui);
    let library = &mut seed.resolved_libraries[0];
    let definition = library
        .graph
        .definitions
        .iter()
        .find(|definition| definition.id == DefinitionId(12))
        .unwrap();
    let node = definition
        .nodes
        .iter()
        .find(|node| {
            node.visual
                .as_ref()
                .is_some_and(|visual| visual.action.is_some())
        })
        .unwrap();
    let source = node
        .visual
        .as_ref()
        .unwrap()
        .action
        .as_ref()
        .unwrap()
        .source
        .clone();
    library.graph.format = otrios_interfaces::oib::library::BEHAVIOR_FORMAT.into();
    library.graph.behaviors = vec![BehaviorDeclaration {
        id: 1,
        definition: DefinitionId(12),
        node: node.id,
        properties: vec![BehaviorProperty {
            id: 1,
            source: PropertyPath {
                references: vec![],
                node: node.id,
                property: PropertyId(1),
            },
        }],
        local: vec![],
        outlets: vec![BehaviorOutlet {
            id: 1,
            source,
            value_type: otrios_interfaces::layout::ValueType::Number,
        }],
        can_capture: true,
    }];
    library.component = Some(ComponentCode {
        abi: abi::ABI,
        sha256: Sha256::digest(&code).into(),
        bytes: code,
    });
    library.graph = crate::library::seal_graph(library.graph.clone()).unwrap();
    seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks[0].exports = library
        .graph
        .exports
        .iter()
        .map(|export| LockedExport {
            export: export.id,
            contract: export.contract,
        })
        .collect();
    ui
}
#[test]
fn independently_loaded_slider_guest_emits_only_into_private_sample_adapter() {
    use otrios_interfaces::oib::component_behavior::{Event, Phase};
    let ui = guest_ui();
    let original = flexbuffers::to_vec(&ui).unwrap();
    let preview = definition_preview(&ui, DefinitionId(12), [640., 480.]).unwrap();
    let runtime = std::sync::Arc::new(crate::canvas::component_behavior::Runtime::default());
    let mut instances = crate::canvas::oib::component_behaviors::Instances::prepare(
        &preview.ui,
        &preview.layout,
        &runtime,
        None,
    )
    .unwrap();
    assert_eq!(runtime.live_instances(), 1);
    let handler = preview
        .ui
        .handlers
        .iter()
        .find(|handler| handler.operation == Operation::Set)
        .unwrap();
    let mut sample = preview.sample_adapter().unwrap();
    let outcome = instances
        .dispatch(
            &preview.ui,
            &mut sample,
            handler.control,
            Event {
                phase: Phase::Move,
                pointer: 7,
                a: 750,
                b: 0,
            },
            true,
        )
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(
        sample.typed_value(handler.fields[0]).unwrap(),
        Value::Number(750.)
    );
    assert!(sample.generate().unwrap().is_none());
    assert_eq!(flexbuffers::to_vec(&ui).unwrap(), original);
}

#[test]
fn local_reference_selects_boundary_and_real_imported_interior_uses_fill_width() {
    let mut ui = fixture::imported();
    let mut wrapper = blank();
    wrapper.name = "LabelledSlider".into();
    wrapper.nodes[0].visual_children.push(NodeId(2));
    wrapper.nodes.push(DefinitionNode {
        id: NodeId(2),
        name: "Slider".into(),
        class: ClassId(3),
        owner: Some(NodeId(1)),
        visual_children: vec![],
        defaults: vec![],
        reference: Some(ComponentReference {
            definition: DefinitionId(12),
            assignments: vec![],
        }),
        visual: Some(source::Visual {
            layout: Some(NodeLayout {
                width: AxisSizing {
                    mode: Sizing::Fill,
                    ..Default::default()
                },
                ..Default::default()
            }),
            resize: None,
            frame: [literal(0.), literal(0.), literal(440.), literal(112.)],
            visible: Expr::Literal(Value::Boolean(true)),
            shape: Shape::Canvas { clip: true },
            semantics: None,
            action: None,
            parameters: vec![],
        }),
    });
    fixture::seed_mut(&mut ui)
        .graph
        .as_mut()
        .unwrap()
        .definitions
        .push(wrapper);
    let preview = definition_preview(&ui, DefinitionId(900), [640., 480.]).unwrap();
    let scene = preview
        .layout
        .resolve_ordered_with_font(
            &defaults(&preview),
            [640., 480.],
            &[],
            Some(crate::dashboard::font::admission_font().unwrap()),
        )
        .unwrap();
    let outer = preview
        .objects
        .iter()
        .find(|mapping| mapping.address.references.is_empty() && mapping.address.node == NodeId(2))
        .unwrap()
        .layout_node
        .unwrap();
    let inner = preview
        .objects
        .iter()
        .find(|mapping| {
            mapping.address.references == [NodeId(2)] && mapping.address.node == NodeId(1)
        })
        .unwrap()
        .layout_node
        .unwrap();
    assert_eq!(
        scene
            .path(u32::from(outer))
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        288.
    );
    assert_eq!(
        scene
            .path(u32::from(inner))
            .unwrap()
            .last()
            .unwrap()
            .node
            .frame
            .width,
        288.
    );
    assert_eq!(
        preview.selection_for_node(u32::from(inner)),
        Some(Selection::DefinitionNode {
            definition: DefinitionId(900),
            node: NodeId(2)
        })
    );
}
#[test]
fn toggle_sample_keeps_boolean_outlet_type_on_normalized_private_field() {
    let ui = fixture::imported();
    let preview = definition_preview(&ui, DefinitionId(13), [640., 480.]).unwrap();
    let handler = preview
        .ui
        .handlers
        .iter()
        .find(|handler| handler.operation == Operation::Toggle)
        .unwrap();
    let field = preview
        .ui
        .state_schema
        .fields
        .iter()
        .find(|field| field.id == handler.fields[0])
        .unwrap();
    assert!(matches!(field.default, Value::Number(0. | 1000.)));
    assert!(
        preview.ui.selectors.iter().any(
            |selector| selector.field == field.id && selector.projection == Projection::Nonzero
        )
    );
    assert!(
        preview
            .sample_adapter()
            .unwrap()
            .generate()
            .unwrap()
            .is_none()
    );
}

fn slider_color(ui: &Ui) -> PropertyId {
    let materialized = materialize(ui).unwrap();
    let seed = crate::canvas::oib::seed(&materialized).unwrap();
    let metadata = seed.designer.as_ref().unwrap();
    let definition = metadata
        .definitions
        .iter()
        .find(|definition| definition.id == DefinitionId(12))
        .unwrap();
    let node = definition
        .nodes
        .iter()
        .find(|node| node.id == definition.root)
        .unwrap();
    document::descriptors(metadata, node.class)
        .unwrap()
        .into_iter()
        .find(|descriptor| descriptor.value_type == otrios_interfaces::layout::ValueType::Color)
        .unwrap()
        .id
}
fn reference(id: u64, color: Option<(PropertyId, AssignmentSource)>) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: format!("Slider {id}"),
        class: ClassId(3),
        owner: Some(NodeId(1)),
        visual_children: vec![],
        defaults: vec![],
        reference: Some(ComponentReference {
            definition: DefinitionId(12),
            assignments: color
                .into_iter()
                .map(|(property, source)| Assignment {
                    target: PropertyPath {
                        references: vec![],
                        node: NodeId(1),
                        property,
                    },
                    source,
                })
                .collect(),
        }),
        visual: Some(source::Visual {
            layout: Some(NodeLayout {
                width: AxisSizing {
                    mode: Sizing::Fill,
                    ..Default::default()
                },
                ..Default::default()
            }),
            resize: None,
            frame: [literal(0.), literal(0.), literal(440.), literal(112.)],
            visible: Expr::Literal(Value::Boolean(true)),
            shape: Shape::Canvas { clip: true },
            semantics: None,
            action: None,
            parameters: vec![],
        }),
    }
}
fn preview_value(
    preview: &DefinitionPreview,
    references: Vec<NodeId>,
    property: PropertyId,
) -> Value {
    let seed = crate::canvas::oib::seed(&preview.ui).unwrap();
    document::graph_property(
        seed,
        preview.instance,
        &PropertyPath {
            references,
            node: NodeId(1),
            property,
        },
        &BTreeMap::new(),
    )
    .unwrap()
    .1
    .unwrap()
}
#[test]
fn semantic_definition_values_preserve_reference_literal_precedence_and_library_bytes() {
    let mut ui = fixture::imported();
    let color = slider_color(&ui);
    let mut wrapper = blank();
    wrapper.nodes[0].visual_children = vec![NodeId(2), NodeId(3)];
    wrapper.nodes.push(reference(2, None));
    wrapper.nodes.push(reference(
        3,
        Some((
            color,
            AssignmentSource::Literal(Value::Color([9, 8, 7, 255])),
        )),
    ));
    fixture::seed_mut(&mut ui)
        .graph
        .as_mut()
        .unwrap()
        .definitions
        .push(wrapper);
    let original = flexbuffers::to_vec(&ui).unwrap();
    let values = BTreeMap::from([(
        PropertyAddress::Definition {
            definition: DefinitionId(12),
            node: NodeId(1),
            property: color,
        },
        Some(Value::Color([1, 2, 3, 255])),
    )]);
    let preview =
        definition_preview_with_values(&ui, DefinitionId(900), [640., 480.], &values).unwrap();
    assert_eq!(
        preview_value(&preview, vec![NodeId(2)], color),
        Value::Color([1, 2, 3, 255])
    );
    assert_eq!(
        preview_value(&preview, vec![NodeId(3)], color),
        Value::Color([9, 8, 7, 255])
    );
    assert_eq!(flexbuffers::to_vec(&ui).unwrap(), original);
    let source = crate::canvas::oib::seed(&ui).unwrap();
    let private = crate::canvas::oib::seed(&preview.ui).unwrap();
    assert_eq!(private.resolved_libraries, source.resolved_libraries);
}
#[test]
fn semantic_definition_override_keeps_authored_forward_source() {
    let mut ui = fixture::imported();
    let color = slider_color(&ui);
    let materialized = materialize(&ui).unwrap();
    let metadata = crate::canvas::oib::seed(&materialized)
        .unwrap()
        .designer
        .as_ref()
        .unwrap();
    let outer_color = document::descriptors(metadata, ClassId(4))
        .unwrap()
        .into_iter()
        .find(|descriptor| descriptor.value_type == otrios_interfaces::layout::ValueType::Color)
        .unwrap()
        .id;
    let mut wrapper = blank();
    wrapper.nodes[0].defaults.push(PropertyDefault {
        property: outer_color,
        value: Value::Color([4, 5, 6, 255]),
    });
    wrapper.nodes[0].visual_children.push(NodeId(2));
    wrapper.nodes.push(reference(
        2,
        Some((
            color,
            AssignmentSource::Forward(PropertyPath {
                references: vec![],
                node: NodeId(1),
                property: outer_color,
            }),
        )),
    ));
    fixture::seed_mut(&mut ui)
        .graph
        .as_mut()
        .unwrap()
        .definitions
        .push(wrapper);
    let values = BTreeMap::from([(
        PropertyAddress::Definition {
            definition: DefinitionId(12),
            node: NodeId(1),
            property: color,
        },
        Some(Value::Color([1, 2, 3, 255])),
    )]);
    let preview =
        definition_preview_with_values(&ui, DefinitionId(900), [640., 480.], &values).unwrap();
    assert_eq!(
        preview_value(&preview, vec![NodeId(2)], color),
        Value::Color([4, 5, 6, 255])
    );
}

fn row_scene(
    clip: bool,
    scale: Option<f32>,
    paint_size: [f32; 2],
) -> crate::dashboard::scene::Scene {
    use otrios_interfaces::scene::{Action, Primitive, Rect, SceneNode, TouchResponder};
    let paint = SceneNode {
        id: 4,
        frame: Rect::new(
            if scale.is_some() { 0. } else { -20. },
            0.,
            paint_size[0],
            paint_size[1],
        ),
        primitive: Primitive::Superellipse {
            radius: 0.,
            exponent: 4.,
            color: [255; 4],
        },
        children: vec![],
        responder: Some(TouchResponder::Drag(Action::AppEvent(7))),
        semantics: None,
    };
    let container = SceneNode {
        id: 3,
        frame: Rect::new(0., 0., if scale.is_some() { 80. } else { 20. }, 40.),
        primitive: scale.map_or(Primitive::Canvas { clip }, |scale| Primitive::Viewport {
            scale,
        }),
        children: vec![paint],
        responder: None,
        semantics: None,
    };
    let row = SceneNode {
        id: 2,
        frame: Rect::new(0., 0., 60., 40.),
        primitive: Primitive::Canvas { clip: false },
        children: vec![container],
        responder: None,
        semantics: None,
    };
    let viewport = SceneNode {
        id: 1,
        frame: Rect::new(0., 0., 200., 200.),
        primitive: Primitive::Canvas { clip: true },
        children: vec![row],
        responder: None,
        semantics: None,
    };
    crate::dashboard::scene::Scene::new(SceneNode {
        id: 0,
        frame: Rect::new(0., 0., 1280., 800.),
        primitive: Primitive::Canvas { clip: true },
        children: vec![viewport],
        responder: None,
        semantics: None,
    })
    .unwrap()
}
#[test]
fn row_admission_uses_ancestor_clips_without_losing_visible_input() {
    let clipped = row_scene(true, None, [100., 40.]);
    crate::canvas::oib::instances::validate_row_geometry(&clipped, 1).unwrap();
    assert!(
        clipped
            .hit_path([10., 10.])
            .iter()
            .any(|node| node.node.id == 4)
    );
    assert!(
        !clipped
            .hit_path([25., 10.])
            .iter()
            .any(|node| node.node.id == 4)
    );
    let unbounded = row_scene(false, None, [100., 40.]);
    assert!(
        crate::canvas::oib::instances::validate_row_geometry(&unbounded, 1)
            .err()
            .unwrap()
            .contains("escapes")
    );
    let small = row_scene(true, None, [20., 20.]);
    assert!(
        crate::canvas::oib::instances::validate_row_geometry(&small, 1)
            .err()
            .unwrap()
            .contains("touch target")
    );
}
#[test]
fn row_admission_uses_real_viewport_scale_before_visible_bounds() {
    let scene = row_scene(true, Some(0.5), [100., 40.]);
    crate::canvas::oib::instances::validate_row_geometry(&scene, 1).unwrap();
}

#[test]
fn preview_reuses_shell_at_definition_budget_without_appending_source_records() {
    let mut ui = fixture::imported();
    let count = crate::canvas::oib::imports::resolve(fixture::seed(&ui))
        .unwrap()
        .definitions
        .len();
    for index in count..MAX_DEFINITIONS {
        let mut definition = blank();
        definition.id = DefinitionId(900 + index as u64);
        definition.name = format!("Blank {index}");
        fixture::seed_mut(&mut ui)
            .graph
            .as_mut()
            .unwrap()
            .definitions
            .push(definition);
    }
    assert_eq!(
        crate::canvas::oib::imports::resolve(fixture::seed(&ui))
            .unwrap()
            .definitions
            .len(),
        MAX_DEFINITIONS
    );
    let selected = DefinitionId(900 + count as u64);
    let original = flexbuffers::to_vec(&ui).unwrap();
    let preview = definition_preview(&ui, selected, [640., 480.]).unwrap();
    assert_eq!(flexbuffers::to_vec(&ui).unwrap(), original);
    let source = fixture::seed(&ui).graph.as_ref().unwrap();
    let private = crate::canvas::oib::seed(&preview.ui)
        .unwrap()
        .graph
        .as_ref()
        .unwrap();
    assert_eq!(private.presented_root, source.presented_root);
    assert_eq!(
        (
            private.preview_viewport,
            private.inspector_viewport,
            private.hierarchy_viewport
        ),
        (
            source.preview_viewport,
            source.inspector_viewport,
            source.hierarchy_viewport
        )
    );
    assert_eq!(private.presets.len(), 2);
    assert_eq!(private.instances.len(), 2);
}

#[test]
fn app_business_field_defaults_are_not_component_sample_defaults() {
    let mut ui = fixture::imported();
    for field in &mut ui.state_schema.fields {
        if matches!(field.default, Value::Number(_)) {
            field.default = Value::Number(999.);
        }
    }
    let preview = definition_preview(&ui, DefinitionId(12), [640., 480.]).unwrap();
    let handler = preview
        .ui
        .handlers
        .iter()
        .find(|handler| handler.operation == Operation::Set)
        .unwrap();
    let sample = preview
        .ui
        .state_schema
        .fields
        .iter()
        .find(|field| field.id == handler.fields[0])
        .unwrap();
    assert_ne!(sample.default, Value::Number(999.));
    assert_ne!(preview.ui.state_schema.id, ui.state_schema.id);
}

#[test]
fn designer_responsive_policy_survives_graph_lowering_and_definition_preview() {
    use otrios_interfaces::layout::{RESPONSIVE_VIEWPORT_FORMAT, ViewportPolicy};
    let ui = crate::test_support::oib_designer::ui(&[]);
    let expected = ViewportPolicy::Responsive {
        minimum: [800., 480.],
    };
    let initial = Layout::from_bytes(&ui.layout).unwrap();
    assert_eq!(initial.viewport_policy(), expected);
    assert_eq!(initial.blueprint().schema, RESPONSIVE_VIEWPORT_FORMAT);
    let materialized = materialize(&ui).unwrap();
    let layout = Layout::from_bytes(&materialized.layout).unwrap();
    assert_eq!(layout.viewport_policy(), expected);
    assert_eq!(layout.blueprint().schema, RESPONSIVE_VIEWPORT_FORMAT);
    for size in [[800., 480.], [1280., 800.], [1920., 1080.], [1600., 900.]] {
        let mut bindings = BTreeMap::new();
        for selector in &materialized.selectors {
            let value = materialized
                .state_schema
                .fields
                .iter()
                .find(|field| field.id == selector.field)
                .unwrap()
                .default
                .clone();
            bindings.insert(
                selector.local,
                project(value, &selector.projection).unwrap().0,
            );
        }
        let scene = layout
            .resolve_ordered_with_font(
                &bindings,
                size,
                &[],
                Some(crate::dashboard::font::admission_font().unwrap()),
            )
            .unwrap();
        assert_eq!([scene.root().frame.width, scene.root().frame.height], size);
    }
    let preview = definition_preview(&ui, DefinitionId(12), [320., 240.]).unwrap();
    assert_eq!(preview.layout.viewport_policy(), expected);
    assert_eq!(
        preview.layout.blueprint().schema,
        RESPONSIVE_VIEWPORT_FORMAT
    );
    let scene = preview
        .layout
        .resolve_ordered_with_font(
            &defaults(&preview),
            [1920., 1080.],
            &[],
            Some(crate::dashboard::font::admission_font().unwrap()),
        )
        .unwrap();
    assert_eq!(
        [scene.root().frame.width, scene.root().frame.height],
        [1920., 1080.]
    );
}
