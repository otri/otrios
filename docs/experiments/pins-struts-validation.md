<!-- SPDX-License-Identifier: Apache-2.0 -->
# Pins and struts validation — 2026-10-19

Implements the parent-local layout-v2 rules in RFC 0005 for the opt-in Canvas app
prototype. The live dashboard's layout-v1 fixtures remain unchanged.

## Automated checks

- `bash scripts/check.sh`: passes. This includes the default layout and all six
  checked-in system UI assets matching their binary authoring fixtures exactly,
  legacy authority process tests and release/cache checks.
- `bash scripts/check-canvas-prototype.sh`: passes the runtime, lint and real
  server/client process checks. Following the advanced authoring-mask addition,
  the final targeted Canvas suite passes all nine integration tests.
- Core Canvas unit suite: 76 pass, including four new resize-policy tests.
- Strict all-feature/all-target workspace Clippy and the browser Wasm build pass.

Runtime tests cover edge pins, center offsets, growth/shrink, proportional spring
shares, all 64 spring-mask combinations at design size, zero-weight springs,
invalid geometry/rules and infeasible results. Integration tests round-trip
arbitrary authored spring masks, retain component design references across
instance sizes, preserve old scene snapshots and IDs, resolve nested components,
keep the resolved geometry out of CRDT fields, and hit-test newly expanded bounds.
A dedicated compatibility test verifies identical legacy bytes and rejects resize
rules in layout v1 or on an instance-owned template root.

## Browser proof

Layout Lab opens two instances of `apps/layout-lab/components.rs` at different
width/height mappings. Both read the same width and radius fields. Dragging the
width slider grew their visible bodies and button widths while preserving fixed
left/bottom gaps and status-circle size.

Observed semantic-overlay widths changed from 29.0625% / 23.5938% of the logical
surface to 35.9219% / 28.3953%, with left=55%, height=6%, and their distinct top
positions unchanged. A real pointer tap in the expanded portion of the first
button (outside its old bounds) invoked Min width. Both instances shrank to their
minimums; overlay widths became 21.25% / 18.125%. The authority acknowledged these
edits. This verifies presentation and touch geometry use the same resolved tree.

The task output `otrios-pins-struts.jpg` shows the final wide state. Only local
prototype browser/host checks were performed for this change; physical photo-frame
GLES/touch/FPS acceptance has not been rerun. The common Rust resolver is used by
both host paths, but that is not a substitute for physical-device measurement.

## Scope

Resolution is parent-local and has no sibling constraint graph. The logical root
remains 1280×800; no viewport-breakpoint or intrinsic-text sizing policy was added.
Rules live in binary UI data; resizing does not publish geometry into durable
state. Source-level reuse compiles into primitive trees. External component-file
admission and template deduplication remain separate work.
