// SPDX-License-Identifier: Apache-2.0
//! DNS/TCP/TLS I/O only. URL, destination and HTTP framing policy lives in core.
use otrios_core::transfers::{AppResponse, AppSource, read_app_response};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
struct DeadlineSocket {
    socket: TcpStream,
    deadline: Instant,
}
impl DeadlineSocket {
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "App fetch deadline"))
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(bytes)
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.socket.flush()
    }
}
fn download(
    source: AppSource,
    addresses: Vec<SocketAddr>,
    deadline: Instant,
) -> Result<AppResponse> {
    // Defense in depth: this layer accepts only previously checked/pinned IPs,
    // never asks the connector or proxy environment to resolve a hostname again.
    source.addresses(&addresses.iter().map(|s| s.ip()).collect::<Vec<_>>())?;
    let mut socket = None;
    for address in addresses {
        if address.port() != 443 {
            return Err("App source port rejected".into());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or("App fetch timed out")?;
        if let Ok(stream) =
            TcpStream::connect_timeout(&address, remaining.min(Duration::from_secs(5)))
        {
            socket = Some(stream);
            break;
        }
    }
    let socket = socket.ok_or("App source connection failed")?;
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
    https_request(&source, socket, deadline, config)
}
fn https_request(
    source: &AppSource,
    socket: TcpStream,
    deadline: Instant,
    config: rustls::ClientConfig,
) -> Result<AppResponse> {
    let name = rustls::pki_types::ServerName::try_from(source.host().to_owned())
        .map_err(|_| "Invalid app TLS host")?;
    let connection =
        rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    let mut stream = rustls::StreamOwned::new(connection, DeadlineSocket { socket, deadline });
    stream
        .write_all(source.request().as_bytes())
        .map_err(|_| "App HTTPS request failed")?;
    read_app_response(std::io::BufReader::new(stream))
}
pub(super) async fn fetch(source: &AppSource) -> Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut current = source.clone();
    for redirects in 0..=3 {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("App fetch timed out")?;
        let lookup = tokio::net::lookup_host((current.host(), 443));
        let addresses: Vec<_> = tokio::time::timeout(remaining, lookup)
            .await
            .map_err(|_| "App DNS timed out")?
            .map_err(|_| "App source DNS failed")?
            .take(9)
            .collect();
        current.addresses(&addresses.iter().map(|s| s.ip()).collect::<Vec<_>>())?;
        let hop = current.clone();
        match tokio::task::spawn_blocking(move || download(hop, addresses, deadline))
            .await
            .map_err(|e| e.to_string())??
        {
            AppResponse::Archive(bytes) => return Ok(bytes),
            AppResponse::Redirect(location) if redirects < 3 => {
                current = source.redirect(&location)?
            }
            _ => return Err("App source redirect limit".into()),
        }
    }
    Err("App source redirect limit".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, net::TcpListener, path::Path, process::Command};

    #[test]
    fn app_https_checks_server_identity_and_never_adds_ambient_credentials() {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "otrios-app-tls-{}",
            otrios_core::canvas::hex(&nonce)
        ));
        fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let openssl = std::env::var("OTRIOS_TEST_OPENSSL").unwrap_or_else(|_| {
            if Path::new("/opt/homebrew/opt/openssl@3/bin/openssl").exists() {
                "/opt/homebrew/opt/openssl@3/bin/openssl".into()
            } else {
                "openssl".into()
            }
        });
        let run = |args: &[&str]| {
            let output = Command::new(&openssl)
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&[
            "req",
            "-x509",
            "-newkey",
            "ed25519",
            "-nodes",
            "-subj",
            "/CN=apps.example",
            "-addext",
            "subjectAltName=DNS:apps.example",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-days",
            "1",
            "-keyout",
            "key.pem",
            "-out",
            "cert.der",
            "-outform",
            "DER",
        ]);
        run(&[
            "pkcs8", "-topk8", "-nocrypt", "-in", "key.pem", "-out", "key.der", "-outform", "DER",
        ]);
        let certificate =
            rustls::pki_types::CertificateDer::from(fs::read(root.join("cert.der")).unwrap());
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(fs::read(root.join("key.der")).unwrap());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server = Arc::new(
            rustls::ServerConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![certificate.clone()], key.into())
                .unwrap(),
        );
        let config = || {
            let mut roots = rustls::RootCertStore::empty();
            roots.add(certificate.clone()).unwrap();
            rustls::ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth()
        };
        // Test the TLS boundary on a local fixture only. Production download()
        // rejects these addresses before connecting; there is no policy bypass.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut observed = vec![];
            for _ in 0..2 {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let connection = rustls::ServerConnection::new(server.clone()).unwrap();
                let mut stream = rustls::StreamOwned::new(connection, socket);
                let mut header = vec![];
                loop {
                    let mut byte = [0];
                    if stream.read_exact(&mut byte).is_err() {
                        break;
                    }
                    header.push(byte[0]);
                    assert!(header.len() <= 2048);
                    if header.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                if header.ends_with(b"\r\n\r\n") {
                    observed.push(String::from_utf8(header).unwrap());
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nzip")
                        .unwrap();
                    stream.flush().unwrap();
                }
            }
            observed
        });
        let path = format!("/apps/counter/{}", "a".repeat(64));
        let source = AppSource::parse(&format!("https://apps.example{path}")).unwrap();
        let AppResponse::Archive(bytes) = https_request(
            &source,
            TcpStream::connect(address).unwrap(),
            Instant::now() + Duration::from_secs(5),
            config(),
        )
        .unwrap() else {
            panic!("archive")
        };
        assert_eq!(bytes, b"zip");
        let other = AppSource::parse(&format!("https://other.example{path}")).unwrap();
        assert!(
            https_request(
                &other,
                TcpStream::connect(address).unwrap(),
                Instant::now() + Duration::from_secs(5),
                config()
            )
            .is_err()
        );
        let observed = handle.join().unwrap();
        assert_eq!(
            observed.len(),
            1,
            "hostname failure cannot send an HTTP request"
        );
        assert!(observed[0].starts_with(&format!(
            "GET {path}/download HTTP/1.1\r\nHost: apps.example\r\n"
        )));
        assert!(!observed[0].to_ascii_lowercase().contains("authorization"));
        assert!(!observed[0].to_ascii_lowercase().contains("cookie"));
    }
}
