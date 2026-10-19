<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live app deployment

The Mac runs one authority and a signed release publisher. A separate OrbStack
module host serves immutable browser resources through `otrios.otri.ca`.
Tailscale exposes the trusted authority to the frame. The frame runs one native
DRM/GBM/EGL/GLES2 host with the accepted rotation and calibrated touch; no
Wayland, SDL or dashboard browser is involved.

## Authority and routes

```sh
dashboard-server --bind 127.0.0.1:8876 --public-bind 127.0.0.1:8878 \
  --app-state /ABS/RUNTIME/apps-state --releases /ABS/RUNTIME/releases \
  --auth-file /ABS/RUNTIME/monitoring-auth.json \
  --origin https://otrios.otri.ca --origin https://HOST.TAILNET.ts.net
```

The trusted listener permits private app admission for tailnet devices. The
public adapter enforces an exact browser Origin, bounded peer/message counts,
and Monitoring's existing HttpOnly session. Both use `/apps/stream` for binary
Canvas mount/sync/view messages and JSON release notices. An anonymous mount
cannot receive Monitoring UI, behavior or state. Private leases are checked on
input, before delivery and at most one second after expiry/logout. `/demo/ink`
is the separate ephemeral trail channel. `/auth/session` manages Monitoring
sign-in. The public proxy has no administrative feed or state mutation routes.

`/health` reports app-root counts and current release, rather than pretending
unrelated apps have one global snapshot. Old `/stream`, `/demo/stream`, `/sync`
and `/layout` global writers return 410. The authorized local monitoring job
continues to upload a complete FlexBuffers Layout to `/presentation`; the
server validates and replaces that read-only aggregate without CRDT history.

Each app has a `.merkle` checkpoint and a bounded `.merkle.log` append journal.
Normal writes flush only changed Merkle nodes plus an atomic intent receipt
before publishing the root or acknowledging. Checkpoints rotate at 4 MiB;
torn tails recover and complete corrupt records fail closed. Clients keep only RAM state, so a live process
can reconnect with pending edits; crashing or closing it can lose unacknowledged
work. Code/fonts/blobs are disposable caches, not document checkpoints.
`--state FILE` now supplies a legacy filename prefix only: app state lives in
`FILE` with extension `.apps`, and the feed cache uses `.presentation.flex`.
The old monolithic document is neither loaded nor written. The Merkle cutover imports current values from trusted per-app `.automerge` files
only when the new checkpoint is absent, retaining those files for recovery.
Legacy operation history and signal history are not imported. See [acceptance](experiments/canvas-merkle-adapter-validation.md).

## Signed publication

`watch-dashboard.py` freezes source inputs, runs Canvas tests, compiles the
modern browser host and ARMv7 direct host, checks JS/GPU/cache contracts, and
signs a complete immutable release. Generated packages and catalogs are not
source fingerprint inputs. `publish-dashboard-module.py` copies verified public
assets into the independent read-only OrbStack volume before advancing the
origin's current pointer. A stale or failed build leaves the previous release.

The release manifest binds `otrios-canvas-merkle-state-v1` state and
`otrios-canvas-sync-v2` transport. Its public `apps/catalog.flex` lists admitted
UI/State pairs and UI hashes; each UI binds its behavior's hash and permissions.
App UI/code resides under `authority/apps/<id>/` in the signed release's
`authority` digest table. These payloads are never copied to the public module
volume or exposed by a static-download route. They travel through authorized
mounts. The client checks the catalog's admitted UI hash before activation.

The native supervisor verifies the pinned Ed25519 publisher signature, sizes,
ARM executable, catalog and compressed font before execution. Browser resources
use the trusted HTTPS origin and manifest hash closure; the browser does not
independently verify the detached signature. A signature does not sandbox
native code: the configured publisher remains trusted.

The Mac runtime is `~/Library/Application Support/OTriOSDashboard`. Credentials,
keys, logs, state, source inbox and build caches stay outside Git. Its login
LaunchAgents include the state server, source watcher, monitoring collector and
public tunnel/module adapter. The watcher follows the configured trusted Git
branch or `queue-dashboard-source.py` feed; do not push incomplete code while
its automatic deployment is active. Installed service scripts and server
binaries must be updated explicitly for a protocol cutover.

```sh
python3 scripts/watch-dashboard.py --config /ABS/RUNTIME/config.json
```

The private watcher config supplies absolute repo/runtime/tool paths and the
pinned Rust build environment. Keep the publisher private key mode 0600; serve
only its public key. The Mac must remain awake, online and logged in. This is
not an always-on cloud deployment.

## Activation, readiness and recovery

A candidate mount qualifies its UI, schema, reducer, authoritative state and
primitive scene before replacing the active scope. Navigation retires capture,
keyboard repeat and the old subscription. Rejected candidates leave the current
app usable. Automatic code replacement waits for confirmed user edits and no active
input; browser state does not enter session storage or IndexedDB. Normal
navigation stays local; the public demos share the frame's data roots.

The frame supervisor keeps exactly one display owner and accepts a release only
after rendered output and a live acknowledged app stream. Its cache limit is
about 2 GiB; the browser immutable resource cache is about 512 MiB, subject to quota.
The font/GPU coverage cache remains independent. New releases have only the
native direct executable, not an SDL recovery binary. Failed compatible
candidates can return to a previously verified cached release.

The initial Canvas protocol cutover cannot automatically run an older global-room
binary against the new authority. Keep the previous server/agent/proxy scripts
and signed release for coordinated operational rollback. Already-open legacy
browser tabs need one visit to the root URL after this cutover; there is no
permanent legacy protocol bridge.

## Validation

Run `bash scripts/check.sh`, all-feature Clippy, the modern Wasm export/GPU/cache
Node tests, and native worker tests. Real-process tests cover per-app isolation,
durable restart, new behavior admission, source reload, exact Origins, private
UI denial, cookie logout/expiry and ephemeral ink. Signing tests include catalog
tampering and a private-payload canary that must not reach the public volume.

Use `scripts/test-dashboard-module-host.py` against the deployed host for HTTP,
resource, proxy and auth policy checks. It complements the Rust tests that
actually encode app mounts. Verify the same signed release in browser and frame
readiness, then inspect an actual native framebuffer and the local presentation
statistics. Matching references prove scope identity, not pixel equivalence or
physical panel color/touch acceptance. Record measured results in
[the migration validation log](experiments/canvas-migration-validation.md).

## Merkle cutover and staged publication

[RFC 0006](rfcs/0006-durable-canvas-merkle-adapter.md) defines server-only writes,
RAM intent overlays and durable deduplication receipts. Existing per-app
Automerge current values import once if a Merkle checkpoint is absent; old files
stay available for coordinated rollback. Do not run two authorities against the
same state directory. A pending batch resumes for up to 24 hours while its
receipt is retained; an uncertain expired batch is rejected explicitly.

Before a protocol cutover, pause the source watcher and build an immutable
candidate without advancing either host's pointer:

```sh
python3 scripts/watch-dashboard.py --config /ABS/STAGING-CONFIG.json --once --stage-only
```

The staged pointer is `build/staged-release.json`; the normal source-id marker
is not advanced. Test a separate authority with a disposable state directory,
then install the matching authority/supervisor and activate the signed staged
pointer on both module host and origin. Resume the watcher only after its source
branch and installed tools match the deployed protocol. The browser's release
gate can move an acknowledged idle tab to the new entry; visit the root URL if
an old tab cannot finish its previous protocol session during the cutover.


## Protocol-independent release discovery

The [stable release-control v1 envelope](rfcs/0008-release-control.md) is a small
JSON hello/upgrade protocol on the code host's `/release/events` WebSocket.
Browser discovery starts before Wasm initialization; the frame uses a separate
`release-watch` bootstrap companion. Each connection sends the client's current
release hash and app-state protocol. The server immediately replies with the
active hash, canonical entry path and current app-state protocol. An obsolete
CRDT protocol never prevents that reply. After a complete signed replica becomes
active, the publisher sends a loopback notification and connected clients receive
an upgrade event. Reconnecting rereads the pointer, recovering missed events.
Healthy clients perform no periodic release HTTP requests. WebSocket heartbeats
and bounded reconnect backoff detect lost connections; they do not poll releases.
The HTTP `/release` pointer remains for root bootstrap and older polling clients.

A finished gesture is always required. While connected, replacement also waits
for confirmed user edits. Disconnected clients may discard unconfirmed RAM edits
for this PoC; no durable client-state migration is required. Candidate frame
acceptance requires rendered output and, when replacing a connected host, a live
state stream. Replacing an explicitly offline host may accept rendered offline
output. Invalid signatures, targets or assets still reject a candidate; cached
rollback and a 30-second failed-candidate retry remain in place.

The code-control service runs independently of the authority, bound to loopback
`127.0.0.1:8879`. Start `release-control --releases <runtime>/releases` with an
`--origin` for each permitted browser code-host Origin. Caddy proxies only
`/release/events`; `/publish` is a local administrative hook. The module publisher
verifies/transfers the replica, atomically activates the origin pointer, then
POSTs `/publish` and requires acknowledgement. A notification failure is reported
and retried; reconnects also reread the last valid pointer. Future CRDT protocol
names in trusted signed manifests are accepted by the frame bootstrap, while
signature, target and asset validation remain mandatory.

The signed ARM build also produces `build/release-watch`. Install that companion
at `/opt/otrios-dashboard/release-watch` before installing the new supervisor.
Set the frame's `--code-url https://otrios.otri.ca` separately from its private
state `--url` (the service templates default `DASHBOARD_CODE_URL` to the public
code host and permit an environment-file override); this keeps code discovery independent of authority availability.
The bootstrap companion and release-control v1 must remain available as app/CRDT
implementations evolve. Updating this companion requires an explicit host update.

Clients predating independent discovery need one launcher-root visit if their
state protocol is already retired. Hard-refreshing an immutable release URL
still loads that old release. Existing HTTP-polling clients can discover this
upgrade through the retained `/release` pointer and then switch to events.
