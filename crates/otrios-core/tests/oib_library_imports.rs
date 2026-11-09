// SPDX-License-Identifier: LGPL-3.0-only
//! Pure resolver checks over synthetic already-verified dependency bodies.
//! Package signatures/trust are tested by the independent library package suite.
#![cfg(feature = "canvas")]
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "support/oib_library_fixture.rs"]
mod fixture;
use fixture::*;
use otrios_core::{
    canvas::{self, oib},
    library,
};
use otrios_interfaces::{
    canvas::Ui,
    layout::{Expr, Value},
    oib::{
        designer::*,
        graph::{self, DefinitionBinding, ImportBinding, Imports, LibraryLock, LockedExport},
        library::{ExportId, LibraryId},
    },
};

#[test]
fn canonical_source_keeps_imported_bodies_separate_from_editable_local_definitions() {
    let ui = admit(&imported());
    oib::validate(&ui).unwrap();
    let source = seed(&ui);
    assert_eq!(
        source
            .graph
            .as_ref()
            .unwrap()
            .definitions
            .iter()
            .map(|definition| definition.id.0)
            .collect::<Vec<_>>(),
        [10, 11]
    );
    assert_eq!(
        source
            .designer
            .as_ref()
            .unwrap()
            .definitions
            .iter()
            .map(|definition| definition.id.0)
            .collect::<Vec<_>>(),
        [10, 11, 12, 13]
    );
    let encoded = canvas::encode_ui(&ui).unwrap();
    let raw: serde_json::Value = flexbuffers::from_slice(&encoded).unwrap();
    let body = &raw["presentation"]["Oib"];
    assert_eq!(body["graph"]["definitions"].as_array().unwrap().len(), 2);
    assert_eq!(body["resolved_libraries"].as_array().unwrap().len(), 1);
    assert_eq!(
        canvas::encode_ui(&canvas::decode_ui(&encoded).unwrap()).unwrap(),
        encoded
    );
    assert_eq!(
        oib::imports::imported_definitions(source),
        [DefinitionId(12), DefinitionId(13)].into_iter().collect()
    );
    let allocations = oib::instances::initial_allocations(source);
    let current = oib::instances::expand(
        &ui,
        &allocations,
        &allocations.iter().map(|allocation| allocation.id).collect(),
        None,
        &Default::default(),
    )
    .unwrap();
    oib::compile(&current, &Default::default()).unwrap();
    assert_eq!(seed(&current).graph.as_ref().unwrap().definitions.len(), 2);
}

#[test]
fn imported_structure_is_read_only_while_local_wrappers_and_causal_sources_remain_available() {
    use otrios_interfaces::oib::authoring::{DefinitionBatch, DefinitionEdit};
    let ui = admit(&imported());
    let source = seed(&ui);
    for operation in [
        DefinitionEdit::RenameDefinition {
            definition: DefinitionId(12),
            name: "Copy?".into(),
        },
        DefinitionEdit::SetVisual {
            definition: DefinitionId(12),
            node: NodeId(1),
            visual: None,
        },
        DefinitionEdit::SetDefault {
            definition: DefinitionId(13),
            node: NodeId(1),
            property: PropertyId(3),
            value: Value::Text("Edited?".into()),
        },
    ] {
        assert!(
            oib::document::authoring::transform(source, &[operation])
                .unwrap_err()
                .contains("read-only")
        );
    }
    let changed = oib::document::authoring::apply(
        &ui,
        &DefinitionBatch {
            generation: 1,
            operations: vec![DefinitionEdit::RenameDefinition {
                definition: DefinitionId(10),
                name: "Local wrapper".into(),
            }],
        },
    )
    .unwrap();
    assert_eq!(seed(&changed).graph.as_ref().unwrap().definitions.len(), 2);
    assert!(
        oib::graph::with_source(
            &ui,
            &source.designer.as_ref().unwrap().definitions,
            &source.graph.as_ref().unwrap().presets
        )
        .unwrap_err()
        .contains("read-only")
    );
    let allocations = oib::instances::initial_allocations(source);
    let address = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    assert!(
        oib::document::cell_for_address(source, &allocations, &address).is_ok(),
        "shared causal overrides remain app-owned"
    );
}

#[test]
fn compatible_defaults_and_visual_revisions_keep_addresses_without_expanding_app_grants() {
    let old = admit(&imported());
    let changed = update(&old, |library| {
        let root = &mut library
            .definitions
            .iter_mut()
            .find(|definition| definition.id == DefinitionId(12))
            .unwrap()
            .nodes[0];
        root.defaults
            .iter_mut()
            .find(|value| value.property == PropertyId(2))
            .unwrap()
            .value = Value::Number(400.);
        root.visual.as_mut().unwrap().frame[0] = Expr::Literal(Value::Number(1.));
        library.exports[0].name = "Styled Slider".into();
    });
    let changed = admit(&changed);
    oib::imports::compatible(seed(&old), seed(&changed)).unwrap();
    assert_ne!(
        oib::imports::lock_digest(seed(&old)).unwrap(),
        oib::imports::lock_digest(seed(&changed)).unwrap()
    );
    assert_eq!(
        oib::contract::admission(&old).unwrap(),
        oib::contract::admission(&changed).unwrap()
    );
    let old_allocations = oib::instances::initial_allocations(seed(&old));
    let new_allocations = oib::instances::initial_allocations(seed(&changed));
    let address = PropertyAddress::Definition {
        definition: DefinitionId(12),
        node: NodeId(1),
        property: PropertyId(2),
    };
    let old_cell = oib::document::cell_for_address(seed(&old), &old_allocations, &address).unwrap();
    let new_cell =
        oib::document::cell_for_address(seed(&changed), &new_allocations, &address).unwrap();
    assert_eq!(old_cell, new_cell);
    assert_eq!(
        oib::effective(seed(&old), &Default::default(), old_cell).unwrap(),
        Value::Number(440.)
    );
    assert_eq!(
        oib::effective(seed(&changed), &Default::default(), new_cell).unwrap(),
        Value::Number(400.)
    );
    assert_eq!(
        oib::effective(
            seed(&changed),
            &[(new_cell, Some(Value::Number(420.)))]
                .into_iter()
                .collect(),
            new_cell
        )
        .unwrap(),
        Value::Number(420.)
    );
}

#[test]
fn lock_namespace_and_class_contract_mismatches_reject_the_complete_candidate() {
    let original = imported();
    for fault in 0..9 {
        let mut ui = original.clone();
        let source = seed_mut(&mut ui);
        match fault {
            0 => source.resolved_libraries.clear(),
            1 => source.resolved_libraries[0].revision = [3; 32],
            2 => {
                source
                    .graph
                    .as_mut()
                    .unwrap()
                    .imports
                    .as_mut()
                    .unwrap()
                    .locks[0]
                    .exports[0]
                    .contract = [9; 32]
            }
            3 => source
                .graph
                .as_mut()
                .unwrap()
                .imports
                .as_mut()
                .unwrap()
                .bindings[0]
                .definitions
                .clear(),
            4 => {
                source
                    .graph
                    .as_mut()
                    .unwrap()
                    .imports
                    .as_mut()
                    .unwrap()
                    .bindings[0]
                    .definitions[0]
                    .target = DefinitionId(10)
            }
            5 => {
                source.resolved_libraries[0].graph.required_classes[0].implementation =
                    ClassKind::OIBDocument
            }
            6 => source.graph.as_mut().unwrap().imports = None,
            7 => {
                let imports = source.graph.as_mut().unwrap().imports.as_mut().unwrap();
                imports.locks[0].exports[1].export = ExportId(999);
                imports.bindings[1].export = ExportId(999);
            }
            _ => {
                let property = &mut source.resolved_libraries[0]
                    .graph
                    .required_classes
                    .iter_mut()
                    .find(|class| class.properties.iter().any(|p| p.id == PropertyId(3)))
                    .unwrap()
                    .properties
                    .iter_mut()
                    .find(|p| p.id == PropertyId(3))
                    .unwrap();
                property.value_type = otrios_interfaces::layout::ValueType::Number;
                property.default = Value::Number(0.);
            }
        }
        assert!(oib::imports::resolve(source).is_err(), "fault {fault}");
        assert_eq!(seed(&original).graph.as_ref().unwrap().definitions.len(), 2);
    }
}

#[test]
fn changed_semantic_contract_or_export_binding_requires_new_admission() {
    let old = admit(&imported());
    let changed = update(&old, |library| {
        library
            .definitions
            .iter_mut()
            .find(|definition| definition.id == DefinitionId(12))
            .unwrap()
            .nodes[0]
            .visual
            .as_mut()
            .unwrap()
            .action = None
    });
    assert!(oib::imports::compatible(seed(&old), seed(&changed)).is_err());
    let mut changed = old.clone();
    seed_mut(&mut changed)
        .graph
        .as_mut()
        .unwrap()
        .imports
        .as_mut()
        .unwrap()
        .bindings
        .swap(0, 1);
    assert!(oib::imports::compatible(seed(&old), seed(&changed)).is_err());
    let mut changed = old.clone();
    seed_mut(&mut changed)
        .graph
        .as_mut()
        .unwrap()
        .grants
        .clear();
    assert!(oib::imports::compatible(seed(&old), seed(&changed)).is_err());
}

#[test]
fn aggregate_definition_node_and_import_source_budgets_are_not_multiplied_per_library() {
    let original = imported();
    let mut definitions = original.clone();
    let source = seed_mut(&mut definitions);
    let graph = source.graph.as_mut().unwrap();
    let prototype = graph.definitions[0].clone();
    for id in 100..113 {
        let mut extra = prototype.clone();
        extra.id = DefinitionId(id);
        graph.definitions.push(extra);
    }
    assert!(
        oib::imports::resolve(source)
            .unwrap_err()
            .contains("aggregate app graph budget")
    );
    let mut nodes = original.clone();
    let source = seed_mut(&mut nodes);
    let root = &mut source.graph.as_mut().unwrap().definitions[0];
    for id in 100..170 {
        root.nodes.push(DefinitionNode {
            id: NodeId(id),
            name: format!("Nonvisual {id}"),
            class: ClassId(1),
            owner: Some(root.root),
            visual_children: vec![],
            defaults: vec![],
            reference: None,
            visual: None,
        });
    }
    assert!(
        oib::imports::resolve(source)
            .unwrap_err()
            .contains("aggregate app graph budget")
    );

    let mut ui = original.clone();
    let source = seed_mut(&mut ui);
    let mut body = source.resolved_libraries[0].graph.clone();
    let prototype = body.definitions[0].clone();
    let mut next = 100;
    while flexbuffers::to_vec(&body).unwrap().len() <= graph::MAX_IMPORTED_BYTES / 4 {
        let mut extra = prototype.clone();
        extra.id = DefinitionId(next);
        next += 1;
        body.definitions.push(extra);
        body = library::seal_graph(body).unwrap();
    }
    let original_body = source.resolved_libraries[0].clone();
    source.resolved_libraries[0].graph = body.clone();
    let imports = source.graph.as_mut().unwrap().imports.as_mut().unwrap();
    for index in 2..=4 {
        let name = LibraryId(format!("{}:controls-{index}", "0".repeat(64)));
        let mut resolved = original_body.clone();
        resolved.library = name.clone();
        resolved.graph = body.clone();
        source.resolved_libraries.push(resolved);
        let mut pin = imports.locks[0].clone();
        pin.library = name.clone();
        imports.locks.push(pin);
        for (offset, export) in [(0, 1), (1, 2)] {
            imports.bindings.push(ImportBinding {
                id: 100 + index * 2 + offset,
                library: name.clone(),
                export: ExportId(export),
                definitions: vec![DefinitionBinding {
                    source: DefinitionId(if export == 1 { 12 } else { 13 }),
                    target: DefinitionId(200 + index * 2 + offset),
                }],
            });
        }
    }
    assert!(
        oib::imports::resolve(source)
            .unwrap_err()
            .contains("source budget")
    );
}

#[test]
fn pinned_old_source_layout_and_handler_closure_stay_immutable_during_candidate_resolution() {
    let old = admit(&imported());
    let bytes = canvas::encode_ui(&old).unwrap();
    let layout = old.layout.clone();
    let handlers = old.handlers.clone();
    let candidate = update(&old, |library| {
        library.definitions[0].nodes[0]
            .visual
            .as_mut()
            .unwrap()
            .frame[0] = Expr::Literal(Value::Number(2.))
    });
    let candidate = admit(&candidate);
    oib::imports::compatible(seed(&old), seed(&candidate)).unwrap();
    assert_ne!(candidate.layout, layout);
    assert_eq!(old.layout, layout);
    assert_eq!(old.handlers, handlers);
    assert_eq!(canvas::encode_ui(&old).unwrap(), bytes);
    assert_ne!(
        oib::imports::lock_digest(seed(&old)).unwrap(),
        oib::imports::lock_digest(seed(&candidate)).unwrap()
    );
}

#[test]
fn an_exact_revision_cannot_be_reused_for_different_library_body_bytes() {
    let old = admit(&imported());
    let mut corrupted = old.clone();
    seed_mut(&mut corrupted).resolved_libraries[0]
        .graph
        .definitions[0]
        .nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .frame[0] = Expr::Literal(Value::Number(2.));
    assert!(
        oib::imports::compatible(seed(&old), seed(&corrupted))
            .unwrap_err()
            .contains("different canonical bodies")
    );
}

#[test]
fn import_lock_codec_has_a_fixed_domain_and_namespace_vector() {
    let mut ui = imported();
    let source = seed_mut(&mut ui);
    let library = LibraryId(format!("{}:x", "0".repeat(64)));
    let imports = Imports {
        format: graph::IMPORTS_FORMAT.into(),
        locks: vec![LibraryLock {
            library: library.clone(),
            revision: [1; 32],
            exports: vec![LockedExport {
                export: ExportId(2),
                contract: [3; 32],
            }],
        }],
        bindings: vec![ImportBinding {
            id: 7,
            library,
            export: ExportId(2),
            definitions: vec![DefinitionBinding {
                source: DefinitionId(5),
                target: DefinitionId(9),
            }],
        }],
    };
    let encoded = flexbuffers::to_vec(&imports).unwrap();
    assert_eq!(
        flexbuffers::from_slice::<Imports>(&encoded).unwrap(),
        imports
    );
    source.graph.as_mut().unwrap().imports = Some(imports);
    let digest = oib::imports::lock_digest(source).unwrap();
    assert_eq!(
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "67563ad07d8afd18ea77253eee57651d8ade327787b02453135df60f5bf3a3cf"
    );
}

#[test]
fn slow_library_reader_pins_one_ack_and_one_flight_then_coalesces_or_explicitly_resyncs() {
    use otrios_core::{
        canvas::Document,
        merkle::{Authority, Error, Replica, Snapshot},
    };
    let ui = admit(&imported());
    let mut document = Document::new(ui.state_schema.clone(), "library:slow-reader").unwrap();
    document.configure_oib(&ui).unwrap();
    let initial = Snapshot::from_bytes(&document.save().unwrap()).unwrap();
    let pinned = initial.clone();
    let mut authority = Authority::new(37, initial);
    let mut subscription = authority.subscribe(&[vec![]], vec![]).unwrap();
    let mut replica = Replica::new(&subscription);
    let first = authority.offer(&mut subscription).unwrap().unwrap();
    assert!(replica.apply(first.bytes()).unwrap());
    subscription.acknowledge(first.cursor, first.root).unwrap();
    let revise = |revision: u8| {
        let mut next = update(&ui, |library| {
            library.definitions[0].nodes[0]
                .visual
                .as_mut()
                .unwrap()
                .frame[0] = Expr::Literal(Value::Number(revision as f32 / 16.))
        });
        let source = seed_mut(&mut next);
        source.resolved_libraries[0].revision = [revision; 32];
        source
            .graph
            .as_mut()
            .unwrap()
            .imports
            .as_mut()
            .unwrap()
            .locks[0]
            .revision = [revision; 32];
        admit(&next)
    };
    let publish = |document: &mut Document, authority: &mut Authority, next: &Ui| {
        let (generation, baseline) = document.oib_baseline().unwrap();
        assert!(
            document
                .adopt_oib_baseline(next, generation, baseline)
                .unwrap()
        );
        let snapshot = Snapshot::from_bytes(&document.save().unwrap()).unwrap();
        authority
            .publish(authority.snapshot().hash(), snapshot)
            .unwrap();
    };
    publish(&mut document, &mut authority, &revise(2));
    let flight = authority.offer(&mut subscription).unwrap().unwrap();
    assert_eq!(flight.cursor.sequence, 2);
    for revision in 3..=32 {
        publish(&mut document, &mut authority, &revise(revision));
        let repeated = authority.offer(&mut subscription).unwrap().unwrap();
        assert_eq!(repeated.cursor, flight.cursor);
        assert_eq!(repeated.bytes(), flight.bytes());
        assert_eq!(
            replica.snapshot().unwrap().hash(),
            pinned.hash(),
            "a slow reader retains its complete old snapshot"
        );
    }
    assert_ne!(pinned.hash(), authority.snapshot().hash());
    assert_eq!(pinned.hash(), first.root.unwrap());
    assert!(replica.apply(flight.bytes()).unwrap());
    subscription
        .acknowledge(flight.cursor, flight.root)
        .unwrap();
    let latest = authority.offer(&mut subscription).unwrap().unwrap();
    assert_eq!(
        latest.cursor.sequence, 3,
        "intermediate library revisions are coalesced, never queued"
    );
    assert!(replica.apply(latest.bytes()).unwrap());
    subscription
        .acknowledge(latest.cursor, latest.root)
        .unwrap();
    assert_eq!(
        replica.snapshot().unwrap().hash(),
        authority.snapshot().hash()
    );
    assert!(authority.offer(&mut subscription).unwrap().is_none());
    assert_eq!(replica.apply(first.bytes()), Err(Error::Stale));
    let mut lost_ram = Replica::new(&subscription);
    publish(&mut document, &mut authority, &revise(33));
    let delta = authority.offer(&mut subscription).unwrap().unwrap();
    assert_eq!(lost_ram.apply(delta.bytes()), Err(Error::ResyncRequired));
    subscription.resync();
    let full = authority.offer(&mut subscription).unwrap().unwrap();
    assert!(full.cursor.sequence > delta.cursor.sequence);
    assert!(lost_ram.apply(full.bytes()).unwrap());
    subscription.acknowledge(full.cursor, full.root).unwrap();
    assert_eq!(
        lost_ram.snapshot().unwrap().hash(),
        authority.snapshot().hash()
    );
    subscription.cancel();
    assert!(matches!(
        authority.offer(&mut subscription),
        Err(Error::Forbidden)
    ));
}
