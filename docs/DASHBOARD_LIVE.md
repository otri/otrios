<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live app deployment

One Rust `dashboard-server` serves signed code, release events, durable Canvas
state, ephemeral ink and Monitoring sessions. It has two loopback listeners:
trusted 8876 for the local display/collector, public 8877 through the existing
Cloudflare Tunnel. Remote administration uses the existing pinned Tailscale SSH.
There is one immutable release store and no Caddy, replica container, replica
publisher daemon or separate release-control server. The authority and tunnel
run as systemd services on the Rockchip photo frame.
The Mac builds and signs releases; it is not required for serving. See
[frame hosting](FRAME_HOSTING.md) for publication, storage and qualification.

```sh
dashboard-server --bind 127.0.0.1:8876 --public-bind 127.0.0.1:8877 \
  --app-state /ABS/RUNTIME/state.apps \
  --presentation-file /ABS/RUNTIME/state.presentation.flex \
  --trusted-peers 4 --public-peers 64 \
  --releases /ABS/RUNTIME/releases --auth-file /ABS/RUNTIME/monitoring-auth.json \
  --origin https://otrios.otri.ca
```

## One negotiated session

Each host opens `/session` and sends the stable v1 JSON hello before any state
bytes. The reply gives the active release, codec compatibility and state
availability. Unknown CRDT versions still receive code upgrades. JSON control,
binary Canvas frames (channel 1) and ephemeral ink (channel 2) share that socket.
The browser opens it before Wasm starts; its worker opens no sockets. The native
host has one network thread outside the state decoder. See [RFC 0009](rfcs/0009-unified-server-session.md).

There is no healthy-client release polling. Publication sends upgrade events;
reconnect hello recovers missed events. `/release/events` remains a control-only
v1 alias for older updaters. `/release` remains a small HTTP pointer. Retired
`/apps/stream` and `/demo/ink` return 410; their bytes are never reinterpreted.

The frame runs one direct DRM/GBM/EGL/GLES2 renderer, with the accepted rotation
and touch calibration. Its supervisor receives release notices over a local Unix
datagram socket. On cold start without a modern cached host, `session-bootstrap`
obtains the hash over `/session` and exits before the renderer connects. Install
it at `/opt/otrios-dashboard/session-bootstrap` alongside the supervisor. No
permanent release-watch process, separate ink socket, Wayland, SDL or browser is
needed on the frame. `--url` and `--code-url` both use `http://127.0.0.1:8876` on the frame.
Plain HTTP/WS is accepted only for literal loopback addresses; remote origins
require HTTPS/WSS. Native signature checks remain mandatory.

## State, policy and failures

The trusted loopback listener admits private apps for the local display and accepts the
monitoring collector's `/presentation` upload. The public listener uses an exact
route/Origin allowlist and Monitoring's HttpOnly cookie; static paths cannot
expose `authority/apps/`. Public demos share the frame's per-app data roots.
Navigation remains local. Private leases are checked on input, expiry and before
queued bytes leave the socket. [Public access](PUBLIC_ACCESS.md) details policy.

Each app has a Merkle checkpoint and bounded append journal. Changed nodes and
intent receipts are durable before roots are published or edits acknowledged.
Checkpoints rotate at 4 MiB. Clients keep document state and pending edits in RAM;
closing one may lose unconfirmed edits. Executable/font/blob caches are bounded
(about 512 MiB web, 128 MiB frame disk budget). Ink expires after five seconds and is never
persisted. Legacy `--state FILE` only derives the app directory/presentation path;
prefer the explicit arguments above. Never run two authorities on the same state.

State recovery runs separately from code/control handling. Recovery failures
leave code and hello available, with state unavailable and public health 503.
An activation can retry recovery; existing compatible sessions receive a readiness
event when it succeeds. A whole-process crash interrupts all serving; that common
failure domain is the accepted cost of consolidation. Bounded queues and socket
write deadlines disconnect slow state readers for resync rather than accumulating
unlimited messages. They cannot eliminate TCP head-of-line blocking.

## Signing and activation

`watch-dashboard.py` freezes source, tests/builds browser and ARM code and signs
an immutable release. It writes into the single local store without activating,
then calls `activate-dashboard-release.py`. The CLI verifies the signature,
manifest, exact public/private paths, sizes and every digest before atomically
advancing `current.json`. It notifies the trusted-only `/admin/activate` hook;
code events do not wait for catalog refresh or successful state decoding.
A notification failure is reported and retried; reconnects also reread metadata.

The public catalog admits UI/State references and hashes. UI/behavior payloads
under `authority/apps/<id>/` travel only through authorized mounts. Their presence
in the local signed store never grants public static access. Native execution
requires the pinned Ed25519 signature; browser resources rely on HTTPS and the
manifest hash closure, without independently verifying the detached signature.
A signature does not sandbox trusted native publisher code.

Private frame configuration lives in `/etc/otrios-authority`, code and helpers
in `/opt/otrios-authority`, and the signed release store and durable app roots in
`/var/lib/otrios-authority`. The native disposable cache remains separate under
`/var/lib/otrios-dashboard`. systemd runs `otrios-authority`, `otrios-tunnel`,
`otrios-monitoring.timer` and the existing `otrios-dashboard-direct` service.
The frame holds the publisher public key, login hash and service credentials;
the signing private key remains on the Mac.

The Mac build/signing workspace remains under
`~/Library/Application Support/OTriOSDashboard`. Its watcher can publish over
pinned SSH using the private config's `frame.ssh` argument list. A complete
replica is verified on the frame before activation; matching authority binaries
are installed separately from the signed renderer artifact. Unchanged server
bytes do not cause a restart. The builder and frame release stores are pruned
to their active release after publication. The Mac serving/tunnel/collector
LaunchAgents are retired. No obsolete release history is required.

```sh
python3 scripts/watch-dashboard.py --config /ABS/RUNTIME/config.json
python3 scripts/watch-dashboard.py --config /ABS/STAGING-CONFIG.json --once --stage-only
python3 scripts/activate-dashboard-release.py --help
```

Staging writes `build/staged-release.json` without advancing the live pointer or
source marker. Test a disposable authority first. Pause the watcher, snapshot
service files and stopped durable state, install the matching authority and frame
supervisor/helper, then activate. Keep one private operational rollback snapshot
and signed cached releases. Rollback across retired protocols must restore the
matching server and agents together; a browser may need a launcher-root visit.

## Acceptance and cleanup

Run `bash scripts/check.sh`, all-feature Clippy, browser session/worker/GPU/cache
Node tests, native networking tests and `scripts/test-dashboard-host.py` against
the deployed origin. Real-process tests cover negotiation, private mounts,
recovery, authorization, ink and durable restart. Verify one actual browser
session, the same signed frame release and a rendered/acknowledged native mount.
Framebuffer captures prove rendered output, not physical touch/color acceptance.

After acceptance remove only retired OTriOS replica containers/volumes, publisher
and release-control launchers, their binaries and ports 8878/8879. Preserve tunnel
configuration/credentials, signing/auth material, durable state, immutable cached
releases, the benchmark branch and the rollback snapshot. Other infrastructure
and Supabase resources are outside scope. Historical experiment reports retain
their original topology and measurements; current operation follows RFC 0009.

## Qualified cutover order

Push the tested source commit and verify that the configured remote can fetch
that exact SHA **before** stopping or activating anything. Freeze and qualify
matching authority/native/browser builds, verify the signed local replica, then
pin the watcher to that tested commit. Do not activate an unpushed source pin.
Retain a rollback copy before replacing installed binaries/helpers.

The frame supervisor and direct service are one deployment unit: install their
matching versions together, preserving the accepted rotation and touch calibration.
The supervisor runs one direct renderer, verifies first presentation and confirmed
app readiness, and retains a verified direct last-good release. There is no SDL
or Wayland fallback. Canonical app state stays on the authority; rollback must
never replace newly confirmed state with an old backup.
