<!-- SPDX-License-Identifier: Apache-2.0 -->
# Public demos and private Monitoring

`https://otrios.otri.ca` opens Home without sign-in. Component Playground, Layout
Lab, Input Lab, Animation Lab and 3D Lab are public apps. Their durable controls
share the photo frame's per-app Merkle state. Navigation, text entry and animation
phase remain local. Monitoring alone requires browser sign-in.

One Rust service on the Rockchip photo frame serves code and state. Cloudflare Tunnel reaches its public
loopback listener on 8877; the local display and collector use trusted 8876.
Remote administration uses pinned SSH over Tailscale. Public
requests never forward to the trusted listener. State recovery failure leaves
signed code and release negotiation available. A whole-process crash affects both.

| Public route | Policy |
| --- | --- |
| `/`, `/release`, `/releases/ID/...` | Signed public code/catalog; exact static path allowlist |
| `/session` | One negotiated WebSocket for release control, scoped state and ink |
| `/release/events` | Stable v1 control-only compatibility alias |
| `/health` | Status and release only; 503 while state unavailable |
| `/auth/session` | GET status, POST sign-in, DELETE sign-out |
| `/monitoring/presentation` | Valid Monitoring session; private read-only view |
| `/apps/stream`, `/demo/ink` | Retired protocols; 410 |
| `/presentation`, `/admin/activate`, other private paths | Not public; 404 even after sign-in |

## Authorization boundary

The public session requires an exact allowed browser Origin and has separate
peer, byte and rate limits. Its v1 hello must finish before binary state is
accepted. Anonymous clients can mount only public app UI/state. State operations
are validated server-side; layout, schema and arbitrary fields cannot be written
by public controls. Monitoring UI/behavior/state are delivered only through an
authorized mount. Private bytes under `authority/apps/` have no static public
route, despite residing beside public assets in the single signed local store.
A lock icon or client-side `authenticated` field grants no authority.

Leaving Monitoring drops its scoped view; logout revokes its server session.
Private leases are checked on input, once a second for expiry, and before queued
private bytes leave the socket. Public code and authorized viewers cannot recall
copies already delivered to a signed-in user. This remains a bounded demo policy,
not the future general OTriOS capability runtime.

## Monitoring sessions

The existing username and seven-word password remain valid. The private mode-600
`--auth-file` contains `username` and an Argon2id `password_hash`. Never commit it,
tunnel credentials or signing keys. Successful login rotates a random 256-bit
opaque session; only its digest is kept server-side. The browser receives a
host-only `Secure; HttpOnly; SameSite=Strict; Path=/` cookie, expiring after eight
hours by default. Sessions are RAM-only; restarting the server signs browsers out.

POST/DELETE require the exact Origin and a custom request header; login uses
JSON. Verification has one worker slot, 20 attempts/minute globally and 128
sessions. Passwords/tokens do not enter URLs, document state or logs. The browser
uses standard password fields and reconnects its single session after login to
acquire the new cookie. The native frame uses the trusted loopback listener.
Only immutable public artifacts enter disposable caches; documents and private
Monitoring views remain in RAM. See [client storage](CLIENT_STORAGE.md).

## Publication and verification

The publisher stages signed assets in one local immutable store. The activation
CLI verifies all public/private digests and sizes before advancing the pointer
and notifying the trusted activation hook. There is no code replica, proxy
container or separate release-control daemon. Healthy clients receive upgrade
events over the existing session; reconnect hello recovers missed events. Release
metadata is independent of state decoding. See [live deployment](DASHBOARD_LIVE.md)
and [RFC 0009](rfcs/0009-unified-server-session.md).

Build/sign on the Mac and publish to the frame as described in [frame hosting](FRAME_HOSTING.md).
Run the required Rust/Python/Node checks, then:

```sh
python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --credentials PRIVATE_LOGIN_FILE
```

Credentials are read privately and never printed/passed as process arguments.
Checks cover canonical signed code, forbidden private/admin paths, Origin gates,
hello negotiation and cookie revocation. Rust process tests exercise actual
private app mounts. `--state-unavailable` checks code/control availability on an
isolated failing state store; `--connect-ip` overrides DNS with normal TLS hostname
verification. Do not stop production state merely to run that test.

Stop the dedicated tunnel to stop public access. To rotate the password, replace
the private hash and restart the server, revoking sessions. Keep a private rollback
snapshot. Existing DNS, Tailscale, kernel, display timing and calibration remain.
