// SPDX-License-Identifier: Apache-2.0
//! Native HTTP/TLS I/O to the configured authority, never to a supplied app URL.
use otrios_core::transfers::{TransferHttpResponse, read_transfer_response};
use std::{
    io::{BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
pub(super) struct Endpoint {
    host: String,
    authority: String,
    port: u16,
    secure: bool,
    session: String,
    identity: Option<PathBuf>,
    cancel: Option<(Arc<AtomicU64>, u64)>,
}
struct DeadlineSocket {
    socket: TcpStream,
    deadline: Instant,
    cancel: Option<(Arc<AtomicU64>, u64)>,
}
impl DeadlineSocket {
    fn remaining(&self) -> std::io::Result<Duration> {
        if self
            .cancel
            .as_ref()
            .is_some_and(|(epoch, owner)| epoch.load(Ordering::Acquire) != *owner)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Transfer account changed",
            ));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "Transfer timed out"))
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        loop {
            self.socket
                .set_read_timeout(Some(self.remaining()?.min(Duration::from_millis(250))))?;
            match self.socket.read(bytes) {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        loop {
            self.socket
                .set_write_timeout(Some(self.remaining()?.min(Duration::from_millis(250))))?;
            match self.socket.write(bytes) {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.flush()
    }
}
trait Connection: Read + Write {}
impl<T: Read + Write> Connection for T {}
pub(super) struct Request<'a> {
    pub(super) path: &'a str,
    pub(super) origin: Option<&'a str>,
    pub(super) content_type: Option<&'a str>,
    pub(super) bytes: u64,
    pub(super) body: &'a mut dyn Read,
    pub(super) limit: usize,
    pub(super) output: &'a mut dyn Write,
    pub(super) timeout: Duration,
}
impl Endpoint {
    pub(super) fn new(url: &str, identity: Option<PathBuf>) -> Result<Self> {
        let session = super::super::app_stream(url);
        let uri: tungstenite::http::Uri = session.parse().map_err(|_| "Invalid authority URL")?;
        let secure = match uri.scheme_str() {
            Some("wss") => true,
            Some("ws") => false,
            _ => return Err("Authority requires HTTP or HTTPS".into()),
        };
        let authority = uri
            .authority()
            .ok_or("Missing authority host")?
            .as_str()
            .to_owned();
        if authority.contains('@') {
            return Err("Authority credentials must use the identity file".into());
        }
        let host = uri
            .host()
            .ok_or("Missing authority host")?
            .trim_matches(['[', ']'])
            .to_owned();
        Ok(Self {
            host,
            authority,
            port: uri.port_u16().unwrap_or(if secure { 443 } else { 80 }),
            secure,
            session,
            identity,
            cancel: None,
        })
    }
    pub(super) fn set_epoch(&mut self, epoch: Arc<AtomicU64>, owner: u64) {
        self.cancel = Some((epoch, owner));
    }
    pub(super) fn current(&self) -> Result<()> {
        if self
            .cancel
            .as_ref()
            .is_some_and(|(epoch, owner)| epoch.load(Ordering::Acquire) != *owner)
        {
            Err("Transfer account changed".into())
        } else {
            Ok(())
        }
    }
    fn connect(&self, deadline: Instant) -> Result<Box<dyn Connection>> {
        self.current()?;
        let addresses: Vec<_> = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|_| "Authority lookup failed")?
            .take(9)
            .collect();
        if addresses.is_empty()
            || addresses.len() > 8
            || !self.secure && addresses.iter().any(|a| !a.ip().is_loopback())
        {
            return Err("Plain HTTP transfers require a loopback authority".into());
        }
        let mut connected = None;
        for address in addresses {
            self.current()?;
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("Transfer timed out")?;
            if let Ok(socket) =
                TcpStream::connect_timeout(&address, remaining.min(Duration::from_secs(5)))
            {
                connected = Some(socket);
                break;
            }
        }
        let socket = DeadlineSocket {
            socket: connected.ok_or("Authority connection failed")?,
            deadline,
            cancel: self.cancel.clone(),
        };
        if !self.secure {
            return Ok(Box::new(socket));
        }
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(self.host.clone())
            .map_err(|_| "Invalid authority TLS host")?;
        let connection =
            rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
        Ok(Box::new(rustls::StreamOwned::new(connection, socket)))
    }
    pub(super) fn request(&self, request: Request<'_>) -> Result<TransferHttpResponse> {
        if !request.path.starts_with("/transfers/")
            || !request
                .path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
        {
            return Err("Invalid transfer target".into());
        }
        let admission =
            super::super::authenticated_request(&self.session, self.identity.as_deref())?;
        let authorization = admission
            .headers()
            .get(tungstenite::http::header::AUTHORIZATION)
            .map(|v| v.to_str().map_err(|_| "Invalid native admission"))
            .transpose()?;
        let deadline = Instant::now() + request.timeout;
        let mut stream = self.connect(deadline)?;
        let mut header = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept-Encoding: identity\r\nX-OTriOS-Request: transfers\r\n",
            if request.content_type.is_some() {
                "POST"
            } else {
                "GET"
            },
            request.path,
            self.authority
        );
        if let Some(token) = authorization {
            header.push_str(&format!("Authorization: {token}\r\n"));
        }
        if let Some(origin) = request.origin {
            if origin.contains(['\r', '\n']) {
                return Err("Invalid transfer origin".into());
            }
            header.push_str(&format!("Origin: {origin}\r\n"));
        }
        if let Some(media) = request.content_type {
            if media.contains(['\r', '\n']) {
                return Err("Invalid transfer media type".into());
            }
            header.push_str(&format!(
                "Content-Type: {media}\r\nContent-Length: {}\r\n",
                request.bytes
            ));
        }
        header.push_str("\r\n");
        stream
            .write_all(header.as_bytes())
            .map_err(|_| "Transfer request failed")?;
        let copied = std::io::copy(&mut request.body.take(request.bytes), &mut stream)
            .map_err(|_| "Transfer upload failed")?;
        if copied != request.bytes {
            return Err("Transfer file changed during upload".into());
        }
        stream.flush().map_err(|_| "Transfer request failed")?;
        read_transfer_response(BufReader::new(stream), request.limit, &mut {
            request.output
        })
    }
}
