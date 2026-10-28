<!-- SPDX-License-Identifier: Apache-2.0 -->
# Canvas foundation qualification and rollout — 2026-10-20

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/canvas-foundation.md) remain available at the pinned historical revision.

The single RAM intent queue and coherent admitted-UI replication are deployed on
the browser and photo frame. Sync v3 includes only required state fields, signal
windows and that writer's durable receipt. The authority retains complete stored
trees and enforces policy/durability before publication. Control hello remains
v1; canonical node/checkpoint/journal formats remain unchanged. The unused
Snapshot override and parallel client state map are removed.

This promotes structural simplification with lower replication traffic. It does
not establish a uniform latency or CPU speedup. [RFC 0010](../rfcs/0010-runtime-foundation.md)
tracks remaining foundation work; packed transport and compression remain open.

## Complete-client frame comparison

Reference `0732fa14aebb9d8b31fb55f9541c39c28b6789e2` matches the previously live
authority/client implementation, including the bounded-inbox fix. Candidate
`c15221a4ebf1e503e306f0b441a18614e8c2706d` adds single-queue dispatch, intent
payload accounting, coherent coverage and writer-local receipts. This compares
the combined foundation; it does not attribute every difference to one change.

Both use pinned Rust 1.90.0 release ARMv7 benchmark builds, matching Mac
authorities and byte-identical signed app catalogs/UI/behavior. Reference release
is `ed105107522f5db8e9f413388ab83724ed6133e76590390314e795b9b3f75219`; candidate
release is `ac0bbd21aa32155029f10df1fcdc56245ed4b9da6628a315124fd571b7ab062c`.
Separate signed manifests declare their matching state protocols. The runner
requires identical catalog bytes and permits a per-side release. No unsigned
release or mixed-codec authority is used.

Each trial uses fresh durable stores, 20 settled warm-up assignments and 180
open-loop input events. Two independent client instances/WebSockets run in one
ARM benchmark process on the RK3126 frame. A final pointer release must durably
confirm and converge at the observer. Three repetitions alternate build order;
10/120 Hz direct trials and 120 Hz delayed trials total 18. A six-event smoke
comparison is excluded. The normal direct Home renderer stays running; the
headless benchmark creates no additional graphics context.

The delayed relay waits 40 ms before each chunk in each direction. This is an
artificial backpressure scenario, not Internet RTT. No compilation runs during
measurements. CPU governor, thermals and background services are not pinned.
Temporary Tailscale routes are restored exactly and frame experiment files are
removed after each matrix.

Medians of three run-level results, including median per-run p95:

| Link / requested Hz | Build | Distinct observed / 180 | Delivered age p95 ms | Final settle ms | Observer payload B/input | Paired CPU ms/input* |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| direct / 10 | reference | 180 | 100.14 | 36.03 | 1643.00 | 18.11 |
| direct / 10 | candidate | 180 | 98.18 | 28.04 | 1217.00 | 17.56 |
| direct / 120 | reference | 39 | 107.40 | 40.33 | 355.98 | 3.91 |
| direct / 120 | candidate | 38 | 114.55 | 53.25 | 256.92 | 4.04 |
| delayed / 120 | reference | 11 | 301.95 | 301.95 | 100.41 | 6.20 |
| delayed / 120 | candidate | 10 | 279.19 | 272.55 | 67.61 | 5.97 |

*wait4 user+system CPU for the process owning both clients, including mount and
warm-up, divided by all 200 input events. It excludes authority, relay, SSH and
GPU/device CPU. Payload includes the session channel byte, excluding network
headers. Delivered age includes only actually observed values; consolidated
inputs are never zero-latency samples. Final settlement is one value/trial, not
a percentile distribution.

At 10 Hz, both deliver all 180 values and received payload falls 25.9%. At high
rates, distinct delivery counts also differ: byte changes cannot all be labelled
codec savings. Rapid direct-input p95/final settlement is somewhat worse while
delayed trials improve; three unpinned repetitions establish no universal
latency/CPU win. The architecture is selected for one edit owner, bounded intents,
narrower subscriptions and lower traffic with preserved final convergence.

## Behavior and host qualification

- Required repository checks, strict workspace all-feature/all-target Clippy,
  Linux CI run 139, 40 signed-build browser checks, 27 signing/supervisor checks
  and six native worker checks pass.
- Core tests cover deadlines, final release/cancel, discrete/text dispatch,
  whole-batch payload rejection and ages across replay/restore. Coverage tests
  prove private ancestor exclusion, missing targets, coherent paths, slow ACK,
  resync and cancellation. Real-process tests exercise both subtree transport
  and the complete Canvas authority, including durable failure/restart.
- The previous state v2 codec gets the new release through opening control v1
  while its incompatible state bytes remain blocked.
- A browser mounts all seven app entries. Component Playground's slider and
  label agree; a separate native process durably sets 73% and the browser follows.
  Layout Lab toggles, Input Lab types `abc` through its keyboard and retains it
  after reload. Focus/keyboard remain local. Animation/3D mount independently;
  browser frame counters continue advancing under the selected state-load mode.
  This observation is not a calibrated GPU benchmark.
- Anonymous Monitoring navigation shows sign-in while public demos remain
  usable. Isolated and deployed public conformance each pass 81 checks, including
  private static-path denial, exact Origin, session expiry/revocation policy and
  public code availability. Private mount enforcement is separately covered by
  real-server tests.

## Live rollout and cleanup

The authority, installed publisher helpers and watcher are pinned to `c15221a`.
Before restart, a private stopped-state rollback snapshot retained the old
binary/helpers/configuration/pointer and complete app stores. The matching new
authority starts with code/control available, and normal signed activation
verifies the new release before notifying it. Durable app state is retained;
no checkpoint import or document reset is needed.

The frame automatically downloads/verifies the new signed native executable.
Renderer PID 3613 reports the same release, admitted/confirmed Home, zero pending
commands, RGB565, rotation 90 and calibrated touch. Exactly one renderer and no
Weston process are present. [Frame acceptance](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/frame-acceptance.json)
and the raw [scanout capture](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/frame.png) confirm
rendered output; the capture uses portrait scanout coordinates. Physical touch,
physical orientation/color and reboot are not re-tested by these observations.

The live public browser mounts the same signed release. The temporary browser
authority and build container are stopped; the one production authority, tunnel,
origins and access policy remain in place. Main and the historical benchmark
branch are unchanged, and PR #6 remains a draft.

The signed renderer and headless benchmark use the same source/toolchain with
their standard feature sets; they are different executables. Their distinct
binary hashes are recorded. No headless measurement is presented as renderer FPS.

Evidence: [direct trials](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/direct-results.jsonl),
[direct metadata](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/direct-metadata.json),
[delayed trials](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/delayed-results.jsonl),
[delayed metadata](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/delayed-metadata.json),
[deployment identifiers](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/deployment.json),
[staged browser](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/staged-browser.png) and
[live browser](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-foundation-2026-10-20/live-browser.png).
