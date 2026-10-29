<!-- SPDX-License-Identifier: Apache-2.0 -->
# Standalone apps on Frame — 2026-10-29

[RFC 0025](../rfcs/0025-standalone-app-bundles.md) now has a bounded, deployed
Canvas app installation path. Counter was built outside the host's app source
registry, signed, installed into the live Frame registry and discovered through
its generated Home launcher. Its packaged schema, UI and pure Wasm reducer ran
on the physical ARM/GLES host and a real Chrome/WebGL browser. No host rebuild
was used for app installation, upgrade or rollback.

Runtime source: `47d8137ae78d33d1d65503cb6139006d2016c6a9`, following the B1
packaging commit `18996da`. The tested source was pushed and its exact SHA fetched
independently over the configured HTTPS remote before changing live services.
Rust 1.90.0; macOS aarch64 builder; existing Linux armv7 Frame and Mali-400 GLES2.
The user requested direct live deployment, so no disposable authority was used.

## Installation and lifecycle

The initial signed bundle-capable host release was
`49ec0c839a9d63890c23ecf2aac14c96317352885d440554994987242da57b11`.
It admitted the existing 11 packages from private signed `.otapp` trees.
Counter was independently authored using the `bundle_counter` example, exported
with `canvas-bundle`, signed with the existing builder publisher, and installed
with `install-app.py --deploy-config`. The publisher private key stayed on the
builder. Bundle resources and code remained outside the public HTTP allowlist.

| Step | Signed release | Observed result |
| --- | --- | --- |
| Install revision 1, +1 | `a2f3f423d759b695eb795068e8582524f1e1f66d06b9d59cda4076813a937839` | Authority and physical native last-good agree; 12 apps; Home shows Counter; browser increments 0 → 1 |
| Native input | same | Home card and +1 activated through calibrated synthetic evdev taps; confirmed state 1 → 2 |
| Upgrade revision 2, +2 | `843e4ca2b7dacda01e45df1668a9e180298e30bbe62cb882ea5618181fc400c7` | Connected browser and Frame hand off; state stays 2; new guest reducer increments 2 → 4 |
| Explicit signed rollback to revision 1 | original install release | State stays 4; original reducer increments 4 → 5; Frame and browser agree |

Installation transferred **163,840 bytes**. Upgrade and rollback each transferred
**112,640 bytes**. These are complete SSH tar payload sizes, including management
scripts, rather than bundle sizes. The installer compared signed manifests and
required every native/browser runtime asset to remain byte-identical. It changed
the catalog and private app closure only. Rollback selected a complete verified
prior release; it did not restore, delete or rewrite canonical authority state.

Counter v1 bundle digest:
`86d123c91d807a9be578ec34e5619a914b4d41afc3d46d70c6baa41558382aa0`.
Counter v2 bundle digest:
`c5b654ae3be53873fa242c6ab74ce9505d17b89fae5a28676d28c9832e5c9f82`.
Each unsigned payload had six files and 29,055 bytes; the detached publisher
receipt adds a seventh file. The count is a shared atomic register with a pure
reducer, not a concurrent-operation counter CRDT.

## Actual execution evidence

The temporary-profile Chrome harness `scripts/test-bundle-browser.mjs` navigated
Home → Counter using accessible controls, rendered the app, sent a guest event,
and remained connected across both upgrade and rollback. All three runs passed
with zero runtime exceptions. Opening hello remained release-control **v1**, with
state protocol **otrios-canvas-sync-v8**, and carried the current release hash after
each reconnect. State protocol bytes were unchanged by this milestone.

A separately executed ARM `bundle_app_probe` mounted the installed package,
reported the shared count and denied unauthenticated Monitoring before any private
UI payload. The production native service itself navigated and executed Counter
through its existing calibrated input reader. Its ready record reported
`gles2-direct`, `app:bundle-counter/ui`, confirmed state, stream readiness, zero
pending commands, idle input, rotation 90 and physical scanout 800×1280. Synthetic
evdev input is device-path qualification, not a claim of human touch testing.
The calibration file hash was unchanged:
`44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

[Machine-readable results](data/app-bundle-frame-2026-10-29/qualification.json)
contain the public browser samples, ARM probe and sanitized native readiness.
Screenshots: [generated Home](data/app-bundle-frame-2026-10-29/home.png),
[browser Counter](data/app-bundle-frame-2026-10-29/browser-counter.png), and
[raw Frame scanout](data/app-bundle-frame-2026-10-29/frame-scanout.png). The raw
scanout retains the existing hardware rotation; no chat content or credentials
are included. Frame was left running Counter revision 1 with confirmed count 5.

## Validation and operational findings

- `bash scripts/check.sh` passed: formatting, workspace Clippy/tests, existing
  Python suites and the new signed lifecycle process test. That test also checks
  interrupted staging, corrupted inner signatures despite valid outer signing,
  corrupt cached releases, lower/conflicting revisions, private paths, restart,
  uninstall, rollback and scoped authoring preservation.
- Host all-feature Clippy passed for core/dashboard/web-host. The whole-workspace
  macOS all-feature run encounters existing Linux-only display examples; it is
  not recorded as a pass. The matching frozen source passed all-feature,
  all-target Clippy for those host packages on Linux/ARM in the existing Docker
  builder. The production source archive excludes browser fixtures, so frozen
  `web/` fixtures were copied into that builder for this check.
- Browser Wasm target check passed. The frozen build repeated native process
  tests, signed lifecycle tests, browser compilation and **74 Node tests**, then
  built and signed matching ARM authority/native/checker binaries.
- `python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only`
  passed **171 checks**, including signed assets, negotiation, private/admin
  isolation and anonymous denial. Authenticated password login was not tested.
  An earlier run overlapped release pruning and saw an old asset 404; the stable
  release rerun passed.
- Initial ARM journal recovery exceeded the deploy helper's six-second wait.
  Health subsequently recovered; activation and native service restart completed.
  The helper now permits a longer recovery window; its four hosting tests pass.
  One transient SSH connection reset was retried. The host rejects SSH forwarding,
  so the guest probe ran directly on ARM rather than through a tunnel.

One private operational rollback copy retains the previous host/services and a
consistent stopped-state snapshot. No backup state was restored. The existing
Linux kernel, drivers, rotation and touch calibration were preserved.

This qualifies bounded signed Canvas installation, not a general OS module loader,
capability imports, multi-publisher revocation or Linux shared-memory IPC. Wasm
still executes through the existing bounded interpreter. Target-native AOT cache
compilation remains the separate [RFC 0026](../rfcs/0026-target-wasm-compilation.md)
experiment; no native AOT result is claimed.
