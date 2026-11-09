// SPDX-License-Identifier: LGPL-3.0-only
//! Owned-slot authority checks. In-process evidence, not physical acceptance.
#![cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
mod app;
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
use otrios_core::{
    canvas::{self, Document, oib},
    merkle::Snapshot,
};
use otrios_interfaces::{
    canvas::{Presentation, StateRequest, Ui},
    oib::{InstanceOperation, authoring::*, designer::*},
};

fn ui() -> Ui {
    let _ = app::manifest();
    let mut ui = app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!()
    };
    let doc = seed.designer.as_mut().unwrap();
    doc.definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(10))
        .unwrap()
        .slots
        .push(SlotDescriptor {
            id: SlotId(1),
            name: "Slider overlay".into(),
            references: vec![],
            node: NodeId(7),
            admitted_classes: vec![ClassId(8)],
            cardinality: [0, 2],
            layout: SlotLayout::Absolute,
        });
    doc.definitions
        .iter_mut()
        .find(|d| d.id == DefinitionId(13))
        .unwrap()
        .slots
        .push(SlotDescriptor {
            id: SlotId(1),
            name: "Owned controls".into(),
            references: vec![],
            node: NodeId(1),
            admitted_classes: vec![ClassId(3)],
            cardinality: [2, 4],
            layout: SlotLayout::Absolute,
        });
    oib::validate(&ui).unwrap();
    ui
}
fn slot(instance: u64) -> Destination {
    Destination::Slot {
        component: ObjectAddress {
            instance: InstanceId(instance),
            references: vec![],
            node: NodeId(1),
        },
        slot: SlotId(1),
    }
}
fn edit(
    doc: &mut Document,
    session: &mut canvas::Session,
    sequence: u64,
    operations: Vec<HierarchyEdit>,
) -> Vec<u8> {
    let bytes = canvas::encode_state_request(&StateRequest::OibInstance {
        epoch: doc.oib_epoch().unwrap(),
        sequence,
        operation: InstanceOperation::Hierarchy(HierarchyBatch {
            generation: doc.oib_status().unwrap().generation,
            operations,
        }),
    })
    .unwrap();
    assert!(doc.receive(session, &bytes).unwrap());
    bytes
}
fn setup() -> (Ui, Document, canvas::Session) {
    let ui = ui();
    let mut doc = Document::new(ui.state_schema.clone(), "d2:hierarchy").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut session, _) = doc.open_session(&[91; 16], 0).unwrap();
    session.set_oib_editable(true);
    (ui, doc, session)
}
fn insert(destination: Destination, frame: Option<[f32; 4]>) -> HierarchyEdit {
    HierarchyEdit::Insert {
        prototype: InstanceId(50),
        destination,
        before: None,
        frame,
    }
}
fn snapshot(doc: &Document) -> Snapshot {
    Snapshot::from_bytes(&doc.save().unwrap()).unwrap()
}

#[test]
fn nested_owned_insert_move_and_explicit_parent_removal_repair_preserve_values() {
    let (_ui, mut doc, mut session) = setup();
    doc.set_typed_authority(&[(1, otrios_interfaces::layout::Value::Number(481.))])
        .unwrap();
    let original = snapshot(&doc).subtree(&[1]).unwrap().hash();
    let generation = doc.oib_status().unwrap().generation;
    edit(
        &mut doc,
        &mut session,
        1,
        vec![insert(slot(20), Some([0., 0., 264., 144.]))],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let parent = doc.oib_allocations().unwrap().last().unwrap().id;
    edit(
        &mut doc,
        &mut session,
        2,
        vec![insert(slot(parent), Some([0., 64., 264., 64.]))],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let child = doc.oib_allocations().unwrap().last().unwrap().id;
    let compiled = doc.oib_compiled(2).unwrap().0;
    let metadata = oib::seed(&compiled).unwrap().designer.as_ref().unwrap();
    assert_eq!(
        metadata
            .instances
            .iter()
            .find(|i| i.id.0 == child)
            .unwrap()
            .owner,
        Owner::Instance(InstanceId(parent))
    );
    assert_eq!(
        metadata
            .instances
            .iter()
            .find(|i| i.id.0 == parent)
            .unwrap()
            .owner,
        Owner::Instance(InstanceId(20))
    );
    let active = doc.oib_status().unwrap().active_root;
    edit(
        &mut doc,
        &mut session,
        3,
        vec![HierarchyEdit::Remove {
            instance: InstanceId(parent),
        }],
    );
    assert_eq!(
        doc.oib_status().unwrap().active_root,
        active,
        "referenced owner removal must reject"
    );
    assert!(doc.oib_status().unwrap().error.is_some());
    edit(
        &mut doc,
        &mut session,
        4,
        vec![
            HierarchyEdit::Remove {
                instance: InstanceId(parent),
            },
            HierarchyEdit::Move {
                instance: InstanceId(child),
                destination: Destination::Canvas,
                before: None,
                frame: None,
            },
        ],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    assert_eq!(doc.oib_status().unwrap().generation, generation + 3);
    assert!(
        !doc.oib_allocations()
            .unwrap()
            .iter()
            .any(|a| a.id == parent)
    );
    assert_eq!(snapshot(&doc).subtree(&[1]).unwrap().hash(), original);
}

#[test]
fn owned_subtree_duplicate_allocates_fresh_roots_and_remaps_internal_ownership() {
    let (_ui, mut doc, mut session) = setup();
    edit(
        &mut doc,
        &mut session,
        1,
        vec![insert(slot(20), Some([0., 0., 264., 144.]))],
    );
    let parent = doc.oib_allocations().unwrap().last().unwrap().id;
    edit(
        &mut doc,
        &mut session,
        2,
        vec![insert(slot(parent), Some([0., 64., 264., 64.]))],
    );
    let child = doc.oib_allocations().unwrap().last().unwrap().id;
    edit(
        &mut doc,
        &mut session,
        3,
        vec![HierarchyEdit::Duplicate {
            instance: InstanceId(parent),
            name: None,
            destination: Destination::Canvas,
            before: None,
            frame: Some([0., 560., 264., 144.]),
        }],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let allocations = doc.oib_allocations().unwrap();
    assert_eq!(allocations.len(), 6);
    let copied_parent = allocations[4].id;
    let copied_child = allocations[5].id;
    assert!(copied_parent > child && copied_child > copied_parent);
    let compiled = doc.oib_compiled(2).unwrap().0;
    let metadata = oib::seed(&compiled).unwrap().designer.as_ref().unwrap();
    assert_eq!(
        metadata
            .instances
            .iter()
            .find(|i| i.id.0 == copied_child)
            .unwrap()
            .owner,
        Owner::Instance(InstanceId(copied_parent))
    );
    assert_eq!(
        metadata
            .instances
            .iter()
            .find(|i| i.id.0 == child)
            .unwrap()
            .owner,
        Owner::Instance(InstanceId(parent))
    );
}

#[test]
fn slot_cardinality_rejects_the_whole_batch_without_consuming_identities() {
    let (_ui, mut doc, mut session) = setup();
    let before = snapshot(&doc).subtree(&[4, 4]).unwrap().to_bytes().unwrap();
    let active = doc.oib_status().unwrap().active_root;
    edit(
        &mut doc,
        &mut session,
        1,
        (0..3)
            .map(|_| HierarchyEdit::Insert {
                prototype: InstanceId(50),
                destination: slot(20),
                before: None,
                frame: None,
            })
            .collect(),
    );
    assert_eq!(
        snapshot(&doc).subtree(&[4, 4]).unwrap().to_bytes().unwrap(),
        before
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    assert!(
        doc.oib_status()
            .unwrap()
            .error
            .unwrap()
            .contains("cardinality")
    );
}

#[test]
fn wrong_slot_class_cardinality_cycles_and_geometry_reject_without_allocating() {
    let (_ui, mut doc, mut session) = setup();
    let original = doc.oib_status().unwrap().active_root;
    let next = snapshot(&doc).subtree(&[4, 4]).unwrap().to_bytes().unwrap();
    edit(
        &mut doc,
        &mut session,
        1,
        vec![HierarchyEdit::Insert {
            prototype: InstanceId(20),
            destination: slot(20),
            before: None,
            frame: None,
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, original);
    assert_eq!(
        snapshot(&doc).subtree(&[4, 4]).unwrap().to_bytes().unwrap(),
        next
    );
    edit(
        &mut doc,
        &mut session,
        2,
        vec![insert(slot(20), Some([0., 0., 264., 144.]))],
    );
    let parent = doc.oib_allocations().unwrap().last().unwrap().id;
    edit(
        &mut doc,
        &mut session,
        3,
        vec![insert(slot(parent), Some([0., 64., 264., 64.]))],
    );
    let child = doc.oib_allocations().unwrap().last().unwrap().id;
    let active = doc.oib_status().unwrap().active_root;
    edit(
        &mut doc,
        &mut session,
        4,
        vec![HierarchyEdit::Move {
            instance: InstanceId(parent),
            destination: slot(child),
            before: None,
            frame: None,
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    edit(
        &mut doc,
        &mut session,
        5,
        vec![HierarchyEdit::Place {
            instance: InstanceId(parent),
            frame: [0., 0., 1., 1.],
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    // A sibling anchor cannot silently identify a different destination list.
    edit(
        &mut doc,
        &mut session,
        6,
        vec![HierarchyEdit::Move {
            instance: InstanceId(child),
            destination: Destination::Canvas,
            before: Some(InstanceId(parent)),
            frame: None,
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
}

#[test]
fn owned_placement_recovers_exactly_and_receipts_do_not_allocate_twice() {
    let (ui, mut doc, mut session) = setup();
    let accepted = edit(
        &mut doc,
        &mut session,
        1,
        vec![insert(slot(20), Some([0., 0., 264., 64.]))],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let active = doc.oib_status().unwrap().active_root;
    let allocations = doc.oib_allocations().unwrap();
    let mut recovered = Document::load(
        ui.state_schema.clone(),
        "d2:hierarchy",
        &doc.save().unwrap(),
    )
    .unwrap();
    recovered.configure_oib(&ui).unwrap();
    let (mut resumed, _) = recovered.open_session(&[91; 16], 1).unwrap();
    resumed.set_oib_editable(true);
    assert!(!recovered.receive(&mut resumed, &accepted).unwrap());
    assert_eq!(recovered.oib_status().unwrap().active_root, active);
    assert_eq!(recovered.oib_allocations().unwrap(), allocations);
    assert_eq!(
        recovered.oib_compiled(2).unwrap().0,
        doc.oib_compiled(2).unwrap().0
    );
}

fn object(instance: u64, references: &[u64]) -> ObjectAddress {
    ObjectAddress {
        instance: InstanceId(instance),
        references: references.iter().copied().map(NodeId).collect(),
        node: NodeId(1),
    }
}
fn source(ui: &Ui, field: u16, toggle: bool) -> otrios_interfaces::layout::Binding {
    let projection = if toggle {
        otrios_interfaces::canvas::Projection::Nonzero
    } else {
        otrios_interfaces::canvas::Projection::Identity
    };
    otrios_interfaces::layout::Binding::Local(
        ui.selectors
            .iter()
            .find(|s| s.field == field && s.projection == projection)
            .unwrap()
            .local,
    )
}
fn control_field(ui: &Ui, target: &ObjectAddress) -> u16 {
    use otrios_interfaces::layout::{Responder, TemplateNode};
    let doc = oib::seed(ui).unwrap().designer.as_ref().unwrap();
    let id = doc
        .mappings
        .objects
        .iter()
        .find(|m| m.address == *target)
        .unwrap()
        .layout_node
        .unwrap();
    fn find(n: &TemplateNode, id: u16) -> Option<&TemplateNode> {
        if n.id == id {
            Some(n)
        } else {
            n.children.iter().find_map(|n| find(n, id))
        }
    }
    let layout = otrios_core::dashboard::layout::Layout::from_bytes(&ui.layout).unwrap();
    let node = layout
        .blueprint()
        .templates
        .iter()
        .find_map(|t| find(&t.root, id))
        .unwrap();
    let Some(Responder::AppEvent { control, .. }) = node.responder else {
        panic!("missing control")
    };
    ui.handlers
        .iter()
        .find(|h| h.control == control)
        .unwrap()
        .fields[0]
}

#[test]
fn explicit_connections_update_bindings_and_effect_fields_together_without_writing_data() {
    let (ui, mut doc, mut session) = setup();
    let original = snapshot(&doc).subtree(&[1]).unwrap().to_bytes().unwrap();
    let slider = object(20, &[7]);
    let toggle = object(20, &[8]);
    edit(
        &mut doc,
        &mut session,
        1,
        vec![
            HierarchyEdit::Connect {
                object: slider.clone(),
                property: PropertyId(4),
                source: Some(source(&ui, 2, false)),
            },
            HierarchyEdit::Connect {
                object: toggle.clone(),
                property: PropertyId(5),
                source: Some(source(&ui, 4, true)),
            },
        ],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let compiled = doc.oib_compiled(2).unwrap().0;
    assert_eq!(control_field(&compiled, &slider), 2);
    assert_eq!(control_field(&compiled, &toggle), 4);
    assert_eq!(control_field(&compiled, &object(30, &[7])), 2);
    assert_eq!(
        snapshot(&doc).subtree(&[1]).unwrap().to_bytes().unwrap(),
        original
    );
    let active = doc.oib_status().unwrap().active_root;
    let mut recovered = Document::load(
        ui.state_schema.clone(),
        "d2:hierarchy",
        &doc.save().unwrap(),
    )
    .unwrap();
    recovered.configure_oib(&ui).unwrap();
    assert_eq!(recovered.oib_status().unwrap().active_root, active);
    assert_eq!(recovered.oib_compiled(2).unwrap().0, compiled);
    edit(
        &mut doc,
        &mut session,
        2,
        vec![
            HierarchyEdit::Connect {
                object: slider.clone(),
                property: PropertyId(4),
                source: None,
            },
            HierarchyEdit::Connect {
                object: toggle.clone(),
                property: PropertyId(5),
                source: None,
            },
        ],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let restored = doc.oib_compiled(2).unwrap().0;
    assert_eq!(control_field(&restored, &slider), 1);
    assert_eq!(control_field(&restored, &toggle), 3);
    assert_eq!(
        snapshot(&doc).subtree(&[1]).unwrap().to_bytes().unwrap(),
        original
    );
}

#[test]
fn rewiring_a_parent_control_leaves_owned_child_controls_independent_and_copies_connections() {
    let (ui, mut doc, mut session) = setup();
    edit(
        &mut doc,
        &mut session,
        1,
        vec![insert(slot(20), Some([0., 0., 264., 144.]))],
    );
    let parent = doc.oib_allocations().unwrap().last().unwrap().id;
    edit(
        &mut doc,
        &mut session,
        2,
        vec![insert(slot(parent), Some([0., 64., 264., 64.]))],
    );
    let child = doc.oib_allocations().unwrap().last().unwrap().id;
    edit(
        &mut doc,
        &mut session,
        3,
        vec![HierarchyEdit::Connect {
            object: object(parent, &[]),
            property: PropertyId(5),
            source: Some(source(&ui, 4, true)),
        }],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let compiled = doc.oib_compiled(2).unwrap().0;
    assert_eq!(control_field(&compiled, &object(parent, &[])), 4);
    assert_eq!(control_field(&compiled, &object(child, &[])), 3);
    edit(
        &mut doc,
        &mut session,
        4,
        vec![HierarchyEdit::Duplicate {
            instance: InstanceId(parent),
            name: Some("copied-parent".into()),
            destination: Destination::Canvas,
            before: None,
            frame: Some([0., 560., 264., 144.]),
        }],
    );
    assert!(
        doc.oib_status().unwrap().error.is_none(),
        "{:?}",
        doc.oib_status().unwrap().error
    );
    let allocations = doc.oib_allocations().unwrap();
    let compiled = doc.oib_compiled(2).unwrap().0;
    assert_eq!(control_field(&compiled, &object(allocations[4].id, &[])), 4);
    assert_eq!(control_field(&compiled, &object(allocations[5].id, &[])), 3);
    assert_eq!(allocations[4].name, "copied-parent");
}

#[test]
fn a_signed_read_selector_does_not_grant_a_new_field_effect_pair() {
    let mut ui = ui();
    ui.selectors.push(otrios_interfaces::canvas::Selector {
        local: 999,
        field: 3,
        projection: otrios_interfaces::canvas::Projection::Identity,
    });
    let mut doc = Document::new(ui.state_schema.clone(), "d2:hierarchy").unwrap();
    doc.configure_oib(&ui).unwrap();
    let (mut session, _) = doc.open_session(&[91; 16], 0).unwrap();
    session.set_oib_editable(true);
    let active = doc.oib_status().unwrap().active_root;
    edit(
        &mut doc,
        &mut session,
        1,
        vec![HierarchyEdit::Connect {
            object: object(20, &[7]),
            property: PropertyId(4),
            source: Some(otrios_interfaces::layout::Binding::Local(999)),
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    assert!(doc.oib_status().unwrap().error.is_some());
    edit(
        &mut doc,
        &mut session,
        2,
        vec![HierarchyEdit::Connect {
            object: object(20, &[8]),
            property: PropertyId(5),
            source: Some(source(&ui, 2, false)),
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
    edit(
        &mut doc,
        &mut session,
        3,
        vec![HierarchyEdit::Connect {
            object: object(20, &[7]),
            property: PropertyId(3),
            source: Some(source(&ui, 2, false)),
        }],
    );
    assert_eq!(doc.oib_status().unwrap().active_root, active);
}
