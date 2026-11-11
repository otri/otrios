// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::canvas::*;

fn fixture(code: Option<&[u8]>) -> BundleTree {
    let ui = Ui {
        format: "otrios-canvas-v2".into(),
        ui_ref: "app:sample/ui/main-v1".into(),
        title: "Sample".into(),
        layout: crate::test_support::empty_layout(),
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

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn explicit_publication_preserves_signed_bytes_and_rejects_untrusted_code() {
    use crate::transfers::{PublicationStore, archive};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let mut tree = fixture(None);
    let digest = inspect(&tree).unwrap().digest;
    let signature = key.sign(&signing_payload(&digest, &public).unwrap());
    attach_signature(&mut tree, &public, signature.as_ref()).unwrap();
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let root = std::env::temp_dir().join(format!("otrios-public-app-{}", hex(&nonce)));
    let store = PublicationStore::open(root.clone(), public).unwrap();
    assert!(PublicationStore::open(root.clone(), public).is_err());
    assert!(
        store.read("sample", &digest).unwrap().is_none(),
        "installed does not mean shared"
    );
    assert!(store.read("../sample", &digest).is_err());
    store.publish(tree.clone()).unwrap();
    store.publish(tree.clone()).unwrap();
    let published = store.read("sample", &digest).unwrap().unwrap();
    assert!(
        published.summary.info.private,
        "sharing does not rewrite signed classification"
    );
    assert_eq!(
        archive::decode(archive::Kind::App, &published.archive).unwrap(),
        tree
    );
    let mut unknown = tree.clone();
    unknown.remove(SIGNATURE_PATH);
    assert!(store.publish(unknown).is_err());
    store.withdraw("sample", &digest).unwrap();
    assert!(store.read("sample", &digest).unwrap().is_none());
    drop(store);
    let store = PublicationStore::open(root.clone(), public).unwrap();
    store.publish(tree).unwrap();
    drop(store);
    let other = PublicationStore::open(root.clone(), [0; 32]).unwrap();
    assert!(other.read("sample", &digest).is_err());
    drop(other);
    std::fs::remove_dir_all(root).unwrap();
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
        "0d6515e3f47c5a9d6334e27a64b27fb36a94882315ce5384710319d6eb20d639"
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
        "f757a51ed024c0a67d6e5b4cf4c213e9d31d68d400803d5b0fca762cf82c972b"
    );
    assert_eq!(
        hex(signature.as_ref()),
        "90c6a3af7374ea17273da56337161ba450d59b5e47b7a49fc25b852c3843b8cb60fa84666048f91af5bae71b95a5d68cfe11c974663439d7717c9ab3841f0602"
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

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
fn sign_metadata_fixture(tree: &mut BundleTree, key: &ring::signature::Ed25519KeyPair) {
    use ring::signature::KeyPair;
    // Deliberately bypass host admission to model bytes signed by a predecessor
    // publisher. Production signing and current bundle verification stay strict.
    reseal(tree);
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let mut digest = Sha256::new();
    digest.update(DIGEST_DOMAIN);
    digest.update(&tree[FILES_PATH]);
    let signature = key.sign(&signing_payload(&hex(&digest.finalize()), &public).unwrap());
    tree.insert(
        SIGNATURE_PATH.into(),
        encode(&BundleSignature {
            format: SIGNATURE_FORMAT.into(),
            key_sha256: hex(&hash(&public)),
            signature: signature.as_ref().to_vec(),
        })
        .unwrap(),
    );
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn authenticated_revision_does_not_admit_unsupported_features_or_unreadable_payloads() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let mut tree = fixture(None);
    change_info(&mut tree, |info| {
        info.revision = 17;
        info.required_features
            .extend(["otrios-oib-v1".into(), "otrios-oib-designer-v1".into()]);
    });
    tree.insert(
        UI_PATH.into(),
        b"obsolete UI encoding, intentionally unreadable".to_vec(),
    );
    tree.insert(SCHEMA_PATH.into(), b"obsolete schema encoding".to_vec());
    tree.insert("Resources/preserved.bin".into(), vec![1, 2, 3]);
    sign_metadata_fixture(&mut tree, &key);
    let original = tree.clone();
    let revision = authenticate_revision(&tree, &public).unwrap();
    assert_eq!(revision.app_id(), "sample");
    assert_eq!(revision.revision(), 17);
    assert_eq!(revision.digest(), inspect_envelope(&tree).unwrap().digest);
    assert!(
        verify(tree.clone(), &public)
            .unwrap_err()
            .contains("Unsupported required bundle feature")
    );
    assert!(inspect(&tree).is_err());
    assert_eq!(
        tree, original,
        "metadata authentication never rewrites predecessor bytes"
    );

    // With supported feature metadata alone, unreadable payloads still cannot
    // become a VerifiedBundle. Revision authentication never decodes them.
    change_info(&mut tree, |info| {
        info.required_features
            .retain(|feature| !feature.starts_with("otrios-oib"))
    });
    sign_metadata_fixture(&mut tree, &key);
    assert!(authenticate_revision(&tree, &public).is_ok());
    assert!(verify(tree, &public).is_err());
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn authenticated_revision_rejects_unsigned_wrong_key_and_tampered_closures() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let mut original = fixture(None);
    sign_metadata_fixture(&mut original, &key);
    assert!(authenticate_revision(&original, &public).is_ok());
    assert!(authenticate_revision(&original, &[8; 32]).is_err());
    let mut unsigned = original.clone();
    unsigned.remove(SIGNATURE_PATH);
    assert!(authenticate_revision(&unsigned, &public).is_err());
    for path in [UI_PATH, INFO_PATH, FILES_PATH, SIGNATURE_PATH] {
        let mut tampered = original.clone();
        tampered.get_mut(path).unwrap()[0] ^= 1;
        assert!(
            authenticate_revision(&tampered, &public).is_err(),
            "tampered {path}"
        );
    }
    let mut rehashed = original;
    rehashed.get_mut(UI_PATH).unwrap().push(1);
    reseal(&mut rehashed);
    assert!(
        authenticate_revision(&rehashed, &public).is_err(),
        "a new hash does not authenticate substituted content"
    );
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn authenticated_revision_preserves_optional_info_but_bounds_package_metadata() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    #[derive(Serialize)]
    struct Extended<'a> {
        #[serde(flatten)]
        info: &'a BundleInfo,
        display_hint: &'a str,
    }
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let original = fixture(None);
    let mut extended = original.clone();
    let info = decode_info(&extended[INFO_PATH]).unwrap();
    let mut bytes = INFO_HEADER.to_vec();
    bytes.extend(
        encode(&Extended {
            info: &info,
            display_hint: "optional predecessor extension",
        })
        .unwrap(),
    );
    extended.insert(INFO_PATH.into(), bytes);
    sign_metadata_fixture(&mut extended, &key);
    assert!(authenticate_revision(&extended, &public).is_ok());
    assert!(verify(extended, &public).is_ok());
    for feature in ["".to_string(), "x".repeat(129), "bad\0feature".into()] {
        let mut invalid = original.clone();
        change_info(&mut invalid, |info| info.required_features.push(feature));
        sign_metadata_fixture(&mut invalid, &key);
        assert!(authenticate_revision(&invalid, &public).is_err());
    }
    let mut missing_kind = original.clone();
    change_info(&mut missing_kind, |info| {
        info.required_features
            .retain(|feature| feature != PACKAGE_FEATURE)
    });
    sign_metadata_fixture(&mut missing_kind, &key);
    assert!(authenticate_revision(&missing_kind, &public).is_err());
    let mut wrong_major = original;
    wrong_major.get_mut(INFO_PATH).unwrap()[6] = 2;
    sign_metadata_fixture(&mut wrong_major, &key);
    assert!(authenticate_revision(&wrong_major, &public).is_err());
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn app_intake_requires_independent_trust_and_separates_reuse_from_host_installation() {
    use crate::{spaces, transfers::*};
    use otrios_interfaces::spaces::{Audience, Change, Request, VERSION};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::from_seed_unchecked(&[11; 32]).unwrap();
    let public = key.public_key().as_ref().try_into().unwrap();
    let original = fixture(None);
    let mut ui = canvas::decode_ui(&original[UI_PATH]).unwrap();
    ui.ui_ref = "app:sample/ui".into();
    let mut app = inspect(&original).unwrap().info.app;
    app.ui_ref = ui.ui_ref.clone();
    let ui = canvas::encode_ui(&ui).unwrap();
    let entry = CatalogEntry {
        app,
        path: "sample".into(),
        private: true,
        ui_hash: hash(&ui),
    };
    let sign = |revision| {
        let mut tree = pack(&entry, revision, &ui, None).unwrap();
        let signature =
            key.sign(&signing_payload(&inspect(&tree).unwrap().digest, &public).unwrap());
        attach_signature(&mut tree, &public, signature.as_ref()).unwrap();
        tree
    };
    let first = sign(1);
    let previous = verify(first.clone(), &public).unwrap();
    let bytes = archive::encode(archive::Kind::App, &first).unwrap();
    let source = AppSource::parse(&format!(
        "https://apps.example/apps/sample/{}",
        previous.summary().digest
    ))
    .unwrap();
    assert!(AppCandidate::qualify(&bytes, &[0; 32], Some(&source)).is_err());
    let wrong_source = AppSource::parse(&format!(
        "https://apps.example/apps/sample/{}",
        "0".repeat(64)
    ))
    .unwrap();
    assert!(AppCandidate::qualify(&bytes, &public, Some(&wrong_source)).is_err());
    let registry = spaces::apply(
        &spaces::empty(),
        "alice",
        &Request {
            version: VERSION,
            expected_generation: 0,
            change: Change::Create {
                name: "Import destination".into(),
            },
        },
        None,
    )
    .unwrap();
    let before = registry.clone();
    let target = AppImportTarget {
        space: registry.spaces[0].id.clone(),
        generation: registry.generation,
        catalog: vec![5; 32],
        release: "a".repeat(64),
        audience: Audience::Closed,
    };
    let candidate = || AppCandidate::qualify(&bytes, &public, Some(&source)).unwrap();
    assert!(
        candidate()
            .admit(
                target.clone(),
                &registry,
                "alice",
                &HostPolicy::default(),
                None,
                100
            )
            .is_err()
    );
    let stage = candidate()
        .admit(
            target.clone(),
            &registry,
            "alice",
            &HostPolicy::default(),
            Some(&previous),
            100,
        )
        .unwrap();
    assert_eq!(
        stage.review().installed.as_deref(),
        Some(previous.summary().digest.as_str())
    );
    assert_eq!(stage.review().affected_spaces, 0);
    assert_eq!(
        registry, before,
        "intake cannot mutate destination metadata"
    );
    assert!(stage.owned_by("alice", &stage.review().stage));
    assert!(!stage.owned_by("bob", &stage.review().stage));
    let auth = |actor: &str, host: &HostPolicy, generation, release: &str, now| {
        let mut current = registry.clone();
        current.generation = generation;
        stage.authorize(&current, actor, host, &target.catalog, release, now)
    };
    auth(
        "alice",
        &HostPolicy::default(),
        registry.generation,
        &target.release,
        101,
    )
    .unwrap();
    assert!(
        auth(
            "bob",
            &HostPolicy::default(),
            registry.generation,
            &target.release,
            101
        )
        .is_err()
    );
    assert!(
        auth(
            "alice",
            &HostPolicy::default(),
            registry.generation + 1,
            &target.release,
            101
        )
        .is_err()
    );
    assert!(
        auth(
            "alice",
            &HostPolicy::default(),
            registry.generation,
            &"b".repeat(64),
            101
        )
        .is_err()
    );
    assert!(
        auth(
            "alice",
            &HostPolicy::default(),
            registry.generation,
            &target.release,
            99
        )
        .is_err()
    );
    assert!(
        auth(
            "alice",
            &HostPolicy::default(),
            registry.generation,
            &target.release,
            stage.review().expires_unix_ms
        )
        .is_err()
    );
    let host = HostPolicy::new(vec!["alice".into()]).unwrap();
    let next_bytes = archive::encode(archive::Kind::App, &sign(2)).unwrap();
    let next = || AppCandidate::qualify(&next_bytes, &public, None).unwrap();
    assert!(
        next()
            .admit(
                target.clone(),
                &registry,
                "alice",
                &HostPolicy::default(),
                Some(&previous),
                100
            )
            .is_err()
    );
    assert!(
        next()
            .admit(
                target.clone(),
                &registry,
                "bob",
                &HostPolicy::new(vec!["bob".into()]).unwrap(),
                Some(&previous),
                100
            )
            .is_err()
    );
    let stage = next()
        .admit(
            target.clone(),
            &registry,
            "alice",
            &host,
            Some(&previous),
            100,
        )
        .unwrap();
    assert_eq!(stage.review().revision, 2);
    assert!(
        stage
            .authorize(
                &registry,
                "alice",
                &HostPolicy::default(),
                &target.catalog,
                &target.release,
                101
            )
            .is_err()
    );
    let mut public_space = registry.clone();
    public_space.spaces[0].audience = Audience::PublicView;
    let public_target = AppImportTarget {
        audience: Audience::PublicView,
        ..target.clone()
    };
    assert!(
        candidate()
            .admit(
                public_target,
                &public_space,
                "alice",
                &host,
                Some(&previous),
                100
            )
            .is_err()
    );
    let (review, verified) = stage.into_parts();
    assert_eq!(review.digest, verified.summary().digest);
    assert_eq!(verified.file(UI_PATH).unwrap(), ui);
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
#[test]
fn package_update_contract_retains_identity_policy_and_schema_without_content_pins() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let key = Ed25519KeyPair::from_seed_unchecked(&[17; 32]).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
    let verified = |mut tree: BundleTree| {
        sign_metadata_fixture(&mut tree, &key);
        verify(tree, &public).unwrap()
    };
    let original = fixture(None);
    let baseline = update_contract(&verified(original.clone())).unwrap();
    let mut presentation = original.clone();
    change_info(&mut presentation, |info| {
        info.revision += 1;
        info.app.title = "Renamed display".into();
    });
    let mut ui = canvas::decode_ui(&presentation[UI_PATH]).unwrap();
    ui.title = "New presentation".into();
    presentation.insert(UI_PATH.into(), canvas::encode_ui(&ui).unwrap());
    assert_eq!(
        update_contract(&verified(presentation)).unwrap(),
        baseline,
        "revision and display content do not change app identity or policy"
    );
    for kind in 0..4 {
        let mut changed = original.clone();
        match kind {
            0 => change_info(&mut changed, |info| info.private = false),
            1 => change_info(&mut changed, |info| {
                info.app.state_ref = "app:sample/state/other".into()
            }),
            2 => change_info(&mut changed, |info| {
                info.required_features.push("otrios-canvas-v3".into())
            }),
            _ => {
                let mut ui = canvas::decode_ui(&changed[UI_PATH]).unwrap();
                ui.state_schema.id = "sample-state-v2".into();
                changed.insert(
                    SCHEMA_PATH.into(),
                    canvas::encode_schema(&ui.state_schema).unwrap(),
                );
                changed.insert(UI_PATH.into(), canvas::encode_ui(&ui).unwrap());
            }
        }
        assert_ne!(
            update_contract(&verified(changed)).unwrap(),
            baseline,
            "identity, delivery, schema and required semantics are stable hot-update boundaries"
        );
    }
    let guest = |extra: &str| {
        wat::parse_str(format!(
            r#"(module
        (func (export "abi_version") (result i32) i32.const 1)
        (func (export "dispatch") (param i32 i32 i32 i32) (result i64) {extra} i64.const 0))"#
        ))
        .unwrap()
    };
    let first = update_contract(&verified(fixture(Some(&guest(""))))).unwrap();
    let next = update_contract(&verified(fixture(Some(&guest("nop"))))).unwrap();
    assert_eq!(
        first, next,
        "an existing app guest may change under its separate admitted ABI"
    );
    assert_ne!(
        first, baseline,
        "adding an app execution requirement changes the contract"
    );
}

#[path = "../../../../apps/live-ui/app.rs"]
mod auto_layout_test_app;

#[test]
fn source_only_auto_layout_requires_feature_in_bundle() {
    let mut ui = auto_layout_test_app::ui(&[]);
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        panic!("expected OIB source")
    };
    let graph = seed.graph.as_mut().unwrap();
    let visual = graph
        .definitions
        .iter_mut()
        .flat_map(|d| &mut d.nodes)
        .find_map(|n| n.visual.as_mut())
        .unwrap();
    visual.layout = Some(otrios_interfaces::layout::NodeLayout::default());
    assert!(uses_auto_layout(&ui).unwrap());
    let bytes = canvas::encode_ui(&ui).unwrap();
    let mut app = auto_layout_test_app::manifest();
    app.ui_ref = ui.ui_ref.clone();
    let entry = CatalogEntry {
        app,
        path: "live-ui".into(),
        private: true,
        ui_hash: hash(&bytes),
    };
    let mut tree = pack(&entry, 1, &bytes, None).unwrap();
    assert!(
        inspect(&tree)
            .unwrap()
            .info
            .required_features
            .iter()
            .any(|f| f == AUTO_LAYOUT_FEATURE)
    );
    change_info(&mut tree, |info| {
        info.required_features.retain(|f| f != AUTO_LAYOUT_FEATURE)
    });
    assert!(
        inspect(&tree)
            .unwrap_err()
            .contains("Missing required auto layout feature")
    );
    let mut normal = fixture(None);
    change_info(&mut normal, |info| {
        info.required_features.push(AUTO_LAYOUT_FEATURE.into());
        info.required_features.sort();
    });
    assert!(inspect(&normal).is_ok());
}

#[test]
fn blueprint_auto_layout_requires_feature_in_bundle() {
    let original = fixture(None);
    let mut ui = canvas::decode_ui(&original[UI_PATH]).unwrap();
    let layout = crate::dashboard::layout::Layout::from_bytes(&ui.layout).unwrap();
    let mut blueprint = layout.blueprint().clone();
    blueprint.schema = otrios_interfaces::layout::AUTO_LAYOUT_FORMAT.into();
    blueprint.templates[0].root.layout = Some(otrios_interfaces::layout::NodeLayout::default());
    ui.layout = crate::dashboard::layout::Layout::encode(&blueprint).unwrap();
    let bytes = canvas::encode_ui(&ui).unwrap();
    let mut info = decode_info(&original[INFO_PATH]).unwrap();
    info.required_features.push(AUTO_LAYOUT_FEATURE.into());
    info.required_features.sort();
    let mut tree = original;
    tree.insert(UI_PATH.into(), bytes);
    tree.insert(INFO_PATH.into(), encode_info(&info).unwrap());
    reseal(&mut tree);
    assert!(inspect(&tree).is_ok());
    change_info(&mut tree, |info| {
        info.required_features.retain(|f| f != AUTO_LAYOUT_FEATURE)
    });
    assert!(
        inspect(&tree)
            .unwrap_err()
            .contains("Missing required auto layout feature")
    );
}
