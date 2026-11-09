// SPDX-License-Identifier: LGPL-3.0-only
//! Local projection checks, not live Frame, authority-process or Linux IPC acceptance.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::canvas::oib::{
    document::{HierarchyKey, HierarchyRow, HierarchyScope, hierarchy},
    instances,
};
use otrios_interfaces::{
    canvas::Presentation,
    oib::{Allocation, Seed, designer::*},
};
use std::collections::BTreeSet;

fn seed() -> Seed {
    let ui =
        otrios_core::canvas::decode_ui(&otrios_core::canvas::encode_ui(&app::ui(&[])).unwrap())
            .unwrap();
    let Some(Presentation::Oib(seed)) = ui.presentation else {
        panic!("fixture")
    };
    *seed
}
fn address(instance: u64, references: &[u64], node: u64) -> Selection {
    Selection::Object(ObjectAddress {
        instance: InstanceId(instance),
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(node),
    })
}
fn find<'a>(rows: &'a [HierarchyRow], selection: &Selection) -> &'a HierarchyRow {
    rows.iter()
        .find(|r| r.selection.as_ref() == Some(selection))
        .unwrap_or_else(|| panic!("missing hierarchy selection: {selection:?}"))
}
fn query(seed: &Seed) -> Vec<HierarchyRow> {
    let allocations = instances::initial_allocations(seed);
    hierarchy(
        seed,
        &allocations,
        &allocations.iter().map(|a| a.id).collect(),
        HierarchyScope::Composition,
    )
    .unwrap()
}
fn closure(rows: &[HierarchyRow]) {
    for (i, row) in rows.iter().enumerate() {
        let mut parent = row.parent.as_ref();
        let mut depth = 0;
        while let Some(key) = parent {
            let p = rows[..i]
                .iter()
                .find(|r| &r.key == key)
                .expect("parent precedes child");
            parent = p.parent.as_ref();
            depth += 1;
            assert!(depth <= 12);
        }
        assert_eq!(rows.iter().filter(|r| r.key == row.key).count(), 1);
    }
}
#[test]
fn composition_fuses_references_keeps_full_identity_names_and_owner_context() {
    let seed = seed();
    assert!(!app::manifest().id.is_empty());
    let rows = query(&seed);
    closure(&rows);
    let document = Selection::Document(DocumentId(1));
    let shell = address(40, &[], 1);
    assert_eq!(
        find(&rows, &shell).parent,
        Some(HierarchyKey::Selection(document.clone()))
    );
    for (id, name) in [(20, "Audio"), (30, "Display")] {
        let panel = address(id, &[], 1);
        let r = find(&rows, &panel);
        assert_eq!(r.name, name);
        assert_eq!(r.component, Some(DefinitionId(10)));
        assert_eq!(r.parent, Some(HierarchyKey::Selection(shell.clone())));
        for (reference, name) in [(7, "Slider reference"), (8, "Toggle reference")] {
            let r = find(&rows, &address(id, &[reference], 1));
            assert_eq!(r.name, name);
            assert_eq!(r.parent, Some(HierarchyKey::Selection(panel.clone())));
            assert_eq!(
                r.reference_wrapper,
                Some(ObjectAddress {
                    instance: InstanceId(id),
                    references: vec![],
                    node: NodeId(reference)
                })
            );
            assert!(
                !rows
                    .iter()
                    .any(|r| r.selection == Some(address(id, &[], reference)))
            );
            assert!(r.component.is_some());
        }
    }
    let group = rows
        .iter()
        .find(|r| matches!(r.key, HierarchyKey::Objects(_)))
        .unwrap();
    assert!(group.selection.is_none());
    assert_eq!(group.parent, Some(HierarchyKey::Selection(document)));
    let config = find(&rows, &address(40, &[], 2));
    assert_eq!(config.name, "Configuration");
    assert_eq!(config.parent, Some(group.key.clone()));
    assert_eq!(config.owner, Some(shell));
    assert!(config.layout_node.is_none());
    assert!(!rows.iter().any(|r| matches!(
        r.selection,
        Some(Selection::Definition(_) | Selection::DefinitionNode { .. })
    )));
}
#[test]
fn definitions_are_separate_and_do_not_expand_occurrences() {
    let seed = seed();
    let rows = hierarchy(&seed, &[], &BTreeSet::new(), HierarchyScope::Components).unwrap();
    closure(&rows);
    let doc = seed.designer.as_ref().unwrap();
    assert_eq!(
        rows.len(),
        doc.definitions.len() + doc.definitions.iter().map(|d| d.nodes.len()).sum::<usize>()
    );
    assert!(rows.iter().all(|r| matches!(
        r.selection,
        Some(Selection::Definition(_) | Selection::DefinitionNode { .. })
    )));
}
#[test]
fn rename_and_signed_reorder_change_presentation_without_rekeying() {
    let before = query(&seed());
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(source)) = &mut ui.presentation else {
        panic!("fixture")
    };
    let graph = source.graph.as_mut().unwrap();
    for definition in &mut graph.definitions {
        for node in &mut definition.nodes {
            node.name = format!("Renamed {}", node.id.0);
            node.visual_children.reverse();
        }
    }
    for instance in &mut graph.instances {
        instance.name = format!("Instance {}", instance.id.0);
        instance.order = u16::MAX - instance.order;
    }
    let normalized =
        otrios_core::canvas::decode_ui(&otrios_core::canvas::encode_ui(&ui).unwrap()).unwrap();
    let Some(Presentation::Oib(seed)) = normalized.presentation else {
        panic!("fixture")
    };
    let after = query(&seed);
    closure(&after);
    assert!(
        before
            .iter()
            .all(|r| after.iter().any(|n| n.key == r.key && n.parent == r.parent))
    );
    assert_eq!(find(&after, &address(20, &[], 1)).name, "Instance 20");
    assert!(
        after
            .iter()
            .position(|r| r.selection == Some(address(30, &[], 1)))
            < after
                .iter()
                .position(|r| r.selection == Some(address(20, &[], 1)))
    );
    assert!(
        after
            .iter()
            .position(|r| r.selection == Some(address(20, &[8], 1)))
            < after
                .iter()
                .position(|r| r.selection == Some(address(20, &[7], 1)))
    );
}
#[test]
fn retired_residents_are_hidden_and_reused_slots_keep_new_identity() {
    let seed = seed();
    let old = Allocation {
        id: 0x10000,
        name: "Old".into(),
        component: 10,
        prototype: 20,
        slot: 0,
    };
    let new = Allocation {
        id: 0x10001,
        name: "New".into(),
        ..old.clone()
    };
    let old_rows = hierarchy(
        &seed,
        std::slice::from_ref(&old),
        &BTreeSet::from([old.id]),
        HierarchyScope::Composition,
    )
    .unwrap();
    let rows = hierarchy(
        &seed,
        std::slice::from_ref(&old),
        &BTreeSet::new(),
        HierarchyScope::Composition,
    )
    .unwrap();
    assert!(
        !rows
            .iter()
            .any(|r| matches!(&r.selection, Some(Selection::Object(a)) if a.instance.0 == old.id))
    );
    assert!(find(&rows, &address(40, &[], 1)).selection.is_some());
    let rows = hierarchy(
        &seed,
        std::slice::from_ref(&new),
        &BTreeSet::from([new.id]),
        HierarchyScope::Composition,
    )
    .unwrap();
    closure(&rows);
    assert_eq!(find(&rows, &address(new.id, &[], 1)).name, "New");
    assert_eq!(
        find(&rows, &address(new.id, &[7], 1)).layout_node,
        find(&old_rows, &address(old.id, &[7], 1)).layout_node
    );
    assert!(
        !old_rows
            .iter()
            .any(|r| r.key == find(&rows, &address(new.id, &[7], 1)).key)
    );
}

#[test]
fn repeated_nested_references_keep_ancestry_and_fuse_each_boundary() {
    // A metadata-only resolver fixture: extend the admitted sample's shape
    // leaves into two references to the same leaf definition. Layout admission
    // is not under test; every new final address has an explicit mapping.
    let mut seed = seed();
    let doc = seed.designer.as_mut().unwrap();
    let slider = doc
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(12))
        .unwrap();
    let mut leaf = slider
        .nodes
        .iter()
        .find(|n| n.id == NodeId(4))
        .unwrap()
        .clone();
    leaf.id = NodeId(1);
    leaf.owner = None;
    for id in [4, 5] {
        let n = slider
            .nodes
            .iter_mut()
            .find(|n| n.id == NodeId(id))
            .unwrap();
        n.reference = Some(ComponentReference {
            definition: DefinitionId(14),
            assignments: vec![],
        });
    }
    doc.definitions.push(ComponentDefinition {
        id: DefinitionId(14),
        name: "Shared leaf".into(),
        root: NodeId(1),
        nodes: vec![leaf],
        exposed: vec![],
        slots: vec![],
    });
    for instance in [20, 30] {
        for reference in [4, 5] {
            doc.mappings.objects.push(ObjectMapping {
                address: ObjectAddress {
                    instance: InstanceId(instance),
                    references: vec![NodeId(7), NodeId(reference)],
                    node: NodeId(1),
                },
                layout_node: Some(60000 + instance as u16 + reference as u16),
            });
        }
    }
    let rows = query(&seed);
    closure(&rows);
    for instance in [20, 30] {
        for reference in [4, 5] {
            let r = find(&rows, &address(instance, &[7, reference], 1));
            assert_eq!(
                r.parent,
                Some(HierarchyKey::Selection(address(instance, &[7], 1)))
            );
            assert_eq!(r.component, Some(DefinitionId(14)));
            assert_eq!(
                r.reference_wrapper,
                Some(ObjectAddress {
                    instance: InstanceId(instance),
                    references: vec![NodeId(7)],
                    node: NodeId(reference)
                })
            );
            assert!(
                !rows
                    .iter()
                    .any(|r| r.selection == Some(address(instance, &[7], reference)))
            );
        }
    }
}

#[test]
fn visual_parent_is_independent_of_owner_and_nonvisual_chains_stay_under_objects() {
    let mut seed = seed();
    let doc = seed.designer.as_mut().unwrap();
    let fixed_id = doc.instances.iter().map(|i| i.id.0).max().unwrap() + 1;
    let panel = doc
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap();
    panel
        .nodes
        .iter_mut()
        .find(|n| n.id == NodeId(10))
        .unwrap()
        .owner = Some(NodeId(1));
    let shell = doc
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(11))
        .unwrap();
    let mut child = shell.nodes[1].clone();
    child.id = NodeId(3);
    child.owner = Some(NodeId(2));
    shell.nodes.push(child.clone());
    doc.mappings.objects.push(ObjectMapping {
        address: ObjectAddress {
            instance: InstanceId(40),
            references: vec![],
            node: NodeId(3),
        },
        layout_node: None,
    });
    // A fixed nonvisual root is not a factory allocation and must remain visible.
    child.id = NodeId(1);
    child.owner = None;
    doc.definitions.push(ComponentDefinition {
        id: DefinitionId(14),
        name: "Fixed object".into(),
        root: NodeId(1),
        nodes: vec![child],
        exposed: vec![],
        slots: vec![],
    });
    doc.instances.push(ComponentInstance {
        id: InstanceId(fixed_id),
        name: "Fixed configuration".into(),
        definition: DefinitionId(14),
        owner: Owner::Instance(InstanceId(40)),
        visual_parent: None,
        order: 2,
        assignments: vec![],
    });
    doc.mappings.objects.push(ObjectMapping {
        address: ObjectAddress {
            instance: InstanceId(fixed_id),
            references: vec![],
            node: NodeId(1),
        },
        layout_node: None,
    });
    let rows = query(&seed);
    closure(&rows);
    let surface = find(&rows, &address(20, &[], 10));
    assert_eq!(
        surface.parent,
        Some(HierarchyKey::Selection(address(20, &[], 9)))
    );
    assert_eq!(surface.owner, Some(address(20, &[], 1)));
    assert_eq!(
        find(&rows, &address(40, &[], 3)).parent,
        Some(HierarchyKey::Selection(address(40, &[], 2)))
    );
    let fixed = find(&rows, &address(fixed_id, &[], 1));
    assert_eq!(fixed.parent, Some(HierarchyKey::Objects(DocumentId(1))));
    assert_eq!(fixed.owner, Some(address(40, &[], 1)));
    assert_eq!(fixed.name, "Fixed configuration");
}

#[test]
fn maximum_six_live_roots_fit_and_retained_inactive_roots_are_filtered() {
    let seed = seed();
    let allocations: Vec<_> = (0..9)
        .map(|slot| Allocation {
            id: 0x10000 + u64::from(slot),
            name: format!("Allocation {slot}"),
            component: 10,
            prototype: if slot % 2 == 0 { 20 } else { 30 },
            slot,
        })
        .collect();
    let live = allocations[3..].iter().map(|a| a.id).collect();
    let rows = hierarchy(&seed, &allocations, &live, HierarchyScope::Composition).unwrap();
    closure(&rows);
    assert!(rows.len() <= 512);
    for a in &allocations[..3] {
        assert!(
            !rows.iter().any(
                |r| matches!(&r.selection, Some(Selection::Object(o)) if o.instance.0 == a.id)
            )
        );
    }
    for a in &allocations[3..] {
        assert_eq!(find(&rows, &address(a.id, &[], 1)).name, a.name);
    }
}

#[test]
fn missing_parents_and_excessive_depth_fail_instead_of_returning_partial_trees() {
    let mut seed = seed();
    let doc = seed.designer.as_mut().unwrap();
    doc.definitions[0].nodes[1].owner = Some(NodeId(999));
    assert!(hierarchy(&seed, &[], &BTreeSet::new(), HierarchyScope::Components).is_err());
    let mut seed = self::seed();
    let d = &mut seed.designer.as_mut().unwrap().definitions[0];
    let template = d.nodes[0].clone();
    for id in 100..114 {
        let mut n = template.clone();
        n.id = NodeId(id);
        n.owner = Some(if id == 100 { d.root } else { NodeId(id - 1) });
        n.visual_children.clear();
        n.reference = None;
        d.nodes.push(n);
    }
    assert!(hierarchy(&seed, &[], &BTreeSet::new(), HierarchyScope::Components).is_err());
}

#[test]
fn nonvisual_reference_navigation_follows_wrapper_owner_without_self_parenting() {
    let mut seed = seed();
    let doc = seed.designer.as_mut().unwrap();
    let shell = doc
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(11))
        .unwrap();
    let mut leaf = shell.nodes[1].clone();
    leaf.id = NodeId(1);
    leaf.owner = None;
    let mut wrapper = leaf.clone();
    wrapper.id = NodeId(3);
    wrapper.name = "Nested configuration".into();
    wrapper.owner = Some(NodeId(2));
    wrapper.reference = Some(ComponentReference {
        definition: DefinitionId(14),
        assignments: vec![],
    });
    shell.nodes.push(wrapper);
    doc.definitions.push(ComponentDefinition {
        id: DefinitionId(14),
        name: "Object component".into(),
        root: NodeId(1),
        nodes: vec![leaf],
        exposed: vec![],
        slots: vec![],
    });
    for (references, node) in [(vec![], 3), (vec![NodeId(3)], 1)] {
        doc.mappings.objects.push(ObjectMapping {
            address: ObjectAddress {
                instance: InstanceId(40),
                references,
                node: NodeId(node),
            },
            layout_node: None,
        });
    }
    let rows = query(&seed);
    closure(&rows);
    let r = find(&rows, &address(40, &[3], 1));
    assert_eq!(r.name, "Nested configuration");
    assert_eq!(r.parent, Some(HierarchyKey::Selection(address(40, &[], 2))));
    assert_eq!(r.owner, Some(address(40, &[], 3)));
    assert!(
        !rows
            .iter()
            .any(|r| r.selection == Some(address(40, &[], 3)))
    );
}

#[test]
fn visual_attachment_to_wrapper_uses_fused_parent_key() {
    // Metadata-only attachment fixture with a fixed component boundary in Shell.
    let mut seed = seed();
    let doc = seed.designer.as_mut().unwrap();
    let fixed_id = doc.instances.iter().map(|i| i.id.0).max().unwrap() + 1;
    let mut leaf = doc
        .definitions
        .iter()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == NodeId(10))
        .unwrap()
        .clone();
    leaf.id = NodeId(1);
    leaf.owner = None;
    let shell = doc
        .definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(11))
        .unwrap();
    let mut wrapper = leaf.clone();
    wrapper.id = NodeId(3);
    wrapper.owner = Some(NodeId(1));
    wrapper.reference = Some(ComponentReference {
        definition: DefinitionId(14),
        assignments: vec![],
    });
    shell.nodes[0].visual_children.push(NodeId(3));
    shell.nodes.push(wrapper);
    doc.definitions.push(ComponentDefinition {
        id: DefinitionId(14),
        name: "Fixed shape".into(),
        root: NodeId(1),
        nodes: vec![leaf],
        exposed: vec![],
        slots: vec![],
    });
    doc.instances.push(ComponentInstance {
        id: InstanceId(fixed_id),
        name: "Attached shape".into(),
        definition: DefinitionId(14),
        owner: Owner::Document,
        visual_parent: Some(ObjectAddress {
            instance: InstanceId(40),
            references: vec![],
            node: NodeId(3),
        }),
        order: 0,
        assignments: vec![],
    });
    for (instance, references, node, layout_node) in [
        (40, vec![], 3, 60000),
        (40, vec![NodeId(3)], 1, 60001),
        (fixed_id, vec![], 1, 60002),
    ] {
        doc.mappings.objects.push(ObjectMapping {
            address: ObjectAddress {
                instance: InstanceId(instance),
                references,
                node: NodeId(node),
            },
            layout_node: Some(layout_node),
        });
    }
    let rows = query(&seed);
    closure(&rows);
    assert_eq!(
        find(&rows, &address(fixed_id, &[], 1)).parent,
        Some(HierarchyKey::Selection(address(40, &[3], 1)))
    );
}

#[test]
fn query_preserves_allocation_live_and_resident_cell_budgets() {
    let seed = seed();
    let allocations: Vec<_> = (0..11)
        .map(|slot| Allocation {
            id: 0x10000 + u64::from(slot),
            name: format!("Allocation {slot}"),
            component: 10,
            prototype: 20,
            slot,
        })
        .collect();
    assert!(
        hierarchy(
            &seed,
            &allocations,
            &BTreeSet::new(),
            HierarchyScope::Composition
        )
        .is_err()
    );
    assert!(
        hierarchy(
            &seed,
            &allocations[..10],
            &BTreeSet::new(),
            HierarchyScope::Composition
        )
        .is_err()
    );
    let live = allocations[..7].iter().map(|a| a.id).collect();
    assert!(hierarchy(&seed, &allocations[..7], &live, HierarchyScope::Composition).is_err());
}
