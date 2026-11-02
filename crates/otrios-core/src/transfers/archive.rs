// SPDX-License-Identifier: LGPL-3.0-only
//! Strict flat ZIP codec. An owned tree is still untrusted until payload qualification.
use super::*;
use crate::bundle::{BundleTree, valid_path};
use flate2::{Crc, Decompress, FlushDecompress, Status};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    App,
    Data,
}
impl Kind {
    fn archive(self) -> usize {
        match self {
            Self::App => MAX_APP_ARCHIVE_BYTES,
            Self::Data => MAX_DATA_ARCHIVE_BYTES,
        }
    }
    fn expanded(self) -> usize {
        match self {
            Self::App => crate::bundle::MAX_TOTAL_BYTES,
            Self::Data => MAX_DATA_BYTES,
        }
    }
    fn file(self) -> usize {
        match self {
            Self::App => crate::bundle::MAX_FILE_BYTES,
            Self::Data => MAX_CHECKPOINT_BYTES,
        }
    }
}
fn invalid<T>() -> Result<T> {
    Err("Invalid portable ZIP structure".into())
}
fn part(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    bytes
        .get(at..at.checked_add(len).ok_or("ZIP offset overflow")?)
        .ok_or("Truncated ZIP".into())
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        part(bytes, at, 2)?.try_into().map_err(|_| "ZIP integer")?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        part(bytes, at, 4)?.try_into().map_err(|_| "ZIP integer")?,
    ))
}
fn crc(bytes: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(bytes);
    crc.sum()
}
#[derive(Debug)]
struct Entry {
    name: String,
    local: usize,
    compressed: usize,
    size: usize,
    crc: u32,
    method: u16,
    flags: u16,
    needed: u16,
    time: u16,
    date: u16,
}
fn name(bytes: &[u8], names: &mut BTreeSet<String>) -> Result<String> {
    let name = std::str::from_utf8(bytes).map_err(|_| "ZIP filename encoding")?;
    if !valid_path(name) || !names.insert(name.to_ascii_lowercase()) {
        return invalid();
    }
    Ok(name.into())
}

/// Parse the complete local/central closure before decoding any entry. No paths
/// are extracted to a caller's filesystem, and no archive permissions are applied.
pub fn decode(kind: Kind, bytes: &[u8]) -> Result<BundleTree> {
    if bytes.len() < 22 || bytes.len() > kind.archive() {
        return invalid();
    }
    let end = bytes.len() - 22;
    if u32_at(bytes, end)? != 0x0605_4b50
        || u16_at(bytes, end + 4)? != 0
        || u16_at(bytes, end + 6)? != 0
        || u16_at(bytes, end + 20)? != 0
    {
        return invalid();
    }
    let count = usize::from(u16_at(bytes, end + 10)?);
    if count == 0
        || count > crate::bundle::MAX_FILES
        || usize::from(u16_at(bytes, end + 8)?) != count
    {
        return invalid();
    }
    let central = u32_at(bytes, end + 16)? as usize;
    let central_size = u32_at(bytes, end + 12)? as usize;
    if central.checked_add(central_size) != Some(end) {
        return invalid();
    }
    let mut entries = Vec::with_capacity(count);
    let mut names = BTreeSet::new();
    let mut at = central;
    let mut expanded = 0usize;
    for _ in 0..count {
        part(bytes, at, 46)?;
        let needed = u16_at(bytes, at + 6)?;
        let flags = u16_at(bytes, at + 8)?;
        let method = u16_at(bytes, at + 10)?;
        let name_len = usize::from(u16_at(bytes, at + 28)?);
        let extra = u16_at(bytes, at + 30)?;
        let comment = u16_at(bytes, at + 32)?;
        let external = u32_at(bytes, at + 38)?;
        let file_type = (external >> 16) & 0xf000;
        if u32_at(bytes, at)? != 0x0201_4b50
            || !matches!(needed, 10 | 20)
            || flags & !0x080e != 0
            || !matches!(method, 0 | 8)
            || method == 0 && flags & 6 != 0
            || method == 8 && needed != 20
            || flags & 8 != 0 && needed != 20
            || name_len == 0
            || name_len > crate::bundle::MAX_PATH_BYTES
            || extra != 0
            || comment != 0
            || u16_at(bytes, at + 34)? != 0
            || !matches!(file_type, 0 | 0x8000)
            || external & 0x18 != 0
        {
            return invalid();
        }
        let name = name(part(bytes, at + 46, name_len)?, &mut names)?;
        let size = u32_at(bytes, at + 24)? as usize;
        let compressed = u32_at(bytes, at + 20)? as usize;
        expanded = expanded.checked_add(size).ok_or("ZIP expanded budget")?;
        if size > kind.file()
            || expanded > kind.expanded()
            || compressed > kind.archive()
            || method == 0 && size != compressed
        {
            return Err("Portable ZIP budget/method mismatch".into());
        }
        entries.push(Entry {
            name,
            local: u32_at(bytes, at + 42)? as usize,
            compressed,
            size,
            crc: u32_at(bytes, at + 16)?,
            method,
            flags,
            needed,
            time: u16_at(bytes, at + 12)?,
            date: u16_at(bytes, at + 14)?,
        });
        at = at.checked_add(46 + name_len).ok_or("ZIP offset overflow")?;
        if at > end {
            return invalid();
        }
    }
    if at != end {
        return invalid();
    }
    entries.sort_by_key(|entry| entry.local);
    let mut spans = Vec::with_capacity(count);
    let mut expected = 0;
    for (index, entry) in entries.iter().enumerate() {
        let at = entry.local;
        let next = entries.get(index + 1).map_or(central, |entry| entry.local);
        if at != expected || at >= central {
            return invalid();
        }
        part(bytes, at, 30)?;
        let name_len = usize::from(u16_at(bytes, at + 26)?);
        if u32_at(bytes, at)? != 0x0403_4b50
            || u16_at(bytes, at + 4)? != entry.needed
            || u16_at(bytes, at + 6)? != entry.flags
            || u16_at(bytes, at + 8)? != entry.method
            || u16_at(bytes, at + 10)? != entry.time
            || u16_at(bytes, at + 12)? != entry.date
            || name_len != entry.name.len()
            || u16_at(bytes, at + 28)? != 0
            || part(bytes, at + 30, name_len)? != entry.name.as_bytes()
        {
            return invalid();
        }
        let local = [
            u32_at(bytes, at + 14)?,
            u32_at(bytes, at + 18)?,
            u32_at(bytes, at + 22)?,
        ];
        let actual = [entry.crc, entry.compressed as u32, entry.size as u32];
        let descriptor = entry.flags & 8 != 0;
        if local != actual && !(descriptor && local == [0; 3]) {
            return invalid();
        }
        let data = at.checked_add(30 + name_len).ok_or("ZIP offset overflow")?;
        let data_end = data
            .checked_add(entry.compressed)
            .ok_or("ZIP offset overflow")?;
        if data_end > next {
            return invalid();
        }
        let tail = next - data_end;
        if descriptor {
            let offset = match tail {
                12 => data_end,
                16 if u32_at(bytes, data_end)? == 0x0807_4b50 => data_end + 4,
                _ => return invalid(),
            };
            if [
                u32_at(bytes, offset)?,
                u32_at(bytes, offset + 4)?,
                u32_at(bytes, offset + 8)?,
            ] != actual
            {
                return invalid();
            }
        } else if tail != 0 {
            return invalid();
        }
        spans.push(data);
        expected = next;
    }
    let mut tree = BundleTree::new();
    for (entry, start) in entries.into_iter().zip(spans) {
        let compressed = part(bytes, start, entry.compressed)?;
        let contents = if entry.method == 0 {
            compressed.to_vec()
        } else {
            let mut decoder = Decompress::new(false);
            // One sentinel byte detects a stream that exceeds its claimed size.
            let mut output = vec![0; entry.size.checked_add(1).ok_or("ZIP output budget")?];
            let status = decoder
                .decompress(compressed, &mut output, FlushDecompress::Finish)
                .map_err(|_| "Invalid raw Deflate")?;
            if status != Status::StreamEnd
                || decoder.total_in() != entry.compressed as u64
                || decoder.total_out() != entry.size as u64
            {
                return invalid();
            }
            output.truncate(entry.size);
            output
        };
        if crc(&contents) != entry.crc {
            return Err("ZIP CRC32 mismatch".into());
        }
        tree.insert(entry.name, contents);
    }
    Ok(tree)
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, value: usize) -> Result<()> {
    out.extend(
        u32::try_from(value)
            .map_err(|_| "ZIP integer budget")?
            .to_le_bytes(),
    );
    Ok(())
}
/// Deterministic Stored ZIP encoding. This function confers no publisher trust.
pub fn encode(kind: Kind, tree: &BundleTree) -> Result<Vec<u8>> {
    if tree.is_empty() || tree.len() > crate::bundle::MAX_FILES {
        return invalid();
    }
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    for (path, data) in tree {
        name(path.as_bytes(), &mut names)?;
        total = total.checked_add(data.len()).ok_or("ZIP byte budget")?;
        if data.len() > kind.file() || total > kind.expanded() {
            return Err("Portable ZIP byte budget".into());
        }
    }
    let mut output = Vec::new();
    let mut central = Vec::new();
    for (path, data) in tree {
        let start = output.len();
        let crc = crc(data);
        output.extend(0x0403_4b50u32.to_le_bytes());
        for value in [10, 0x0800, 0, 0, 33] {
            put16(&mut output, value);
        }
        output.extend(crc.to_le_bytes());
        put32(&mut output, data.len())?;
        put32(&mut output, data.len())?;
        put16(&mut output, path.len() as u16);
        put16(&mut output, 0);
        output.extend(path.as_bytes());
        output.extend(data);
        central.extend(0x0201_4b50u32.to_le_bytes());
        for value in [(3 << 8) | 20, 10, 0x0800, 0, 0, 33] {
            put16(&mut central, value);
        }
        central.extend(crc.to_le_bytes());
        put32(&mut central, data.len())?;
        put32(&mut central, data.len())?;
        for value in [path.len() as u16, 0, 0, 0, 0] {
            put16(&mut central, value);
        }
        central.extend((0o100644u32 << 16).to_le_bytes());
        put32(&mut central, start)?;
        central.extend(path.as_bytes());
    }
    let start = output.len();
    let size = central.len();
    output.extend(central);
    output.extend(0x0605_4b50u32.to_le_bytes());
    for value in [0, 0, tree.len() as u16, tree.len() as u16] {
        put16(&mut output, value);
    }
    put32(&mut output, size)?;
    put32(&mut output, start)?;
    put16(&mut output, 0);
    if output.len() > kind.archive() {
        return Err("Portable ZIP archive budget".into());
    }
    Ok(output)
}
