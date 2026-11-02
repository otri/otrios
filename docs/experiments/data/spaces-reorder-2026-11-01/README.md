<!-- SPDX-License-Identifier: Apache-2.0 -->
# Space reordering evidence

- `populated-browser.json` and `edge-drag-390.png`: four synthetic Spaces/fourteen
  apps, offscreen destination reordering with touch and mouse, stationary edge
  scrolling and a stable mount epoch; exact deployed Wasm.
- `map-browser.json`: normal map/settings/app-order/responsive regression.
- `frame-ready.json`, `public-conformance.json`, `mac-promoted.json`: signed
  activation, native/services readiness, 171 public checks and Mac pointer alignment.
- `frame-admin.json`: explicitly authorized existing-device membership grants;
  no credentials, private identities, mappings or data inventories.
- `frame-title.json`: successful physical-reader reorder, stable epoch and
  authoritative completion. Input-busy sampling can observe stale presented frames.
- `frame-capture.json`: runtime-derived title coordinates and same-item drop;
  no metadata write. Navigation was deliberately skipped and is not counted.

All physical contacts are synthetic events through the actual calibrated reader.
Private coordinates, metadata and logs stay on Frame. See the
[qualification record](../../spaces-reorder-2026-11-01.md).
