// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::{self, Document, oib};
use otrios_interfaces::{
    canvas::Presentation,
    layout::{Binding, Expr, Value},
    oib::designer::*,
};

fn admitted() -> otrios_interfaces::canvas::Ui {
    assert_eq!(app::manifest().id, "oib-designer");
    let source = app::ui(&[]);
    let encoded = canvas::encode_ui(&source).unwrap();
    canvas::decode_ui(&encoded).unwrap()
}
fn from_graph(
    edit: impl FnOnce(&mut otrios_interfaces::oib::graph::Document),
) -> otrios_interfaces::canvas::Ui {
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    edit(seed.graph.as_mut().unwrap());
    canvas::decode_ui(&canvas::encode_ui(&ui).unwrap()).unwrap()
}
fn address(instance: u64) -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(instance),
        path: PropertyPath {
            references: vec![NodeId(7)],
            node: NodeId(1),
            property: PropertyId(3),
        },
    }
}
#[test]
fn canonical_graph_generates_one_layout_and_real_initial_instances() {
    let ui = admitted();
    let seed = oib::seed(&ui).unwrap();
    let graph = seed.graph.as_ref().unwrap();
    assert_eq!(
        graph.instances.iter().map(|i| i.id.0).collect::<Vec<_>>(),
        vec![20, 30, 40]
    );
    assert!(graph.presets.iter().any(|p| p.id == 50 && p.palette));
    let encoded = canvas::encode_ui(&ui).unwrap();
    let raw: serde_json::Value = flexbuffers::from_slice(&encoded).unwrap();
    let root = &raw["presentation"]["Oib"];
    assert_eq!(
        root.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["format", "graph"]
    );
    assert!(!encoded.is_empty());
    oib::validate(&ui).unwrap();
    let mut state = Document::new(ui.state_schema.clone(), "graph:test").unwrap();
    state.configure_oib(&ui).unwrap();
    assert_eq!(
        state
            .oib_allocations()
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![20, 30]
    );
    let (current, layout) = state.oib_compiled(2).unwrap();
    assert_eq!(oib::seed(&current).unwrap().instances.len(), 2);
    assert!(layout.blueprint().templates.len() == 1);
    assert_eq!(
        canvas::encode_ui(&canvas::decode_ui(&encoded).unwrap()).unwrap(),
        encoded
    );
}
#[test]
fn independent_initial_ids_names_and_order_do_not_select_another_presets_binding() {
    let ui = from_graph(|graph| {
        let a = graph
            .instances
            .iter_mut()
            .find(|i| i.id == InstanceId(20))
            .unwrap();
        a.preset = 30;
        a.name = "Twenty".into();
        a.order = 9;
        let b = graph
            .instances
            .iter_mut()
            .find(|i| i.id == InstanceId(30))
            .unwrap();
        b.preset = 20;
        b.name = "Thirty".into();
        b.order = 3;
    });
    let mut state = Document::new(ui.state_schema.clone(), "graph:permuted").unwrap();
    state.configure_oib(&ui).unwrap();
    let allocations = state.oib_allocations().unwrap();
    assert_eq!(
        allocations
            .iter()
            .map(|a| (a.id, a.prototype, a.name.as_str()))
            .collect::<Vec<_>>(),
        vec![(30, 20, "Thirty"), (20, 30, "Twenty")]
    );
    let (compiled, _) = state.oib_compiled(2).unwrap();
    let projected = oib::seed(&compiled).unwrap();
    let values = state.oib_values(2).unwrap();
    for (id, label) in [(20, "Brightness"), (30, "Volume")] {
        let actual =
            oib::document::cell_for_address(projected, &allocations, &address(id)).unwrap();
        let signed =
            oib::document::cell_for_address(oib::seed(&ui).unwrap(), &allocations, &address(id))
                .unwrap();
        assert_eq!(actual, signed);
        assert_eq!(
            oib::effective(projected, &values, actual).unwrap(),
            Value::Text(label.into())
        );
        let object = projected
            .designer
            .as_ref()
            .unwrap()
            .instances
            .iter()
            .find(|i| i.id == InstanceId(id))
            .unwrap();
        assert_eq!(object.owner, Owner::Document);
        assert_eq!(
            object.visual_parent.as_ref().unwrap().instance,
            InstanceId(40)
        );
    }
}
#[test]
fn shared_visual_geometry_is_inherited_until_a_preset_explicitly_overrides_it() {
    let ui = admitted();
    let source = oib::seed(&ui).unwrap().graph.as_ref().unwrap();
    let mut definitions = source.definitions.clone();
    definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter_mut()
        .find(|n| n.id == NodeId(1))
        .unwrap()
        .visual
        .as_mut()
        .unwrap()
        .frame[2] = Expr::Literal(Value::Number(500.));
    let changed = oib::graph::with_source(&ui, &definitions, &source.presets).unwrap();
    oib::validate(&changed).unwrap();
    let mut state = Document::new(ui.state_schema.clone(), "graph:geometry").unwrap();
    state.configure_oib(&changed).unwrap();
    let (compiled, layout) = state.oib_compiled(2).unwrap();
    let metadata = oib::seed(&compiled).unwrap().designer.as_ref().unwrap();
    fn find(
        n: &otrios_interfaces::layout::TemplateNode,
        id: u16,
    ) -> Option<&otrios_interfaces::layout::TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|c| find(c, id))
        }
    }
    for instance in [20, 30] {
        let id = metadata
            .mappings
            .objects
            .iter()
            .find(|m| {
                m.address
                    == ObjectAddress {
                        instance: InstanceId(instance),
                        references: vec![],
                        node: NodeId(1),
                    }
            })
            .unwrap()
            .layout_node
            .unwrap();
        let node = layout
            .blueprint()
            .templates
            .iter()
            .find_map(|t| find(&t.root, id))
            .unwrap();
        assert_eq!(node.frame[2], Expr::Literal(Value::Number(500.)));
    }
}
#[test]
fn empty_canvas_has_no_fake_instances_or_property_registers() {
    let ui = from_graph(|graph| {
        graph.presets.retain(|p| !p.palette);
        graph.instances.retain(|i| i.id == graph.presented_root);
        graph.definitions.retain(|d| d.id == DefinitionId(11));
    });
    oib::validate(&ui).unwrap();
    assert!(oib::seed(&ui).unwrap().cells.is_empty());
    let mut state = Document::new(ui.state_schema.clone(), "graph:empty").unwrap();
    state.configure_oib(&ui).unwrap();
    assert!(state.oib_allocations().unwrap().is_empty());
    assert!(state.oib_values(2).unwrap().is_empty());
    let snapshot = otrios_core::merkle::Snapshot::from_bytes(&state.save().unwrap()).unwrap();
    assert!(snapshot.subtree(&[4, 1]).is_err());
    assert!(snapshot.subtree(&[4, 2]).is_err());
    state.oib_compiled(2).unwrap();
}
#[test]
fn canonical_visuals_reject_renderer_local_bindings_and_undeclared_effects() {
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed.graph.as_mut().unwrap().definitions[0].nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .frame[0] = Expr::State(Binding::Local(1));
    assert!(canvas::decode_ui(&canvas::encode_ui(&ui).unwrap()).is_err());
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed.graph.as_mut().unwrap().grants.retain(|g| g.field != 1);
    assert!(canvas::decode_ui(&canvas::encode_ui(&ui).unwrap()).is_err());
}
#[test]
fn unbound_controls_stay_disabled_until_a_typed_granted_connection_is_authored() {
    use otrios_interfaces::{
        canvas::{Projection, StateRequest},
        oib::{
            InstanceOperation,
            authoring::{ConnectionSource, DocumentEdit, HierarchyEdit},
        },
    };
    let ui = from_graph(|graph| {
        graph
            .presets
            .iter_mut()
            .find(|p| p.id == 20)
            .unwrap()
            .assignments
            .retain(|a| {
                !matches!(
                    a.source,
                    otrios_interfaces::oib::graph::Source::AppBinding { .. }
                )
            });
    });
    oib::validate(&ui).unwrap();
    let mut state = Document::new(ui.state_schema.clone(), "graph:unbound").unwrap();
    state.configure_oib(&ui).unwrap();
    let fields = [1, 2, 3, 4].map(|id| state.typed_value(id).unwrap());
    let (before, _) = state.oib_compiled(2).unwrap();
    let object = ObjectAddress {
        instance: InstanceId(20),
        references: vec![NodeId(7)],
        node: NodeId(1),
    };
    fn find(
        n: &otrios_interfaces::layout::TemplateNode,
        id: u16,
    ) -> Option<&otrios_interfaces::layout::TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|c| find(c, id))
        }
    }
    let node_id = |ui: &otrios_interfaces::canvas::Ui| {
        oib::seed(ui)
            .unwrap()
            .designer
            .as_ref()
            .unwrap()
            .mappings
            .objects
            .iter()
            .find(|m| m.address == object)
            .unwrap()
            .layout_node
            .unwrap()
    };
    let before_layout = otrios_core::dashboard::layout::Layout::from_bytes(&before.layout).unwrap();
    assert!(
        before_layout
            .blueprint()
            .templates
            .iter()
            .find_map(|t| find(&t.root, node_id(&before)))
            .unwrap()
            .responder
            .is_none()
    );
    let (mut session, _) = state.open_session(&[83; 16], 0).unwrap();
    session.set_oib_editable(true);
    let request = canvas::encode_state_request(&StateRequest::OibInstance {
        epoch: state.oib_epoch().unwrap(),
        sequence: 1,
        operation: InstanceOperation {
            generation: state.oib_status().unwrap().generation,
            operations: vec![DocumentEdit::Hierarchy(HierarchyEdit::Connect {
                object: object.clone(),
                property: PropertyId(4),
                source: Some(ConnectionSource {
                    field: 1,
                    projection: Projection::Identity,
                }),
            })],
        },
    })
    .unwrap();
    assert!(state.receive(&mut session, &request).unwrap());
    assert!(
        state.oib_status().unwrap().error.is_none(),
        "{:?}",
        state.oib_status().unwrap().error
    );
    let (after, _) = state.oib_compiled(2).unwrap();
    let after_layout = otrios_core::dashboard::layout::Layout::from_bytes(&after.layout).unwrap();
    let Some(otrios_interfaces::layout::Responder::AppEvent { control, .. }) = after_layout
        .blueprint()
        .templates
        .iter()
        .find_map(|t| find(&t.root, node_id(&after)))
        .unwrap()
        .responder
    else {
        panic!("connected slider responder")
    };
    assert_eq!(
        after
            .handlers
            .iter()
            .find(|h| h.control == control)
            .unwrap()
            .fields,
        vec![1]
    );
    assert_eq!(
        [1, 2, 3, 4].map(|id| state.typed_value(id).unwrap()),
        fields
    );
}
#[test]
fn definitions_own_visual_geometry_and_palette_records_are_not_document_objects() {
    let ui = admitted();
    let seed = oib::seed(&ui).unwrap();
    let graph = seed.graph.as_ref().unwrap();
    let panel = graph
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    assert!(
        panel
            .nodes
            .iter()
            .find(|n| n.id == NodeId(12))
            .unwrap()
            .visual
            .is_some()
    );
    assert_eq!(panel.slots.len(), 1);
    assert!(graph.instances.iter().all(|i| i.id != InstanceId(50)));
    let stripped = oib::graph::strip(&ui).unwrap();
    let layout = otrios_core::dashboard::layout::Layout::from_bytes(&stripped.layout).unwrap();
    fn find(
        n: &otrios_interfaces::layout::TemplateNode,
        id: u16,
    ) -> Option<&otrios_interfaces::layout::TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|c| find(c, id))
        }
    }
    let viewport = layout
        .blueprint()
        .templates
        .iter()
        .find_map(|t| find(&t.root, graph.preview_viewport))
        .unwrap();
    assert!(viewport.children.is_empty());
    assert!(stripped.selectors.is_empty());
    assert!(matches!(stripped.presentation, Some(Presentation::Oib(_))));
}

#[test]
fn compound_visual_parameters_use_current_causal_values_during_graph_lowering() {
    use otrios_interfaces::{
        canvas::Projection,
        oib::graph::{Parameter, ParameterSource},
    };
    let ui = from_graph(|graph| {
        let panel = graph
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(10))
            .unwrap();
        let visual = panel
            .nodes
            .iter_mut()
            .find(|n| n.id == NodeId(7))
            .unwrap()
            .visual
            .as_mut()
            .unwrap();
        visual.parameters.push(Parameter {
            local: 99,
            source: ParameterSource::Property(PropertyPath {
                references: vec![NodeId(7)],
                node: NodeId(1),
                property: PropertyId(2),
            }),
            projection: Projection::Identity,
        });
        visual.frame[0] = Expr::Multiply(
            Box::new(Expr::Multiply(
                Box::new(Expr::Parameter(99)),
                Box::new(Expr::Parameter(99)),
            )),
            Box::new(Expr::Literal(Value::Number(0.00001))),
        );
    });
    let seed = oib::seed(&ui).unwrap();
    let allocations = oib::instances::initial_allocations(seed);
    let shared = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    let cell = oib::document::cell_for_address(seed, &allocations, &shared).unwrap();
    let values = std::collections::BTreeMap::from([(cell, Some(Value::Number(400.)))]);
    let current = oib::instances::expand(
        &ui,
        &allocations,
        &allocations.iter().map(|a| a.id).collect(),
        None,
        &values,
    )
    .unwrap();
    let layout = oib::compile(&current, &values).unwrap();
    let object = ObjectAddress {
        instance: InstanceId(20),
        references: vec![],
        node: NodeId(7),
    };
    let node_id = oib::seed(&current)
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .mappings
        .objects
        .iter()
        .find(|m| m.address == object)
        .unwrap()
        .layout_node
        .unwrap();
    fn find(
        n: &otrios_interfaces::layout::TemplateNode,
        id: u16,
    ) -> Option<&otrios_interfaces::layout::TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|c| find(c, id))
        }
    }
    let node = layout
        .blueprint()
        .templates
        .iter()
        .find_map(|t| find(&t.root, node_id))
        .unwrap();
    assert_eq!(
        node.frame[0],
        Expr::Multiply(
            Box::new(Expr::Multiply(
                Box::new(Expr::Literal(Value::Number(400.))),
                Box::new(Expr::Literal(Value::Number(400.)))
            )),
            Box::new(Expr::Literal(Value::Number(0.00001)))
        )
    );
}

#[test]
fn explicit_preset_geometry_wins_over_linked_frame_parameter_outputs() {
    use otrios_interfaces::{
        canvas::Projection,
        oib::graph::{Parameter, ParameterSource},
    };
    let ui = from_graph(|graph| {
        let root = graph
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(10))
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| n.id == NodeId(1))
            .unwrap()
            .visual
            .as_mut()
            .unwrap();
        root.parameters.push(Parameter {
            local: 99,
            source: ParameterSource::Property(PropertyPath {
                references: vec![NodeId(7)],
                node: NodeId(1),
                property: PropertyId(2),
            }),
            projection: Projection::Number {
                scale: 0.01,
                offset: 0.,
            },
        });
        root.frame[0] = Expr::Parameter(99);
        graph.presets.iter_mut().find(|p| p.id == 20).unwrap().frame = Some([0., 0., 520., 360.]);
    });
    let mut state = Document::new(ui.state_schema.clone(), "graph:frame-override").unwrap();
    state.configure_oib(&ui).unwrap();
    let (current, layout) = state.oib_compiled(2).unwrap();
    let object = ObjectAddress {
        instance: InstanceId(20),
        references: vec![],
        node: NodeId(1),
    };
    let node_id = oib::seed(&current)
        .unwrap()
        .designer
        .as_ref()
        .unwrap()
        .mappings
        .objects
        .iter()
        .find(|m| m.address == object)
        .unwrap()
        .layout_node
        .unwrap();
    fn find(
        n: &otrios_interfaces::layout::TemplateNode,
        id: u16,
    ) -> Option<&otrios_interfaces::layout::TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|c| find(c, id))
        }
    }
    assert_eq!(
        layout
            .blueprint()
            .templates
            .iter()
            .find_map(|t| find(&t.root, node_id))
            .unwrap()
            .frame[0],
        Expr::Literal(Value::Number(0.))
    );
}

#[test]
fn palette_capacity_is_independent_of_live_and_resident_instance_limits() {
    let ui = from_graph(|graph| {
        let template = graph.presets.iter().find(|p| p.id == 50).unwrap().clone();
        for id in 60..68 {
            let mut preset = template.clone();
            preset.id = id;
            preset.name = format!("Power {id}");
            graph.presets.push(preset);
        }
    });
    let seed = oib::seed(&ui).unwrap();
    assert_eq!(seed.instances.len(), 11);
    let mut state = Document::new(ui.state_schema.clone(), "graph:palette-capacity").unwrap();
    state.configure_oib(&ui).unwrap();
    assert_eq!(state.oib_allocations().unwrap().len(), 2);
    assert!(
        oib::seed(&state.oib_compiled(2).unwrap().0)
            .unwrap()
            .cells
            .len()
            <= otrios_interfaces::oib::MAX_CELLS
    );
}

#[test]
fn source_and_presented_hierarchies_share_semantic_navigation_without_renderer_predictions() {
    let ui = admitted();
    let seed = oib::seed(&ui).unwrap();
    let mut allocations = oib::instances::initial_allocations(seed);
    allocations.push(otrios_interfaces::oib::Allocation {
        id: 65536,
        name: "Dynamic Power".into(),
        component: 13,
        prototype: 50,
        slot: 2,
    });
    let live = allocations.iter().map(|a| a.id).collect();
    let source = oib::document::hierarchy(
        seed,
        &allocations,
        &live,
        oib::document::HierarchyScope::Composition,
    )
    .unwrap();
    let properties = oib::instances::expanded_seed(seed, &allocations).unwrap();
    let expanded = oib::document::hierarchy(
        &properties,
        &allocations,
        &live,
        oib::document::HierarchyScope::Composition,
    )
    .unwrap();
    let current = oib::instances::expand(
        &ui,
        &allocations,
        &live,
        None,
        &std::collections::BTreeMap::new(),
    )
    .unwrap();
    let presented = oib::document::hierarchy(
        oib::seed(&current).unwrap(),
        &allocations,
        &live,
        oib::document::HierarchyScope::Composition,
    )
    .unwrap();
    let navigation = |rows: &[oib::document::HierarchyRow]| {
        rows.iter()
            .map(|r| (r.key.clone(), r.parent.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(navigation(&source), navigation(&expanded));
    assert_eq!(navigation(&source), navigation(&presented));
    let panel = Selection::Object(ObjectAddress {
        instance: InstanceId(20),
        references: vec![],
        node: NodeId(1),
    });
    let slider = Selection::Object(ObjectAddress {
        instance: InstanceId(20),
        references: vec![NodeId(7)],
        node: NodeId(1),
    });
    assert_eq!(
        source
            .iter()
            .find(|r| r.selection.as_ref() == Some(&slider))
            .unwrap()
            .parent,
        Some(oib::document::HierarchyKey::Selection(panel))
    );
    assert!(
        source
            .iter()
            .find(|r| r.selection.as_ref() == Some(&slider))
            .unwrap()
            .layout_node
            .is_none()
    );
    assert!(
        presented
            .iter()
            .find(|r| r.selection.as_ref() == Some(&slider))
            .unwrap()
            .layout_node
            .is_some()
    );
}
