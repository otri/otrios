# RFC 0019: Retained fixed coordinates and SIMD drawing batches

Status: proposed plan, native first. The separately deployable Layout Lab text
scroll demo is a workload and inspection tool; it does not enable this migration.
Requested 2026-10-27. The [qualified opt-in baseline](0018-fixed-point-layout-simd.md)
remains the scalar/pixel control.

## Measured problem and priorities

The admitted NEON translation kernel costs 18.11 ns/quad versus 26.27 ns float32,
but complete unchanged CPU frames cost 4.158 ms fixed/NEON versus 3.798 ms float32.
Eight-item checked flow is also slower. These totals do not isolate conversion
cost from tree traversal, allocations, wide division or vertex packing.

The current fixed path admits scene floats into fixed hierarchy placements,
converts placements/clips to float viewport values, then admits those floats
back to fixed for culling. It also traverses a cloned float-normalized tree and
builds maps each compose. Glyph packing vectorizes two four-lane additions before
scalar conversion and emission of 256 bytes per glyph. Removing this repeated
work has priority over expanding a fast arithmetic microbenchmark.

Scrolling must continue to retain geometry and change viewport uniforms/ranges.
Do not manufacture per-glyph translation work to justify SIMD. Seven repeated
passes approximately amortize the translation microbenchmark's admission versus
float32, but retained scrolling needs none of those full-geometry passes.

## Internal representation

- Admit changed source values once into a retained, indexed scene cache. Keep
  node identity and immutable revisions; invalidate only dependencies affected
  by edits, resizing, font/scale changes or ancestor transforms.
- Preserve wide checked origins and scales for hierarchy composition and small
  offsets through large cancellation. Retain the current Q32.32 semantics and
  fallback for inputs outside that representation.
- Rebase bounded drawing batches to a wide origin plus local i32 coordinates
  with 16 fractional bits. Four local coordinates fit a 128-bit SIMD batch.
  Chunk long strips when necessary; never narrow public coordinate admission.
- Prove complete batch ranges at admission. Hot loops may then use ordinary
  vector additions/comparisons with the proved no-overflow invariant. Keep
  checked wide operations for setup, scales, ratios and exceptional cases.
- Keep canonical font size and style identity independent of rounded positions.
  Cache style metrics and accumulated advances; do not round every character's
  advance separately.

All implementation remains in LGPL runtime paths. Public float contracts may
remain boundary inputs; this internal migration needs no state wire change.
Any later wire change must bump its declared protocol while retaining release
control's opening v1 compatibility envelope.

## Stages, in order

| Stage | Change | Expected SIMD work | Evidence needed |
| --- | --- | --- | --- |
| 0 | Attribute CPU costs separately: admission, hierarchy, wide math, culling, conversion, allocation/packing and host copies | None required | Same frame workload; p50/p95, bytes copied/allocated, rebuild/coast distinction |
| 1 | Retain fixed placements and GPU-facing rounded counterparts; remove float/fixed round trips and duplicate normalization | Integer offsets and clip comparisons | Unchanged frames avoid admission; exact invalidation and held-snapshot tests |
| 2 | Compile dirty transforms into contiguous arrays grouped by compatible parent/scale | Four i32 x/y/bounds lanes; batched min/max and visibility masks | Actual native instructions and reduced full CPU time |
| 3 | Batch glyph packing and final conversion; write directly into reusable presentation staging | Integer packing plus vector fixed-to-float conversion | Assembly proves vector conversion; preserve rounding and pixel gates |
| 4 | Compare native GLES2 vertex formats | GL_FLOAT control, GL_FIXED positions, rebased GL_SHORT positions | Cold/warm upload, submission, GPU/present timings and native pixels |
| 5 | Compile shared batches for Wasm SIMD128 | Same integer semantics, host vector lowering | Wasm SIMD opcodes plus real browser CPU/pixel tests; scalar fallback |
| 6 | Expand fixed layout and geometry only where profiles justify it | Repeated admitted sibling/layout operations | Full pipeline improvement, not isolated kernel throughput |

Each stage is independently reviewable and switchable. Keep scalar fixed and
float32 controls available throughout. A stage that loses full-frame performance
stays experimental even if it improves a microbenchmark.

## Culling and geometry

Keep binary search for ordered glyph strips and reject whole offscreen runs
before searching their horizontal ranges. SIMD is useful for broad-phase tests
of independent dirty bounds, not as a replacement with a full linear scan.
Do not submit culled geometry, rasterize offscreen glyphs for each fractional
phase, or rebuild immutable vertex buffers during coasting.

Start fixed geometry with rectangles, clip bounds and linear transforms.
Curve evaluation, tessellation, line intersections, division-heavy constraints
and nonlinear functions need separate range/error analysis. Cache static shapes
and use wide checked scalar setup where appropriate. Shader curve coverage and
gamma blending are separate from CPU coordinate representation; fixed CPU math
does not remove Mali fragment precision limits.

## GLES2 format experiment

Native GLES2 `glVertexAttribPointer` accepts signed 16.16 `GL_FIXED`. The API
converts it to a floating-point shader input. This can eliminate CPU conversion
of position attributes without adding an explicit shader conversion. It does
not reduce the 32-bit position bandwidth or guarantee hardware conversion on
this driver. Verify the actual Mali path and timing.

The vertex shader still receives floating-point values. `GL_FIXED` does not
preserve every bit of a large 16.16 value through shader arithmetic. Subtract
the wide viewport/batch origin before submission, keep shader positions local,
and preserve the fixed CPU representation for layout and culling. Measure both
raw-to-shader conversion and complete frames; do not describe this as an
integer shader pipeline.

Rebased `GL_SHORT` coordinates are another candidate. Fold their power-of-two
scale into the projection matrix where possible, preserve their full range and
prove clipping/sample precision. Do not introduce extra draw calls or shader
work without measuring them: the earlier native vertex-correction experiment
fell to 28.32 fps. Text color is already packed to RGBA8 in the native cached
path; preserve that saving. Reduce repeated metadata/copies before adding a new
GPU representation.

WebGL 1 excludes GL_FIXED. Short attributes are the portable GPU-format candidate;
CPU-only SIMD conversion is the lower-risk shared experiment. No claim is made
that changing vertex type will improve this specific GPU.

## Native and web implementations

The first target is the actual Cortex-A7/Mali-400. Prefer safe Rust arrays and
compiler vectorization, verify `vadd.i32`, comparisons/packing and vector `VCVT`
in generated code. NEON supports four-lane fixed/integer-to-float conversion;
our current per-glyph emission does not guarantee that LLVM selects it. The
wider checked layout solver must not silently become a wrapping vector solver.
A need for unsafe intrinsics requires a separately reviewed boundary proposal.

WebAssembly exposes portable 128-bit SIMD. Compile the same batch operations
with `-C target-feature=+simd128` and verify the emitted Wasm operations; source
array syntax is not proof. Keep batches inside Wasm rather than crossing the
JS/Wasm boundary per glyph. Retain transferable buffers and avoid repeated
packing in JS. Begin with standard deterministic integer SIMD, without relaxed
SIMD or different rounding rules.

Ship scalar and SIMD Wasm variants and feature-detect before selecting a module.
An unsupported engine can reject a SIMD module even if it would never execute
its SIMD function. Use the same admitted data and output tests across both
variants, and test actual desktop and mobile engines when available. Engine
lowering and CPU memory behavior determine the gain; it is not automatically
the same as NEON's gain.

## Acceptance

- Preserve signed source/fixture provenance and the original float32 captures.
- Compare scalar fixed, NEON and Wasm SIMD output over normal, small/large,
  negative-bearing, fractional, nested-scale, clipped and overflow cases.
- Maintain the unchanged maximum channel error gate of 8; save foreground/edge
  statistics and reference/output/difference images. Native rotations 0/90 and
  browser DPR 1/2/3.2 remain required. No background-diluted error averages.
- Record complete CPU p50/p95, cold rebuild and coasting separately. A suggested
  default-enablement gate is at least 10% median full CPU improvement with no
  material p95 regression over repeated, isolated native trials; agree the gate
  before selecting a winner. Microkernel gains alone are insufficient.
- Maintain at least 30 kernel-presented fps for the dense native fixture, with
  unchanged density, coverage/color path, scanout format and cache memory bound.
  The demo reports actual native presentations or browser drawn frames, not
  state-worker tick rate; those counters must remain distinguishable.
- Run the required repository, native GPU and real browser checks for each
  affected stage. Preserve clipping, ordering, input recovery and hot upgrades.

## Primary references

- [Arm NEON Programmer's Guide, vector VCVT](https://documentation-service.arm.com/static/63299276e68c6809a6b41308).
- [GLES2 specification, vertex arrays/types](https://registry.khronos.org/OpenGL/specs/es/2.0/es_full_spec_2.0.pdf).
- [WebGL 1 specification, fixed point restrictions](https://registry.khronos.org/webgl/specs/latest/1.0/).
- [Rust Wasm SIMD support and module compatibility](https://doc.rust-lang.org/core/arch/wasm32/index.html).
- [WebAssembly portable SIMD design](https://github.com/WebAssembly/spec/blob/main/proposals/simd/SIMD.md).
