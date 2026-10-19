<!-- SPDX-License-Identifier: Apache-2.0 -->
# Durable Canvas adapter acceptance — 2026-10-19

The browser and RK3126 photo frame now run the [RFC 0006 adapter](../rfcs/0006-durable-canvas-merkle-adapter.md).
The implementation is commit `6dbb379d3107f93f6c5a505b671c6d1015239ac0`, following
three logical commits for adapter/storage, recovery fixture isolation and signed
protocol staging. The pre-Merkle baseline remains tagged
`2026-10-19-canvas-apps-milestone` at `8dd9bc9`.

Signed release:
`0b52c65a500d3026a9de4825199329383d5269fed359cf54945a1890fd2c2fd7`.
It declares `otrios-canvas-merkle-state-v1` / `otrios-canvas-sync-v2`;
the existing app UI contract remains `otrios-canvas-v2`.

## Automated acceptance

- `bash scripts/check.sh`: passed, including 93 core unit tests, 27 Canvas
  integration tests, nine Merkle integration tests, separate-process replication,
  host policy/storage tests and Python publication checks.
- Strict workspace Clippy with all features/targets: passed.
- Four native worker tests: passed, including receiving peer edits and reconnect
  resynchronization, bounded presentation delivery and rejecting client state files.
- 31 Node host/cache/catalog/GPU tests: passed. The actual release Wasm also passed
  the Canvas-only export check. ARMv7 direct-host cross-compilation passed.
- Storage tests cover incomplete tails, interrupted checkpoint rotation and complete
  corrupt records. Real server tests cover a failed journal append without an ACK,
  restart and safe replay of the RAM intent.
- Adapter tests cover pending edits versus peer changes, a lost ACK followed by a
  newer peer assignment, duplicate/changed replay, malformed or forbidden fields,
  bounded receipts, expiry and trusted legacy current-value import.
- GitHub CI run 87 passed for implementation commit `6dbb379`.

## Real browser reconnect test

Two real browser tabs used an isolated authority on port 8898 and the final signed
Wasm/UI. After stopping that authority, the slider changed locally from 92% to
42%, with an explicit offline/unconfirmed status. Restarting the authority from
its checkpoint/journal confirmed 42% and updated the second tab. Reloading the
second tab retained 42% without a client document checkpoint. This was disposable
staging state; it did not replace live app values.

This test initially exposed a reconnect input freeze. The final implementation
keeps same-app reconnects editable and preserves the existing mount/input state;
presentation and accessibility values read the same pending overlay.

## Live cutover and policy

The watcher was paused before staging. The old authority was stopped for a cold
backup, then the new authority started against the new signed catalog while the
public module host still served the previous release. A field-by-field comparison
against the cold legacy files verified all seven migrated apps before exposure:

| App | Preserved fields |
| --- | ---: |
| Home | 0 |
| Component Playground | 5 |
| Layout Lab | 3 |
| Input Lab | 2 |
| Animation Lab | 4 |
| 3D Lab | 4 |
| Monitoring | 0 |

The public signed replica was then advanced. All 73 deployed module/access checks
passed against `https://otrios.otri.ca`: public asset digests and protocol,
private authority artifact isolation, exact Origin enforcement, Monitoring
cookie login, expiry/revocation behavior and administrative route isolation.
Private app mount policy is additionally covered by server integration tests.
The actual browser displayed public demos, migrated Input Lab values and Layout
Lab independently; choosing Monitoring showed its sign-in screen.

## Real native frame acceptance

The signed-release supervisor accepted the same release on the RK3126. A bounded
trial replaced the existing service's one renderer with the same signed binary
opened directly in 3D Lab, using the normal RGB565 DRM/GBM/EGL/GLES2 path and
accepted rotation/touch calibration. Weston and the browser service were inactive.

Changing the browser's count slider from 100% to 50% updated the native state and
rendered label; returning it to 100% restored the original value. Native readiness
reported count 64, `state_confirmed: true`, `pending_commands: 0` and the new release.
A captured native framebuffer showed the rendered 3D Lab, rather than relying only
on a transport ACK. A fresh browser reload also recovered the accepted value.

The temporary service override was removed, the regular supervisor restored to
Home, and exactly one native renderer confirmed. The continuous module publisher
and source watcher were restored on `feat/canvas-merkle-state`, with the activated
build fingerprint verified. Existing shared values were preserved.

This acceptance does not claim a new latency/FPS benchmark or a physical finger
test. The generic library supports causal register joins; Canvas currently uses
one authority's ordered field assignments, not multi-authority federation or
per-character text merging. Unacknowledged edits survive a live-process reconnect
in RAM; closing/crashing the client can lose them. General collection selectors,
arbitrary subtree queries and shared-memory IPC remain outside this increment.
