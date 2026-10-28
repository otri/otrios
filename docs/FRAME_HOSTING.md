<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame authority

The RK3126 hosts the Rust authority, signed browser/native release store and
Cloudflare Tunnel for `https://otrios.otri.ca`. The existing native display uses
the authority's trusted loopback listener. Prometheus and Grafana stay on Linode;
a frame timer queries Prometheus directly over HTTPS/Tailscale and publishes a
complete local Monitoring view. The Mac is only a builder and signing publisher.

## Services and boundaries

| Service | Role | Bound |
| --- | --- | --- |
| `otrios-authority` | Code, release hello/upgrades, durable app roots, auth, ink | 4 trusted / 64 public sessions; 192 MiB memory high, 256 MiB hard |
| `otrios-tunnel` | Existing hostname to `127.0.0.1:8877` | 96 MiB high, 128 MiB hard |
| `otrios-monitoring.timer` | Five-minute HTTPS collector and native chart projection | One-shot 240 seconds, 96 MiB hard |
| `otrios-dashboard-direct` | Existing KMS/GLES2 display and touch | Separate process; 128 MiB artifact disk cache |

Each WebSocket starts with a 4 KiB read buffer; the existing maximum message and
frame bounds still apply. These are configured limits, not measured working sets
or capacity guarantees.
Both HTTP listeners remain loopback-only. The tunnel exposes only the public
listener. Native state and code use `http://127.0.0.1:8876`, including the stable
v1 hello on `/session`; remote code still requires HTTPS and signed native bytes.
Browser clients retain the existing cookie/private-admission boundaries.

No kernel, graphics driver, panel, rotation, calibration, CMA or zram policy
changes are involved. Local confirmed edits and cached code remain usable when
the Internet is unavailable; public access and fresh metrics require networking.
Frame power and home networking determine public site availability.

For example, an authority configured with separate state and presentation paths
and the frame's peer limits can be started with:

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
each reconnect hello sends the current release hash and recovers missed events.
`/release/events` remains a control-only v1 alias for older updaters. `/release` remains a small HTTP pointer. Retired
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
closing one may lose unconfirmed edits. Connected upgrades wait for confirmed
intents and finished gestures; a disconnected client may discard unconfirmed RAM
edits when upgrading. Executable/font/blob caches are bounded
(about 512 MiB web; native agent default 2 GiB, overridden to 128 MiB by the
checked-in frame service). Ink expires after five seconds and is never
persisted. Legacy `--state FILE` only derives the app directory/presentation path;
prefer the explicit arguments above. Never run two authorities on the same state.

State recovery runs separately from code/control handling. Recovery failures
leave code and hello available, with state unavailable and public health 503.
An activation can retry recovery; existing compatible sessions receive a readiness
event when it succeeds. A whole-process crash interrupts all serving; that common
failure domain is the accepted cost of consolidation. Bounded queues and socket
write deadlines disconnect slow state readers for resync rather than accumulating
unlimited messages. They cannot eliminate TCP head-of-line blocking.

## Installation layout

- `/opt/otrios-authority`: ARM `dashboard-server`, `dashboard-monitoring`, verified
  ARM `cloudflared`, collector and activation helpers.
- `/etc/otrios-authority`: service-owned mode-0600 Monitoring login hash,
  dedicated Prometheus Basic credential, collector config, publisher public key
  and the existing tunnel credential/config. No signing private key or Linode SSH
  key is installed on the frame.
- `/var/lib/otrios-authority`: `releases/current.json`, the complete active signed
  release, `state.apps`, and the latest complete private presentation.
- `/opt/otrios-dashboard` and `/var/lib/otrios-dashboard`: existing native
  supervisor, bootstrap helper and independent disposable cache.

Units in `tools/dashboard/` run the authority/tunnel/collector as the `otrios`
service user. Initial provisioning creates that user and installs the units and
private configuration. Verify the signed store before starting the authority.
The native service uses `/etc/otrios-dashboard.conf` with both `DASHBOARD_URL`
and `DASHBOARD_CODE_URL` set to `http://127.0.0.1:8876`.

## Build and publication

Rust 1.90.0 and Cargo.lock remain pinned; there are no new crate dependencies.
`bash scripts/build-dashboard-armv7.sh` cross-compiles the authority, chart
renderer, direct host, bootstrap helper and app packer on the build machine.

The private watcher config accepts:

```json
{
  "frame": {
    "ssh": ["ssh", "-oBatchMode=yes", "-oStrictHostKeyChecking=yes",
            "-oUserKnownHostsFile=/PRIVATE/pinned-known-hosts", "root@FRAME_TAILNET_IP"]
  }
}
```

Keep the other established builder settings and the publisher keys on the Mac.
The watcher freezes source, builds and tests browser/ARM artifacts, signs and
stages the complete replica locally. With `frame` configured, it invokes
`deploy-dashboard-frame.py`; without it, local-only mode invokes
`activate-dashboard-release.py`. Explicit frame deployment is also supported:

```sh
python3 scripts/deploy-dashboard-frame.py --config /PRIVATE/config.json \
  --pointer /PRIVATE/build/staged-release.json \
  --server /PRIVATE/build/frame-dashboard-server \
  --monitoring /PRIVATE/build/frame-dashboard-monitoring \
  --bootstrap /PRIVATE/build/session-bootstrap
```

The receiver verifies every signed asset before changing the pointer. Authority
executables and helpers are installed through trusted SSH; they are not part of
the signed renderer update manifest. Server byte changes restart the authority,
which revokes RAM-only browser sessions; renderer-only updates retain hot reload.
`/admin/activate` publishes upgrades after the complete replica is installed.
Release discovery remains independent of state negotiation/recovery.

Published stores retain only the active release. The renderer temporarily keeps
its running release during activation; an active-only cache prune can follow
confirmed readiness. Old open browser releases may need a launcher-root reload
after pruning. There is no historical rollback promise.

```sh
python3 scripts/prune-dashboard-releases.py --releases /var/lib/otrios-authority/releases --apply
python3 scripts/prune-dashboard-releases.py --releases /var/lib/otrios-dashboard --native-cache --apply
```

The pruner locks authority activation, refuses missing/invalid pointers and
symlink release entries, and removes only obsolete digest-named directories.
Stop the native updater before manually pruning its cache during a cutover.
It never deletes durable app roots or unrelated hardware recovery backups.

## Signing and activation

The frame activation CLI verifies the signature, manifest, exact public/private
paths, sizes and every digest before atomically advancing `current.json`. It
notifies the trusted-only `/admin/activate` hook; code events do not wait for
catalog refresh or successful state decoding. A notification failure is reported
and retried; reconnects also reread metadata.

The public catalog admits UI/State references and hashes. UI/behavior payloads
under `authority/apps/<id>/` travel only through authorized mounts. Their presence
in the local signed store never grants public static access. Native execution
requires the pinned Ed25519 signature; browser resources rely on HTTPS and the
manifest hash closure, without independently verifying the detached signature.
A signature does not sandbox trusted native publisher code.

The Mac build/signing workspace is under
`~/Library/Application Support/OTriOSDashboard`; the frame holds only the
publisher public key. Unchanged authority bytes do not trigger a restart.

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

## Hard cutover and checks

Qualify a disposable ARM authority first. Pause the watcher and snapshot the
current frame service files and stopped canonical checkpoints, journals and latest
presentation before replacing matching binaries and helpers. Verify the signed
release and data digests, preserve ownership and start the existing frame services.
Never run independently writable authorities against the same state or route
traffic between them. Preserve canonical app data through activation and rollback.
The original Mac-to-frame migration and its measurements are historical; see the
[frame authority cutover report](experiments/frame-authority-2026-10-20.md).

Run `bash scripts/check.sh`, all-feature Clippy, browser worker/session/cache
tests, real server-process checks, and the public host conformance checker.
For a production-origin check without the private login:

```sh
python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only
```

This skips authenticated login; the full checker still requires `--credentials`.
On hardware verify stable loopback hello, native signed/confirmed readiness,
direct Prometheus collection, shared browser/native mounts, peer admission limits,
durable restart, public/private route isolation and release activation.
Measure service memory, login peaks and rendering/ACK behavior under browser
load. A framebuffer capture does not establish physical touch/color acceptance.

```sh
systemctl status otrios-authority otrios-tunnel otrios-monitoring.timer otrios-dashboard-direct
curl -fsS http://127.0.0.1:8876/health
journalctl -u otrios-authority -u otrios-monitoring -u otrios-tunnel --since today
```

After acceptance remove only retired OTriOS replica containers/volumes, publisher
and release-control launchers, their binaries and ports 8878/8879. Preserve tunnel
configuration/credentials, signing/auth material, durable state, immutable cached
releases, the benchmark branch and the rollback snapshot. Other infrastructure
and Supabase resources are outside scope. Historical experiment reports retain
their original topology and measurements; current operation follows RFC 0009.


## Isolated session stress test

`canvas-session-stress` uses the real Canvas client, signed app catalog, Component
Playground behavior and state protocol. Run the generator on the build machine;
`stress-frame-authority.py` stages a disposable ARM authority on the frame with
fresh app state. It leaves the production authority and display running. The
configured SSH array must retain the established pinned host key and identity.

```sh
cargo build --release --locked -p otrios-dashboard \
  --features canvas,host-sync --bin canvas-session-stress
python3 scripts/stress-frame-authority.py --config /PRIVATE/config.json \
  --client target/release/canvas-session-stress --server /PRIVATE/ARM/dashboard-server \
  --catalog /PRIVATE/app-catalog.flex --output target/frame-stress-RUN
```

The default ramp opens 64 through 4,096 negotiated idle sockets, then 32 through
512 mounted readers, then 1 through 64 writers among 64 mounted clients at a
requested 10 cycles/second. Each write cycle edits Linear, Rotary and both Touch
Pad coordinates. Mounted mode waits for durable confirmation between cycles;
report achieved throughput alongside latency, because the requested rate can
exceed that mode's confirmed throughput. `--phase active --open-loop --hz 30`
instead offers drag events on the input clock, allows the real client's bounded
intent coalescing, and releases gestures when input stops. In drag mode ACK
samples describe fully confirmed spans, not individual event latency. Both modes
check that every client settles to the same confirmed four values.

Counts are configurable with `--socket-counts`, `--mounted-counts`,
`--active-counts`, `--clients` and their `--max-*` bounds. The runner records
binary hashes, per-case setup/input/settle times, ACK quantiles, traffic, generator
loop timing, one-second CPU/RSS/cgroup/swap samples and production health probes.
CPU percentages for a service use one core as 100%; system CPU uses all cores.

A temporary Tailscale-only TCP relay exposes the disposable public listener;
it contributes network/relay cost to the result. This does not measure browser
rendering, Cloudflare Tunnel capacity, physical touch responsiveness or long-term
soak behavior. The target and relay each have a 256 MiB cgroup cap. Tests stop
when available system RAM drops below 256 MiB or a production health probe fails.
The runner removes temporary units, relay and test state on exit. A cgroup cap,
client deadline or generator bottleneck must be identified separately from a
hardware capacity limit. See the [dated experiment report](experiments/frame-authority-2026-10-20.md)
for measured results.


Complete-client burst qualification includes the Playground ink welcome and
real pointer down/move/up paths, using a separate external generator:

```sh
cargo build --release --locked -p otrios-dashboard \
  --features canvas,host-sync --bin canvas-burst-stress
python3 scripts/stress-frame-authority.py --config /PRIVATE/config.json \
  --client target/release/canvas-burst-stress --server /PRIVATE/ARM/dashboard-server \
  --catalog /PRIVATE/app-catalog.flex --output target/frame-burst-RUN \
  --phase burst --cohorts 64 --seconds 10 --hz 30
```

The sparse profiles last 100 seconds, with one one-second gesture per client.
Other profiles use `--seconds`; `--burst-overlap` can fix an aligned writer count
for comparisons. The [burst report](experiments/frame-burst-capacity-2026-10-20.md)
defines prospective latency/coverage gates and records which cohorts meet them.
The older state-only generator does not qualify ink-enabled clients.
The revised qualification passed at 16 complete clients sharing one room with
1% intended input duty and three simultaneous 30 Hz gestures. The 32- and
64-client collision probes missed the remote ink latency budget. The configured
64-public-session admission quota is separate from this responsiveness target;
neither result establishes a physical connection maximum.
