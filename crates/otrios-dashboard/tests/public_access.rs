// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "server")]
use otrios_core::dashboard::{Replica, SyncSession, layout::Layout, motion::Intent};
use otrios_interfaces::layout::{Expr, Property, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{Message, client::IntoClientRequest};
const ORIGIN: &str = "https://otrios.otri.ca";
#[test]
fn public_and_native_ink_share_only_ephemeral_points_with_server_assigned_peers() {
    use otrios_interfaces::ink::{self, Point};
    let room = room_with_auth(false, 28800);
    let private = private_view(&room);
    let state = std::fs::read(room.root.join("state.automerge")).ok();
    let mut bad = format!("ws://{}/demo/ink", room.public)
        .into_client_request()
        .unwrap();
    bad.headers_mut()
        .insert("Origin", "https://unrelated.example".parse().unwrap());
    assert!(
        matches!(tungstenite::connect(bad),Err(tungstenite::Error::Http(r)) if r.status().as_u16()==403)
    );
    let mut a = socket(&room.public, "/demo/ink", ORIGIN);
    let mut b = socket(&room.private, "/demo/ink", ORIGIN);
    let read = |s: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>| {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match s.read() {
                Ok(Message::Binary(b)) => return b,
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    assert!(Instant::now() < until)
                }
                other => panic!("Unexpected ink message: {other:?}"),
            }
        }
    };
    let first = ink::decode_server(&read(&mut a)).unwrap();
    let second = ink::decode_server(&read(&mut b)).unwrap();
    assert!(first.2.is_none());
    assert!(second.2.is_none());
    assert_ne!(first.0, second.0);
    let points = [
        Point {
            phase: 0,
            stroke: 1,
            position: [100, 200],
        },
        Point {
            phase: 1,
            stroke: 1,
            position: [400, 600],
        },
        Point {
            phase: 2,
            stroke: 1,
            position: [800, 900],
        },
    ];
    a.send(Message::Binary(
        points
            .iter()
            .copied()
            .flat_map(Point::encode)
            .collect::<Vec<_>>()
            .into(),
    ))
    .unwrap();
    let live = read(&mut b);
    assert_eq!(live.len(), points.len() * ink::SERVER_BYTES);
    for (bytes, point) in live.chunks_exact(ink::SERVER_BYTES).zip(points) {
        let (peer, _, p) = ink::decode_server(bytes).unwrap();
        assert_eq!(peer, first.0);
        assert_eq!(p, Some(point));
    }
    let mut c = socket(&room.public, "/demo/ink", ORIGIN);
    read(&mut c);
    let history = read(&mut c);
    assert_eq!(history.len(), live.len());
    std::thread::sleep(Duration::from_millis(5200));
    let mut late = socket(&room.public, "/demo/ink", ORIGIN);
    read(&mut late);
    assert!(
        matches!(late.read(),Err(tungstenite::Error::Io(e)) if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut))
    );
    assert_eq!(std::fs::read(room.root.join("state.automerge")).ok(), state);
    assert_eq!(
        request(&room.private, "GET", "/presentation", &[], &[]).1,
        private
    );
}
struct Room {
    child: Child,
    private: String,
    public: String,
    root: PathBuf,
}
impl Drop for Room {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}
fn request(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.0\r\nHost: {address}\r\nContent-Length: {}\r\n",
        body.len()
    )
    .unwrap();
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").unwrap();
    }
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(body).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let split = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let status = std::str::from_utf8(&bytes[..split])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, bytes[split + 4..].to_vec())
}
fn room_with_auth(auth: bool, seconds: u64) -> Room {
    let root = std::env::temp_dir().join(format!(
        "otrios-public-{}-{}",
        std::process::id(),
        port().replace(':', "-")
    ));
    std::fs::create_dir_all(&root).unwrap();
    let private = port();
    let public = port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_dashboard-server"));
    command.args([
        "--bind",
        &private,
        "--public-bind",
        &public,
        "--origin",
        ORIGIN,
        "--state",
        root.join("state.automerge").to_str().unwrap(),
    ]);
    if auth {
        use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
        let salt = SaltString::encode_b64(b"test-only-otrios-salt").unwrap();
        let hash = Argon2::default()
            .hash_password(b"test-only-password", &salt)
            .unwrap()
            .to_string();
        let path = root.join("auth.json");
        std::fs::write(
            &path,
            serde_json::json!({"username":"test-user","password_hash":hash}).to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        command.args([
            "--auth-file",
            path.to_str().unwrap(),
            "--session-seconds",
            &seconds.to_string(),
        ]);
    }
    let child = command.stdout(Stdio::null()).spawn().unwrap();
    let mut room = Room {
        child,
        private,
        public,
        root,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut socket) = TcpStream::connect(&room.public) {
            socket
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let _ = socket.write_all(b"GET /health HTTP/1.0\r\n\r\n");
            let mut response = String::new();
            if socket.read_to_string(&mut response).is_ok()
                && response.contains("\"status\":\"ok\"")
            {
                break;
            }
        }
        assert!(room.child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    room
}
fn socket(
    address: &str,
    path: &str,
    origin: &str,
) -> tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>> {
    let mut request = format!("ws://{address}{path}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", origin.parse().unwrap());
    let (mut socket, _) = tungstenite::connect(request).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
    }
    socket
}
fn private_view(room: &Room) -> Vec<u8> {
    let mut b = Layout::bootstrap().unwrap().blueprint().clone();
    let i = b.root.children.iter_mut().find(|i| i.id == 2).unwrap();
    i.properties.retain(|p| p.key != 1);
    i.properties.push(Property {
        key: 1,
        value: Expr::Literal(Value::Text("PRIVATE MONITORING CANARY".into())),
    });
    let bytes = Layout::encode(&b).unwrap();
    assert_eq!(
        request(
            &room.private,
            "POST",
            "/presentation",
            &[
                ("Content-Type", "application/octet-stream"),
                ("Origin", ORIGIN)
            ],
            &bytes
        )
        .0,
        200
    );
    bytes
}
#[test]
fn anonymous_demo_stream_shares_frame_state_but_never_sends_monitoring_or_admin_routes() {
    let room = room_with_auth(false, 28800);
    let private = private_view(&room);
    for path in [
        "/presentation",
        "/layout",
        "/sync",
        "/releases/abc/manifest.json",
    ] {
        assert_eq!(request(&room.public, "GET", path, &[], &[]).0, 404);
    }
    assert!(
        !String::from_utf8(request(&room.public, "GET", "/health", &[], &[]).1)
            .unwrap()
            .contains("presentation")
    );
    let mut invalid = format!("ws://{}/stream", room.public)
        .into_client_request()
        .unwrap();
    invalid
        .headers_mut()
        .insert("Origin", "https://unrelated.example".parse().unwrap());
    assert!(
        matches!(tungstenite::connect(invalid),Err(tungstenite::Error::Http(r)) if r.status().as_u16()==403)
    );
    let mut socket = socket(&room.public, "/stream", ORIGIN);
    let mut demo = Replica::new().unwrap();
    demo.set_motion(Intent {
        count: 48,
        speed: 2.,
        ..Default::default()
    })
    .unwrap();
    demo.set_control(0, [830, 0]).unwrap();
    demo.set_control(1, [210, 0]).unwrap();
    demo.set_control(2, [120, 940]).unwrap();
    let mut session = SyncSession::default();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut synced = false;
    while Instant::now() < deadline {
        if let Some(bytes) = demo.generate_sync(&mut session).unwrap() {
            socket.send(Message::Binary(bytes.into())).unwrap();
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                assert!(!bytes.starts_with(b"OTML1"));
                assert!(!bytes.windows(private.len()).any(|w| w == private));
                demo.receive_sync(&mut session, &bytes).unwrap();
            }
            Ok(Message::Ping(_)) => socket.flush().unwrap(),
            Ok(Message::Text(_) | Message::Pong(_)) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            other => panic!("unexpected {other:?}"),
        }
        let saved =
            Replica::load(&std::fs::read(room.root.join("state.automerge")).unwrap()).unwrap();
        if saved.motion().unwrap().count == 48
            && saved.controls().unwrap().level == 830
            && saved.controls().unwrap().dial == 210
            && saved.controls().unwrap().pad == [120, 940]
        {
            synced = true;
            break;
        }
    }
    assert!(synced);
    assert_eq!(
        request(&room.private, "GET", "/presentation", &[], &[]).1,
        private
    );
}

#[test]
fn acknowledged_edits_survive_authority_restart_and_fresh_ram_only_subscribers() {
    fn sync_client(room: &Room, client: &mut Replica) {
        let mut wire = socket(&room.public, "/demo/stream", ORIGIN);
        let mut session = SyncSession::default();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(bytes) = client.generate_sync(&mut session).unwrap() {
                wire.send(Message::Binary(bytes.into())).unwrap();
            }
            match wire.read() {
                Ok(Message::Binary(bytes)) => {
                    assert!(!bytes.starts_with(b"OTML1"));
                    client.receive_sync(&mut session, &bytes).unwrap();
                }
                Ok(Message::Ping(_)) => wire.flush().unwrap(),
                Ok(Message::Text(_) | Message::Pong(_)) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                other => panic!("Unexpected sync {other:?}"),
            }
            if client.is_confirmed(&session) {
                break;
            }
            assert!(Instant::now() < until, "Server acknowledgement timed out");
        }
    }
    let mut room = room_with_auth(false, 28800);
    let mut client = Replica::new().unwrap();
    client.set_control(0, [735, 0]).unwrap();
    client.set_control(2, [321, 654]).unwrap();
    sync_client(&room, &mut client);
    let saved = Replica::load(&std::fs::read(room.root.join("state.automerge")).unwrap()).unwrap();
    assert_eq!(saved.controls().unwrap(), client.controls().unwrap());
    drop(client);
    room.child.kill().unwrap();
    room.child.wait().unwrap();
    room.child = Command::new(env!("CARGO_BIN_EXE_dashboard-server"))
        .args([
            "--bind",
            &room.private,
            "--public-bind",
            &room.public,
            "--origin",
            ORIGIN,
            "--state",
            room.root.join("state.automerge").to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(&room.public).is_err() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut fresh = Replica::new().unwrap();
    sync_client(&room, &mut fresh);
    assert_eq!(fresh.controls().unwrap(), saved.controls().unwrap());
}

fn login(room: &Room, origin: &str, password: &str, old: Option<&str>) -> (u16, String) {
    let bytes = serde_json::json!({"username":"test-user","password":password}).to_string();
    let mut tcp = TcpStream::connect(&room.public).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(tcp,"POST /auth/session HTTP/1.0\r\nHost: {}\r\nOrigin: {origin}\r\nX-OTriOS-Request: session\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",room.public,bytes.len()).unwrap();
    if let Some(cookie) = old {
        write!(tcp, "Cookie: {cookie}\r\n").unwrap();
    }
    write!(tcp, "\r\n{bytes}").unwrap();
    let mut reply = String::new();
    tcp.read_to_string(&mut reply).unwrap();
    let status = reply.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, reply)
}
fn login_cookie(room: &Room) -> String {
    let (status, reply) = login(room, ORIGIN, "test-only-password", None);
    assert_eq!(status, 200, "{reply}");
    let line = reply
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap();
    for attribute in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(line.contains(attribute));
    }
    assert!(
        !reply
            .split("\r\n\r\n")
            .nth(1)
            .unwrap()
            .contains("test-only-password")
    );
    line.split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}
fn private_socket(
    room: &Room,
    cookie: &str,
) -> tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>> {
    let mut request = format!("ws://{}/monitoring/stream", room.public)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", ORIGIN.parse().unwrap());
    request
        .headers_mut()
        .insert("Cookie", cookie.parse().unwrap());
    let (mut socket, _) = tungstenite::connect(request).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    }
    socket
}
#[test]
fn monitoring_requires_a_cookie_and_logout_revokes_existing_streams_and_admin_access() {
    let room = room_with_auth(true, 28800);
    let bytes = private_view(&room);
    for path in ["/auth/session", "/monitoring/presentation"] {
        assert_eq!(request(&room.public, "GET", path, &[], &[]).0, 401);
    }
    for path in ["/monitoring/stream", "/monitoring/stream?token=forged"] {
        let mut req = format!("ws://{}{path}", room.public)
            .into_client_request()
            .unwrap();
        req.headers_mut().insert("Origin", ORIGIN.parse().unwrap());
        assert!(
            matches!(tungstenite::connect(req),Err(tungstenite::Error::Http(r)) if r.status().as_u16()==401)
        );
    }
    assert_eq!(
        login(
            &room,
            "https://unrelated.example",
            "test-only-password",
            None
        )
        .0,
        403
    );
    assert_eq!(login(&room, ORIGIN, "wrong-password", None).0, 401);
    assert_eq!(
        request(
            &room.public,
            "POST",
            "/auth/session",
            &[("Origin", ORIGIN), ("Content-Type", "application/json")],
            b"{}"
        )
        .0,
        403
    );
    assert_eq!(
        request(
            &room.public,
            "POST",
            "/auth/session",
            &[
                ("Origin", ORIGIN),
                ("X-OTriOS-Request", "session"),
                ("Content-Type", "application/json")
            ],
            &vec![b'x'; 4096]
        )
        .0,
        413
    );
    let cookie = login_cookie(&room);
    assert_eq!(
        request(
            &room.public,
            "GET",
            "/monitoring/presentation",
            &[("Cookie", &cookie)],
            &[]
        )
        .1,
        bytes
    );
    for path in ["/layout", "/presentation", "/sync"] {
        assert_eq!(
            request(
                &room.public,
                "POST",
                path,
                &[("Cookie", &cookie), ("Origin", ORIGIN)],
                &[]
            )
            .0,
            404
        );
    }
    let mut socket = private_socket(&room, &cookie);
    let Message::Binary(view) = socket.read().unwrap() else {
        panic!("missing view")
    };
    assert_eq!(&view[5..], bytes);
    assert!(view.starts_with(b"OTML1"));
    assert_eq!(
        request(
            &room.public,
            "DELETE",
            "/auth/session",
            &[
                ("Cookie", &cookie),
                ("Origin", "https://unrelated.example"),
                ("X-OTriOS-Request", "session")
            ],
            &[]
        )
        .0,
        403
    );
    assert_eq!(
        request(
            &room.public,
            "DELETE",
            "/auth/session",
            &[
                ("Cookie", &cookie),
                ("Origin", ORIGIN),
                ("X-OTriOS-Request", "session")
            ],
            &[]
        )
        .0,
        200
    );
    assert_eq!(
        request(
            &room.public,
            "GET",
            "/monitoring/presentation",
            &[("Cookie", &cookie)],
            &[]
        )
        .0,
        401
    );
    assert!(matches!(socket.read(), Ok(Message::Close(_))));
    assert_eq!(request(&room.public, "GET", "/health", &[], &[]).0, 200);
}
#[test]
fn monitoring_sessions_rotate_and_expire_without_changing_demo_state() {
    let room = room_with_auth(true, 2);
    private_view(&room);
    let before = std::fs::read(room.root.join("state.automerge")).unwrap();
    let cookie = login_cookie(&room);
    let (status, _) = login(&room, ORIGIN, "test-only-password", Some(&cookie));
    assert_eq!(status, 200);
    assert_eq!(
        request(
            &room.public,
            "GET",
            "/auth/session",
            &[("Cookie", &cookie)],
            &[]
        )
        .0,
        401
    );
    let cookie = login_cookie(&room);
    let mut socket = private_socket(&room, &cookie);
    assert!(matches!(socket.read(), Ok(Message::Binary(_))));
    assert!(matches!(socket.read(), Ok(Message::Close(_))));
    assert_eq!(
        request(
            &room.public,
            "GET",
            "/auth/session",
            &[("Cookie", &cookie)],
            &[]
        )
        .0,
        401
    );
    assert_eq!(
        std::fs::read(room.root.join("state.automerge")).unwrap(),
        before
    );
}
