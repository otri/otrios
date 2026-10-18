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
    s
}

#[test]
fn anonymous_clients_get_public_apps_but_no_private_ui_state_or_view() {
    let room = room(false, 28800);
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
        let mut r = format!("ws://{}/apps/stream", room.public)
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
    let room = room(true, 28800);
    let expected = view(&room);
    let cookie = cookie(&room);
    let mut s = socket(&room.public, Some(&cookie));
    mount(&mut s, &room, "monitoring", 1);
    assert!(matches!(
        read(&mut s),
        ServerMessage::Mounted { epoch: 1, .. }
    ));
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
#[test]
fn rotated_and_expired_sessions_revoke_private_bytes_without_writing_state() {
    let room = room(true, 2);
    view(&room);
    let before = std::fs::read(room.root.join("state.apps/demo.automerge")).unwrap();
    let old = cookie(&room);
    assert_eq!(login(&room, Some(&old)).0, 200);
    assert_eq!(
        request(
            &room.public,
            "GET",
            "/auth/session",
            &[("Cookie", &old)],
            &[]
        )
        .0,
        401
    );
    let cookie = cookie(&room);
    let mut s = socket(&room.public, Some(&cookie));
    mount(&mut s, &room, "monitoring", 1);
    assert!(matches!(read(&mut s), ServerMessage::Mounted { .. }));
    loop {
        if let ServerMessage::AccessRequired { epoch: 1 } = read(&mut s) {
            break;
        }
    }
    assert_eq!(
        std::fs::read(room.root.join("state.apps/demo.automerge")).unwrap(),
        before
    );
}

#[test]
fn public_and_native_ink_share_only_ephemeral_points_with_server_assigned_peers() {
    use otrios_interfaces::ink::{self, Point};
    let room = room(false, 28800);
    let private = view(&room);
    let state = std::fs::read(room.root.join("state.apps/demo.automerge")).ok();
    let mut bad = format!("ws://{}/demo/ink", room.public)
        .into_client_request()
        .unwrap();
    bad.headers_mut()
        .insert("Origin", "https://unrelated.example".parse().unwrap());
    assert!(
        matches!(tungstenite::connect(bad),Err(tungstenite::Error::Http(r)) if r.status().as_u16()==403)
    );
    let mut a = ink_socket(&room.public, "/demo/ink", ORIGIN);
    let mut b = ink_socket(&room.private, "/demo/ink", ORIGIN);
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
    let mut c = ink_socket(&room.public, "/demo/ink", ORIGIN);
    read(&mut c);
    let history = read(&mut c);
    assert_eq!(history.len(), live.len());
    std::thread::sleep(Duration::from_millis(5200));
    let mut late = ink_socket(&room.public, "/demo/ink", ORIGIN);
    read(&mut late);
    assert!(
        matches!(late.read(),Err(tungstenite::Error::Io(e)) if matches!(e.kind(),std::io::ErrorKind::WouldBlock|std::io::ErrorKind::TimedOut))
    );
    assert_eq!(
        std::fs::read(room.root.join("state.apps/demo.automerge")).ok(),
        state
    );
    assert_eq!(
        request(&room.private, "GET", "/presentation", &[], &[]).1,
        private
    );
}
