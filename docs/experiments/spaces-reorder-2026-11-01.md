<!-- SPDX-License-Identifier: Apache-2.0 -->
# Space title reordering — 2026-11-01

The reordering follow-up is deployed on Frame and <https://otrios.otri.ca>.
Runtime source is `1d058fdc33b1255cf293f4facecc6a25027a1933`, signed release
`bc1aa7847a6a4f4f42b7cc4482f7dc2adef9a20c2c199a278ec1a93aeb7a146f`,
and exact deployed Wasm SHA-256
`c5c8a2f36bc3cf940aad76ae57a298ef48e1b30fc65689e440aa5abcfdc567ac`.
Later probe/documentation changes do not change that deployed runtime. State
protocol v17 and registry OTSP6 remain unchanged; no data migration occurred.

The edit preview previously rebuilt the map on every pointer move. Frame's
native input router correctly cancels contacts when their mount changes, so this
broke title dragging. Preview movement now changes only local presentation.
Holding a dragged title or app near either map edge scrolls toward an offscreen
destination; moving into the middle stops it. Cancel and same-item drops do not
write metadata. Metadata/navigation rebuilds invalidate stale gestures.

Frame also lacked Admin membership in Household and Brass Relay. Aaron explicitly
authorized granting its existing identity Admin in those two Spaces. The existing
offline provisioner made those grants; it preserved active collection bindings,
app-state bytes during provisioning, auth configuration, audiences, layouts and
touch calibration. Private pre-change metadata and provisioning logs stayed on
Frame. No data snapshot was restored. The first plan's directory was not readable
by the service account; moving the private plan into a service-readable directory
resolved that operator-tool issue without changing the provisioner.

## Validation and acceptance

- `bash scripts/check.sh` passed for the runtime change, including strict Clippy,
  210 core library tests, 32 Spaces UI tests and 22 separate authority/client
  process cases. The canonical signed build's selected tests and 80 Node cases
  passed. No new dependencies were added.
- Both map browser suites passed against the exact deployed Wasm with clean
  source. The four-Space, fourteen-app fixture exercises a 390-pixel touch
  viewport and 1280-pixel mouse viewport: the destination begins above the
  visible area, a stationary edge hold reveals it, the mount epoch stays fixed,
  and the authoritative order updates. These are Chromium touch emulation tests,
  not an observed iPhone Safari run. Existing fade, fixed-chrome, sign-in,
  app-reorder, responsive and mounted-peer checks remain green.
- Frame readiness and 171 public HTTPS conformance checks passed. Mac and Frame
  pointers match the independently verified signed closure.
- Actual calibrated evdev input placed Brass Relay before Household and retained
  that requested order. The map epoch stayed fixed throughout the drag, and
  the authority recorded its management change. An initial probe used estimated
  padding coordinates and did not complete the reorder. The reusable read-only
  `space_map_probe` now derives exact title coordinates from the shared runtime
  on Frame, keeping authenticated metadata in a private pipe. A subsequent
  same-item drag passed with stable capture/epoch and unchanged management
  generation. Neither probe wrote app content or exported pixels. Readiness
  input-busy samples are asynchronous; the title result records this rather than
  treating every sampled old frame as a fresh input acknowledgement.
- The later probe example passed host and ARM compilation and strict Clippy;
  Python probes passed local and embedded-remote syntax checks and were exercised
  on Frame. `--skip-navigation` capture acceptance does not count navigation,
  scroll rounds or breadcrumb tests that it did not execute.

[Sanitized evidence](data/spaces-reorder-2026-11-01/README.md) includes only fixture
screenshots and aggregate results. No human finger-touch observation or measured
frame rate is claimed. The complete redesign remains an active goal: retained
data UX, cross-Space moves and portable transfers follow in milestone order.
