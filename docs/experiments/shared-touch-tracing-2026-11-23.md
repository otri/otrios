<!-- SPDX-License-Identifier: Apache-2.0 -->
# Shared Touch Tracing — 2026-11-23

Touch Tracing now uses square world-space grid cells, synchronized strokes in its
existing Space-owned app Merkle document, and an independent uniform camera per
device. The initial reference is 1280×800. Fit Drawing fits retained strokes or
resets an empty canvas. Clear is shared. Retention is bounded to 64 segments of
192 points; a long gesture continues in a new segment with a shared endpoint.

Mouse left drag draws; right/middle or Space drag pans. One touch draws; two
contacts pan/pinch without leaving a stray stroke. Browser pinch uses Ctrl-wheel
and Safari gesture events. The signed-in profile's Developer Mode reveals
**Scroll: Pan / Scroll: Zoom**, persisted with the user's preferences. Pan is the
default; Zoom anchors at the mouse position. Pinch works in either setting.

The runtime is generic (`InkOptions.drawing`), with no app-name dispatch. It uses
subtree 5 and existing app journals, checkpoints, leases and writer receipts.
Public's existing public-edit audience and app collection identity were retained;
an explicit same-audience policy refresh enabled the app's new Edit capability.
State protocol is v44; the opening release-control envelope remains v1. A signed
runtime-only v44 release was qualified before activating the shared app bundle,
providing a compatible development rollback.

## Focused checks

Passed with Rust 1.90.0 and the locked dependency graph:

- `cargo test --locked -p otrios-core --features canvas shared_drawing_merges --lib`:
  independent writers, durable document recovery, Clear fencing late writes and
  denial of writes from a read-only lease.
- `cargo test --locked -p otrios-core --features canvas drawing_camera_preserves --lib`:
  zoom pivot, square projection, pinch without ink, and extreme zoom/offscreen
  points within admitted scene budgets.
- `cargo test --locked -p otrios-core --features canvas --test canvas shared_touch_tracing_syncs`:
  live client/authority wire flow across aspect ratios, camera-only state,
  shared Clear, phone viewport density, and remote cached-scene invalidation.
- Existing self-profile persistence/revision-conflict test with the new scroll
  preference enabled; `node --test scripts/test-dashboard-session.mjs` (15 tests).
- `cc -Wall -Wextra -Werror crates/otrios-linux-display/tests/touch.c` on OrbStack:
  legacy input, independent type-B contact IDs, one-event output buffers,
  stream loss, held-contact suppression and cleanup.
- `cargo test --locked -p otrios-dashboard --features direct,canvas --bin dashboard-direct native_input::tests`
  on macOS: 4 passed. The initial OrbStack attempt lacked EGL headers; the safe
  worker tests ran on macOS and the actual native adapter built on the ARM builder.
- `bash scripts/build-dashboard-web.sh`; ARM release build of dashboard-direct
  and dashboard-monitoring with `--features direct,server,canvas`; OrbStack
  canvas-server/canvas-bundle build with `--features canvas,server,host-sync`.
  JavaScript syntax and `git diff --check` passed. No broad repository suite ran.

## Live acceptance

The desktop browser (1402×877), phone-sized browser (400×800), and Frame (1280×800)
joined the same Public drawing. Browser dots and injected native strokes converged
on all three clients, including 45 points across four segments and a later
32-point native segment. Drawing survived browser reload, signed native runtime
replacement and an authority restart. A panned desktop camera and a pinched Frame
camera survived authority reconnect without synchronizing their transforms.

The first visual check exposed a shell minimum that expanded the phone view to
1280×2560. The final viewport uses 400×800 logical units, with 60-CSS-pixel toolbar
controls. A captured WebGL frame confirmed readable controls and square cells
across the phone viewport. The preview screenshot service stopped producing
frames; a temporary draw-call wrapper captured the completed canvas before
compositing, then restored the original function.

Frame's evdev controller advertises ten slots, but its old reader and safe bridge
collapsed contacts into one pointer. Both now preserve contact IDs. On the live
native host, two calibrated injected contacts changed zoom from 1 to about 1.51
and changed the camera center while keeping the 45-point/four-segment drawing
unchanged. The physical calibration and controller configuration hashes are
unchanged. Physical iPhone/Safari gestures and laptop trackpad feel still need
user trial; browser wheel/pinch and right-drag routing used dispatched events.
The profile switch's persistence was tested in core; an authenticated browser
profile switch was not manually exercised during this run.

A repeated remote Clear exposed missing native redraw invalidation for an app
with no scalar selectors. Drawing subtree hash changes now invalidate cached
presentation and report a changed frame to the native owner; receipt-only
changes do not force a drawing repaint.

Final signed release:
`e04b491a5771beec605c1ffaff2e2f358a5eca9efdc59ce57f43f87ad7ae2a9f`.
Native SHA-256:
`a763ef0a9f2426eea24a39ecb030eba3ad31491319d4de039d68635aca8fc46b`.
Wasm SHA-256:
`bbca89a66656f22c28a7632292b478bbe32547ea1bbcfb5f1e6197735551f234`.
The complete signed closure was verified before activation; app contracts,
identities and calibration were retained. Frame and both browser clients reported
the final release. Monitoring's local timer and publishing service remained active.

On the final native build, a browser mark appeared on Frame (one point/segment)
and a browser Clear removed it without another native touch or reconnect. Two
injected native contacts then changed the camera from center [640,400], zoom 1,
to approximately [627.27,372.46], zoom 1.51043, while retaining zero ink points.
A subsequent native draw produced 32 points in one segment on all three clients.
The first-tap fixture also covers unchanged per-slot coordinates at open and after
stream-loss recovery; the reader queries current MT positions instead of guessing.


The final 32-point drawing and all three distinct camera transforms were unchanged
after restarting the authority. Final shared Clear reached Frame without touch,
and Fit Drawing restored its empty reference camera [640,400], zoom 1. Frame was
left in Touch Tracing with zero retained points, confirmed state and a live
stream. Its installed executable hash matches the signed native asset above.
Physical gesture feel remains for user acceptance; the compatible v44 rollback
was retained and accepted-work builder pruning was deferred.
