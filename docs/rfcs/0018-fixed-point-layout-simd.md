# RFC 0018: Fixed point layout and SIMD stretch milestone

Status: experimental candidate, disabled by default; not accepted. Requested
2026-10-26, measured 2026-10-27. [Evaluation and unresolved pixel gate](../experiments/fixed-point-text-2026-10-27.md).

## Objective and baseline

Use checked fixed point coordinates for UI layout, glyph placement, clipping and
viewport offsets. Preserve deterministic additions/comparisons across small and
large coordinates. Evaluate SIMD for bulk packing, transforms and culling on the
actual Cortex-A7, with a portable scalar implementation as the reference.

The baseline is [RFC 0017's float32 retained renderer](0017-retained-text-viewports.md),
its committed [capture and measurements](../experiments/text-rendering-performance.md),
and the exact admitted Roboto resource. Keep sampling density, coverage shader,
color pipeline, scanout format, projection and workload fixed during comparisons.
Changing filtering and coordinate representation together would hide regressions.

## Representation to evaluate

The pilot uses i64 Q48.16 for resize coordinates, i64 Q32.32 for numeric
expressions and accumulated glyph advances, and admitted local i32 Q16 lanes
for glyph packing. Viewport offsets and flow/culling remain float32. A 16.16 i32 position alone
does not cover the current ±100,000 admission bound; widen checked intermediates
or use separate bounded types. Do not silently narrow that public limit.
Accumulate advances before rounding to the final presentation grid so rounding
each character cannot introduce proportional drift along a long line.

Admit existing float-valued contracts through explicit, finite, range-checked
conversion, with a documented rounding rule. Keep the state wire protocol stable
while changing internal arithmetic. Any future wire-format change needs a declared
protocol bump. Convert to GLES/WebGL attributes at the presentation boundary.

Use safe Rust and compiler vectorization first. The frame reports NEON and VFPv4
on Cortex-A7. Verify generated instructions and benchmark an explicitly targeted
build; do not claim SIMD from array syntax or throughput alone. Rust 1.90's
portable SIMD API is unstable. New architecture intrinsics requiring unsafe Rust
would need a separate reviewed boundary amendment, not a workspace-wide lint change.

GPU curve evaluation is a separate precision problem. Lima's fragment operations,
including emulated integers, use FP16. Preserve bounded local coordinates and
the FP24 texture-coordinate address path. CPU fixed point does not change these
hardware arithmetic limits or make nonlinear shader operations exact.

## Acceptance gates

- Record scalar float32, scalar fixed point and SIMD fixed point CPU timings with
  the same font, geometry, text and input sequence. Report cold generation,
  unchanged frames and fractional scrolling separately.
- Exercise small and large positions, negative side bearings, long advances,
  clipping on all four edges, nested scales, fractional phases, both native
  rotations, DPR changes, reflow and cursor recovery. Check overflow and rounding
  boundaries with deterministic scalar tests.
- Compare actual native and browser pixels against the retained float32 baseline,
  and fixed scalar against SIMD output. Save reference/output/difference images,
  maximum and mean channel errors, changed-pixel counts and edge-only statistics.
  Do not dilute the result with large unchanged backgrounds or loosen existing
  thresholds to obtain a passing result. Investigate and explain every new error.
- Run the required repository checks and the actual Mali tests. Maintain at least
  30 kernel-presented fps for the dense scrolling fixture across repeated runs.
  Sign-off requires both performance and pixel comparisons; scaffold checks alone
  do not qualify.

The prototype and comparison tools are implemented behind `fixed-ui-math`.
The isolated integer kernel is faster, checked layout is slower, and the
large-scale/DPR pixel gate fails. No default enablement or complete fixed point
UI migration has been accepted by this RFC.
