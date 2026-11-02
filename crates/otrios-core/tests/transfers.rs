// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
use otrios_core::{
    bundle::BundleTree,
    canvas,
    transfers::archive::{self, Kind},
};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/transfer-vectors.json")).unwrap()
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn vector(name: &str) -> Vec<u8> {
    unhex(vectors()[name]["hex"].as_str().unwrap())
}
fn expected() -> BundleTree {
    vectors()["files_hex"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(p, v)| (p.clone(), unhex(v.as_str().unwrap())))
        .collect()
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn put32(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn central(bytes: &[u8]) -> usize {
    u32_at(bytes, bytes.len() - 6)
}

#[test]
fn independent_stored_deflated_and_streamed_vectors_preserve_exact_files() {
    for (name, digest) in [
        (
            "stored",
            "7bdff53f24c656741bd4933c34d367d952b2eae38bcbaf850f4c1f10653708c4",
        ),
        (
            "deflated",
            "99b503632690831982ce2060785efc8b282716e30b018d80054467f0cf4f6a8e",
        ),
        (
            "descriptor",
            "0e2dce2ea1c811f0bbd280c2fd745d7e04b6262ab9fcb64ca4f6fb94a811ad66",
        ),
        (
            "descriptor_without_signature",
            "24c232af61f22e7a607ed066b678aae2264aaa73efbb0e1bfe9b36a24a476b90",
        ),
    ] {
        let bytes = vector(name);
        assert_eq!(canvas::hex(&canvas::hash(&bytes)), digest);
        assert_eq!(archive::decode(Kind::App, &bytes).unwrap(), expected());
        assert_eq!(archive::decode(Kind::Data, &bytes).unwrap(), expected());
    }
    let encoded = archive::encode(Kind::App, &expected()).unwrap();
    assert_eq!(encoded, archive::encode(Kind::App, &expected()).unwrap());
    assert_eq!(archive::decode(Kind::App, &encoded).unwrap(), expected());
}

#[test]
fn path_alias_traversal_links_and_special_files_are_rejected() {
    for name in ["case_alias", "traversal", "symlink"] {
        assert!(archive::decode(Kind::App, &vector(name)).is_err(), "{name}");
    }
    for path in [
        "../escape",
        "/root",
        "a\\b",
        "CON.txt",
        "a/../b",
        "a//b",
        "a.",
        "a/",
        "é",
    ] {
        assert!(
            archive::encode(Kind::App, &BundleTree::from([(path.into(), vec![])])).is_err(),
            "{path}"
        );
    }
    let mut bytes = vector("stored");
    let at = central(&bytes);
    put32(&mut bytes, at + 38, 0o020644 << 16);
    assert!(archive::decode(Kind::App, &bytes).is_err());
}

#[test]
fn incomplete_encrypted_unsupported_or_inconsistent_records_cannot_decode() {
    let good = vector("deflated");
    let at = central(&good);
    for end in 0..good.len() {
        assert!(archive::decode(Kind::App, &good[..end]).is_err());
    }
    for (offset, value) in [
        (at + 8, 1),
        (at + 10, 9),
        (at + 6, 45),
        (at + 34, 1),
        (at + 30, 1),
        (at + 32, 1),
    ] {
        let mut bad = good.clone();
        bad[offset..offset + 2].copy_from_slice(&u16::try_from(value).unwrap().to_le_bytes());
        assert!(archive::decode(Kind::App, &bad).is_err(), "offset {offset}");
    }
    let mut bad = good.clone();
    bad[30] ^= 1;
    assert!(
        archive::decode(Kind::App, &bad).is_err(),
        "local name differs from central name"
    );
    let mut bad = good.clone();
    let last = bad.len() - 22;
    bad[last + 4] = 1;
    assert!(archive::decode(Kind::App, &bad).is_err(), "multi-disk");
    let mut bad = good.clone();
    bad.push(0);
    assert!(archive::decode(Kind::App, &bad).is_err(), "trailing bytes");
}

#[test]
fn crc_payload_overlap_and_unlisted_gaps_fail_before_publication() {
    assert!(
        archive::decode(Kind::App, &vector("deflate_trailing_bytes")).is_err(),
        "a valid Deflate stream cannot hide extra compressed bytes"
    );
    let good = vector("stored");
    let at = central(&good);
    let mut bad = good.clone();
    let data = 30 + usize::from(u16_at(&bad, 26));
    bad[data] ^= 1;
    assert!(archive::decode(Kind::App, &bad).is_err(), "CRC");
    let mut bad = good.clone();
    let second = at + 46 + usize::from(u16_at(&bad, at + 28));
    put32(&mut bad, second + 42, 0);
    assert!(
        archive::decode(Kind::App, &bad).is_err(),
        "overlapping local records"
    );
    let mut bad = good.clone();
    bad.insert(at, 0);
    let end = bad.len() - 22;
    put32(&mut bad, end + 16, at + 1);
    assert!(
        archive::decode(Kind::App, &bad).is_err(),
        "unlisted gap before central directory"
    );
    let mut bad = vector("descriptor");
    let at = central(&bad);
    let data = 30 + usize::from(u16_at(&bad, 26));
    let tail = data + u32_at(&bad, at + 20);
    bad[tail + 4] ^= 1;
    assert!(archive::decode(Kind::App, &bad).is_err(), "descriptor CRC");
}

#[test]
fn hostile_size_claims_are_rejected_before_expanding_them() {
    let good = vector("deflated");
    let at = central(&good);
    let mut bad = good.clone();
    put32(&mut bad, at + 24, otrios_core::bundle::MAX_FILE_BYTES + 1);
    assert!(archive::decode(Kind::App, &bad).is_err());
    let mut bad = good;
    put32(
        &mut bad,
        at + 24,
        otrios_core::transfers::MAX_CHECKPOINT_BYTES + 1,
    );
    assert!(archive::decode(Kind::Data, &bad).is_err());
    let mut files = expected();
    files.insert(
        "State/checkpoint.bin".into(),
        vec![0; otrios_core::bundle::MAX_FILE_BYTES + 1],
    );
    assert!(archive::encode(Kind::App, &files).is_err());
    let data = archive::encode(Kind::Data, &files).unwrap();
    assert_eq!(archive::decode(Kind::Data, &data).unwrap(), files);
}

fn data_info() -> otrios_core::transfers::DataInfo {
    use otrios_core::transfers::*;
    DataInfo {
        format: DATA_FORMAT.into(),
        bundle_id: "notes".into(),
        schema_sha256: "a".repeat(64),
        codec: "otrios-canvas-portable-v1".into(),
        codec_version: 1,
        checkpoint: "State/checkpoint.bin".into(),
    }
}
// Malformed metadata must be rejected on its own merits, with an otherwise
// correct integrity closure rather than an unrelated stale digest.
fn refresh_manifest(tree: &mut BundleTree) {
    use otrios_core::transfers::{DATA_FILES_FORMAT, DataFile, DataFiles};
    let manifest = DataFiles {
        format: DATA_FILES_FORMAT.into(),
        files: tree
            .iter()
            .filter(|(p, _)| p.as_str() != "Integrity/files.flex")
            .map(|(path, bytes)| DataFile {
                path: path.clone(),
                bytes: bytes.len() as u64,
                sha256: canvas::hex(&canvas::hash(bytes)),
            })
            .collect(),
    };
    tree.insert(
        "Integrity/files.flex".into(),
        flexbuffers::to_vec(manifest).unwrap(),
    );
}
#[test]
fn data_container_preserves_owned_bytes_and_requires_complete_integrity() {
    use otrios_core::transfers::*;
    let asset = b"owned attachment".to_vec();
    let hash = canvas::hex(&canvas::hash(&asset));
    let assets = std::collections::BTreeMap::from([(hash.clone(), asset)]);
    // Container qualification alone does not claim these opaque bytes are a
    // portable checkpoint. The codec must independently qualify them later.
    let encoded = encode_data(data_info(), b"opaque checkpoint".to_vec(), assets.clone()).unwrap();
    let tree = archive::decode(Kind::Data, &encoded).unwrap();
    let checked = inspect_data(tree.clone()).unwrap();
    assert_eq!(checked.info(), &data_info());
    assert_eq!(checked.checkpoint(), b"opaque checkpoint");
    assert_eq!(checked.assets(), &assets);
    for path in ["Info.flex", "State/checkpoint.bin", "Integrity/files.flex"] {
        let mut bad = tree.clone();
        bad.get_mut(path).unwrap()[0] ^= 1;
        assert!(inspect_data(bad).is_err(), "{path}");
    }
    let mut bad = tree.clone();
    bad.remove(&format!("Assets/{hash}"));
    assert!(inspect_data(bad).is_err());
    let mut bad = tree;
    bad.insert("Code/behavior.wasm".into(), vec![0]);
    refresh_manifest(&mut bad);
    assert!(inspect_data(bad).is_err());
}

#[test]
fn data_metadata_rejects_unknown_codecs_fields_and_noncanonical_numbers() {
    use otrios_core::transfers::*;
    let mut info = data_info();
    info.codec = "unknown".into();
    assert!(encode_data(info, vec![1], Default::default()).is_err());
    assert!(encode_data(data_info(), vec![], Default::default()).is_err());
    let encoded = encode_data(data_info(), vec![1], Default::default()).unwrap();
    let tree = archive::decode(Kind::Data, &encoded).unwrap();
    for (field, value) in [
        ("codec_version", serde_json::json!(1.0)),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut bad = tree.clone();
        let mut info = serde_json::to_value(data_info()).unwrap();
        info[field] = value;
        let mut raw = DATA_INFO_HEADER.to_vec();
        raw.extend(flexbuffers::to_vec(info).unwrap());
        bad.insert("Info.flex".into(), raw);
        refresh_manifest(&mut bad);
        assert!(inspect_data(bad).is_err());
    }
}

#[test]
fn collection_requires_every_trusted_root_and_exact_owned_asset_closure() {
    use otrios_core::transfers::*;
    use otrios_interfaces::{
        canvas::{Field, StateSchema},
        layout::Value,
        spaces::RootSchema,
    };
    let schema = StateSchema {
        id: "portable-multi-root-v1".into(),
        fields: vec![Field {
            id: 1,
            name: "value".into(),
            writable: true,
            default: Value::Number(0.),
        }],
    };
    let policies = ["app:notes/state", "app:notes/detail/state"].map(|state_ref| RootPolicy {
        state_ref: state_ref.into(),
        schema: schema.clone(),
        chat: false,
    });
    let schema_hash = otrios_core::spaces::collection_schema(
        &policies
            .iter()
            .map(|p| RootSchema {
                state_ref: p.state_ref.clone(),
                schema: p.schema.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let roots = policies
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let mut doc = canvas::Document::new(p.schema.clone(), &p.state_ref).unwrap();
            doc.set(&[(1, 42 + index as u16)]).unwrap();
            OwnedRoot {
                state_ref: p.state_ref.clone(),
                canvas: doc.portable_checkpoint().unwrap(),
                chat: None,
            }
        })
        .collect::<Vec<_>>();
    let mut info = data_info();
    info.codec = COLLECTION_CODEC.into();
    info.schema_sha256 = schema_hash.clone();
    let inspect = |checkpoint, assets| {
        let bytes = encode_data(info.clone(), checkpoint, assets).unwrap();
        inspect_data(archive::decode(Kind::Data, &bytes).unwrap()).unwrap()
    };
    let bytes = encode_collection(&roots).unwrap();
    let imported = QualifiedCollection::qualify(
        inspect(bytes.clone(), Default::default()),
        "notes",
        &schema_hash,
        &policies,
    )
    .unwrap();
    let stage = std::env::temp_dir().join(format!(
        "otrios-portable-collection-{}-{}",
        std::process::id(),
        canvas::hex(&canvas::hash(&bytes))
    ));
    std::fs::create_dir(&stage).unwrap();
    let collection = "1".repeat(32);
    imported
        .write_stage(&stage, &policies[0].state_ref, &collection)
        .unwrap();
    for (index, p) in policies.iter().enumerate() {
        let file = if index == 0 {
            stage.join(format!("collection-{collection}.merkle"))
        } else {
            stage.join(format!(
                "collection-{collection}.data/doc-{}.merkle",
                canvas::hex(&canvas::hash(p.state_ref.as_bytes()))
            ))
        };
        let doc = canvas::Document::load(
            p.schema.clone(),
            &p.state_ref,
            &std::fs::read(file).unwrap(),
        )
        .unwrap();
        assert_eq!(doc.value(1).unwrap(), 42 + index as u16);
        assert_eq!(doc.portable_checkpoint().unwrap(), roots[index].canvas);
    }
    assert!(
        imported
            .write_stage(&stage, &policies[0].state_ref, &collection)
            .is_err()
    );
    std::fs::remove_dir_all(stage).unwrap();
    let incomplete = encode_collection(&roots[..1]).unwrap();
    assert!(
        QualifiedCollection::qualify(
            inspect(incomplete, Default::default()),
            "notes",
            &schema_hash,
            &policies
        )
        .is_err()
    );
    assert!(
        QualifiedCollection::qualify(
            inspect(bytes.clone(), Default::default()),
            "other",
            &schema_hash,
            &policies
        )
        .is_err()
    );
    let extra = b"unreferenced asset".to_vec();
    let assets = std::collections::BTreeMap::from([(canvas::hex(&canvas::hash(&extra)), extra)]);
    assert!(
        QualifiedCollection::qualify(
            inspect(bytes.clone(), assets),
            "notes",
            &schema_hash,
            &policies
        )
        .is_err()
    );
    let mut trailing = bytes;
    trailing.push(0);
    assert!(
        QualifiedCollection::qualify(
            inspect(trailing, Default::default()),
            "notes",
            &schema_hash,
            &policies
        )
        .is_err()
    );
}

#[test]
fn export_pin_freezes_every_confirmed_root_and_rejects_incomplete_asset_closure() {
    use otrios_core::transfers::{PinnedCollection, PinnedRoot, QualifiedCollection, RootPolicy};
    use otrios_interfaces::{
        canvas::{Field, StateSchema},
        layout::Value,
        spaces::RootSchema,
    };
    use std::collections::BTreeMap;
    let schema = StateSchema {
        id: "export-pin-v1".into(),
        fields: vec![Field {
            id: 1,
            name: "value".into(),
            writable: true,
            default: Value::Number(0.),
        }],
    };
    let policies = ["app:notes/state", "app:notes/other"].map(|state_ref| RootPolicy {
        state_ref: state_ref.into(),
        schema: schema.clone(),
        chat: false,
    });
    let schema_hash = otrios_core::spaces::collection_schema(
        &policies
            .iter()
            .map(|p| RootSchema {
                state_ref: p.state_ref.clone(),
                schema: p.schema.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut documents = policies
        .iter()
        .map(|p| canvas::Document::new(p.schema.clone(), &p.state_ref).unwrap())
        .collect::<Vec<_>>();
    for (index, doc) in documents.iter_mut().enumerate() {
        doc.set(&[(1, 40 + index as u16)]).unwrap();
        doc.open_session(&[5; 16], 0).unwrap();
    }
    let roots = || {
        documents
            .iter()
            .cloned()
            .map(|document| PinnedRoot {
                document,
                chat: None,
            })
            .collect::<Vec<_>>()
    };
    assert!(
        PinnedCollection::pin("notes", &schema_hash, &policies, vec![], BTreeMap::new()).is_err()
    );
    assert!(
        PinnedCollection::pin(
            "notes",
            &schema_hash,
            &policies,
            roots(),
            BTreeMap::from([("a".repeat(64), vec![1])])
        )
        .is_err()
    );
    let pin =
        PinnedCollection::pin("notes", &schema_hash, &policies, roots(), BTreeMap::new()).unwrap();
    let confirmed = documents
        .iter()
        .map(|d| d.portable_checkpoint().unwrap())
        .collect::<Vec<_>>();
    for doc in &mut documents {
        doc.set(&[(1, 99)]).unwrap();
    }
    let archive = pin.encode().unwrap();
    let data = otrios_core::transfers::inspect_data(archive::decode(Kind::Data, &archive).unwrap())
        .unwrap();
    let mut expected_info = data_info();
    expected_info.codec = otrios_core::transfers::COLLECTION_CODEC.into();
    expected_info.schema_sha256 = schema_hash.clone();
    assert_eq!(data.info(), &expected_info);
    let imported = QualifiedCollection::qualify(data, "notes", &schema_hash, &policies).unwrap();
    let stage = std::env::temp_dir().join(format!(
        "otrios-export-pin-{}",
        canvas::hex(&canvas::hash(&archive))
    ));
    std::fs::create_dir(&stage).unwrap();
    let collection = "3".repeat(32);
    imported
        .write_stage(&stage, &policies[0].state_ref, &collection)
        .unwrap();
    for (index, policy) in policies.iter().enumerate() {
        let file = if index == 0 {
            stage.join(format!("collection-{collection}.merkle"))
        } else {
            stage.join(format!(
                "collection-{collection}.data/doc-{}.merkle",
                canvas::hex(&canvas::hash(policy.state_ref.as_bytes()))
            ))
        };
        let restored = canvas::Document::load(
            policy.schema.clone(),
            &policy.state_ref,
            &std::fs::read(file).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.value(1).unwrap(), 40 + index as u16);
        assert_eq!(restored.portable_checkpoint().unwrap(), confirmed[index]);
    }
    std::fs::remove_dir_all(stage).unwrap();
}

#[test]
fn export_delivery_rechecks_admin_and_exact_live_or_sealed_dataset() {
    use otrios_core::{
        spaces,
        transfers::{DatasetRef, authorize_dataset},
    };
    use otrios_interfaces::spaces::*;
    let mut registry = spaces::empty();
    let policy = AppPolicy {
        app: "notes".into(),
        schema: "b".repeat(64),
        closed_only: false,
        editable: true,
        comments: false,
    };
    let apply = |registry: &Registry, actor, change| {
        spaces::apply(
            registry,
            actor,
            &Request {
                version: VERSION,
                expected_generation: registry.generation,
                change,
            },
            Some(&policy),
        )
        .unwrap()
    };
    registry = apply(
        &registry,
        "alice",
        Change::Create {
            name: "Private".into(),
        },
    );
    let space = registry.spaces[0].id.clone();
    registry = apply(
        &registry,
        "alice",
        Change::Attach {
            space: space.clone(),
            app: "notes".into(),
        },
    );
    let selected = DatasetRef {
        space: space.clone(),
        app: "notes".into(),
        collection: registry.spaces[0].apps[0].collection.clone(),
        schema: policy.schema.clone(),
    };
    authorize_dataset(&registry, "alice", &selected, &policy).unwrap();
    assert!(authorize_dataset(&registry, "bob", &selected, &policy).is_err());
    registry = apply(
        &registry,
        "alice",
        Change::Remove {
            space: space.clone(),
            app: "notes".into(),
        },
    );
    authorize_dataset(&registry, "alice", &selected, &policy).unwrap();
    let sealed = DatasetRef {
        collection: "c".repeat(32),
        ..selected.clone()
    };
    registry.retained.push(RetainedDataset {
        space: space.clone(),
        policy: policy.clone(),
        collection: sealed.collection.clone(),
        label: "Notes before replacement".into(),
        origin: DatasetOrigin::BeforeReplacement,
        generation: registry.generation,
    });
    spaces::validate(&registry).unwrap();
    authorize_dataset(&registry, "alice", &sealed, &policy).unwrap();
    let wrong_app = DatasetRef {
        app: "another".into(),
        ..sealed.clone()
    };
    assert!(authorize_dataset(&registry, "alice", &wrong_app, &policy).is_err());
    registry = apply(
        &registry,
        "alice",
        Change::Membership {
            space: space.clone(),
            identity: "bob".into(),
            role: Some(Role::Admin),
        },
    );
    registry = apply(
        &registry,
        "bob",
        Change::Membership {
            space: space.clone(),
            identity: "alice".into(),
            role: Some(Role::Member),
        },
    );
    assert!(authorize_dataset(&registry, "alice", &selected, &policy).is_err());
    assert!(authorize_dataset(&registry, "alice", &sealed, &policy).is_err());
    authorize_dataset(&registry, "bob", &sealed, &policy).unwrap();
    registry = apply(
        &registry,
        "bob",
        Change::DeleteData {
            space,
            app: "notes".into(),
            collection: selected.collection.clone(),
            remove: true,
        },
    );
    assert!(authorize_dataset(&registry, "bob", &selected, &policy).is_err());
    authorize_dataset(&registry, "bob", &sealed, &policy).unwrap();
}

fn staged_fixture() -> (
    otrios_interfaces::spaces::Registry,
    otrios_core::transfers::ReplacementTarget,
    Vec<u8>,
) {
    use otrios_core::{spaces, transfers::*};
    use otrios_interfaces::{canvas::StateSchema, spaces::*};
    let roots = vec![RootPolicy {
        state_ref: "app:notes/state".into(),
        schema: StateSchema {
            id: "stage-empty-v1".into(),
            fields: vec![],
        },
        chat: false,
    }];
    let schema = spaces::collection_schema(&[RootSchema {
        state_ref: roots[0].state_ref.clone(),
        schema: roots[0].schema.clone(),
    }])
    .unwrap();
    let policy = AppPolicy {
        app: "notes".into(),
        schema: schema.clone(),
        closed_only: false,
        editable: true,
        comments: false,
    };
    let mut registry = spaces::apply(
        &spaces::empty(),
        "alice",
        &Request {
            version: VERSION,
            expected_generation: 0,
            change: Change::Create {
                name: "Staged data".into(),
            },
        },
        None,
    )
    .unwrap();
    let space = registry.spaces[0].id.clone();
    registry = spaces::apply(
        &registry,
        "alice",
        &Request {
            version: VERSION,
            expected_generation: registry.generation,
            change: Change::Attach {
                space: space.clone(),
                app: "notes".into(),
            },
        },
        Some(&policy),
    )
    .unwrap();
    let dataset = DatasetRef {
        space,
        app: "notes".into(),
        collection: registry.spaces[0].apps[0].collection.clone(),
        schema,
    };
    let document = canvas::Document::new(roots[0].schema.clone(), &roots[0].state_ref).unwrap();
    let bytes = PinnedCollection::pin(
        "notes",
        &policy.schema,
        &roots,
        vec![PinnedRoot {
            document,
            chat: None,
        }],
        Default::default(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let target = ReplacementTarget {
        dataset,
        policy,
        primary: roots[0].state_ref.clone(),
        roots,
        catalog: vec![4; 32],
        generation: registry.generation,
        audience: Audience::Closed,
    };
    (registry, target, bytes)
}

#[test]
fn data_review_qualifies_before_admission_and_expires_without_changing_the_binding() {
    use otrios_core::{
        spaces::{self, Error},
        transfers::DataCandidate,
    };
    let (registry, target, bytes) = staged_fixture();
    let original = registry.clone();
    assert!(DataCandidate::qualify(target.clone(), &bytes[..bytes.len() - 1]).is_err());
    assert!(
        DataCandidate::qualify(target.clone(), &bytes)
            .unwrap()
            .admit(&registry, "bob", &target.catalog, 100)
            .is_err()
    );
    let staged = DataCandidate::qualify(target.clone(), &bytes)
        .unwrap()
        .admit(&registry, "alice", &target.catalog, 100)
        .unwrap();
    let review = staged.review();
    assert!(spaces::valid_id(&review.stage));
    assert_ne!(review.stage, review.replacement.replacement);
    assert_ne!(review.replacement.replacement, target.dataset.collection);
    assert_eq!(review.archive_sha256, canvas::hex(&canvas::hash(&bytes)));
    assert!(staged.owned_by("alice", &review.stage));
    assert!(!staged.owned_by("bob", &review.stage));
    assert_eq!(
        registry, original,
        "staging does not publish or detach any dataset"
    );
    staged
        .authorize(&registry, "alice", &target.catalog, 100)
        .unwrap();
    assert_eq!(
        staged.authorize(&registry, "bob", &target.catalog, 100),
        Err(Error::Denied)
    );
    assert_eq!(
        staged.authorize(&registry, "alice", &[5; 32], 100),
        Err(Error::Conflict)
    );
    assert_eq!(
        staged.authorize(&registry, "alice", &target.catalog, 99),
        Err(Error::Unavailable)
    );
    assert_eq!(
        staged.authorize(&registry, "alice", &target.catalog, review.expires_unix_ms),
        Err(Error::Unavailable)
    );
    let mut changed = registry.clone();
    changed.generation += 1;
    assert_eq!(
        staged.authorize(&changed, "alice", &target.catalog, 100),
        Err(Error::Conflict)
    );
    assert!(
        DataCandidate::qualify(target.clone(), &bytes)
            .unwrap()
            .admit(&changed, "alice", &target.catalog, 100)
            .is_err()
    );
}

#[test]
fn code_publication_requires_both_host_policy_and_the_current_reviewed_space_attachment() {
    use otrios_core::{
        spaces::Error,
        transfers::{HostPolicy, authorize_share},
    };
    use otrios_interfaces::spaces::*;
    let (mut registry, target, _) = staged_fixture();
    let digest = "d".repeat(64);
    let installed = InstalledApp {
        bundle_sha256: Some(digest.clone()),
        code_bytes: Some(1024),
        revision: Some(1),
        policy: target.policy.clone(),
        title: "Canonical Notes".into(),
        ui_ref: "app:notes/ui".into(),
        state_ref: "app:notes/state".into(),
    };
    let command = Command::ShareApp {
        version: VERSION,
        id: 1,
        space: target.dataset.space.clone(),
        app: "notes".into(),
        digest: digest.clone(),
        expected_generation: registry.generation,
        catalog: target.catalog.clone(),
    };
    let host = HostPolicy::new(vec!["alice".into(), "operator-only".into()]).unwrap();
    authorize_share(
        &registry,
        "alice",
        &host,
        &command,
        &installed,
        &target.catalog,
    )
    .unwrap();
    assert_eq!(
        authorize_share(
            &registry,
            "alice",
            &HostPolicy::default(),
            &command,
            &installed,
            &target.catalog
        ),
        Err(Error::Denied)
    );
    assert_eq!(
        authorize_share(
            &registry,
            "operator-only",
            &host,
            &command,
            &installed,
            &target.catalog
        ),
        Err(Error::Denied)
    );
    registry.spaces[0].members.push(Member {
        identity: "bob".into(),
        role: Role::Admin,
        joined_generation: registry.generation,
    });
    assert_eq!(
        authorize_share(
            &registry,
            "bob",
            &host,
            &command,
            &installed,
            &target.catalog
        ),
        Err(Error::Denied)
    );
    assert_eq!(
        authorize_share(&registry, "alice", &host, &command, &installed, &[8; 32]),
        Err(Error::Conflict)
    );
    let mut wrong = installed.clone();
    wrong.bundle_sha256 = Some("f".repeat(64));
    assert_eq!(
        authorize_share(&registry, "alice", &host, &command, &wrong, &target.catalog),
        Err(Error::Conflict)
    );
    registry.spaces[0].apps[0].attached = false;
    assert_eq!(
        authorize_share(
            &registry,
            "alice",
            &host,
            &command,
            &installed,
            &target.catalog
        ),
        Err(Error::Unavailable)
    );
}

#[test]
fn app_sources_keep_identity_and_reject_local_or_ambiguous_destinations() {
    use otrios_core::transfers::{AppSource, public_address};
    let path = format!("/apps/counter/{}", "a".repeat(64));
    let source = AppSource::parse(&format!("https://Apps.Example{path}")).unwrap();
    assert_eq!(source.host(), "apps.example");
    assert_eq!(
        source.download_url(),
        format!("https://apps.example{path}/download")
    );
    assert!(source.request().contains("Accept-Encoding: identity\r\n"));
    for base in [
        "http://apps.example",
        "https://user@apps.example",
        "https://apps.example:443",
        "https://apps.example.",
        "https://-apps.example",
    ] {
        assert!(
            AppSource::parse(&format!("{base}{path}")).is_err(),
            "{base}"
        );
    }
    for suffix in [
        "?token=secret",
        "#fragment",
        "/download/",
        "/%64ownload",
        "\r\nHost: private",
    ] {
        assert!(AppSource::parse(&format!("https://apps.example{path}{suffix}")).is_err());
    }
    for ip in [
        "0.1.2.3",
        "10.0.0.1",
        "100.64.1.1",
        "127.0.0.1",
        "169.254.169.254",
        "172.31.255.1",
        "192.168.1.1",
        "192.0.2.1",
        "192.88.99.1",
        "198.19.1.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "::ffff:127.0.0.1",
        "64:ff9b::a00:1",
        "fc00::1",
        "fe80::1",
        "ff02::1",
        "2001::1",
        "2001:db8::1",
        "2002:7f00:1::",
        "3fff:fff::1",
        "4000::1",
    ] {
        assert!(!public_address(ip.parse().unwrap()), "{ip}");
        assert!(
            source
                .addresses(&["1.1.1.1".parse().unwrap(), ip.parse().unwrap()])
                .is_err(),
            "mixed DNS {ip}"
        );
    }
    for ip in [
        "1.1.1.1",
        "8.8.8.8",
        "172.32.0.1",
        "192.0.1.1",
        "192.1.2.3",
        "2606:4700::1111",
        "2001:4860:4860::8888",
    ] {
        assert!(public_address(ip.parse().unwrap()), "{ip}");
        source.addresses(&[ip.parse().unwrap()]).unwrap();
    }
    assert!(source.addresses(&[]).is_err());
    assert!(source.addresses(&["1.1.1.1".parse().unwrap(); 9]).is_err());
    source
        .redirect(&format!("https://cdn.example{path}/download"))
        .unwrap();
    source.redirect(&format!("{path}/download")).unwrap();
    for location in [
        format!("http://cdn.example{path}/download"),
        format!(
            "https://cdn.example/apps/counter/{}/download",
            "b".repeat(64)
        ),
        format!(
            "https://cdn.example/apps/another/{}/download",
            "a".repeat(64)
        ),
        "//private/path".into(),
    ] {
        assert!(source.redirect(&location).is_err());
    }
}

#[test]
fn app_http_framing_is_bounded_and_does_not_accept_ambiguous_bodies() {
    use otrios_core::transfers::{AppResponse, read_app_response};
    for response in [
        "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nzip",
        "HTTP/1.0 200 OK\r\n\r\nzip",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nSet-Cookie: secret=a\r\nSet-Cookie: secret=b\r\n\r\n1\r\nz\r\n2\r\nip\r\n0\r\n\r\n",
    ] {
        let AppResponse::Archive(bytes) = read_app_response(response.as_bytes()).unwrap() else {
            panic!("archive")
        };
        assert_eq!(bytes, b"zip");
    }
    let AppResponse::Redirect(location) = read_app_response(
        b"HTTP/1.1 307 Redirect\r\nLocation: https://apps.example/apps/id/hash/download\r\n\r\n"
            .as_slice(),
    )
    .unwrap() else {
        panic!("redirect")
    };
    assert!(location.starts_with("https://apps.example"));
    for response in [
        "HTTP/1.1 200 OK\n\nzip",
        "HTTP/1.1 200 OK\r\nContent-Length: 3\r\ncontent-length: 3\r\n\r\nzip",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\nzip",
        "HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: +3\r\n\r\nzip",
        "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nzip",
        "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\nzip",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffff\r\n",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nzipXX0\r\n\r\n",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nAuthorization: secret\r\n\r\n",
        "HTTP/1.1 302 Found\r\n\r\n",
        "HTTP/1.1 401 Denied\r\n\r\n",
    ] {
        assert!(
            read_app_response(response.as_bytes()).is_err(),
            "{response}"
        );
    }
    let oversized_header = format!("HTTP/1.1 200 OK\r\nX-Large: {}\r\n\r\n", "x".repeat(16384));
    assert!(read_app_response(oversized_header.as_bytes()).is_err());
}
