<!-- SPDX-License-Identifier: Apache-2.0 -->
# Read-only Canvas admission on the frame — 2026-10-20

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/canvas-readonly.md) remain available at the pinned historical revision.

Source improvement committed as `214ea5e10c565f319abebbd39eec94bd9d814f36`;
not deployed at measurement time. Reference is deployed foundation `c15221a`.
A schema with no writable fields needs an ephemeral lease and acknowledgements,
not a durable writer receipt. New read-only sessions now leave the document root,
checkpoint and journal unchanged. Read-only writes and uncertain resume reject.
The initial Mounted packet already contains its offered revision; the authority
no longer repeats it as Sync before the client has acknowledged it.

| Median of six trials per binary | Foundation | Read-only lease |
| --- | ---: | ---: |
| Durable changes across 150 mounts | 150 | 0 |
| Journal delta payload bytes across mounts | 242,300 | 0 |
| Inner wire bytes per mount | 1,248 | 848 |
| Admission/offer/receive/ACK p50 µs | 24,911.821 | 1,068.521 |
| Admission/offer/receive/ACK p95 µs | 40,535.667 | 1,301.708 |
| Final checkpoint bytes | 46,777 | 741 |
| Root unchanged | no | yes |

This is an **in-process admission benchmark**, not end-to-end network latency or
storage fsync cost. Old receipts accumulate and increase scan/hash work within
the trial; this result does not imply every steady-state edit becomes 23× faster.
The useful structural result is zero durable mutation for a read-only subscriber.

Both frozen sources use the same measurement-only example, Rust 1.90 release,
ARMv7 Linux hard float on the RK3126 Cortex-A7. Each trial has 20 warm-up and
150 measured fresh writer nonces. Two process blocks reverse order; each process
runs three trials, for twelve total. The existing direct renderer stays running.
Clocks/thermals are unpinned. No network, filesystem, GPU or private payload is
part of the measured loop. Temporary frame binaries were removed.

[Raw trials](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-readonly-2026-10-20/results.jsonl) and
[commit/archive/binary/harness metadata](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/canvas-readonly-2026-10-20/metadata.json)
allow attribution. The harness later removes the obsolete binary-mode setter for
sync v4; the metadata hash identifies the exact v3 harness used here.

Core coverage: 1,030 read-only admissions do not consume writer slots, sessions
still follow authority updates, write/resume policy rejects atomically. A real
server test mounts Monitoring eight times without changing checkpoint/journal.
The private-candidate expiry test now advances directly from Mounted to the next
mount rather than waiting for the deliberately removed duplicate Sync.
