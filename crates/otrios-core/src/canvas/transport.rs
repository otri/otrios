// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded sync-v4 packets. Canonical Merkle, UI and schema bytes are independent.
use super::*;
use std::cell::RefCell;
const DIGEST_BYTES: usize = 32;
#[derive(Default)]
struct Workspace {
    compressed: Vec<u8>,
    decoded: Vec<u8>,
    // Keep the cache-line-aligned table on the heap: the pinned macOS TLS
    // storage must not inherit that over-alignment from an inline enum.
    table: Option<Box<lz4_flex::block::CompressTable>>,
    grows: u64,
}
thread_local! { static WORKSPACE: RefCell<Workspace> = RefCell::new(Workspace::default()); }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct WireUsage {
    pub compression_capacity: usize,
    pub decode_capacity: usize,
    pub table_capacity_bound: usize,
    pub buffer_grows: u64,
}
/// Per-current-thread reusable scratch, not packet ownership or process RSS.
pub fn wire_usage() -> WireUsage {
    WORKSPACE.with(|cell| {
        let w = cell.borrow();
        WireUsage {
            compression_capacity: w.compressed.capacity(),
            decode_capacity: w.decoded.capacity(),
            table_capacity_bound: if w.table.is_some() { 16 * 1024 } else { 0 },
            buffer_grows: w.grows,
        }
    })
}
fn prepare(buffer: &mut Vec<u8>, size: usize, max: usize) -> Result<bool> {
    if size > max {
        return Err("Canvas codec workspace limit".into());
    }
    let grew = buffer.capacity() < size;
    if grew {
        let class = [512, 2048, 8192, 32768, 131072, max]
            .into_iter()
            .find(|n| *n >= size)
            .ok_or("Canvas workspace class")?;
        *buffer = Vec::with_capacity(class);
    }
    buffer.resize(size, 0);
    Ok(grew)
}
/// Compress one raw packet as an independent LZ4 block only if it saves at least
/// 64 bytes and 12.5%, including its SHA-256 integrity field. Does not mutate a
/// failed/small candidate, use dictionaries, or nest compressed packets.
pub fn compress_packet_into(raw: &[u8], output: &mut Vec<u8>) -> Result<bool> {
    decoded_packet_bytes(raw)?;
    if raw[4] != 0 || raw[HEADER] >= 32 {
        return Err("Compression requires a raw outer Canvas packet".into());
    }
    output.clear();
    output.extend_from_slice(raw);
    compress_in_place(output)
}
fn compress_in_place(raw: &mut Vec<u8>) -> Result<bool> {
    let decoded = decoded_packet_bytes(raw)?;
    if raw[4] != 0 {
        return Err("Canvas packet already compressed".into());
    }
    if decoded < 512 {
        return Ok(false);
    }
    WORKSPACE.with(|cell| {
        let mut w = cell.try_borrow_mut().map_err(err)?;
        let bound = lz4_flex::block::get_maximum_output_size(decoded);
        let max = lz4_flex::block::get_maximum_output_size(MAX_BYTES - HEADER);
        w.grows += u64::from(prepare(&mut w.compressed, bound, max)?);
        w.table
            .get_or_insert_with(|| Box::new(lz4_flex::block::CompressTable::small()));
        let Workspace {
            compressed, table, ..
        } = &mut *w;
        let length = lz4_flex::block::compress_into_with_table(
            &raw[HEADER..],
            compressed,
            table
                .as_deref_mut()
                .ok_or("Missing compression workspace")?,
        )
        .map_err(err)?;
        let total = HEADER + DIGEST_BYTES + length;
        if total + 64 > raw.len() || total > raw.len() - raw.len() / 8 {
            return Ok(false);
        }
        let digest = Sha256::digest(&raw[HEADER..]);
        raw.truncate(HEADER);
        raw[4] = 1;
        raw.extend_from_slice(&digest);
        raw.extend_from_slice(&compressed[..length]);
        Ok(true)
    })
}

use otrios_interfaces::layout::Value;
const MAGIC: &[u8; 4] = b"OTC4";
const HEADER: usize = 10;
const MAX_REF: usize = 128;
const MAX_CODES: usize = 8;

struct Writer<'a>(&'a mut Vec<u8>);
impl Writer<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        if self.0.len().saturating_add(bytes.len()) > MAX_BYTES {
            return Err("Canvas packed size limit".into());
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn u8(&mut self, v: u8) -> Result<()> {
        self.put(&[v])
    }
    fn u16(&mut self, v: u16) -> Result<()> {
        self.put(&v.to_le_bytes())
    }
    fn u32(&mut self, v: u32) -> Result<()> {
        self.put(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) -> Result<()> {
        self.put(&v.to_le_bytes())
    }
    fn fixed(&mut self, bytes: &[u8], len: usize) -> Result<()> {
        if bytes.len() != len {
            return Err("Canvas fixed field length".into());
        }
        self.put(bytes)
    }
    fn blob(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_BYTES {
            return Err("Canvas blob limit".into());
        }
        self.u32(bytes.len() as u32)?;
        self.put(bytes)
    }
    fn text(&mut self, text: &str, limit: usize) -> Result<()> {
        if text.len() > limit {
            return Err("Canvas text limit".into());
        }
        self.blob(text.as_bytes())
    }
    fn value(&mut self, v: &Value) -> Result<()> {
        match v {
            Value::Number(n) => {
                self.u8(0)?;
                self.put(&n.to_le_bytes())
            }
            Value::Text(s) => {
                self.u8(1)?;
                self.text(s, MAX_TEXT_BYTES)
            }
            Value::Boolean(b) => {
                self.u8(2)?;
                self.u8(u8::from(*b))
            }
            Value::Color(c) => {
                self.u8(3)?;
                self.put(c)
            }
            Value::Series(values) => {
                if values.len() > MAX_SERIES_POINTS {
                    return Err("Canvas series limit".into());
                }
                self.u8(4)?;
                self.u16(values.len() as u16)?;
                for n in values {
                    self.put(&n.to_le_bytes())?;
                }
                Ok(())
            }
            Value::TimedSeries(values) => {
                if values.len() > MAX_SERIES_POINTS {
                    return Err("Canvas series limit".into());
                }
                self.u8(5)?;
                self.u16(values.len() as u16)?;
                for point in values {
                    for n in point {
                        self.put(&n.to_le_bytes())?;
                    }
                }
                Ok(())
            }
        }
    }
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.0.len() {
            return Err("Truncated Canvas packet".into());
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().map_err(err)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn blob(&mut self, limit: usize) -> Result<Vec<u8>> {
        let n = self.u32()? as usize;
        if n > limit {
            return Err("Canvas blob limit".into());
        }
        Ok(self.take(n)?.to_vec())
    }
    fn text(&mut self, limit: usize) -> Result<String> {
        String::from_utf8(self.blob(limit)?).map_err(err)
    }
    fn value(&mut self) -> Result<Value> {
        Ok(match self.u8()? {
            0 => Value::Number(f32::from_le_bytes(self.array()?)),
            1 => Value::Text(self.text(MAX_TEXT_BYTES)?),
            2 => Value::Boolean(match self.u8()? {
                0 => false,
                1 => true,
                _ => return Err("Invalid Canvas boolean".into()),
            }),
            3 => Value::Color(self.array()?),
            kind @ (4 | 5) => {
                let count = self.u16()? as usize;
                let width = if kind == 4 { 4 } else { 8 };
                if count > MAX_SERIES_POINTS || count * width > self.0.len() {
                    return Err("Canvas series length limit".into());
                }
                if kind == 4 {
                    let mut values = Vec::with_capacity(count);
                    for _ in 0..count {
                        values.push(f32::from_le_bytes(self.array()?));
                    }
                    Value::Series(values)
                } else {
                    let mut values = Vec::with_capacity(count);
                    for _ in 0..count {
                        values.push([
                            f32::from_le_bytes(self.array()?),
                            f32::from_le_bytes(self.array()?),
                        ]);
                    }
                    Value::TimedSeries(values)
                }
            }
            _ => return Err("Unknown Canvas value kind".into()),
        })
    }
    fn finish(self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err("Trailing Canvas packet bytes".into())
        }
    }
}
/// Upper bound checked before any typed payload allocation. Reserved flags fail
/// closed; compression is introduced as an independent bounded codec increment.
pub fn decoded_packet_bytes(bytes: &[u8]) -> Result<usize> {
    if bytes.len() < HEADER + 1
        || bytes.len() > MAX_BYTES
        || &bytes[..4] != MAGIC
        || bytes[4] > 1
        || bytes[5] != 0
    {
        return Err("Unsupported Canvas packet header".into());
    }
    let length = u32::from_le_bytes(bytes[6..10].try_into().map_err(err)?) as usize;
    if length == 0 || length > MAX_BYTES - HEADER {
        return Err("Canvas packet length limit".into());
    }
    if (bytes[4] == 0 && length != bytes.len() - HEADER)
        || (bytes[4] == 1
            && (bytes.len() <= HEADER + DIGEST_BYTES || bytes.len() >= HEADER + length))
    {
        return Err("Canvas encoded packet length".into());
    }
    Ok(length)
}
fn read<T>(bytes: &[u8], parse: impl FnOnce(&mut Reader<'_>) -> Result<T>) -> Result<T> {
    let length = decoded_packet_bytes(bytes)?;
    if bytes[4] == 0 {
        let mut r = Reader(&bytes[HEADER..]);
        let value = parse(&mut r)?;
        r.finish()?;
        return Ok(value);
    }
    WORKSPACE.with(|cell| {
        let mut w = cell.try_borrow_mut().map_err(err)?;
        w.grows += u64::from(prepare(&mut w.decoded, length, MAX_BYTES - HEADER)?);
        let actual =
            lz4_flex::block::decompress_into(&bytes[HEADER + DIGEST_BYTES..], &mut w.decoded)
                .map_err(err)?;
        if actual != length
            || Sha256::digest(&w.decoded).as_slice() != &bytes[HEADER..HEADER + DIGEST_BYTES]
        {
            return Err("Canvas decompressed integrity mismatch".into());
        }
        let mut r = Reader(&w.decoded);
        let value = parse(&mut r)?;
        r.finish()?;
        Ok(value)
    })
}

fn write(
    out: &mut Vec<u8>,
    tag: u8,
    encode: impl FnOnce(&mut Writer<'_>) -> Result<()>,
) -> Result<()> {
    out.clear();
    out.extend_from_slice(&[b'O', b'T', b'C', b'4', 0, 0, 0, 0, 0, 0]);
    let mut writer = Writer(out);
    writer.u8(tag)?;
    encode(&mut writer)?;
    let length = (out.len() - HEADER) as u32;
    out[6..10].copy_from_slice(&length.to_le_bytes());
    Ok(())
}
macro_rules! owned {
    ($name:ident,$into:ident,$ty:ty) => {
        pub fn $name(value: &$ty) -> Result<Vec<u8>> {
            let mut bytes = Vec::with_capacity(128);
            $into(value, &mut bytes)?;
            Ok(bytes)
        }
    };
}
owned!(encode_client, encode_client_into, ClientMessage);
owned!(encode_server, encode_server_into, ServerMessage);
owned!(
    encode_state_request,
    encode_state_request_into,
    StateRequest
);
owned!(
    encode_state_response,
    encode_state_response_into,
    StateResponse
);

pub fn encode_client_into(value: &ClientMessage, out: &mut Vec<u8>) -> Result<()> {
    let tag = match value {
        ClientMessage::Mount { .. } => 1,
        ClientMessage::AcceptMount { .. } => 2,
        ClientMessage::CancelMount { .. } => 3,
        ClientMessage::Sync { .. } => 4,
        ClientMessage::Leave { .. } => 5,
    };
    write(out, tag, |w| match value {
        ClientMessage::Mount {
            epoch,
            ui_ref,
            state_ref,
            catalog_hash,
            known_code,
            writer,
            resume,
        } => {
            w.u32(*epoch)?;
            w.text(ui_ref, MAX_REF)?;
            w.text(state_ref, MAX_REF)?;
            w.fixed(catalog_hash, 32)?;
            if known_code.len() > MAX_CODES {
                return Err("Canvas code cache count".into());
            }
            w.u8(known_code.len() as u8)?;
            for hash in known_code {
                w.fixed(hash, 32)?;
            }
            w.fixed(writer, 16)?;
            w.u64(*resume)
        }
        ClientMessage::Sync { epoch, bytes } => {
            w.u32(*epoch)?;
            w.blob(bytes)
        }
        ClientMessage::AcceptMount { epoch }
        | ClientMessage::CancelMount { epoch }
        | ClientMessage::Leave { epoch } => w.u32(*epoch),
    })
}
pub fn decode_client(bytes: &[u8]) -> Result<ClientMessage> {
    read(bytes, |r| {
        let tag = r.u8()?;
        let epoch = r.u32()?;
        let message = match tag {
            1 => {
                let ui_ref = r.text(MAX_REF)?;
                let state_ref = r.text(MAX_REF)?;
                let catalog_hash = r.take(32)?.to_vec();
                let count = r.u8()? as usize;
                if count > MAX_CODES || count * 32 > r.0.len() {
                    return Err("Canvas code cache count".into());
                }
                let mut known_code = Vec::with_capacity(count);
                for _ in 0..count {
                    known_code.push(r.take(32)?.to_vec());
                }
                ClientMessage::Mount {
                    epoch,
                    ui_ref,
                    state_ref,
                    catalog_hash,
                    known_code,
                    writer: r.take(16)?.to_vec(),
                    resume: r.u64()?,
                }
            }
            2 => ClientMessage::AcceptMount { epoch },
            3 => ClientMessage::CancelMount { epoch },
            4 => ClientMessage::Sync {
                epoch,
                bytes: r.blob(MAX_BYTES)?,
            },
            5 => ClientMessage::Leave { epoch },
            _ => return Err("Unknown Canvas client tag".into()),
        };
        Ok(message)
    })
}
pub fn encode_server_into(value: &ServerMessage, out: &mut Vec<u8>) -> Result<()> {
    let tag = match value {
        ServerMessage::View { .. } => 17,
        ServerMessage::Mounted { .. } => 18,
        ServerMessage::Sync { .. } => 19,
        ServerMessage::AccessRequired { .. } => 20,
        ServerMessage::Error { .. } => 21,
    };
    write(out, tag, |w| match value {
        ServerMessage::View { epoch, bytes } | ServerMessage::Sync { epoch, bytes } => {
            w.u32(*epoch)?;
            w.blob(bytes)
        }
        ServerMessage::Mounted {
            epoch,
            ui,
            ui_hash,
            code,
            state,
        } => {
            w.u32(*epoch)?;
            w.blob(ui)?;
            w.fixed(ui_hash, 32)?;
            w.blob(code)?;
            w.blob(state)
        }
        ServerMessage::AccessRequired { epoch } => w.u32(*epoch),
        ServerMessage::Error { epoch, message } => {
            w.u32(*epoch)?;
            w.text(message, 2048)
        }
    })
}
pub fn decode_server(bytes: &[u8]) -> Result<ServerMessage> {
    read(bytes, |r| {
        let tag = r.u8()?;
        let epoch = r.u32()?;
        let message = match tag {
            17 => ServerMessage::View {
                epoch,
                bytes: r.blob(MAX_BYTES)?,
            },
            18 => ServerMessage::Mounted {
                epoch,
                ui: r.blob(MAX_BYTES)?,
                ui_hash: r.take(32)?.to_vec(),
                code: r.blob(MAX_BYTES)?,
                state: r.blob(MAX_BYTES)?,
            },
            19 => ServerMessage::Sync {
                epoch,
                bytes: r.blob(MAX_BYTES)?,
            },
            20 => ServerMessage::AccessRequired { epoch },
            21 => ServerMessage::Error {
                epoch,
                message: r.text(2048)?,
            },
            _ => return Err("Unknown Canvas server tag".into()),
        };
        Ok(message)
    })
}
pub fn encode_state_request_into(value: &StateRequest, out: &mut Vec<u8>) -> Result<()> {
    let tag = match value {
        StateRequest::Hello { .. } => 33,
        StateRequest::Write { .. } => 34,
        StateRequest::Ack { .. } => 35,
        StateRequest::Resync => 36,
    };
    write(out, tag, |w| match value {
        StateRequest::Hello { writer, resume } => {
            w.fixed(writer, 16)?;
            w.u64(*resume)
        }
        StateRequest::Write { sequence, values } => {
            if values.len() > MAX_FIELDS {
                return Err("Canvas command batch limit".into());
            }
            w.u64(*sequence)?;
            w.u8(values.len() as u8)?;
            for (id, value) in values {
                w.u16(*id)?;
                w.value(value)?;
            }
            Ok(())
        }
        StateRequest::Ack {
            authority,
            lease,
            sequence,
            root,
        } => {
            w.u64(*authority)?;
            w.u64(*lease)?;
            w.u64(*sequence)?;
            w.fixed(root, 32)
        }
        StateRequest::Resync => Ok(()),
    })
}
pub fn decode_state_request(bytes: &[u8]) -> Result<StateRequest> {
    if bytes.get(4) != Some(&0) {
        return Err("Compressed inner state envelope is forbidden".into());
    }
    read(bytes, |r| {
        let message = match r.u8()? {
            33 => StateRequest::Hello {
                writer: r.take(16)?.to_vec(),
                resume: r.u64()?,
            },
            34 => {
                let sequence = r.u64()?;
                let count = r.u8()? as usize;
                if count > MAX_FIELDS || count * 3 > r.0.len() {
                    return Err("Canvas command batch limit".into());
                }
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push((r.u16()?, r.value()?));
                }
                StateRequest::Write { sequence, values }
            }
            35 => StateRequest::Ack {
                authority: r.u64()?,
                lease: r.u64()?,
                sequence: r.u64()?,
                root: r.take(32)?.to_vec(),
            },
            36 => StateRequest::Resync,
            _ => return Err("Unknown Canvas request tag".into()),
        };
        Ok(message)
    })
}
pub fn encode_state_response_into(value: &StateResponse, out: &mut Vec<u8>) -> Result<()> {
    let tag = match value {
        StateResponse::Mount { .. } => 49,
        StateResponse::Update { .. } => 50,
    };
    write(out, tag, |w| match value {
        StateResponse::Mount {
            authority,
            lease,
            frame,
        } => {
            w.u64(*authority)?;
            w.u64(*lease)?;
            w.blob(frame)
        }
        StateResponse::Update { frame } => w.blob(frame),
    })
}
pub fn decode_state_response(bytes: &[u8]) -> Result<StateResponse> {
    if bytes.get(4) != Some(&0) {
        return Err("Compressed inner state envelope is forbidden".into());
    }
    read(bytes, |r| {
        let message = match r.u8()? {
            49 => StateResponse::Mount {
                authority: r.u64()?,
                lease: r.u64()?,
                frame: r.blob(MAX_BYTES)?,
            },
            50 => StateResponse::Update {
                frame: r.blob(MAX_BYTES)?,
            },
            _ => return Err("Unknown Canvas response tag".into()),
        };
        Ok(message)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check_roundtrip<T: std::fmt::Debug + PartialEq>(
        value: &T,
        encode: fn(&T) -> Result<Vec<u8>>,
        decode: fn(&[u8]) -> Result<T>,
    ) {
        let bytes = encode(value).unwrap();
        assert_eq!(*value, decode(&bytes).unwrap());
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end]).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode(&extra).is_err());
        let length = (extra.len() - HEADER) as u32;
        extra[6..10].copy_from_slice(&length.to_le_bytes());
        assert!(decode(&extra).is_err());
        for index in [0, 4, 5] {
            let mut bad = bytes.clone();
            bad[index] = 255;
            assert!(decode(&bad).is_err());
        }
    }
    #[test]
    fn all_variants_and_typed_values_roundtrip_and_reject_partial_or_extra_packets() {
        for value in [
            ClientMessage::Mount {
                epoch: 1,
                ui_ref: "ui".into(),
                state_ref: "state".into(),
                catalog_hash: vec![1; 32],
                known_code: vec![vec![2; 32]],
                writer: vec![3; 16],
                resume: 9,
            },
            ClientMessage::AcceptMount { epoch: 1 },
            ClientMessage::CancelMount { epoch: 2 },
            ClientMessage::Sync {
                epoch: 3,
                bytes: vec![1, 2, 3],
            },
            ClientMessage::Leave { epoch: 4 },
        ] {
            check_roundtrip(&value, encode_client, decode_client);
        }
        for value in [
            ServerMessage::View {
                epoch: 1,
                bytes: vec![1; 256],
            },
            ServerMessage::Mounted {
                epoch: 1,
                ui: vec![2; 128],
                ui_hash: vec![3; 32],
                code: vec![4; 128],
                state: vec![5; 256],
            },
            ServerMessage::Sync {
                epoch: 2,
                bytes: vec![7; 256],
            },
            ServerMessage::AccessRequired { epoch: 3 },
            ServerMessage::Error {
                epoch: 4,
                message: "denied".into(),
            },
        ] {
            check_roundtrip(&value, encode_server, decode_server);
        }
        let values = vec![
            (1, Value::Number(123.)),
            (2, Value::Text("typed 🦀".into())),
            (3, Value::Boolean(true)),
            (4, Value::Color([1, 2, 3, 4])),
            (5, Value::Series(vec![0., 1., 2.])),
            (6, Value::TimedSeries(vec![[0., 1.], [0.5, 2.]])),
        ];
        for value in [
            StateRequest::Hello {
                writer: vec![1; 16],
                resume: 10,
            },
            StateRequest::Write {
                sequence: 11,
                values,
            },
            StateRequest::Ack {
                authority: 1,
                lease: 2,
                sequence: 3,
                root: vec![4; 32],
            },
            StateRequest::Resync,
        ] {
            check_roundtrip(&value, encode_state_request, decode_state_request);
        }
        for value in [
            StateResponse::Mount {
                authority: 1,
                lease: 2,
                frame: vec![1; 256],
            },
            StateResponse::Update {
                frame: vec![2; 256],
            },
        ] {
            check_roundtrip(&value, encode_state_response, decode_state_response);
        }
    }
    #[test]
    fn fixed_endianness_and_direction_tags_are_explicit() {
        let bytes = encode_client(&ClientMessage::AcceptMount { epoch: 0x01020304 }).unwrap();
        assert_eq!(hex(&bytes), "4f5443340000050000000204030201");
        assert!(decode_server(&bytes).is_err());
        assert!(decode_state_request(&bytes).is_err());
        assert!(decode_state_response(&bytes).is_err());
    }
    #[test]
    fn declared_lengths_and_counts_are_checked_before_materializing_payloads() {
        let mut bytes = encode_state_request(&StateRequest::Write {
            sequence: 1,
            values: vec![],
        })
        .unwrap();
        bytes[HEADER + 1 + 8] = 65;
        assert!(decode_state_request(&bytes).is_err());
        let mut bytes = encode_server(&ServerMessage::View {
            epoch: 1,
            bytes: vec![],
        })
        .unwrap();
        bytes[HEADER + 1 + 4..HEADER + 1 + 4 + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_server(&bytes).is_err());
        bytes[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_server(&bytes).is_err());
        let mut bytes = encode_state_request(&StateRequest::Write {
            sequence: 1,
            values: vec![(1, Value::Boolean(true))],
        })
        .unwrap();
        *bytes.last_mut().unwrap() = 2;
        assert!(decode_state_request(&bytes).is_err());
        let mut bytes = encode_state_request(&StateRequest::Write {
            sequence: 1,
            values: vec![(1, Value::Text("x".into()))],
        })
        .unwrap();
        *bytes.last_mut().unwrap() = 255;
        assert!(decode_state_request(&bytes).is_err());
        assert!(
            encode_state_request(&StateRequest::Hello {
                writer: vec![0; 17],
                resume: 0
            })
            .is_err()
        );
        assert!(
            encode_server(&ServerMessage::View {
                epoch: 1,
                bytes: vec![0; MAX_BYTES]
            })
            .is_err()
        );
        assert!(
            encode_state_request(&StateRequest::Write {
                sequence: 1,
                values: vec![(1, Value::TimedSeries(vec![[0., 0.]; MAX_SERIES_POINTS + 1]))]
            })
            .is_err()
        );
    }
    #[test]
    fn compression_is_independent_checked_and_reuses_bounded_scratch() {
        let value = ServerMessage::View {
            epoch: 1,
            bytes: vec![7; 2048],
        };
        let raw = encode_server(&value).unwrap();
        assert_eq!(raw[4], 0);
        let mut compressed = Vec::new();
        assert!(compress_packet_into(&raw, &mut compressed).unwrap());
        assert!(compressed.len() + 64 < raw.len());
        assert_eq!(decode_server(&compressed).unwrap(), value);
        let usage = wire_usage();
        for _ in 0..32 {
            assert!(compress_packet_into(&raw, &mut compressed).unwrap());
            assert_eq!(decode_server(&compressed).unwrap(), value);
            assert_eq!(wire_usage(), usage);
        }
        for end in 0..compressed.len() {
            assert!(decode_server(&compressed[..end]).is_err());
        }
        for index in [HEADER, compressed.len() - 1] {
            let mut bad = compressed.clone();
            bad[index] ^= 255;
            assert!(decode_server(&bad).is_err());
        }
        let mut bad = compressed.clone();
        bad[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_server(&bad).is_err());
        assert_eq!(wire_usage(), usage, "oversize must not grow scratch");
        let small = encode_server(&ServerMessage::AccessRequired { epoch: 2 }).unwrap();
        assert!(!compress_packet_into(&small, &mut compressed).unwrap());
        assert_eq!(compressed, small, "no retained prior packet suffix");
    }
    #[test]
    fn incompressible_packets_stay_raw_and_inner_packets_cannot_compress() {
        let mut seed = 1234567u32;
        let payload = (0..2048)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        let raw = encode_server(&ServerMessage::View {
            epoch: 1,
            bytes: payload,
        })
        .unwrap();
        let mut out = Vec::new();
        assert!(!compress_packet_into(&raw, &mut out).unwrap());
        assert_eq!(raw, out);
        let mut inner = encode_state_response(&StateResponse::Update {
            frame: vec![0; 2048],
        })
        .unwrap();
        assert!(compress_packet_into(&inner, &mut out).is_err());
        assert!(compress_in_place(&mut inner).unwrap());
        assert!(
            decode_state_response(&inner).is_err(),
            "nested expansion forbidden"
        );
        let mut request = encode_state_request(&StateRequest::Write {
            sequence: 1,
            values: vec![(1, Value::Text("a".repeat(2048)))],
        })
        .unwrap();
        assert!(compress_in_place(&mut request).unwrap());
        assert!(decode_state_request(&request).is_err());
    }
    #[test]
    fn decoded_byte_rate_is_charged_before_expansion_and_backwards_time_cannot_reset_it() {
        let packet = encode_server(&ServerMessage::View {
            epoch: 1,
            bytes: vec![0; 32768],
        })
        .unwrap();
        let mut compressed = Vec::new();
        assert!(compress_packet_into(&packet, &mut compressed).unwrap());
        let packet = compressed;
        assert_eq!(packet[4], 1);
        let charged = decoded_packet_bytes(&packet).unwrap() + HEADER;
        let mut budget = DecodeBudget::default();
        let count = DecodeBudget::BYTES_PER_SECOND / charged;
        for _ in 0..count {
            assert_eq!(budget.admit(&packet, 5000).unwrap(), charged);
        }
        assert!(budget.admit(&packet, 5000).is_err());
        assert!(budget.admit(&packet, 0).is_err());
        assert!(budget.admit(&packet, 5999).is_err());
        assert!(budget.admit(&packet, 6000).is_ok());
        let mut invalid = packet;
        invalid[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(budget.admit(&invalid, 7000).is_err());
    }
    #[test]
    fn reusable_builders_do_not_grow_or_leave_old_packet_suffixes() {
        let mut out = Vec::new();
        encode_server_into(
            &ServerMessage::View {
                epoch: 1,
                bytes: vec![1; 16384],
            },
            &mut out,
        )
        .unwrap();
        let capacity = out.capacity();
        for _ in 0..10 {
            encode_server_into(&ServerMessage::AccessRequired { epoch: 2 }, &mut out).unwrap();
            assert_eq!(out.len(), HEADER + 5);
            assert_eq!(out.capacity(), capacity);
            assert_eq!(
                decode_server(&out).unwrap(),
                ServerMessage::AccessRequired { epoch: 2 }
            );
        }
    }
}

/// Decode work is charged by declared uncompressed bytes before decompression.
/// Hosts supply monotonic milliseconds; backwards clocks cannot reset a window.
#[derive(Default)]
pub struct DecodeBudget {
    clock: u64,
    start: Option<u64>,
    charged: usize,
}
impl DecodeBudget {
    pub const BYTES_PER_SECOND: usize = 8 * 1024 * 1024;
    pub fn admit(&mut self, packet: &[u8], now: u64) -> Result<usize> {
        let bytes = decoded_packet_bytes(packet)?
            .checked_add(HEADER)
            .ok_or("Canvas decode charge overflow")?;
        self.clock = self.clock.max(now);
        if self
            .start
            .is_none_or(|at| self.clock.saturating_sub(at) >= 1000)
        {
            self.start = Some(self.clock);
            self.charged = 0;
        }
        let next = self
            .charged
            .checked_add(bytes)
            .ok_or("Canvas decode charge overflow")?;
        if next > Self::BYTES_PER_SECOND {
            return Err("Canvas decoded-byte rate limit".into());
        }
        self.charged = next;
        Ok(bytes)
    }
}
