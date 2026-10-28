<!-- SPDX-License-Identifier: Apache-2.0 -->
# Browser startup and app navigation — 2026-10-25

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/browser-startup-profile.md) remain available at the pinned historical revision.

This is the frozen baseline. The subsequent [deployment and before/after
measurements](startup-improvements.md) describe the implemented changes.

Home became interactive in **2.22 s cold** and **0.95 s on reload** in a controlled
fast-4G browser replay. Home → app navigation took **145–232 ms median after tap
release**, depending on the app. Monitoring was excluded.

These are Chromium measurements on a cloud CPU with software rendering, not
iPad/Safari field timings. Separate public HTTPS probes found a substantial
delivery problem worth investigating before tuning smaller startup steps.
Production code, state and deployment configuration were not changed.

## Conditions and completion criteria

- Release: `d9fa55ceab9782ef59b54d849b09730b448c479f862e87a066207f70a09a8e10`;
  source base: `fc6b868b05178cc7646a7dc3ec61dcd5abc37428`.
- Chromium 154.0.8037.57, Linux x86_64, Xeon Platinum 8573C, four-core container
  quota; no CPU throttling; SwiftShader; 800 × 1280 viewport, DPR 1, touch enabled.
- Nominal link: **9 Mbps down, 1.5 Mbps up, 60 ms RTT**. One TCP scheduler shares
  each direction's bandwidth across HTTP connections **and WebSocket traffic**.
  Calibration checks concurrent downloads, uploads, RTT and delivery before FIN.
  These are the [DevTools Fast 4G nominal targets][fast4g]; the scheduler does not
  reproduce DevTools' empirical preset adjustment factors.
- Actual public CDN response bodies and compression were captured. Manifest hash,
  publisher Ed25519 signature and decoded asset hashes were verified before replay.
  Dynamic API and state WebSocket requests used an optimized, disposable local
  authority with fresh app-state roots per scenario.
- Each cold run started a fresh Chromium process/profile. Startup begins at
  navigation, excluding browser launch. Warm runs reload the same profile after
  cache readiness; baseline runs first visit all five public apps. Startup-only
  prototypes reload after Home, so warm variant comparisons are less controlled.
- Completion requires the destination UI and controls to be installed, a GL draw
  submitted, then the next animation frame. This is a presentation proxy, not a
  physical display measurement. URL changes alone do not count.
- Navigation starts from idle Home, using trusted touch/pointer events. Primary
  timing starts at pointer release; full pointer-down timing is retained too.
  A requested 30 ms hold typically lasted longer due to automation scheduling;
  timestamps come from the actual events and include input delay.
- HTTP/1.1 replay excludes DNS, TLS, CDN/tunnel latency, radio behavior and TCP
  congestion. Authority CPU/storage is local x86_64 rather than the ARM frame.
  Instrumentation, worker attachment and software rendering add overhead. This
  is a reproducible comparison baseline, not an estimate of complete iPad startup.

## Startup

| Scenario | Cold median | Observed cold range | Warm median | Runs |
| --- | ---: | ---: | ---: | ---: |
| Current release, fast 4G | 2,219 ms | 2,037–2,601 ms | 954 ms | 5 |
| Start font/catalog alongside main Wasm | 2,228 ms | 1,808–3,590 ms | 1,069 ms | 5 |
| Share compiled Wasm module with worker | 2,093 ms | 2,030–6,334 ms | 935 ms | 5 |
| Both local prototypes | 2,070 ms | 1,848–3,470 ms | 895 ms | 5 |
| Current release, unshaped control | 1,456 ms | 1,267–1,471 ms | 715 ms | 3 |

[Archived figure: Startup phase comparison](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/browser-startup-2026-10-25/startup.svg)

Bars partition a representative median run at observed bootstrap milestones;
they are elapsed intervals, not exclusive CPU attribution. Lines show the full
observed range. All outliers remain in the dataset.

The main Wasm asset is **2,970,576 decoded bytes**, delivered as **777,007 Brotli
bytes** in the replay snapshot. The state worker currently initializes Wasm again.
Passing the main thread's compiled `WebAssembly.Module` to a separate worker
instance reduced median worker Wasm initialization from **103.9 ms to 1.1 ms**
(0.9 ms in the combined prototype). Instances and memories remain independent.

The combined prototype improved the cold median by **149 ms / 6.7%**. Ranges
overlap, samples are few and software-rendering outliers are large; this is a
promising direction, not a demonstrated iPad improvement. Parallel resource fetch
reduced the median post-Wasm font/catalog/setup interval from roughly 237 to
114 ms, but did not demonstrate an overall startup improvement by itself.
All prototype transforms were applied only by the local replay harness.

## Home → app navigation

Five first visits and five repeat visits per app, returning Home between taps.
Each cell gives median **tap release → rendered destination**, with min–max.
Repeats occur in the same loaded browser session, after visiting the other apps.

| App | First visit | Repeat visit | Received per first mount¹ |
| --- | ---: | ---: | ---: |
| Component Playground | 229 ms (200–565) | 168 ms (166–203) | 49,361 B |
| Layout Lab | 200 ms (189–221) | 193 ms (167–227) | 56,485 B |
| Input Lab | 232 ms (212–248) | 218 ms (201–233) | 100,836 B |
| Animation Lab | 153 ms (136–187) | 136 ms (134–177) | 18,798 B |
| 3D Lab | 145 ms (134–162) | 162 ms (136–175) | 18,770 B |

¹ Binary WebSocket application payload bytes during the measured transition;
not TCP/TLS overhead. Repeats still deliver the full UI. Playground's cached
behavior saves 218 bytes on repeat visits. No repeat improvement is established
for 3D Lab. Home returns measured 194 ms median, range 153–931 ms, across 50 taps.

[Archived figure: App navigation timings](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/browser-startup-2026-10-25/navigation.svg)

Full pointer-down → presentation medians were respectively **280/208, 240/244,
270/262, 191/176 and 193/202 ms** for first/repeat visits. Per-transition data also
separates release-to-mount dispatch, network/authority response, worker processing
and final rendering. Those phase medians need not sum to the total median.
Nested Layout Lab screens were sized in the codec experiment below; their tap
latencies were not sampled in this Home-card pass.

## Public delivery probe

Three sequential HTTPS Wasm downloads, requested **from the frame through its
public Cloudflare URL**, took **4,717 / 4,706 / 4,342 ms**. All returned
`CF-Cache-Status: DYNAMIC`, with no `Age`; Brotli payloads were about 777 KB and
decoded to the same release hash. Response headers arrived in 218–242 ms.
Direct loopback-origin reads of the 2.97 MB identity body took 355 ms initially,
then 77 ms. These probes identify the public delivery path as a priority; they
do not isolate Cloudflare, tunnel, origin compression or network throughput as
the sole cause. They are neither mobile measurements nor additive to replay time.

## Recommended sequence

1. **Investigate public immutable asset delivery and edge caching.** Check cache
   rules, Development Mode and tunnel/origin compression for public
   `/releases/{id}/web/*` and the public catalog. Cache only admitted immutable
   public bytes; keep discovery, state and authenticated/private routes dynamic.
   Verify an actual edge `HIT` and repeat the HTTPS probes before claiming a win.
   At 9 Mbps, the Wasm body alone has about 0.69 s ideal payload transfer time.
2. **Share the compiled Wasm module with the state worker.** The initialization
   step improved consistently in local trials. Preserve separate instances and
   memory, verify Safari support and measure on iPad before accepting total
   startup savings. Parallel font/catalog loading can be evaluated alongside it,
   but the present samples do not show an independent total-time benefit.
3. **Reduce large and repeated mount delivery.** Trial existing sync-v4 LZ4 for
   large `Mounted` responses, then consider a bounded UI cache keyed by the
   admitted catalog UI hash. A cache needs explicit admission/invalidation and
   reconnect behavior; behavior-code caching alone does not cache UI. Keep small
   interactive deltas cheap and measure frame encode/iPad decode cost first.

The existing codec's UI+code-only experiment produced:

| UI | Raw Mounted packet | LZ4 packet | Encode / decode² |
| --- | ---: | ---: | ---: |
| Home | 42,022 B | 18,394 B | 143 / 79 µs |
| Component Playground | 47,172 B | 21,340 B | 134 / 80 µs |
| Layout Lab | 54,784 B | 23,811 B | 184 / 83 µs |
| Layout Lab / signal | 20,096 B | 9,670 B | 60 / 30 µs |
| Layout Lab / cards | 25,294 B | 11,982 B | 68 / 39 µs |
| Input Lab | 99,448 B | 37,826 B | 297 / 137 µs |
| Animation Lab | 16,868 B | 7,906 B | 45 / 21 µs |
| 3D Lab | 16,844 B | 7,908 B | 42 / 24 µs |

² Optimized x86_64, 20 warmups then 100 timed iterations, reusable codec buffers.
Empty state; excludes UI interpretation, filesystem and durability. These are not
ARM/Safari CPU costs. Input Lab's reduction corresponds to about **55 ms ideal
payload time** at 9 Mbps, not an observed navigation improvement. Round trips
through the existing decoder were checked; production compression was not enabled.

## Reproduction and evidence

Requires Node 24, Chromium, Python 3 and the pinned Rust toolchain. Capture public
responses from a machine allowed to reach the site (this run used pinned SSH to
the frame); no signing private key is needed:

```sh
python3 scripts/capture-dashboard-public.py --url https://otrios.otri.ca > public.tar
mkdir snapshot
tar -xf public.tar -C snapshot
cargo build --release --locked --offline -p otrios-core --features server,canvas --bin dashboard-server
```

Prepare a separate release store containing that complete admitted release and
its `current.json`, plus fresh disposable state. Start the optimized authority
with `--bind 127.0.0.1:18935 --public-bind 127.0.0.1:18936
--origin http://127.0.0.1:18937 --app-state DISPOSABLE_STATE --releases RELEASE_STORE`.
Do not point it at production state. Then:

```sh
node scripts/profile-dashboard-browser.mjs --snapshot snapshot/snapshot.json \
  --publisher-key PUBLIC_PUBLISHER.pem --upstream http://127.0.0.1:18936 \
  --port 18937 --network fast4g --runs 5 --output target/browser-profile
```

The harness writes per-run milestones, resource metadata, touch timings and
WebSocket sizes, plus a cold Home screenshot. Use fresh state roots and
`--startup-only --variant parallel|shared-module|combined` for the local
experiments; use `--network none` for the unshaped control. Run codec costs with:

```sh
cargo run --release --locked --offline -p otrios-core --features canvas \
  --example canvas_wire_cost -- --app-release RELEASE_DIRECTORY
```

Committed evidence: [summary and samples](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/browser-startup-2026-10-25/summary.json),
[100 navigation transitions](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/browser-startup-2026-10-25/navigation.csv),
[cold Home screenshot](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/browser-startup-2026-10-25/cold-home.png).
Successful matrix: **23 cold runs, 23 warm reloads, 50 app taps and 50 Home returns**.
`bash scripts/check.sh` passed (185 Rust tests and 40 Python checks); Node syntax
and Python compilation checks passed. No new dependencies or wire-format changes.

[fast4g]: https://github.com/ChromeDevTools/devtools-frontend/blob/main/front_end/core/sdk/NetworkManager.ts
