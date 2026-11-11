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
            fallback,
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
