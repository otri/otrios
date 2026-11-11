// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
use otrios_interfaces::{
    canvas::CatalogEntry,
    oib::graph::{
        DefinitionBinding, ImportBinding, Imports, LibraryLock, LockedExport, ResolvedLibrary,
    },
};
use otrios_interfaces::{
    canvas::Projection,
    layout::Binding,
    oib::{graph, library::FEATURE},
};
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[path = "../../../../apps/live-ui/app.rs"]
mod app;

fn graph() -> Graph {
    let class = ClassDescriptor {
        id: ClassId(1),
        name: "View".into(),
        implementation: ClassKind::O2DView,
        base: None,
        properties: vec![PropertyDescriptor {
            id: PropertyId(1),
            name: "Accent".into(),
            label: "Accent".into(),
            value_type: ValueType::Color,
            default: Value::Color([85, 136, 255, 255]),
            number_range: None,
            number_step: None,
            units: None,
            group: InspectorGroup::Appearance,
            editor: PropertyEditor::Palette,
            policy: PropertyPolicy {
                live_literal: true,
                app_binding: false,
            },
        }],
        owned_child_classes: vec![ClassId(1)],
        visual_child_classes: vec![ClassId(1)],
    };
    seal_graph(Graph {
        behaviors: vec![],
        format: FORMAT.into(),
        required_classes: vec![class],
        definitions: vec![ComponentDefinition {
            id: DefinitionId(10),
            name: "Card".into(),
            root: NodeId(1),
            nodes: vec![DefinitionNode {
                id: NodeId(1),
                name: "Card root".into(),
                class: ClassId(1),
                owner: None,
                visual_children: vec![],
                defaults: vec![PropertyDefault {
                    property: PropertyId(1),
                    value: Value::Color([85, 136, 255, 255]),
                }],
                reference: None,
                visual: Some(graph::Visual {
                    layout: None,
                    frame: [0., 0., 100., 50.].map(|n| Expr::Literal(Value::Number(n))),
                    resize: None,
                    visible: Expr::Literal(Value::Boolean(true)),
                    shape: Shape::Superellipse {
                        radius: Expr::Literal(Value::Number(8.)),
                        exponent: 4.,
                        color: Expr::Parameter(0),
                    },
                    semantics: None,
                    action: None,
                    parameters: vec![graph::Parameter {
                        local: 0,
                        source: graph::ParameterSource::Property(PropertyPath {
                            references: vec![],
                            node: NodeId(1),
                            property: PropertyId(1),
                        }),
                        projection: Projection::Identity,
                    }],
                }),
            }],
            properties: vec![],
            next_property: 0,
            exposed: vec![],
            slots: vec![],
        }],
        exports: vec![Export {
            id: ExportId(1),
            name: "Card".into(),
            definition: DefinitionId(10),
            contract: [0; 32],
        }],
    })
    .unwrap()
}
fn info(key: &[u8; 32]) -> LibraryInfo {
    LibraryInfo {
        format: INFO_FORMAT.into(),
        library: LibraryId(format!("{}:sample-2d", hex(&hash(key)))),
        version: "1.0".into(),
        required_features: vec![FEATURE.into()],
        entry_graph: GRAPH_PATH.into(),
    }
}
fn reseal(tree: &mut BundleTree) {
    tree.insert(
        FILES_PATH.into(),
        bundle::encode(&bundle::file_manifest(tree)).unwrap(),
    );
}
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
fn keys() -> (ring::signature::Ed25519KeyPair, [u8; 32]) {
    use ring::signature::KeyPair;
    let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let public = key.public_key().as_ref().try_into().unwrap();
    (key, public)
}
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
fn signed(
    info: &LibraryInfo,
    graph: &Graph,
    key: &ring::signature::Ed25519KeyPair,
    public: &[u8; 32],
) -> BundleTree {
    let mut tree = pack(info, graph).unwrap();
    let signature = key.sign(&signing_payload(&inspect(&tree).unwrap().digest, public).unwrap());
    attach_signature(&mut tree, public, signature.as_ref()).unwrap();
    tree
}

#[test]
fn exact_graph_codec_and_library_kind_have_deterministic_identity() {
    let graph = graph();
    let info = info(&[7; 32]);
    let tree = pack(&info, &graph).unwrap();
    assert_eq!(pack(&info, &graph).unwrap(), tree);
    assert_eq!(decode_graph(&encode_graph(&graph).unwrap()).unwrap(), graph);
    assert_eq!(decode_info(&encode_info(&info).unwrap()).unwrap(), info);
    let before = inspect(&tree).unwrap();
    assert_eq!(
        before.digest,
        "7929295e410267305f44c6e096f73e206b4256661abddbe93a9b3639c1d6c31d"
    );
    assert_eq!(
        hex(&graph.exports[0].contract),
        "1fa034bb165b9a0d0f4ea61deb1441d0d1dc7e6dfc90e9d62f04d15803749f3f"
    );
    assert!(bundle::inspect(&tree).is_err());
    let mut changed = tree.clone();
    changed.get_mut(INFO_PATH).unwrap()[0] = b'A';
    reseal(&mut changed);
    assert!(inspect(&changed).is_err());
    for path in tree.keys() {
        let mut changed = tree.clone();
        changed.get_mut(path).unwrap()[0] ^= 1;
        assert!(inspect(&changed).is_err(), "tamper {path}");
    }
    let mut forged = graph;
    forged.exports[0].contract[0] ^= 1;
    assert!(
        encode_graph(&forged).is_err(),
        "declared contracts must equal derived semantics"
    );
}
#[test]
fn styles_defaults_labels_and_paint_order_do_not_become_export_contracts() {
    let original = graph();
    let contract = original.exports[0].contract;
    let mut changed = original.clone();
    changed.definitions[0].name = "Beautiful card".into();
    changed.definitions[0].nodes[0].name = "Surface".into();
    changed.definitions[0].nodes[0].defaults[0].value = Value::Color([255, 136, 85, 255]);
    changed.definitions[0].nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .frame[2] = Expr::Literal(Value::Number(140.));
    changed.required_classes[0].properties[0].default = Value::Color([1, 2, 3, 255]);
    changed.required_classes[0].properties[0].label = "Color".into();
    changed.required_classes[0].properties[0].editor = PropertyEditor::ReadOnly;
    changed = seal_graph(changed).unwrap();
    assert_eq!(changed.exports[0].contract, contract);
    assert_ne!(
        encode_graph(&changed).unwrap(),
        encode_graph(&original).unwrap()
    );
    let mut incompatible = original.clone();
    incompatible.required_classes[0].properties[0]
        .policy
        .live_literal = false;
    incompatible = seal_graph(incompatible).unwrap();
    assert_ne!(incompatible.exports[0].contract, contract);
    assert!(
        required_classes_compatible(&incompatible.required_classes, &original.required_classes)
            .is_err()
    );
    assert!(
        required_classes_compatible(&changed.required_classes, &original.required_classes).is_ok()
    );

    let mut layered = original;
    for id in [2, 3] {
        let mut child = layered.definitions[0].nodes[0].clone();
        child.id = NodeId(id);
        child.owner = Some(NodeId(1));
        layered.definitions[0].nodes.push(child);
    }
    layered.definitions[0].nodes[0].visual_children = vec![NodeId(2), NodeId(3)];
    let mut layered = seal_graph(layered).unwrap();
    let contract = layered.exports[0].contract;
    layered.definitions[0].nodes[0].visual_children.reverse();
    assert_eq!(
        seal_graph(layered).unwrap().exports[0].contract,
        contract,
        "paint order changes presentation without changing object identity"
    );
}
#[test]
fn data_only_boundary_and_aggregate_package_bounds_fail_closed() {
    let original = graph();
    let tree = pack(&info(&[7; 32]), &original).unwrap();
    for path in [
        "Code/component.wasm",
        "Resources/image.png",
        "Dependencies/transitive.otlib/Info.flex",
        "UI/main.flex",
        "Notices/../escape",
    ] {
        let mut invalid = tree.clone();
        invalid.insert(path.into(), vec![1]);
        reseal(&mut invalid);
        assert!(inspect(&invalid).is_err(), "forbidden {path}");
    }
    let mut oversized = tree;
    oversized.insert("Notices/huge".into(), vec![0; MAX_PACKAGE_BYTES]);
    reseal(&mut oversized);
    assert!(inspect(&oversized).is_err());
    let mut bound = original.clone();
    bound.definitions[0].nodes[0].visual.as_mut().unwrap().frame[0] =
        Expr::State(Binding::Local(1));
    assert!(seal_graph(bound).is_err());
    let mut cycle = original;
    cycle.definitions[0].nodes[0].reference = Some(ComponentReference {
        definition: DefinitionId(10),
        assignments: vec![],
    });
    assert!(seal_graph(cycle).is_err());
}
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn signatures_bind_library_kind_publisher_exact_closure_and_owned_snapshot() {
    let (key, public) = keys();
    let graph = graph();
    let original = signed(&info(&public), &graph, &key, &public);
    let verified = verify(original.clone(), &public).unwrap();
    assert_eq!(verified.graph(), &graph);
    assert_eq!(verified.revision(), inspect(&original).unwrap().revision);
    assert!(verify(original.clone(), &[8; 32]).is_err());
    let mut unsigned = original.clone();
    unsigned.remove(SIGNATURE_PATH);
    assert!(verify(unsigned, &public).is_err());
    let mut substituted = original.clone();
    substituted.get_mut(GRAPH_PATH).unwrap()[0] ^= 1;
    assert!(verify(substituted.clone(), &public).is_err());
    assert_eq!(
        verified.file(GRAPH_PATH).unwrap(),
        encode_graph(&graph).unwrap()
    );
    let mut wrong_scope = info(&public);
    wrong_scope.library = info(&[8; 32]).library;
    let wrong_scope = signed(&wrong_scope, &graph, &key, &public);
    assert!(verify(wrong_scope, &public).is_err());
    let mut cross = pack(&info(&public), &graph).unwrap();
    let digest = inspect(&cross).unwrap().digest;
    let signature = key.sign(&bundle::signing_payload(&digest, &public).unwrap());
    attach_signature(&mut cross, &public, signature.as_ref()).unwrap();
    assert!(
        verify(cross, &public).is_err(),
        "app signature domain never authenticates a library"
    );
}
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn nested_package_binding_keeps_unrelated_payloads_and_checks_independent_trust() {
    let (key, public) = keys();
    let source = app::ui(&[]);
    let seed = canvas::oib::seed(&source).unwrap();
    let src = seed.graph.as_ref().unwrap();
    let body = seal_graph(Graph {
        behaviors: vec![],
        format: FORMAT.into(),
        required_classes: src.classes.clone(),
        definitions: src
            .definitions
            .iter()
            .filter(|d| d.id == DefinitionId(12))
            .cloned()
            .collect(),
        exports: vec![Export {
            id: ExportId(1),
            name: "Slider".into(),
            definition: DefinitionId(12),
            contract: [0; 32],
        }],
    })
    .unwrap();
    let library = verify(signed(&info(&public), &body, &key, &public), &public).unwrap();
    let lock = LibraryLock {
        library: library.summary().info.library.clone(),
        revision: library.revision(),
        exports: vec![LockedExport {
            export: ExportId(1),
            contract: body.exports[0].contract,
        }],
    };
    let mut ui = source.clone();
    let Some(otrios_interfaces::canvas::Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!()
    };
    let graph = seed.graph.as_mut().unwrap();
    graph.definitions.retain(|d| d.id != DefinitionId(12));
    graph.imports = Some(Imports {
        format: graph::IMPORTS_FORMAT.into(),
        locks: vec![lock.clone()],
        bindings: vec![ImportBinding {
            id: 60,
            library: lock.library.clone(),
            export: ExportId(1),
            definitions: vec![DefinitionBinding {
                source: DefinitionId(12),
                target: DefinitionId(12),
            }],
        }],
    });
    seed.resolved_libraries = vec![ResolvedLibrary {
        library: lock.library.clone(),
        revision: lock.revision,
        graph: body.clone(),
        component: None,
    }];
    let bytes = canvas::encode_ui(&source).unwrap();
    let entry = CatalogEntry {
        app: app::manifest(),
        path: "live-ui".into(),
        ui_hash: hash(&bytes),
        private: false,
    };
    let mut base = bundle::pack(&entry, 1, &bytes, None).unwrap();
    for path in [
        "Resources/retained.bin",
        "Dependencies/opaque.bin",
        "Notices/license.txt",
    ] {
        base = bundle::with_payload(base, path, b"retained").unwrap();
    }
    let bound = bundle::with_oib_ui_and_libraries(
        base,
        &ui,
        std::slice::from_ref(&lock),
        std::slice::from_ref(&library),
        &public,
    )
    .unwrap();
    for path in [
        "Resources/retained.bin",
        "Dependencies/opaque.bin",
        "Notices/license.txt",
    ] {
        assert_eq!(bound[path], b"retained");
    }
    assert!(!bound.contains_key(SIGNATURE_PATH));
    assert_eq!(
        bundle::inspect(&bound).unwrap().info.libraries,
        vec![lock.clone()]
    );
    let mut signed_app = bound.clone();
    let app_signature = key.sign(
        &bundle::signing_payload(&bundle::inspect(&signed_app).unwrap().digest, &public).unwrap(),
    );
    bundle::attach_signature(&mut signed_app, &public, app_signature.as_ref()).unwrap();
    bundle::verify(signed_app, &public).unwrap();

    let mut next_body = body.clone();
    next_body.definitions[0].nodes[0].defaults[0].value = Value::Color([255, 136, 85, 255]);
    let next_body = seal_graph(next_body).unwrap();
    assert_eq!(next_body.exports[0].contract, body.exports[0].contract);
    let mut next_info = info(&public);
    next_info.version = "1.1".into();
    let next_library = verify(signed(&next_info, &next_body, &key, &public), &public).unwrap();
    let mut next_lock = lock.clone();
    next_lock.revision = next_library.revision();
    let mut next_ui = ui.clone();
    let Some(otrios_interfaces::canvas::Presentation::Oib(seed)) = &mut next_ui.presentation else {
        panic!()
    };
    seed.graph.as_mut().unwrap().imports.as_mut().unwrap().locks = vec![next_lock.clone()];
    seed.resolved_libraries[0].revision = next_lock.revision;
    seed.resolved_libraries[0].graph = next_body;
    let replacement = bundle::with_oib_ui_and_libraries(
        bound.clone(),
        &next_ui,
        std::slice::from_ref(&next_lock),
        std::slice::from_ref(&next_library),
        &public,
    )
    .unwrap();
    assert!(
        !replacement
            .keys()
            .any(|path| path.starts_with(&bundle::library_root(&lock.revision)))
    );
    assert!(
        replacement
            .keys()
            .any(|path| path.starts_with(&bundle::library_root(&next_lock.revision)))
    );
    assert_eq!(replacement["Dependencies/opaque.bin"], b"retained");
    assert_eq!(replacement["Resources/retained.bin"], b"retained");
    assert_eq!(
        bundle::with_oib_ui_and_libraries(
            replacement.clone(),
            &next_ui,
            std::slice::from_ref(&next_lock),
            &[],
            &public,
        )
        .unwrap(),
        replacement,
        "unchanged verified libraries survive an authoring rebuild"
    );
    let mut orphan = replacement;
    for (path, bytes) in library.files() {
        orphan.insert(
            format!("{}{path}", bundle::library_root(&lock.revision)),
            bytes.to_vec(),
        );
    }
    reseal(&mut orphan);
    assert!(
        bundle::inspect(&orphan).is_err(),
        "an old unpinned library root cannot linger as a hidden dependency"
    );

    let mut tampered = bound.clone();
    let path = format!("{}{}", bundle::library_root(&lock.revision), SIGNATURE_PATH);
    let mut receipt: BundleSignature = bundle::metadata(&tampered[&path]).unwrap();
    receipt.signature[0] ^= 1;
    tampered.insert(path, bundle::encode(&receipt).unwrap());
    reseal(&mut tampered);
    let signature = key.sign(
        &bundle::signing_payload(&bundle::inspect(&tampered).unwrap().digest, &public).unwrap(),
    );
    bundle::attach_signature(&mut tampered, &public, signature.as_ref()).unwrap();
    assert!(
        bundle::verify(tampered, &public).is_err(),
        "valid outer app signature cannot replace independent library trust"
    );
    let Some(otrios_interfaces::canvas::Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!()
    };
    seed.resolved_libraries[0].graph.definitions[0].nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .frame[0] = Expr::Literal(Value::Number(10.));
    assert!(
        bundle::with_oib_ui_and_libraries(bound, &ui, std::slice::from_ref(&lock), &[], &public)
            .is_err(),
        "transported body must equal exact signed bytes"
    );
}

#[test]
fn auto_layout_requires_explicit_library_feature() {
    let mut graph = graph();
    graph.definitions[0].nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .layout = Some(otrios_interfaces::layout::NodeLayout::default());
    let graph = seal_graph(graph).unwrap();
    let mut info = info(&[7; 32]);
    assert!(
        pack(&info, &graph)
            .unwrap_err()
            .contains("Missing required auto layout feature")
    );
    info.required_features
        .push(bundle::AUTO_LAYOUT_FEATURE.into());
    info.required_features.sort();
    assert!(inspect(&pack(&info, &graph).unwrap()).is_ok());
    assert!(pack(&info, &self::graph()).is_ok());
}

#[test]
fn named_endpoint_signature_and_cursor_are_part_of_library_admission() {
    let original = graph();
    let mut candidate = original.clone();
    let mut descriptor = candidate.required_classes[0].properties[0].clone();
    descriptor.id = PropertyId(2);
    descriptor.name = "background".into();
    candidate.definitions[0].properties.push(descriptor);
    candidate.definitions[0].next_property = 3;
    let sealed = seal_graph(candidate).unwrap();
    assert_ne!(
        sealed.exports[0].contract, original.exports[0].contract,
        "adding a declared endpoint changes the export contract"
    );
    assert_eq!(
        behavior_property(
            &sealed,
            DefinitionId(10),
            &PropertyPath {
                references: vec![],
                node: NodeId(1),
                property: PropertyId(2)
            }
        )
        .unwrap()
        .name,
        "background"
    );
    let mut body = sealed.clone();
    body.definitions[0].properties[0].default = Value::Color([1, 2, 3, 255]);
    body.definitions[0].properties[0].name = "surface".into();
    assert_eq!(
        seal_graph(body).unwrap().exports[0].contract,
        sealed.exports[0].contract,
        "names and valid defaults remain body data, consistent with class contracts"
    );
    let mut changed = sealed.clone();
    changed.definitions[0].properties[0].value_type = ValueType::Text;
    changed.definitions[0].properties[0].default = Value::Text("caption".into());
    changed.definitions[0].properties[0].editor = PropertyEditor::TextSuggestions;
    assert_ne!(
        seal_graph(changed).unwrap().exports[0].contract,
        sealed.exports[0].contract
    );
    for cursor in [0, 2, 4] {
        let mut invalid = sealed.clone();
        invalid.definitions[0].next_property = cursor;
        assert!(seal_graph(invalid).is_err());
    }
    let mut invalid = sealed.clone();
    invalid.definitions[0].properties[0].id = PropertyId(5);
    invalid.definitions[0].next_property = 6;
    assert!(
        seal_graph(invalid).is_err(),
        "sparse declarations are gated until hole-aware endpoint restoration exists"
    );
    let mut invalid = sealed.clone();
    invalid.definitions[0].properties[0].policy.app_binding = true;
    assert!(seal_graph(invalid).is_err());
    let mut invalid = sealed;
    invalid.definitions[0].properties[0].name = "Accent".into();
    assert!(seal_graph(invalid).is_err());
}

#[test]
fn an_exhausted_class_property_namespace_keeps_existing_library_admission() {
    let mut candidate = graph();
    candidate.required_classes[0].properties[0].id = PropertyId(u64::MAX);
    candidate.definitions[0].nodes[0].defaults[0].property = PropertyId(u64::MAX);
    if let graph::ParameterSource::Property(path) = &mut candidate.definitions[0].nodes[0]
        .visual
        .as_mut()
        .unwrap()
        .parameters[0]
        .source
    {
        path.property = PropertyId(u64::MAX);
    }
    assert!(
        seal_graph(candidate).is_ok(),
        "an empty declaration table requires no successor allocation"
    );
}
