<!-- SPDX-License-Identifier: Apache-2.0 -->
# Public demos and private Monitoring

`https://otrios.otri.ca` serves the public launcher and signed code. Public Space
uses the explicit Public edit audience: anonymous visitors can read and edit its
editable experiments and use temporary guest profiles in Brass Relay. Operations'
Monitoring remains closed to nonmembers. Public view and Public comment retain
their narrower permissions; Public comment still requires sign-in to comment.
Navigation, editing focus and animation phase remain local.

Navigation entries serve app-specific page titles and Open Graph metadata in
the initial HTML. Home uses `OTriOS`; `/brass-relay` uses the browser title
`OTriOS · The Brass Relay`, `og:title` of `The Brass Relay`, and `og:site_name`
of `OTriOS`. Other routes use their public catalog titles, including Monitoring
before sign-in. Client navigation updates the same title metadata. Server title
values are HTML-escaped, while immutable release assets remain unchanged.
This follows [Apple's link-preview guidance](https://developer.apple.com/documentation/technotes/tn3156-create-rich-previews-for-messages):
preview metadata must be present without running JavaScript.

The 2026-10-27 metadata update was qualified with `bash scripts/check.sh`, the
server's `unified_session` process test (including unavailable state and escaped
titles), all 69 signed-builder Node tests, and `scripts/test-chat-browser.mjs`
against the exact signed candidate. Browser checks cover raw route metadata,
navigation titles and Monitoring before sign-in. Public GETs verified titles on
Home, Brass Relay, Test Playground and Monitoring after deployment, together with
signed HTML/JS/Wasm hashes and native readiness. The release at that qualification was:
`f8cacaa81ae3a748dd853b4357403077f1d48735a4fcd2fa28c02dc4b36b9e00`.
At qualification, the code rollback was retained at
`/var/lib/otrios-authority/rollback/2026-10-27-open-graph`.
For the current release, read `/release` or the opening `/session` hello;
the qualification hash above is historical.

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
| `/spaces` | Filtered Space discovery and authenticated management |
| `/monitoring/presentation` | Legacy read path; managed clients use an admitted Monitoring mount |
| `/apps/stream`, `/demo/ink` | Retired protocols; 410 |
| `/presentation`, `/admin/activate`, other private paths | Not public; 404 even after sign-in |

## Authorization boundary

The public session requires an exact allowed browser Origin and has separate
peer, byte and rate limits. Its v1 hello must finish before binary state is
accepted. Anonymous clients can mount only public Space app UI/state. State operations
are validated server-side. Public edit grants the app's declared editable values
and bounded OIB authoring; it grants neither Space administration, moderation,
host-code publication nor arbitrary schema changes. Read-only app policy remains
read-only even in Public edit. Monitoring UI/behavior/state are delivered only through an
authorized mount. Private bytes under `authority/bundles/` (or legacy
`authority/apps/`) have no static public
route, despite residing beside public assets in the single signed local store.
A lock icon or client-side `authenticated` field grants no authority.

Leaving Monitoring drops its scoped view; logout revokes its server session.
Private leases are checked on input, once a second for revocation, and before queued
private bytes leave the socket. Public code and authorized viewers cannot recall
copies already delivered to a signed-in user. This remains a bounded demo policy,
not the future general OTriOS capability runtime.

## Monitoring sessions

The existing named login and password remain valid. The private mode-600
`--auth-file` uses the version-1 `users` and `native` identity records described in
[Spaces identity configuration](SPACES.md#identity-configuration). Human entries
retain an Argon2id `password_hash`; device/collector entries retain token digests.
Space roles and service grants are separate from authentication. Never commit it,
tunnel credentials or signing keys. Successful login rotates a random 256-bit
opaque session; only its digest is kept server-side. The browser receives a
host-only `Secure; HttpOnly; SameSite=Strict; Path=/` persistent cookie. Server
sessions have no time or idle expiry and survive refreshes, deployments and
server restarts. Sign Out durably revokes that browser's token; another browser
stays signed in. Login rotation and explicit account/credential changes revoke
superseded grants. The browser cookie uses a 400-day lifetime, renewed whenever
an authenticated session check succeeds; browsers may impose retention limits
or users may clear site data.

Only token digests and credential-bound identity records are stored in a private,
atomically replaced mode-600 session file, outside signed releases and app data.
Its default is `--app-state` with extension `sessions.json` (on Frame,
`/var/lib/otrios-authority/state.sessions.json`); without app state it is adjacent
to the auth file. `--sessions-file` selects an explicit private path. One authority
owns its lock. Corrupt/unreadable state fails closed, and a failed durable login
or sign-out reports failure instead of confirming an uncommitted change.
The former `--session-seconds` option is removed.

A failed status request caused by rate limiting or a server/proxy outage does
not change the last confirmed browser sign-in status. An authoritative 401 still
revokes it immediately; protected requests are always checked by the authority.
Authentication state is recovered independently of CRDT state and release discovery.

POST/DELETE require the exact Origin and a custom request header; login uses
JSON. Verification has one worker slot, 20 attempts/minute globally and 128
sessions. Reaching that bound rejects a new login rather than evicting an existing
browser; replacing the current browser token does not consume another slot. Passwords/tokens do not enter URLs, document state or logs. The browser
uses standard password fields and reconnects its single session after login to
acquire the new cookie. The native Frame uses its private token file on the
loopback listener; loopback access itself grants no managed admission.
Only immutable public artifacts enter disposable caches; documents and private
Monitoring views remain in RAM. See [client storage](CLIENT_STORAGE.md).

## Publication and verification

The publisher stages signed assets in one local immutable store. The activation
CLI verifies all public/private digests and sizes before advancing the pointer
and notifying the trusted activation hook. There is no code replica, proxy
container or separate release-control daemon. Healthy clients receive upgrade
events over the existing session; reconnect hello recovers missed events. Release
metadata is independent of state decoding. See [live deployment](FRAME_HOSTING.md)
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
