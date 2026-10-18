<!-- SPDX-License-Identifier: Apache-2.0 -->
# Direct display validation — 2026-10-19

The RK3126 now runs the dashboard through the owned DRM/GBM/EGL/GLES2 backend, with Weston and the browser stopped. The earlier SDL KMSDRM experiments below establish the progression to that backend. Linux/Mesa and the existing drivers remain. Actual DRM client listings showed exactly one dashboard owning DRM master. The SDL cursor was removed. The user confirmed direct touch works after permitting SDL/udev's AF_NETLINK device discovery; before that correction, rendering ran but the process had no open input device. Stationary cubes in the live room were separately explained by its persisted `playing=false` intent, not a stopped render loop. Readiness now reports motion intent and requires an accessible touch device.

The existing 800×1280 mode remains at 66 MHz with totals 880×1324, approximately 56.6 Hz. CPU/GPU clocks, kernel, panel timings, bootloader, accepted touch matrix and existing boot recovery are unchanged. Every benchmark ran alone, with no live room/state options; the controller restored the original services after normal completion and handled interruption. Complete direct-service failure was deliberately injected with ExecStart=/usr/bin/false; the normal dashboard and Weston resumed, exactly one native process remained, and the saved console mode was restored. The direct unit is not enabled at boot.

## Measurements

All ordinary cases ran for 20 seconds. The first two columns below measure host frame-start cadence on the real device; they are not display feedback. Each release binds the signed ARMv7 executable and browser module.

| Scene | Direct baseline | Opaque/culling optimization | Tiled copies + batching |
|---|---:|---:|---:|
| 12 circles, idle | 50.04 FPS | 49.40 FPS | Not sampled |
| 64 circles, local CRDT/stalls | 28.23 FPS | 28.26 FPS | Not sampled |
| 12 cubes, idle | 28.74 FPS | 50.81 FPS | 54.63 FPS |
| 64 cubes, idle | 28.29 FPS | 49.41 FPS | Not sampled |
| 64 cubes, local CRDT/stalls | 28.27 FPS | 31.58 FPS | 53.83 FPS |

The paired Weston baseline was about 22.2 FPS idle and 21.2 FPS under local load. The load cases request 30 changes/s with a deliberate 250 ms state-worker sleep about once a second. Actual achieved workload varies: the final tiled 64-cube case committed 134 isolated load revisions in 20.046 seconds, about 6.68/s. This does not establish 30 Hz CRDT processing. Presentation continues independently from the last view during worker stalls.

The tiled sample used signed release `072814a462b077422ea139dcad6ecadd7083c10f16a57a06f89a71d8d94edc78`. Its in-process libdrm read counter recorded **54.846 presented FPS** for 12 cubes after two seconds of warm-up: p95 17.668 ms, p99 35.300 ms, 32 missed vblanks over 976 events/17.777 seconds. The loaded 64-cube case recorded **53.997 presented FPS**, p95 17.808 ms, p99 35.309 ms and 47 missed vblanks over 959 events/17.742 seconds. These are kernel completion timestamps, not photon/input latency, and **do not meet the no-missed-refresh target**.

Raw aggregate files: [paired baseline](direct-display-baseline.json), [opaque geometry](direct-display-opaque.json), [tiled copies](direct-display-tiled.json), and [alternative experiments](direct-display-alternatives.json). Baseline release is `cb268fd341df4fca70b9fa5ab605b8b8d563e0df96d11a5dc6e3a4bb8ff786b3`; the opaque path is `8de7ec0197e2bfebd2eb5c3a17941f7211ef549794a720957b03e38ba552a2aa`. Later source refactoring and a smaller HUD target have passed regression checks and live signed reload, but these tables are not measurements of those later binaries.

## What changed and what did not help

Opaque animated geometry disables destination blending; hidden faces of closed, nonintersecting opaque cubes are culled. The cache and telemetry use separate targets. Conservative 32×8 tile coverage copies only regions containing foreground primitives; skipping exact opaque canvas-background triangles preserves the final pixels, including occluded regions and translucent content. Copy regions are nonoverlapping and travel with their immutable geometry. In the 12-cube sample, copied area fell from 409,600 to 222,464 pixels/frame, about 46%. Batching reduced median CPU submission to roughly 0.64 ms. A later HUD target occupies only its 64-pixel logical band rather than a full-screen allocation.

Reissuing static geometry through SDL every frame cost about 10.8 ms CPU and reached 28.3 FPS; it was removed. A one-pixel completion readback reached 52.0 FPS but included about 17 ms of synchronization/resolve/readback work and is retained only as an isolated diagnostic. It is not a pure GPU timer or a production render policy. Toggling the old KMS double-buffer hint did not establish a different effective policy on the pinned SDL build.

Per-syscall ptrace tracing perturbs the timing: the optimized case that ordinarily ran near 51 FPS dropped to about 28.3 FPS while traced. Those separate trace rows must not be used as normal presentation acceptance. The optional ARM32 loader-audit counter reads completion records already returned to libdrm, retaining a bounded event array with no per-frame logging. It preserves read results/errno and records only the configured DRM card's event data. The measured callback/event method adds a small amount of work; it is not asserted to have zero overhead.

## Regression checks and next backend

The final `bash scripts/check.sh` passes the core, binary-fixture, monitoring, real-process HTTP/WebSocket, Linux-display geometry, signing/direct-readiness and DRM record parser checks. The shared native state-worker/backpressure test passes separately. Linux platform checks additionally exercised partial initialization and actual EGL pixels on the frame. All-feature strict Clippy and formatting pass. Tests cover conservative copy coverage/nonoverlap, opacity handling, culled near-face silhouette, reversible rotation and GPU projection, accepted calibration and discovery readiness, and existing CRDT/input/navigation/keyboard behavior.

[The direct-display guide](../DIRECT_DISPLAY.md) describes service recovery and tools. [RFC 0001](../rfcs/0001-direct-linux-gles2.md) proposes owning display/EGL/GLES2/evdev directly, retaining GPU buffers and comparing true RGB565 scanout against 32-bit output. That review was accepted; the later owned-backend results below supersede this earlier implementation status.

## Owned DRM/EGL/GLES2 backend

RFC 0001 was accepted by the user on 2026-10-19. The SDL-free development host passed ARMv7 geometry bounds/partial-init checks and real Mesa/Lima EGL pbuffer pixel tests in both formats, including retained VBO rendering and source-over alpha. Its dynamic dependencies contain libdrm, GBM, EGL and GLESv2, with no SDL. Both formats match the kernel framebuffer inspection. Static header image comparison found at most 4/2/4 RGB channel difference for RGB565 versus 32-bit output, consistent with quantization; this is not a physical panel colour/gradient acceptance test.

All rows below are kernel completion feedback after two seconds of warm-up in 20-second isolated trials at unchanged clocks. These use the **former cube geometry**, before the subsequently requested tetrahedron change. The complete reports and binary SHA-256 are in [direct-display-owned-gles.json](direct-display-owned-gles.json).

| Scene | XRGB8888 FPS / missed vblanks | RGB565 FPS / missed vblanks |
|---|---:|---:|
| 12 circles, idle | 55.979 / 12 | 56.480 / 3 |
| 64 circles, local CRDT/stalls | 28.323 / 510 | 45.528 / 200 |
| 12 cubes, idle | 56.202 / 8 | **56.647 / 0** |
| 64 cubes, idle | 55.425 / 22 | 56.480 / 3 |
| 64 cubes, local CRDT/stalls | 54.423 / 40 | 55.980 / 12 |

RGB565 uses pitch 1,600 bytes versus 3,200 bytes for 32-bit output. The 12-cube RGB565 row has p95 17.658 ms, p99 17.665 ms, max 17.679 ms, with 1,021 completion events over 18.006 seconds. The loaded 64-cube RGB565 row still misses 12 vblanks, p99 35.290 ms; its isolated CRDT worker achieved 197 updates in 20.068 seconds (9.82/s), not the requested 30/s. The 64-circle case remains below refresh rate. Therefore **only the bounded 12-cube row meets the no-missed-refresh target**; this does not establish universal refresh lock or later tetrahedron performance.
