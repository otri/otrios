# RFC 0019: Native rendering optimization and SIMD evaluation

Status: native evaluation complete; selected retention and coverage improvements
qualified and deployed. Fixed-point and alternate GPU position pilots retired.
Further SIMD work is deferred until a measured float batch justifies it.
Requested 2026-10-27.

[RFC 0020](0020-chunked-text-documents.md) proposes the next workload: long,
reflowed documents with bounded chunk generation, retained viewport scrolling
and a path to rich text. Its generation measurements will determine whether
float SIMD earns a place in the selected renderer.

## Selected outcome

Keep float CPU geometry and `GL_FLOAT` GPU positions. Retain Canvas layout/editor
bounds during panning, immutable glyph geometry, viewport offsets, binary range
culling, canonical font scale caching and native scissor reuse. Adaptive alpha
coverage storage uses one channel for ordinary glyphs with a bounded RGBA scratch
target; large entries retain the qualified RGBA path. Preserve the explicit RGBA
compatibility/reference control.

The [native qualification report](../experiments/native-retained-simd-2026-10-27.md)
records the original experiments and deployed acceptance. Dense native replay
measured 56.41–56.65 kernel-presented fps; full-screen momentum dragging measured
41.98–42.63 fps. These are interval averages, with slower windows remaining.
The goal of at least 30 fps passed; a 60 fps lock was not established.

Optimized float coasting measured 718–728 us versus 720–732 us fixed/NEON.
Float rebuilds measured 10.710–10.724 ms versus 12.795–12.890 ms fixed/NEON.
The admitted NEON translation kernel alone measured 18.11 ns/quad versus 26.27 ns
float32 and 30.07 ns scalar fixed. Retained scrolling does not translate every
glyph, so this kernel gain did not improve complete scrolling.

`GL_FIXED` kept the same position bandwidth and increased cold packing cost.
Rebased `GL_SHORT` reduced vertex bytes but failed the existing pixel gate.
Remove these encoding paths, runtime switches and fixed-point pilot modules,
features, check modes and dedicated probes. Keep historical measurement data and
original baselines, plus useful generic profiling and pixel-comparison tools.
No state wire format or public coordinate admission changes are required.

## Future SIMD decision

NEON supports four-lane float32 operations; fixed point is not a prerequisite.
Potential targets are independent dirty transforms/bounds and bulk vertex
packing. First profile the selected renderer to identify a material batch.
Keep inputs and output buffers contiguous, amortize setup, and avoid repeated
conversion or interleaving. Use safe Rust and compiler vectorization first.
Verify emitted vector instructions rather than inferring SIMD from array syntax.
New unsafe intrinsics require a separately reviewed boundary amendment.

Preserve viewport-based scrolling and conservative culling. Reject offscreen
work, use binary search for ordered strips, and never manufacture full-glyph
translation passes to justify SIMD. SIMD arithmetic does not repair floating
point cancellation, nonlinear precision or Mali fragment precision limits.

If a native batch wins complete-frame measurements, evaluate the same float
batch with Wasm SIMD128. Verify emitted Wasm operations and real browser cost.
Keep scalar fallback and select a compatible module before loading: unsupported
engines can reject a SIMD module even if its vector functions are never called.
Do not introduce variant loading or browser build complexity before that win.

## Qualification for another implementation

- Preserve the original font, density, coverage/color pipeline, scanout format,
  signed fixture provenance and independent reference captures.
- Record cold/rebuild and warm/coasting CPU p50/p95 separately, including
  allocations, conversion/packing and bytes transferred. An isolated kernel
  gain must translate into a useful complete-frame improvement. Agree a
  selection threshold before choosing a new default.
- Compare native pixels at rotations 0/90 and browser DPR 1/2/3.2, including
  fractional phases, hierarchy scales, small/large glyphs, clipping, negative
  bearings, input recovery and held immutable snapshots. Preserve the maximum
  channel error gate of eight and foreground/edge statistics.
- Maintain at least 30 kernel-presented fps on repeated dense native trials.
  Distinguish presented frames from worker ticks and report slower windows.
- Run required repository checks, actual native GPU tests and real browser
  checks. Keep licensing and safe-Rust boundaries intact.

## Primary references

- [Arm NEON Programmer’s Guide](https://documentation-service.arm.com/static/63299276e68c6809a6b41308).
- [GLES2 specification, vertex arrays/types](https://registry.khronos.org/OpenGL/specs/es/2.0/es_full_spec_2.0.pdf).
- [WebGL 1 specification, fixed point restrictions](https://registry.khronos.org/webgl/specs/latest/1.0/).
- [Rust Wasm SIMD support and module compatibility](https://doc.rust-lang.org/core/arch/wasm32/index.html).
- [WebAssembly portable SIMD design](https://github.com/WebAssembly/spec/blob/main/proposals/simd/SIMD.md).
