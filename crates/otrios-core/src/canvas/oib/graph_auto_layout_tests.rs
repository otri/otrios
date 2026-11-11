// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::layout::{AutoLayout, AxisSizing, NodeLayout, Sizing};
#[allow(dead_code)]
#[path = "../../../tests/support/oib_library_fixture.rs"]
mod fixture;
fn visual(width: f32, height: f32, layout: NodeLayout) -> source::Visual {
    source::Visual {
        layout: Some(layout),
        frame: [literal(0.), literal(0.), literal(width), literal(height)],
        resize: None,
        visible: Expr::Literal(Value::Boolean(true)),
        shape: Shape::Canvas { clip: true },
        semantics: None,
        action: None,
        parameters: vec![],
    }
}
fn wrapper(width: f32) -> ComponentDefinition {
    ComponentDefinition {
        id: DefinitionId(900),
        name: "LabelledSlider".into(),
        root: NodeId(1),
        nodes: vec![
            DefinitionNode {
                id: NodeId(1),
                name: "LabelledSlider".into(),
                class: ClassId(4),
                owner: None,
                visual_children: vec![NodeId(2)],
                defaults: vec![],
                reference: None,
                visual: Some(visual(
                    width,
                    200.,
                    NodeLayout {
                        height: AxisSizing {
                            mode: Sizing::Hug,
                            ..AxisSizing::default()
                        },
                        container: Some(AutoLayout {
                            padding: [16.; 4],
                            gap: 8.,
                            ..AutoLayout::default()
                        }),
                        ..NodeLayout::default()
                    },
                )),
            },
            DefinitionNode {
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
                visual: Some(visual(
                    440.,
                    112.,
                    NodeLayout {
                        width: AxisSizing {
                            mode: Sizing::Fill,
                            ..AxisSizing::default()
                        },
                        ..NodeLayout::default()
                    },
                )),
            },
        ],
        properties: vec![],
        next_property: 0,
        exposed: vec![],
        slots: vec![],
    }
}
#[test]
fn auto_layout_local_wrapper_allocates_real_imported_slider_and_pinned_track() {
    for width in [320., 520.] {
        let mut ui = fixture::imported();
        let seed = fixture::seed_mut(&mut ui);
        let graph = seed.graph.as_mut().unwrap();
        graph.definitions.push(wrapper(width));
        graph.presets.push(source::Preset {
            id: 901,
            name: "LabelledSlider".into(),
            definition: DefinitionId(900),
            assignments: vec![],
            frame: None,
            palette: true,
        });
        let compiled = materialize(&ui).unwrap();
        let seed = super::super::seed(&compiled).unwrap();
        let meta = seed.designer.as_ref().unwrap();
        let lookup = |references: Vec<NodeId>, node| {
            meta.mappings
                .objects
                .iter()
                .find(|m| {
                    m.address.instance == InstanceId(901)
                        && m.address.references == references
                        && m.address.node == NodeId(node)
                })
                .unwrap()
                .layout_node
                .unwrap()
        };
        let outer = lookup(vec![], 2);
        let inner = lookup(vec![NodeId(2)], 1);
        let track = lookup(vec![NodeId(2)], 4);
        // Isolate the compiled reference component from unrelated app bindings.
        // The real immutable library closure and graph compiler stay in the path.
        let mut blueprint = Layout::from_bytes(&compiled.layout)
            .unwrap()
            .blueprint()
            .clone();
        let wrapper_id = lookup(vec![], 1);
        fn find(node: &TemplateNode, id: u16) -> Option<&TemplateNode> {
            if node.id == id {
                Some(node)
            } else {
                node.children.iter().find_map(|n| find(n, id))
            }
        }
        let compiled_wrapper = blueprint
            .templates
            .iter()
            .find_map(|t| find(&t.root, wrapper_id))
            .unwrap()
            .clone();
        let mut root = blueprint.templates[0].root.clone();
        root.children = vec![compiled_wrapper];
        root.layout = None;
        blueprint.templates[0].root = root;
        blueprint.templates.truncate(1);
        blueprint.composition = None;
        let layout = Layout::from_blueprint(blueprint).unwrap();
        let slots = BTreeMap::new();
        let scene = layout
            .resolve_ordered_with_font(
                &slots,
                [1280., 800.],
                &[],
                Some(crate::dashboard::font::admission_font().unwrap()),
            )
            .unwrap();
        let frame = |id| {
            scene
                .path(u32::from(id))
                .unwrap()
                .last()
                .unwrap()
                .node
                .frame
        };
        assert_eq!(frame(outer).width, width - 32.);
        assert_eq!(frame(inner).width, width - 32.);
        assert_eq!(frame(inner).height, 112.);
        assert_eq!(frame(track).width, width - 32.);
        assert_eq!(frame(inner).x, 0.);
    }
}
