<!-- SPDX-License-Identifier: Apache-2.0 -->
# Layout Lab cleanup and frame recovery — 2026-10-25

Raw measurements and figures from this report were retired from the current tree.
The [original report and artifacts](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/layout-lab-cleanup-recovery.md) remain available at the pinned historical revision.

The full corrected release is
`295314db6477aff80372b0c0ef1b4276f6f5da38aa797fa2a601f5367d789dcb`, built from
`c448b1608154c984922790a0be1161cf3115a016`. Home and Layout Lab load on the user's
iPad after reloading the public site. Both authority health endpoints return
200, and the physical native renderer has a confirmed Home mount on that release.

## Cause and correction

The previous active release advertised nine screens in its catalog while its
signed package closure contained only seven app bundles. The example Layout Lab
Signal and Cards documents were absent, so authority recovery failed while
opening `authority/apps/layout-lab/signal/ui.flex`. The service remained running
but returned 503 and could not serve app State.

Layout Lab now declares only its main screen at `/layout-lab`. Its normal toggle,
graph, sliders and reusable cards remain; the two example buttons, subroutes and
duplicate UI documents are removed. Generic nested navigation and shared-State
coverage remain in disposable test fixtures. The shipped catalog and package
closure now agree on seven app screens. Retired example paths return 404.

An initial full deployment was overwritten by the old publisher on OTriProMax.
The user persistently disabled and unloaded `com.otri.otrios-dashboard-watch`,
verified no publisher subprocess remained, and confirmed the old Mac server,
tunnel and monitoring agents were disabled. Recovery then used a **full** signed
deployment of the authority, browser, native renderer, helpers, catalog and app
packages, including the startup improvements from PR #13. It did not reuse the
inconsistent active catalog or replace durable app State.

## Verification

- Full deployment transferred 21,032,960 bytes. Complete signature and asset
  verification passed before activation; both authority and native caches advanced
  to the corrected release and obsolete releases were pruned.
- Public HTTPS acceptance passed **95 checks**, covering code hashes, original
  MIME/cache headers, compressed Wasm delivery, release/session negotiation,
  available app State, retired-route 404s and private/admin isolation.
- Chromium staging acceptance passed app navigation, direct links, reload/history,
  portrait/landscape layout, private admission and absence of the example screens.
  The user separately confirmed **Home and Layout Lab load on iPad**.
- The physical native renderer reports `gles2-direct`, 800×1280, rotation 90,
  `app:home/ui` / `app:home/state-v3`, connected and state-confirmed. The authority
  reports seven durable app roots. No attended physical touch acceptance is claimed.
- The release, server hash, health and native confirmation were observed over
  more than the previous publisher's approximately 100-second interval.
- `spd-weston`, `spd-browser`, `spd-kiosk-gate` and `seatd` remain absent.
- Source validation from the full build: **187 Rust tests, 41 Python checks,
  49 JavaScript/Wasm tests**, independent-memory Wasm acceptance, all-feature ARM
  Clippy and ARM/browser builds. No new dependency or protocol version.

See [recovery evidence](https://github.com/otri/otrios/blob/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments/data/layout-lab-recovery-2026-10-25.json) for timed health
and native observations. Earlier performance measurements remain frozen to their
recorded release; this recovery does not claim a new performance measurement.

The cloud-prepared Cloudflare edge-cache rule is separate from loading recovery;
this verification does not claim that rule was applied or that an edge HIT occurred.
