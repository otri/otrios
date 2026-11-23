<!-- SPDX-License-Identifier: Apache-2.0 -->
# Retained drawing and viewport presence — 2026-11-23

Touch Tracing now keeps completed, world-space stroke chunks in GPU VBOs, culls
invisible chunks, changes camera uniforms during pan/zoom and rebuilds only a
growing tail. A decoded-state cache keyed by the drawing subtree hash removes
repeated FlexBuffers decoding from input/presentation. Snapshot clones remain
isolated despite sharing that cache. Drawing backgrounds precede the world layer;
controls and participant outlines follow it. The grid uses a retained thin-line
template positioned by the shader, avoiding a full-screen fragment pass.

Ephemeral viewport rectangles use the existing scoped auxiliary connection.
Labels use the OTriOS display profile and a session suffix; Brass Relay calling
cards remain room-local. Stationary peers advertise every 1.5 seconds, and the
relay expires presence after five seconds. Other rectangles appear during local
camera movement, fade, and pass input through. Camera/presence do not write app
history. State/auxiliary negotiation is v45; opening release-control v1 is intact.

## Measured Frame results

Baseline runtime: `6869c4482869c647a56f2a543ea5d9cfc3cea776ba8a811079e34412c1c2814d`.
Measured native runtime: `8a412c64dc0a0a3d3efb4c13523d4ac078cc3ca5d714e814538afc4d26bf1500`.
Final release: `5826d7ed8cf0fd23bdd1d3cf2d56856b1023d2f9e7fb0bb3ef4695fc9bc65631`;
its native artifact is identical to the measured one. The final browser-only
change ignores transient zero-sized viewport reports during resize.

Both slow trials replayed the same calibrated two-contact translation sequence:
30 moves at nominal 10 Hz, then lift, over about 3.12 seconds. Both traces process
62 input events, have zero dropped diagnostic rows, and keep developer touch-zone
feedback enabled. No core profiling or GPU finish was enabled. Selection uses
Touch Tracing epoch 4, unique local-input-associated present serials and the
sampled gesture interval, following `bench_telemetry.rs`'s documented recipe.

| Metric | Previous | Retained drawing |
| --- | ---: | ---: |
| Median sample-to-present-return | 141.49 ms | 65.69 ms |
| p95 sample-to-present-return | 203.46 ms | 93.27 ms |
| Median pointer processing | 31.64 ms | 0.65 ms |
| Median composition | 14.75 ms | 2.27 ms |
| Input-associated updates | 25 | 31 |
| Host vertex uploads during selected frames | 7,158,016 B | 1,271,680 B |
| Maximum successive return gap | 176.60 ms | 264.77 ms |

The newer trial includes another live participant's temporary outline; the
previous version had no viewport feature. Baseline data had 1,928 points in 26
segments; final data has 1,929 points in 27 segments. A synthetic navigation touch
during a restart likely added the single dot. No existing stroke was removed,
Clear was never used, and all later measured pans kept the drawing count fixed.
This is a short live comparison, not a controlled statistical benchmark or a
claim of uniform frame pacing: the larger isolated return gap remains visible.

The final camera starts at [640,400], zoom 1, and ends at approximately
[667.395,393.199], zoom 1.00662, matching the prior replay's end transform. Its
49 retained chunks occupy 274,176 bytes. The rebuild count stayed 49 throughout
pan, while visible chunks fell from 49 to 48. Upload totals include UI/presence
feedback, not just world geometry.

A faster follow-up sends 240 paired-contact moves at nominal 60 Hz over 4.27
seconds. It completed in the same native process without the input-queue overflow
seen on the old renderer. The device reader reported 353 changed samples and
112 input-associated presentations (26.23/sec), median latency 75.98 ms, p95
95.57 ms and maximum 111.37 ms. Median return interval was 35.31 ms, p95 53.08 ms,
maximum 176.50 ms. A second browser participant joined during this stress trial.
Completed chunks still had zero rebuilds, and points/segments stayed 1,929/27.
These are observed local update rates, not a panel refresh-rate measurement.

Raw bounded traces, SHA-256 digests of their uncompressed bytes, result summaries
and trial provenance are retained in
[the measurement folder](retained-drawing-2026-11-23/results.json).
The existing process clock is used for input latency; DRM intervals are analyzed
only within their separate kernel clock domain.

## Validation and live acceptance

- `cargo test --locked -p otrios-core --features canvas --test canvas shared_touch_tracing_syncs`:
  passed after the final core change. It covers cross-aspect replication, local
  cameras, Clear retirement, isolated cloned snapshots and drawing painter order.
- Focused core `completed_chunks_survive_camera_changes_and_only_the_tail_rebuilds`
  and `viewport_presence_is_bounded_ephemeral_and_never_takes_input` passed.
  The focused authority relay identity/compaction/expiry regression passed.
- `node --test scripts/test-dashboard-session.mjs`: 15 passed for v45.
  Two focused signed-release activation/protocol tests passed. Changed JS syntax
  and `git diff --check` passed. No broad repository preflight ran.
- Locked release Web/Wasm and ARM dashboard-direct/dashboard-monitoring builds
  passed; the OrbStack authority's server/canvas build passed. An initial test
  command omitted the required canvas feature and failed to compile; rerunning
  with `--features canvas` passed.
- The rebuilt `world_pixel_probe` ran on Frame's actual Mali400 GLES2 path:
  rest 800 stroke pixels, pan 800, zoom 3,050, with successful GL calls. Frame
  reports zero vertex-texture units. `/proc/cpuinfo` reports Cortex-A7, NEON and
  VFPv4. Float VBOs are the common baseline; neither texture-indexed vertices nor
  a new NEON/fixed-point path is claimed.
- Live desktop and 400×800 browser peers displayed the shared drawing. Panning
  offscreen culled visible chunks 49→0→49 without a rebuild. Each camera stayed
  local. Labels included the OTriOS profile/Frame identity and distinct Visitor
  session suffixes, then disappeared after the fade in a visible tab. A WebGL
  loss/restore replay returned to graphics=ready with the same drawing and no GL
  error. GPU readback confirmed grid and strokes remained visible below the HUD.
  Preview screenshots were unavailable; bounded readback was used for inspection.
- A transient zero-area browser resize exposed an existing fatal viewport fit;
  the final JS guard now defers it. A zero-width resize replay and subsequent
  400×800 / 800×400 resizes stayed live. This is preview-browser testing, not a
  new physical iPhone gesture qualification.
- Complete signed closures were verified before activation. One activation check
  mistakenly probed an unsupported URL and restored the compatible code pointer;
  retrying with `/release` succeeded. Protocol changes restart the authority;
  release discovery remains independent of old state negotiation.
- Temporary native diagnostic drop-ins were removed and the direct service was
  restarted normally. Monitoring remains pre-authorized, its collection timer
  and publisher run, and revision 212 was acknowledged after the server update.
  Existing calibration, identity and user preferences were retained.

## Accepted-work closeout

The user accepted the performance improvements on 2026-11-23. The documented
builder maintenance dry run and apply both completed with no blocked paths.
The existing builder was stopped throughout and remains stopped, matching its
prior configuration. Cleanup removed 11,994,002,265 logical bytes of disposable
Cargo/Wasm caches (8,845,303,808 allocated bytes inventoried) and 12 inactive
local signed releases (463,880,839 bytes). Filesystem space recovery
may differ because of shared blocks. The active signed release, signing keys,
configuration, app state, toolchains and top-level checker executables remain.
No new build, runtime activation or app-data changes were needed for closeout.
