<!-- SPDX-License-Identifier: Apache-2.0 -->
# Home scrolling and sign-in qualification — 2026-11-01

This follow-up to [the Spaces map](spaces-map-frame-2026-11-01.md) fixes home
scrolling, application return targets and sign-in across presentation rebuilds.
The operational commands are in [Spaces](../SPACES.md#home-input-deployment-checks).
[Sanitized evidence](data/home-scroll-2026-11-01/README.md) contains synthetic
browser results and aggregate physical-device acceptance; no private Frame pixels
or file-level state inventory are included.

## Behavior and cause

Home touch scrolling uses the existing momentum controller. Pointer moves and
animation ticks update presentation without rebuilding the host or changing its
mount epoch. Browser wheel, trackpad and keyboard scrolling use the same local
offset. Controls clipped outside the map become available again when scrolled
back into view. The bottom arrows are replaced by a paper-colored fade that
disappears at the lower bound.

The initial candidate selected the first clipped canvas as its viewport. That
was the outer page, so scrolling also moved the background, header and footer,
exposing the renderer's dark clear color. This caused the reported black fade
and could expose the bottom edge on Frame. The corrected renderer explicitly
identifies the map node. Only its children move; page chrome stays fixed. The
existing premultiplied-alpha blend function was retained.

Interactive canvas hit targets now include their declared padding while honoring
ancestor clipping. Apps show a single `< OTriOS` return breadcrumb with the app
title rather than a separate Quit control. Brass Relay's return includes the
logical top-left corner.

Sign-in intent survives viewport and metadata rebuilds until authentication or
cancellation. Background session refreshes wait for pending cookie mutations,
and the form retains its password while showing “Signing in…”. Password hashing
parameters are unchanged.

## Source and compatibility

- Runtime source: `cf3782f408a57fe37706dd5a6d3bfbb380a7a341`.
- Signed release: `54922c1c300dadd6b85ef120240bb0ee895a6b6d23b71d233574fc62c6d07391`.
- Exact release Wasm SHA-256: `b8f06a6aca9cdeef1f6111ef65e8cb3cd6cdd161e164bff8172e61b2cd00444f`.
- State protocol v17, opening release-control v1 and registry OTSP6 are unchanged.
  No state migration or new dependency is needed.

The canonical `scripts/watch-dashboard.py --once --stage-only` workflow froze
clean source, built Web and ARM hosts and signed all 12 app closures with the
Mac's existing signer. Existing strict SSH host pinning is retained. Later
documentation commits do not change the deployed runtime.

## Completed validation

- `bash scripts/check.sh` passed: formatting, strict Clippy, workspace tests,
  210 core library tests, 30 Spaces UI tests, tooling/bundle checks and 22
  separate authority/client process cases. `TMPDIR=/private/tmp` avoids macOS
  local-socket path length limits.
- The signed build's selected native/Wasm validations and 80 Node tests passed,
  including 10 monitoring-session tests. Linux evdev fixture tests passed.
- Both `scripts/qualify-spaces-browser.py` drivers passed against the exact
  release Wasm, recording clean source and no runtime errors. The denser
  `--populated` fixture has four Spaces and fourteen apps. It checks repeated
  bottom/top reversals, fixed header/footer geometry and actual fade/footer
  pixels, and signs in with one submission after resizing the open form.
  Its local sign-in observation was 1,025 ms; this is not a production latency
  guarantee. Broader regression covered isolation, signed visitor comments,
  retention, reconnection, revocation and attachment publication/download.

## Preservation and acceptance

A fresh quiescent rollback kit was verified on Frame and privately on the Mac:
259 inventory entries. Services resumed before archive transfer. The accepted
OTSP6 fallback `df40326c5dd7b2391beae584bb79a10f4c0bb7482b4a61e8b62468388ae0408b`
and matching helpers remain preserved outside release pruning. Rollback must
switch compatible code while retaining current safety roots and confirmed data;
restoring an old registry is not part of this presentation update.

Final activation passed signature/closure verification on Frame. Mac and Frame
release pointers agree. Authority, direct display, tunnel and monitoring are
active; the collector completed successfully. Public HTTPS conformance passed
171 checks. It does not test Aaron's production password.

The final native host reports the expected current/runtime release, confirmed
state, ready stream, idle input and zero pending commands. GLES2 direct scanout
uses the existing 800×1280 RGB565 output rotated 90° and one calibrated device.
`scripts/test-frame-home.py` passed three full scroll round trips, immediate
reversal at both bounds, unchanged map epoch, and Brass Relay → map return at
logical `(5,5)`. Its first app-handoff assertion was too strict about the temporary
loading view; the reusable probe now waits for the signed app view while requiring
the correct native release throughout. No runtime rebuild was needed for that
probe correction.

The on-Frame preservation audit found all 55 baseline state files present,
with 52 byte-identical and three updated. A private-copy document/receipt audit
accounted for those changes without stopping services: eleven documents' values,
signals and root payloads remained unchanged; one Component Playground document
had a new confirmed writer update, which was retained. All 270 baseline writer
receipts, all baseline signals/root payloads and confirmed chat were preserved.
Thirty-seven other state files were byte-identical; one collector-owned generated
Monitoring presentation changed. Safety roots, owner lock, auth configuration and
touch calibration remained byte-identical. No state was restored.

This is automated software acceptance through the physical input reader. It does
not claim a measured frame rate or a final human observation of touch feel or the
Frame's bottom pixels. The denser browser fixture checks actual rendered pixels
using the same corrected runtime and exact deployed Wasm. Private Frame pixels
remain on the device.
