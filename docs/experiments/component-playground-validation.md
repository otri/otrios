<!-- SPDX-License-Identifier: Apache-2.0 -->
# Component playground: native update preparation

2026-10-19, RK3126 Cortex-A7 photo frame, existing direct GLES2/RGB565 service
running alone as the display owner. The user reported fast browser-to-browser
updates and accurate but slow physical touch tracking on the frame.

`dashboard-profile` replays 30 captured slider updates against a disposable
in-memory replica. It resolves the actual responder coordinates and checks the
final shared value. It never connects to sync, opens a display, or writes a state
file. Build the same probe with the old core (82c762a) and updated core, then run
both on the frame against the **same frozen** state and presentation files:

```sh
cargo build --release --locked -p otrios-dashboard --bin dashboard-profile \
  --target armv7-unknown-linux-gnueabihf
dashboard-profile STATE_FILE PRESENTATION_FILE
```

The private input files stay on the frame and are removed after the run. No
Monitoring content or credentials appear in the output. The normal display
service remains active, so these are wall-clock CPU preparation costs under
live load, not isolated GPU or input-to-panel measurements.

| Mean milliseconds per shared slider update | Before | After |
| --- | ---: | ---: |
| Input and CRDT publication | 233.676 | 30.863 |
| Shell scene resolution | 2.338 | 2.315 |
| Triangle generation | 11.282 | 4.510 |
| SDL texture-copy regions | 10.272 | **0 in direct host** |
| Returning serialized document | 15.440 | 0.038 |
| Vertices in final scene | 35,898 | 16,176 |

The probe deliberately still measures the optional SDL calculation (5.172 ms
after), but the direct host no longer calls it. Selected CPU stages total about
273 ms before and 38 ms after. This **does not establish a frame rate**: it excludes
state-file fsync, networking, GPU uploads, page flips and touch scheduling. Local
previews between shared publications avoid the CRDT publication stage entirely.

The main bottleneck was requesting full Automerge Changes just to inspect their
roots, which re-encoded the whole history on every edit. Publication now checks
new change metadata against already validated heads. Fresh loads still validate
all roots; an imported second root is rejected even when the valid seed is also
present. The exact size-limited serialization is retained for persistence and
network handoff rather than repeatedly re-encoded. Base and private presentation
snapshots retain separate scene caches and revalidate changed bindings/layouts.
Horizontal glyph runs replace adjacent pixel rectangles without changing pixel
coverage, clipping, primitives or responder geometry.

Validation: required `scripts/check.sh`, strict all-feature/all-target Clippy,
native bounded-mailbox/no-client-persistence test; coverage tests for glyphs and fractional
clipping, pinned snapshot reuse/invalidation, and rejection of grafted history.
Physical touch tracking was accepted before this optimization; perceived update
speed after deployment still needs confirmation on the panel.

## Follow-up: independent local feedback

The user found that release improved dragging but remained slow. Native touch,
scene preparation, document merges and fsync were still serialized on one worker.
The native host now gives touch/scene preparation a presentation model with
bounded pending intent. A separate application worker owns reconciliation,
network handoff and durable saves. Absolute control values coalesce independently;
discrete commands retain order. Acknowledgements cannot erase newer pending
values, and release feedback stays visible until the corresponding published
replica is acknowledged. Stop applies the final commands and waits up to three seconds for authority acknowledgement; it never writes a client checkpoint. Automatic reload requires confirmation before initiating stop.

The native regression test holds the application worker behind a deterministic
gate and verifies that drag/release feedback completes while the state file is
unchanged, then releases the gate and verifies the exact value survives on disk.
`dashboard-profile --deferred STATE_FILE PRESENTATION_FILE` measures just the
presentation path with publication held back. It is still a CPU probe, not a
touch-to-panel or refresh-lock measurement.


## Single CRDT owner and transient ink

The frame user confirmed ephemeral trails now track quickly, but reported 2–3 s
when following durable edits from another device. A frozen 14,626-byte authority
snapshot was copied into temporary frame RAM for ten disposable update probes.
No display owner, clocks, calibration or live state was changed by those probes.

| Mean CPU stage | Before | Single owner + fast acknowledgement |
| --- | ---: | ---: |
| Wire receive/ack experiment | 90.813 ms | 62.415 ms |
| Full-document decode/merge between workers | 335.046 ms | removed |
| Replica clone for publication | 2.317 ms | 2.105 ms |

The transport now moves ordered protocol bytes only; application reconciliation
owns one mutable replica, while presentation and ink retain their independent
threads. Pure protocol acknowledgements retain sync bookkeeping without
recompressing history/rebuilding an unchanged view. New real-WebSocket tests
cover concurrent local/remote edits and reconnection with a fresh sync session.
The ready file records actual remote receive-to-present CPU/queue/display timing;
this excludes the sending device, authority and network transit before receipt.
The probe figures above are not end-to-end latency or FPS measurements.


## Deployed acceptance

Signed release `9f508c3f98b014c120922484dcecb8506f130f62616eccca543a98766f1dc6f7`
ran on the public Wasm host and the frame's single direct GLES2 renderer. Native
ready metadata showed server-confirmed state, no pending commands, controls
navigation preserved across hot reload, and a remote receive-to-present sample
of 169.131 ms. This one sample is not a latency percentile and excludes sender,
authority and transit before the frame receives the packet. The frame user
reported “Massive improvement!” after the single-owner update, having already
confirmed fast ephemeral traces before it.

Browser diagnostics confirmed 9,221,154 artifact payload bytes across 24 entries
with a 536,870,912-byte cap. The frame artifact cache was below its
2,147,483,648-byte cap; legacy client document/view/route files were absent, and
runtime notices were on tmpfs. The legacy checkpoint was merged and acknowledged
by the authority before retirement. Weston/browser remained stopped. Existing
Monitoring authentication and private-view isolation remain part of deployment
conformance checks.
