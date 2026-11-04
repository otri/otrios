// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "server")]
mod common;
use common::*;
use otrios_interfaces::canvas::ServerMessage;
use std::{
    net::TcpStream,
    time::{Duration, Instant},
};
use tungstenite::{Message, client::IntoClientRequest};
fn ink_socket(address: &str, path: &str, origin: &str) -> Socket {
    let mut req = format!("ws://{address}{path}")
        .into_client_request()
        .unwrap();
    req.headers_mut().insert("Origin", origin.parse().unwrap());
    let (mut s, _) = tungstenite::connect(req).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(t) = s.get_mut() {
        t.set_read_timeout(Some(Duration::from_millis(30))).unwrap();
    }
    common::session_client::negotiate(&mut s).unwrap();
    s.send(Message::Text(
        serde_json::json!({"type":"ink","enabled":true})
            .to_string()
            .into(),
    ))
    .unwrap();
    s
}

#[test]
fn anonymous_clients_get_public_apps_but_no_private_ui_state_or_view() {
    let room = room(false);
    view(&room);
    for path in [
        "/presentation",
        "/layout",
        "/sync",
        "/authority/apps/monitoring/ui.flex",
    ] {
        assert_eq!(request(&room.public, "GET", path, &[], &[]).0, 404);
    }
    assert_eq!(
        request(&room.public, "GET", "/monitoring/presentation", &[], &[]).0,
        401
    );
    let mut s = socket(&room.public, None);
    mount(&mut s, &room, "monitoring", 1);
    assert!(matches!(
        read(&mut s),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
    mount(&mut s, &room, "demo", 2);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 2, .. }
    ));
    for origin in [None, Some("https://unrelated.example")] {
        let mut r = format!("ws://{}/session", room.public)
            .into_client_request()
            .unwrap();
        if let Some(o) = origin {
            r.headers_mut().insert("Origin", o.parse().unwrap());
        }
        assert!(
            matches!(tungstenite::connect(r),Err(tungstenite::Error::Http(r))if r.status().as_u16()==403)
        );
    }
}
#[test]
fn monitoring_signin_and_logout_revoke_existing_app_lease() {
    let room = room(true);
    let expected = view(&room);
    let cookie = cookie(&room);
    let mut s = socket(&room.public, Some(&cookie));
    mount(&mut s, &room, "monitoring", 1);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 1, .. }
    ));
    accept(&mut s, 1);
    loop {
        if let ServerMessage::View { bytes, .. } = read(&mut s) {
            assert_eq!(bytes, expected);
            break;
        }
    }
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
    assert!(matches!(
        read(&mut s),
        ServerMessage::AccessRequired { epoch: 1 }
    ));
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
    mount(&mut s, &room, "demo", 2);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 2, .. }
    ));
}
fn restart_authority(room: &mut Room) {
    room.child.kill().unwrap();
    room.child.wait().unwrap();
    room.child = std::process::Command::new(env!("CARGO_BIN_EXE_dashboard-server"))
        .args([
            "--bind",
            &room.private,
            "--public-bind",
            &room.public,
            "--apps",
            room.root.join("apps").to_str().unwrap(),
            "--state",
            room.root.join("state.merkle").to_str().unwrap(),
            "--auth-file",
            room.root.join("auth.json").to_str().unwrap(),
            "--origin",
            ORIGIN,
        ])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(&room.public).is_ok()
            && request(&room.public, "GET", "/health", &[], &[]).0 == 200
        {
            break;
        }
        assert!(room.child.try_wait().unwrap().is_none(), "authority exited");
        assert!(Instant::now() < deadline, "authority restart timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn browser_sessions_survive_refresh_and_process_restarts_until_explicit_signout() {
    let mut room = room(true);
    view(&room);
    let before = std::fs::read(state_file(&room, "demo")).unwrap();
    let first = cookie(&room);
    let second = cookie(&room);
    let status = |room: &Room, token: &str| {
        request(
            &room.public,
            "GET",
            "/auth/session",
            &[("Cookie", token)],
            &[],
        )
        .0
    };
    for _ in 0..3 {
        let (code, _, headers) = response(
            &room.public,
            "GET",
            "/auth/session",
            &[("Cookie", &first)],
            &[],
        );
        assert_eq!(code, 200);
        assert!(headers.contains("Max-Age=34560000"));
        assert!(headers.contains("HttpOnly; SameSite=Strict"));
    }
    restart_authority(&mut room);
    assert_eq!(status(&room, &first), 200);
    assert_eq!(status(&room, &second), 200);
    // The retained cookie also still opens a protected app on a new socket.
    let mut s = socket(&room.public, Some(&first));
    mount(&mut s, &room, "monitoring", 1);
    assert!(matches!(read(&mut s), ServerMessage::Mounted { .. }));
    drop(s);
    assert_eq!(
        request(
            &room.public,
            "DELETE",
            "/auth/session",
            &[
                ("Cookie", &first),
                ("Origin", ORIGIN),
                ("X-OTriOS-Request", "session")
            ],
            &[]
        )
        .0,
        200
    );
    restart_authority(&mut room);
    assert_eq!(status(&room, &first), 401);
    assert_eq!(status(&room, &second), 200);
    // Login rotation is durable too; a replaced cookie never regains access.
    assert_eq!(login(&room, Some(&second)).0, 200);
    restart_authority(&mut room);
    assert_eq!(status(&room, &second), 401);
    assert_eq!(std::fs::read(state_file(&room, "demo")).unwrap(), before);
}

#[test]
fn public_and_native_ink_share_only_ephemeral_points_with_server_assigned_peers() {
    use otrios_interfaces::ink::{self, Point};
    let room = room(false);
    let private = view(&room);
    let state = std::fs::read(state_file(&room, "demo")).ok();
    let mut bad = format!("ws://{}/session", room.public)
        .into_client_request()
        .unwrap();
    bad.headers_mut()
        .insert("Origin", "https://unrelated.example".parse().unwrap());
    assert!(
        matches!(tungstenite::connect(bad),Err(tungstenite::Error::Http(r)) if r.status().as_u16()==403)
    );
    let mut a = ink_socket(&room.public, "/session", ORIGIN);
    let mut b = ink_socket(&room.private, "/session", ORIGIN);
    let read = |s: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>| {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match s.read() {
                Ok(Message::Binary(b)) => return b.slice(1..),
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
        otrios_interfaces::session::frame(
            2,
            points
                .iter()
                .copied()
                .flat_map(Point::encode)
                .collect::<Vec<_>>(),
        )
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
    let mut c = ink_socket(&room.public, "/session", ORIGIN);
    read(&mut c);
    let history = read(&mut c);
    assert_eq!(history.len(), live.len());
    std::thread::sleep(Duration::from_millis(5200));
    let mut late = ink_socket(&room.public, "/session", ORIGIN);
    read(&mut late);
    assert!(
        matches!(late.read(),Err(tungstenite::Error::Io(e)) if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut))
    );
    assert_eq!(std::fs::read(state_file(&room, "demo")).ok(), state);
    assert_eq!(
        request(&room.private, "GET", "/presentation", &[], &[]).1,
        private
    );
}

#[test]
fn private_candidate_revocation_preserves_the_previous_public_activation() {
    let room = room(true);
    view(&room);
    let cookie = cookie(&room);
    let mut s = socket(&room.public, Some(&cookie));
    mount(&mut s, &room, "demo", 1);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 1, .. }
    ));
    accept(&mut s, 1);
    // Mounted already carried this revision; no duplicate Sync is emitted.
    mount(&mut s, &room, "monitoring", 2);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 2, .. }
    ));
    // Explicit revocation replaces the former time-based expiry trigger.
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
    // No private Sync or View may be sent before acceptance, even at revocation.
    assert!(matches!(
        read(&mut s),
        ServerMessage::AccessRequired { epoch: 2 }
    ));
    accept(&mut s, 2);
    send(
        &mut s,
        otrios_interfaces::canvas::ClientMessage::CancelMount { epoch: 2 },
    );
    mount(&mut s, &room, "demo", 3);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 3, .. }
    ));
}
