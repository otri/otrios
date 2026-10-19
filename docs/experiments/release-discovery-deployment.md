<!-- SPDX-License-Identifier: Apache-2.0 -->
# Independent release discovery and replication rollout — 2026-10-19

Historical interim deployment. The [stable hello/event rollout](release-hello-deployment.md)
supersedes its five-second polling and disconnected-edit waiting policy.

Final source commit: `661667ea49260ac489b37bcdfd786c80c556cba7` on
`feat/canvas-replication-pipeline`. Signed release:
`b7dd4d99f4d4513f140cc5980767dc87aa29aefe9ce4ed714a2623d5f62d9c71`.
The public OrbStack module host, private origin and native photo frame use the
same release. Main remains at `9de7618`; the benchmark branch remains at
`abf122ab2dbd2c37c9b66d6c183a1cf860698df6`. No merge accompanies deployment.

## Failure and change

Mac Safari had remained at release `ef2c34…`, using the retired `/demo/stream`
state protocol. Its upgrade notification arrived through that same stream, so
it could no longer discover an upgrade. Hard refresh reloaded its immutable old
URL. That release also encoded Touch Pad Y in the opposite direction to the
Canvas app runtime, producing mirrored trails between versions.

The browser now discovers releases directly from the code host's uncached
`/release` pointer, immediately and every five seconds. Discovery starts before
Wasm initialization, survives state-stream failure, limits descriptor size and
request time, deduplicates unchanged pointers and resumes after back/forward
cache restoration. The code host's atomic pointer selects a release; state-stream
hints no longer select browser code. The native supervisor also polls this HTTP
pointer independently and ignores stale notice files. Failed candidates retry
after a bounded 30-second backoff so a temporary state outage does not
permanently blacklist the current code pointer.

The runtime separates `reload_ready` from successful mount/stream confirmation.
A clean initial mount or reconnect cannot strand a runtime. Pending RAM intent,
issued writes and captured input still block automatic replacement. Superseding
an upgrade reopens the browser command gate. If an incompatible authority can
never confirm existing edits, automatic replacement deliberately continues to
wait; a user can close/reload and lose those edits under the RAM-only policy.
Clients older than this fix need one root-URL visit if their previous state
protocol has already been retired; immutable historical artifacts are unchanged.

The deployed replication increments are negotiated binary response fields,
UI-requested signal retention, bounded reusable Merkle encoding workspace and
explicit TCP_NODELAY on authority sockets. Canonical hashes, policy enforcement,
durable-before-confirm ordering and RAM-only client edits remain unchanged.
Packed FlatBuffers envelopes, configurable dispatch deadlines and compression
are still later increments. The [earlier controlled measurements](canvas-replication-pipeline.md)
record their individual costs; this rollout is an acceptance check, not a new
latency or whole-device performance benchmark.

## Evidence

- Required `bash scripts/check.sh` passes. The first parallel run encountered
  `AddrInUse` during process-test startup; the complete serial rerun passed.
  Strict all-feature/all-target Clippy passes, as do all five native worker tests.
- The signed build compiles the browser Wasm and ARMv7 direct host and passes all
  35 Node tests. New tests cover discovery with no state protocol, transient host
  failures, size/time bounds, invalid entries, cancellation/supersession, pending
  edits, active gestures and clean offline reload readiness.
- A real in-app browser loaded the preceding production-signed release
  `d5bc2e6ae9daf491fb5bda690868f60e415b4ffb1c6cf451c30f5594e3d18270`
  through an isolated loopback code host. Its state endpoint returned 410 for
  every connection. Advancing only that code host's pointer moved the browser
  automatically to the final production-signed release, without a manual
  refresh or successful state connection. Production publisher trust was unchanged.
- The native cold-start test rejects the initial WebSocket mount with HTTP 410
  and still receives a presented-frame record eligible for clean code replacement.
  Blank startup frames report reload readiness; captures wait for a qualified app.
  A supervisor lifecycle test fails the first candidate while the authority is
  unavailable, retains the verified older host and accepts the same candidate
  after the 30-second retry backoff. All 22 signing/supervisor tests pass.
- The optimized authority ran against fresh disposable stores and the actual
  signed catalog. Both legacy-vector and binary clients completed five measured
  paired durable writes plus two warm-up writes. Existing process tests cover
  private leases, restart, receipt replay, journal failure and app isolation.
- Live Safari moved automatically from the previous compatible release to the
  new release, retained Component Playground navigation and reported
  `Live shared space`. This verifies the rollout transition; the separate
  loopback test verifies discovery when the state protocol is unavailable.
- The native frame reports the same signed release, confirmed state,
  `reload_ready: true`, no pending commands and exactly one native renderer
  (PID 27149 at acceptance). Its direct GLES2/RGB565 output remains 800×1280 with
  rotation 90 and calibrated touch. The final native capture matches the inspected
  Home capture pixel-for-pixel; its RGB digest is in the acceptance record.
  Physical finger interaction was not re-tested in this rollout.
- All 73 deployed public-host checks pass: exact asset digests, public catalog
  and Wasm, exact Origins, private artifact denial, Monitoring login/logout and
  cookie revocation, plus denied administrative routes. Public app controls
  continue to share the frame's state.

## Activation and recovery

The source publisher was paused while the inactive signed candidate was built
and tested. The authority was stopped briefly for a consistent private backup
of its existing app stores and presentation, then replaced with the matching
optimized binary. The previous binary and configuration were retained for
rollback; no store was reset or replaced. The frame supervisor was updated and
restarted before activating the complete verified module replica and origin
pointer. Authority restart revokes its memory-only Monitoring sessions.

The source publisher is resumed, pinned to the exact deployed commit. The public
module replica and origin pointers match, and the installed authority's SHA-256
matches the tested release binary. The isolated loopback test server and browser
tab were closed. No DNS, tunnel, kernel, display-mode or touch-calibration change
was made. See the [machine-readable acceptance record](data/release-discovery-2026-10-19.json)
and [operational release-discovery contract](../DASHBOARD_LIVE.md#protocol-independent-release-discovery).
