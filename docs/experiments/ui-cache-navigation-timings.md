<!-- SPDX-License-Identifier: Apache-2.0 -->
# Public UI cache and iPad timing qualification

2026-10-25. Signed full-runtime candidate `f469a04009c0b8085eec6902059bb744d439f9d5908bf539a9f541143e9def59`.
See [RFC 0016](../rfcs/0016-ui-cache-and-navigation-timings.md) and
[measurement data](data/ui-cache-navigation-timings.json).

Open `https://otrios.otri.ca/?timings=1`, tap between apps, then open **Timings**
and **Copy report**. Selecting the report is the clipboard fallback on Safari.
The query survives app navigation. Reports stay in RAM and contain no user input,
credentials or app State. Their completion boundary is installed UI/controls,
GL submission and the next animation frame; physical display latency and the
end of decorative animations are not measured. The startup measurement includes
the opt-in panel/module cost. Normal browsing does not load the timing module.

The shared Rust cache applies to browser and native: eight qualified public UI
documents, capped at 2 MiB. State always comes from a fresh authoritative mount.
Private UI is neither retained nor omitted, including on authorized mounts.
Cache hits move retained bytes instead of cloning the whole document. A single
target digest replaces any need for a second UI inventory or host-specific
cache. Packed encoding now uses the same magic constant as decoding.

## Checks

- `bash scripts/check.sh`: 192 Rust tests and 41 Python tests; workspace formatting and Clippy pass.
- Node test suite including timing, worker, session, cache, UI update and host exports: 54 tests pass. Compiled Wasm module sharing/separate-memory acceptance also passes.
- ARMv7 all-feature/all-target Clippy and optimized native/server/helper builds pass.
- Disposable signed authority: 100 HTTP/session checks pass. This is public-only acceptance; authenticated login was not exercised.
- Real Chromium/WebGL: startup timing, ten measured app visits, smaller repeat mounts for all five public demos, local keyboard actions excluded, query preservation, and no diagnostic UI/module by default pass.
- Browser navigation acceptance passes: direct links, refresh, portrait Home/Layout Lab, Back/Forward without replacing the page/session, private Monitoring sign-in boundary, and removed example routes.
- Core/server regressions cover fresh State with cached UI, wrong/missing digests retaining the old view, bounded revision retention, private exclusion and access denial, invalid hint flags, previous-codec rejection and release-control v1 discovery for sync-v4 clients.

## Fast 4G measurements

Three sequential Chromium 154/SwiftShader runs on the cloud CPU, after builds
and other browser tests finished. A shared TCP scheduler shapes both HTTP and
WebSocket traffic to 9 Mbps down / 1.5 Mbps up / 60 ms RTT. HTTP replay has no
DNS/TLS/CDN delay; the State authority is disposable. Monitoring is excluded.
The earlier overlapping build/browser trial is excluded from these results.

Median cold Home was 2.13 s, and same-profile warm reload was 1.14 s. The three
cold samples were 2.13/2.81/2.12 s, warm 1.14/0.98/1.17 s. No cold-start gain is
claimed for this change. Real Safari/iPad timings remain to be collected using
the report above; these Chromium numbers are not iPad predictions.

| App | First bytes | Repeat bytes | First release-to-render ms | Repeat release-to-render ms |
| --- | ---: | ---: | ---: | ---: |
| Component Playground | 21666 | 2248 | 223 | 141 |
| Layout Lab | 21377 | 1760 | 186 | 133 |
| Input Lab | 38822 | 1447 | 212 | 134 |
| Animation Lab | 9018 | 1989 | 154 | 134 |
| 3D Lab | 9035 | 1985 | 146 | 147 |

Bytes count received State-channel traffic during each navigation, including
channel headers, with medians over three runs. The panel's `responseBytes`
instead counts only the encoded packet that qualifies the mount, excluding its
channel/WebSocket headers. Repeat responses were about 78–96% smaller. First
versus repeat timings also include existing behavior/font/renderer warm-up;
these measurements do not isolate latency saved solely by UI caching. Layout
Lab's repeat samples were 119/444/133 ms, and 3D showed no median latency gain.
The useful firm result is lower repeat traffic, plus a device report that can
separate input queueing, worker dispatch, response wait, qualification and render
work when another long navigation occurs.

## Deployment

Full deployment and native/public readiness are verified after activation; a
UI-only publish is insufficient because this release changes the state codec.
The Cloudflare cache-rule task remains separate.
