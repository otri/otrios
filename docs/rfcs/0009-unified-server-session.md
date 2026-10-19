<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0009: one server and negotiated client session

Status: implemented and deployed on browser and frame on 2026-10-19.
Acceptance: [deployment record](../experiments/unified-session-deployment.md). Supersedes RFC 0008's deployment/process
separation, preserving its stable v1 bootstrap envelope and offline-edit policy.

## Deployment target

One Rust dashboard-server process owns public code, release metadata/events,
state replication, ephemeral ink and Monitoring sessions. Keep a trusted loopback
listener (8876, reached through Tailscale) and a public loopback listener (8877,
reached through the existing Cloudflare Tunnel). Public routing remains an exact
allowlist. Listener count is independent of process count. Consolidate on macOS
first; containerizing the same executable is a later packaging step.

The source watcher is development tooling, not a request-serving dependency.
Signing/activation is an on-demand CLI. The tunnel and existing Tailscale daemon
remain connectivity infrastructure. No Caddy, code-replica container, replica
publisher daemon or separate release-control server remains in the final path.

## Stable opening handshake

Open `/session`; the client first sends a bounded UTF-8 JSON hello:

```json
{"type":"hello","version":1,"release":null,"state_protocol":"otrios-canvas-sync-v2","state":true}
```

The optional `state` flag defaults false, preserving v1 control-only clients.
The server's v1 hello includes the current release descriptor, `refresh`,
`compatible` and `state_available`. Compatibility refers to the server's actual
compiled state codec, not merely the release hash. Unknown hello/state versions
still receive the current code descriptor. Never decode or send CRDT payloads
before compatible negotiation. An incompatible connection remains control-only
so it can receive upgrades. A corrupt or unavailable state store must not prevent
hello, code delivery or upgrade events.

Keep `/release/events` as a small alias of the same handler for existing v1
updaters. It adds no service or backend connection. Old `/apps/stream` and
`/demo/ink` protocols can return Gone after deployment; existing control clients
can still discover the new release. Never silently reinterpret legacy bytes.

## Multiplexing and resource ownership

JSON text frames carry stable control: hello, upgrade, state availability and
`{"type":"ink","enabled":true}` subscription control. Binary frames have a
one-byte channel tag: 1 = the existing bounded Canvas binary payload; 2 = the
existing bounded ink payload. Channel tags are transport metadata outside
canonical state encoding/hashes. Ink stays ephemeral, scoped to an explicit
subscription, bounded and expiring after five seconds.

Each client has one remote WebSocket. State processing uses bounded input/output
queues and its existing authority lock; the socket loop handles release control
without acquiring that lock. Prioritize release/availability control over data,
retain write deadlines and disconnect slow state readers for resync. TCP still
has ordering and head-of-line blocking; a frame already being sent cannot be
preempted. Enforce separate state, ink and control limits. Auth revocation is
rechecked before delivering queued private bytes.

The browser's small network/bootstrap owner starts before Wasm initialization.
It forwards accepted state/ink frames to the app worker and receives outgoing
frames from it. The worker opens no sockets. App changes reuse the session;
Monitoring sign-in reconnects it to acquire the changed cookie. Presentation
continues on its own clock and app mounts retain their existing scoped leases.

The native network thread negotiates and multiplexes the same session, outside
the CRDT decoder. It notifies the update supervisor through a local Unix datagram
socket. Remove the permanent native release-watch connection and separate ink
network thread. A one-shot session-bootstrap utility obtains the release through
the same hello when starting without a running host; it exits before that host
opens its session. No simultaneous bootstrap and renderer connections are needed.

## Activation and state availability

The signing/activation CLI verifies the signed manifest, exact public/private
asset paths, target, sizes and digests in the single immutable release store.
Only complete verified releases become current. A trusted-only activation hook
rereads the pointer, updates the release watch and requests catalog refresh in
process; publication has no HTTP proxy hop or duplicate module copy. Release
metadata does not wait for state recovery/catalog refresh. Code does not depend
on successful CRDT decoding to become discoverable.

Start the network service with state initially unavailable, then load/recover
state on a blocking worker. A stable availability event enables replication
when recovery completes. Recovery failure leaves code/control accessible and
state unavailable, with a diagnostic in private logs. Public access remains
closed to private Monitoring data if authentication configuration fails.

## Incremental implementation and acceptance

1. Commit this plan and amend AGENTS.md's compatibility boundary. Add the session
   contract, in-process release control, bounded state worker and ink multiplexing.
   Test real process hello mismatch, binary-before-hello rejection, no private
   payload before authorization, channel bounds, reconnect, publication races,
   slow-reader behavior and failed state recovery with code still accessible.
2. Migrate the browser bootstrap/app-worker boundary. Test negotiation before any
   state write, stale socket rejection, late Wasm readiness, ink subscribe/unsubscribe,
   reconnect/cookie rotation, connected edit/gesture gates, offline replacement and
   upgrade while the state engine is unavailable. Verify one actual browser socket.
3. Migrate native networking and local supervisor notices. Test channel handling,
   incompatible codec gate, bounded queues, bootstrap lifecycle and signature/cache
   boundaries. Cross-build and exercise the real frame with one renderer/session.
4. Replace replica publication with verified local activation. Stage signed assets
   and run required checks, full feature lint, browser and native process tests.
5. Pause the source watcher, back up current service files and durable state, then
   switch the existing tunnel target port from Caddy to the Rust public listener
   by stopping Caddy and restarting the authority with public port 8877. Tunnel
   configuration and private Tailscale routing stay unchanged. Activate the staged
   release, migrate the frame updater, and verify browser/frame shared state,
   Monitoring isolation, hello/upgrade/reconnect and one WebSocket per client.
6. Commit deployment evidence and cleanup. Resume the publisher pinned to the exact
   tested runtime commit. Main and the benchmark reference remain unchanged.

## Cleanup gates

After acceptance, remove the standalone release-control binary/source, old
long-running release-watch, obsolete Caddy/Compose/bootstrap files, Docker replica
publisher and its launcher, stale ports 8878/8879, unused client ink/control socket
paths, tests tied to retired topology and duplicate docs. Update current operational
instructions and mark historical reports as historical rather than rewriting their
measurements. Keep the v1 alias, immutable releases needed by clients/rollback,
private signing/auth material, the benchmark branch and one private rollback
snapshot. Remove only OTriOS containers/volumes retired by this migration; unrelated
OrbStack, infrastructure and Supabase resources are outside scope.

A whole-process crash now interrupts both code and state delivery. This is an
accepted simplification; store/codec failures should instead be contained as
unavailable state. Client state remains RAM-only; disconnected pending edits may
be lost during upgrade, while connected clients finish confirmed edits/gestures.
