<!-- SPDX-License-Identifier: Apache-2.0 -->
# Single-queue dispatch prototype — 2026-10-19

The prototype removes the UI's second pending map and flush clock. Continuous
numeric assignments go directly into the adapter's bounded RAM overlay, retaining
one immutable issued write. The default remains 33 ms; later samples cannot push
a ready-to-send deadline back. Idle edits, final release, discrete actions, ACK,
resync and replay preserve their bypass rules. No state/node/transport format or
merge rule changed. [RFC 0007](../rfcs/0007-bounded-replication-pipeline.md) describes
policy and bounds. This is a feature-branch prototype, **not a live client rollout**.

There is no demonstrated overall performance win. At 10 Hz the median observer
payload/input is identical and timing is similar. At 120 Hz the candidate delivers
40 distinct values instead of 38, so its somewhat larger payload is not a codec
regression; CPU/input is also somewhat higher. Artificially delayed trials do not
show a reliable freshness or CPU improvement. Keep the live client release until
further transport work provides stronger evidence. A simpler queue alone should
not be advertised as an acceleration.

## Method and results

Reference `0732fa14aebb9d8b31fb55f9541c39c28b6789e2` includes the same open-loop
harness and bounded socket-inbox fix, retaining the previous client staging queue.
Candidate `fe01be5d88857189bfefe556513e19ad8ef2b432` includes dispatch runtime
commit `d5e369f`. Frozen source/binary hashes are recorded in the linked metadata.
The same catalog/UI/schema/behavior from signed release
`ed105107522f5db8e9f413388ab83724ed6133e76590390314e795b9b3f75219` is used for
both. Rust 1.90.0 optimized ARMv7 clients run on the RK3126/Cortex-A7 frame;
matching disposable durable authorities run on the Mac. The normal direct Home
renderer remains running, with no second renderer or Wayland instance.

Three alternating repetitions per case, fresh state for every trial, 20 settled
warm-up assignments followed by 180 timed numeric input events. Input continues
without waiting for each ACK. A final pointer-release event flushes the latest
value; both writer confirmation and observer convergence are required. Observer
latencies cover only values actually delivered, not assignments consolidated
before issue. Never count a skipped input as a zero-latency delivery. Final
settlement is one sample/trial, not a percentile distribution. Values below are
medians of three run-level values, including median per-run p95.

| Link / input Hz | Build | Distinct observed / 180 | Delivered age p95 ms | Final settle ms | Observer payload B/input | Paired CPU ms/input* |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| direct / 10 | reference | 180 | 102.86 | 26.70 | 1643.0 | 18.24 |
| direct / 10 | candidate | 180 | 102.18 | 27.39 | 1643.0 | 18.09 |
| direct / 120 | reference | 38 | 113.54 | 49.92 | 346.9 | 4.17 |
| direct / 120 | candidate | 40 | 109.06 | 38.07 | 365.1 | 4.51 |
| delayed / 120 | reference | 11 | 222.72 | 203.41 | 100.4 | 5.94 |
| delayed / 120 | candidate | 10 | 225.05 | 222.34 | 91.3 | 5.97 |

*CPU is wait4 user+system time for both headless clients, including mount and the
20 warm-up assignments, divided by all 200 input events. It excludes the relay,
SSH, authority and live renderer; it is not whole-device CPU. Payload excludes
mount/warm-up and includes the session channel byte, but excludes TCP/IP/Tailscale
overhead. Scheduling lateness, actual input span, per-run CPU/RSS and wire counts
remain in raw output. A lower payload caused by fewer delivered values is not a
compression result.

The direct case uses normal Tailscale transport through frame loopback relays.
The delayed case adds 40 ms before forwarding each relay chunk in **each** direction;
it is an artificial backpressure test, not a measured Internet RTT. Chunking and
coalescing can affect its delay. CPU/governor/thermals and background services were
not pinned. Builds/checks finished before the final matrix. Identical-reference
pilot trials and earlier engineering attempts are excluded.

The 18 trials restored the exact original Tailscale Serve JSON and removed their
frame experiment directories. Live state was never used/reset. These are headless
replication measurements, not physical touch-to-photon or GPU benchmarks. Main
and the pre-Merkle benchmark reference are unchanged.

## Verification and next work

Required repository checks, workspace all-feature/all-target Clippy and native
worker tests pass locally. Tests cover a deadline that later samples cannot defer,
ACK priority, final flush, monotonic clocks, immutable issued replay, field bounds,
RAM reconnect and existing policy/durability/snapshot semantics. Numeric staging
uses a bounded stack array and duplicate checking uses a field bit mask; maps,
nodes, packet buffers and issued vectors still allocate. No whole-transaction
allocation-free claim or new dependency is made.

The earlier unified-session CI failure was reproduced with a blocked state worker
and four credited frames. The inbox now accepts that bounded burst while control
traffic remains independent; CI run 127 passed this fix. A later CI run exposed an
independent bind/drop/rebind port race in parallel fixtures. Fixtures now ask the
actual child server to bind port 0 and read its bound addresses, rather than
choosing a supposedly free port in advance. This does not serialize or skip tests.
The socket fix can be rolled out independently of the unpromoted client prototype.

Next compare a schema-defined packed envelope with the exact same canonical nodes,
then measure bounded optional compression on identical uncompressed batches.
Neither is implemented by this dispatch experiment.

Raw evidence: [direct trials](data/canvas-dispatch-2026-10-19/direct-results.jsonl),
[direct metadata](data/canvas-dispatch-2026-10-19/direct-metadata.json),
[delayed trials](data/canvas-dispatch-2026-10-19/delayed-results.jsonl),
[delayed metadata](data/canvas-dispatch-2026-10-19/delayed-metadata.json).
