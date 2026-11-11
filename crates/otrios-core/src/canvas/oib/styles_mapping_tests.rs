// SPDX-License-Identifier: LGPL-3.0-only
//! Canonical Designer component paths, not a second synthetic property engine.
use super::*;
use crate::canvas::oib::{self, document, graph, instances};
use otrios_interfaces::{canvas::Presentation, layout::Value, oib::designer::*};

fn path(references: &[u64], node: u64, property: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
        property: PropertyId(property),
    }
}
fn prepared(keep_default: bool) -> otrios_interfaces::canvas::Ui {
    let mut ui = crate::test_support::oib_designer::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("Missing fixture source")
    };
    let graph = seed.graph.as_mut().unwrap();
    if !keep_default {
        graph
            .definitions
            .iter_mut()
            .find(|d| d.id == DefinitionId(10))
            .unwrap()
            .nodes
            .iter_mut()
            .find(|n| n.id == NodeId(1))
            .unwrap()
            .defaults
            .retain(|default| default.property != PropertyId(1));
    }
    graph::materialize(&ui).unwrap()
}
fn catalog(seed: &otrios_interfaces::oib::Seed) -> Admitted {
    let doc = seed.designer.as_ref().unwrap();
    let definition = doc
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    let class = definition
        .nodes
        .iter()
        .find(|n| n.id == definition.root)
        .unwrap()
        .class;
    let reference = SystemRef {
        owner: super::Owner::Document(doc.id),
        system: SystemId(1),
    };
    let variable = VariableRef {
        system: reference.clone(),
        variable: VariableId(1),
    };
    Admitted::new(
        Catalog {
            format: FORMAT.into(),
            systems: vec![System {
                reference: reference.clone(),
                name: "base".into(),
                variables: vec![Variable {
                    id: VariableId(1),
                    name: "background".into(),
                    availability: Availability::Active,
                    values: ColorModes {
                        light: [240, 240, 240, 255],
                        dark: [12, 12, 12, 255],
                    },
                }],
                contracts: vec![Contract {
                    id: ContractId(1),
                    name: "panel".into(),
                    classes: vec![class],
                    fields: vec![Field {
                        id: FieldId(1),
                        name: "background".into(),
                        domain: Domain::Body,
                    }],
                }],
                styles: vec![Style {
                    id: StyleId(1),
                    name: "panel.primary".into(),
                    availability: Availability::Active,
                    contract: ContractId(1),
                    defaults: vec![Entry {
                        field: FieldId(1),
                        value: FieldValue::Variable(variable),
                    }],
                    patches: vec![],
                }],
            }],
        },
        &BTreeSet::from([reference.owner]),
    )
    .unwrap()
}
fn binding(catalog: &Admitted) -> ComponentStyle {
    ComponentStyle {
        definition: DefinitionId(10),
        style: catalog.styles.keys().next().unwrap().clone(),
        fields: vec![PropertyMapping {
            field: FieldId(1),
            targets: vec![path(&[], 1, 1)],
        }],
    }
}
fn scope() -> ObjectAddress {
    ObjectAddress {
        instance: InstanceId(20),
        references: vec![],
        node: NodeId(1),
    }
}
fn context(mode: Mode) -> Context<'static> {
    Context {
        mode,
        state: StateMask::default(),
        document: &[],
        instance: &[],
    }
}
fn address() -> PropertyAddress {
    PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: path(&[], 1, 1),
    }
}

#[test]
fn mapped_style_uses_real_component_color_path_without_mutating_source() {
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let before = flexbuffers::to_vec(seed).unwrap();
    let catalog = catalog(seed);
    let mapped = Mapped::new(&catalog, seed, binding(&catalog)).unwrap();
    let allocations = instances::initial_allocations(seed);
    let values = BTreeMap::new();
    for (mode, expected) in [(Mode::Light, 240), (Mode::Dark, 12)] {
        let output = mapped
            .resolve(&scope(), &allocations, &values, context(mode))
            .unwrap();
        let PropertyPaint::Style {
            field,
            source,
            trace: fallback,
        } = &output.properties()[&address()]
        else {
            panic!("Style should win above class fallback")
        };
        assert_eq!(*field, FieldId(1));
        assert_eq!(source.color, [expected, expected, expected, 255]);
        assert!(
            fallback
                .hops
                .iter()
                .any(|hop| matches!(hop.role, document::SourceRole::ClassDefault) && hop.masked)
        );
    }
    assert!(values.is_empty());
    assert_eq!(flexbuffers::to_vec(seed).unwrap(), before);
}

#[test]
fn mapped_style_preserves_node_shared_and_occurrence_literals_then_reset() {
    let ui = prepared(true);
    let seed = oib::seed(&ui).unwrap();
    let catalog = catalog(seed);
    let mapped = Mapped::new(&catalog, seed, binding(&catalog)).unwrap();
    let allocations = instances::initial_allocations(seed);
    let output = mapped
        .resolve(
            &scope(),
            &allocations,
            &BTreeMap::new(),
            context(Mode::Dark),
        )
        .unwrap();
    assert!(
        matches!(&output.properties()[&address()], PropertyPaint::Explicit(trace) if trace.hops.iter().any(|hop| !hop.masked && matches!(hop.role, document::SourceRole::NodeDefault)))
    );

    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let catalog = self::catalog(seed);
    let mapped = Mapped::new(&catalog, seed, binding(&catalog)).unwrap();
    let allocations = instances::initial_allocations(seed);
    let cell = document::cell_for_address(seed, &allocations, &address()).unwrap();
    let shared = document::cell_for_address(
        seed,
        &allocations,
        &PropertyAddress::Definition {
            definition: DefinitionId(10),
            node: NodeId(1),
            property: PropertyId(1),
        },
    )
    .unwrap();
    let mut values = BTreeMap::from([(shared, Some(Value::Color([7; 4])))]);
    let output = mapped
        .resolve(&scope(), &allocations, &values, context(Mode::Dark))
        .unwrap();
    assert!(
        matches!(&output.properties()[&address()], PropertyPaint::Explicit(trace) if trace.value == Some(Value::Color([7;4])))
    );
    values.insert(cell, Some(Value::Color([9; 4])));
    let output = mapped
        .resolve(&scope(), &allocations, &values, context(Mode::Dark))
        .unwrap();
    assert!(
        matches!(&output.properties()[&address()], PropertyPaint::Explicit(trace) if trace.value == Some(Value::Color([9;4])) && trace.overridden)
    );
    let sibling = ObjectAddress {
        instance: InstanceId(30),
        ..scope()
    };
    let sibling_address = PropertyAddress::Occurrence {
        instance: sibling.instance,
        path: path(&[], 1, 1),
    };
    let output = mapped
        .resolve(&sibling, &allocations, &values, context(Mode::Light))
        .unwrap();
    assert!(
        matches!(&output.properties()[&sibling_address], PropertyPaint::Explicit(trace) if trace.value == Some(Value::Color([7;4])) && !trace.overridden),
        "the first occurrence override does not leak to its sibling"
    );
    values.insert(cell, None);
    values.insert(shared, None);
    let output = mapped
        .resolve(&scope(), &allocations, &values, context(Mode::Dark))
        .unwrap();
    assert!(
        matches!(&output.properties()[&address()], PropertyPaint::Style { source, .. } if source.color == [12,12,12,255])
    );
}

#[test]
fn mapped_style_preserves_forwarding_and_rejects_bound_control_retyping() {
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let catalog = catalog(seed);
    let mut binding = binding(&catalog);
    binding.fields[0].targets = vec![path(&[7], 1, 1)];
    let mapped = Mapped::new(&catalog, seed, binding).unwrap();
    let allocations = instances::initial_allocations(seed);
    let output = mapped
        .resolve(
            &scope(),
            &allocations,
            &BTreeMap::new(),
            context(Mode::Light),
        )
        .unwrap();
    assert!(output.properties().values().all(
        |paint| matches!(paint, PropertyPaint::Explicit(trace) if trace.immediate_source.is_some())
    ));

    let bound = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: path(&[7], 1, 4),
    };
    let trace = document::property_sources(seed, &allocations, &bound, &BTreeMap::new()).unwrap();
    assert!(trace.binding.is_some() && trace.value.is_none() && trace.read_only);
    let mut invalid = self::binding(&catalog);
    invalid.fields[0].targets = vec![path(&[7], 1, 4)];
    assert!(
        Mapped::new(&catalog, seed, invalid).is_err(),
        "bound numeric control cannot become a style color"
    );
}

#[test]
fn mapped_style_rejects_type_path_scope_and_owner_mismatches() {
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let catalog = catalog(seed);
    for target in [
        path(&[7], 1, 2),
        path(&[7], 1, 4),
        path(&[999], 1, 1),
        path(&[], 999, 1),
    ] {
        let mut invalid = binding(&catalog);
        invalid.fields[0].targets = vec![target];
        assert!(Mapped::new(&catalog, seed, invalid).is_err());
    }
    let mut invalid = binding(&catalog);
    invalid.fields[0].targets.push(path(&[], 1, 1));
    assert!(Mapped::new(&catalog, seed, invalid).is_err());
    let mut invalid = binding(&catalog);
    invalid.fields.clear();
    assert!(Mapped::new(&catalog, seed, invalid).is_err());
    let mut invalid = binding(&catalog);
    invalid.definition = DefinitionId(12);
    assert!(Mapped::new(&catalog, seed, invalid).is_err());
    let mapped = Mapped::new(&catalog, seed, binding(&catalog)).unwrap();
    let allocations = instances::initial_allocations(seed);
    for scope in [
        ObjectAddress {
            instance: InstanceId(999),
            ..scope()
        },
        ObjectAddress {
            references: vec![NodeId(7)],
            ..scope()
        },
        ObjectAddress {
            node: NodeId(2),
            ..scope()
        },
    ] {
        assert!(
            mapped
                .resolve(&scope, &allocations, &BTreeMap::new(), context(Mode::Dark))
                .is_err()
        );
    }
    let mut foreign = seed.clone();
    foreign.graph.as_mut().unwrap().id = DocumentId(999);
    assert!(Mapped::new(&catalog, &foreign, binding(&catalog)).is_err());

    let mut source = catalog.catalog.clone();
    source.systems[0].contracts[0].fields.push(Field {
        id: FieldId(2),
        name: "text".into(),
        domain: Domain::Body,
    });
    source.systems[0].styles[0].defaults.push(Entry {
        field: FieldId(2),
        value: FieldValue::Literal([255; 4]),
    });
    let catalog = Admitted::new(
        source,
        &BTreeSet::from([catalog.catalog.systems[0].reference.owner.clone()]),
    )
    .unwrap();
    let mut invalid = binding(&catalog);
    invalid.fields.push(PropertyMapping {
        field: FieldId(2),
        targets: vec![path(&[], 1, 1)],
    });
    assert!(
        Mapped::new(&catalog, seed, invalid).is_err(),
        "two differently named fields cannot compete for one property"
    );
}

fn color_shapes(
    ui: &otrios_interfaces::canvas::Ui,
) -> BTreeMap<u16, otrios_interfaces::layout::Expr> {
    use otrios_interfaces::layout::{Shape, TemplateNode};
    fn visit(node: &TemplateNode, result: &mut BTreeMap<u16, otrios_interfaces::layout::Expr>) {
        match &node.shape {
            Shape::Text { color, .. }
            | Shape::Superellipse { color, .. }
            | Shape::Circle { color }
            | Shape::SeriesPath { color, .. }
            | Shape::TimedSeriesPath { color, .. } => {
                result.insert(node.id, color.clone());
            }
            _ => {}
        }
        for child in &node.children {
            visit(child, result);
        }
    }
    let layout = crate::dashboard::layout::Layout::from_bytes(&ui.layout).unwrap();
    let mut result = BTreeMap::new();
    for template in &layout.blueprint().templates {
        visit(&template.root, &mut result);
    }
    result
}
fn without_paint(ui: &otrios_interfaces::canvas::Ui) -> otrios_interfaces::layout::Blueprint {
    use otrios_interfaces::layout::{Expr, Shape, TemplateNode};
    fn visit(node: &mut TemplateNode) {
        match &mut node.shape {
            Shape::Text { color, .. }
            | Shape::Superellipse { color, .. }
            | Shape::Circle { color }
            | Shape::SeriesPath { color, .. }
            | Shape::TimedSeriesPath { color, .. } => {
                *color = Expr::Literal(Value::Color([0; 4]));
            }
            _ => {}
        }
        for child in &mut node.children {
            visit(child);
        }
    }
    let mut blueprint = crate::dashboard::layout::Layout::from_bytes(&ui.layout)
        .unwrap()
        .blueprint()
        .clone();
    for template in &mut blueprint.templates {
        visit(&mut template.root);
    }
    blueprint
}

#[test]
fn styled_compiler_changes_real_color_outputs_preserving_geometry_and_input() {
    use otrios_interfaces::layout::Expr;
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let source_before = flexbuffers::to_vec(seed).unwrap();
    let catalog = catalog(seed);
    let binding = binding(&catalog);
    let scope = scope();
    let allocations = instances::initial_allocations(seed);
    let live = allocations.iter().map(|a| a.id).collect();
    let values = BTreeMap::new();
    let baseline = graph::expand(&ui, &allocations, &live, None, &values).unwrap();
    let mut results = vec![];
    for (mode, channel) in [(Mode::Light, 240), (Mode::Dark, 12)] {
        let frame = Frame::new(
            seed,
            &allocations,
            &catalog,
            &[Occurrence {
                component: &binding,
                scope: &scope,
                context: context(mode),
            }],
        )
        .unwrap();
        let styled = graph::expand_styled(&ui, &allocations, &live, &values, &frame).unwrap();
        assert_eq!(styled.selectors, baseline.selectors);
        assert_eq!(styled.handlers, baseline.handlers);
        assert_eq!(
            without_paint(&styled),
            without_paint(&baseline),
            "color styling cannot rewrite geometry, controls or semantics"
        );
        let colors = color_shapes(&styled);
        assert!(
            colors.values().any(
                |value| *value == Expr::Literal(Value::Color([channel, channel, channel, 255]))
            ),
            "real template color must use resolved style"
        );
        results.push(colors);
        let trace =
            document::property_sources_styled(seed, &allocations, &address(), &values, &frame)
                .unwrap();
        assert_eq!(
            trace.value,
            Some(Value::Color([channel, channel, channel, 255]))
        );
        assert!(
            trace
                .hops
                .iter()
                .any(|hop| !hop.masked && matches!(hop.role, document::SourceRole::Style(_)))
        );
    }
    assert_ne!(results[0], results[1]);
    assert_eq!(flexbuffers::to_vec(seed).unwrap(), source_before);
    assert!(
        values.is_empty(),
        "style colors never enter causal property values"
    );
}

#[test]
fn styled_resolver_traces_outer_style_through_forwarding_and_masks_inner_style() {
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let catalog = catalog(seed);
    let mut source = catalog.catalog.clone();
    let slider = seed
        .designer
        .as_ref()
        .unwrap()
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(12))
        .unwrap();
    source.systems[0].contracts[0].classes.push(
        slider
            .nodes
            .iter()
            .find(|n| n.id == slider.root)
            .unwrap()
            .class,
    );
    source.systems[0].contracts[0].classes.sort();
    let catalog = Admitted::new(
        source,
        &BTreeSet::from([catalog.catalog.systems[0].reference.owner.clone()]),
    )
    .unwrap();
    let outer = binding(&catalog);
    let inner = ComponentStyle {
        definition: DefinitionId(12),
        ..outer.clone()
    };
    let outer_scope = scope();
    let inner_scope = ObjectAddress {
        references: vec![NodeId(7)],
        ..scope()
    };
    let allocations = instances::initial_allocations(seed);
    let frame = Frame::new(
        seed,
        &allocations,
        &catalog,
        &[
            Occurrence {
                component: &outer,
                scope: &outer_scope,
                context: context(Mode::Light),
            },
            Occurrence {
                component: &inner,
                scope: &inner_scope,
                context: context(Mode::Dark),
            },
        ],
    )
    .unwrap();
    let target = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: path(&[7], 1, 1),
    };
    let mut values = BTreeMap::new();
    let trace =
        document::property_sources_styled(seed, &allocations, &target, &values, &frame).unwrap();
    assert_eq!(trace.value, Some(Value::Color([240, 240, 240, 255])));
    assert_eq!(trace.inherited, trace.value);
    let styles: Vec<_> = trace
        .hops
        .iter()
        .filter(|hop| matches!(hop.role, document::SourceRole::Style(_)))
        .collect();
    assert_eq!(styles.len(), 2);
    assert!(styles[0].masked && !styles[1].masked);
    assert_eq!(styles[0].address, target);
    assert_eq!(styles[1].address, address());
    let cell = document::cell_for_address(seed, &allocations, &target).unwrap();
    values.insert(cell, Some(Value::Color([9; 4])));
    let trace =
        document::property_sources_styled(seed, &allocations, &target, &values, &frame).unwrap();
    assert_eq!(trace.value, Some(Value::Color([9; 4])));
    assert_eq!(trace.inherited, Some(Value::Color([240, 240, 240, 255])));
    assert!(
        trace
            .hops
            .iter()
            .filter(|hop| matches!(hop.role, document::SourceRole::Style(_)))
            .all(|hop| hop.masked)
    );
    values.insert(cell, None);
    let live = allocations.iter().map(|a| a.id).collect();
    let styled = graph::expand_styled(&ui, &allocations, &live, &values, &frame).unwrap();
    assert!(color_shapes(&styled).values().any(|color| *color
        == otrios_interfaces::layout::Expr::Literal(Value::Color([240, 240, 240, 255]))));
}

#[test]
fn styled_frame_rejects_stale_source_allocations_and_duplicate_uses() {
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let catalog = catalog(seed);
    let binding = binding(&catalog);
    let scope = scope();
    let allocations = instances::initial_allocations(seed);
    let usage = || Occurrence {
        component: &binding,
        scope: &scope,
        context: context(Mode::Dark),
    };
    assert!(Frame::new(seed, &allocations, &catalog, &[usage(), usage()]).is_err());
    let frame = Frame::new(seed, &allocations, &catalog, &[usage()]).unwrap();
    let mut changed = ui.clone();
    let Some(Presentation::Oib(source)) = &mut changed.presentation else {
        unreachable!()
    };
    source.graph.as_mut().unwrap().name.push_str(" changed");
    let live = allocations.iter().map(|a| a.id).collect();
    assert!(graph::expand_styled(&changed, &allocations, &live, &BTreeMap::new(), &frame).is_err());
    let mut changed_allocations = allocations.clone();
    changed_allocations[0].name.push('x');
    assert!(
        document::property_sources_styled(
            seed,
            &changed_allocations,
            &address(),
            &BTreeMap::new(),
            &frame
        )
        .is_err()
    );
    let oversized = vec![
        Override {
            field: FieldId(1),
            scope: Scope::All,
            value: FieldValue::Literal([0; 4])
        };
        MAX_OVERRIDES / 2 + 1
    ];
    let oversized_context = Context {
        document: &oversized,
        ..context(Mode::Dark)
    };
    assert!(
        matches!(Frame::new(seed, &allocations, &catalog, &[
        Occurrence { component: &binding, scope: &scope, context: oversized_context },
        Occurrence { component: &binding, scope: &scope, context: oversized_context },
    ]), Err(error) if error.contains("limit")),
        "aggregate override budget checked before resolving uses"
    );
}

#[test]
fn styled_compiler_keeps_focus_overlay_gate_and_bound_control_source() {
    let ui = prepared(false);
    let seed = oib::seed(&ui).unwrap();
    let catalog = catalog(seed);
    let mut source = catalog.catalog.clone();
    source.systems[0].contracts[0].fields.push(Field {
        id: FieldId(2),
        name: "focus".into(),
        domain: Domain::Focus,
    });
    source.systems[0].styles[0].defaults.push(Entry {
        field: FieldId(2),
        value: FieldValue::Literal([255; 4]),
    });
    let catalog = Admitted::new(
        source,
        &BTreeSet::from([catalog.catalog.systems[0].reference.owner.clone()]),
    )
    .unwrap();
    let binding = binding(&catalog);
    let scope = scope();
    let allocations = instances::initial_allocations(seed);
    let frame = Frame::new(
        seed,
        &allocations,
        &catalog,
        &[Occurrence {
            component: &binding,
            scope: &scope,
            context: Context {
                state: StateMask {
                    focus_visible: true,
                    ..Default::default()
                },
                ..context(Mode::Dark)
            },
        }],
    )
    .unwrap();
    assert_eq!(frame.focus()[&scope].len(), 1);
    let live = allocations.iter().map(|a| a.id).collect();
    assert!(
        matches!(graph::expand_styled(&ui, &allocations, &live, &BTreeMap::new(), &frame), Err(error) if error.contains("focus-overlay")),
        "unsupported focus overlay must not disappear silently"
    );
    let target = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: path(&[7], 1, 4),
    };
    let trace =
        document::property_sources_styled(seed, &allocations, &target, &BTreeMap::new(), &frame)
            .unwrap();
    assert!(trace.binding.is_some() && trace.value.is_none() && trace.read_only);
    assert!(
        !trace
            .hops
            .iter()
            .any(|hop| matches!(hop.role, document::SourceRole::Style(_)))
    );
}
