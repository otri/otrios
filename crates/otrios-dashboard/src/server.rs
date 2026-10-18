// SPDX-License-Identifier: Apache-2.0
//! Local, unauthenticated research-room server. Not an internet-facing service.
use otrios_core::dashboard::{MAX_DOCUMENT_BYTES, Replica};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tiny_http::{Header, Method, Response, Server};
fn persist(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut bind = "127.0.0.1:8876".to_string();
    let mut state = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => bind = args.next().ok_or("Missing --bind address")?,
            "--state" => state = Some(PathBuf::from(args.next().ok_or("Missing --state file")?)),
            _ => {
                return Err(
                    "Usage: dashboard-server [--bind 127.0.0.1:8876] [--state FILE]".into(),
                );
            }
        }
    }
    let mut replica = if let Some(p) = &state {
        if p.exists() {
            Replica::load(&fs::read(p)?)?
        } else {
            Replica::new()?
        }
    } else {
        Replica::new()?
    };
    let server = Server::http(&bind)?;
    println!("OTriOS dashboard: http://{bind}/");
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dashboard");
    for mut request in server.incoming_requests() {
        let (status, kind, body) = if request.method() == &Method::Post && request.url() == "/sync"
        {
            // Reject cross-site browser writes; this service is same-origin only.
            let origin = request.headers().iter().find(|h| h.field.equiv("Origin"));
            let host = request.headers().iter().find(|h| h.field.equiv("Host"));
            let origin_ok = origin.is_none_or(|o| {
                host.is_some_and(|h| o.value.as_str() == format!("http://{}", h.value.as_str()))
            });
            if !origin_ok {
                (403, "text/plain", b"Origin rejected".to_vec())
            } else {
                let mut body = Vec::new();
                let read = request
                    .as_reader()
                    .take(MAX_DOCUMENT_BYTES as u64 + 1)
                    .read_to_end(&mut body);
                let mut candidate = replica.clone();
                let result = read
                    .map_err(|e| e.to_string())
                    .and_then(|_| candidate.merge(&body));
                match result {
                    Ok(_) => {
                        let bytes = candidate.save();
                        match state.as_ref().map(|p| persist(p, &bytes)).transpose() {
                            Ok(_) => {
                                replica = candidate;
                                (200, "application/octet-stream", bytes)
                            }
                            Err(_) => (
                                500,
                                "text/plain",
                                b"Persistence failed; old state retained".to_vec(),
                            ),
                        }
                    }
                    Err(_) => (
                        400,
                        "text/plain",
                        b"Invalid or oversized dashboard document".to_vec(),
                    ),
                }
            }
        } else if request.method() == &Method::Get {
            let route = request.url().split('?').next().unwrap_or("");
            let asset = match route {
                "/" => Some(("index.html", "text/html; charset=utf-8")),
                "/app.js" => Some(("app.js", "text/javascript")),
                "/style.css" => Some(("style.css", "text/css")),
                "/pkg/otrios_dashboard.js" => Some(("pkg/otrios_dashboard.js", "text/javascript")),
                "/pkg/otrios_dashboard_bg.wasm" => {
                    Some(("pkg/otrios_dashboard_bg.wasm", "application/wasm"))
                }
                _ => None,
            };
            match asset.and_then(|(path, kind)| fs::read(web.join(path)).ok().map(|b| (kind, b))) {
                Some((kind, body)) => (200, kind, body),
                None => (404, "text/plain", b"Not found".to_vec()),
            }
        } else {
            (405, "text/plain", b"Method not allowed".to_vec())
        };
        let response = Response::from_data(body)
            .with_status_code(status)
            .with_header(Header::from_bytes("Content-Type", kind).unwrap())
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
            .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap());
        let _ = request.respond(response);
    }
    Ok(())
}
