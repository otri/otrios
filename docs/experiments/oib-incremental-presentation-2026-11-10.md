<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB incremental presentation — 2026-11-10

This follows the [measured Apply hotspots](oib-hotspots-2026-11-09.md).
The implementation removes redundant preparation before adding worker concurrency.

## Architecture

- Retain admitted typed Blueprints through source, graph, instance and placement
  transforms. Serialized artifacts retain the same byte admission limits and
  exact canonical output; internal transforms avoid deserializing their own output.
- Share authored and expanded metadata by exact source/binding leaf hashes and
  admitted UI identity. Cache canonical baseline digests with their binding.
  Equivalent active/draft compilation also shares the result regardless of selection.
- Classify effective property dependencies after inheritance and terminal-output
  precedence. Proven Color-only edits retain expanded graph, qualified layout,
  responder mappings and input identities. Numeric, text, structural and composed
  color dependencies without a complete tracked path use full compilation.
- Keep connected confirmation, held-input, focus and composition activation gates.
  Inspector value/status updates can retain hierarchy/tool models and compatible
  row identities. The first override can add a Reset row and change inspector shape.
- Separate per-draw straight RGBA from immutable vertex and glyph buffers in GLES2
  and WebGL. Color updates preserve coverage resources and need no vertex upload.
  Retained preparation admits visible primitives plus 64 logical pixels of overscan;
  an uncached primitive entering the visible clip refreshes the bounded working set.
  Exact clips, painter order and input semantics remain authoritative.

There is no new dependency, state wire format, executable ABI, worker pool or
unconfirmed speculative activation. GL ownership remains on the renderer thread.

## Validation

Focused checks completed before the signed build:

- Core compiler/cache tests: **9 passed**, including exact artifact parity,
  selection-independent reuse and source/binding/UI invalidation.
- Dashboard/layout/render tests: **72 passed**. Designer tests: **49 passed**,
  one existing manual timing diagnostic ignored. These cover full-resolution
  paint parity, inherited outputs, held confirmed input, focus/keyboard geometry,
  stable hierarchy/actions and structural/numeric/text fallback.
- Core example compilation passed. Canvas integration test compilation passed;
  this was a scaffold compile, not a separate integration acceptance run.
- Web GPU scene checks: **15 passed**. Worker checks: **16 passed**. Host export
  check: **1 passed**. Native C draw helper checks passed, including alpha paint,
  analytic/cached tint, zero upload and restoration of legacy color arrays.
- Focused core Clippy with warnings denied and formatting passed. The final
  normalization-context refactor was followed by **9 retained renderer tests**.
- Signed Mac tools, Wasm and ARM hosts built. Wasm emitted the existing three
  feature-specific dead-code warnings in collection-transfer helpers.

The full suite and a browser interaction campaign were intentionally deferred
in favor of focused live checks.

## Signed deployment and native acceptance

Implementation commit `bec2bdd` built signed release
`ecfeca9f059e0252bc2ddc0d5aa49205e3211bf60087892c2a908004ba073abe`.
The existing Frame authority and GLES2 host activated it successfully; public
health, confirmed native readiness and existing service checks passed. The code
change restarted the authority. The readiness check itself did not restart it.
Monitoring resumed successfully. A compatible signed code rollback remains
available. No app state reset, pixel export, new service or trust change occurred.
State protocol remains `otrios-canvas-sync-v30`; the v1 opening control envelope
and the OIB UI/library payloads are unchanged.

The calibrated native input probe selected Display, edited Accent, tapped Apply,
waited for confirmed active/presented generations, and restored gold. It passed
before tracing and again after restoring the normal service environments. Final
active/presented generation is **28**, Display selected, no pending input/commands.
Both native and authority process environments were checked to contain no
profiling variables, and task drop-ins/tmpfs buffers were removed.

## Actual Apply timings

Software evdev touch-down to confirmed native presentation, milliseconds:

| Profiling-off control | Blue Apply | Gold restore |
| --- | ---: | ---: |
| Previous release | 789.6 | 691.3 |
| New release, before tracing | 397.5 | 288.9 |
| New release, normal environment restored | 392.0 | 326.2 |

These include an intentional **80 ms touch hold** and up to about **5 ms polling**.
Readiness publication adds its own delay beyond the polling interval. These are
small ordered trials, not a distribution or photon-latency measurement.
The separate traced run completed actual native presentation in **356.9 / 277.4 ms**
(**276.5 / 197.1 ms after touch release**). Trace/control differences do not
establish a profiling-overhead estimate.

| Traced pipeline segment, ms | Previous blue/gold | New blue/gold |
| --- | ---: | ---: |
| Apply release handler | 42.1 / 44.0 | 14.1 / 14.0 |
| Offer → response arrival | 292.8 / 136.2 | 136.5 / 87.3 |
| Client receive processing | 375.6 / 331.7 | 73.3 / 48.5 |
| Final CPU composition | 16.6 / 15.0 | 11.8 / 11.0 |
| Renderer preparation/upload + GL submit | 13.3 / 12.0 | 13.0 / 11.9 |
| Present wait | 18.0 / 25.9 | 19.3 / 17.4 |

Native layout decodes dropped **14 → 0** in each traced Apply. Encodes dropped
**6 → 2 / 0**, compile-layout calls **2 → 0**, metadata expansions **25 → 3**,
and actual scene resolutions **9 → 2**. The first color uses the retained paint
compiler; restoring the preceding color hits the small compilation MRU. Authority
decodes are also zero; metadata expansions are one per trial. Exact raw causal,
schema and closure validation still runs.

Remaining latency is mainly the serial authority response and native receive.
Authority process was **126.5 / 72.8 ms**, including one durable sync of
**17.4 / 17.7 ms**. Cold binding normalization and two boundary encodes remain
material. Warm exclusive validation was **15.4 ms** on authority and **12.7 ms** on
native; import/export checks and inspector projection also remain. Inclusive
profiler spans overlap and must not be added to their children.

The full inspector interaction still changes text/status and rebuilds retained
geometry twice. The recorded final presentation upload counter was **202,032 bytes**
per trial, previously **308,736**. Pure recolor's zero-upload contract is covered
by focused host tests; it does not mean that the entire Apply interaction has no
vertex uploads. Finer inspector/chunk retention and repeated import-contract
qualification are the next concrete targets. A worker pool remains deferred.

## ARM microbenchmark

The optimized 128-node rounded scene resolves expressions, constructs and validates
a scene in **0.612 ms**, approximately **209,231 nodes/sec**. This stage excludes
OIB compilation, CRDT work, IO, GPU upload and presentation.

Retained color changes dropped from **14.385 ms → 0.672 ms**. In the clipped case,
16 visible leaves now prepare **2,496 triangle vertices / 60,096 mesh bytes**,
previously **19,812 / 477,012** despite the same visibility. Cold retained rebuild
was **2.072 ms**, previously **17.350 ms**. New unchanged/color update overhead in
that clipped case was **0.371 / 0.380 ms**. The synthetic workload isolates stages;
it does not predict total app latency or arbitrary layout scalability.

## Reproduction and evidence

Commands actually run include the focused core tests/Clippy above, the three host
scripts, `scripts/test-canvas-host-exports.mjs`, the established stage-only signed
builder, `scripts/deploy-dashboard-frame.py`, and `scripts/check-frame-release.py`.
The ARM example was built for `armv7-unknown-linux-gnueabihf`, transferred by pinned
SSH, SHA-256 checked, and run with `--nodes 128 --iterations 200` before removal.
The automatic builder is restored after source promotion; no accepted-work artifact
maintenance is implied by this development qualification.

The configured toolchain wrapper supplied the pinned build environment. Actual
focused host commands included:

```sh
python3 /tmp/otrios-d2-cargo.py clippy -p otrios-core --features canvas --lib --locked --offline -- -D warnings
python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib retained_text --locked --offline
python3 /tmp/otrios-d2-run.py node scripts/test_gpu_scene.mjs
python3 /tmp/otrios-d2-run.py node scripts/test-canvas-worker.mjs
python3 /tmp/otrios-d2-run.py node scripts/test-canvas-host-exports.mjs
python3 scripts/test_linux_display_bindings.py
```


Raw evidence: [controls](data/oib-incremental-presentation-2026-11-10/control-apply.json),
[restored-environment controls](data/oib-incremental-presentation-2026-11-10/control-restored-apply.json),
[traced trial](data/oib-incremental-presentation-2026-11-10/traced-apply.json),
[analysis](data/oib-incremental-presentation-2026-11-10/analysis.json),
[ARM benchmark](data/oib-incremental-presentation-2026-11-10/layout-arm.jsonl),
[readiness](data/oib-incremental-presentation-2026-11-10/readiness.json),
[environment restoration](data/oib-incremental-presentation-2026-11-10/environment-restored.json),
[metadata](data/oib-incremental-presentation-2026-11-10/metadata.json), and
[native probe](data/oib-incremental-presentation-2026-11-10/native-probe.py).
Reproduce trace reduction with the [existing analyzer](data/oib-hotspots-2026-11-09/analyze.py)
and the new data directory/traced-trial paths. Native input/worker/renderer and
authority JSONL in that directory preserve the individual events.
