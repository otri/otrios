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
native bounded-mailbox/persistence test; coverage tests for glyphs and fractional
clipping, pinned snapshot reuse/invalidation, and rejection of grafted history.
Physical touch tracking was accepted before this optimization; perceived update
speed after deployment still needs confirmation on the panel.
