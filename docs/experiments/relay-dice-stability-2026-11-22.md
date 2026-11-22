<!-- SPDX-License-Identifier: Apache-2.0 -->
# Relay dice visibility and settling stability — 2026-11-22

`90170fd` fixes intermittent missing faces and collision/settling flicker.
Two regressions reproduced failures against the previous implementation:
perspective silhouette coverage failed (seed 1, 100 ms), and seed 19 changed
orientation by 2.464 radians (141 degrees) in one 120 Hz sample while settling.

The face test had used an orthographic positive-normal-Z threshold despite
perspective projection. It now tests the actual face-to-camera vector. Entire
convex dice are painted in camera-distance order rather than interleaving faces
from different dice by their average depth. The disjoint equal-size enclosing
collision spheres provide a consistent near/far order through projected overlaps.

Settling previously interpolated positions after collision solving and kept
blending from a spinning quaternion. The latter could change the shortest-arc
quaternion sign halfway through the animation. The source orientation is now
locked once at the transition. Positions remain in one fixed-step simulation,
with damped attraction, contact impulses and separation solved together. Spatial
slot ordering avoids pulling dice through each other. Presentation interpolates
between adjacent 120 Hz states; subpixel convergence sleeps residual drift.
An entering die now remembers crossing the left boundary: collision reversal
while still off-screen cannot clamp/teleport it directly into the viewport.
Settling velocity is bounded, including dense ten-die throws.

Recorded results, seeded replay, clockwise entry and the 4.3-second playback
boundary remain. No dependencies, event schema, state protocol, graphics ABI or
app contracts changed. Physical phone/Frame playback is not claimed here.

## Focused validation

`cargo test --locked -p otrios-core --features canvas --lib canvas::chat::effects::tests`
passes six tests: perspective silhouette closure; per-frame position/orientation
continuity over multiple seeds for two- and ten-die throws; contact impulses and
separation; recorded resting faces/floor contact; clockwise entry; and finite,
clipped deterministic output across phone/Frame viewport sizes. Both new
regressions failed before the fix. A rendered triangle contact sheet around the
former seed-19 rotation jump was visually inspected (before/after at 2375, 2383
and 2392 ms). The full repository preflight was not run.

## Signed deployment and focused live checks

Mac tooling, release browser Wasm and ARMv7 release client builds completed.
Active release:
`7a75f061538619ffc1d76ba9f4d547fa95da697575756adab1a55f348de3d6ef`.
Complete signatures/digests and installed bundle closures verified on Mac and
Linux; all 85 authority files, app contracts and catalog bytes are unchanged.
The import guard passed before activation. Both release pointers match, with
compatible baseline
`af997501a8bcc7541fa978d39bab33eaec5995c32e4ee481173d1a43285a7cec`
retained in the VM's `code-rollback/relay-dice-stability-baseline.json`.

Focused live HTTP checks confirmed healthy state availability, current-release
pinning from `/brass-relay`, and exact signed app, worker, bindings and Wasm bytes.
Browser UI and physical iPhone/Frame playback were not run for this fix. No
public test messages were posted. Acceptance, verification and inspected
before/after captures are retained in
`~/Library/Application Support/OTriOSOrbStack/checks/relay-dice-stability-*`.
The authority, tunnel and Monitoring timer remain active; the builder is stopped.

## Narrower dice lens follow-up

`d11ab2e` increases the minimum dice camera distance from 12 to 48 world units,
with matched focal length preserving size on the centre plane. Distance also
scales with the larger viewport dimension and die scale so unusually large
viewports do not restore the wide-angle edge distortion. Projection, face
visibility, shadows and whole-die paint ordering share this distance.

The existing `perspective_dice_faces_cover_their_silhouette` test passed (one
focused case). Matching posed fixtures at the left, centre and right of a
1280 × 800 viewport were rendered and visually inspected. Mac tooling/browser
release Wasm and ARMv7 client builds passed. The physics and event contracts
were not modified. No broad preflight or physical iPhone/Frame test was run.

Signed release
`177c579c06f6f97a8596ec0eae6d0c65ab3d0d2be1d86bc13f932fa4cd55b7d6`
is active on Mac and VM. Complete signatures/digests, bundle closures and import
guard passed; all 85 authority files, app contracts and catalog bytes are
unchanged. Live HTTP checks confirmed healthy state, current navigation pinning
and signed app/worker/bindings/Wasm bytes. Compatible prior release is retained
in `code-rollback/relay-dice-camera-baseline.json`. Evidence is saved under
`~/Library/Application Support/OTriOSOrbStack/checks/relay-dice-camera-*`.
