<!-- SPDX-License-Identifier: Apache-2.0 -->
# Unified service acceptance — 2026-10-19

The deployed dashboard now uses one Rust serving process and one remote WebSocket
per client. Trusted port 8876 and public port 8877 share that process. The existing
Tailscale route and Cloudflare Tunnel target are unchanged. The source watcher is
development tooling; the monitoring collector is scheduled work. Neither is a
request-serving dependency. [RFC 0009](../rfcs/0009-unified-server-session.md) records
the contract and fault model; [operations](../DASHBOARD_LIVE.md) describes deployment.

A later [server-only burst fix](canvas-dispatch.md#server-only-rollout) advances the
runtime pin while retaining this signed client release.

## Accepted release

- Runtime/tooling commit: `d1d6bd6d37d3827e444f0c3bd75327a5aaa67d9b`.
- Signed browser/native release: `ed105107522f5db8e9f413388ab83724ed6133e76590390314e795b9b3f75219`.
- Opening control envelope remains v1; state codec remains `otrios-canvas-sync-v2`.
- State/pending intent remain RAM-only on clients, with durable state and policy
  enforcement on the server. No canonical Merkle or persisted-state format changed.
- The publisher follows the exact tested runtime commit, with the retired
  `module_volume` option removed. Its frozen-source fingerprint matches that commit.
- `main` remains `9de7618810cfe93939da00dd21c8149340f8206e`;
  `bench/canvas-adapter-frame` remains `abf122ab2dbd2c37c9b66d6c183a1cf860698df6`.
  Draft PR #6 remains unmerged.

## Automated and real-host checks

`bash scripts/check.sh` passed, including Rust process/durability/policy tests,
27 signing/supervisor tests and the existing flip/counter checks. Strict dashboard
Clippy passed with all targets/features; native worker tests passed (6). The signed
build passed 40 Node tests covering bootstrap, app worker, GPU/cache and catalog
contracts, plus browser syntax and shader generation checks. Both Wasm and ARMv7
release builds completed. No dependencies or unsafe-code boundaries were added.

Real server tests cover incompatible hello versions/codecs, binary-before-hello
rejection, metadata while the app lock is held, missing/invalid pointers, recovery
failure, recovery on activation without restarting an existing session, scoped
private mounts, logout/expiry, durable confirmation and multiplexed ink. Client
mailboxes are bounded; stale data/output cannot cross connection epochs. These
checks exercise boundaries, not a production-scale load test.

An isolated real browser admitted Home and Component Playground. Private health
reported exactly one public session before and after navigation, state editing
and ink subscription. With a deliberately invalid disposable state path, the
server kept signed assets and hello available. The browser automatically moved
from signed fixture `9848bef6444a2129df87ba748cad96d576cef836c98a8865d95b35c85d5deb74`
to the accepted release after activation, while state stayed unavailable. The
unavailable-state conformance mode passed 62 checks. There are no periodic release
fetches in the healthy session owner; this is also asserted in its Node tests.

The open production browser upgraded from the former signed release automatically
and admitted Home. The public conformance tool passed 81 checks: exact signed
assets, MIME/cache headers, private artifact/admin denial, exact Origins, stable
hello and Monitoring login/logout/revocation. Rust tests separately encode and
exercise actual private app mounts. A server restart revokes browser sessions.

On the frame, the supervisor exercised cold bootstrap, then cached-host resume.
Final process/socket inspection found one renderer, one established TLS connection
owned by it, no permanent release watcher, no remaining bootstrap helper and no
Weston. Readiness reported the accepted release, admitted Home UI/state, confirmed
state, calibrated touch, DRM, rotation 90 and RGB565 (5/6/5/0). A captured framebuffer
showed the rendered Home scene. Physical finger input was not re-tested.

A bounded 45-second native 3D trial stopped the supervisor first and restored it
on exit; it never ran two renderers. A browser Play edit changed native presentation
from paused to playing at 46 tetrahedra, with confirmed state and one trusted
session. The observed sync-receipt-to-present sample was 44.06 ms; it excludes
upstream input/network time and is not a latency percentile. Playback was restored
to paused afterward. The trial measured 55.81 kernel page-flip FPS, p50 17.653 ms
and p95 17.659 ms, with 36 missed vblanks. It mixed paused/playing periods and is
smoke evidence, not an A/B performance claim. Raw bounded trial data and final
readiness/process counts are in [the acceptance JSON](unified-session-deployment.json).

## Cleanup and rollback

Before replacement, private service/configuration files, the old executable and
stopped durable state were snapshotted. The duplicate module store was archived
and verified against 23 content-addressed manifests and 336 asset digests before
removal. The rollback snapshot and immutable releases remain private/outside Git;
no credentials or signing keys were committed.

Removed the standalone release-control server, replica publisher LaunchAgents,
Caddy container and duplicate Docker volume, their runtime files, obsolete client
socket code and old source/tests. Ports 8878 and 8879 have no listener. The same
server PID owns 8876/8877. The ARM builder is stopped, with its image/cache retained
for future builds. Unrelated infrastructure and Supabase containers were untouched.
Only the unchanged live tunnel configuration/credentials remain in the former
`module-host` directory; its path was retained to avoid changing tunnel setup.

The frame had rebooted into the old Wayland boot service before this rollout.
That boot service is now disabled, the direct service is enabled, and automatic
fallback to the retired Wayland host was removed. The obsolete release-watch
executable was removed after acceptance; a private frame rollback archive remains.
The direct service restarted successfully after the trial; no physical reboot was
performed as part of this acceptance.

Operational rollback must restore the matching old server, proxy/control and frame
agents together, plus their release pointer. A browser may need a launcher-root
visit across retired protocols. The retained v1 alias supports old release discovery,
not old state transport. One-process fatal failure affects both code and state;
recoverable state/codec failures instead leave code/control available. TCP still
has head-of-line blocking despite bounded, prioritized application queues.
