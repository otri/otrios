// SPDX-License-Identifier: Apache-2.0
//! Standalone codec checks using existing locked workspace serializers, without
//! adding a dev dependency to the public-contract crate. See the contract note.
use otrios_interfaces::oib::designer::*;

fn path() -> PropertyPath {
    PropertyPath {
        references: vec![NodeId(7), NodeId(9)],
        node: NodeId(3),
        property: PropertyId(1),
    }
}

#[test]
fn address_and_key_reference_bytes() {
    let address = PropertyAddress::Occurrence {
        instance: InstanceId(20),
        path: path(),
    };
    let key = RootCellKey {
        path: path(),
        key: 5,
        initial_cell: 200,
    };
    let bytes = flexbuffers::to_vec(&address).unwrap();
    let key_bytes = flexbuffers::to_vec(&key).unwrap();
    assert_eq!(
        hex(&bytes),
        "4f6363757272656e636500696e7374616e63650070617468007265666572656e6365730007096e6f64650070726f706572747900030f0b1e03010303011908084402372f020102140d0824014c0101010924022401"
    );
    assert_eq!(
        hex(&key_bytes),
        "70617468007265666572656e6365730007096e6f64650070726f706572747900030f0b1e0301030301190808446b657900696e697469616c5f63656c6c00030e1341030103c80520080824062401"
    );
    assert_eq!(
        flexbuffers::from_slice::<PropertyAddress>(&bytes).unwrap(),
        address
    );
    assert_eq!(
        flexbuffers::from_slice::<RootCellKey>(&key_bytes).unwrap(),
        key
    );
}

#[test]
fn reordered_rows_keep_explicit_keys_and_canonical_bytes() {
    let first = RootCellKey {
        path: path(),
        key: 5,
        initial_cell: 200,
    };
    let second = RootCellKey {
        path: PropertyPath {
            references: vec![NodeId(8), NodeId(9)],
            ..path()
        },
        key: 2,
        initial_cell: 201,
    };
    let mut authored = vec![first.clone(), second.clone()];
    let mut reordered = vec![second, first];
    // Canonical row ordering does not allocate or renumber keys.
    authored.sort_by_key(|row| row.key);
    reordered.sort_by_key(|row| row.key);
    assert_eq!(authored[0].key, 2);
    assert_eq!(authored[1].key, 5);
    assert_eq!(
        flexbuffers::to_vec(&authored).unwrap(),
        flexbuffers::to_vec(&reordered).unwrap()
    );
}

#[test]
fn option_omission_and_unknown_field_rejection() {
    let mapping = ObjectMapping {
        address: ObjectAddress {
            instance: InstanceId(20),
            references: vec![],
            node: NodeId(3),
        },
        layout_node: None,
    };
    let bytes = flexbuffers::to_vec(&mapping).unwrap();
    let reader = flexbuffers::Reader::get_root(bytes.as_slice()).unwrap();
    assert_eq!(reader.get_map().unwrap().len(), 1);
    assert_eq!(
        flexbuffers::from_slice::<ObjectMapping>(&bytes).unwrap(),
        mapping
    );
    let bad = std::collections::BTreeMap::from([
        ("live_literal", true),
        ("app_binding", false),
        ("script", true),
    ]);
    assert!(
        flexbuffers::from_slice::<PropertyPolicy>(&flexbuffers::to_vec(&bad).unwrap()).is_err()
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
