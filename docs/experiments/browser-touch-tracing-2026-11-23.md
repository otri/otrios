<!-- SPDX-License-Identifier: Apache-2.0 -->
# Browser Touch Tracing opening fix — 2026-11-23

Opening Touch Tracing triggered Web accessibility enumeration of every scene
control. `Client::control_value` assumed every non-Activate handler had at least
one numeric field. The local drawing pad deliberately has none, so indexing its
first field panicked and aborted Wasm before the app could be presented. The
matching keyboard-adjustment path made the same assumption.

Both paths now return no numeric value/no adjustment for fieldless gestures.
The existing Touch Tracing regression exercises all of its controls through both
APIs. It passed with:
`cargo test --locked -p otrios-core --features canvas --test canvas touch_tracing_is_fullscreen_local_persistent_and_clearable`.
The pinned `bash scripts/build-dashboard-web.sh` build passed.

Signed release:
`3cbb2ab53e2836a46c820d97b7fd3886e5aba9b3f9b71fbe8e9c2fcb08e3bfec`.
Only the Wasm file and its gzip sidecar changed. Native/collector binaries,
app bundles, catalog, contracts and protocol v43 are unchanged. The complete
signed closure was verified before activation. The existing native updater
restarts even for this Web-only executable change; the ephemeral Frame strokes
therefore reset, while its calibration and durable app data remain unchanged.

Live browser acceptance reproduced the prior index-out-of-bounds panic, then
confirmed recovery onto the new signed release. Home → Test Playground → Touch
Tracing opened successfully, exposing Drawing surface, Clear and Exit without a
numeric slider value. Pointer input produced 762 retained points in seven segments;
they were unchanged after six seconds and after an ArrowRight key on the focused
surface. Clear emptied the drawing and Exit returned to Test Playground with no
visible error. Browser automation's pointer drag timed out while still delivering
input; a direct canvas click completed successfully, and retained-ink statistics
were checked separately. No broad suite was run.
