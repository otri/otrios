<!-- SPDX-License-Identifier: Apache-2.0 -->
# Independent app UI update validation — 2026-10-24

All seven apps use one package factory API and the same UI replacement contract.
See [RFC 0013](../rfcs/0013-app-ui-updates.md) for the signed closure, runtime reuse,
publication and activation boundaries.

## Automated validation

- `RUST_TEST_THREADS=1 bash scripts/check.sh`: passes formatting, strict workspace
  Clippy, 168 Rust tests and the Python release/direct/hosting/publishing checks.
- All browser session, worker, cache, catalog, viewport, UI update and GPU suites:
  45 JavaScript tests pass. The Wasm release build/bindings and export check pass.
- ARM all-feature/all-target workspace Clippy and release host builds pass.
- Core tests replace each of the seven apps' UI documents through the same API,
  preserve state, reject incompatible contracts, preserve the current scene on
  rejection, and make repeated admission a no-op without advancing its epoch.
- Partial transfer tests reconstruct a complete verified signed release from the
  installed baseline; missing changed assets and stale baselines are rejected.
- Repacking all apps preserves all 15 package files' bytes and timestamps.
  Repeating an unchanged UI publish reports `UNCHANGED` and skips deployment.

## Real browser experiment

A disposable local authority serves the same browser runtime as the frame build.
Home remains open in portrait while Playground remains open in landscape, with
its linear slider confirmed above 75 percent. The publisher changes only Home's
introduction to “A shared runtime. Ready to explore.”

`node scripts/test-responsive-browser.mjs --url http://127.0.0.1:18877 --wait-home-update --output target/app-ui-browser`
passes. Both pages retain `performance.timeOrigin` and their WebSocket sessions. Their
addresses advance to the new release using browser history without navigation,
so an explicit refresh loads the current release after old folders are pruned. Home retains compatible semantic keyboard focus. Playground
retains its mount epoch and confirmed slider value. Later navigation opens the
new Home document through the updated catalog. A window marker also survives,
proving the existing page remained loaded.

The browser retains its verified manifest in RAM; unit tests also check updates
when the previous manifest URL has already been pruned. This fixture uses its
own signing identity and does not establish physical device acceptance.

[Machine-readable results](app-ui-updates.json)

![Home before](app-ui-home-before-update.png)

![Home after](app-ui-home-after-update.png)

## Deployment

The initial runtime support and browser address handling were installed, then
source `ddd64de` changed Home's introduction from “Ready to explore.” to
“Ready to explore!” through the UI-data-only path. The final signed release is
`46741ef2a37bf5b582fbc2712bf14a471e13614709d2161bf1e8ea413cffaf98`.

Only `apps/catalog.flex` and `authority/apps/home/ui.flex` changed. The transfer
was 102,400 bytes (100 KiB), versus 19,650,560 bytes (18.7 MiB) for the initial
runtime package. No native or browser Wasm build ran for this UI change; the
native authoring-tool rebuild took 1.45 seconds in the warm environment.

The authority PID remained 10726; the native renderer PID remained
10995. Home advanced from epoch 1 to 2, rendered the new UI,
and reported confirmed state with a live stream. Its runtime release remained
`6c8b8b864dc61e1b31495ac6476a07eed471fa93c6ff8e4a697f5ac7f86cd7a4`. Repeating the same publish reports
`UNCHANGED` and performs no transfer. All seven apps also passed unchanged
UI-only packing/publication checks.

Public HTTPS conformance passes 70 checks (signed public asset integrity,
release/session negotiation, private/admin isolation and anonymous denial).
Monitoring login and physical touch/color/performance were not exercised.
No new rollback or external backup tooling is introduced.
