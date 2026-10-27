# Chunked document reader qualification — 2026-10-27

RFC 0020 delivery 1 passes its native presentation, bounded-memory and pixel
qualification. Three 30-second physical-frame runs measure **56.38–56.45
kernel-presented fps**, with zero loading frames while generating incoming
chunks at 4,000 logical px/s and reversing. These are reader-probe measurements,
using the production document GPU adapter; they do not establish the full
Layout Lab chrome's frame rate or a 60 fps lock. Rich text and editing remain
delivery 2. [Machine-readable results](chunked-documents-2026-10-27.json).

The Layout Lab **Long document** experiment opens `/layout-lab/document`, with
about 1 MiB of wrapped text, drag/coast scrolling and source-anchored resize.
This change is qualified on a branch; it does not by itself activate a release.

## Selected implementation

The LGPL runtime owns immutable UTF-8 source, grapheme/source ranges, paged
width-independent advances, streaming line breaking, sparse paragraph
checkpoints, a height index, source anchors and independently versioned chunks.
One paragraph can span many pages; source admission does not build its mesh.
Local glyph positions remain float32; document heights/origins accumulate in
float64 and subtract the viewport origin before conversion to GPU coordinates.

Normal scrolling publishes transforms and visible ranges over retained buffers.
Conservative line/ink bounds reject offscreen work; the client additionally
rejects whole chunks outside ancestor clipping before GPU admission. A bounded
window prioritizes visible content and direction-aware prefetch. Measurement,
breaking and packing yield in 64-cluster batches, with a native 2 ms turn target
and a deterministic Wasm operation quota. Cold jumps/reflow can explicitly load;
estimated heights are never advertised as an exact document end.

Native has 128 document slots alongside its three UI/overlay/HUD layers.
Browser resource deltas are acknowledged separately from coalesced frame
metadata. Both adapters retain independent resources, preserve painter order,
and repair dependents after coverage-cache invalidation. Context recovery pauses
transfers and replays current resources. It also handles the interval between
actual WebGL loss and delivery of its asynchronous notification.

The core CPU cap is 8 MiB, with another 8 MiB reserved for host staging and
transfers; geometry GPU accounting is capped at 8 MiB and new raw uploads at
512 KiB per frame. Source admission allows at most 16 MiB UTF-8, 65,536 blocks,
256 bytes per extended grapheme, and 8 MiB index metadata. Owned capacities and
conservative GPU payload accounting are not allocator/driver RSS measurements.
The existing 512 KiB UI envelope still limits compressed embedded resources.

Typography is the declared single-font LTR prototype: greedy breaks after
ASCII space/tab/hyphen, four-space tab stops, whole-grapheme overflow and a
4,096-cluster hard line limit. Unsupported multi-scalar graphemes produce one
replacement marker, retaining their complete original source range. No Unicode
normalization, general shaping, bidi, styled spans or document editing is claimed.
UI format v4 admits this presentation; state sync v5 and control v1 are unchanged.
No new dependency, fixed-point pilot or architecture-specific runtime variant.

## Actual frame presentation

Cortex-A7 / Mali-400, Rust 1.90.0, unchanged font and text density, physical
800×1280 RGB565, rotation 90, alpha atlas scale 2, stable quads. The worker and
single pending-frame channel share immutable resources with the display. The
probe uses `crates/otrios-dashboard/src/document_gpu.rs`, not an offline flattened
mesh. Each run enters previously unmeshed content and reverses halfway through.

| Run | Kernel span | FPS | p50 / p95 / p99 interval, ms | Missed vblanks | Loading frames |
| --- | --- | --- | --- | --- | --- |
| 1 | 30.011 s | 56.447 | 17.653 / 17.661 / 17.668 | 6 | 0 |
| 2 | 30.011 s | 56.447 | 17.653 / 17.661 / 17.672 | 6 | 0 |
| 3 | 30.011 s | 56.380 | 17.654 / 17.661 / 17.675 | 8 | 0 |

Each run generates 352,789 drawable glyphs and uploads 1,126 chunks, totaling
90,595,584 raw bytes across the run; it evicts 1,070 GPU resources. Thus these
are not just warm interior replay measurements. Draw p95 is 4.764–4.899 ms;
worker turn p95 is 1.950–1.970 ms. Core working peak is 7,478,628 bytes or less,
with 71,192 index bytes. Cold GPU admission reports zero loading frames too.

Source validation is 67.5–72.0 ms, initial visible layout 8.17–8.77 ms. Display
startup to the first valid kernel completion is 1.29–1.34 s and is reported
separately; scrolling starts only after that completion. The measured window
then covers at least 30 seconds of actual kernel timestamps, excluding the
initial modeset's zero timestamp. The controller saves/restores tty mode, stops
only the direct kiosk while owning the exclusive display lock, and restores its
previous running state. Touch calibration and authority data are untouched.

[Saved native readback](chunked-document-native-2026-10-27.png) retains the
physical rotation used by the frame.

## Complete CPU work and memory

Deterministic corpora include varied paragraphs, a roughly 69 KiB paragraph,
8,192-character tokens, tabs, CRLF, negative bearings and unsupported graphemes.
Each CPU traversal visits the complete document at a simulated 4,000 px/s,
60 Hz viewport cadence; it reports CPU cost, not presentation FPS. All final
runs have zero loading turns and complete identical admitted source ranges.

| Source | Admission | First page | Exact heights | Full traversal CPU | Warm viewport p95 | Core peak | Index | Visible reflow |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 100 KiB | 6.46 ms | 9.79 ms | 198 ms | 555 ms | 16 µs | 7,472,964 B | 9,784 B | 10.22 ms |
| 1 MiB | 57.67 ms | 9.03 ms | 1.806 s | 5.790 s | 16 µs | 7,482,628 B | 71,192 B | 20.62 ms |
| 10 MiB | 581.03 ms | 9.54 ms | 17.702 s | 59.062 s | 16 µs | 7,483,456 B | 708,536 B | 17.39 ms |

The resident page count plateaus at 133–134. The 1 MiB traversal spans 4,331
viewport turns, roughly 72.2 seconds of maximum-rate consumption, completed in
5.79 CPU seconds: about 12.5× the required rate, above the 2× target. It packs
911,490 glyphs. The 10 MiB traversal packs 9,118,035 glyphs; source/index grows
while resident working memory plateaus. Warm viewport motion generates no new
geometry. Visible reflow is separate from completing all offscreen heights.

Generation viewport turns have p95 2.095–2.121 ms. The 2 ms work target is a
cooperative boundary, not preemption: the final 64-cluster step and bookkeeping
can overshoot. Maximum profiled turns are 2.50–2.85 ms. Source admission is a
separate bounded cold operation and is not covered by that turn budget.

## Portable packing and rejected NEON experiment

Five alternating paired 1 MiB trials compare identical work counts with zero
loading turns. Cached scaled glyph templates plus contiguous quad copies reduce
median complete generation from 6.358 to 6.006 seconds, about 5.5%; the portable
change is retained. Window planning once per turn, sequential token hints and
ASCII grapheme/lookup fast paths remove avoidable CPU work. Mixed Unicode uses
the pinned full segmentation implementation; randomized boundary tests check
the optimized path against it.

A separate five-pair experiment adds `-C target-cpu=cortex-a7
-C target-feature=+neon`. The CPU reports NEON support; disassembly of the actual
reader work contains vector loads/stores (`vld1`/`vst1`), with no attributed float
vector arithmetic. Median complete generation is **5.983 s generic versus
6.054 s NEON**, about 1.2% slower. Both execute the same 2,065,018 measured/broken
clusters, 911,490 packed glyphs and 21,870 line operations. The experiment fails
the agreed ≥10% complete-generation gate. No NEON flags, module selection,
unsafe intrinsics or Wasm SIMD variant are selected. These paired experiments
precede the final token hint; their exact binary hashes are in the data file.

## Correctness and pixels

An independent unpaged scalar reference searches fitting prefixes and chooses
legal breaks without the reader's streaming carry/checkpoints. Integration
compares every line/source range at widths 110, 267.25 and 704, including tabs,
CRLF, empty paragraphs, unsupported clusters and exact total height. Runtime
checks cover huge paragraphs, long tokens/zero advances, cache eviction, held
snapshots/backpressure, source identity/limits, reflow anchors and 17-million-pixel
origins with 0.125-pixel scrolling. Navigation/privacy clearing releases reader
state; document panning preserves the UI tree and shared app state.

Native reference/chunked replay compares five frames at rotations 0 and 90,
including fractional offsets and reflow. All ten pairs pass the unchanged
maximum channel-error gate of eight; observed maximum is **7**, worst mean
foreground error **0.01055**, worst mean edge error **0.01067**. The first frame's
1,080 visible quads have identical glyph attributes and at most 0.000153 logical
pixels of placement difference between grouped reference runs and local chunk
coordinates. These small coordinate-rounding differences accompany the edge
errors; pixels are not claimed identical. Saved candidate/reference/difference
images were inspected. Browser comparisons at DPR 1, 2 and 3.2 pass all 15 pairs
in each of default/stable-quad modes, maximum channel error **1**.

Actual Chrome acceptance exercises the 1 MiB demo, touch drag/coast, independent
client offsets, source anchors across portrait resize, three consecutive real
`WEBGL_lose_context` loss/restore cycles, and resource retirement on Back.
Browser screenshots and native readbacks are preserved with the raw evidence.

## Commands and acceptance

- `bash scripts/check.sh`: passed, including real process/Unix-socket tests,
  release signatures/activation and app publication fixtures.
- `bash scripts/build-dashboard-web.sh`: passed; nine app screens qualify.
- The CI JavaScript suite plus GPU-scene tests: 63 passed.
- ARM `cargo clippy --workspace --all-targets --all-features --release --locked
  --target armv7-unknown-linux-gnueabihf -- -D warnings`: passed.
- Actual frame native worker tests: 7 passed, including TCP reconnect,
  slow-consumer navigation, nested entry and bootstrap incompatibility handling.
- Actual frame ignored GPU tests: 8 passed in each of seven alpha/RGBA,
  scale 0/2/3 and linear-light configurations. Independent slot order,
  retirement and shared-coverage regression is included.
- `document_cost`, `document_frames`, `text_pixel_probe`,
  `document_scroll_probe CORPUS 30 4000 90 CAPTURE`,
  `scripts/compare-text-browser.mjs --dpr 1,2,3.2` (both quad modes), and
  `scripts/test-responsive-browser.mjs --document`: actual results above.

The initial copied native worker binary lacked its adjacent font; rerunning with
the verified font resource supplied explicitly passed. Early development runs
with loading turns or stale EOF estimates are excluded from paired-work claims.
The final 30-second timing gate replaces the preliminary five-second smoke run.

Deferred input reminder: slow/noisy lift-off can kick momentum. A later shared
input task will retain up to 200 ms of touch history, sample position/velocity/
acceleration before release, and box-filter the last sample against the recent
trajectory. That filter is not part of this reader change.

Raw binaries, corpora, controllers, captures, source snapshot and logs are
archived locally at `target/document-reader/qualification-evidence.tar.gz`
(SHA-256 `14c2df1ec685430cf0778a00e1bcbeecb87e75e882fcfe88f6b1c7ea677c33b9`).
All 56 owned remote trial files (137,258,677 bytes) were hash-verified against
the local archive before removal. The disposable local authority and ARM
builder were stopped after acceptance; the frame kiosk was restored. Original
renderer baselines and earlier evidence archives remain intact.
