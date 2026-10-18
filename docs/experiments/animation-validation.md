<!-- SPDX-License-Identifier: Apache-2.0 -->
# Animation validation — 2026-10-19

The demos and ownership boundary are implemented; smooth panel-rate playback on the RK3126 remains unachieved. Separating CRDT work prevents a 250 ms state-worker pause from directly becoming a 250 ms drawing pause, but does not remove the current graphics/presentation bottleneck.

## Actual photo-frame measurements

Five 20-second runs on the real RK3126, native Wayland/SDL2/GLES2, logical 1280×800, Mesa Lima/Mali400, Weston GL. The panel reports 800×1280 at approximately 56.6 Hz. Observed clocks were CPU 600 MHz and GPU 148.5 MHz; this experiment did not alter clocks, kernel, compositor, display mode or touch calibration. The normal dashboard service was paused during each isolated benchmark set and restored afterward.

Native artifact/release measured: `43c813da4cd58ba0e81fda14b8469fed4b427bf22775eaa36559843e0ae83572`. The final paired release `f0db2a06c12b8546f643bb7f3977d68fc6866858baf9569ce44ba99cfebb0b7a` contains byte-identical native code (SHA-256 `fe2705800739a74f98add416849633276b50296aacf2b51cdccf5ea9b83adf3c`), with an additional browser route-clock suspension fix. The following table is host frame-start cadence, **not display presentation feedback or input-to-photon latency**. The drawing loop uses ordinary deadline pacing (up to 60 Hz); deliberate 250 ms stalls are confined to the state worker. The load/stall rows request 30 updates/s and add a 250 ms state-worker sleep approximately once a second. The actual achieved workload is reported; every successful load revision reaches the bounded view-publication path. It is not evidence of sustained 30 Hz CRDT processing on this CPU.

| Scenario | FPS | p95 interval (ms) | p99 interval (ms) | Achieved isolated updates/s |
|---|---:|---:|---:|---:|
| motion-12-idle | 22.24 | 50.88 | 53.57 | 0.00 |
| motion-64-load-stall | 21.21 | 55.42 | 67.10 | 6.17 |
| cubes-12-idle | 22.20 | 51.13 | 53.74 | 0.00 |
| cubes-64-idle | 22.16 | 51.57 | 53.65 | 0.00 |
| cubes-64-load-stall | 21.16 | 54.63 | 61.86 | 6.69 |

At 12 cubes with no load, median CPU composition/submission was 2.10 ms, while the median present call took 41.64 ms. With 64 cubes and load/stalls, those medians were 6.82 ms and 39.03 ms. The slow cadence is present without state load; the roughly one-FPS drop with load does not resemble a 250 ms freeze in the drawing loop. All five runs remain below the panel's refresh rate, with most frame intervals over 25 ms.

An earlier 12-cube run measured about 16.4 FPS. Caching HUD geometry reduced typical CPU submission from about 7 ms to about 2 ms, but by itself did not raise FPS. Clearing the solid stage and sampling only cached header/footer regions raised the native baseline to about 22 FPS. Disabling SDL's extra vsync wait was also tested (about 21.6 FPS on the earlier path) and was not retained. Raw baseline and final data are in [animation-frame-results.json](animation-frame-results.json).

## Real network changes, separate disposable room

A temporary server on a private Tailscale address held a fresh document, with no production state or credentials. A bounded producer requested 10 Hz for 20 seconds and committed **195 motion-intent changes**, sending 49,550 incremental-protocol bytes. The native frame ran the 12-cube demo for 35 seconds and processed **146 received view publications**; incremental merging and latest-view coalescing can combine several changes. It measured **20.78 FPS**, p95 59.30 ms, p99 68.36 ms, with median CPU composition/submission 2.70 ms and present-call time 43.30 ms.

After the producer stopped, the server's persisted final snapshot and the native presented/ready snapshot matched exactly: `80cd6d484cc4b1b1948ff9cdd3698d7c55f704a350983aee951cc3a52cb2ef71`. This demonstrates final-state convergence across the process/network boundary, not display of every intermediate mutation. The temporary listener was removed and the normal frame service restored. The production room's exact saved bytes were unchanged across these tests.

## Browser and regression checks

The in-app browser exercised both demos, the isolated CRDT/stall controls, local navigation, keyboard field activation, literal key output/Backspace, and browser Back through keyboard/screen layers. The final Mac in-app browser sample with the 250 ms worker busy-stall active recorded 120.00 FPS across the most recent 3,600 intervals, p95 9.60 ms, p99 10.20 ms and no intervals over 25 ms (14,080 total frame intervals). It is recorded alongside the raw results. These desktop browser timings do not establish iPad/iPhone performance or physical keyboard touch acceptance.

`bash scripts/check.sh` passes 61 checks: 46 core, two binary fixture tests, three monitoring adapter tests, five real-process HTTP/WebSocket tests and five OpenSSL signing/tamper checks. An additional native state-worker test passes (62 total), showing that navigation/persistence complete while the one-slot presentation mailbox is full. All-feature strict Clippy, formatting and both browser-module syntax checks pass. Linux CI also builds Wasm/native and exercises the 64-cube animation path under Mesa/Xvfb; software Mesa CI is a compatibility check, not hardware performance acceptance.

Core tests cover phase continuity across intent publication, no CRDT growth over 600 animation frames, pause/resume, bounded background-clock recovery, counting long foreground hitches, typed binary intent validation, remote malformed-input rejection, bounded mesh output, isolated stress convergence/history reset, and shared-intent versus local-load/navigation responders. Original seed/snapshot compatibility and prior keyboard/layout/sync tests remain covered.

## Remaining limits

This is a bounded CPU-projected 3D workload rasterized with GLES2/WebGL triangles, without a general depth-buffered GPU backend. It does not establish panel-rate animation, actual scanout timing, worst-case input latency, or production throughput under many peers. The next performance work should profile the native present/compositor path and update render resources by component, with compositor presentation feedback before claiming smooth 56.6 Hz output.
