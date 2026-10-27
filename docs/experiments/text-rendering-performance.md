# Text rendering baseline — 2026-10-26

The isolated Mali-400 renderer exceeds the 30 fps target with a scrolling screen
of approximately 2,780 visible Roboto glyphs. Three runs measured **35.35, 35.53
and 35.72 kernel-presented fps**. The panel refreshes at approximately 56.65 Hz.
These are development measurements, not a deployment or a claim of a 60 Hz panel.

| Workload | Kernel-presented fps | Notes |
| --- | ---: | --- |
| Original Input Lab, static | 28.32 | Earlier renderer and keyboard painting |
| Original Input Lab, fractional drag | 5.10 | 34,258 coverage misses over 360 frames |
| Sampled atlas plus CPU preparation fixes, Input Lab drag | 55–56 | Approximately 288 visible glyphs |
| Dense page, sampled atlas with rebuilt meshes | 18.8–28.3 | Rasterization reuse alone was insufficient |
| Dense page, retained viewports, reduced GLES rebinding | 35.35 / 35.53 / 35.72 | Culling, clipping and fractional offsets; 28 coverage misses total per run |

The dense fixture contains 28 independently clipped rows, five repetitions of
“The quick brown fox jumps over the lazy dog.” per row, at text scale 2. There are
5,040 retained nonspace glyphs; only the roughly 2,780 visible glyphs are submitted
per frame. Rendering uses 800×1,280 RGB565 scanout with rotation 90. Each run warms
up for 120 presentations and measures the next 240 using kernel page-flip sequence
numbers and timestamps. The final runs' CPU submission median is about 4.1 ms.

The replay isolates rendering from app/state/network work. A separate Cortex-A7
run of the synthetic scene, client tick/pointer processing and mesh preparation
measured 11.70 ms p50 / 13.96 ms p95 with rebuilt meshes, versus 4.19 ms p50 /
4.68 ms p95 with retained text. These costs are not added to replay FPS or claimed
as an end-to-end input-latency measurement; the native application uses separate
state/layout and presentation owners.

[Raw measurements and artifact hashes](data/text-rendering-2026-10-26.json).
The [native float32 baseline capture](assets/text-scroll-f32-2x.png) is synthetic
text captured during the measured run, rotated back to logical landscape.

## Hardware and gamma tables

The actual kernel reports Mali-400 GP plus two pixel processors, a 64 KiB,
four-way L2 with 64-byte lines and a 64-bit external bus, at 148.5 MHz. This is
the installed configuration, not an assumption about every Mali-400. Public
[Arm specifications](https://support.arm.com/compute-ip/mali-400) describe a
configurable 8–256 KiB L2; they do not establish this chip's private instruction
or texture-cache capacities. Those private cache sizes remain unverified.
[Mesa Lima documentation](https://docs.mesa3d.org/drivers/lima.html) explains that
fragment math uses FP16, even with `highp`; integer shader operations are lowered
to floating point too. CPU fixed point does not by itself fix that shader behavior.

`tools/dashboard/gamma-lut-probe.c` compared full-screen offscreen shaders on the
actual Mali driver. Inputs were already-linear foreground/background colors;
coverage was an arithmetic ramp. There were five warmups and 60 `glFinish` draws.

| Transfer | Table bytes | ms/draw | Maximum byte difference from FP16 arithmetic |
| --- | ---: | ---: | ---: |
| Arithmetic sRGB encoding | 0 | 73.07 | 0 |
| Linear-filtered 1D, 64 entries | 64 | 66.10 | 4 |
| Linear-filtered 1D, 256 entries | 256 | 66.14 | 1 |
| 64×64 addition/encoding table | 4,096 | 73.18 | 5 |
| 64³ source/destination/coverage table | 262,144 | 98.51 | 10 |

A 6-bit × 6-bit table is indeed only 4 KiB with one byte per entry. Here it maps
already weighted linear source and destination contributions to encoded output.
Table sizes above describe uploaded L8 bytes; driver tiling, alignment or format
expansion can change physical GPU storage. Cache residency was not measured.
Arbitrary source, destination and coverage are three independent inputs. Obtaining
the destination, decoding arbitrary input colors, and sampling glyph coverage
were not measured by this shader test. Table size alone did not predict speed;
there are no cache counters proving the cause of the difference. The 256-byte 1D
table was the best measured tradeoff. The existing optional linear-scene compositor
still needs a whole-scene target and resolve; this experiment does not enable it
by default or establish 30 fps with gamma-correct arbitrary blending.

## Pixels and the next baseline

Native tests use the admitted Roboto font and independent interior/hole references.
At 2× and 3×, retained/scissored text versus CPU-clipped text in the same filtered
pipeline differed by at most four byte values across tested offsets 0, 0.25,
20.5 and 110, at rotations 0 and 90. The comparison test rejects differences above
eight; it also rejects invalid culled ranges. Interior, hole, opacity, font-generation
invalidation and all-256-value color-transfer tests run separately.

Filtered coverage is intentionally approximate. Chromium's 180-glyph comparison
against direct analytic rendering measured edge maxima of 79–81 bytes at 2× and
71–73 at 3× over quarter-pixel phases. Whole-frame means were 0.064–0.073 and
0.031–0.034 bytes respectively; background pixels dilute those means, so they
are not a substitute for an edge comparison. These results do **not** pass the
older two-byte pixel-identity probe. The exact reference probe remains available.
Do not silently broaden its threshold to claim identity.

`capture-text-sampling-browser.mjs` records these metrics and produces stacked
PNGs: filtered output, direct reference, and an 8× amplified absolute difference.
The [2× comparison](assets/f32-sampling-2x-phase-025.png),
[3× comparison](assets/f32-sampling-3x-phase-025.png), and
[browser measurements](data/browser-sampling-f32-2026-10-26.json) preserve that baseline.
The committed float32 baseline is retained for the next milestone. Fixed point
and SIMD must be compared against the same filtering pipeline, dimensions, font,
phases and clip bounds so their errors cannot hide behind the sampling change.

## Reproduce

Generate the isolated fixture with Rust 1.90:

```sh
cargo run --release --locked -p otrios-core --features canvas \
  --example text_scroll_frames -- /tmp/dense.mesh --dense --retained
```

Omit `--retained` for the rebuilding baseline. The generator also writes a `.font`
packet for browser comparisons. Compile `text_scroll_probe` with the existing
Linux ARMv7 cross toolchain and EGL/GBM/DRM development sysroot:

```sh
cargo build --release --locked -p otrios-linux-display --features platform \
  --example text_scroll_probe --target armv7-unknown-linux-gnueabihf
```

Copy the fixture, executable and controller to disposable frame storage. Run as
root with the explicitly computed SHA-256 values and a new output directory:

```sh
python3 benchmark-text-rendering.py --executable ./text_scroll_probe \
  --sha256 BINARY_SHA256 --fixture ./dense.mesh --fixture-sha256 FIXTURE_SHA256 \
  --output ./text-results --repeat 3 --scale 2 --minimum-fps 30
```

The controller takes the display-trial lock, saves console mode, stops only the
direct display service, runs as kiosk, and restores the prior service in `finally`.
It checks that the renderer is actually Mali400 and fails if any run misses the
target. It never writes shared app state or activates a release.

```sh
node scripts/capture-text-sampling-browser.mjs --fixture /tmp/dense.mesh \
  --chromium /path/to/chromium --output target/text-rendering
node scripts/test_gpu_scene.mjs
bash scripts/check.sh
```

Compile the platform crate's tests for ARM and run its ignored Mesa tests on the
frame for native pixels. A browser/llvmpipe result is not Mali acceptance.
