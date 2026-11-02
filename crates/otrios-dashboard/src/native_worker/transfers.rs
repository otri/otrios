// SPDX-License-Identifier: Apache-2.0
//! One bounded I/O worker. Shared core owns admission, reviews and commits.
#[path = "transfers/folder.rs"]
mod folder;
#[path = "transfers/http.rs"]
mod http;
use http::{Endpoint, Request};
use otrios_interfaces::transfers::*;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
pub(super) enum Event {
    Ready(String),
    Reply(u32, Box<TransferReply>),
}
pub(super) struct Worker {
    send: mpsc::SyncSender<(u64, TransferTask)>,
    receive: mpsc::Receiver<Event>,
    epoch: Arc<AtomicU64>,
    actor: Option<String>,
}
impl Worker {
    pub(super) fn start(url: &str, path: &Path) -> Result<Self> {
        let folder = folder::Folder::bind(path)?;
        let endpoint = Endpoint::new(
            url,
            std::env::var_os("OTRIOS_IDENTITY_FILE").map(Into::into),
        )?;
        let (send, tasks) = mpsc::sync_channel(1);
        let (replies, receive) = mpsc::sync_channel(2);
        let epoch = Arc::new(AtomicU64::new(1));
        let current = epoch.clone();
        thread::Builder::new()
            .name("transfers".into())
            .spawn(move || {
                run(folder, endpoint, tasks, replies, current);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            send,
            receive,
            epoch,
            actor: None,
        })
    }
    pub(super) fn actor(&mut self, actor: Option<&str>) {
        if self.actor.as_deref() != actor {
            self.actor = actor.map(str::to_owned);
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
    }
    pub(super) fn submit(&self, task: TransferTask) -> Option<(u32, TransferReply)> {
        let id = task.id;
        self.send
            .try_send((self.epoch.load(Ordering::Acquire), task))
            .err()
            .map(|_| {
                (
                    id,
                    TransferReply::Failed {
                        message: "Finish the current transfer and try again.".into(),
                    },
                )
            })
    }
    pub(super) fn poll(&self) -> Option<Event> {
        self.receive.try_recv().ok()
    }
}
fn context(endpoint: &Endpoint) -> Result<String> {
    let mut bytes = Vec::new();
    let response = endpoint.request(Request {
        path: "/transfers/context",
        origin: None,
        content_type: None,
        bytes: 0,
        body: &mut std::io::empty(),
        limit: 16 * 1024,
        output: &mut bytes,
        timeout: Duration::from_secs(3),
    })?;
    if response.status != 200 || response.media.as_deref() != Some("application/json") {
        return Err("Native transfer admission unavailable".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid transfer context")?;
    let origin = value["origin"]
        .as_str()
        .ok_or("Public transfer origin unavailable")?;
    let source = otrios_core::transfers::AppSource::parse(&format!(
        "{origin}/apps/sample/{}",
        "0".repeat(64)
    ))?;
    if source.download_url().split("/apps/").next() != Some(origin) {
        return Err("Invalid transfer origin".into());
    }
    Ok(origin.into())
}
fn run(
    mut folder: folder::Folder,
    mut endpoint: Endpoint,
    tasks: mpsc::Receiver<(u64, TransferTask)>,
    replies: mpsc::SyncSender<Event>,
    epoch: Arc<AtomicU64>,
) {
    let mut origin = None;
    let mut retry = Instant::now();
    let mut seen = epoch.load(Ordering::Acquire);
    loop {
        let current = epoch.load(Ordering::Acquire);
        if seen != current {
            folder.clear();
            seen = current;
        }
        if origin.is_none() && Instant::now() >= retry {
            endpoint.set_epoch(epoch.clone(), current);
            retry = Instant::now() + Duration::from_secs(5);
            if let Ok(value) = context(&endpoint) {
                origin = Some(value.clone());
                if replies.send(Event::Ready(value)).is_err() {
                    break;
                }
            }
        }
        match tasks.recv_timeout(Duration::from_millis(250)) {
            Ok((owner, task)) => {
                if owner != epoch.load(Ordering::Acquire) {
                    continue;
                }
                endpoint.set_epoch(epoch.clone(), owner);
                let result = origin
                    .as_deref()
                    .ok_or_else(|| "Transfer admission unavailable".into())
                    .and_then(|origin| perform(&mut folder, &endpoint, origin, &task));
                let reply = result.unwrap_or_else(|message| TransferReply::Failed { message });
                if owner == epoch.load(Ordering::Acquire)
                    && replies
                        .send(Event::Reply(task.id, Box::new(reply)))
                        .is_err()
                {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
    }
}
fn perform(
    folder: &mut folder::Folder,
    endpoint: &Endpoint,
    origin: &str,
    task: &TransferTask,
) -> Result<TransferReply> {
    let app = matches!(task.target, TransferTarget::App { .. });
    let path = task.target.path();
    match &task.io {
        TransferIo::Pick => Ok(TransferReply::Files {
            files: folder.list(app)?,
        }),
        TransferIo::Copy { .. } => {
            Err("Select the displayed URL to copy it on another device.".into())
        }
        TransferIo::Export => {
            let TransferTarget::Data { dataset } = &task.target else {
                return Err("Select app data to export".into());
            };
            let mut download = folder.download(&dataset.app)?;
            let response = endpoint.request(Request {
                path: &path,
                origin: None,
                content_type: None,
                bytes: 0,
                body: &mut std::io::empty(),
                limit: download.limit,
                output: &mut download.file,
                timeout: Duration::from_secs(120),
            })?;
            accepted(response.status)?;
            if response.media.as_deref() != Some(DATA_MEDIA_TYPE) {
                return Err("Invalid data download".into());
            }
            endpoint.current()?;
            Ok(TransferReply::Exported {
                name: folder.finish(download)?,
            })
        }
        TransferIo::File { file } => {
            let (mut file, bytes) = folder.selected(file, app)?;
            intake(
                endpoint,
                origin,
                &path,
                if app { APP_MEDIA_TYPE } else { DATA_MEDIA_TYPE },
                bytes,
                &mut file,
                app,
            )
        }
        TransferIo::Url { url } => {
            if !app || url.len() > 512 {
                return Err("Use an immutable public HTTPS app URL".into());
            }
            intake(
                endpoint,
                origin,
                &path,
                "text/plain; charset=utf-8",
                url.len() as u64,
                &mut url.as_bytes(),
                true,
            )
        }
    }
}
fn accepted(status: u16) -> Result<()> {
    match status {
        200 => Ok(()),
        401 | 403 => {
            Err("Space Admin admission changed. Close this review and sign in again.".into())
        }
        409 => Err("The Space or app changed. Close this review and try again.".into()),
        413 => Err("The file exceeds the transfer size limit.".into()),
        429 | 503 => Err("Another transfer is busy. Try again shortly.".into()),
        _ => Err("The authority could not qualify this transfer.".into()),
    }
}
fn intake(
    endpoint: &Endpoint,
    origin: &str,
    path: &str,
    media: &str,
    bytes: u64,
    body: &mut dyn std::io::Read,
    app: bool,
) -> Result<TransferReply> {
    let mut output = Vec::new();
    let response = endpoint.request(Request {
        path,
        origin: Some(origin),
        content_type: Some(media),
        bytes,
        body,
        limit: 16 * 1024,
        output: &mut output,
        timeout: Duration::from_secs(45),
    })?;
    accepted(response.status)?;
    if response.media.as_deref() != Some("application/json") {
        return Err("Invalid transfer review".into());
    }
    if app {
        Ok(TransferReply::App {
            review: serde_json::from_slice(&output).map_err(|_| "Invalid app review")?,
        })
    } else {
        Ok(TransferReply::Data {
            review: serde_json::from_slice(&output).map_err(|_| "Invalid data review")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
    };
    fn read_request(socket: &mut TcpStream) -> (String, Vec<u8>) {
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut input = BufReader::new(socket);
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            assert!(!line.is_empty());
            headers.push_str(&line);
            if line == "\r\n" {
                break;
            }
            assert!(headers.len() < 8192);
        }
        let length = headers
            .lines()
            .find_map(|s| s.strip_prefix("Content-Length: "))
            .map_or(0, |n| n.parse::<usize>().unwrap());
        let mut body = vec![0; length];
        input.read_exact(&mut body).unwrap();
        (headers, body)
    }
    fn target() -> TransferTarget {
        TransferTarget::Data {
            dataset: DatasetRef {
                space: "b".repeat(32),
                app: "counter".into(),
                collection: "c".repeat(32),
                schema: "e".repeat(64),
            },
        }
    }
    #[test]
    fn native_export_picker_upload_round_trip_streams_and_preserves_exact_review_integers() {
        let fixture = folder::tests::Fixture::new();
        fixture.put("identity", "f".repeat(64).as_bytes());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            Some(fixture.0.join("identity")),
        )
        .unwrap();
        let expected = DataReview {
            stage: "a".repeat(32),
            replacement: Replacement {
                space: "b".repeat(32),
                app: "counter".into(),
                collection: "c".repeat(32),
                replacement: "d".repeat(32),
                expected_generation: u64::MAX,
            },
            audience: otrios_interfaces::spaces::Audience::Closed,
            schema_sha256: "e".repeat(64),
            archive_sha256: "f".repeat(64),
            bytes: 7,
            expires_unix_ms: u64::MAX,
        };
        let review = serde_json::to_vec(&expected).unwrap();
        let server = thread::spawn(move || {
            for step in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                let (headers, body) = read_request(&mut socket);
                assert!(headers.contains(&format!("Authorization: Bearer {}\r\n", "f".repeat(64))));
                assert!(headers.contains("X-OTriOS-Request: transfers\r\n"));
                if step == 0 {
                    assert!(headers.starts_with("GET /transfers/context HTTP/1.1\r\n"));
                    assert!(!headers.contains("Origin:"));
                    let context = br#"{"origin":"https://otrios.example"}"#;
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", context.len()).unwrap();
                    socket.write_all(context).unwrap();
                } else if step == 1 {
                    assert!(headers.starts_with("GET /transfers/data/"));
                    assert!(!headers.contains("Origin:"));
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {DATA_MEDIA_TYPE}\r\nTransfer-Encoding: chunked\r\n\r\n3\r\npay\r\n4\r\nload\r\n0\r\n\r\n").unwrap();
                } else {
                    assert!(headers.starts_with("POST /transfers/data/"));
                    assert!(headers.contains("Origin: https://otrios.example\r\n"));
                    assert!(headers.contains(&format!("Content-Type: {DATA_MEDIA_TYPE}\r\n")));
                    assert_eq!(body, b"payload");
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", review.len()).unwrap();
                    socket.write_all(&review).unwrap();
                }
            }
        });
        let origin = context(&endpoint).unwrap();
        let mut folder = folder::Folder::bind(&fixture.0).unwrap();
        let task = |id, io| TransferTask {
            id,
            target: target(),
            io,
        };
        let TransferReply::Exported { name } = perform(
            &mut folder,
            &endpoint,
            &origin,
            &task(1, TransferIo::Export),
        )
        .unwrap() else {
            panic!("no export")
        };
        assert_eq!(std::fs::read(fixture.0.join(&name)).unwrap(), b"payload");
        let TransferReply::Files { files } =
            perform(&mut folder, &endpoint, &origin, &task(2, TransferIo::Pick)).unwrap()
        else {
            panic!("no files")
        };
        assert_eq!(files.len(), 1);
        assert_eq!(
            perform(
                &mut folder,
                &endpoint,
                &origin,
                &task(
                    3,
                    TransferIo::File {
                        file: files[0].id.clone()
                    }
                )
            )
            .unwrap(),
            TransferReply::Data { review: expected }
        );
        server.join().unwrap();
    }
    #[test]
    fn incomplete_wrong_media_redirect_and_over_budget_downloads_never_publish_files() {
        let fixture = folder::tests::Fixture::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint =
            Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), None).unwrap();
        let server = thread::spawn(move || {
            for response in [
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {DATA_MEDIA_TYPE}\r\nContent-Length: 20\r\n\r\nshort"
                ),
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 3\r\n\r\nbad"
                    .into(),
                "HTTP/1.1 302 Found\r\nLocation: https://evil.example/\r\n\r\n".into(),
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {DATA_MEDIA_TYPE}\r\nContent-Length: {}\r\n\r\n",
                    MAX_DATA_ARCHIVE_BYTES + 1
                ),
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                read_request(&mut socket);
                socket.write_all(response.as_bytes()).unwrap();
            }
        });
        let mut folder = folder::Folder::bind(&fixture.0).unwrap();
        for id in 1..=4 {
            assert!(
                perform(
                    &mut folder,
                    &endpoint,
                    "https://otrios.example",
                    &TransferTask {
                        id,
                        target: target(),
                        io: TransferIo::Export
                    }
                )
                .is_err()
            );
            assert!(folder.list(false).unwrap().is_empty());
            assert_eq!(
                std::fs::read_dir(&fixture.0).unwrap().count(),
                1,
                "only owner lock remains"
            );
        }
        server.join().unwrap();
    }

    #[test]
    fn account_change_interrupts_a_stalled_native_download_and_erases_its_temporary_file() {
        let fixture = folder::tests::Fixture::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut endpoint =
            Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), None).unwrap();
        let epoch = Arc::new(AtomicU64::new(1));
        endpoint.set_epoch(epoch.clone(), 1);
        let (started, start) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            read_request(&mut socket);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: {DATA_MEDIA_TYPE}\r\nContent-Length: 100\r\n\r\nx"
            )
            .unwrap();
            socket.flush().unwrap();
            started.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let mut folder = folder::Folder::bind(&fixture.0).unwrap();
        let (done, result) = mpsc::sync_channel(1);
        let operation = thread::spawn(move || {
            let response = perform(
                &mut folder,
                &endpoint,
                "https://otrios.example",
                &TransferTask {
                    id: 1,
                    target: target(),
                    io: TransferIo::Export,
                },
            );
            assert!(response.is_err());
            assert!(folder.list(false).unwrap().is_empty());
            done.send(()).unwrap();
        });
        start.recv_timeout(Duration::from_secs(3)).unwrap();
        epoch.store(2, Ordering::Release);
        result
            .recv_timeout(Duration::from_secs(3))
            .expect("cancel must not wait for the 120s download deadline");
        operation.join().unwrap();
        release.send(()).unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
    }
}
