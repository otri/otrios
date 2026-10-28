<!-- SPDX-License-Identifier: Apache-2.0 -->
# Early dashboard and app cutovers

This summary preserves selected outcomes from the 2026-10-18–24 dashboard and
Canvas cutovers. The tests and physical confirmations apply to the releases then
examined; they do not establish current hardware performance or acceptance.
Full historical reports and artifacts remain accessible at the pinned Git revision
linked below. Current operation is described in [frame hosting](../FRAME_HOSTING.md)
and the [app guide](../../apps/README.md).

## Initial dashboard and physical input

The initial native GLES2 frame and browser shared state, recovered after authority
restart, and advanced a signed code release while retaining the snapshot. Real
process tests covered restart, origins, update delivery and signing checks. The
user confirmed frame touch updating iPad and iPhone, and on 2026-10-19 confirmed
Input Lab entry in both fields (`abc` and `xyz`), Backspace and Back on the frame.
These confirmations cover those specific builds and interactions, not panel color,
typing latency or the later migrated app. Framebuffer readback is not a physical
panel or touch test. The historical primitive-tree refactor and keyboard checks
had their own separate acceptance limits.

## Canvas prototype and migration

The initial app package prototype exercised app isolation, process boundaries,
Wasm/browser builds and signing checks; it was initially separate from the deployed
dashboard. The later signed migration mounted app-owned UI and state on the browser
and frame, with Monitoring private and durable acknowledgments. Automated tests and
frame readiness did not establish attended physical touch acceptance for the
migrated build.

On the RK3126 frame, a 15-second isolated 64-tetrahedron, 30 Hz RAM CRDT workload
after two seconds of warmup measured **53.80 kernel page flips/s**, 17.654 ms median
and 17.667 ms p95 frame interval, and 431 load updates. Composition/submission
was 2.720 ms median and 4.309 ms p95. A framebuffer capture caused a 282 ms
stall; the approximately 56.65 Hz panel and this run do not establish a 60 fps
lock. A separate five-second animation result included readback and is not a
steady-state comparison. Raw timing data is available in the pinned historical
[frame results](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/canvas-app-frame-results.json).

## Layout, navigation and package updates

The 2026-10-23 modular layout work exercised shared components in portrait and
landscape, with browser tests, ARM builds and frame deployment. The subsequent
reorder test used two Chromium/WebGL clients and tested shared updates, focus,
rotation during capture and navigation. One measured move sent 140 client binary
bytes across write and acknowledgment; this was a correctness sample, not a
throughput or physical touch benchmark. Browser touch was automated.

The 2026-10-24 UI and behavior update qualifications tested separately admitted
app artifacts, signed updates and state/session continuity with browser, native
and process checks. The navigation qualification tested direct child routes,
Back/Forward, private admission and a renamed UI route without changing the
child's state or session. These automated checks did not establish attended
physical touch, panel color or device performance. Later changes and current
limits are recorded in the [RFC index](../rfcs/README.md).

## Historical source reports

- [dashboard-validation.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/dashboard-validation.md)
- [canvas-apps-validation.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/canvas-apps-validation.md)
- [canvas-migration-validation.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/canvas-migration-validation.md)
- [modern-app-cleanup-validation.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/modern-app-cleanup-validation.md)
- [modular-screen-layout.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/modular-screen-layout.md)
- [home-card-reordering.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/home-card-reordering.md)
- [semantic-navigation.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/semantic-navigation.md)
- [app-ui-updates.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/app-ui-updates.md)
- [app-behavior-updates.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/app-behavior-updates.md)
- [app-style-restoration.md](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/app-style-restoration.md)
