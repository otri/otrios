// SPDX-License-Identifier: LGPL-3.0-only
//! Deterministic conformance producer, kept outside runtime implementation.
use otrios_core::{
    canvas::{self, Document, chat},
    merkle::{Edit, Snapshot},
    spaces,
    transfers::*,
};
use otrios_interfaces::{
    canvas::{Field, StateSchema},
    layout::Value,
    spaces::RootSchema,
};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
fn record(bytes: &[u8]) -> Json {
    json!({"hex":canvas::hex(bytes),"sha256":canvas::hex(&canvas::hash(bytes))})
}
pub fn build() -> Json {
    let scope = "app:portable-fixture/state";
    let schema = StateSchema {
        id: "portable-fixture-v1".into(),
        fields: vec![
            Field {
                id: 1,
                name: "number".into(),
                default: Value::Number(0.),
                writable: true,
            },
            Field {
                id: 2,
                name: "text".into(),
                default: Value::Text(String::new()),
                writable: true,
            },
            Field {
                id: 3,
                name: "ready".into(),
                default: Value::Boolean(false),
                writable: false,
            },
        ],
    };
    let values = [
        Value::Number(42.),
        Value::Text("測定 ✓".into()),
        Value::Boolean(true),
    ];
    let edits = schema
        .fields
        .iter()
        .zip(&values)
        .map(|(field, value)| {
            let mut domain = b"OTriOS\0canvas-field-schema\0v1\0".to_vec();
            domain.extend(flexbuffers::to_vec((scope, &schema.id, field)).unwrap());
            Edit {
                path: vec![0, u64::from(field.id - 1)],
                schema: canvas::hash(&domain).try_into().unwrap(),
                value: Some(flexbuffers::to_vec(value).unwrap()),
            }
        })
        .collect::<Vec<_>>();
    let initial = edits
        .iter()
        .zip(&schema.fields)
        .map(|(edit, field)| Edit {
            path: edit.path.clone(),
            schema: edit.schema,
            value: Some(flexbuffers::to_vec(&field.default).unwrap()),
        })
        .collect::<Vec<_>>();
    let fields = Snapshot::empty()
        .edit([0; 16], &initial)
        .unwrap()
        .0
        .edit([17; 16], &edits)
        .unwrap()
        .0;
    let body = CanvasCheckpoint {
        format: CANVAS_CODEC.into(),
        scope: scope.into(),
        schema: schema.clone(),
        fields: fields.to_bytes().unwrap(),
        signals: None,
    };
    let mut canvas = CANVAS_CHECKPOINT_HEADER.to_vec();
    canvas.extend(flexbuffers::to_vec(&body).unwrap());
    let imported = Document::from_portable_checkpoint(schema.clone(), scope, &canvas).unwrap();
    assert_eq!(imported.portable_checkpoint().unwrap(), canvas);
    let mut signal_domain = b"OTriOS\0canvas-metadata-schema\0v1\0".to_vec();
    signal_domain.extend((scope.len() as u32).to_le_bytes());
    signal_domain.extend(scope.as_bytes());
    signal_domain.extend(3u64.to_le_bytes());
    signal_domain.extend(0u64.to_le_bytes());
    let samples = [(100i64, 2.0f32), (103, 4.2)];
    let signals = Snapshot::empty()
        .edit(
            [17; 16],
            &[Edit {
                path: vec![0, 0],
                schema: canvas::hash(&signal_domain).try_into().unwrap(),
                value: Some(flexbuffers::to_vec(samples).unwrap()),
            }],
        )
        .unwrap()
        .0;
    let mut signal_body = body.clone();
    signal_body.signals = Some(signals.to_bytes().unwrap());
    let mut with_signals = CANVAS_CHECKPOINT_HEADER.to_vec();
    with_signals.extend(flexbuffers::to_vec(signal_body).unwrap());
    assert_eq!(
        Document::from_portable_checkpoint(schema.clone(), scope, &with_signals)
            .unwrap()
            .portable_checkpoint()
            .unwrap(),
        with_signals
    );
    let auxiliary = "app:portable-fixture/aux-state";
    let aux_schema = StateSchema {
        id: "portable-fixture-aux-v1".into(),
        fields: vec![],
    };
    let aux = Document::new(aux_schema.clone(), auxiliary)
        .unwrap()
        .portable_checkpoint()
        .unwrap();
    let asset = b"portable attachment\n";
    let asset_hash = canvas::hex(&canvas::hash(asset));
    let messages = vec![
        chat::Message {
            sequence: 1,
            at: 1_700_000_000_001,
            sender: chat::Profile {
                id: "ab".repeat(16),
                name: "Ada".into(),
                avatar: 3,
            },
            kind: chat::Kind::Message,
            text: "Original immutable dispatch".into(),
            nonce: format!("{:032x}", 1),
            assets: vec![chat::Asset {
                name: "fixture.txt".into(),
                bytes: asset.len() as u32,
                sha256: asset_hash.clone(),
            }],
        },
        chat::Message {
            sequence: 4,
            at: 1_700_000_000_004,
            sender: chat::Profile {
                id: "ab".repeat(16),
                name: "Ada after rename".into(),
                avatar: 7,
            },
            kind: chat::Kind::Message,
            text: "History retains gaps and names.\n共有データ".into(),
            nonce: format!("{:032x}", 4),
            assets: vec![],
        },
    ];
    let mut chat = CHAT_CHECKPOINT_HEADER.to_vec();
    chat.extend(chat::message_schema().id);
    chat.extend(9u64.to_le_bytes());
    chat.extend((messages.len() as u32).to_le_bytes());
    let mut rows = Vec::new();
    for message in &messages {
        let row = Snapshot::empty()
            .edit(
                [22; 16],
                &[Edit {
                    path: vec![chat::MESSAGE_FIELD],
                    schema: chat::message_schema().id,
                    value: Some(flexbuffers::to_vec(message).unwrap()),
                }],
            )
            .unwrap()
            .0;
        let bytes = row.to_bytes().unwrap();
        chat.extend(message.sequence.to_le_bytes());
        chat.extend((bytes.len() as u32).to_le_bytes());
        chat.extend(&bytes);
        rows.push(json!({"sequence":message.sequence,"snapshot":record(&bytes),"root_sha256":canvas::hex(&row.hash()),"message_flex":record(&flexbuffers::to_vec(message).unwrap())}));
    }
    assert_eq!(chat::QualifiedHistory::decode(&chat).unwrap().len(), 2);
    let roots = [
        OwnedRoot {
            state_ref: scope.into(),
            canvas: canvas.clone(),
            chat: Some(chat.clone()),
        },
        OwnedRoot {
            state_ref: auxiliary.into(),
            canvas: aux.clone(),
            chat: None,
        },
    ];
    let collection = encode_collection(&roots).unwrap();
    let policies = vec![
        RootPolicy {
            state_ref: scope.into(),
            schema: schema.clone(),
            chat: true,
        },
        RootPolicy {
            state_ref: auxiliary.into(),
            schema: aux_schema.clone(),
            chat: false,
        },
    ];
    let schema_hash = spaces::collection_schema(
        &policies
            .iter()
            .map(|p| RootSchema {
                state_ref: p.state_ref.clone(),
                schema: p.schema.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let info = DataInfo {
        format: DATA_FORMAT.into(),
        bundle_id: "portable-fixture".into(),
        schema_sha256: schema_hash.clone(),
        codec: COLLECTION_CODEC.into(),
        codec_version: 1,
        checkpoint: "State/checkpoint.bin".into(),
    };
    let assets = BTreeMap::from([(asset_hash.clone(), asset.to_vec())]);
    let archive = encode_data(info.clone(), collection.clone(), assets).unwrap();
    let files = archive::decode(archive::Kind::Data, &archive).unwrap();
    QualifiedCollection::qualify(
        inspect_data(files.clone()).unwrap(),
        "portable-fixture",
        &schema_hash,
        &policies,
    )
    .unwrap();
    json!({"format":"otrios-portable-data-fixtures-v1", "producer":"Rust 1.90; FlexBuffers 25.12.19; deterministic causal actors 17/22; no authority identity",
        "canvas":{"scope":scope,"schema":schema,"values":values,"fields_root_sha256":canvas::hex(&fields.hash()),"fields_snapshot":record(&body.fields),"checkpoint":record(&canvas)},
        "auxiliary":{"scope":auxiliary,"schema":aux_schema,"checkpoint":record(&aux)},
        "with_signals":{"checkpoint":record(&with_signals),"snapshot":record(&signals.to_bytes().unwrap()),"root_sha256":canvas::hex(&signals.hash()),"samples":samples},
        "chat":{"high_water":9,"messages":messages,"rows":rows,"checkpoint":record(&chat)},
        "collection":{"schema_sha256":schema_hash,"checkpoint":record(&collection)},"info":info,
        "files":files.iter().map(|(name, bytes)|(name.clone(),record(bytes))).collect::<BTreeMap<_,_>>(),"archive":record(&archive)})
}
