# Unified admin panels and cross-Space moves — 2026-11-01

Status: validated and deployed to the existing Frame/Web environment. M4 portable
transfers remain outstanding. Aggregate evidence is in
[admin-moves-acceptance.json](data/spaces-map-2026-11-01/admin-moves-acceptance.json).

Implementation commit: `333f5e70966e068c79cefc279259ae077c8a81e5`.
`bash scripts/check.sh` exited 0, including 27 real-authority process tests.
The final native-keyboard routing refinement passed all 33 focused UI tests,
targeted library Clippy with warnings denied, and the release Wasm build.
The shared modal lets a captured keyboard contact reach the ordinary key router
even where it overlaps the panel's pan region. An existing populated-settings
test now exercises a physical-style down/up on a software key, rather than
substituting a hardware edit for that path.

This milestone combines M3 moves with the user-requested admin UI completion
against the approved RFC 0028 prototype. Space, app, retained dataset, profile,
Deleted Spaces and People & access use the same modal surface over the map.
Name editing is implicit; appearance includes None, presets and custom RGB.
Access edits have one bounded, single-Space publication and explicit audience
review. Real accepted-session counts and verified bundle facts replace mock
statistics. Profile display names do not change identity or membership.

Moves support App and data, App only and Data only, including retained datasets,
occupied-destination backups, fresh collection identities, correlated durable
outcomes and recovery before admission. Collector/service-backed scopes reject
without a separate rebinding plan. No general transaction API is introduced.

State protocol is v19, Spaces envelope v4, opening release-control envelope v1.
Registry framing advances from OTSP7 to OTSP8 for custom RGB colors. OTSP1–7
recovery preserves collection identities and decisions. M2 rejects OTSP8;
code rollback needs a compatible writer. Never downgrade current metadata.
The paired pre-cutover backup is available for an explicitly authorized reset.
No live data reset has been used during this milestone.

The pre-cutover kit `2026-11-01-spaces-admin-moves` preserves the previous
`a77bffea8c6f1e09aa8b27d120b549db1c12bab36a84504b8026e00e27032f00`
release and paired state/configuration. All 267 files verified on the private
Mac copy; active services were restored before transfer. Private metadata and
inventories were not exported to the repository. A private on-Frame before-state
audit is captured for comparison after activation.

The canonical signed build staged
`50e8bc2e1ece14aa9feafc366df095ea475c93e991c2be29cf05d19db7a28a0c`.
Pinned SSH deployment activated that exact release in both authority and native
reader. All 171 public signed-code/protocol/isolation checks passed; this probe
does not test production password login. Safari opened the deployed public Space,
entered its single app directly and returned through the OTriOS breadcrumb.

The matching ARM helper and calibrated physical-device reader exercised Admin
title capture, Remove App / Remove Data review and cross-Space app drop review.
Every reviewed mutation was cancelled. The map epoch stayed stable during drag,
generation did not change, app writes remained zero and calibration was unchanged.
The maximum checked readiness wait was 899.6 ms. This is synthetic evdev input
through the actual physical reader, not a claim of human finger observation;
no physical pixels or private inventories were exported.

The after-state audit confirms unchanged Space identities, names, layout, roles
and bindings; 55 non-collector payload files are byte-identical. Authentication,
device identity, calibration and collector binding are unchanged. Primary and
latest registry use OTSP8. No data restore or reset was performed.

Safari desktop acceptance used a disposable signed fixture with 14 apps across
four Spaces. It confirmed first-attempt sign-in, opening populated settings,
implicit Space rename plus immediate palette selection, custom dark background
with light labels, app bundle identity and minimal actions, and cancelling
People & access. All panels use the visible viewport and light text inputs;
the inert backdrop contributes no accessible controls or spoken text.

The populated fixture exposed a real layout-budget panic: the map plus native
keyboard exceeded one 128 KiB layout. The fix composes an independently admitted
inert map scene beneath the panel. Visible map pages also bound app previews.
This preserves the public layout budget. The focused UI suite stays at 33
tests; obsolete full-screen management tests and its broad browser driver were
removed. Populated settings, custom-color validation and modal isolation are
covered inside existing critical-path tests.

Reusable tooling added or refined:

- `qualify-spaces-browser.py --populated --serve` provides a synthetic Safari
  preview; `--refresh` re-signs Web bytes without resetting fixture login/data.
  Its loopback-only adapter changes only disposable fixture cookies; production
  Secure/HttpOnly cookie policy is unchanged.
- `audit-frame-state.py` keeps inventories, metadata and hashes on Frame and
  exports only aggregate before/after results.
- `promote-frame-release.py` checks signed bytes, matching Frame/native pointers,
  frozen source and a private paired backup before pinning the Mac watcher to
  the validated implementation commit.
- The canonical ARM build also produces the matching `space_map_probe` helper
  for calibrated physical-reader acceptance.

Share App, Export Data, Replace Data and Upload are reserved for M4. Their
presence in these settings is not evidence that portable transfers work.
No final review PR will be opened until the scoped goal is complete and deployed.
