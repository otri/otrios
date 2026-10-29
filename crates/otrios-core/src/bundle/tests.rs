// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::canvas::*;

fn fixture(code: Option<&[u8]>) -> BundleTree {
    let ui = Ui {
        format: "otrios-canvas-v2".into(),
        ui_ref: "app:sample/ui/main-v1".into(),
        title: "Sample".into(),
        layout: vec![],
        state_schema: StateSchema {
            id: "sample-state-v1".into(),
            fields: vec![],
        },
        selectors: vec![],
        handlers: vec![],
        navigation: vec![],
        behavior: code.map(|code| BehaviorRef {
            sha256: hash(code),
            abi: 1,
            writes: vec![],
        }),
        presentation: None,
    };
    let bytes = canvas::encode_ui(&ui).unwrap();
    pack(
        &CatalogEntry {
            app: App {
                id: "sample".into(),
                title: "Sample".into(),
                ui_ref: ui.ui_ref,
                state_ref: "app:sample/state/main-v1".into(),
                status: "prototype".into(),
                dependencies: vec![],
            },
            path: "sample".into(),
            private: true,
            ui_hash: hash(&bytes),
        },
        1,
        &bytes,
        code,
    )
    .unwrap()
}

fn reseal(tree: &mut BundleTree) {
    tree.insert(FILES_PATH.into(), encode(&file_manifest(tree)).unwrap());
}

fn change_info(tree: &mut BundleTree, change: impl FnOnce(&mut BundleInfo)) {
    let mut info = decode_info(&tree[INFO_PATH]).unwrap();
    change(&mut info);
    tree.insert(INFO_PATH.into(), encode_info(&info).unwrap());
    reseal(tree);
}

#[test]
fn reproducible_identity_and_exact_closure_with_private_classification() {
    let tree = fixture(None);
    assert_eq!(tree, fixture(None));
    let result = inspect(&tree).unwrap();
    assert_eq!(result.files, 4);
    assert!(result.info.private);
    assert!(!tree.keys().any(|path| path.starts_with("Contents/")));
    // Pinned interoperability vector for the complete fixed fixture, not merely a round trip.
    assert_eq!(
        result.digest,
        "66df587db07f61fd40d17ac50896eb76f63240a098e499b1f47937eea67b1bdb"
    );
    let mut changed = tree.clone();
    change_info(&mut changed, |info| info.revision += 1);
    assert_ne!(inspect(&changed).unwrap().digest, result.digest);
    for path in tree.keys().filter(|path| path.as_str() != FILES_PATH) {
        let mut bad = tree.clone();
        bad.get_mut(path).unwrap()[0] ^= 1;
        assert!(inspect(&bad).is_err(), "tamper: {path}");
        let mut missing = tree.clone();
        missing.remove(path);
        assert!(inspect(&missing).is_err(), "missing: {path}");
    }
    let mut extra = tree;
    extra.insert("Resources/unlisted.bin".into(), vec![1]);
    assert!(inspect(&extra).is_err());
    reseal(&mut extra);
    assert!(inspect(&extra).is_ok());
}

#[test]
fn info_extensions_are_ignored_but_required_semantics_fail_closed() {
    #[derive(Serialize)]
    struct Extended<'a> {
        #[serde(flatten)]
        info: &'a BundleInfo,
        display_hint: &'a str,
    }
    let mut tree = fixture(None);
    let before = inspect(&tree).unwrap();
    let mut bytes = INFO_HEADER.to_vec();
    bytes.extend(
        encode(&Extended {
            info: &before.info,
            display_hint: "future optional metadata",
        })
        .unwrap(),
    );
    assert_eq!(decode_info(&bytes).unwrap(), before.info);
    tree.insert(INFO_PATH.into(), bytes);
    reseal(&mut tree);
    assert_ne!(inspect(&tree).unwrap().digest, before.digest);
    assert!(with_revision(tree.clone(), 2).is_err());
    change_info(&mut tree, |info| {
        info.required_features.push("future-sandbox-v2".into())
    });
    assert!(inspect(&tree).is_err());
    let mut tree = fixture(None);
    change_info(&mut tree, |info| {
        info.capabilities.push("filesystem.write".into())
    });
    assert!(inspect(&tree).is_err());
    change_info(&mut tree, |info| {
        info.capabilities.clear();
        info.required_features.clear();
    });
    assert!(inspect(&tree).is_err());
}

#[test]
fn headers_types_and_cross_file_references_are_checked() {
    let tree = fixture(None);
    let info = decode_info(&tree[INFO_PATH]).unwrap();
    for (field, value) in [
        ("revision", serde_json::json!("1")),
        ("private", serde_json::json!(1)),
        (
            "required_features",
            serde_json::json!("otrios-canvas-package-v1"),
        ),
        ("capabilities", serde_json::Value::Null),
        ("entry_ui", serde_json::json!(true)),
        ("behavior", serde_json::json!(17)),
    ] {
        let mut object = serde_json::to_value(&info).unwrap();
        object[field] = value;
        let mut bytes = INFO_HEADER.to_vec();
        bytes.extend(encode(&object).unwrap());
        assert!(decode_info(&bytes).is_err(), "coerced field: {field}");
    }
    for index in 0..INFO_HEADER.len() {
        let mut bytes = tree[INFO_PATH].clone();
        bytes[index] ^= 1;
        assert!(decode_info(&bytes).is_err());
    }
    let mut bad = tree.clone();
    change_info(&mut bad, |info| info.app.ui_ref = "wrong-ui".into());
    assert!(inspect(&bad).is_err());
    let mut bad = tree.clone();
    change_info(&mut bad, |info| info.revision = 0);
    assert!(inspect(&bad).is_err());
    let mut bad = tree.clone();
    change_info(&mut bad, |info| info.entry_ui = "../UI/main.flex".into());
    assert!(inspect(&bad).is_err());
    let mut bad = tree;
    bad.insert(
        SCHEMA_PATH.into(),
        canvas::encode_schema(&StateSchema {
            id: "different".into(),
            fields: vec![],
        })
        .unwrap(),
    );
    reseal(&mut bad);
    assert!(inspect(&bad).is_err());
    let mut b = flexbuffers::Builder::default();
    {
        let mut map = b.start_map();
        map.push("revision", "1");
    }
    let mut bytes = INFO_HEADER.to_vec();
    bytes.extend_from_slice(b.view());
    assert!(decode_info(&bytes).is_err());
}

#[test]
fn duplicate_metadata_keys_and_wrong_catalog_hash_are_rejected() {
    let tree = fixture(None);
    let mut bytes = tree[INFO_PATH].clone();
    let at = bytes.windows(9).position(|v| v == b"revision\0").unwrap();
    bytes[at..at + 8].copy_from_slice(b"behavior");
    assert!(
        decode_info(&bytes)
            .unwrap_err()
            .contains("unique and sorted")
    );
    let info = decode_info(&tree[INFO_PATH]).unwrap();
    let entry = CatalogEntry {
        app: info.app,
        path: String::new(),
        private: false,
        ui_hash: vec![0; 32],
    };
    assert!(pack(&entry, 1, &tree[UI_PATH], None).is_err());
    assert_eq!(
        hex(&hash(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn wasm_bytes_are_bound_without_executing_the_guest() {
    // This has a start function that traps. Inspection must not instantiate it.
    let code = wat::parse_str("(module (func $start unreachable) (start $start))").unwrap();
    let mut tree = fixture(Some(&code));
    assert!(inspect(&tree).is_ok());
    tree.get_mut(CODE_PATH).unwrap().push(0);
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
    let mut tree = fixture(None);
    tree.insert(CODE_PATH.into(), code);
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
}

#[test]
fn unsafe_portability_paths_and_case_aliases_are_rejected() {
    for path in [
        "../x",
        "/x",
        "a//b",
        "a/./b",
        "a/../b",
        "a\\b",
        "a:x",
        "a\0b",
        "a/CON.txt",
        "a/file.",
        "a/é",
        "a/white space",
        "a/b/c/d/e/f/g/h/i",
    ] {
        assert!(!valid_path(path), "{path:?}");
    }
    for path in [
        "Resources/A.png",
        "Dependencies/lib-1/module.wasm",
        "Notices/LICENSE.txt",
    ] {
        assert!(valid_path(path), "{path:?}");
    }
    for path in [
        "Contents/x",
        "State/x",
        "Cache/x",
        "Integrity/publisher.sig",
    ] {
        let mut tree = fixture(None);
        tree.insert(path.into(), vec![]);
        reseal(&mut tree);
        assert!(inspect(&tree).is_err(), "{path}");
    }
    let mut tree = fixture(None);
    tree.insert("Resources/Foo/a".into(), vec![]);
    tree.insert("Resources/foo/b".into(), vec![]);
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
    let mut tree = fixture(None);
    tree.insert("Resources/a".into(), vec![]);
    tree.insert("Resources/a/b".into(), vec![]);
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
}

#[test]
fn manifest_order_duplicates_and_budgets_are_enforced() {
    let mut tree = fixture(None);
    let mut manifest = file_manifest(&tree);
    manifest.files.reverse();
    tree.insert(FILES_PATH.into(), encode(&manifest).unwrap());
    assert!(inspect(&tree).is_err());
    manifest.files = vec![manifest.files[0].clone(); 2];
    tree.insert(FILES_PATH.into(), encode(&manifest).unwrap());
    assert!(inspect(&tree).is_err());
    let mut tree = fixture(None);
    tree.insert("Resources/big".into(), vec![0; MAX_FILE_BYTES]);
    reseal(&mut tree);
    assert!(inspect(&tree).is_ok());
    tree.get_mut("Resources/big").unwrap().push(0);
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
    let mut tree = fixture(None);
    for index in tree.len()..MAX_FILES {
        tree.insert(format!("Resources/file-{index}"), vec![]);
    }
    reseal(&mut tree);
    assert!(inspect(&tree).is_ok());
    tree.insert("Resources/overflow".into(), vec![]);
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
    assert!(decode_info(&vec![0; MAX_METADATA_BYTES + 1]).is_err());
    let mut tree = fixture(None);
    for index in 0..32 {
        tree.insert(format!("Resources/large-{index}"), vec![0; MAX_FILE_BYTES]);
    }
    reseal(&mut tree);
    assert!(inspect(&tree).is_err());
    assert!(valid_path(&format!(
        "Resources/{}",
        "a".repeat(MAX_PATH_BYTES - 10)
    )));
    assert!(!valid_path(&format!(
        "Resources/{}",
        "a".repeat(MAX_PATH_BYTES - 9)
    )));
}

#[test]
fn truncated_and_seeded_mutated_metadata_never_panics() {
    let tree = fixture(None);
    let bytes = &tree[INFO_PATH];
    for end in 0..bytes.len() {
        let _ = decode_info(&bytes[..end]);
    }
    let mut seed = 0x0025_ab19u32;
    for _ in 0..2048 {
        let mut bytes = bytes.clone();
        for _ in 0..3 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let at = seed as usize % bytes.len();
            bytes[at] ^= (seed >> 24) as u8;
        }
        let _ = decode_info(&bytes);
    }
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn signed_snapshot_requires_external_trust_and_survives_source_mutation() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let mut tree = fixture(None);
    let digest = inspect(&tree).unwrap().digest;
    assert!(verify(tree.clone(), &public).is_err());
    let signature = key.sign(&signing_payload(&digest, &public).unwrap());
    // Independently generated with OpenSSL 3 from the fixed PKCS#8 seed fixture.
    assert_eq!(
        hex(&public),
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
    );
    assert_eq!(
        hex(&hash(&signing_payload(&digest, &public).unwrap())),
        "0996b55102ecccf252a59417edb7e3eb4bcf340e5e1170491df7b0a1111112dc"
    );
    assert_eq!(
        hex(signature.as_ref()),
        "2860dcb0eacf89cb91064e5670a1e4e0cbf9f0016e072f600f8e63fc4fa49a1ff2437980bada9b642d7e04bb7fe97246878dd2a73cca5521c0384f1f6e7c5f0d"
    );
    attach_signature(&mut tree, &public, signature.as_ref()).unwrap();
    assert_eq!(inspect(&tree).unwrap().digest, digest);
    let verified = verify(tree.clone(), &public).unwrap();
    assert_eq!(verified.summary().digest, digest);
    assert!(verify(tree.clone(), &[8; 32]).is_err());
    change_info(&mut tree, |info| info.revision += 1);
    assert!(verify(tree.clone(), &public).is_err());
    assert_eq!(verified.summary().info.revision, 1);
    assert!(verified.file(UI_PATH).is_ok());
    assert!(public_key_der(&public).is_err());
    assert!(attach_signature(&mut tree, &public, &[0; 63]).is_err());
}
