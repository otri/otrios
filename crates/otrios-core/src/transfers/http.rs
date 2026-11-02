// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded HTTP/1 framing for the narrow HTTPS app fetch adapter.
use super::*;
use std::io::{Read, Write};

pub enum AppResponse {
    Archive(Vec<u8>),
    Redirect(String),
}
fn line(input: &mut impl Read, remaining: &mut usize) -> Result<String> {
    let mut out = Vec::new();
    loop {
        if *remaining == 0 {
            return Err("App response framing budget".into());
        }
        let mut byte = [0];
        input
            .read_exact(&mut byte)
            .map_err(|_| "Incomplete app HTTP response")?;
        *remaining -= 1;
        out.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    if !out.ends_with(b"\r\n")
        || out[..out.len() - 2]
            .iter()
            .any(|b| !matches!(b, b'\t' | 32..=126))
    {
        return Err("Invalid app HTTP response line".into());
    }
    out.truncate(out.len() - 2);
    String::from_utf8(out).map_err(|_| "Invalid app HTTP header".into())
}
fn head(input: &mut impl Read) -> Result<(u16, std::collections::BTreeMap<String, String>)> {
    let mut header_budget = 16 * 1024;
    let status = line(input, &mut header_budget)?;
    let mut status = status.splitn(3, ' ');
    if !matches!(status.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
        return Err("Unsupported app HTTP response".into());
    }
    let code = status.next().ok_or("Missing app HTTP status")?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Invalid transfer HTTP status".into());
    }
    let code = code
        .parse::<u16>()
        .map_err(|_| "Invalid transfer HTTP status")?;
    if !(100..=599).contains(&code) {
        return Err("Invalid transfer HTTP status".into());
    }
    let mut headers = std::collections::BTreeMap::new();
    loop {
        let field = line(input, &mut header_budget)?;
        if field.is_empty() {
            break;
        }
        let (name, value) = field.split_once(':').ok_or("Invalid app HTTP header")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        {
            return Err("Invalid app HTTP header name".into());
        }
        let name = name.to_ascii_lowercase();
        let value = value.trim().to_string();
        // Unrelated repeatable fields (e.g. Set-Cookie) are ignored entirely.
        if matches!(
            name.as_str(),
            "content-length"
                | "transfer-encoding"
                | "content-encoding"
                | "location"
                | "content-type"
        ) && headers.insert(name, value).is_some()
        {
            return Err("Ambiguous app HTTP framing".into());
        }
    }
    Ok((code, headers))
}
fn body(
    input: &mut impl Read,
    headers: &std::collections::BTreeMap<String, String>,
    limit: usize,
    output: &mut impl Write,
) -> Result<usize> {
    if headers
        .get("content-encoding")
        .is_some_and(|v| !v.eq_ignore_ascii_case("identity"))
    {
        return Err("Compressed app HTTP response rejected".into());
    }
    let length = headers
        .get("content-length")
        .map(|s| {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Invalid app HTTP length");
            }
            s.parse::<usize>().map_err(|_| "Invalid app HTTP length")
        })
        .transpose()?;
    if length.is_some_and(|n| n > limit) {
        return Err("App download byte budget".into());
    }
    let mut bytes: usize = 0;
    let copy = |input: &mut dyn Read, output: &mut dyn Write, count: usize| -> Result<()> {
        let mut remaining = count;
        let mut buffer = [0; 65536];
        while remaining > 0 {
            let n = input
                .read(&mut buffer[..remaining.min(65536)])
                .map_err(|_| "Incomplete transfer body")?;
            if n == 0 {
                return Err("Incomplete transfer body".into());
            }
            output
                .write_all(&buffer[..n])
                .map_err(|_| "Transfer sink failed")?;
            remaining -= n;
        }
        Ok(())
    };
    if let Some(encoding) = headers.get("transfer-encoding") {
        if length.is_some() || !encoding.eq_ignore_ascii_case("chunked") {
            return Err("Ambiguous or unsupported HTTP encoding".into());
        }
        let mut framing_budget = 256 * 1024;
        loop {
            let size = line(input, &mut framing_budget)?;
            if size.is_empty() || size.len() > 8 || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Invalid transfer HTTP chunk".into());
            }
            let size =
                usize::from_str_radix(&size, 16).map_err(|_| "Invalid transfer HTTP chunk")?;
            if size == 0 {
                if !line(input, &mut framing_budget)?.is_empty() {
                    return Err("HTTP trailers unsupported".into());
                }
                break;
            }
            bytes = bytes
                .checked_add(size)
                .filter(|n| *n <= limit)
                .ok_or("Transfer body budget")?;
            copy(input, output, size)?;
            if !line(input, &mut framing_budget)?.is_empty() {
                return Err("Invalid HTTP chunk ending".into());
            }
        }
    } else if let Some(length) = length {
        copy(input, output, length)?;
        bytes = length;
    } else {
        let mut buffer = [0; 65536];
        loop {
            let n = input
                .read(&mut buffer)
                .map_err(|_| "Incomplete transfer body")?;
            if n == 0 {
                break;
            }
            bytes = bytes
                .checked_add(n)
                .filter(|n| *n <= limit)
                .ok_or("Transfer body budget")?;
            output
                .write_all(&buffer[..n])
                .map_err(|_| "Transfer sink failed")?;
        }
    }
    Ok(bytes)
}

/// Response bytes stream only into an isolated host sink. Non-success responses
/// return their status without copying any body or following a redirect.
pub struct TransferHttpResponse {
    pub status: u16,
    pub media: Option<String>,
    pub bytes: usize,
}
pub fn read_transfer_response(
    mut input: impl Read,
    limit: usize,
    mut output: impl Write,
) -> Result<TransferHttpResponse> {
    let (status, headers) = head(&mut input)?;
    let bytes = if status == 200 {
        body(&mut input, &headers, limit, &mut output)?
    } else {
        0
    };
    Ok(TransferHttpResponse {
        status,
        media: headers.get("content-type").cloned(),
        bytes,
    })
}
pub fn read_app_response(mut input: impl Read) -> Result<AppResponse> {
    let (status, mut headers) = head(&mut input)?;
    if matches!(status, 301 | 302 | 303 | 307 | 308) {
        return Ok(AppResponse::Redirect(
            headers.remove("location").ok_or("Missing app redirect")?,
        ));
    }
    if status != 200 {
        return Err("App source did not return a download".into());
    }
    let mut bytes = Vec::new();
    body(&mut input, &headers, MAX_APP_ARCHIVE_BYTES, &mut bytes)?;
    Ok(AppResponse::Archive(bytes))
}
