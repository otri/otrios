<!-- SPDX-License-Identifier: Apache-2.0 -->
# Safari direct-touch drawing follow-up — 2026-11-23

The user reported reversed panning, zoom-in jumps when adding a second finger,
and pinch-in leaving iOS 27 Safari for its tab overview. The deployed canvas
computed `touch-action: pinch-zoom`: the browser was explicitly allowed to own
the same pinch the app was interpreting. Gesture listeners also returned before
preventing the browser default whenever direct-touch pointers were active.

Shared drawing now uses `touch-action: none`, plus non-passive touchstart/move
and gesture listeners that cancel browser defaults on that surface. Direct
pointer contacts own the camera; Safari gesture events supply the trackpad
fallback only when no direct contacts are present. Wheel events cannot apply a
second camera transform during that gesture. Other app screens retain their
browser zoom policy. WebKit's [tab-overview interaction issue](https://bugs.webkit.org/show_bug.cgi?id=199560)
and Apple's [touch-action guidance](https://developer.apple.com/videos/play/wwdc2019/203/)
support using both the CSS declaration and default cancellation; the reported
physical iOS 27 browser-chrome behavior still needs user confirmation.

Pointer movement batches now merge contacts in sample-time order instead of
replaying one finger's entire batch before the other's. Pending movement drains
before contact down/up boundaries. Camera changes occur only on Move, so an Up
or Cancel with missing/stale coordinates cannot turn a lift into a large pinch.
Panning retains direct manipulation: content follows the fingers.

Validation:

- `node --test scripts/test-touch-input.mjs`: 4 passed, including interleaved
  contacts and bounded chronological batches.
- `cargo test --locked -p otrios-core --features canvas pinch_closes --lib`:
  passed. Tests contraction below initial zoom, expansion back, content following
  fingers right/down, and zero-coordinate lift without a transform or ink.
- Locked Web/Wasm and ARM dashboard-direct/dashboard-monitoring builds passed;
  JS syntax and `git diff --check` passed. No broad suite ran.
- Live 400×800 browser view computes touch-action none. Dispatched direct-touch
  contraction and pan reached the expected camera: starting center [832,592],
  zoom 0.5; ending center [1216,976], zoom 0.25. Lifting did not alter it. Browser
  viewport scale stayed 1 and the existing 775 points/seven segments remained.
  Dispatched touchstart and Safari-style gesture start/change/end were all
  default-prevented. Early intermediate metadata reads lagged presentation;
  the final settled transform matched the expected result. This is an event
  replay in the preview browser, not a physical iOS Safari qualification.

Signed release:
`6869c4482869c647a56f2a543ea5d9cfc3cea776ba8a811079e34412c1c2814d`.
The complete signed closure was verified before activation. App bundles,
contracts, collector, identities, protocol v44 and opening control v1 are
unchanged. The previous compatible code release remains available. Frame
reported this runtime/UI release with confirmed state and a live stream; it
resumed Monitoring. No drawing Clear or app-data replacement was performed.

## Physical iPhone acceptance

The user confirmed: “That works very well now on iPhone.” Other devices remain
for their next check. The builder maintenance review ran against the SPDStudio1
runtime; destructive pruning is deferred so compatible rollback remains available
through those checks. No additional builds or deployment were needed.
