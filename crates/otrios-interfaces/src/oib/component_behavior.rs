// SPDX-License-Identifier: Apache-2.0
//! RFC 0036 component behavior ABI 1: canonical public records, not execution.
//!
//! The importing runtime independently admits properties, outlets, capture and
//! app bindings. A well-formed record grants no write or input authority.
use super::designer::{InstanceId, MAX_REFERENCE_DEPTH, NodeId, ObjectAddress};
use crate::layout::Value;
use std::collections::BTreeSet;

pub const ABI: u16 = 1;
pub const FEATURE: &str = "otrios-component-behavior-v1";
pub const MEMORY_EXPORT: &str = "memory";
pub const VERSION_EXPORT: &str = "abi_version";
pub const INPUT_BUFFER_EXPORT: &str = "input_buffer";
pub const DISPATCH_EXPORT: &str = "dispatch";
pub const MAX_MESSAGE_BYTES: usize = 8192;
pub const MAX_COMMANDS: usize = 16;
pub const MAX_FIELDS: usize = 64;
pub const INPUT_HEADER_BYTES: usize = 48;
pub const OUTPUT_HEADER_BYTES: usize = 16;
pub const COMMAND_HEADER_BYTES: usize = 16;
pub const INPUT_MAGIC: &[u8; 8] = b"OTCBIN1\0";
pub const OUTPUT_MAGIC: &[u8; 8] = b"OTCBOUT1";
const MAX_TEXT_BYTES: usize = 4096;
const MAX_SERIES_POINTS: usize = 2016;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Down = 0,
    Move = 1,
    Up = 2,
    Cancel = 3,
    Activate = 4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub phase: Phase,
    /// Host input identity, scoped to this occurrence's admitted input target.
    pub pointer: u32,
    /// Normalized local input, inclusive 0..=1000.
    pub a: u16,
    pub b: u16,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// Declared property identity, never an app-state field number.
    pub id: u64,
    pub value: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    /// Signed stable behavior entry within the verified library.
    pub behavior: u64,
    pub address: ObjectAddress,
    pub event: Event,
    /// Effective exposed properties in strictly increasing identity order.
    pub properties: Vec<Field>,
    /// Host-owned resumable local properties, in the same canonical order.
    pub local: Vec<Field>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    SetLocal {
        property: u64,
        value: Value,
    },
    Emit {
        outlet: u64,
        value: Value,
    },
    /// Request capture only for the current occurrence's admitted input target.
    Capture,
    ReleaseCapture,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    /// Ordered commands; at most one write/emission per corresponding identity,
    /// and at most one capture disposition in the complete result.
    pub commands: Vec<Command>,
}

fn failure() -> String {
    "Invalid component behavior ABI 1 record".into()
}
fn valid_number(value: f32) -> bool {
    value.is_finite() && value.to_bits() != (-0.0f32).to_bits()
}
fn check_fields(fields: &[Field]) -> Result<(), String> {
    if fields.len() > MAX_FIELDS
        || fields.iter().any(|f| f.id == 0)
        || fields.windows(2).any(|w| w[0].id >= w[1].id)
    {
        return Err(failure());
    }
    Ok(())
}
fn check_input(input: &Input) -> Result<(), String> {
    if input.behavior == 0
        || input.address.instance.0 == 0
        || input.address.node.0 == 0
        || input.address.references.len() > MAX_REFERENCE_DEPTH
        || input.address.references.iter().any(|id| id.0 == 0)
        || input.event.a > 1000
        || input.event.b > 1000
    {
        return Err(failure());
    }
    check_fields(&input.properties)?;
    check_fields(&input.local)
}
fn check_output(output: &Output) -> Result<(), String> {
    if output.commands.len() > MAX_COMMANDS {
        return Err(failure());
    }
    let mut local = BTreeSet::new();
    let mut outlets = BTreeSet::new();
    let mut capture = false;
    for command in &output.commands {
        let valid = match command {
            Command::SetLocal { property, .. } => *property != 0 && local.insert(*property),
            Command::Emit { outlet, .. } => *outlet != 0 && outlets.insert(*outlet),
            Command::Capture | Command::ReleaseCapture => !std::mem::replace(&mut capture, true),
        };
        if !valid {
            return Err(failure());
        }
    }
    Ok(())
}
#[derive(Default)]
struct Writer(Vec<u8>);
impl Writer {
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_MESSAGE_BYTES)
        {
            return Err(failure());
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn u16(&mut self, value: u16) -> Result<(), String> {
        self.bytes(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> Result<(), String> {
        self.bytes(&value.to_le_bytes())
    }
    fn number(&mut self, value: f32) -> Result<(), String> {
        if !valid_number(value) {
            return Err(failure());
        }
        self.bytes(&value.to_bits().to_le_bytes())
    }
    fn value(&mut self, value: &Value) -> Result<(), String> {
        let (tag, len) = match value {
            Value::Number(_) => (1, 4),
            Value::Boolean(_) => (2, 1),
            Value::Text(text) if text.len() <= MAX_TEXT_BYTES && !text.contains('\0') => {
                (3, text.len())
            }
            Value::Color(_) => (4, 4),
            Value::Series(values) if values.len() <= MAX_SERIES_POINTS => (5, values.len() * 4),
            Value::TimedSeries(values) if values.len() <= MAX_SERIES_POINTS => {
                (6, values.len() * 8)
            }
            _ => return Err(failure()),
        };
        self.bytes(&[tag, 0])?;
        self.u16(u16::try_from(len).map_err(|_| failure())?)?;
        match value {
            Value::Number(value) => self.number(*value),
            Value::Boolean(value) => self.bytes(&[u8::from(*value)]),
            Value::Text(value) => self.bytes(value.as_bytes()),
            Value::Color(value) => self.bytes(value),
            Value::Series(values) => {
                for value in values {
                    self.number(*value)?;
                }
                Ok(())
            }
            Value::TimedSeries(values) => {
                for point in values {
                    for value in point {
                        self.number(*value)?;
                    }
                }
                Ok(())
            }
        }
    }
    fn fields(&mut self, fields: &[Field]) -> Result<(), String> {
        for field in fields {
            self.u64(field.id)?;
            self.value(&field.value)?;
        }
        Ok(())
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(failure());
        }
        Ok(Self { bytes, offset: 0 })
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.offset.checked_add(len).ok_or_else(failure)?;
        let result = self.bytes.get(self.offset..end).ok_or_else(failure)?;
        self.offset = end;
        Ok(result)
    }
    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn zeros(&mut self, len: usize) -> Result<(), String> {
        if self.take(len)?.iter().any(|b| *b != 0) {
            return Err(failure());
        }
        Ok(())
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| failure())?,
        ))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| failure())?,
        ))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| failure())?,
        ))
    }
    fn number(&mut self) -> Result<f32, String> {
        let value = f32::from_bits(self.u32()?);
        if !valid_number(value) {
            return Err(failure());
        }
        Ok(value)
    }
    fn value(&mut self) -> Result<Value, String> {
        let tag = self.byte()?;
        self.zeros(1)?;
        let len = usize::from(self.u16()?);
        let mut payload = Reader::new(self.take(len)?)?;
        let value = match (tag, len) {
            (1, 4) => Value::Number(payload.number()?),
            (2, 1) => Value::Boolean(match payload.byte()? {
                0 => false,
                1 => true,
                _ => return Err(failure()),
            }),
            (3, 0..=MAX_TEXT_BYTES) => {
                let text = std::str::from_utf8(payload.take(len)?).map_err(|_| failure())?;
                if text.contains('\0') {
                    return Err(failure());
                }
                Value::Text(text.into())
            }
            (4, 4) => Value::Color(payload.take(4)?.try_into().map_err(|_| failure())?),
            (5, _) if len % 4 == 0 && len / 4 <= MAX_SERIES_POINTS => {
                let mut values = Vec::with_capacity(len / 4);
                for _ in 0..len / 4 {
                    values.push(payload.number()?);
                }
                Value::Series(values)
            }
            (6, _) if len % 8 == 0 && len / 8 <= MAX_SERIES_POINTS => {
                let mut values = Vec::with_capacity(len / 8);
                for _ in 0..len / 8 {
                    values.push([payload.number()?, payload.number()?]);
                }
                Value::TimedSeries(values)
            }
            _ => return Err(failure()),
        };
        payload.finish()?;
        Ok(value)
    }
    fn fields(&mut self, count: usize) -> Result<Vec<Field>, String> {
        if count > MAX_FIELDS {
            return Err(failure());
        }
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(Field {
                id: self.u64()?,
                value: self.value()?,
            });
        }
        check_fields(&result)?;
        Ok(result)
    }
    fn finish(self) -> Result<(), String> {
        if self.offset != self.bytes.len() {
            return Err(failure());
        }
        Ok(())
    }
}

/// Encode one canonical context. Declared capabilities are runtime-owned.
pub fn encode_input(input: &Input) -> Result<Vec<u8>, String> {
    check_input(input)?;
    let mut writer = Writer::default();
    writer.bytes(INPUT_MAGIC)?;
    writer.bytes(&[input.event.phase as u8, 0])?;
    writer.u16(input.event.a)?;
    writer.u16(input.event.b)?;
    writer.u16(input.properties.len() as u16)?;
    writer.u16(input.local.len() as u16)?;
    writer.bytes(&[input.address.references.len() as u8, 0])?;
    writer.bytes(&input.event.pointer.to_le_bytes())?;
    writer.u64(input.address.instance.0)?;
    writer.u64(input.address.node.0)?;
    writer.u64(input.behavior)?;
    for reference in &input.address.references {
        writer.u64(reference.0)?;
    }
    writer.fields(&input.properties)?;
    writer.fields(&input.local)?;
    Ok(writer.0)
}
/// Decode a bounded canonical context; no trailing bytes or alternate forms.
pub fn decode_input(bytes: &[u8]) -> Result<Input, String> {
    let mut reader = Reader::new(bytes)?;
    if reader.take(8)? != INPUT_MAGIC {
        return Err(failure());
    }
    let phase = match reader.byte()? {
        0 => Phase::Down,
        1 => Phase::Move,
        2 => Phase::Up,
        3 => Phase::Cancel,
        4 => Phase::Activate,
        _ => return Err(failure()),
    };
    reader.zeros(1)?;
    let a = reader.u16()?;
    let b = reader.u16()?;
    let properties = usize::from(reader.u16()?);
    let local = usize::from(reader.u16()?);
    let references = usize::from(reader.byte()?);
    reader.zeros(1)?;
    if properties > MAX_FIELDS || local > MAX_FIELDS || references > MAX_REFERENCE_DEPTH {
        return Err(failure());
    }
    let pointer = reader.u32()?;
    let instance = InstanceId(reader.u64()?);
    let node = NodeId(reader.u64()?);
    let behavior = reader.u64()?;
    let mut path = Vec::with_capacity(references);
    for _ in 0..references {
        path.push(NodeId(reader.u64()?));
    }
    let input = Input {
        behavior,
        address: ObjectAddress {
            instance,
            references: path,
            node,
        },
        event: Event {
            phase,
            pointer,
            a,
            b,
        },
        properties: reader.fields(properties)?,
        local: reader.fields(local)?,
    };
    reader.finish()?;
    check_input(&input)?;
    Ok(input)
}
/// Encode an ordered, canonical command result. Admission remains runtime-owned.
pub fn encode_output(output: &Output) -> Result<Vec<u8>, String> {
    check_output(output)?;
    let mut writer = Writer::default();
    writer.bytes(OUTPUT_MAGIC)?;
    writer.u16(output.commands.len() as u16)?;
    writer.bytes(&[0; 6])?;
    for command in &output.commands {
        let (tag, target) = match command {
            Command::SetLocal { property, .. } => (1, *property),
            Command::Emit { outlet, .. } => (2, *outlet),
            Command::Capture => (3, 0),
            Command::ReleaseCapture => (4, 0),
        };
        writer.bytes(&[tag])?;
        writer.bytes(&[0; 7])?;
        writer.u64(target)?;
        if let Command::SetLocal { value, .. } | Command::Emit { value, .. } = command {
            writer.value(value)?;
        }
    }
    Ok(writer.0)
}
/// Decode a bounded result before the runtime validates the complete command set.
pub fn decode_output(bytes: &[u8]) -> Result<Output, String> {
    let mut reader = Reader::new(bytes)?;
    if reader.take(8)? != OUTPUT_MAGIC {
        return Err(failure());
    }
    let count = usize::from(reader.u16()?);
    reader.zeros(6)?;
    if count > MAX_COMMANDS {
        return Err(failure());
    }
    let mut commands = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = reader.byte()?;
        reader.zeros(7)?;
        let target = reader.u64()?;
        commands.push(match tag {
            1 => Command::SetLocal {
                property: target,
                value: reader.value()?,
            },
            2 => Command::Emit {
                outlet: target,
                value: reader.value()?,
            },
            3 if target == 0 => Command::Capture,
            4 if target == 0 => Command::ReleaseCapture,
            _ => return Err(failure()),
        });
    }
    reader.finish()?;
    let output = Output { commands };
    check_output(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        Input {
            behavior: 11,
            address: ObjectAddress {
                instance: InstanceId(20),
                references: vec![NodeId(2)],
                node: NodeId(3),
            },
            event: Event {
                phase: Phase::Move,
                pointer: 7,
                a: 750,
                b: 250,
            },
            properties: vec![Field {
                id: 1,
                value: Value::Number(400.),
            }],
            local: vec![Field {
                id: 2,
                value: Value::Boolean(true),
            }],
        }
    }
    fn output() -> Output {
        Output {
            commands: vec![
                Command::SetLocal {
                    property: 2,
                    value: Value::Boolean(true),
                },
                Command::Emit {
                    outlet: 9,
                    value: Value::Number(750.),
                },
                Command::Capture,
            ],
        }
    }
    fn hex(text: &str) -> Vec<u8> {
        let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        text.as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn canonical_input_golden_vector() {
        let bytes = hex("4f544342494e3100 0100ee02fa00010001000100 07000000
            1400000000000000 0300000000000000 0b00000000000000 0200000000000000
            0100000000000000 01000400 0000c843
            0200000000000000 02000100 01");
        assert_eq!(bytes.len(), 85);
        assert_eq!(encode_input(&input()).unwrap(), bytes);
        assert_eq!(decode_input(&bytes).unwrap(), input());
    }
    #[test]
    fn canonical_output_golden_vector() {
        let bytes = hex("4f5443424f555431 0300000000000000
            0100000000000000 0200000000000000 02000100 01
            0200000000000000 0900000000000000 01000400 00803b44
            0300000000000000 0000000000000000");
        assert_eq!(bytes.len(), 77);
        assert_eq!(encode_output(&output()).unwrap(), bytes);
        assert_eq!(decode_output(&bytes).unwrap(), output());
    }
    #[test]
    fn input_and_output_reject_every_truncation_and_trailing_byte() {
        let input = encode_input(&input()).unwrap();
        let output = encode_output(&output()).unwrap();
        for end in 0..input.len() {
            assert!(decode_input(&input[..end]).is_err(), "input {end}");
        }
        for end in 0..output.len() {
            assert!(decode_output(&output[..end]).is_err(), "output {end}");
        }
        let mut input = input;
        input.push(0);
        assert!(decode_input(&input).is_err());
        let mut output = output;
        output.push(0);
        assert!(decode_output(&output).is_err());
    }
    #[test]
    fn all_value_families_have_one_canonical_encoding() {
        let mut source = input();
        source.properties = [
            Value::Number(-1.25),
            Value::Boolean(false),
            Value::Text("é日本".into()),
            Value::Color([1, 2, 3, 4]),
            Value::Series(vec![0., -2., 3.]),
            Value::TimedSeries(vec![[0.25, 2.], [0.5, 3.]]),
            Value::Text(String::new()),
            Value::Series(vec![]),
            Value::TimedSeries(vec![]),
        ]
        .into_iter()
        .enumerate()
        .map(|(n, value)| Field {
            id: n as u64 + 1,
            value,
        })
        .collect();
        let encoded = encode_input(&source).unwrap();
        let decoded = decode_input(&encoded).unwrap();
        assert_eq!(source, decoded);
        assert_eq!(encode_input(&decoded).unwrap(), encoded);
        for phase in [
            Phase::Down,
            Phase::Move,
            Phase::Up,
            Phase::Cancel,
            Phase::Activate,
        ] {
            source.event.phase = phase;
            assert_eq!(
                decode_input(&encode_input(&source).unwrap())
                    .unwrap()
                    .event
                    .phase,
                phase
            );
        }
    }
    #[test]
    fn noncanonical_values_reserved_bytes_and_counts_are_rejected() {
        let source = input();
        for offset in [9, 19] {
            let mut bytes = encode_input(&source).unwrap();
            bytes[offset] = 1;
            assert!(decode_input(&bytes).is_err());
        }
        for offset in [10, 15, 18] {
            let mut bytes = encode_output(&output()).unwrap();
            bytes[offset] = 1;
            assert!(decode_output(&bytes).is_err());
        }
        let mut bytes = encode_input(&source).unwrap();
        bytes[8] = 5;
        assert!(decode_input(&bytes).is_err());
        let mut bytes = encode_input(&source).unwrap();
        bytes[10..12].copy_from_slice(&1001u16.to_le_bytes());
        assert!(decode_input(&bytes).is_err());
        let mut bytes = encode_input(&source).unwrap();
        bytes[14..16].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(decode_input(&bytes).is_err());
        let mut bytes = encode_output(&output()).unwrap();
        bytes[8..10].copy_from_slice(&17u16.to_le_bytes());
        assert!(decode_output(&bytes).is_err());
        for value in [
            Value::Number(-0.),
            Value::Number(f32::NAN),
            Value::Number(f32::INFINITY),
            Value::Series(vec![-0.]),
            Value::TimedSeries(vec![[0., f32::NEG_INFINITY]]),
            Value::Text("a\0b".into()),
        ] {
            let mut source = source.clone();
            source.properties[0].value = value;
            assert!(encode_input(&source).is_err());
        }
        let mut bytes = encode_input(&source).unwrap();
        bytes[68..72].copy_from_slice(&(-0.0f32).to_bits().to_le_bytes());
        assert!(decode_input(&bytes).is_err());
        let mut bytes = encode_input(&source).unwrap();
        *bytes.last_mut().unwrap() = 2;
        assert!(decode_input(&bytes).is_err());
    }
    #[test]
    fn identities_field_order_and_command_uniqueness_are_canonical() {
        let mut source = input();
        source.address.instance.0 = 0;
        assert!(encode_input(&source).is_err());
        let mut source = input();
        source.properties.push(source.properties[0].clone());
        assert!(encode_input(&source).is_err());
        let mut source = input();
        source.properties.insert(
            0,
            Field {
                id: 2,
                value: Value::Number(1.),
            },
        );
        assert!(encode_input(&source).is_err());
        let mut source = input();
        source.address.references = vec![NodeId(1); MAX_REFERENCE_DEPTH + 1];
        assert!(encode_input(&source).is_err());
        for commands in [
            vec![Command::Capture, Command::ReleaseCapture],
            vec![
                Command::Emit {
                    outlet: 1,
                    value: Value::Number(0.)
                };
                2
            ],
            vec![
                Command::SetLocal {
                    property: 1,
                    value: Value::Number(0.)
                };
                2
            ],
            vec![Command::Emit {
                outlet: 0,
                value: Value::Number(0.),
            }],
        ] {
            assert!(encode_output(&Output { commands }).is_err());
        }
        assert_eq!(
            decode_output(
                &encode_output(&Output {
                    commands: vec![Command::ReleaseCapture]
                })
                .unwrap()
            )
            .unwrap()
            .commands,
            vec![Command::ReleaseCapture]
        );
    }
    #[test]
    fn aggregate_byte_budget_is_checked_before_buffer_growth() {
        let mut source = input();
        source.address.references.clear();
        source.local.clear();
        source.properties = vec![
            Field {
                id: 1,
                value: Value::Text("x".repeat(4096)),
            },
            Field {
                id: 2,
                value: Value::Text("y".repeat(4024)),
            },
        ];
        let bytes = encode_input(&source).unwrap();
        assert_eq!(bytes.len(), MAX_MESSAGE_BYTES);
        assert_eq!(decode_input(&bytes).unwrap(), source);
        source.properties[1].value = Value::Text("y".repeat(4025));
        assert!(encode_input(&source).is_err());
        assert!(decode_input(&vec![0; MAX_MESSAGE_BYTES + 1]).is_err());
        let mut output = Output {
            commands: vec![
                Command::Emit {
                    outlet: 1,
                    value: Value::Text("x".repeat(4096)),
                },
                Command::Emit {
                    outlet: 2,
                    value: Value::Text("y".repeat(4040)),
                },
            ],
        };
        let bytes = encode_output(&output).unwrap();
        assert_eq!(bytes.len(), MAX_MESSAGE_BYTES);
        assert_eq!(decode_output(&bytes).unwrap(), output);
        output.commands[1] = Command::Emit {
            outlet: 2,
            value: Value::Text("y".repeat(4041)),
        };
        assert!(encode_output(&output).is_err());
        assert!(decode_output(&vec![0; MAX_MESSAGE_BYTES + 1]).is_err());
    }
}
