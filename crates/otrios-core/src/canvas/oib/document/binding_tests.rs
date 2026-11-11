// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

fn descriptor(id: u64, value_type: ValueType) -> PropertyDescriptor {
    PropertyDescriptor {
        id: PropertyId(id),
        name: format!("value_{id}"),
        label: "Value".into(),
        value_type,
        default: if value_type == ValueType::Boolean {
            Value::Boolean(false)
        } else {
            Value::Number(0.)
        },
        number_range: (value_type == ValueType::Number).then_some([0., 1000.]),
        number_step: None,
        units: None,
        group: InspectorGroup::Behavior,
        editor: PropertyEditor::ReadOnly,
        policy: PropertyPolicy {
            live_literal: false,
            app_binding: true,
        },
    }
}
fn classes() -> Vec<ClassDescriptor> {
    [
        ClassKind::O2DPanel,
        ClassKind::O2DSlider,
        ClassKind::O2DToggle,
        ClassKind::O2DView,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, implementation)| ClassDescriptor {
        id: ClassId(index as u64 + 1),
        name: format!("{implementation:?}"),
        implementation,
        base: None,
        properties: match implementation {
            ClassKind::O2DSlider => vec![descriptor(1, ValueType::Number)],
            ClassKind::O2DToggle => vec![descriptor(1, ValueType::Boolean)],
            _ => vec![],
        },
        owned_child_classes: vec![],
        visual_child_classes: vec![],
    })
    .collect()
}
fn node(id: u64, class: u64) -> DefinitionNode {
    DefinitionNode {
        id: NodeId(id),
        name: format!("Node {id}"),
        class: ClassId(class),
        owner: None,
        visual_children: vec![],
        defaults: vec![],
        reference: None,
        visual: None,
    }
}
fn concrete(id: u64, class: u64) -> ComponentDefinition {
    ComponentDefinition {
        id: DefinitionId(id),
        name: format!("Component {id}"),
        root: NodeId(1),
        nodes: vec![node(1, class)],
        properties: vec![],
        next_property: 0,
        exposed: vec![],
        slots: vec![],
    }
}
fn path(references: &[u64], node: u64, property: u64) -> PropertyPath {
    PropertyPath {
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
        property: PropertyId(property),
    }
}
fn wrapper(id: u64, target: u64, value_type: ValueType) -> ComponentDefinition {
    let mut definition = concrete(id, 1);
    definition.properties = vec![descriptor(2, value_type)];
    definition.next_property = 3;
    let mut reference = node(2, 4);
    reference.owner = Some(NodeId(1));
    reference.reference = Some(ComponentReference {
        definition: DefinitionId(target),
        assignments: vec![],
    });
    definition.nodes.push(reference);
    definition.exposed = vec![ExposedProperty {
        property: PropertyId(2),
        targets: vec![path(&[2], 1, 1)],
    }];
    definition
}

#[test]
fn binding_kind_accepts_signed_controls_and_typed_root_routes() {
    let classes = classes();
    let mut outer = wrapper(100, 101, ValueType::Number);
    outer.exposed[0].targets[0].property = PropertyId(2);
    let definitions = [
        concrete(10, 2),
        concrete(20, 3),
        wrapper(101, 10, ValueType::Number),
        outer,
        wrapper(200, 20, ValueType::Boolean),
    ];
    assert_eq!(
        binding_kind(&classes, &definitions, DefinitionId(10), &path(&[], 1, 1)).unwrap(),
        ClassKind::O2DSlider
    );
    assert_eq!(
        binding_kind(&classes, &definitions, DefinitionId(20), &path(&[], 1, 1)).unwrap(),
        ClassKind::O2DToggle
    );
    assert_eq!(
        binding_kind(&classes, &definitions, DefinitionId(100), &path(&[], 1, 2)).unwrap(),
        ClassKind::O2DSlider
    );
    assert_eq!(
        binding_kind(&classes, &definitions, DefinitionId(200), &path(&[], 1, 2)).unwrap(),
        ClassKind::O2DToggle
    );
}

#[test]
fn binding_kind_memoizes_fanout_but_preserves_longer_reference_depth() {
    let classes = classes();
    let mut outer = wrapper(100, 10, ValueType::Number);
    let mut repeated = outer.nodes[1].clone();
    repeated.id = NodeId(3);
    outer.nodes.push(repeated);
    outer.exposed[0].targets.push(path(&[3], 1, 1));
    let mut definitions = vec![concrete(10, 2), outer];
    assert_eq!(
        binding_kind(&classes, &definitions, DefinitionId(100), &path(&[], 1, 2)).unwrap(),
        ClassKind::O2DSlider
    );
    // Visit the leaf on the short first branch before reaching it through eight
    // additional wrappers. Reusing its proof must not hide the nine-edge path.
    definitions[1].nodes[2]
        .reference
        .as_mut()
        .unwrap()
        .definition = DefinitionId(101);
    definitions[1].exposed[0].targets[1].property = PropertyId(2);
    for id in 101..=108 {
        let mut next = wrapper(id, if id == 108 { 10 } else { id + 1 }, ValueType::Number);
        if id != 108 {
            next.exposed[0].targets[0].property = PropertyId(2);
        }
        definitions.push(next);
    }
    assert!(
        binding_kind(&classes, &definitions, DefinitionId(100), &path(&[], 1, 2))
            .unwrap_err()
            .contains("depth")
    );
}

#[test]
fn binding_kind_rejects_forged_declared_property_on_slider_class_root() {
    let classes = classes();
    let mut forged = concrete(10, 2);
    forged.properties.push(descriptor(2, ValueType::Number));
    forged.next_property = 3;
    assert!(
        binding_kind(
            &classes,
            &[forged.clone()],
            DefinitionId(10),
            &path(&[], 1, 2)
        )
        .is_err()
    );
    forged.exposed.push(ExposedProperty {
        property: PropertyId(2),
        targets: vec![path(&[], 1, 1)],
    });
    assert_eq!(
        binding_kind(&classes, &[forged], DefinitionId(10), &path(&[], 1, 2)).unwrap(),
        ClassKind::O2DSlider
    );
}

#[test]
fn binding_kind_rejects_type_range_policy_and_empty_route_claims() {
    let classes = classes();
    let original = wrapper(100, 10, ValueType::Number);
    for change in 0..5 {
        let mut invalid = original.clone();
        match change {
            0 => invalid.properties[0].number_range = Some([0., 100.]),
            1 => {
                invalid.properties[0] = descriptor(2, ValueType::Boolean);
            }
            2 => invalid.properties[0].policy.live_literal = true,
            3 => invalid.properties[0].policy.app_binding = false,
            _ => invalid.exposed[0].targets.clear(),
        }
        assert!(
            binding_kind(
                &classes,
                &[concrete(10, 2), invalid],
                DefinitionId(100),
                &path(&[], 1, 2)
            )
            .is_err(),
            "invalid claim {change}"
        );
    }
}

#[test]
fn binding_kind_rejects_ambiguous_missing_and_cyclic_paths() {
    let classes = classes();
    let original = wrapper(100, 10, ValueType::Number);
    let mut missing = original.clone();
    missing.exposed[0].targets[0].references = vec![NodeId(999)];
    assert!(
        binding_kind(
            &classes,
            &[concrete(10, 2), missing],
            DefinitionId(100),
            &path(&[], 1, 2)
        )
        .is_err()
    );
    let mut ambiguous = original.clone();
    ambiguous.nodes.push(ambiguous.nodes[0].clone());
    assert!(
        binding_kind(
            &classes,
            &[concrete(10, 2), ambiguous],
            DefinitionId(100),
            &path(&[], 1, 2)
        )
        .is_err()
    );
    let mut alias_cycle = concrete(100, 1);
    alias_cycle.properties = vec![
        descriptor(2, ValueType::Number),
        descriptor(3, ValueType::Number),
    ];
    alias_cycle.next_property = 4;
    alias_cycle.exposed = vec![
        ExposedProperty {
            property: PropertyId(2),
            targets: vec![path(&[], 1, 3)],
        },
        ExposedProperty {
            property: PropertyId(3),
            targets: vec![path(&[], 1, 2)],
        },
    ];
    assert!(
        binding_kind(
            &classes,
            &[alias_cycle],
            DefinitionId(100),
            &path(&[], 1, 2)
        )
        .is_err()
    );
    let mut reference_cycle = original;
    reference_cycle.nodes[1]
        .reference
        .as_mut()
        .unwrap()
        .definition = DefinitionId(100);
    assert!(
        binding_kind(
            &classes,
            &[reference_cycle],
            DefinitionId(100),
            &path(&[], 1, 2)
        )
        .is_err()
    );
}
