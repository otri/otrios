<!-- SPDX-License-Identifier: Apache-2.0 -->
# Startup delivery improvements — 2026-10-25

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/startup-improvements.md) remain available at the pinned historical revision.

Deployed release:
`fe98da6b5478528c8abc07944d4df57a170268f6fccad1711f083f8fccf396b2`.
Implementation source: `cd78e5a`, stacked on the current app-owned navigation work.
The authority and physical native renderer are healthy and confirmed on this
release, with the same Home card order, seven State roots, 800×1280 output and
rotation 90. No service enablement was changed.

The rollup improves public asset delivery, shares compiled browser Wasm, and
compresses large app mounts. **The Cloudflare edge-cache rule is prepared but
not applied**: no account/API connection is configured. Current Wasm probes still
return `DYNAMIC`; an actual edge `HIT` remains an operational follow-up.

## Implemented behavior

- **Precompress immutable web assets on the build host.** The publisher generates
  deterministic gzip sidecars for compressible HTML, CSS, JS and Wasm, includes
  them in the signed manifest, and retains identity files. Activation verifies
  both the signed compressed digest and bounded decoded source digest. HTTP
  serves the gzip representation when accepted, preserving the original MIME
  type and URL, with `Vary: Accept-Encoding` and immutable public cache headers.
  Small/already-compressed assets stay unchanged. No new compression dependency.
- **Compile browser Wasm once.** Main Wasm, font and catalog requests start
  together. The main thread instantiates the compiled module and clones it to
  the state worker, which creates its own instance and memory. An engine that
  rejects module cloning falls back to the worker's existing independent load.
- **Compress bulk app mounts.** The state worker on the authority attempts the
  existing sync-v4 LZ4 option for `Mounted` packets of at least 8 KiB, outside the
  authority lock. Existing minimum savings/integrity/expansion bounds still apply.
  Small mounts, interactive state deltas, release control and ink retain their
  previous encodings. Browser and native already support the v4 compression flag;
  no new handshake, protocol or persistent UI cache is introduced.

An initial public check caught Cloudflare normalizing origin `Accept-Encoding`
while `no-transform` prevented downstream adaptation. The final server permits
edge transformation. Identity and gzip are checked publicly; quality-factor and
malformed-token handling are checked directly against the origin. Prototype
measurements from the earlier header version are excluded from the final matrix.

## Results

The [baseline methodology](browser-startup-profile.md) is retained: nominal
9 Mbps down / 1.5 Mbps up / 60 ms RTT, including WebSocket bandwidth, actual
captured public response bodies, fresh Chromium profiles, optimized disposable
x86_64 authority, unthrottled CPU and SwiftShader, 800×1280/DPR 1. Each scenario
has five cold runs, five same-profile reloads and first/repeat Home-card visits.
Monitoring is excluded. Public signatures and signed asset hashes are verified.

These are cloud Chromium results, not iPad/Safari or physical tap-to-photon timing.
Completion is destination UI/controls plus a submitted GL draw and next animation
frame. The replay excludes DNS/TLS, CDN/tunnel delays and radio/congestion effects;
the separate frame-to-public HTTPS probes must not be added to the replay values.

| Metric | Before median | Deployed median | Deployed min–max |
| --- | ---: | ---: | ---: |
| Cold Home | 2,219 ms | **2,051 ms** | 1,952–2,187 ms |
| Warm Home reload | 954 ms | 1,057 ms | 984–1,227 ms |
| Worker Wasm initialization | 103.9 ms | **1.0 ms** | 0.6–2.2 ms |
| Separate public HTTPS Wasm read¹ | 4,706 ms | **2,374 ms** | 2,063–2,380 ms |

Cold median improved by 168 ms / 7.6%; observed ranges overlap and sample counts
are small. **Warm startup did not improve** in this sample. Worker initialization
and transferred UI bytes show a much clearer change than whole-page timings.

¹ Three sequential requests from the frame through its public Cloudflare URL,
excluding body decode time. These are single-asset path probes, not mobile page
loads. Direct origin gzip reads took 322 ms initially and 26 ms warm. Public
Wasm remains `DYNAMIC`, with no `Age`. Requests occurred at different times, so
the approximately 50% public median reduction does not isolate one network cause.
Origin/tunnel transfer falls from 2,970,576 identity bytes to 831,978 prebuilt gzip
bytes; the client-side snapshot grew from 777,007 Brotli bytes to 831,978 gzip
bytes, a 7.1% payload tradeoff. The decoded Wasm hash is unchanged.

Median **tap release → rendered destination**:

| App | Before first / repeat | Deployed first / repeat | Received bytes before → deployed² |
| --- | ---: | ---: | ---: |
| Component Playground | 229 / 168 ms | **187 / 167 ms** | 49,361 → 21,694 |
| Layout Lab | 200 / 193 ms | **195 / 155 ms** | 56,485 → 23,320 |
| Input Lab | 232 / 218 ms | **184 / 165 ms** | 100,836 → 38,807 |
| Animation Lab | 153 / 136 ms | **149 / 132 ms** | 18,798 → 9,016 |
| 3D Lab | 145 / 162 ms | **150 / 132 ms** | 18,770 → 9,041 |

² First-visit median binary WebSocket application bytes received during the
transition; includes any accompanying state messages, excludes TCP/TLS overhead.
Input Lab traffic fell about 61.5%. Repeat visits still mount the full UI; this
change compresses delivery rather than introducing a UI cache or local mount path.

[Archived figure: Startup and navigation comparison](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/startup-improvements-2026-10-25/comparison.svg)

All samples, including outliers, are retained. One Input Lab first visit took
**4,327 ms** after release: about 3,180 ms before the mount was sent, 1,049 ms for
the response, 26 ms in the worker and 71 ms through presentation. Its repeat took
724 ms. That points to dispatch/host scheduling as a useful next trace, but this
run does not isolate the cause. The navigation plot uses a log scale to retain
the full range. These data do not establish a reliable tail-latency improvement
or a first-visit speedup for every app.

## Validation and operational follow-up

- `bash scripts/check.sh`: **186 Rust tests and 41 Python checks** passed.
- All-feature ARM Clippy and optimized ARM build passed; release x86_64 authority
  and browser Wasm builds passed. Cargo.lock and dependency declarations unchanged.
- **49 JavaScript/Wasm tests** passed. `node scripts/test-canvas-module-sharing.mjs`
  initializes the actual generated host twice from one cloned compiled module,
  verifies independent memories, and constructs/frees presentation and app objects.
- Staged HTTP checks: **101**; live public HTTPS checks: **98**. Tests cover signed
  compressed assets, decoding/hash equality, identity fallback, MIME/cache headers,
  private/admin isolation and existing session compatibility. Origin-only encoding
  parser cases account for the three-count difference.
- Chromium verifies direct/nested links, refresh/history, both orientations and
  private admission. The final measured matrix has **5 cold + 5 warm runs,
  50 app taps and 50 Home returns**, without editing live app State.
- The physical native renderer confirms the compressed Home mount. No attended
  physical touch or iPad/Safari performance acceptance was performed.
- A fresh qualified runtime receipt is recorded. Repacking Home as a UI-only
  change produces the **same release ID**, including the gzip sidecars, preserving
  incremental publication and idempotence.

Apply the [prepared Cloudflare rule](../../tools/dashboard/cloudflare-cache-rule.md)
with account access, then verify `HIT` on the immutable Wasm URL and no-store on
`/` and `/release`. The rule excludes state, discovery and private/authenticated
endpoints. Existing unrelated Cloudflare rules must be preserved.

Evidence: [summary, ranges and samples](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/startup-improvements-2026-10-25/summary.json),
[100 navigation transitions](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/startup-improvements-2026-10-25/navigation.csv).
Use the capture/replay commands in the baseline report with the new release and
`--variant baseline`; historical experimental transforms target the frozen old
release, while the new implementation is already present in the captured assets.
The measured server hashes are recorded in the summary.

The separate [frame service inventory](frame-services.md) details every installed
OTriOS/SPD unit and its consumers. The old `spd-kiosk-gate` remains a cleanup
candidate; this rollout leaves its enablement and the original kiosk units alone.
