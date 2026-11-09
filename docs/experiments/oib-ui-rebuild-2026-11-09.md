<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB UI rebuild diagnosis — 2026-11-09

The Designer spends substantial time preparing derived editor state, including
when the document is unchanged. This investigation adds an opt-in timing
diagnostic; it does not change the deployed runtime or claim an optimization.

## Live Frame observation

The Frame was running signed release
`ca310b5ca4a75a43b7891a1492085bfae35a04bf29cddff54d4f8160ea39d8e1`,
with OIB Designer mounted, state confirmed, its stream ready, no input capture
and zero pending commands. A 3.016-second `/proc` thread CPU sample measured the
UI/state worker at **90.17% of one core**. The display/main thread used 3.65%,
the input thread 2.65%, and the network thread 0.66%. A preceding sample measured
the same hot thread at 91.83%.

Thread ownership follows creation order in
[`direct_host.rs`](../../crates/otrios-dashboard/src/direct_host.rs): the state
worker is created before touch input and HUD, and its network owner is started
from [`native_worker.rs`](../../crates/otrios-dashboard/src/native_worker.rs).
The measurement records CPU consumption, rather than sampled user-space stacks.
No service was stopped, no live edit was issued, and no app data was changed.

Raw observation: [frame-idle.json](data/oib-ui-rebuild-2026-11-09/frame-idle.json).

## Optimized stage measurements

The diagnostic used Rust 1.90.0, the optimized release test profile, and an
Apple M2 Max running macOS 27.0.1. It decoded the exact installed release's
`oib-designer.otapp/UI/main.flex`, including the resolved library baseline,
then created an isolated in-process authority and client with two instances.
It did not read the live app's durable data. These are **Mac stage timings**,
not Frame input-to-display latency, browser timings, or network benchmarks.

| Stage | Median |
| --- | ---: |
| Confirmed property: Mount refresh | 42.801 ms |
| Status-only rejection: Mount refresh | 38.805 ms |
| Warm root selection: Designer rebuild | 37.942 ms |
| One hierarchy move availability check | 15.438 ms |
| Property authority/client synchronization | 13.601 ms |
| One hierarchy duplicate availability check | 12.081 ms |
| Root selection rebuild in Test mode | 3.550 ms |
| Direct OIB compilation | 2.948 ms |
| Unchanged healthy status text | 0.316 ms |
| Unchanged healthy client tick | 0.314 ms |
| Unchanged tick with a rejected status | 0.005 ms |

Each unchanged path has 20 samples, most rebuild stages three, and cold
selection/status-only rejection one. Property refresh ranged from 42.456 to
42.936 ms; its synchronization ranged from 13.569 to 15.360 ms. The latter
includes both in-process authority and client work and excludes live socket
transport and persistence. Stages overlap and must not all be added together.
Sub-microsecond samples are truncated to zero, not proof of zero work.

The rejection experiment retains the same active UI generation, but its error
status short-circuits normal capacity formatting. This isolates the expensive
healthy status path without changing the implementation. Edit/Test mode
comparison and individual candidate timings support the toolbar diagnosis;
they are not an exact partition of every substage in a rebuild.

Raw [installed-library results](data/oib-ui-rebuild-2026-11-09/installed.jsonl)
and [local-definition fixture results](data/oib-ui-rebuild-2026-11-09/local.jsonl)
retain every sample. [Metadata and aggregate values](data/oib-ui-rebuild-2026-11-09/summary.json)
record the exact input hash and source revision.

Run with the established private build environment or an equivalent Rust setup:

```sh
cargo test -p otrios-core --features canvas --lib --release --locked --offline \
  profile_oib_designer_refresh_stages -- --ignored --nocapture
```

Set `OTRIOS_OIB_PROFILE_UI` to the installed `UI/main.flex` to use the library
baseline. Without that variable the diagnostic uses the repository's local
production fixture with two and six instances. The external input must retain
the known Designer fixture's semantic addresses. This is an ignored diagnostic,
with no wall-clock pass/fail threshold.

Both diagnostic variants passed (one test each, 4.20 seconds for local source
and 0.61 seconds for the installed UI). The existing
`frame_focus_direct_inputs_commit_once_and_reset_without_changing_bindings`
test also passed (one test, 0.92 seconds) after factoring its shared setup
through an equivalent helper accepting a UI argument.

## Runtime causes

1. `Client::tick` calls `Mount::refresh` for OIB mounts on every owner turn.
   `refresh_designer` computes `oib_status_text` **before** checking its cached
   revision. Status generation asks for live instances and available slots;
   those functions repeatedly read and clone authored source. Even the unchanged
   fast path therefore performs significant document processing. The native
   owner sleeps only two milliseconds between turns.
2. `Document::oib_source` decodes the complete source and reserializes it for
   canonical validation on each call. `oib_authored_ui` also serializes and hashes
   definitions and presets before checking its UI cache. That cache avoids
   rebuilding a materialized UI, but still charges substantial work for hits.
   Placements and connections independently reread the same source.
3. The hierarchy toolbar calls `oib_hierarchy_allowed` for candidate insert,
   move and duplicate actions. Each call clones a document, stages a real
   structural transaction, prepares the complete candidate, compiles it and
   validates the result. This work decides whether to show a button; the user
   has not selected that operation. The graph-authoring toolbar has a cache,
   while these hierarchy candidates are regenerated on editor reconstruction.
4. A property confirmation changes the OIB root and invalidates the Designer
   revision. It consequently regenerates structural tools along with property
   labels and layout. Status-only rejection changes can also cause that work.

Relevant implementation:
[`client.rs`](../../crates/otrios-core/src/canvas/client.rs),
[`oib_designer.rs`](../../crates/otrios-core/src/canvas/client/oib_designer.rs),
[`source.rs`](../../crates/otrios-core/src/canvas/state/oib/source.rs),
[`state/oib.rs`](../../crates/otrios-core/src/canvas/state/oib.rs),
[`lifecycle.rs`](../../crates/otrios-core/src/canvas/state/oib/lifecycle.rs),
[`hierarchy.rs`](../../crates/otrios-core/src/canvas/state/oib/hierarchy.rs),
[`structure.rs`](../../crates/otrios-core/src/canvas/state/oib/structure.rs).

## Full source build is a separate cost

The last complete L1 correction build recorded Cargo phases of 16.47 seconds
for Mac tools, 3.68 seconds for the `canvas-pack` feature variant, 20.37 seconds
for the Wasm host and 28.60 seconds for ARM hosts/tools: **69.12 seconds of
Cargo-reported time**, before other packaging, signing and deployment work.
Those phases run sequentially for any changed watcher fingerprint.

[`watch-dashboard.py`](../../scripts/watch-dashboard.py) recreates the frozen
source with `shutil.copyfile`, giving unchanged files new timestamps, and
deletes `/src/crates` before extracting the ARM source. The logs show repeated
compilation; Cargo dirty-reason tracing was not run, so timestamp invalidation
is a plausible additional cause, not separately measured attribution.
The two Mac invocations also use different feature sets.

[`publish-oib-library.py`](../../scripts/publish-oib-library.py) already avoids
Cargo for signed library data changes. The runtime editor slowdown described
above occurs after loading and is independent of host compilation.

## Recommended implementation order

1. Check cheap immutable revision/confirmation/saving/editability inputs before
   rebuilding status text. Leave unchanged ticks free of source decoding.
2. Keep one validated source/cache entry keyed by the exact source leaf and
   signed baseline identity. Reuse decoded source, placements, connections and
   resolved metadata; validate new bytes when their identities change.
3. Split property/status presentation refresh from structural tool generation.
   Replace transactional toolbar simulations with a bounded pure preflight,
   retaining complete authority validation when an operation is submitted.
   Any cached preflight must include every input affecting its decision,
   including baseline, source, residency, selection and relevant values.
4. Profile the resulting implementation on the Frame before broadening the
   change to incremental graph/layout compilation. Retain current input
   capture, pending-edit and coherent presentation activation barriers.

No production optimization, deployment, broad suite, or Linux IPC acceptance
was performed as part of this diagnosis.
