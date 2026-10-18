<!-- SPDX-License-Identifier: Apache-2.0 -->
# App visual restoration

The app migration retained behavior but reduced Home, Component Playground and
Layout Lab to bare controls. Restore the earlier shell's composition in each
app's own UI source, without restoring the shell or global dashboard document.

- Home: three-column launcher, app descriptions, muted secondary type, lime entry
  cues and Monitoring's lock in the final card. No Back/Home controls on Home.
- Playground: three instrument panels, thumb slider and large percentage, a real
  clockwise rotary indicator with tick marks, gridded drawing pad and five-second
  trails. The former unexplained Output toggle is now an Accent switch: it changes
  instrument colors without disabling interaction or changing another app's state.
- Layout Lab: large ON/OFF toggle, five-minute signal chart with grid, and two
  reusable pinned components with shared width and radius controls. Time expiry
  stays on the host clock and never needs a stream of artificial state writes.

All styling is compiled into each admitted binary UI description. Shared authoring
supports colored text and state-selected primitive colors. Two numeric projections
map a normalized turn to an indicator's X/Y position, matching the dial's existing
clockwise touch mapping. These are local presentation calculations, not extra CRDT
fields. Schemas, state references, ephemeral ink and access policy are unchanged.
The compatible authority must be installed before admitting this catalog because
its selector descriptions contain the new color and turn projection variants.

Validation: the required workspace checks, lint and process tests pass, including
23 Canvas integration tests and 31 browser resource/GPU/lifecycle checks. The new
pointer test checks all four dial quadrants and verifies that muting the accent
leaves controls interactive. Existing tests exercise all seven app descriptions at
scalar extremes, reusable component pins, and ephemeral pad coordinates and durable
release. Gesture tests resolve the authored pad bounds rather than duplicating its
old screen placement. Actual browser screenshots were inspected for all three apps.

## Deployed acceptance

Source commit `3c02cfc`; signed release
`fc04290a350a787453a2d979cfdb410918b402acf46caa3cbcbec021900c42b7`.
The existing watcher built and published the browser and ARMv7 host pair. Public
Playground was inspected on the actual HTTPS host; all 73 deployed access checks
passed again, including private artifact denial and Monitoring session revocation.

Actual direct-GLES framebuffer captures of Home, Playground and Layout Lab were
inspected on the Rockchip frame, using the compressed font resource, retained glyph
atlas and RGB565 scanout. Each short capture ran alone with the normal service
stopped, and a trap restored that service. Static Home/Playground intentionally
stopped drawing after two startup frames; capture index 1 was used for those idle
screens rather than waiting for an unnecessary third frame. These are visual
acceptance checks, not new animation performance measurements.

The final readiness report confirmed this release, acknowledged `app:home/state`,
zero pending commands and calibrated touch. Exactly one native renderer remained,
with no Weston/browser. Physical finger interaction remains a manual acceptance
check; pointer/process tests do not prove physical-panel touch or color.
