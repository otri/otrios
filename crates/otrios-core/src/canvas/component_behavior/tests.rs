// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use otrios_interfaces::{
    layout::Value,
    oib::{
        component_behavior::{Command, Event, Phase, encode_output},
        designer::{InstanceId, NodeId, ObjectAddress},
    },
};

fn input() -> Input {
    Input {
        behavior: 1,
        address: ObjectAddress {
            instance: InstanceId(20),
            references: vec![NodeId(2)],
            node: NodeId(3),
        },
        event: Event {
            phase: Phase::Move,
            pointer: 17,
            a: 420,
            b: 0,
        },
        properties: vec![],
        local: vec![],
    }
}
fn output() -> Output {
    Output {
        commands: vec![Command::Emit {
            outlet: 1,
            value: Value::Number(420.),
        }],
    }
}
fn wasm(text: &str) -> Vec<u8> {
    wat::parse_str(text).unwrap()
}
fn digest(bytes: &[u8]) -> CodeHash {
    Sha256::digest(bytes).into()
}
fn module_with(output: &[u8], dispatch: &str, extra: &str) -> Vec<u8> {
    let data: String = output.iter().map(|byte| format!("\\{byte:02x}")).collect();
    wasm(&format!(
        r#"(module
            (memory (export "memory") 1 1)
            (func (export "abi_version") (result i32) i32.const 1)
            (func (export "input_buffer") (result i32) i32.const 0)
            (data (i32.const 16384) "{data}")
            {extra}
            (func (export "dispatch") (param i32 i32) (result i64) {dispatch})
        )"#,
    ))
}
fn module() -> Vec<u8> {
    let output = encode_output(&output()).unwrap();
    let packed = ((output.len() as u64) << 32) | 16_384;
    module_with(&output, &format!("i64.const {packed}"), "")
}
fn compile(runtime: &Runtime, bytes: &[u8]) -> Arc<Compiled> {
    runtime.compile(bytes, digest(bytes)).unwrap()
}

#[test]
fn exact_modules_share_immutable_code_and_guest_calls_decode_complete_output() {
    let runtime = Runtime::default();
    let bytes = module();
    let a = compile(&runtime, &bytes);
    let b = compile(&runtime, &bytes);
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(a.code_hash(), digest(&bytes));
    let mut guest = runtime.instantiate(a).unwrap();
    assert_eq!(guest.code_hash(), digest(&bytes));
    assert_eq!(guest.call(&input()).unwrap(), output());
    assert_eq!(runtime.live_instances(), 1);
    drop(guest);
    assert_eq!(runtime.live_instances(), 0);
}

#[test]
fn retired_revision_code_is_not_kept_by_the_runtime_cache() {
    let runtime = Runtime::default();
    let output = encode_output(&output()).unwrap();
    let packed = ((output.len() as u64) << 32) | 16_384;
    let mut revisions = Vec::new();
    for revision in 0..MAX_INSTANCES {
        let bytes = module_with(
            &output,
            &format!("i64.const {packed}"),
            &format!("(global i32 (i32.const {revision}))"),
        );
        revisions.push(compile(&runtime, &bytes));
    }
    let bytes = module_with(
        &output,
        &format!("i64.const {packed}"),
        "(global i32 (i32.const 100))",
    );
    assert!(runtime.compile(&bytes, digest(&bytes)).is_err());
    let retired = Arc::downgrade(revisions.last().unwrap());
    drop(revisions.pop());
    assert!(retired.upgrade().is_none());
    revisions.push(compile(&runtime, &bytes));
    assert_eq!(runtime.modules.lock().unwrap().len(), MAX_INSTANCES);
    assert_eq!(runtime.live_instances(), 0);
}

#[test]
fn memory_and_globals_are_scratch_between_calls_and_occurrences() {
    let output = encode_output(&output()).unwrap();
    let packed = ((output.len() as u64) << 32) | 16_384;
    // The first result is 2, whether called again on the same occurrence or on a
    // sibling. A retained global or memory counter would incorrectly produce 4.
    let body = format!(
        "global.get $counter i32.const 1 i32.add global.set $counter
         i32.const 32768 i32.const 32768 i32.load i32.const 1 i32.add i32.store
         i32.const 16420 global.get $counter i32.const 32768 i32.load i32.add
         f32.convert_i32_u f32.store i64.const {packed}"
    );
    let bytes = module_with(&output, &body, "(global $counter (mut i32) (i32.const 0))");
    let runtime = Runtime::default();
    let compiled = compile(&runtime, &bytes);
    let mut a = runtime.instantiate(compiled.clone()).unwrap();
    let mut b = runtime.instantiate(compiled).unwrap();
    let expected = Output {
        commands: vec![Command::Emit {
            outlet: 1,
            value: Value::Number(2.),
        }],
    };
    assert_eq!(a.call(&input()).unwrap(), expected);
    assert_eq!(a.call(&input()).unwrap(), expected);
    assert_eq!(b.call(&input()).unwrap(), expected);
}

#[test]
fn active_and_staged_occurrences_share_one_budget_and_failed_loads_release_it() {
    let runtime = Runtime::default();
    let compiled = compile(&runtime, &module());
    runtime.check_capacity(MAX_INSTANCES).unwrap();
    let mut guests = Vec::new();
    for _ in 0..MAX_INSTANCES {
        guests.push(runtime.instantiate(compiled.clone()).unwrap());
    }
    assert_eq!(runtime.live_instances(), MAX_INSTANCES);
    assert!(runtime.check_capacity(1).is_err());
    assert!(runtime.check_capacity(usize::MAX).is_err());
    assert!(runtime.instantiate(compiled.clone()).is_err());
    drop(guests.pop());
    runtime.check_capacity(1).unwrap();
    guests.push(runtime.instantiate(compiled.clone()).unwrap());
    assert_eq!(runtime.live_instances(), MAX_INSTANCES);
    let another_runtime = Runtime::default();
    assert!(another_runtime.instantiate(compiled).is_err());
    assert_eq!(another_runtime.live_instances(), 0);
}

#[test]
fn code_identity_imports_memory_and_table_limits_are_enforced() {
    let runtime = Runtime::default();
    let bytes = module();
    assert!(runtime.compile(&bytes, [0; 32]).is_err());
    assert!(runtime.compile(&[], digest(&[])).is_err());
    let oversized = vec![0; MAX_CODE_BYTES + 1];
    assert!(runtime.compile(&oversized, digest(&oversized)).is_err());
    let text = String::from_utf8(wat_placeholder()).unwrap();
    for text in [
        text.replace(
            "(memory (export \"memory\") 1 1)",
            "(memory (export \"memory\") 0 1)",
        ),
        text.replace(
            "(memory (export \"memory\") 1 1)",
            "(memory (export \"memory\") 1)",
        ),
        text.replace(
            "(memory (export \"memory\") 1 1)",
            "(memory (export \"memory\") 2 2)",
        ),
        text.replace("(module", "(module (import \"host\" \"run\" (func))"),
        text.replace("(module", "(module (table 0 funcref)"),
        text.replace("(module", "(module (memory 1 1)"),
        text.replace(
            "(memory (export \"memory\") 1 1)",
            "(memory (export \"memory\") (export \"alias\") 1 1)",
        ),
    ] {
        let bytes = wasm(&text);
        assert!(runtime.compile(&bytes, digest(&bytes)).is_err(), "{text}");
        assert_eq!(runtime.live_instances(), 0);
    }
}
fn wat_placeholder() -> Vec<u8> {
    br#"(module
        (memory (export "memory") 1 1)
        (func (export "abi_version") (result i32) i32.const 1)
        (func (export "input_buffer") (result i32) i32.const 0)
        (func (export "dispatch") (param i32 i32) (result i64) i64.const 0)
    )"#
    .to_vec()
}

#[test]
fn bad_exports_initialization_traps_fuel_and_input_ranges_reject_admission() {
    let runtime = Runtime::default();
    let text = String::from_utf8(wat_placeholder()).unwrap();
    for text in [
        text.replace("i32.const 1)", "i32.const 0)"),
        text.replace(
            "(export \"abi_version\") (result i32)",
            "(export \"abi_version\") (result i64)",
        )
        .replace("i32.const 1)", "i64.const 1)"),
        text.replace(
            "(export \"input_buffer\") (result i32) i32.const 0",
            "(export \"input_buffer\") (result i32) i32.const -1",
        ),
        text.replace(
            "(export \"input_buffer\") (result i32) i32.const 0",
            "(export \"input_buffer\") (result i32) i32.const 65535",
        ),
        text.replace(
            "(module",
            "(module (func $start unreachable) (start $start)",
        ),
        text.replace(
            "(module",
            "(module (func $start (loop $spin br $spin)) (start $start)",
        ),
        text.replace(
            "(export \"dispatch\") (param i32 i32) (result i64) i64.const 0",
            "(export \"dispatch\") (result i32) i32.const 0",
        ),
    ] {
        let bytes = wasm(&text);
        assert!(runtime.compile(&bytes, digest(&bytes)).is_err(), "{text}");
        assert_eq!(runtime.live_instances(), 0);
    }
}

#[test]
fn dispatch_traps_fuel_memory_ranges_and_malformed_complete_outputs_are_rejected() {
    let output = encode_output(&output()).unwrap();
    let runtime = Runtime::default();
    for body in [
        "unreachable".to_string(),
        "(loop $spin br $spin) unreachable".into(),
        "i32.const 65535 i32.load drop i64.const 0".into(),
        format!("i64.const {}", (16_u64 << 32) | u64::from(u32::MAX)),
        format!("i64.const {}", (8193_u64 << 32) | 16384),
        "i64.const 0".into(),
    ] {
        let bytes = module_with(&output, &body, "");
        let compiled = compile(&runtime, &bytes);
        let mut guest = runtime.instantiate(compiled).unwrap();
        assert!(guest.call(&input()).is_err());
    }
    let mut malformed = encode_output(&Output {
        commands: vec![
            Command::Emit {
                outlet: 1,
                value: Value::Number(420.),
            },
            Command::Capture,
        ],
    })
    .unwrap();
    // A valid first command must not escape when the second record is invalid.
    malformed[40] = 255;
    let packed = ((malformed.len() as u64) << 32) | 16384;
    let bytes = module_with(&malformed, &format!("i64.const {packed}"), "");
    let compiled = compile(&runtime, &bytes);
    assert!(
        runtime
            .instantiate(compiled)
            .unwrap()
            .call(&input())
            .is_err()
    );
}

#[test]
fn memory_range_uses_checked_arithmetic_on_all_host_widths() {
    assert_eq!(memory_range(65520, 16, 65536).unwrap(), 65520..65536);
    assert!(memory_range(65520, 17, 65536).is_err());
    assert!(memory_range(usize::MAX, 1, 65536).is_err());
    assert!(memory_range(0, MAX_MESSAGE_BYTES + 1, 65536).is_err());
}
