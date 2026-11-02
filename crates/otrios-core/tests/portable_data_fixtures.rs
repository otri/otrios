// SPDX-License-Identifier: LGPL-3.0-only
#![cfg(feature = "canvas")]
#[path = "fixtures/portable_data_case.rs"]
mod portable_data_case;
use otrios_core::{
    canvas::{self, Document, chat},
    transfers::*,
};
use otrios_interfaces::{canvas::StateSchema, layout::Value};
use serde_json::Value as Json;
use std::{collections::BTreeMap, fs, path::PathBuf};
fn vectors() -> Json {
    serde_json::from_str(include_str!("fixtures/portable-data-fixtures.json")).unwrap()
}
fn bytes(value: &Json) -> Vec<u8> {
    let hex = value["hex"].as_str().unwrap();
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        canvas::hex(&canvas::hash(&bytes)),
        value["sha256"].as_str().unwrap()
    );
    bytes
}
#[test]
fn canonical_metadata_and_checkpoints_match_published_bytes_and_pinned_sha256() {
    let v = vectors();
    for (key, digest) in [
        (
            "canvas",
            "56a89ee11487736fc6e556342d5748d0759820f9e5f909f324acaca105eb2121",
        ),
        (
            "auxiliary",
            "ec1f6a60b8fcfcd25147fc4261f78815db84eacfaa14be04bfd64790bae466a3",
        ),
        (
            "chat",
            "88c910194c553b350bb268ed2e5aab90de3d9342910b1b2a72513547a0255c97",
        ),
        (
            "collection",
            "8f7cc25edc5c385305248ad1aa11e229d4d6a588d1ec75dcfb6b7113a184ce80",
        ),
        (
            "with_signals",
            "92d0084292718672b596347e2dbcde933e07e446b4966b8e2052e1fd71c07fce",
        ),
    ] {
        assert_eq!(
            canvas::hex(&canvas::hash(&bytes(&v[key]["checkpoint"]))),
            digest
        );
    }
    assert_eq!(
        canvas::hex(&canvas::hash(&bytes(&v["archive"]))),
        "730ccd65407c0cc6357ae918c1c3358d2b666310438e3066ba1e1cf0c1e4046e"
    );
    assert_eq!(
        portable_data_case::build(),
        v,
        "Encoder changes require a reviewed format/fixture decision"
    );
    let files = archive::decode(archive::Kind::Data, &bytes(&v["archive"])).unwrap();
    let expected = v["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| (name.clone(), bytes(value)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(files, expected);
    let data = inspect_data(files).unwrap();
    assert_eq!(
        data.info(),
        &serde_json::from_value::<DataInfo>(v["info"].clone()).unwrap()
    );
    assert_eq!(data.checkpoint(), bytes(&v["collection"]["checkpoint"]));
}

#[test]
fn frozen_signal_history_survives_import_without_enabling_writer_policy() {
    let v = vectors();
    let schema: StateSchema = serde_json::from_value(v["canvas"]["schema"].clone()).unwrap();
    let scope = v["canvas"]["scope"].as_str().unwrap();
    let bytes = bytes(&v["with_signals"]["checkpoint"]);
    let mut doc = Document::from_portable_checkpoint(schema.clone(), scope, &bytes).unwrap();
    assert_eq!(doc.portable_checkpoint().unwrap(), bytes);
    assert_eq!(doc.signal(1, 103, 30), vec![[0.9, 2.], [1., 4.2]]);
    doc.set_wall_time(104);
    doc.set(&[(1, 43)]).unwrap();
    assert_eq!(
        doc.signal(1, 104, 30).len(),
        2,
        "imported history cannot enable authority capture policy"
    );
    assert!(Document::load(schema, scope, &doc.save().unwrap()).is_ok());
}
struct Stage(PathBuf);
impl Stage {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "otrios-vector-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn frozen_collection_import_preserves_all_roots_full_causal_history_and_owned_assets_after_reopen()
{
    let v = vectors();
    let policies = ["canvas", "auxiliary"].map(|key| RootPolicy {
        state_ref: v[key]["scope"].as_str().unwrap().into(),
        schema: serde_json::from_value::<StateSchema>(v[key]["schema"].clone()).unwrap(),
        chat: key == "canvas",
    });
    let imported = QualifiedCollection::qualify(
        inspect_data(archive::decode(archive::Kind::Data, &bytes(&v["archive"])).unwrap()).unwrap(),
        "portable-fixture",
        v["collection"]["schema_sha256"].as_str().unwrap(),
        &policies,
    )
    .unwrap();
    let stage = Stage::new();
    let cid = "1".repeat(32);
    imported
        .write_stage(&stage.0, &policies[0].state_ref, &cid)
        .unwrap();
    for (index, policy) in policies.iter().enumerate() {
        let file = if index == 0 {
            stage.0.join(format!("collection-{cid}.merkle"))
        } else {
            stage.0.join(format!(
                "collection-{cid}.data/doc-{}.merkle",
                canvas::hex(&canvas::hash(policy.state_ref.as_bytes()))
            ))
        };
        let doc = Document::load(
            policy.schema.clone(),
            &policy.state_ref,
            &fs::read(file).unwrap(),
        )
        .unwrap();
        let key = if index == 0 { "canvas" } else { "auxiliary" };
        assert_eq!(
            doc.portable_checkpoint().unwrap(),
            bytes(&v[key]["checkpoint"])
        );
        if index == 0 {
            let expected: Vec<Value> =
                serde_json::from_value(v["canvas"]["values"].clone()).unwrap();
            for (i, value) in expected.iter().enumerate() {
                assert_eq!(&doc.typed_value(i as u16 + 1).unwrap(), value);
            }
        }
    }
    let path = stage
        .0
        .join(format!("collection-{cid}.data/chat-primary.journal"));
    let room = chat::Room::open(Some(path), 1_800_000_000_000).unwrap();
    assert_eq!(
        room.pin_portable_history().unwrap().encode().unwrap(),
        bytes(&v["chat"]["checkpoint"])
    );
    let assets = chat::assets::Assets::new(&stage.0, &cid).unwrap();
    let messages: Vec<chat::Message> =
        serde_json::from_value(v["chat"]["messages"].clone()).unwrap();
    for message in messages {
        for asset in message.assets {
            assert_eq!(
                assets.read(&asset).unwrap(),
                bytes(&v["files"][format!("Assets/{}", asset.sha256)])
            );
        }
    }
    assert!(
        imported
            .write_stage(&stage.0, &policies[0].state_ref, &cid)
            .is_err()
    );
}
#[test]
fn frozen_checkpoints_reject_truncation_schema_changes_and_causal_authority_substitution() {
    let v = vectors();
    let schema: StateSchema = serde_json::from_value(v["canvas"]["schema"].clone()).unwrap();
    let scope = v["canvas"]["scope"].as_str().unwrap();
    let canvas = bytes(&v["canvas"]["checkpoint"]);
    for end in [0, 7, 8, canvas.len() - 1] {
        assert!(Document::from_portable_checkpoint(schema.clone(), scope, &canvas[..end]).is_err());
    }
    let mut checkpoint: CanvasCheckpoint = flexbuffers::from_slice(&canvas[8..]).unwrap();
    checkpoint.fields = bytes(&v["chat"]["rows"][0]["snapshot"]);
    let mut bad = CANVAS_CHECKPOINT_HEADER.to_vec();
    bad.extend(flexbuffers::to_vec(checkpoint).unwrap());
    assert!(Document::from_portable_checkpoint(schema, scope, &bad).is_err());
    let chat = bytes(&v["chat"]["checkpoint"]);
    for end in [0, 7, 39, 51, chat.len() - 1] {
        assert!(chat::QualifiedHistory::decode(&chat[..end]).is_err());
    }
    let mut bad = chat.clone();
    bad[40..48].copy_from_slice(&0u64.to_le_bytes());
    assert!(chat::QualifiedHistory::decode(&bad).is_err());
    let mut bad = chat;
    bad.push(0);
    assert!(chat::QualifiedHistory::decode(&bad).is_err());
}
