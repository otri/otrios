// SPDX-License-Identifier: LGPL-3.0-only
use super::*;

fn source() -> (LibraryInfo, Graph) {
    let source: serde_json::Value =
        serde_json::from_str(include_str!("../../../../libraries/oib-2d/source.json")).unwrap();
    let mut info: LibraryInfo = serde_json::from_value(source["info"].clone()).unwrap();
    let mut graph: Graph = serde_json::from_value(source["graph"].clone()).unwrap();
    info.format = BEHAVIOR_INFO_FORMAT.into();
    info.required_features
        .push(otrios_interfaces::oib::component_behavior::FEATURE.into());
    info.required_features.sort();
    info.required_features.dedup();
    graph.format = BEHAVIOR_FORMAT.into();
    let path = PropertyPath {
        references: vec![],
        node: NodeId(1),
        property: PropertyId(4),
    };
    graph.behaviors = vec![BehaviorDeclaration {
        id: 1,
        definition: DefinitionId(12),
        node: NodeId(1),
        properties: vec![BehaviorProperty {
            id: 1,
            source: path.clone(),
        }],
        local: vec![LocalProperty {
            id: 1,
            default: Value::Boolean(false),
        }],
        outlets: vec![BehaviorOutlet {
            id: 1,
            source: path,
            value_type: ValueType::Number,
        }],
        can_capture: true,
    }];
    (info, seal_graph(graph).unwrap())
}
fn guest(extra: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
      (memory (export "memory") 1 1)
      (func (export "abi_version") (result i32) i32.const 1)
      (func (export "input_buffer") (result i32) i32.const 0)
      (data (i32.const 2048) "OTCBOUT1\00\00\00\00\00\00\00\00")
      (func (export "dispatch") (param i32 i32) (result i64)
        {extra} i64.const 68719478784))"#
    ))
    .unwrap()
}
fn code(bytes: Vec<u8>) -> ComponentCode {
    ComponentCode {
        abi: 1,
        sha256: Sha256::digest(&bytes).into(),
        bytes,
    }
}
#[test]
fn signed_behavior_semantics_are_distinct_from_code_and_local_defaults() {
    let (info, graph) = source();
    let first = pack_with_behavior(&info, &graph, &guest("")).unwrap();
    let next = pack_with_behavior(&info, &graph, &guest("nop")).unwrap();
    let a = inspect(&first).unwrap();
    let b = inspect(&next).unwrap();
    assert_ne!(a.revision, b.revision);
    assert_ne!(
        a.component.as_ref().unwrap().sha256,
        b.component.as_ref().unwrap().sha256
    );
    assert_eq!(a.graph.exports, b.graph.exports);
    let mut data_only = graph.clone();
    data_only.format = FORMAT.into();
    data_only.behaviors.clear();
    assert_eq!(
        export_contract(&data_only, ExportId(3)).unwrap(),
        graph.exports[2].contract,
        "an unrelated export does not acquire a behavior contract"
    );
    let mut defaults = graph.clone();
    defaults.behaviors[0].local[0].default = Value::Boolean(true);
    assert_eq!(
        export_contract(&defaults, ExportId(1)).unwrap(),
        graph.exports[0].contract
    );
    defaults.behaviors[0].local[0].default = Value::Text("changed type".into());
    assert_ne!(
        export_contract(&defaults, ExportId(1)).unwrap(),
        graph.exports[0].contract
    );
    let mut permissions = graph.clone();
    permissions.behaviors[0].can_capture = false;
    assert_ne!(
        export_contract(&permissions, ExportId(1)).unwrap(),
        graph.exports[0].contract
    );
}
#[test]
fn behavior_code_requires_explicit_versioned_package_and_complete_closure() {
    let (info, graph) = source();
    assert!(pack(&info, &graph).is_err());
    let mut old_info = info.clone();
    old_info.format = INFO_FORMAT.into();
    old_info
        .required_features
        .retain(|f| f != otrios_interfaces::oib::component_behavior::FEATURE);
    assert!(pack_with_behavior(&old_info, &graph, &guest("")).is_err());
    let mut old_graph = graph.clone();
    old_graph.format = FORMAT.into();
    old_graph.behaviors.clear();
    let old_graph = seal_graph(old_graph).unwrap();
    assert!(pack_with_behavior(&old_info, &old_graph, &guest("")).is_err());
    let mut old_header = encode_info(&info).unwrap();
    old_header[..8].copy_from_slice(INFO_HEADER);
    assert!(decode_info(&old_header).is_err());
    let tree = pack_with_behavior(&info, &graph, &guest("")).unwrap();
    let mut missing = tree.clone();
    missing.remove(CODE_PATH);
    missing.insert(
        FILES_PATH.into(),
        bundle::encode(&bundle::file_manifest(&missing)).unwrap(),
    );
    assert!(inspect(&missing).is_err());
    let mut invalid = code(guest(""));
    invalid.sha256[0] ^= 1;
    assert!(validate_component(&graph, Some(&invalid)).is_err());
    invalid = code(guest(""));
    invalid.abi = 2;
    assert!(validate_component(&graph, Some(&invalid)).is_err());
    assert!(validate_component(&graph, Some(&code(vec![0; MAX_COMPONENT_BYTES + 1]))).is_err());
}
#[test]
fn behavior_declarations_and_guest_module_are_fully_admitted() {
    let (info, graph) = source();
    let mut invalid = graph.clone();
    invalid.behaviors[0].outlets[0].value_type = ValueType::Text;
    assert!(seal_graph(invalid).is_err());
    let mut invalid = graph.clone();
    invalid.behaviors[0].node = NodeId(999);
    assert!(seal_graph(invalid).is_err());
    let mut invalid = graph.clone();
    invalid.behaviors.push(invalid.behaviors[0].clone());
    assert!(seal_graph(invalid).is_err());
    let imports = wat::parse_str(r#"(module (import "env" "x" (func)))"#).unwrap();
    assert!(pack_with_behavior(&info, &graph, &imports).is_err());
}
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn independently_signed_guest_cannot_be_replaced_by_resealing_its_manifest() {
    use ring::signature::KeyPair;
    let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let (mut info, graph) = source();
    info.library = LibraryId(format!("{}:behavior", hex(&hash(&public))));
    let mut tree = pack_with_behavior(&info, &graph, &guest("")).unwrap();
    let receipt = key.sign(&signing_payload(&inspect(&tree).unwrap().digest, &public).unwrap());
    attach_signature(&mut tree, &public, receipt.as_ref()).unwrap();
    let verified = verify(tree.clone(), &public).unwrap();
    assert_eq!(verified.component().unwrap().bytes, guest(""));
    tree.insert(CODE_PATH.into(), guest("nop"));
    tree.insert(
        FILES_PATH.into(),
        bundle::encode(&bundle::file_manifest(&tree)).unwrap(),
    );
    assert!(inspect(&tree).is_ok());
    assert!(verify(tree, &public).is_err());
}
